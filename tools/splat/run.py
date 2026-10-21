# Splat training: footage of a real place or thing made a 3D Gaussian splat (raea's asset store runs this;
# rdb::assets::Store::splat, `raea-motion splat train`). Writes OUT/splat.ply and OUT/splat.ply.json.
#
#   python run.py --input VIDEO|IMAGE_DIR --out DIR --brush BIN [--frames N] [--steps N] [--max-res PX] [--from S]
#                 [--seconds N] [--max-splats N] [--matte none|black] [--match sequential|exhaustive] [--eval-every N]
#
# How: frames (a video: the sharpest frame in each of N equal windows, by the variance of its Laplacian; a folder: its
# pictures, N of them evenly), long side at most --max-res; COLMAP (pycolmap) with one shared SIMPLE_RADIAL camera,
# sequential (video) or exhaustive matching, incremental mapping; the largest model undistorted to PINHOLE; then Brush
# (Rust + wgpu, on Metal) trains 3DGS on it. `--matte black`: footage of an object on black (a studio turntable) gets an
# alpha from its silhouette, so Brush trains the object alone with nothing behind it.
#
# splat.ply: Brush's export, standard 3DGS (x y z, f_dc_*, f_rest_*, opacity as a logit, scale_* as logs, rot_0..3 as
# w x y z), in COLMAP's coordinates, untouched (rotating it would mean rotating its SH too).
# splat.ply.json, the sidecar, in Bevy's axes (+Y up, a camera looks down its -Z):
#   {version, transform: 16 numbers (column-major 4x4: file coordinates -> raea's world), count, sh_degree, cameras:
#    [{frame, time, position, lookAt, up, fov (vertical degrees)}] one per training frame in source order, width,
#    height (training pictures), bounds: {min, max} (2nd and 98th percentile of the splats in the world), focus (how
#    the centre was found), registered, frames, error (mean reprojection, px), psnr (Brush's eval, if any), steps,
#    seconds: {frames, sfm, train}, source, notes}
# The world: +Y is up (the direction every camera's right axis is level with: footage is shot without roll), the
# origin is the point the cameras look at (the least-squares meeting point of their axes; the median of the sparse
# points when they don't meet), the first camera is on +Z, and the scale puts the cameras 4 m from the origin on
# average.
import argparse, glob, json, math, os, re, shutil, subprocess, sys, time

import numpy as np
from PIL import Image

ap = argparse.ArgumentParser()
ap.add_argument("--input", required=True)
ap.add_argument("--out", required=True)
ap.add_argument("--brush", required=True)
ap.add_argument("--frames", type=int, default=150)
ap.add_argument("--steps", type=int, default=20000)
ap.add_argument("--max-res", type=int, default=1600)
ap.add_argument("--from", dest="start", type=float, default=0.0)
ap.add_argument("--seconds", type=float, default=0.0)
ap.add_argument("--max-splats", type=int, default=2000000)
ap.add_argument("--matte", default="none", choices=["none", "black"])
ap.add_argument("--match", default="", choices=["", "sequential", "exhaustive"])
ap.add_argument("--eval-every", type=int, default=16)
ap.add_argument("--keep", action="store_true", help="keep the work folder (frames, COLMAP, Brush's eval renders)")
a = ap.parse_args()
t0 = time.time()
times = {}


def say(*m):
    print("splat:", *m, file=sys.stderr, flush=True)


out = os.path.abspath(a.out)
work = os.path.join(out, "work")
shutil.rmtree(work, ignore_errors=True)
os.makedirs(f"{work}/cand")
os.makedirs(f"{work}/img")
notes = []
src = os.path.abspath(a.input)
N = max(8, a.frames)

# ---- frames ----------------------------------------------------------------------------------------------------------
import cv2

