#!/usr/bin/env python3
"""The studio fixture's measurements (spec/motion.feature, "Studio light"). Renders frames of studio/edit.tsx, and
of copies with a feature taken out (soft shadows hard, the Environment or AmbientOcclusion removed), and checks:

  env:off      frame 5 (the Environment at intensity 0) is byte-identical to the frame without the Environment
  env:fill     at frame 30 the Environment lifts the banana's cast shadow (its darkest tenth brighter by 10+ levels)
  soft         soft shadows change the cast shadow's edge (40+ of its 4x4 blocks, averaged: PCSS's penumbra is
               dithered) and make it gentler: its steepest block-to-block step at least 1.4x smaller than hard's
  ao           AmbientOcclusion darkens the cast shadow and the contact under the banana (its mean by 3+ levels) and
               leaves the far cyc as it is (within 1 level)
  highkey      the cyc near the camera reads #f7f7f5 within 4 levels a channel, towards the horizon pure white, and
               the banana's lit peel keeps its colour (max - min channel 120 or more: saturated, not washed out)
  determinism  two renders of frame 30 are byte-identical

  python3 sdk/motion/bench/bevy/studio/check.py [RAEA_MOTION_BIN]     exits 1 on any failure
"""
import os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "../../../../.."))
BIN = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("RAEA_MOTION_BIN", os.path.join(ROOT, "sdk/react/target/release/raea-motion"))
EDIT = os.path.join(HERE, "edit.tsx")
W, H = 960, 540
OUT = tempfile.mkdtemp(prefix="raea-studio-check-")
SRC = open(EDIT).read()


def variant(name):
    if name == "all":
        return EDIT
    s = SRC
    if name == "hard":
        s = s.replace("const SOFT = true; // fx:soft", "const SOFT = false; // fx:soft")
    else:
        s = "\n".join(l for l in s.splitlines() if f"{{/* fx:{name} */}}" not in l)
    p = os.path.join(HERE, f".{name}.tsx")
    open(p, "w").write(s)
    return p


def render(name, frame, tag=""):
    edit = variant(name)
    png = os.path.join(OUT, f"{name}-f{frame}{tag}.png")
    try:
        r = subprocess.run([BIN, "render", edit, "--frame", str(frame), "-o", png], env=dict(os.environ, RAEA_READBACK="rgba"), capture_output=True, text=True)
    finally:
        if edit != EDIT:
            os.remove(edit)
    if r.returncode != 0:
        sys.exit(f"render of frame {frame} ({name}) failed:\n{r.stderr[-2000:]}")
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", png, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], capture_output=True).stdout
    assert len(raw) == W * H * 3
    return raw


def px(img, x, y):
    i = (y * W + x) * 3
    return img[i], img[i + 1], img[i + 2]


def lum(c):
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def box(img, x0, y0, x1, y1):
    return [px(img, x, y) for y in range(y0, y1) for x in range(x0, x1)]


def mean(cs):
    return sum(lum(c) for c in cs) / max(len(cs), 1)


failures = []


def check(name, ok, detail):
    print(f"{'PASS' if ok else 'FAIL'}  {name}: {detail}")
    if not ok:
        failures.append(name)


all5, noenv5 = render("all", 5), render("env", 5)
d = sum(1 for i in range(0, len(all5), 3) if all5[i:i + 3] != noenv5[i:i + 3])
check("env:off", d == 0, f"frame 5 at intensity 0 vs no Environment: {d} pixels differ")

all30, noenv30, hard30, noao30 = render("all", 30), render("env", 30), render("hard", 30), render("ao", 30)


def grey(c):
    return max(c) - min(c) < 30  # not the banana


# the shadow region: grey pixels on and around the disc, right of and below the banana
REGION = (380, 240, 760, 420)


