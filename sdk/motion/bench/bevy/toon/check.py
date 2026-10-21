#!/usr/bin/env python3
"""The toon fixture's measurements (spec/motion.feature, "Toon surfaces and ink outlines"). Renders frames of
toon/edit.tsx with raea-motion and checks, in the pixels:

  bands        the far sphere (steps 3, no rim), scanned from its lit side to its dark side through its centre: three
               plateaus, darker away from the light, each edge between them at most 2 px wide
  hard         the near sphere (steps 2): its two bands meet in an edge at most 2 px wide
  outline      the ink band at the left and right of both spheres (the same size, one 9 units further back) is the
               outline's width (3 px) within 1 px, the same near and far
  closed       every pixel touching the background is background, ink, or a blend of the two: no surface colour shows
               through a gap in an outline (the box's hard edges included), at frames 0, 30 and 59
  still        the near sphere doesn't move and the light doesn't either: its pixels are identical at frames 10, 30 and
               50 (no band shimmer, no acne, no z-fighting between the hull and the surface)
  determinism  two renders of frame 30 are byte-identical

  python3 sdk/motion/bench/bevy/toon/check.py [RAEA_MOTION_BIN]     exits 1 on any failure
"""
import math, os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "../../../../.."))
BIN = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("RAEA_MOTION_BIN", os.path.join(ROOT, "sdk/react/target/release/raea-motion"))
EDIT = os.path.join(HERE, "edit.tsx")
W, H = 960, 540
OUT = tempfile.mkdtemp(prefix="raea-toon-check-")

# the fixture's camera and parts (edit.tsx)
CAM, LOOK, FOV = (0.0, 0.9, 7.5), (0.0, 0.2, 0.0), 40.0
NEAR, FAR, RADIUS = (0.2, 0.3, 0.0), (1.4, 1.6, -9.0), 0.75
LIGHT = (4.0, 5.0, 3.0)
INK_W = 3.0


def render(frame, tag=""):
    png = os.path.join(OUT, f"f{frame}{tag}.png")
    env = dict(os.environ, RAEA_READBACK="rgba")
    r = subprocess.run([BIN, "render", EDIT, "--frame", str(frame), "-o", png], env=env, capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"render of frame {frame} failed:\n{r.stderr[-2000:]}")
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", png, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], capture_output=True).stdout
    assert len(raw) == W * H * 3, (len(raw), png)
    return raw


def px(img, x, y):
    i = (y * W + x) * 3
    return img[i], img[i + 1], img[i + 2]


def luma(c):
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def sub(a, b):
    return [a[i] - b[i] for i in range(3)]


def norm(a):
    l = math.sqrt(sum(x * x for x in a))
    return [x / l for x in a]


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def dot(a, b):
    return sum(a[i] * b[i] for i in range(3))


def project(p):
    """A world point on screen (pixels), as Bevy's camera looking at LOOK draws it."""
    f = norm(sub(LOOK, CAM))
    r = norm(cross(f, [0, 1, 0]))
    u = cross(r, f)
    d = sub(p, CAM)
    x, y, z = dot(d, r), dot(d, u), dot(d, f)
    k = (H / 2) / math.tan(math.radians(FOV) / 2)
    return W / 2 + k * x / z, H / 2 - k * y / z, k * RADIUS / z


def scan(img, c, d, reach):
    """Pixels from the centre c out along d (a unit vector on screen), `reach` pixels, both ways: lit side first."""
    out = []
    for t in range(-int(reach), int(reach) + 1):
        x, y = round(c[0] + d[0] * t), round(c[1] + d[1] * t)
        out.append(px(img, x, y))
    return out


