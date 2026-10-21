"""Drawings in sand: a picture (media/mixed/sand/src/<id>.jpg, public domain, media/mixed/SOURCES.txt) traced into
sand by an unseen finger, stroke by stroke, then a wind that fills the lines in and carries the sand off. Writes
media/mixed/sand/<id>.mp4 (1920x1080, 30 fps) and <id>.json ({seconds, drawn, wind}: when the last line is done and
when the wind starts), for mixed.tsx's <SandDrawing>.

The lines: the picture's contours (smoothed with edges kept, then Canny's edges, which are one pixel and joined by
hysteresis), walked into strokes, the longest kept; ordered so the finger goes on from near where it lifted, big lines first. The sand: a height field
(dunes, wind ripples, grain) the strokes carve, each groove with a ridge of pushed-up sand either side, lit by a low
warm light from the upper left so the grooves read as shadow and the ridges catch the light. Dust puffs off the tip
as it moves; the wind is a front crossing the frame that fills the grooves and blows streaks of sand along.

  uv run --with numpy --with opencv-contrib-python-headless python -I tools/sand.py [ID ...] [--lines] [--at S,S,...]
  (--lines: only the line art, to media/mixed/sand/<id>.lines.png; --at: only stills at those seconds, to
  media/mixed/sand/<id>.<s>.png; both to tune a picture's settings)
"""
import json, subprocess, sys
from pathlib import Path

import cv2
import numpy as np

HERE = Path(__file__).resolve().parent.parent
SRC, OUT = HERE / "media/mixed/sand/src", HERE / "media/mixed/sand"
W, H, FPS = 1920, 1080, 30
BOX = (1440, 690)  # the drawing fits in this, centred (inside the film's letterbox, 1920x816)

# per picture: the crop (fractions x0, y0, x1, y1), the working width the edges are found at (smaller = bolder,
# fewer lines), the blur before the edges (sigma, working px) and which edges count (the strongest `edge`% of the
# gradient starts a line, 40% of that continues one), the shortest stroke kept (working px), the line budget (output
# px of groove), and how long the drawing takes (s)
PICS = {
    "rubicon": dict(crop=(0.035, 0.03, 0.965, 0.9), work=520, sigma=1.1, edge=10, min_len=14, budget=52000, draw=9.0),
    "pompey": dict(crop=(0.285, 0.43, 0.75, 0.975), work=560, sigma=0.9, edge=10, min_len=10, budget=42000, draw=4.0),
    "camuccini": dict(crop=(0.0, 0.0, 1.0, 1.0), work=560, sigma=1.3, edge=14, min_len=14, budget=52000, draw=9.0),
    "gerome": dict(crop=(0.0, 0.08, 1.0, 1.0), work=560, sigma=1.3, edge=15, min_len=14, budget=46000, draw=4.3),
    "antony": dict(crop=(0.0, 0.0, 1.0, 1.0), work=540, sigma=1.3, edge=13, min_len=14, budget=50000, draw=3.7),
    "coin": dict(crop=(0.04, 0.04, 0.96, 0.96), work=540, sigma=1.2, edge=10, min_len=10, budget=30000, draw=7.0),
}
INTRO, HOLD, WIND, TAIL = 0.7, 1.8, 2.6, 0.5
rng = np.random.default_rng(7)


def smoothstep(a, b, v):
    u = np.clip((v - a) / (b - a), 0, 1)
    return u * u * (3 - 2 * u)


# ------------------------------------------------------------------ the lines


def edges(gray, sigma, pct):
    """Canny's edges of the picture smoothed with its edges kept (bilateral twice, then a little blur): one pixel wide,
    the strongest `pct` per cent of the gradient starting a line."""
    g = (gray * 255).astype(np.uint8)
    for _ in range(2):
        g = cv2.bilateralFilter(g, 9, 40, 7)
    g = cv2.GaussianBlur(g, (0, 0), sigma)
    mag = np.hypot(cv2.Sobel(g, cv2.CV_32F, 1, 0), cv2.Sobel(g, cv2.CV_32F, 0, 1))
    hi = float(np.percentile(mag, 100 - pct))
    return cv2.Canny(g, 0.4 * hi, hi, L2gradient=True) > 0


