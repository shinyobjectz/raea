"""media/sky/stars.hdr: the film's night sky, made here rather than photographed (every scene is at night). An
equirectangular Radiance HDR: black overhead, a deep blue glow low down, a faint milky band across it and about
9000 stars, their brightness a power law and their colour from blue-white to amber. Seeded, so it's the same file
every time.

  uv run --with numpy python -I tools/stars.py
"""
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent.parent
W, H = 4096, 2048
rng = np.random.default_rng(44)

v = (np.arange(H) + 0.5) / H  # 0 at the zenith, 1 at the nadir
u = (np.arange(W) + 0.5) / W
lat = (0.5 - v) * np.pi  # +pi/2 overhead
lon = u * 2 * np.pi
LON, LAT = np.meshgrid(lon, lat)
up = np.sin(LAT)

# the sky's own glow: black overhead, a deep blue haze at the horizon, the ground below it a dark grey-blue
img = np.zeros((H, W, 3), np.float32)
haze = np.exp(-np.clip(up, 0, None) / 0.12)
img += haze[..., None] * np.array([0.05, 0.075, 0.15], np.float32)
img[up < 0] = np.array([0.02, 0.025, 0.04], np.float32)

# the milky band: a great circle tilted across the sky, soft noise along it
x, y, z = np.cos(LAT) * np.cos(LON), up, np.cos(LAT) * np.sin(LON)
n = np.array([0.35, 0.55, 0.76]); n /= np.linalg.norm(n)
d = x * n[0] + y * n[1] + z * n[2]
def grow(a, h, w):
    """`a` resized to h x w, bilinear (so the band's noise has no blocks)"""
    ys, xs = np.linspace(0, a.shape[0] - 1, h), np.linspace(0, a.shape[1] - 1, w)
    y0, x0 = np.floor(ys).astype(int), np.floor(xs).astype(int)
    y1, x1 = np.minimum(y0 + 1, a.shape[0] - 1), np.minimum(x0 + 1, a.shape[1] - 1)
    fy, fx = (ys - y0)[:, None], (xs - x0)[None, :]
    top = a[y0][:, x0] * (1 - fx) + a[y0][:, x1] * fx
    bot = a[y1][:, x0] * (1 - fx) + a[y1][:, x1] * fx
    return (top * (1 - fy) + bot * fy).astype(np.float32)


noise = np.zeros((H, W), np.float32)
for octave in range(5):
    s = 2 ** octave
    noise += grow(rng.random((4 * s, 8 * s)).astype(np.float32), H, W) / s
noise = (noise - noise.min()) / (noise.max() - noise.min())
band = np.exp(-(d / 0.16) ** 2) * noise ** 2 * (up > -0.02)
img += band[..., None] * np.array([0.09, 0.085, 0.1], np.float32)

# stars: uniform over the sphere above the horizon, brightness a power law, a 2x2 footprint for the bright ones
N = 9000
zs = rng.uniform(0.02, 1, N)  # sin(latitude)
ph = rng.uniform(0, 2 * np.pi, N)
row = ((0.5 - np.arcsin(zs) / np.pi) * H).astype(int)
col = (ph / (2 * np.pi) * W).astype(int) % W
b = 0.05 * rng.pareto(1.6, N) + 0.02
b = np.minimum(b, 6.0) * (0.35 + 0.65 * np.clip(zs / 0.25, 0, 1))  # dimmer through the haze low down
t = rng.random(N)
tint = np.stack([0.75 + 0.35 * t, 0.85 + 0.1 * t, 1.15 - 0.4 * t], 1).astype(np.float32)
for r, c, bb, tt in zip(row, col, b, tint):
    img[r, c] += bb * tt
    if bb > 0.4:
        for dr, dc in ((0, 1), (1, 0), (0, -1), (-1, 0)):
            img[(r + dr) % H, (c + dc) % W] += 0.25 * bb * tt


def rgbe(a):
    m = a.max(-1)
    e = np.floor(np.log2(np.maximum(m, 1e-32))) + 1
    scale = np.where(m > 1e-32, 256.0 / np.exp2(e), 0)
    out = np.zeros(a.shape[:2] + (4,), np.uint8)
    out[..., :3] = np.clip(a * scale[..., None], 0, 255).astype(np.uint8)
    out[..., 3] = np.where(m > 1e-32, e + 128, 0).astype(np.uint8)
    return out


out = HERE / "media/sky/stars.hdr"
with open(out, "wb") as f:
    f.write(b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n" + f"-Y {H} +X {W}\n".encode())
    f.write(rgbe(img).tobytes())
print(out)
