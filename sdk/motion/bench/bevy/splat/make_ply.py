"""The splat fixture's test cloud (media/test.ply): a procedural 3DGS file, not a trained one, so the fixture is small
and every property of the format is exercised: anisotropic, rotated gaussians, a range of opacities, and degree-3
spherical harmonics whose first-order terms tint each side of the shape by the direction it's seen from (a camera
going round sees the colour shift). A striped torus standing on a disc of thin, flat splats, in raea's frame (+Y up,
metres), so it needs no sidecar.

    python3 sdk/motion/bench/bevy/splat/make_ply.py
"""
import math
import os
import random
import struct

rnd = random.Random(7)
C0 = 0.28209479177387814


def logit(p):
    return math.log(p / (1 - p))


def quat_towards(n):
    """A rotation taking +Z to n (w x y z), so a flat splat's thin axis (z) lies along a normal."""
    z = (0.0, 0.0, 1.0)
    c = (z[1] * n[2] - z[2] * n[1], z[2] * n[0] - z[0] * n[2], z[0] * n[1] - z[1] * n[0])
    d = z[0] * n[0] + z[1] * n[1] + z[2] * n[2]
    w = 1 + d
    if w < 1e-6:
        return (0.0, 1.0, 0.0, 0.0)
    q = (w, c[0], c[1], c[2])
    l = math.sqrt(sum(v * v for v in q))
    return tuple(v / l for v in q)


splats = []
# a torus standing up (its hole along z), centre 0.9 m up: stripes round its tube
R, r = 0.6, 0.22
for i in range(7000):
    u = rnd.random() * 2 * math.pi
    v = rnd.random() * 2 * math.pi
    nx, ny, nz = math.cos(u) * math.cos(v), math.sin(u) * math.cos(v), math.sin(v)
    x = (R + r * math.cos(v)) * math.cos(u)
    y = (R + r * math.cos(v)) * math.sin(u) + 0.9
    z = r * math.sin(v)
    stripe = int(u / (2 * math.pi) * 12) % 2
    col = (0.95, 0.35, 0.12) if stripe else (0.95, 0.9, 0.82)
    splats.append(((x, y, z), (0.035, 0.035, 0.006), quat_towards((nx, ny, nz)), 0.92, col, (nx, ny, nz)))
# a disc of floor, flat splats facing up, darker to the rim
for i in range(3000):
    a = rnd.random() * 2 * math.pi
    d = math.sqrt(rnd.random()) * 1.6
    x, z = math.cos(a) * d, math.sin(a) * d
    g = 0.55 - 0.25 * d / 1.6
    col = (g * 0.8, g * 0.9, g)
    splats.append(((x, 0.0, z), (0.05, 0.05, 0.004), quat_towards((0.0, 1.0, 0.0)), 0.5 + 0.45 * rnd.random(), col, (0.0, 1.0, 0.0)))

props = ["x", "y", "z", "nx", "ny", "nz", "f_dc_0", "f_dc_1", "f_dc_2"] + [f"f_rest_{k}" for k in range(45)] + ["opacity", "scale_0", "scale_1", "scale_2", "rot_0", "rot_1", "rot_2", "rot_3"]
head = "ply\nformat binary_little_endian 1.0\nelement vertex %d\n" % len(splats) + "".join(f"property float {p}\n" for p in props) + "end_header\n"
out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "media", "test.ply")
os.makedirs(os.path.dirname(out), exist_ok=True)
with open(out, "wb") as f:
    f.write(head.encode())
    for (p, s, q, op, col, n) in splats:
        dc = [(c - 0.5) / C0 for c in col]
        # degree 1 (channel-major in the file: 15 of red's, 15 of green's, 15 of blue's): seen from +x the red side
        # warms, from -x it cools; Y1 terms are -C1*y, C1*z, -C1*x, so the coefficient on -x shifts with view x
        rest = [0.0] * 45
        rest[0 * 15 + 2] = -0.25  # red rises looking from +x (dir.x < 0 towards the splat... sign tested by eye)
        rest[2 * 15 + 2] = 0.25
        # a little of degree 3, so a file with all 45 is read through to the end
        rest[1 * 15 + 14] = 0.05
        f.write(struct.pack("<%df" % len(props), *p, *n, *dc, *rest, logit(op), *[math.log(v) for v in s], *q))
print(out, len(splats))