meta = []  # per training picture: {name, frame, time}
if os.path.isdir(src):
    pics = sorted(p for p in glob.glob(os.path.join(src, "*")) if p.lower().endswith((".jpg", ".jpeg", ".png", ".tif", ".tiff", ".webp")))
    if len(pics) < 3:
        sys.exit(f"splat: {src} has {len(pics)} pictures; a splat needs at least a few dozen views")
    pick = pics if len(pics) <= N else [pics[round(i * (len(pics) - 1) / (N - 1))] for i in range(N)]
    for i, p in enumerate(pick):
        im = Image.open(p).convert("RGB")
        s = min(1.0, a.max_res / max(im.size))
        if s < 1.0:
            im = im.resize((round(im.size[0] * s), round(im.size[1] * s)), Image.LANCZOS)
        name = f"{i + 1:04d}.png"
        im.save(f"{work}/img/{name}")
        meta.append({"name": name, "frame": pics.index(p), "time": None, "file": os.path.basename(p)})
    video = False
    say(f"{len(meta)} pictures from {src}")
else:
    video = True
    probe = json.loads(subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,r_frame_rate,duration,color_space", "-of", "json", src], capture_output=True, text=True, check=True).stdout)["streams"][0]
    W, H = int(probe["width"]), int(probe["height"])
    num, den = probe["r_frame_rate"].split("/")
    fps = float(num) / float(den)
    dur = float(probe.get("duration") or 0)
    seconds = a.seconds if a.seconds > 0 else max(0.0, dur - a.start)
    matrix = {"bt470bg": "bt601", "smpte170m": "bt601"}.get(probe.get("color_space", ""), "bt709")
    s = min(1.0, a.max_res / max(W, H))
    sw, sh = round(W * s / 2) * 2, round(H * s / 2) * 2
    # three candidates per picture kept (all the source's frames when it has fewer), the sharpest of each window wins
    total = max(1, int(seconds * fps))
    step = max(1, total // (N * 3))
    rng = ["-ss", f"{a.start:.6f}", "-i", src, "-t", f"{seconds:.6f}"]
    vf = f"select='not(mod(n\\,{step}))',scale={sw}:{sh}:flags=lanczos:in_color_matrix={matrix}:in_range=tv,format=rgb24"
    subprocess.run(["ffmpeg", "-v", "error", "-y", *rng, "-vf", vf, "-fps_mode", "passthrough", "-start_number", "0", "-q:v", "2", f"{work}/cand/%05d.jpg"], check=True)
    # candidates as JPEG (PNG cost ~1.7 GB for 150 frames' worth); the winners are written back out as PNG
    cands = sorted(glob.glob(f"{work}/cand/*.jpg"))
    if len(cands) < 3:
        sys.exit(f"splat: {len(cands)} frames in {a.start}s + {seconds}s of {src}")
    sharp = []
    for c in cands:
        g = cv2.imread(c, cv2.IMREAD_GRAYSCALE)
        g = cv2.resize(g, (g.shape[1] // 2, g.shape[0] // 2), interpolation=cv2.INTER_AREA)
        sharp.append(float(cv2.Laplacian(g, cv2.CV_32F).var()))
    n = min(N, len(cands))
    edges = np.linspace(0, len(cands), n + 1)
    for i in range(n):
        lo, hi = int(round(edges[i])), max(int(round(edges[i])) + 1, int(round(edges[i + 1])))
        k = lo + int(np.argmax(sharp[lo:hi]))
        name = f"{i + 1:04d}.png"
        cv2.imwrite(f"{work}/img/{name}", cv2.imread(cands[k]))
        f = int(round(a.start * fps)) + k * step
        meta.append({"name": name, "frame": f, "time": round(f / fps, 6)})
    shutil.rmtree(f"{work}/cand")
    say(f"{len(meta)} frames at {sw}x{sh} from {len(cands)} candidates ({fps:g} fps, {a.start:g}s + {seconds:g}s)")
times["frames"] = round(time.time() - t0, 1)

# ---- structure from motion -------------------------------------------------------------------------------------------
import pycolmap

pycolmap.logging.minloglevel = 2
t1 = time.time()
db = f"{work}/db.db"
reader = pycolmap.ImageReaderOptions()
reader.camera_model = "SIMPLE_RADIAL"
ext = pycolmap.FeatureExtractionOptions()
ext.sift.max_num_features = 8192
ext.use_gpu = False
pycolmap.extract_features(db, f"{work}/img", camera_mode=pycolmap.CameraMode.SINGLE, reader_options=reader, extraction_options=ext, device=pycolmap.Device.cpu)
say(f"features extracted ({time.time() - t1:.0f} s)")
mo = pycolmap.FeatureMatchingOptions()
mo.use_gpu = False
match = a.match or ("sequential" if video and len(meta) > 60 else "exhaustive")
if match == "sequential":
    pair = pycolmap.SequentialPairingOptions()
    pair.overlap = 15
    pair.quadratic_overlap = True
    pycolmap.match_sequential(db, matching_options=mo, pairing_options=pair, device=pycolmap.Device.cpu)
else:
    pycolmap.match_exhaustive(db, matching_options=mo, device=pycolmap.Device.cpu)
say(f"matched ({match}, {time.time() - t1:.0f} s)")
opts = pycolmap.IncrementalPipelineOptions()
opts.multiple_models = False
opts.random_seed = 1
os.makedirs(f"{work}/sparse")
progress = [0]


def registered():
    progress[0] += 1
    if progress[0] % 20 == 0:
        say(f"{progress[0]} frames registered ({time.time() - t1:.0f} s)")


maps = pycolmap.incremental_mapping(db, f"{work}/img", f"{work}/sparse", options=opts, next_image_callback=registered)
if not maps:
    sys.exit("splat: COLMAP registered no frames: the footage needs parallax (a camera moving round its subject), texture and sharp frames")
model = max(maps.values(), key=lambda r: r.num_reg_images())
reg = model.num_reg_images()
err = float(model.compute_mean_reprojection_error())
say(f"{reg} of {len(meta)} frames registered, {model.num_points3D()} points, reprojection error {err:.2f} px ({time.time() - t1:.0f} s)")
if reg < max(8, len(meta) // 4):
    sys.exit(f"splat: COLMAP registered only {reg} of {len(meta)} frames: too few views to train on (the footage needs a camera moving round a still subject, with texture; water, sky and moving things don't match)")
if reg < len(meta) * 2 // 3:
    notes.append(f"only {reg} of {len(meta)} frames registered")
os.makedirs(f"{work}/model")
model.write(f"{work}/model")
# undistorted to PINHOLE: Brush takes PINHOLE/SIMPLE_PINHOLE cameras as they are
pycolmap.undistort_images(f"{work}/dense", f"{work}/model", f"{work}/img")
ds = f"{work}/dataset"
os.makedirs(f"{ds}/sparse")
os.rename(f"{work}/dense/images", f"{ds}/images")
os.rename(f"{work}/dense/sparse", f"{ds}/sparse/0")
times["sfm"] = round(time.time() - t1, 1)
und = pycolmap.Reconstruction(f"{ds}/sparse/0")

# ---- matte: an object on black gets an alpha from its silhouette ---------------------------------------------------------
if a.matte == "black":
    for p in sorted(glob.glob(f"{ds}/images/*")):
        im = cv2.imread(p, cv2.IMREAD_COLOR)
        m = (im.max(axis=2) > 22).astype(np.uint8) * 255
        k = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (9, 9))
        m = cv2.morphologyEx(m, cv2.MORPH_CLOSE, k)
        m = cv2.morphologyEx(m, cv2.MORPH_OPEN, k)
        # the silhouette's outer contours filled, so the object's own dark folds stay solid
        cs, _ = cv2.findContours(m, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
        fill = np.zeros_like(m)
        area = m.shape[0] * m.shape[1]
        cv2.drawContours(fill, [c for c in cs if cv2.contourArea(c) > area * 0.002], -1, 255, -1)
        alpha = cv2.GaussianBlur(fill, (5, 5), 0)
        rgba = np.dstack([im, alpha])
        cv2.imwrite(os.path.splitext(p)[0] + ".png", rgba)
        if not p.endswith(".png"):
            os.remove(p)
    say("matted: the object alone on transparent")

# ---- training: Brush --------------------------------------------------------------------------------------------------
t2 = time.time()
bout = f"{work}/brush"
os.makedirs(bout)
cmd = [a.brush, ds, "--total-steps", str(a.steps), "--max-resolution", str(a.max_res), "--max-splats", str(a.max_splats),
       "--export-path", bout, "--export-every", str(a.steps), "--export-name", "export_{iter}.ply", "--sh-degree", "3", "--seed", "42"]
if a.eval_every > 0 and len(meta) >= 2 * a.eval_every:
    cmd += ["--eval-split-every", str(a.eval_every), "--eval-every", str(max(1000, a.steps // 4)), "--eval-save-to-disk"]
say("training:", " ".join(cmd[1:]))
import threading

log = []
p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1)
reader_t = threading.Thread(target=lambda: [log.append(l.rstrip()) for l in p.stdout], daemon=True)
reader_t.start()


def psnr_of(d):
    """Brush's eval renders in d against the held-out pictures they render: the mean PSNR (dB) over them."""
    vals = []
    for r in sorted(glob.glob(f"{d}/*.png")):
        gt = glob.glob(f"{ds}/images/{os.path.splitext(os.path.basename(r))[0]}.*")
        if not gt:
            continue
        x = cv2.imread(r, cv2.IMREAD_COLOR).astype(np.float64)
        y = cv2.imread(gt[0], cv2.IMREAD_COLOR).astype(np.float64)
        if x.shape != y.shape:
            y = cv2.resize(y, (x.shape[1], x.shape[0]), interpolation=cv2.INTER_AREA)
        mse = np.mean((x - y) ** 2)
        vals.append(10 * math.log10(255.0 ** 2 / max(mse, 1e-10)))
    return float(np.mean(vals)) if vals else None


# Brush prints nothing while it trains: say how far it is every 30 s (a stall watcher sees it working), with each eval's
# PSNR as it lands
seen = set()
beat = time.time()
while p.poll() is None:
    try:
        p.wait(timeout=1)
    except subprocess.TimeoutExpired:
        pass
    if time.time() - beat >= 30:
        beat = time.time()
        evs = sorted(glob.glob(f"{bout}/eval_*"), key=lambda d: int(d.rsplit("_", 1)[1]))
        msg = f"training, {time.time() - t2:.0f} s"
        for d in evs[:-1]:
            if d not in seen:
                seen.add(d)
                q = psnr_of(d)
                msg += f"; step {d.rsplit('_', 1)[1]} eval PSNR {q:.2f}" if q else ""
        say(msg)
rc = p.wait()
reader_t.join(timeout=5)
with open(f"{out}/brush.log", "w") as f:
    f.write("\n".join(log))
plys = sorted(glob.glob(f"{bout}/*.ply"), key=os.path.getmtime)
if rc != 0 or not plys:
    sys.exit(f"splat: Brush failed ({rc}); its log is {out}/brush.log: {' | '.join(log[-5:])}")
times["train"] = round(time.time() - t2, 1)
evs = sorted(glob.glob(f"{bout}/eval_*"), key=lambda d: int(d.rsplit("_", 1)[1]))
psnr = round(psnr_of(evs[-1]), 3) if evs and psnr_of(evs[-1]) else None
brush_ply = plys[-1]
say(f"trained in {times['train']:.0f} s" + (f", held-out PSNR {psnr:.2f} dB" if psnr else ""))

# ---- the ply: its count and positions ---------------------------------------------------------------------------------
with open(brush_ply, "rb") as f:
    head = b""
    while not head.endswith(b"end_header\n"):
        c = f.read(1)
        if not c:
            sys.exit("splat: Brush's ply has no header")
        head += c
    hdr = head.decode("ascii", "replace")
    count = int(re.search(r"element vertex (\d+)", hdr).group(1))
    props = re.findall(r"property (\w+) (\w+)", hdr)
    if "binary_little_endian" not in hdr:
        notes.append("the ply isn't binary little endian")
    sizes = {"float": 4, "float32": 4, "double": 8, "uchar": 1, "uint8": 1, "half": 2, "float16": 2, "int": 4, "uint": 4}
    dt = np.dtype([(n, {"float": "<f4", "float32": "<f4", "double": "<f8", "uchar": "u1", "uint8": "u1", "half": "<f2", "float16": "<f2", "int": "<i4", "uint": "<u4"}[t]) for t, n in props])
    data = np.frombuffer(f.read(count * dt.itemsize), dtype=dt, count=count)
names = [n for _, n in props]
rest = sum(1 for n in names if n.startswith("f_rest_"))
sh_degree = {0: 0, 9: 1, 24: 2, 45: 3}.get(rest, None)
for need in ["x", "y", "z", "f_dc_0", "f_dc_1", "f_dc_2", "opacity", "scale_0", "scale_1", "scale_2", "rot_0", "rot_1", "rot_2", "rot_3"]:
    if need not in names:
        sys.exit(f"splat: Brush's ply has no {need}")
# written again in the reference 3DGS layout (INRIA's order: x y z nx ny nz f_dc_0..2 f_rest_0.. opacity scale_0..2
# rot_0..3, all float32): Brush writes its properties in name order (f_rest_0, f_rest_1, f_rest_10...), which readers
# that go by position get wrong
order = ["x", "y", "z", "nx", "ny", "nz", "f_dc_0", "f_dc_1", "f_dc_2", *[f"f_rest_{i}" for i in range(rest)], "opacity", "scale_0", "scale_1", "scale_2", "rot_0", "rot_1", "rot_2", "rot_3"]
std = np.zeros(count, dtype=np.dtype([(n, "<f4") for n in order]))
for n in order:
    if n in names:
        std[n] = data[n].astype(np.float32)
with open(f"{out}/splat.ply", "wb") as f:
    f.write(("ply\nformat binary_little_endian 1.0\ncomment raea splat (trained by Brush " + "v0.3.0" + ")\n"
             f"element vertex {count}\n" + "".join(f"property float {n}\n" for n in order) + "end_header\n").encode("ascii"))
    f.write(std.tobytes())
P = np.stack([data["x"], data["y"], data["z"]], 1).astype(np.float64)

# ---- the world: up, centre, scale -------------------------------------------------------------------------------------
cams = []  # (meta, R world->cam, C centre)
by_name = {m["name"]: m for m in meta}
for img in und.images.values():
    if not img.has_pose:
        continue
    T = img.cam_from_world() if callable(getattr(img, "cam_from_world", None)) else img.cam_from_world
    R = np.array(T.rotation.matrix())
    t = np.array(T.translation)
    key = os.path.splitext(img.name)[0] + ".png"
    cams.append((by_name.get(key) or by_name.get(img.name) or {"name": img.name, "frame": None, "time": None}, R, -R.T @ t, img.camera_id))
cams.sort(key=lambda c: c[0]["name"])
Cs = np.array([c[2] for c in cams])
rights = np.array([c[1][0] for c in cams])
downs = np.array([c[1][1] for c in cams])
fwds = np.array([c[1][2] for c in cams])
mean_up = -downs.mean(0)
mean_up /= np.linalg.norm(mean_up)
# up: level with every camera's right axis (no roll), the least-squares normal of the right axes
w, v = np.linalg.eigh(rights.T @ rights / len(cams))
if w[1] < 0.02:
    # the right axes barely turn (a dolly, a pan of a few degrees): the mean up, with the right axis taken out
    r0 = v[:, 2]
    up = mean_up - r0 * (mean_up @ r0)
    focus_note = "up: the cameras' mean up"
else:
    up = v[:, 0] * np.sign(v[:, 0] @ mean_up)
    focus_note = "up: level with the cameras' right axes"
up /= np.linalg.norm(up)
# centre: where the camera axes meet, if they meet near the sparse points
pts = np.array([p.xyz for p in und.points3D.values()]) if und.num_points3D() else np.zeros((0, 3))
A = np.zeros((3, 3))
b = np.zeros(3)
for C, d in zip(Cs, fwds):
    M = np.eye(3) - np.outer(d, d)
    A += M
    b += M @ C
ws = np.linalg.eigvalsh(A)
centre, how = None, ""
if ws[0] > 0.05 * len(cams):
    c = np.linalg.solve(A, b)
    # in front of the cameras, and among the points
    spread = np.median(np.linalg.norm(Cs - Cs.mean(0), axis=1)) + 1e-9
    ahead = np.mean([(c - C) @ d > 0 for C, d in zip(Cs, fwds)])
    near = len(pts) == 0 or np.linalg.norm(c - np.median(pts, 0)) < 4 * max(spread, np.median(np.linalg.norm(pts - np.median(pts, 0), axis=1)))
    if ahead > 0.8 and near:
        centre, how = c, "the point the cameras look at"
if centre is None:
    centre, how = (np.median(pts, 0) if len(pts) else P.mean(0)), "the median of the sparse points"
# the frame: Y up, the first camera on +Z
y = up
z0 = Cs[0] - centre
z = z0 - y * (z0 @ y)
if np.linalg.norm(z) < 1e-6:
    z = np.cross(rights[0], y)
z /= np.linalg.norm(z)
x = np.cross(y, z)
Rw = np.stack([x, y, z])  # rows: world axes in file coordinates, so Rw @ p takes file to world
dist = np.mean(np.linalg.norm(Cs - centre, axis=1))
scale = 4.0 / max(dist, 1e-9)
M4 = np.eye(4)
M4[:3, :3] = scale * Rw
M4[:3, 3] = -scale * Rw @ centre


def world(p):
    return (M4[:3, :3] @ np.asarray(p).T).T + M4[:3, 3]


cam_json = []
for (m, R, C, cid), d, dn in zip(cams, fwds, downs):
    cam = und.cameras[cid]
    fy = float(cam.params[1] if len(cam.params) > 3 else cam.params[0]) if cam.model.name == "PINHOLE" else float(cam.params[0])
    pos = world(C)
    fw = Rw @ d
    upv = Rw @ (-dn)
    look = pos + fw * float(np.linalg.norm(pos))
    cam_json.append({"frame": m.get("frame"), "time": m.get("time"), "position": [round(float(q), 5) for q in pos], "lookAt": [round(float(q), 5) for q in look],
                     "up": [round(float(q), 5) for q in upv], "fov": round(math.degrees(2 * math.atan(cam.height / (2 * fy))), 4), **({"file": m["file"]} if "file" in m else {})})
Pw = world(P)
lo, hi = np.percentile(Pw, 2, axis=0), np.percentile(Pw, 98, axis=0)
heights = np.array([c["position"][1] for c in cam_json])
say(f"world: {how}; {focus_note}; cameras {dist * scale:.2f} m out, at heights {heights.min():.2f}..{heights.max():.2f} m; splats in {np.round(lo, 2).tolist()}..{np.round(hi, 2).tolist()}")
if heights.mean() < -0.5:
    notes.append("the cameras sit below the centre on average: check up")
# Brush keeps COLMAP's frame: its splats' median should sit near the sparse points'
if len(pts):
    off = np.linalg.norm(np.median(P, 0) - np.median(pts, 0)) / (np.median(np.linalg.norm(pts - np.median(pts, 0), axis=1)) + 1e-9)
    if off > 1.0:
        notes.append(f"the splats' median is {off:.1f} point-spreads from the sparse points': Brush may have moved the frame")
cam0 = und.cameras[cams[0][3]]
side = {
    "version": 1,
    "transform": [round(float(q), 8) for q in M4.T.reshape(-1)],
    "count": count,
    "sh_degree": sh_degree,
    "cameras": cam_json,
    "width": int(cam0.width),
    "height": int(cam0.height),
    "bounds": {"min": [round(float(q), 4) for q in lo], "max": [round(float(q), 4) for q in hi]},
    "focus": how,
    "registered": reg,
    "frames": len(meta),
    "error": round(err, 4),
    "psnr": psnr,
    "steps": a.steps,
    "seconds": times,
    "source": src,
    "matte": a.matte,
    "notes": notes,
}
with open(f"{out}/splat.ply.json", "w") as f:
    json.dump(side, f, indent=1)
if a.keep:
    say(f"work kept in {work}")
else:
    keep = sorted(glob.glob(f"{bout}/**/*.png", recursive=True))[-6:]
    if keep:
        os.makedirs(f"{out}/eval", exist_ok=True)
        for k in keep:
            shutil.copy(k, f"{out}/eval/{os.path.basename(os.path.dirname(k))}-{os.path.basename(k)}")
    shutil.rmtree(work, ignore_errors=True)
say(f"done: {count} splats, SH degree {sh_degree}, in {time.time() - t0:.0f} s")
