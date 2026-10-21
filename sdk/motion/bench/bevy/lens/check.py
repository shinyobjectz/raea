#!/usr/bin/env python3
"""The lens fixture's measurements (spec/motion.feature, "The camera's lens"). Renders frames of lens/edit.tsx and
checks, in the pixels:

  off          frame 0 (Fisheye at strength 0) is byte-identical to the same frame with the Fisheye taken out
  straight     at frame 59 (strength 1) the horizontal and vertical lines through the centre stay on the centre row and
               column, within 1 px, all the way across
  curves       at frame 59 the horizontal line one unit above the centre bows outward: its distance from the centre row
               never grows from the middle towards either side, and the ends are clearly nearer (barrel); the same for
               the vertical line one unit right of the centre
  page         the 2D label over the 3D is pixel-identical at frames 0 and 59 (the page isn't bent)
  determinism  two renders of frame 59 are byte-identical

  python3 sdk/motion/bench/bevy/lens/check.py [RAEA_MOTION_BIN]     exits 1 on any failure
"""
import os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "../../../../.."))
BIN = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("RAEA_MOTION_BIN", os.path.join(ROOT, "sdk/react/target/release/raea-motion"))
EDIT = os.path.join(HERE, "edit.tsx")
W, H = 960, 540
OUT = tempfile.mkdtemp(prefix="raea-lens-check-")


def render(edit, frame, tag=""):
    png = os.path.join(OUT, f"f{frame}{tag}.png")
    r = subprocess.run([BIN, "render", edit, "--frame", str(frame), "-o", png], env=dict(os.environ, RAEA_READBACK="rgba"), capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"render of frame {frame} failed:\n{r.stderr[-2000:]}")
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", png, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], capture_output=True).stdout
    assert len(raw) == W * H * 3
    return raw


def lum(img, x, y):
    i = (y * W + x) * 3
    return 0.2126 * img[i] + 0.7152 * img[i + 1] + 0.0722 * img[i + 2]


failures = []


def check(name, ok, detail):
    print(f"{'PASS' if ok else 'FAIL'}  {name}: {detail}")
    if not ok:
        failures.append(name)


# off: the same edit without its Fisheye
src = open(EDIT).read()
off_edit = os.path.join(HERE, ".off-edit.tsx")
open(off_edit, "w").write("\n".join(l for l in src.splitlines() if "{/* lens */}" not in l))
try:
    f0, f0_off = render(EDIT, 0), render(off_edit, 0, "off")
finally:
    os.remove(off_edit)
check("off", f0 == f0_off, "strength 0 = no Fisheye, byte for byte" if f0 == f0_off else f"{sum(1 for a, b in zip(f0, f0_off) if a != b)} bytes differ")

f59, f59b = render(EDIT, 59), render(EDIT, 59, "b")
check("determinism", f59 == f59b, "two renders of frame 59 byte-identical" if f59 == f59b else "they differ")


def line_y(img, x, y0, half=8):
    """The darkest row near y0 in column x (the centre of a line's run), or None where a crossing line fills it."""
    ys = range(max(0, y0 - half), min(H, y0 + half + 1))
    vals = [(lum(img, x, y), y) for y in ys]
    dark = [y for v, y in vals if v < 60]
    if not dark or len(dark) > 6:
        return None
    return sum(dark) / len(dark)


def line_x(img, y, x0, half=8):
    xs = range(max(0, x0 - half), min(W, x0 + half + 1))
    dark = [x for x in xs if lum(img, x, y) < 60]
    if not dark or len(dark) > 6:
        return None
    return sum(dark) / len(dark)


cy, cx = H // 2, W // 2
ys = [y for y in (line_y(f59, x, cy) for x in range(8, W - 8, 4)) if y is not None]
xs = [x for x in (line_x(f59, y, cx) for y in range(80, H - 8, 4)) if x is not None]
dy = max(abs(y - (cy - 0.5)) for y in ys)
dx = max(abs(x - (cx - 0.5)) for x in xs)
check("straight", dy <= 1.0 and dx <= 1.0 and len(ys) > 150 and len(xs) > 80, f"centre row line off by at most {dy:.2f} px over {len(ys)} columns, centre column line {dx:.2f} px over {len(xs)} rows")


def trace(img, start, along, find):
    """Follow a line from the centre outwards both ways: its offset from the centre at each step."""
    out = {}
    for sign in (1, -1):
        cur = start
        for k in range(0, 470, 2):
            t = (cx if along == "x" else cy) + sign * k
            if not (8 <= t < (W if along == "x" else H) - 8):
                break
            v = find(img, t, round(cur))
            if v is None:
                continue
            cur = v
            out[sign * k] = v
    return out


# the line one unit above the centre: at strength 0 it is at frame 0's row
y1 = next(y for y in range(cy - 4, 0, -1) if lum(f0, cx + 37, y) < 60)
xs1 = next(x for x in range(cx + 4, W) if lum(f0, x, cy + 37) < 60)
row = trace(f59, line_y(f59, cx + 2, y1, 30) or y1, "x", line_y)
col = trace(f59, line_x(f59, cy + 2, xs1, 30) or xs1, "y", line_x)


def monotone(t, centre):
    ok = True
    worst = 0.0
    for sign in (1, -1):
        ks = sorted(k for k in t if k * sign >= 0)
        if sign < 0:
            ks = ks[::-1]
        d = [abs(t[k] - centre) for k in ks]
        for a, b in zip(d, d[1:]):
            if b > a + 0.75:  # half a pixel of sampling noise, then a quarter
                ok = False
                worst = max(worst, b - a)
    mid = abs(t[min(t, key=abs)] - centre)
    ends = [abs(t[max(t)] - centre), abs(t[min(t)] - centre)]
    return ok and max(ends) < mid - 5, mid, ends, worst


ok_r, mid_r, ends_r, w_r = monotone(row, cy)
ok_c, mid_c, ends_c, w_c = monotone(col, cx)
check("curves", ok_r and ok_c, f"line above centre: {mid_r:.1f} px from the centre row in the middle, {ends_r[0]:.1f} / {ends_r[1]:.1f} at the ends ({len(row)} samples); line right of centre: {mid_c:.1f} px mid, {ends_c[0]:.1f} / {ends_c[1]:.1f} ends ({len(col)} samples); worst step back {max(w_r, w_c):.2f} px")

diff = sum(1 for y in range(24, 68) for x in range(24, 204) if f0[(y * W + x) * 3:(y * W + x) * 3 + 3] != f59[(y * W + x) * 3:(y * W + x) * 3 + 3])
check("page", diff == 0, f"the label's {180 * 44} pixels at frames 0 and 59: {diff} differ")

print(f"frames in {OUT}")
sys.exit(1 if failures else 0)