def trace(skel):
    """A one-pixel skeleton walked into polylines: from each end, then around what's left (loops)."""
    sk = skel.copy().astype(np.uint8)
    hh, ww = sk.shape
    nb = [(-1, -1), (-1, 0), (-1, 1), (0, -1), (0, 1), (1, -1), (1, 0), (1, 1)]

    def neighbours(y, x):
        return [(y + dy, x + dx) for dy, dx in nb if 0 <= y + dy < hh and 0 <= x + dx < ww and sk[y + dy, x + dx]]

    strokes = []

    def walk(y, x):
        pts = [(x, y)]
        sk[y, x] = 0
        while True:
            n = neighbours(y, x)
            if not n:
                break
            # straight on if possible: prefer 4-neighbours, then diagonals
            n.sort(key=lambda q: abs(q[0] - y) + abs(q[1] - x))
            y, x = n[0]
            sk[y, x] = 0
            pts.append((x, y))
        return pts

    count = cv2.filter2D(sk, -1, np.ones((3, 3), np.float32), borderType=cv2.BORDER_CONSTANT) - sk
    ends = list(zip(*np.nonzero((sk == 1) & (count == 1))))
    for y, x in ends:
        if sk[y, x]:
            strokes.append(walk(y, x))
    for y, x in zip(*np.nonzero(sk)):
        if sk[y, x]:
            strokes.append(walk(y, x))
    return strokes


def chaikin(p, n=2):
    for _ in range(n):
        if len(p) < 3:
            return p
        q = [p[0]]
        for a, b in zip(p[:-1], p[1:]):
            q += [0.75 * a + 0.25 * b, 0.25 * a + 0.75 * b]
        q.append(p[-1])
        p = np.array(q)
    return p


def lines(pid):
    c = PICS[pid]
    img = cv2.imread(str(SRC / f"{pid}.jpg"), cv2.IMREAD_GRAYSCALE)
    h0, w0 = img.shape
    x0, y0, x1, y1 = c["crop"]
    img = img[int(y0 * h0) : int(y1 * h0), int(x0 * w0) : int(x1 * w0)]
    scale = c["work"] / img.shape[1]
    img = cv2.resize(img, (c["work"], round(img.shape[0] * scale)), interpolation=cv2.INTER_AREA).astype(np.float32) / 255
    # even out the light, so dark passages still give lines and bright ones don't drown
    img = cv2.createCLAHE(clipLimit=2.0, tileGridSize=(6, 6)).apply((img * 255).astype(np.uint8)).astype(np.float32) / 255
    skel = cv2.ximgproc.thinning(edges(img, c["sigma"], c["edge"]).astype(np.uint8) * 255) > 0
    skel[:5], skel[-5:], skel[:, :5], skel[:, -5:] = 0, 0, 0, 0  # not the picture's own border
    raw = [np.array(s, np.float32) for s in trace(skel)]
    raw = [s for s in raw if len(s) >= c["min_len"]]
    # into the output frame: fit the box, centred
    hh, ww = img.shape
    f = min(BOX[0] / ww, BOX[1] / hh)
    off = np.array([(W - ww * f) / 2, (H - hh * f) / 2], np.float32)
    strokes = []
    for s in raw:
        s = cv2.approxPolyDP(s.reshape(-1, 1, 2), 0.9, False).reshape(-1, 2)
        s = chaikin(s.astype(np.float32)) * f + off
        strokes.append(s)
    # the budget: the longest strokes, up to so much groove
    length = lambda s: float(np.linalg.norm(np.diff(s, axis=0), axis=1).sum())
    strokes.sort(key=length, reverse=True)
    kept, total = [], 0.0
    for s in strokes:
        if total > c["budget"]:
            break
        kept.append(s)
        total += length(s)
    return order(kept, length), (ww, hh, f, off)


def order(strokes, length):
    """The finger's order: from the top left, each next stroke the one that starts nearest where it lifted, longer
    ones a little sooner (so the picture's big lines come first), turned round if its far end is nearer."""
    left = list(strokes)
    tip = np.array([0.0, 0.0])
    out = []
    while left:
        best, bi, flip = None, 0, False
        for i, s in enumerate(left):
            for fl, end in ((False, s[0]), (True, s[-1])):
                score = np.linalg.norm(end - tip) - 0.25 * min(length(s), 600)
                if best is None or score < best:
                    best, bi, flip = score, i, fl
        s = left.pop(bi)
        s = s[::-1] if flip else s
        out.append(s)
        tip = s[-1]
    return out


