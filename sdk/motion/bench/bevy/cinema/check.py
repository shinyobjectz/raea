#!/usr/bin/env python3
"""The cinema fixture's measurements (spec/motion.feature, "Motion blur and toon surfaces under the camera's
effects"). Renders frames of cinema/edit.tsx, and of copies with one effect's lines taken out, and checks, in the
pixels:

  off:mblur    frame 10, where MotionBlur is absent (it comes in at frame 20, so the edit draws each frame after the one
               before), is byte-identical to the same frame with the MotionBlur taken out of the edit
  on:NAME      at frame 45 each effect changes the picture (against the copy without it)
  ao           AmbientOcclusion changes frame 10 too (it is on throughout, and turns MSAA off for the piece)
  blur         at frame 45 MotionBlur changes the moving ball's pixels and no others (Bevy's blur smears inside a
               moving part; its silhouette and toon ink, which have no motion vectors, stay sharp)
  hard         the window's patch of sun on the floor (a toon band edge) stays a hard edge, at most 3 px from floor to
               patch, under the fog, the grade, motion blur and AO at frame 45 (depth of field softens it on purpose)
  determinism  two renders of frame 45 (every effect on) are byte-identical

  python3 sdk/motion/bench/bevy/cinema/check.py [RAEA_MOTION_BIN]     exits 1 on any failure
"""
import os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "../../../../.."))
BIN = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("RAEA_MOTION_BIN", os.path.join(ROOT, "sdk/react/target/release/raea-motion"))
EDIT = os.path.join(HERE, "edit.tsx")
W, H = 960, 540
OUT = tempfile.mkdtemp(prefix="raea-cinema-check-")
SRC = open(EDIT).read()


def variant(without):
    """The edit with the lines of these effects taken out (written next to it, so its imports resolve)."""
    if not without:
        return EDIT
    p = os.path.join(HERE, f".without-{'-'.join(without)}.tsx")
    open(p, "w").write("\n".join(l for l in SRC.splitlines() if not any(f"{{/* fx:{w} */}}" in l for w in without)))
    return p


def render(without, frame, tag=""):
    edit = variant(without)
    png = os.path.join(OUT, f"{'-'.join(without) or 'all'}-f{frame}{tag}.png")
    try:
        r = subprocess.run([BIN, "render", edit, "--frame", str(frame), "-o", png], env=dict(os.environ, RAEA_READBACK="rgba"), capture_output=True, text=True)
    finally:
        if edit != EDIT:
            os.remove(edit)
    if r.returncode != 0:
        sys.exit(f"render of frame {frame} without {without} failed:\n{r.stderr[-2000:]}")
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", png, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], capture_output=True).stdout
    assert len(raw) == W * H * 3
    return raw


def lum(img, x, y):
    i = (y * W + x) * 3
    return 0.2126 * img[i] + 0.7152 * img[i + 1] + 0.0722 * img[i + 2]


def ndiff(a, b):
    return sum(1 for i in range(0, len(a), 3) if a[i:i + 3] != b[i:i + 3])


failures = []


def check(name, ok, detail):
    print(f"{'PASS' if ok else 'FAIL'}  {name}: {detail}")
    if not ok:
        failures.append(name)


EFFECTS = ["vfog", "dof", "grade", "mblur"]
all10, all45 = render([], 10), render([], 45)
d = ndiff(all10, render(["mblur"], 10))
check("off:mblur", d == 0, f"frame 10 with the blur off vs without it: {d} pixels differ")
for fx in EFFECTS:
    d = ndiff(all45, render([fx], 45))
    check(f"on:{fx}", d > W * H // 100, f"frame 45 with vs without it: {d} pixels ({100 * d / (W * H):.1f}%) differ")
d = ndiff(all10, render(["ao"], 10))
check("ao", d > 0, f"frame 10 with vs without AmbientOcclusion: {d} pixels differ")

# motion blur alone: it changes the moving ball and nothing else (the walls, the hero and the floor stand still)
blur, sharp = render(["vfog", "dof", "grade"], 45), render(["vfog", "dof", "grade", "mblur"], 45)
changed = [(i // 3 % W, i // 3 // W) for i in range(0, len(blur), 3) if blur[i:i + 3] != sharp[i:i + 3]]
# the ball at frame 45: x = -1.1, y = 0.5, z = 3: on screen about (230, 470), radius about 80 px
inside = sum(1 for x, y in changed if 120 <= x <= 340 and 370 <= y < H)
check("blur", inside > 2000 and inside == len(changed), f"MotionBlur changes {inside} pixels around the moving ball and {len(changed) - inside} elsewhere")

# the floor's patch of sun: found at frame 10 (brightest floor pixels), measured at frame 45 down its centre column
# the patch is the floor's 55-75 luma pixels below the hero at frame 10 without depth of field (on from the first frame,
# focused on the back wall there: the floor is soft), against the shade's 28-35 and the ball's 47-52; its bottom (near)
# edge, down a column through it
sharp10 = render(["dof"], 10)
patch = [(x, y) for y in range(430, H - 10) for x in range(300, 760, 2) if 55 <= lum(sharp10, x, y) <= 75]
if not patch:
    check("hard", False, "no sunlit patch found on the floor at frame 10")
else:
    cx = sorted(p[0] for p in patch)[len(patch) // 2]
    ybot = max(p[1] for p in patch if p[0] == cx)
    # under the fog, the grade, motion blur and AO (depth of field would blur it on purpose: it isn't in focus)
    nodof = render(["dof"], 45)
    col = [lum(nodof, cx, y) for y in range(ybot - 8, ybot + 8)]
    lo, hi = min(col), max(col)
    a, b = lo + 0.2 * (hi - lo), lo + 0.8 * (hi - lo)
    mid = sum(1 for v in col if a < v < b)
    check("hard", hi - lo > 15 and mid <= 3, f"the patch's near edge at x {cx}, y {ybot}: luma {hi:.0f} -> {lo:.0f} over {mid} px between 20% and 80%")

again = render([], 45, "b")
check("determinism", all45 == again, "two renders of frame 45 byte-identical" if all45 == again else f"{ndiff(all45, again)} pixels differ")

print(f"frames in {OUT}")
sys.exit(1 if failures else 0)