def plateaus(line, tol=6, least=3):
    """Runs of near-equal colour at least `least` pixels long: (start, end, colour), with the gaps between them."""
    runs, s = [], 0
    for i in range(1, len(line) + 1):
        if i == len(line) or max(abs(line[i][k] - line[i - 1][k]) for k in range(3)) > tol:
            if i - s >= least:
                runs.append((s, i - 1, line[s + (i - s) // 2]))
            s = i
    return runs


def light_dir_on_screen(centre3):
    """The direction, on screen, from a sphere's centre towards its lit side."""
    l = norm(sub(LIGHT, [0, 0, 0]))
    a = project(centre3)
    b = project([centre3[i] + 0.5 * l[i] for i in range(3)])
    return norm([b[0] - a[0], b[1] - a[1], 0])[:2]


failures = []


def check(name, ok, detail):
    print(f"{'PASS' if ok else 'FAIL'}  {name}: {detail}")
    if not ok:
        failures.append(name)


f30 = render(30)
f30b = render(30, "b")
check("determinism", f30 == f30b, "two renders of frame 30 byte-identical" if f30 == f30b else f"{sum(1 for i in range(len(f30)) if f30[i] != f30b[i])} bytes differ")

bg = px(f30, 4, 4)
ink_l = 45  # the ink (Oklab lightness 0.2) is about luma 30; the darkest band is above 60


# bands on the far sphere: from its lit side through its centre, inside the outline
fx, fy, fr = project(FAR)
d = light_dir_on_screen(FAR)
line = scan(f30, (fx, fy), d, fr - INK_W - 1)[::-1]  # lit side first
runs = [r for r in plateaus(line) if luma(r[2]) > ink_l]
lums = [round(luma(r[2]), 1) for r in runs]
edges = [runs[i + 1][0] - runs[i][1] - 1 for i in range(len(runs) - 1)]
check("bands", len(runs) == 3 and all(lums[i] > lums[i + 1] for i in range(len(lums) - 1)) and all(e <= 2 for e in edges),
      f"far sphere (steps 3), lit side first: {len(runs)} bands, luma {lums}, edges {edges} px wide")

# the near sphere's two bands meet in a hard edge
nx, ny, nr = project(NEAR)
d = light_dir_on_screen(NEAR)
line = scan(f30, (nx, ny), d, nr * 0.7)[::-1]
runs = [r for r in plateaus(line) if luma(r[2]) > ink_l]
lums = [round(luma(r[2]), 1) for r in runs]
edges = [runs[i + 1][0] - runs[i][1] - 1 for i in range(len(runs) - 1)]
check("hard", len(runs) == 2 and lums[0] > lums[1] and all(e <= 2 for e in edges), f"near sphere (steps 2), inner 70%: {len(runs)} bands, luma {lums}, edges {edges} px")


def ink_width(img, cx, cy, r, side):
    """Ink coverage across the silhouette on a horizontal line through the centre: sum over pixels of how dark each is
    between the background and the ink (an anti-aliased edge pixel counts in part)."""
    y = round(cy)
    xs = range(round(cx + side * (r - 8)), round(cx + side * (r + 10)), side)
    cols = [px(img, x, y) for x in xs]
    lb = luma(bg)
    li = min(luma(c) for c in cols)
    # pixels between the surface and the outline count up to the darkest; past it, against the background
    k = min(range(len(cols)), key=lambda i: luma(cols[i]))
    inner = [c for c in cols[:k] if luma(c) < 0.5 * (li + luma(cols[0]))]
    outer = cols[k:]
    cover = len(inner) + sum(min(1.0, max(0.0, (lb - luma(c)) / max(lb - li, 1))) for c in outer)
    return cover


widths = {f"{n} {s}": ink_width(f30, *project(p), 1 if s == "right" else -1) for n, p in (("near", NEAR), ("far", FAR)) for s in ("left", "right")}
ok = all(abs(w - INK_W) <= 1.0 for w in widths.values()) and abs(widths["near left"] - widths["far left"]) <= 1.0
check("outline", ok, "ink px at the silhouette " + ", ".join(f"{k} {v:.2f}" for k, v in widths.items()) + f" (want {INK_W} +- 1, near = far)")


def gaps(img):
    """Pixels touching the background that are neither background, ink nor a blend of the two."""
    lb = luma(bg)
    bad = 0
    edge = 0
    isbg = bytearray(W * H)
    for y in range(H):
        for x in range(W):
            c = px(img, x, y)
            isbg[y * W + x] = max(abs(c[k] - bg[k]) for k in range(3)) <= 3
    for y in range(1, H - 1):
        for x in range(1, W - 1):
            if isbg[y * W + x] or not (isbg[y * W + x - 1] or isbg[y * W + x + 1] or isbg[(y - 1) * W + x] or isbg[(y + 1) * W + x]):
                continue
            edge += 1
            c = px(img, x, y)
            a = 1 - luma(c) / lb  # how much ink, were it black
            if a < 0.08:
                bad += 1  # as bright as the background, but not it: a surface showing
                continue
            ink = [(c[k] - bg[k] * (1 - a)) / a for k in range(3)]
            if luma(ink) > 0.3 * 255 or min(ink) < -40:
                bad += 1
    return bad, edge


for fr_, img in ((0, render(0)), (30, f30), (59, render(59))):
    bad, edge = gaps(img)
    check(f"closed@{fr_}", bad <= max(2, edge // 1000), f"{bad} of {edge} pixels at the background's edge show a surface instead of ink")

# the still sphere: identical across frames
f10, f50 = render(10), render(50)
x0, x1 = round(nx - nr - 6), round(nx + nr + 6)
y0, y1 = round(ny - nr - 6), round(ny + nr + 6)
diff = 0
for y in range(y0, y1):
    for x in range(x0, x1):
        if px(f10, x, y) != px(f30, x, y) or px(f50, x, y) != px(f30, x, y):
            diff += 1
check("still", diff == 0, f"the near sphere's {(x1 - x0) * (y1 - y0)} pixels at frames 10, 30, 50: {diff} differ")

print(f"frames in {OUT}")
sys.exit(1 if failures else 0)
