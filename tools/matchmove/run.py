# Matchmove: the camera of a stretch of footage, solved into a world an agent can place things in (raea's asset store
# runs this; rdb::assets::Store::matchmove). Writes OUT/solve.json and, with a subject, OUT/cut/NNNN.png (the subject cut
# out of each frame at full size: the footage's pixels where the matte is, transparent elsewhere).
#
#   python run.py --video CLIP --from S --seconds N --out DIR [--subject X,Y,W,H --perceive BIN] [--height M]
#                 [--fov DEG] [--smooth FRAMES]
#
# How: frames at half size; the subject (an EdgeTAM matte, tracked from a box by `raea-perceive track`) masked out of
# the features; COLMAP (pycolmap) with one shared SIMPLE_RADIAL camera, sequential matching, incremental mapping. Frames
# it doesn't register get their pose by interpolation (slerp), and the path is smoothed lightly. Too little parallax
# (a pan, a tripod) falls back to a rotation-only solve: homographies from Lucas-Kanade flow, K^-1 H K. No rotation
# at all is a static camera. The world: the ground is the dominant plane of the sparse points facing up (RANSAC), at
# y = 0 with +Y up; the origin is where frame 0's centre ray meets the ground (else under the camera), frame 0 looks
# down -Z; scaled so the camera stands --height metres over the ground. Without points (static, rotation), the ground
# comes from the subject's feet: a person walking on it (their matte's foot line against their height in pixels) gives
# the horizon, hence the pitch.
#
# solve.json, Bevy's axes (a camera looks down its -Z with +Y up):
#   {version, mode: "sfm"|"rotation"|"static", width, height, fps, from, seconds, frames: N, fov (vertical, degrees),
#    focal (pixels at full size), k1, registered, error (mean reprojection, full-size pixels), camera_height, scale,
#    cameras: [{t, position: [x,y,z], quaternion: [x,y,z,w] (camera to world), solved: bool}] one per frame,
#    points: [[x,y,z]...] (up to 3000), ground: {y: 0, normal: [0,1,0]}, cut: "cut/%04d.png" | null, notes: [...]}
import argparse, glob, json, math, os, shutil, subprocess, sys, time

import numpy as np
from PIL import Image, ImageFilter

ap = argparse.ArgumentParser()
ap.add_argument("--video", required=True)
ap.add_argument("--from", dest="start", type=float, default=0.0)
ap.add_argument("--seconds", type=float, default=0.0)
ap.add_argument("--out", required=True)
ap.add_argument("--subject", default="")
ap.add_argument("--perceive", default="")
ap.add_argument("--height", type=float, default=1.6)
ap.add_argument("--fov", type=float, default=0.0)
ap.add_argument("--smooth", type=float, default=1.0)
a = ap.parse_args()
t_start = time.time()


def say(*m):
    print("matchmove:", *m, file=sys.stderr, flush=True)


out = os.path.abspath(a.out)
work = os.path.join(out, "work")
shutil.rmtree(work, ignore_errors=True)
os.makedirs(f"{work}/img")
notes = []