def blocks(img, n=4):
    """The shadow region's grey 4x4 blocks, averaged (PCSS's penumbra is dithered: averaging reads its ramp)."""
    x0, y0, x1, y1 = REGION
    out = []
    for by in range(y0, y1, n):
        for bx in range(x0, x1, n):
            cs = box(img, bx, by, bx + n, by + n)
            if all(grey(c) for c in cs):
                out.append(mean(cs))
    return out


def blocks_at(img, n=4):
    """The region's grey 4x4 blocks by position, averaged."""
    x0, y0, x1, y1 = REGION
    out = {}
    for by in range(y0, y1, n):
        for bx in range(x0, x1, n):
            cs = box(img, bx, by, bx + n, by + n)
            if all(grey(c) for c in cs):
                out[(bx, by)] = mean(cs)
    return out


def shadow_stats(img):
    ls = sorted(blocks(img))
    k = len(ls) // 50 + 1
    lo = sum(ls[:k]) / k
    hi = ls[int(len(ls) * 0.6)]
    a, b = lo + 0.2 * (hi - lo), lo + 0.8 * (hi - lo)
    return lo, hi, sum(1 for l in ls if a < l < b)


lo_env, _, _ = shadow_stats(all30)
lo_noenv, _, _ = shadow_stats(noenv30)
check("env:fill", lo_env > lo_noenv + 10, f"the shadow's darkest blocks: luma {lo_noenv:.0f} without the Environment, {lo_env:.0f} with it")

# the penumbra: the 4x4 blocks soft shadows change (by 12+ levels); across them the shadow's steepest step to a
# neighbouring block is gentler with soft shadows
S, Hd = blocks_at(all30), blocks_at(hard30)
changed = [k for k in S if k in Hd and abs(S[k] - Hd[k]) > 12]


def step(B, k):
    x, y = k
    return max((abs(B[n] - B[k]) for n in ((x + 4, y), (x - 4, y), (x, y + 4), (x, y - 4)) if n in B), default=0)


g_soft = sum(step(S, k) for k in changed) / max(len(changed), 1)
g_hard = sum(step(Hd, k) for k in changed) / max(len(changed), 1)
check("soft", len(changed) >= 40 and g_hard >= 1.4 * g_soft, f"{len(changed)} blocks of the shadow's edge change; their steepest step: {g_hard:.0f} levels hard, {g_soft:.0f} soft")


def shade(img):
    """The cast shadow's mean luma: grey pixels of the region darker than the lit disc."""
    cs = [c for c in box(img, *REGION) if grey(c) and lum(c) < 180]
    return mean(cs)


with_ao, without = shade(all30), shade(noao30)
far_ao, far_no = mean(box(all30, 40, 120, 200, 160)), mean(box(noao30, 40, 120, 200, 160))
check("ao", without - with_ao >= 3 and abs(far_ao - far_no) <= 1, f"the cast shadow's mean luma: {without:.1f} without AO, {with_ao:.1f} with; far cyc {far_no:.1f} / {far_ao:.1f}")

near = box(all30, 20, 500, 120, 530)
near_avg = [sum(c[k] for c in near) / len(near) for k in range(3)]
far = box(all30, 20, 140, 120, 160)
far_min = min(min(c) for c in far)
peel = sorted((c for c in box(all30, 300, 120, 700, 420) if max(c) - min(c) > 60), key=lum, reverse=True)
top = peel[: max(1, len(peel) // 10)]
sat = sum(max(c) - min(c) for c in top) / len(top) if top else 0
target = (0xf7, 0xf7, 0xf5)
ok = all(abs(near_avg[k] - target[k]) <= 4 for k in range(3)) and far_min >= 254 and sat >= 120
check("highkey", ok, f"near cyc #{''.join(f'{round(v):02x}' for v in near_avg)}, far cyc's darkest channel {far_min}, the brightest peel's saturation {sat:.0f}")

again = render("all", 30, "b")
check("determinism", all30 == again, "two renders of frame 30 byte-identical" if all30 == again else "they differ")

print(f"frames in {OUT}")
sys.exit(1 if failures else 0)