# ------------------------------------------------------------------ the sand


def fbm(h, w, scales, gains, seed):
    r = np.random.default_rng(seed)
    out = np.zeros((h, w), np.float32)
    for s, g in zip(scales, gains):
        n = cv2.resize(r.standard_normal((max(2, h // s), max(2, w // s))).astype(np.float32), (w, h), interpolation=cv2.INTER_CUBIC)
        out += g * n
    return out


def fly(dust, blow, dt):
    """Move the dust (x, y, vx, vy, height, rise, age, life, size: it flies out, rises and settles, slowing) and the
    blown sand (x, y, vx, vy, age, life) on by dt; the ones past their life go."""
    for p in dust:
        p[6] += dt
        p[0] += p[2] * dt
        p[1] += p[3] * dt
        p[2] *= 0.94
        p[3] *= 0.94
        p[4] = max(p[4] + p[5] * dt, 0)
        p[5] -= 30 * dt
    for p in blow:
        p[4] += dt
        p[0] += p[2] * dt
        p[1] += p[3] * dt
    return [p for p in dust if p[6] <= p[7]], [p for p in blow if p[4] <= p[5]]


def render(pid, at=None):
    c = PICS[pid]
    strokes, _ = lines(pid)
    seg_len = [np.linalg.norm(np.diff(s, axis=0), axis=1) for s in strokes]
    # the path: each stroke drawn at the finger's pace, the hops between them in the air at three times it
    hop = [0.0] + [float(np.linalg.norm(strokes[i][0] - strokes[i - 1][-1])) / 3 for i in range(1, len(strokes))]
    total = sum(float(l.sum()) for l in seg_len) + sum(hop)
    speed = total / c["draw"]
    drawn_at = INTRO + c["draw"]
    wind_at = drawn_at + HOLD
    seconds = wind_at + WIND + TAIL
    n = int(round(seconds * FPS))

    yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)
    # the sand at rest: long soft dunes, wind ripples across them, and grain
    dunes = fbm(H, W, [360, 160], [6.0, 2.0], 1)
    ripple_phase = fbm(H, W, [300], [2.2], 2)
    grain = cv2.GaussianBlur(rng.standard_normal((H, W)).astype(np.float32), (0, 0), 0.55)
    albedo_n = fbm(H, W, [240, 60], [0.05, 0.03], 3)
    sparkle = (rng.random((H, W)) > 0.9985).astype(np.float32)
    front_noise = fbm(H, W, [180, 50], [0.18, 0.06], 4)
    # wind ripples in patches, not all over
    ripple_amp = 0.5 * np.clip(0.5 + fbm(H, W, [420, 200], [0.6, 0.3], 5), 0.1, 1.2)

    # the light: low from the upper left, warm; a soft pool of it on the drawing, dark at the edges
    L = np.array([-0.75, -0.5, 0.36], np.float32)  # about 20 degrees up: raking, so the relief reads
    L /= np.linalg.norm(L)
    pool = np.exp(-2 * (((xx - W * 0.47) / (W * 0.5)) ** 2 + ((yy - H * 0.48) / (H * 0.55)) ** 2))
    pool = 0.015 + 1.15 * pool
    sand = np.array([0.76, 0.63, 0.49], np.float32) ** 2.2  # linear albedo, a pale grey-tan

    SIG = 2.4  # the groove's half-width (px)
    canvas = np.zeros((H, W), np.float32)
    dust = []  # [x, y, vx, vy, z, vz, age, life, size]
    blow = []
    k0 = 1 / (np.sqrt(2 * np.pi) * SIG)
    k1 = 1 / (np.sqrt(2 * np.pi) * SIG * 2.6)

    stills = None if at is None else {int(round(a * FPS)): a for a in at}
    proc = None if stills is not None else subprocess.Popen(["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
                             "-c:v", "libx264", "-preset", "slow", "-crf", "20", "-pix_fmt", "yuv420p", "-movflags", "+faststart", str(OUT / f"{pid}.mp4")], stdin=subprocess.PIPE)
    done_s = 0.0  # path length drawn so far
    si, pi, pfrac = 0, 0, 0.0  # where the finger is: stroke, segment, fraction along it
    in_hop = 0.0
    for fi in range(n):
        t = fi / FPS
        # ---- the finger: draw this frame's length of path
        moved = []
        if INTRO <= t <= drawn_at + 1 / FPS and si < len(strokes):
            budget = speed / FPS
            while budget > 0 and si < len(strokes):
                if in_hop > 0:
                    step = min(budget, in_hop)
                    in_hop -= step
                    budget -= step
                    continue
                s, ls = strokes[si], seg_len[si]
                if pi >= len(ls):
                    si, pi, pfrac = si + 1, 0, 0.0
                    if si < len(strokes):
                        in_hop = hop[si]
                    continue
                seg = ls[pi] * (1 - pfrac)
                a = s[pi] + (s[pi + 1] - s[pi]) * pfrac
                if budget >= seg:
                    b = s[pi + 1]
                    budget -= seg
                    pi, pfrac = pi + 1, 0.0
                else:
                    pfrac += budget / max(ls[pi], 1e-6)
                    b = s[pi] + (s[pi + 1] - s[pi]) * pfrac
                    budget = 0
                # pressure: each stroke its own, easing in at its start
                press = 0.75 + 0.25 * np.sin(si * 2.1) ** 2
                cv2.line(canvas, (int(a[0] * 16), int(a[1] * 16)), (int(b[0] * 16), int(b[1] * 16)), float(press), 1, cv2.LINE_AA, 4)
                moved.append((a, b))
        # ---- dust off the tip, as much as it moved
        for a, b in moved:
            d = b - a
            dl = float(np.linalg.norm(d)) + 1e-6
            for _ in range(int(dl * 0.1 + rng.random())):
                perp = np.array([-d[1], d[0]]) / dl * rng.choice([-1, 1])
                v = perp * rng.uniform(25, 90) + d / dl * rng.uniform(5, 25) + np.array([14.0, -4.0])
                dust.append([b[0] + rng.normal(0, 2), b[1] + rng.normal(0, 2), v[0], v[1], 0.0, rng.uniform(18, 40), 0.0, rng.uniform(0.4, 1.1), rng.uniform(0.7, 1.6)])

        # ---- the wind: a front from the left that fills the grooves in behind it
        wu = (t - wind_at) / WIND
        if wu > -0.1:
            front = wu * 1.35 - xx / W + front_noise - 0.1
            fill = smoothstep(0.0, 0.35, front)
            fx = (wu * 1.35 - 0.1) * W  # where the front is, in px
            for _ in range(int(240 * smoothstep(-0.05, 0.1, wu) * (1 - smoothstep(0.85, 1.05, wu)))):
                y = rng.uniform(140, H - 140)
                blow.append([fx + rng.normal(0, 90), y, rng.uniform(420, 900), rng.normal(0, 25), 0.0, rng.uniform(0.25, 0.6)])
        else:
            fill = 0.0
        ripple_shift = max(0.0, t - wind_at) * 9.0

        dust, blow = fly(dust, blow, 1 / FPS)
        if stills is not None and fi not in stills:
            continue

        # ---- the height: sand at rest, carved
        G = np.minimum(cv2.GaussianBlur(canvas, (0, 0), SIG) / k0, 1.2)
        R = np.maximum(cv2.GaussianBlur(canvas, (0, 0), SIG * 2.6) / k1 - 0.75 * G, 0)
        carve = (1 - fill) if isinstance(fill, np.ndarray) else 1.0
        ripples = ripple_amp * np.sin((xx * 0.86 + yy * 0.5 + ripple_shift) / 11.0 + ripple_phase)
        Hf = dunes + ripples + 0.22 * grain + carve * (-4.2 * G + 1.6 * R)
        Hf = Hf.astype(np.float32)
        gx = cv2.Sobel(Hf, cv2.CV_32F, 1, 0, ksize=3) / 8
        gy = cv2.Sobel(Hf, cv2.CV_32F, 0, 1, ksize=3) / 8
        nz = 1 / np.sqrt(gx * gx + gy * gy + 1)
        lam = np.clip((-gx * L[0] - gy * L[1] + L[2]) * nz, 0, 1)
        ao = 1 - 0.45 * carve * np.minimum(G, 1)
        shade = (0.08 + 1.7 * lam) * ao * pool
        col = sand[None, None, :] * (1 + albedo_n[..., None] + 0.06 * grain[..., None]) * shade[..., None]
        col += sparkle[..., None] * lam[..., None] * 0.35 * pool[..., None]
        col *= np.array([1.0, 0.84, 0.64], np.float32)  # the light's warmth: low sun, or a torch

        # ---- dust: lit specks risen off the surface, each with its shadow on the sand below; blown streaks
        lit = np.zeros((H, W), np.float32)
        shadow = np.zeros((H, W), np.float32)
        for p in dust:
            fade = (1 - p[6] / p[7]) ** 1.5
            r = p[8] * (1 + p[6] * 1.5)
            cv2.circle(lit, (int(p[0] * 4), int((p[1] - p[4]) * 4)), max(1, int(r * 4)), 0.75 * fade, -1, cv2.LINE_AA, 2)
            cv2.circle(shadow, (int((p[0] + p[4] * 0.9) * 4), int((p[1] + p[4] * 0.6) * 4)), max(1, int(r * 5)), 0.5 * fade, -1, cv2.LINE_AA, 2)
        for p in blow:
            fade = np.sin(np.pi * p[4] / p[5])
            cv2.line(lit, (int((p[0] - p[2] / FPS) * 4), int((p[1] - p[3] / FPS) * 4)), (int(p[0] * 4), int(p[1] * 4)), 0.35 * fade, 1, cv2.LINE_AA, 2)
        if dust or blow:
            lit = cv2.GaussianBlur(lit, (0, 0), 1.3)
            shadow = cv2.GaussianBlur(shadow, (0, 0), 2.0)
            col *= (1 - np.clip(shadow, 0, 0.6))[..., None]
            dcol = np.minimum(sand * 2.2 * pool[..., None], 1)
            a = np.clip(lit, 0, 1)[..., None]
            col = col * (1 - a) + dcol * a

        # ---- the camera: a slow push in on the drawing
        img = np.clip(col, 0, 1) ** (1 / 2.2)
        z = 1.0 + 0.05 * (t / seconds)
        M = np.array([[z, 0, W / 2 * (1 - z)], [0, z, H / 2 * (1 - z)]], np.float32)
        img = cv2.warpAffine(img, M, (W, H), flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_REFLECT)
        frame = (img * 255 + rng.random((H, W, 1)) * 0.9).astype(np.uint8)
        if proc is None:
            cv2.imwrite(str(OUT / f"{pid}.{stills[fi]:g}.png"), frame[..., ::-1])
            continue
        proc.stdin.write(frame.tobytes())
        if fi % 60 == 0:
            print(f"  {pid} {fi}/{n}", flush=True)
    if proc is None:
        return
    proc.stdin.close()
    proc.wait()
    meta = dict(seconds=round(seconds, 3), drawn=round(drawn_at, 3), wind=round(wind_at, 3), strokes=len(strokes))
    (OUT / f"{pid}.json").write_text(json.dumps(meta) + "\n")
    print(f"{pid}: {len(strokes)} strokes, {seconds:.1f} s")


if __name__ == "__main__":
    args = sys.argv[1:]
    ids = [a for i, a in enumerate(args) if not a.startswith("--") and not (i and args[i - 1] == "--at")] or list(PICS)
    for pid in ids:
        if "--lines" in sys.argv:
            strokes, _ = lines(pid)
            img = np.full((H, W), 255, np.uint8)
            for s in strokes:
                cv2.polylines(img, [s.astype(np.int32)], False, 0, 2, cv2.LINE_AA)
            cv2.imwrite(str(OUT / f"{pid}.lines.png"), img)
            print(pid, len(strokes), "strokes")
        else:
            at = next((a.split("=", 1)[1] if "=" in a else sys.argv[sys.argv.index(a) + 1] for a in sys.argv if a.startswith("--at")), None)
            render(pid, [float(x) for x in at.split(",")] if at else None)