probe = json.loads(subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,r_frame_rate,duration,color_space", "-of", "json", a.video], capture_output=True, text=True, check=True).stdout)["streams"][0]
W, H = int(probe["width"]), int(probe["height"])
num, den = probe["r_frame_rate"].split("/")
fps = float(num) / float(den)
dur = float(probe.get("duration") or 0)
seconds = a.seconds if a.seconds > 0 else max(0.0, dur - a.start)
# the decoders' colour: BT.709 limited range unless the file says otherwise (ffmpeg would assume BT.601 when untagged)
matrix = {"bt470bg": "bt601", "smpte170m": "bt601"}.get(probe.get("color_space", ""), "bt709")
sw, sh = W // 2, H // 2
rng = ["-ss", f"{a.start:.6f}", "-i", a.video, "-t", f"{seconds:.6f}"]
subprocess.run(["ffmpeg", "-v", "error", "-y", *rng, "-vf", f"scale={sw}:{sh}:flags=area:in_color_matrix={matrix}:in_range=tv,format=rgb24", "-start_number", "1", f"{work}/img/%04d.png"], check=True)
frames = sorted(glob.glob(f"{work}/img/*.png"))
N = len(frames)
if N < 2:
    sys.exit(f"matchmove: {N} frames in {a.start}s + {seconds}s of {a.video}")
say(f"{N} frames at {sw}x{sh} ({fps:g} fps) from {a.start:g}s")

# ---- the subject's matte (EdgeTAM through raea-perceive), dilated and inverted as COLMAP's feature mask ----------------
mattes = None
if a.subject:
    box = [float(v) for v in a.subject.split(",")]
    if len(box) != 4:
        sys.exit("matchmove: --subject is X,Y,W,H in 0..1 of the frame")
    if not a.perceive or not os.path.exists(a.perceive):
        sys.exit(f"matchmove: the subject's matte needs raea-perceive (EdgeTAM), not found at {a.perceive!r}")
    os.makedirs(f"{work}/matte")
    r = subprocess.run([a.perceive, "track", f"{work}/matte", ",".join(map(str, box)), *frames], capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"matchmove: tracking the subject: {r.stderr.strip().splitlines()[-1] if r.stderr.strip() else r.returncode}")
    mattes = sorted(glob.glob(f"{work}/matte/*.png"))
    os.makedirs(f"{work}/mask")
    for i, m in enumerate(mattes):
        mm = Image.open(m).convert("L").resize((sw, sh), Image.BILINEAR).filter(ImageFilter.MaxFilter(15))
        # COLMAP keeps features only where the mask is non-zero: the subject (and its edge) is zero
        Image.eval(mm, lambda v: 0 if v > 60 else 255).save(f"{work}/mask/{os.path.basename(frames[i])}.png")
    say(f"subject tracked through {len(mattes)} frames")

# ---- structure from motion -------------------------------------------------------------------------------------------
import pycolmap

pycolmap.logging.minloglevel = 2  # COLMAP's progress is thousands of lines; warnings and errors only
model, err, k1, focal_half = None, None, 0.0, None
try:
    db = f"{work}/db.db"
    reader = pycolmap.ImageReaderOptions()
    reader.camera_model = "SIMPLE_RADIAL"
    if mattes:
        reader.mask_path = f"{work}/mask"
    ext = pycolmap.FeatureExtractionOptions()
    ext.sift.max_num_features = 8000
    ext.use_gpu = False
    pycolmap.extract_features(db, f"{work}/img", camera_mode=pycolmap.CameraMode.SINGLE, reader_options=reader, extraction_options=ext, device=pycolmap.Device.cpu)
    say(f"features extracted ({time.time() - t_start:.0f} s)")
    pair = pycolmap.SequentialPairingOptions()
    pair.overlap = 12
    pair.quadratic_overlap = True
    mo = pycolmap.FeatureMatchingOptions()
    mo.use_gpu = False
    pycolmap.match_sequential(db, matching_options=mo, pairing_options=pair, device=pycolmap.Device.cpu)
    say(f"frames matched ({time.time() - t_start:.0f} s)")
    progress = [0]

    def registered():
        progress[0] += 1
        if progress[0] % 10 == 0:
            say(f"{progress[0]} frames registered ({time.time() - t_start:.0f} s)")
    opts = pycolmap.IncrementalPipelineOptions()
    opts.multiple_models = False
    opts.random_seed = 1
    os.makedirs(f"{work}/sparse")
    maps = pycolmap.incremental_mapping(db, f"{work}/img", f"{work}/sparse", options=opts, next_image_callback=registered)
    if maps:
        model = max(maps.values(), key=lambda r: r.num_reg_images())
except Exception as e:  # a failed solve is a fallback, not an error
    notes.append(f"COLMAP failed: {e}")
    model = None

poses = {}  # frame index (0-based) -> (R_wc 3x3 OpenCV camera to world, C centre)
points = np.zeros((0, 3))
if model is not None and model.num_reg_images() > 0:
    cam = next(iter(model.cameras.values()))
    focal_half = float(cam.params[0])
    k1 = float(cam.params[3]) if len(cam.params) > 3 else 0.0
    err = float(model.compute_mean_reprojection_error()) * (W / sw)
    for img in model.images.values():
        if not img.has_pose:
            continue
        T = img.cam_from_world() if callable(getattr(img, "cam_from_world", None)) else img.cam_from_world
        R = np.array(T.rotation.matrix())
        t = np.array(T.translation)
        poses[int(img.name[:4]) - 1] = (R.T, -R.T @ t)
    points = np.array([p.xyz for p in model.points3D.values()]) if model.num_points3D() else np.zeros((0, 3))
    say(f"COLMAP registered {len(poses)} of {N} frames, {len(points)} points, error {err:.2f} px, f {focal_half * 2:.0f} px, k1 {k1:.3f}")

mode = "sfm"
if len(poses) >= max(3, 0.5 * N) and len(points) >= 50:
    C = np.array([poses[i][1] for i in sorted(poses)])
    baseline = float(np.max(np.linalg.norm(C - C[0], axis=1)))
    depth = float(np.median(np.linalg.norm(points - C.mean(0), axis=1)))
    if baseline < 0.01 * depth:
        mode = "rotation"
        notes.append(f"too little parallax for structure (baseline {baseline / depth:.4f} of the scene's depth): rotation only")
else:
    mode = "rotation"
    notes.append(f"COLMAP registered {len(poses)} of {N} frames: falling back to a rotation-only solve")

# ---- the rotation-only fallback: homographies from Lucas-Kanade flow ------------------------------------------------
if mode != "sfm":
    import cv2

    fv = a.fov if a.fov > 0 else (math.degrees(2 * math.atan(sh / 2 / focal_half)) if (focal_half and len(poses) >= 0.5 * N) else 40.0)
    if a.fov <= 0:
        notes.append(f"vertical field of view {'from COLMAP' if (focal_half and len(poses) >= 0.5 * N) else 'assumed'}: {fv:.1f} degrees (--fov to set it)")
    focal_half = sh / 2 / math.tan(math.radians(fv) / 2)
    K = np.array([[focal_half, 0, sw / 2], [0, focal_half, sh / 2], [0, 0, 1]])
    Ki = np.linalg.inv(K)
    Rs = [np.eye(3)]  # world-to-camera rotations, frame 0 the world
    prev = cv2.cvtColor(cv2.imread(frames[0]), cv2.COLOR_BGR2GRAY)
    total = 0.0
    for i in range(1, N):
        cur = cv2.cvtColor(cv2.imread(frames[i]), cv2.COLOR_BGR2GRAY)
        m = None
        if mattes:
            m = cv2.imread(f"{work}/mask/{os.path.basename(frames[i - 1])}.png", cv2.IMREAD_GRAYSCALE)
        p0 = cv2.goodFeaturesToTrack(prev, 1500, 0.005, 8, mask=m)
        Rr = np.eye(3)
        if p0 is not None and len(p0) >= 12:
            p1, st, _ = cv2.calcOpticalFlowPyrLK(prev, cur, p0, None, winSize=(21, 21), maxLevel=3)
            ok = st.ravel() == 1
            if ok.sum() >= 12:
                Hm, inl = cv2.findHomography(p0[ok], p1[ok], cv2.RANSAC, 1.0)
                if Hm is not None:
                    M = Ki @ Hm @ K
                    U, _, Vt = np.linalg.svd(M)
                    Rr = U @ Vt
                    if np.linalg.det(Rr) < 0:
                        Rr = -Rr
        total += math.degrees(math.acos(max(-1.0, min(1.0, (np.trace(Rr) - 1) / 2))))
        Rs.append(Rr @ Rs[-1])
        prev = cur
    if total < 0.3:
        mode = "static"
        notes.append(f"the camera doesn't move (it turns {total:.2f} degrees in all): a static camera")
        Rs = [np.eye(3)] * N
    poses = {i: (Rs[i].T, np.zeros(3)) for i in range(N)}
    points = np.zeros((0, 3))
    err = None
    k1 = 0.0

# ---- fill, smooth ----------------------------------------------------------------------------------------------------


def quat_of(R):
    # (x, y, z, w) of a rotation matrix
    tr = np.trace(R)
    if tr > 0:
        s = math.sqrt(tr + 1.0) * 2
        q = [(R[2, 1] - R[1, 2]) / s, (R[0, 2] - R[2, 0]) / s, (R[1, 0] - R[0, 1]) / s, 0.25 * s]
    elif R[0, 0] > R[1, 1] and R[0, 0] > R[2, 2]:
        s = math.sqrt(1.0 + R[0, 0] - R[1, 1] - R[2, 2]) * 2
        q = [0.25 * s, (R[0, 1] + R[1, 0]) / s, (R[0, 2] + R[2, 0]) / s, (R[2, 1] - R[1, 2]) / s]
    elif R[1, 1] > R[2, 2]:
        s = math.sqrt(1.0 + R[1, 1] - R[0, 0] - R[2, 2]) * 2
        q = [(R[0, 1] + R[1, 0]) / s, 0.25 * s, (R[1, 2] + R[2, 1]) / s, (R[0, 2] - R[2, 0]) / s]
    else:
        s = math.sqrt(1.0 + R[2, 2] - R[0, 0] - R[1, 1]) * 2
        q = [(R[0, 2] + R[2, 0]) / s, (R[1, 2] + R[2, 1]) / s, 0.25 * s, (R[1, 0] - R[0, 1]) / s]
    q = np.array(q)
    return q / np.linalg.norm(q)


def mat_of(q):
    x, y, z, w = q
    return np.array([[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
                     [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
                     [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]])


def slerp(q0, q1, u):
    d = float(np.dot(q0, q1))
    if d < 0:
        q1, d = -q1, -d
    if d > 0.9995:
        q = q0 + u * (q1 - q0)
        return q / np.linalg.norm(q)
    th = math.acos(d)
    return (math.sin((1 - u) * th) * q0 + math.sin(u * th) * q1) / math.sin(th)


def ground_plane(points, up, above, tries=3000, seed=1):
    """The ground in sparse points: the plane (RANSAC, then refit by SVD) holding the most points among those facing
    roughly `up` (walls don't), its normal turned toward the point `above` (a camera). (normal, a point on it, how many
    points it holds), or None with under 30 points or under 20 on any plane."""
    if len(points) < 30:
        return None
    rs = np.random.default_rng(seed)
    c = np.median(points, axis=0)
    spread = float(np.median(np.linalg.norm(points - c, axis=1)))
    thr = 0.02 * spread
    best = (0, None)
    for _ in range(tries):
        s = points[rs.choice(len(points), 3, replace=False)]
        n = np.cross(s[1] - s[0], s[2] - s[0])
        if np.linalg.norm(n) < 1e-12:
            continue
        n /= np.linalg.norm(n)
        if abs(np.dot(n, up)) < 0.75:
            continue
        inl = np.abs((points - s[0]) @ n) < thr
        if inl.sum() > best[0]:
            best = (int(inl.sum()), inl)
    if best[1] is None or best[0] < 20:
        return None
    Pin = points[best[1]]
    cen = Pin.mean(0)
    n = np.linalg.svd(Pin - cen)[2][2]
    if np.dot(n, up) < 0:
        n = -n
    if np.dot(above - cen, n) < 0:  # the cameras are above the ground
        n = -n
    return (n, cen, best[0])


solved = sorted(poses)
Q = np.zeros((N, 4))
P = np.zeros((N, 3))
for i in range(N):
    if i in poses:
        Q[i], P[i] = quat_of(poses[i][0]), poses[i][1]
        continue
    lo = max([j for j in solved if j < i], default=None)
    hi = min([j for j in solved if j > i], default=None)
    if lo is None or hi is None:
        j = hi if lo is None else lo
        Q[i], P[i] = quat_of(poses[j][0]), poses[j][1]
    else:
        u = (i - lo) / (hi - lo)
        Q[i] = slerp(quat_of(poses[lo][0]), quat_of(poses[hi][0]), u)
        P[i] = (1 - u) * poses[lo][1] + u * poses[hi][1]
filled = N - len(solved)
if filled:
    notes.append(f"{filled} frames COLMAP didn't register were interpolated between their neighbours")
for i in range(1, N):  # one hemisphere, so averages don't cancel
    if np.dot(Q[i], Q[i - 1]) < 0:
        Q[i] = -Q[i]
if a.smooth > 0 and N > 2:
    r = max(1, int(math.ceil(3 * a.smooth)))
    k = np.exp(-0.5 * (np.arange(-r, r + 1) / a.smooth) ** 2)

    def blur(X):
        Y = np.zeros_like(X)
        for i in range(N):
            idx = np.clip(np.arange(i - r, i + r + 1), 0, N - 1)
            Y[i] = (X[idx] * k[:, None]).sum(0) / k.sum()
        return Y

    Q = blur(Q)
    Q /= np.linalg.norm(Q, axis=1, keepdims=True)
    P = blur(P)

# ---- the world: ground at y = 0, +Y up, metres -------------------------------------------------------------------------
Rwc = [mat_of(q) for q in Q]  # OpenCV camera (x right, y down, z forward) to world
up_cams = np.mean([-R[:, 1] for R in Rwc], axis=0)
up_cams /= np.linalg.norm(up_cams)
fwd0 = Rwc[0][:, 2]
ground = ground_plane(points, up_cams, P[0])
if ground is not None:
    notes.append(f"ground: {ground[2]} of {len(points)} points on one plane (RANSAC)")
if ground is None:
    notes.append("ground: no plane in the points; taken level under the camera (from the subject's feet when there is one)")

# the camera's pitch and height without points: the subject's feet against their height in pixels
pitch_from_feet, height_from_feet = None, None
if ground is None and mattes:
    rows = []
    for m in mattes:
        g = np.asarray(Image.open(m).convert("L")) > 127
        ys = np.where(g.any(1))[0]
        if len(ys) < 4:
            continue
        mh = g.shape[0]
        top, bot = ys[0] / mh, (ys[-1] + 1) / mh
        if bot < 0.985 and top > 0.005:  # whole, feet in frame
            rows.append((bot * sh, (bot - top) * sh))
    if len(rows) >= 8:
        A = np.array(rows)
        k_, horizon = np.polyfit(A[:, 1], A[:, 0], 1)  # feet_y = horizon + k * height_px (k = camera height / person height)
        if 0 < k_ < 3:
            pitch_from_feet = math.atan((horizon - sh / 2) / focal_half)  # horizon above centre: the camera looks down
            height_from_feet = k_ * 1.7
            notes.append(f"from the subject's feet: horizon at {horizon / sh:.3f} of the frame (pitch {math.degrees(-pitch_from_feet):.1f} degrees down), camera {height_from_feet:.2f} m up for a 1.7 m person")

if ground is not None:
    n, cen, _ = ground
else:
    # level: up is the cameras' mean up, tilted by the pitch the feet gave
    n = up_cams.copy()
    if pitch_from_feet is not None:
        # rotate frame 0's up about its x axis by the pitch (camera looks down by -pitch)
        ax = Rwc[0][:, 0]
        th = pitch_from_feet
        n = n * math.cos(th) + np.cross(ax, n) * math.sin(th) + ax * np.dot(ax, n) * (1 - math.cos(th))
        n /= np.linalg.norm(n)
    cam_h = height_from_feet if (height_from_feet and a.height == 1.6) else a.height
    cen = P[0] - n * cam_h  # a point on the ground, cam_h under frame 0's camera (in solve units; scale 1 below)

# the world's axes: +Y = n; -Z = frame 0's forward along the ground; X = Y x Z
yax = n / np.linalg.norm(n)
f_on = fwd0 - yax * np.dot(fwd0, yax)
if np.linalg.norm(f_on) < 1e-6:
    f_on = Rwc[0][:, 1] - yax * np.dot(Rwc[0][:, 1], yax)
zax = -f_on / np.linalg.norm(f_on)
xax = np.cross(yax, zax)
Rg = np.stack([xax, yax, zax])  # world rows
h0 = float(np.dot(P[0] - cen, yax))
want_h = a.height if (ground is not None or not height_from_feet or a.height != 1.6) else height_from_feet
scale = want_h / h0 if h0 > 1e-9 else 1.0
# origin: where frame 0's centre ray meets the ground, if it does within 60 camera heights; else under the camera
d = float(np.dot(fwd0, yax))
foot = P[0] - yax * h0
origin = foot
if d < -1e-6:
    s = h0 / -d
    if s < 60 * h0:
        origin = P[0] + fwd0 * s
        notes.append(f"origin: where frame 0's centre ray meets the ground, {s * scale:.1f} m from the camera")
F = np.diag([1.0, -1.0, -1.0])  # OpenCV camera axes to Bevy's
cams = []
for i in range(N):
    Rb = Rg @ Rwc[i] @ F
    pos = scale * (Rg @ (P[i] - origin))
    cams.append({"t": round(a.start + i / fps, 6), "position": [round(float(v), 5) for v in pos], "quaternion": [round(float(v), 7) for v in quat_of(Rb)], "solved": bool(i in poses)})
pts = []
if len(points):
    sel = points if len(points) <= 3000 else points[np.random.default_rng(2).choice(len(points), 3000, replace=False)]
    pts = [[round(float(v), 4) for v in scale * (Rg @ (p - origin))] for p in sel]

fov_v = math.degrees(2 * math.atan(sh / 2 / focal_half))

# ---- the subject cut out at full size (the footage's pixels where the matte is) --------------------------------------
cut = None
if mattes:
    os.makedirs(f"{out}/cut", exist_ok=True)
    import cv2

    undistort = None
    if k1 != 0:
        # each undistorted output pixel's place in the footage: SIMPLE_RADIAL, centred, at full size
        fF = focal_half * W / sw
        u, v = np.meshgrid(np.arange(W, dtype=np.float32), np.arange(H, dtype=np.float32))
        xn, yn = (u - W / 2) / fF, (v - H / 2) / fF
        d = 1 + k1 * (xn * xn + yn * yn)
        undistort = ((xn * d * fF + W / 2).astype(np.float32), (yn * d * fF + H / 2).astype(np.float32))
    raw = subprocess.run(["ffmpeg", "-v", "error", *rng, "-vf", f"scale=in_color_matrix={matrix}:in_range=tv,format=rgb24", "-f", "rawvideo", "-"], capture_output=True, check=True).stdout
    fb = W * H * 3
    for i in range(min(len(mattes), len(raw) // fb)):
        rgb = np.frombuffer(raw, np.uint8, fb, i * fb).reshape(H, W, 3)
        m = Image.open(mattes[i]).convert("L").resize((W, H), Image.BILINEAR).filter(ImageFilter.GaussianBlur(1.2))
        al = np.asarray(m).copy()
        al[al < 12] = 0  # the blur's faint spill (and EdgeTAM's low scores) isn't the subject
        rgba = np.dstack([rgb * (al[..., None] > 0), al]).astype(np.uint8)
        if undistort is not None:  # the plate is drawn undistorted (k1): the cut-out is too, so they stay aligned
            rgba = cv2.remap(rgba, undistort[0], undistort[1], cv2.INTER_LINEAR, borderMode=cv2.BORDER_CONSTANT, borderValue=0)
        Image.fromarray(rgba, "RGBA").save(f"{out}/cut/{i + 1:04d}.png", compress_level=4)
    cut = "cut/%04d.png"

solve = {
    "version": 1, "mode": mode, "width": W, "height": H, "fps": fps, "from": a.start, "seconds": round(N / fps, 6), "frames": N,
    "fov": round(fov_v, 5), "focal": round(focal_half * W / sw, 3), "k1": round(k1, 5),
    "registered": len(solved) if mode == "sfm" else N, "error": round(err, 3) if err is not None else None,
    "camera_height": round(float(want_h), 3), "scale": round(float(scale), 6),
    "ground": {"y": 0, "normal": [0, 1, 0]}, "cameras": cams, "points": pts, "cut": cut, "notes": notes,
    "seconds_to_solve": round(time.time() - t_start, 1),
}
json.dump(solve, open(f"{out}/solve.json", "w"))
shutil.rmtree(work, ignore_errors=True)
say(f"{mode}: {solve['registered']} of {N} frames, error {solve['error']}, fov {fov_v:.1f}; {'; '.join(notes)}")
