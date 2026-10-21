#!/usr/bin/env python3
"""The layers fixture's measurements (spec/motion.feature, "Page layers under and over the 3D"). Renders frames of
layers/edit.tsx, and of a copy with the Canvas3D taken out (the page alone), and checks:

  under        the page under the 3D is the page alone, byte for byte, everywhere outside the sphere's silhouette (its
               projected circle, 3 px of anti-aliasing allowed)
  occlusion    inside the silhouette the sphere hides the orange disc (no pixel there is the disc's orange)
  over         the green bar after the Canvas3D is drawn over the sphere: its pixels across the sphere are its green
  determinism  two renders of frame 30 are byte-identical

  python3 sdk/motion/bench/bevy/layers/check.py [RAEA_MOTION_BIN]     exits 1 on any failure
"""
import math, os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "../../../../.."))
BIN = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("RAEA_MOTION_BIN", os.path.join(ROOT, "sdk/react/target/release/raea-motion"))
EDIT = os.path.join(HERE, "edit.tsx")
W, H = 960, 540
OUT = tempfile.mkdtemp(prefix="raea-layers-check-")
SRC = open(EDIT).read()
FRAMES = 60
CAM_Z, FOV, R = 6.0, 40.0, 1.1


def page_only():
    """The edit without its Canvas3D (the lines from the first `layers:3d` marker to the second)."""
    out, skip = [], False
    for l in SRC.splitlines():
        if "{/* layers:3d */}" in l:
            skip = not skip
            if not skip:
                continue
        if not skip:
            out.append(l)
    p = os.path.join(HERE, ".page-only.tsx")
    open(p, "w").write("\n".join(out).replace("AmbientLight, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, Sphere", "Canvas3D"))
    return p


def render(edit, frame, tag):
    png = os.path.join(OUT, f"{tag}-f{frame}.png")
    r = subprocess.run([BIN, "render", edit, "--frame", str(frame), "-o", png], env=dict(os.environ, RAEA_READBACK="rgba"), capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"render of frame {frame} ({tag}) failed:\n{r.stderr[-2000:]}")
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", png, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], capture_output=True).stdout
    assert len(raw) == W * H * 3
    return raw


def px(img, x, y):
    i = (y * W + x) * 3
    return img[i], img[i + 1], img[i + 2]


failures = []


def check(name, ok, detail):
    print(f"{'PASS' if ok else 'FAIL'}  {name}: {detail}")
    if not ok:
        failures.append(name)


f = 30
layered = render(EDIT, f, "layered")
po = page_only()
try:
    alone = render(po, f, "page")
finally:
    os.remove(po)

# the sphere on screen: its centre's x (interpolate -0.6..0.6 over the frames) and its silhouette's radius
x3 = -0.6 + 1.2 * f / (FRAMES - 1)
k = (H / 2) / math.tan(math.radians(FOV) / 2)
cx, cy = W / 2 + k * x3 / CAM_Z, H / 2
rad = k * R / math.sqrt(CAM_Z * CAM_Z - R * R)

outside = diff_out = inside = disc_inside = 0
for y in range(H):
    for x in range(W):
        d = math.hypot(x + 0.5 - cx, y + 0.5 - cy)
        if d > rad + 3:
            outside += 1
            if px(layered, x, y) != px(alone, x, y):
                diff_out += 1
        elif d < rad - 3 and not (250 <= y < 290):
            inside += 1
            r, g, b = px(layered, x, y)
            if r > 220 and 110 < g < 160 and b < 60:
                disc_inside += 1
check("under", diff_out == 0, f"{diff_out} of {outside} pixels outside the sphere differ from the page alone")
check("occlusion", inside > 1000 and disc_inside == 0, f"{disc_inside} of {inside} pixels inside the sphere show the orange disc")
bar = [px(layered, x, 270) for x in range(int(cx - rad + 6), int(cx + rad - 6))]
greens = sum(1 for c in bar if c == px(alone, 20, 270))
check("over", greens == len(bar), f"{greens} of {len(bar)} pixels of the bar across the sphere are its green")

again = render(EDIT, f, "again")
check("determinism", again == layered, "two renders of frame 30 byte-identical" if again == layered else "they differ")

print(f"frames in {OUT}")
sys.exit(1 if failures else 0)
