"""Air for mixed.tsx, procedural (no footage): media/mixed/air/beam.png, a shaft of light as it shows in dusty air, and
haze_{warm,cool}_{0,1,2}.png, sheets of slow haze that tile left to right (each wraps onto itself, so a sheet sliding
across a scene never shows a seam). Straight alpha: the colour is the light's, the alpha how much air catches it.

  uv run --with numpy --with opencv-python-headless python -I tools/air.py
"""
from pathlib import Path

import cv2
import numpy as np

OUT = Path(__file__).resolve().parent.parent / "media/mixed/air"
OUT.mkdir(parents=True, exist_ok=True)
rng = np.random.default_rng(44)


def smoothstep(a, b, v):
    u = np.clip((v - a) / (b - a), 0, 1)
    return u * u * (3 - 2 * u)


def fbm(h, w, scales, gains, aspect=1.0):
    """Noise that wraps at its edges (white noise filtered in frequency), octaves summed, 0..1."""
    fy, fx = np.fft.fftfreq(h)[:, None], np.fft.fftfreq(w)[None, :]
    out = np.zeros((h, w))
    for s, g in zip(scales, gains):
        r = np.sqrt((fx * w / s) ** 2 + (fy * h / (s * aspect)) ** 2)
        layer = np.real(np.fft.ifft2(np.fft.fft2(rng.standard_normal((h, w))) * np.exp(-(r**2))))
        out += g * layer / layer.std()
    return (out - out.min()) / (out.max() - out.min())


def save(name, rgb, a):
    img = np.dstack([np.broadcast_to(np.array(rgb[::-1], np.float32) / 255, a.shape + (3,)), a]) * 255
    cv2.imwrite(str(OUT / name), img.round().astype(np.uint8))


# the beam: soft sides and a brighter core, streaks of dust running its length, strongest where it enters (the top)
# and pooling a little where it meets the floor
h, w = 1024, 256
y = np.linspace(0, 1, h)[:, None]
x = np.linspace(-1, 1, w)[None, :]
across = np.exp(-((x / 0.55) ** 4)) * (0.7 + 0.3 * np.exp(-((x / 0.18) ** 2)))
along = (1 - 0.65 * smoothstep(0.0, 1.0, y)) * smoothstep(0.0, 0.06, y) + 0.25 * np.exp(-(((y - 0.97) / 0.04) ** 2))
streaks = 0.55 + 0.45 * fbm(h, w, [24, 60], [1, 0.5], aspect=1 / 40)  # long along the beam, fine across it
motes = fbm(h, w, [60, 120], [1, 0.6])
air = across * along * streaks * (0.85 + 0.3 * motes)
save("beam.png", (255, 230, 186), np.clip(air * 0.3, 0, 1))

# the haze: billows in a band, thinning toward the top and the foot of the sheet
h, w = 512, 1024
y = np.linspace(0, 1, h)[:, None]
band = smoothstep(0.0, 0.35, y) * (1 - smoothstep(0.7, 1.0, y))
for i in range(3):
    n = fbm(h, w, [2, 5, 11, 24], [1, 0.55, 0.3, 0.15])
    a = smoothstep(0.35, 0.85, n) * band * 0.3
    # dim: night air, glowing faintly with the light it holds and hiding a little of what's behind it
    save(f"haze_warm_{i}.png", (92, 66, 46), a)
    save(f"haze_cool_{i}.png", (58, 66, 82), a)
print(f"air to {OUT}")
