"""Fire from footage: a real bonfire (Feu.ogv, Wikimedia Commons, CC BY 2.0) keyed off its night sky into 60-frame
loops of straight-alpha PNGs for mixed.tsx's <Footage> (a flipbook of ImagePlanes in the scene): media/mixed/fire/
pyre_NNN.png is the whole burning stack, flames_NNN.png only the flames over it, soft at the foot, to stand on a
pyre built in 3D.

The key treats the fire as light added to the night, in linear light: the light is what's above the sky's own colour (each row's,
from the crop's outer columns at their darkest across the clip, since the dusk is lighter toward the horizon), alpha is its brightest channel and the colour is the light divided by alpha. Drawn premultiplied
(rgb + (1 - a) x scene) that is the light added over the scene, as a screen blend would add it, with no sky fringe.
The last 12 frames dissolve into the first, so frame 59 runs on into frame 0.

  uv run --with numpy --with opencv-python-headless python -I tools/fire.py SOURCE.ogv
"""
import json, subprocess, sys, tempfile
from pathlib import Path

import cv2
import numpy as np

HERE = Path(__file__).resolve().parent.parent
OUT = HERE / "media/mixed/fire"
SRC = Path(sys.argv[1])
AT, N, X = 56.0, 60, 12  # start (s), loop frames, frames dissolved across the seam
# x, y, w, h in the 1280x720 source, how far up the foot fades (px), and where the base is (px from the top)
VARIANTS = {"pyre": ((470, 0, 320, 690), 26, 672), "flames": ((470, 0, 320, 500), 110, 480)}

def lin(c):
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def srgb(l):
    l = np.clip(l, 0, 1)
    return np.where(l <= 0.0031308, l * 12.92, 1.055 * l ** (1 / 2.4) - 0.055)


def smoothstep(a, b, v):
    u = np.clip((v - a) / (b - a), 0, 1)
    return u * u * (3 - 2 * u)

for name, ((x, y, w, h), fade, foot) in VARIANTS.items():
    tmp = Path(tempfile.mkdtemp())
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", str(AT), "-i", SRC, "-vf", f"fps=30,crop={w}:{h}:{x}:{y}", "-frames:v", str(N + X), tmp / "f%03d.png"], check=True)
    frames = [cv2.imread(str(tmp / f"f{i + 1:03d}.png")).astype(np.float32) / 255 for i in range(N + X)]
    # soft edges, so the crop never shows: the sides and the foot fade out
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    edge = smoothstep(0, 100, xx) * smoothstep(0, 100, w - 1 - xx) * smoothstep(0, fade, h - 1 - yy) * smoothstep(0, 60, yy)
    # and a taper about the fire's column (wider up where the flames lean), so the lit crowd and buildings either side stay out
    cx, spread = 152.0, 100.0 + 30.0 * (1 - yy / h)
    edge *= np.exp(-(((xx - cx) / spread) ** 2))

    # the sky behind each row: the outer 30 px either side, at their darkest over the clip, a little over
    lo = np.min(frames, axis=0)
    sky = np.median(np.concatenate([lo[:, :30], lo[:, -30:]], axis=1), axis=1)[:, None, :]
    sky = cv2.GaussianBlur(sky, (1, 31), 0) * 1.25

    OUT.mkdir(parents=True, exist_ok=True)
    glow = []
    for i in range(N):
        f = frames[i]
        if i < X:  # the seam: the frames after the loop's end dissolve into its start
            u = (i + 1) / (X + 1)
            f = (1 - u) * frames[N + i] + u * f
        # only warm or bright light is fire's: the dusk near the horizon is lighter than the sky's estimate, but it's
        # blue-green and dim, while the fire is orange, and white where it's hottest
        warm = np.maximum(smoothstep(0.015, 0.1, f[..., 2] - f[..., 0]), smoothstep(0.45, 0.7, f.min(axis=2)))
        # in linear light, since the engine adds it there: it linearises the colour, then multiplies by alpha
        light = np.maximum(lin(f) - lin(sky), 0) * (edge * warm)[..., None]
        light[..., 0] = np.minimum(light[..., 0], light[..., 2])  # no blue above the red: what's left of the sky
        # the flames at full strength, the smoke they light dimmed: under agx the faint glow turned a pale cream
        light *= (np.clip(light.max(axis=2) / 0.25, 0, 1) ** 0.9)[..., None]
        a = np.clip(light.max(axis=2), 0, 1)
        rgb = srgb(np.where(a[..., None] > 1e-4, light / np.maximum(a[..., None], 1e-4), 0))
        out = np.dstack([np.clip(rgb, 0, 1), a]) * 255
        cv2.imwrite(str(OUT / f"{name}_{i:03d}.png"), out.round().astype(np.uint8))
        glow.append(float(light.mean()))
    # each frame's light, 1 on average: a light by the fire flickers with the footage
    m = sum(glow) / len(glow)
    meta = dict(frames=N, w=w, h=h, foot=round(foot / h, 3), glow=[round(g / m, 3) for g in glow])
    (OUT / f"{name}.json").write_text(json.dumps(meta) + "\n")
    print(f"{name}: {N} frames, {w}x{h}, to {OUT}; glow {min(meta['glow'])}..{max(meta['glow'])}")
