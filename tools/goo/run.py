"""Goo: objects melted into viscous goo, baked into a mesh sequence raea plays back by frame (<Goo>, physics.goo).

    run.py SPEC.json OUT.rseq [--glb DIR --every N] [--cpu]

An MLS-MPM solver (Hu et al. 2018) in Taichi on Metal. Each object's mesh (glTF nodes, by name) is sampled into
particles carrying its colour (the base colour texture at the nearest surface point's UV). Before its `start` an
object is a frozen obstacle (other goo stops against it; it isn't drawn). From `start` it's live: a fixed-corotated
elastic solid that, particle by particle (the top first, seeded noise), goes plastic (its yield dropping to nothing)
and then a viscous fluid (shear modulus to 0, a viscous stress eta*(C + C^T) rising), so it slumps, pools, runs off
an edge and drips. Static box colliders (the table) with Coulomb friction, and cups: hollow cylinders moved by a
pose track (a mug lifted and tipped), whose walls carry the goo with them, and which an object can fill (coffee).

Deterministic: the particle-to-grid scatter adds fixed-point integers (atomic float sums would depend on the GPU's
order), everything else is per particle or per node, and the surface is numpy/scipy/scikit-image on the CPU, so the
same SPEC writes the same bytes.

Each output frame k (time k / fps) is a surface: the live particles splatted on a grid (cloud-in-cell), blurred,
marching cubes, Taubin smoothing, normals, colours from a colour field; written as one zstd blob of the .rseq format
(see write_rseq). OUT.rseq.json holds the stats and each object's hideFrame (when its original model should go).

SPEC: {solver ("mpm"; "sph": a thin liquid, handed to sph.py), fps, frames, seed, dx (grid cell, m; particles are dx/2
apart), gravity, bounds {min, max} (particles leaving are dropped), colliders [{box {min, max}, friction}], wet (0.3:
goo colours darker and more saturated, as a wet surface is), surface {blur (1: grid nodes), level (0.33 of the rest
density), flat (1: the blur across, x and z, over the blur up)}, workers (5: processes surfacing frames), objects
[...]}. An object: {name, src (a .glb), nodes (names), transform (column-major 4x4 applied to the nodes' world-space
vertices), start (s; null: an obstacle throughout, never drawn), melt (s from solid to fluid), stiffness (Young's
modulus, Pa: 3e4 jelly), viscosity (Pa s, 12: runs off an edge and drips within ~3 s of being fluid; 40: thick pools
that barely reach the edge), density, fill ("solid" | "shell"), thickness (a shell's, m), color ("#rrggbb" instead of
the material's), obstacle_until (s: an obstacle before its start goes away then), residual (0.12: the share of shear
stiffness a fluid keeps) and yield (0.005: the strain it must pass to flow), which make it a Bingham fluid that holds
ridges and a rounded lip, and runny (1.5 s: once fluid, residual and yield halve this often and the viscosity relaxes
towards half, so what held its shape gives way and drips), cohesion (0.6: the share of its pressure it keeps under
tension, so goo holds together in strands; a liquid's is near 0, or it pours as a rope)}. An object may fill a cup
instead of a glTF's nodes: {cup (its index), level (m above the cup's foot, inside)} with no src: a liquid, from start
0 with melt near 0, residual 0 and yield 0, so it has no shear stiffness and is all pressure and viscosity.

A cup: {radius (inside, m), height (foot to rim), base (the inside floor's height above the foot), wall (thickness;
at least 2.5 cells, outward, so nothing tunnels), friction, poses [[t, 16 numbers: the cup's frame to world,
column-major], ...] (t ascending, seconds from the goo's start; between them positions lerp and turns slerp; before
the first and after the last it holds)}. In the cup's frame its axis is +y, its foot at y 0. A grid node inside the
wall or floor takes the wall's velocity there (from the pose now and a substep on) for its normal part, as the static
boxes do, so the goo is pushed, carried and poured.
"""

import json
import math
import os
import struct
import sys
import time
from pathlib import Path

import numpy as np

T0 = time.time()


def say(*a):
    print(f"goo [{time.time() - T0:6.1f}s]", *a, file=sys.stderr, flush=True)


# ---------------------------------------------------------------------------------------------------------- glTF
COMP = {5120: np.int8, 5121: np.uint8, 5122: np.int16, 5123: np.uint16, 5125: np.uint32, 5126: np.float32}
NCOMP = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}


class Glb:
    def __init__(self, path):
        b = Path(path).read_bytes()
        if b[:4] != b"glTF":
            raise SystemExit(f"goo: {path} isn't a .glb")
        n = struct.unpack_from("<I", b, 12)[0]
        self.j = json.loads(b[20 : 20 + n])
        off = 20 + n
        self.bin = b""
        if off < len(b):
            ln = struct.unpack_from("<I", b, off)[0]
            self.bin = b[off + 8 : off + 8 + ln]
        self.images = {}

    def view(self, i):
        v = self.j["bufferViews"][i]
        o = v.get("byteOffset", 0)
        return self.bin[o : o + v["byteLength"]], v.get("byteStride")

    def acc(self, i):
        a = self.j["accessors"][i]
        dt = np.dtype(COMP[a["componentType"]])
        k = NCOMP[a["type"]]
        cnt = a["count"]
        if "bufferView" not in a:
            return np.zeros((cnt, k), np.float32)
        raw, stride = self.view(a["bufferView"])
        o = a.get("byteOffset", 0)
        if stride and stride != dt.itemsize * k:
            out = np.empty((cnt, k), dt)
            for r in range(cnt):
                out[r] = np.frombuffer(raw, dt, k, o + r * stride)
        else:
            out = np.frombuffer(raw, dt, cnt * k, o).reshape(cnt, k)
        out = out.astype(np.float64) if dt.kind == "f" else out.astype(np.int64)
        if a.get("normalized"):
            out = out / float(np.iinfo(dt).max)
        return out

    def image(self, tex_index):
        from PIL import Image
        import io

        tex = self.j["textures"][tex_index]
        src = tex.get("source")
        if src is None:
            return None
        if src not in self.images:
            im = self.j["images"][src]
            if "bufferView" not in im:
                self.images[src] = None
            else:
                raw, _ = self.view(im["bufferView"])
                self.images[src] = np.asarray(Image.open(io.BytesIO(raw)).convert("RGB"), np.float64) / 255.0
        return self.images[src]

    def world(self):
        """each node's world matrix, by index"""
        js = self.j
        par = {}
        for i, n in enumerate(js["nodes"]):
            for c in n.get("children", []):
                par[c] = i
        loc = []
        for n in js["nodes"]:
            if "matrix" in n:
                m = np.array(n["matrix"], np.float64).reshape(4, 4).T
            else:
                t = n.get("translation", [0, 0, 0])
                x, y, z, w = n.get("rotation", [0, 0, 0, 1])
                s = n.get("scale", [1, 1, 1])
                r = np.array([[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
                              [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
                              [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]])
                m = np.eye(4)
                m[:3, :3] = r * np.array(s)[None, :]
                m[:3, 3] = t
            loc.append(m)
        out = {}

        def w(i):
            if i not in out:
                out[i] = (w(par[i]) if i in par else np.eye(4)) @ loc[i]
            return out[i]

        for i in range(len(js["nodes"])):
            w(i)
        return out


def srgb_hex(h):
    h = h.lstrip("#")
    return np.array([int(h[i : i + 2], 16) / 255.0 for i in (0, 2, 4)])


def lin_to_srgb(c):
    c = np.clip(c, 0, 1)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * np.power(c, 1 / 2.4) - 0.055)


def object_mesh(glb, wm, names, xform):
    """the nodes' primitives in world space: [(triangles (n,3,3), colour factor (sRGB), corner UVs (n,3,2) or None,
    base colour texture (h,w,3 sRGB) or None)]"""
    js = glb.j
    by = {n.get("name"): i for i, n in enumerate(js["nodes"])}
    prims = []
    for nm in names:
        if nm not in by:
            raise SystemExit(f"goo: no node named {nm!r} in the glb")
        ni = by[nm]
        node = js["nodes"][ni]
        if "mesh" not in node:
            continue
        m = xform @ wm[ni]
        for pr in js["meshes"][node["mesh"]]["primitives"]:
            if pr.get("mode", 4) != 4:
                continue
            p = glb.acc(pr["attributes"]["POSITION"])[:, :3]
            p = p @ m[:3, :3].T + m[:3, 3]
            idx = glb.acc(pr["indices"])[:, 0] if "indices" in pr else np.arange(len(p))
            idx = idx.reshape(-1, 3)
            mat = js["materials"][pr["material"]] if "material" in pr else {}
            pbr = mat.get("pbrMetallicRoughness", {})
            fac = lin_to_srgb(np.array(pbr.get("baseColorFactor", [1, 1, 1, 1])[:3]))
            uv, img = None, None
            bt = pbr.get("baseColorTexture")
            key = f"TEXCOORD_{bt.get('texCoord', 0)}" if bt else None
            if bt is not None and key in pr["attributes"]:
                img = glb.image(bt["index"])
                if img is not None:
                    uv = glb.acc(pr["attributes"][key])[idx]
            prims.append((p[idx], fac, uv, img))
    if not prims:
        raise SystemExit(f"goo: nodes {names} have no triangles")
    return prims


def sample_surface(prims, spacing, rng, override=None):
    """points on the triangles about `spacing` apart, each with its colour (the texture at its own UV)"""
    P, Cc = [], []
    for tris, fac, uvs, img in prims:
        a, b, c = tris[:, 0], tris[:, 1], tris[:, 2]
        area = 0.5 * np.linalg.norm(np.cross(b - a, c - a), axis=1)
        n_per = np.maximum(np.ceil(area / (spacing * spacing) * 2.0).astype(np.int64), 1)
        tot = int(n_per.sum())
        if tot > 3_000_000:
            n_per = np.maximum(n_per * 3_000_000 // tot, 1)
        t_ = np.repeat(np.arange(len(tris)), n_per)
        r1 = rng.random(len(t_))
        r2 = rng.random(len(t_))
        s = np.sqrt(r1)
        w = np.stack([1 - s, s * (1 - r2), s * r2], 1)
        # and the corners, so small triangles leave no gaps
        t_ = np.concatenate([t_, np.repeat(np.arange(len(tris)), 3)])
        w = np.concatenate([w, np.tile(np.eye(3), (len(tris), 1))])
        pts = (w[:, :, None] * tris[t_]).sum(1)
        if override is not None:
            col = np.broadcast_to(override, pts.shape)
        elif img is not None:
            uv = (w[:, :, None] * uvs[t_]).sum(1)
            h, wd, _ = img.shape
            px = np.clip((np.mod(uv[:, 0], 1.0) * wd).astype(int), 0, wd - 1)
            py = np.clip((np.mod(uv[:, 1], 1.0) * h).astype(int), 0, h - 1)
            col = img[py, px] * fac[None, :]
        else:
            col = np.broadcast_to(fac, pts.shape)
        P.append(pts)
        Cc.append(np.asarray(col, np.float64))
    return np.concatenate(P), np.concatenate(Cc)


class Cup:
    """a hollow cylinder on a pose track (the spec's cup)"""

    def __init__(self, c, dx):
        self.ri = float(c["radius"])
        self.h = float(c["height"])
        self.base = float(c.get("base", 0.01))
        self.ro = self.ri + max(float(c.get("wall", 0.005)), 2.5 * dx)
        self.fr = float(c.get("friction", 0.1))
        poses = sorted(c["poses"], key=lambda q: q[0])
        self.ts = np.array([q[0] for q in poses], np.float64)
        ms = [np.array(q[1:17], np.float64).reshape(4, 4).T for q in poses]
        self.ps = np.array([m[:3, 3] for m in ms])
        self.qs = np.array([quat_of(m[:3, :3]) for m in ms])
        for k in range(1, len(self.qs)):
            if np.dot(self.qs[k], self.qs[k - 1]) < 0:
                self.qs[k] = -self.qs[k]

    def pose(self, t):
        """(R, c): the cup's frame at time t, rotation and origin"""
        ts = self.ts
        if t <= ts[0]:
            return mat_of(self.qs[0]), self.ps[0]
        if t >= ts[-1]:
            return mat_of(self.qs[-1]), self.ps[-1]
        i = int(np.searchsorted(ts, t) - 1)
        u = (t - ts[i]) / max(ts[i + 1] - ts[i], 1e-9)
        return mat_of(slerp(self.qs[i], self.qs[i + 1], u)), self.ps[i] + (self.ps[i + 1] - self.ps[i]) * u


def quat_of(m):
    """a rotation matrix's unit quaternion (w, x, y, z)"""
    tr = m[0, 0] + m[1, 1] + m[2, 2]
    if tr > 0:
        s = 2.0 * math.sqrt(tr + 1.0)
        q = [0.25 * s, (m[2, 1] - m[1, 2]) / s, (m[0, 2] - m[2, 0]) / s, (m[1, 0] - m[0, 1]) / s]
    elif m[0, 0] > m[1, 1] and m[0, 0] > m[2, 2]:
        s = 2.0 * math.sqrt(1.0 + m[0, 0] - m[1, 1] - m[2, 2])
        q = [(m[2, 1] - m[1, 2]) / s, 0.25 * s, (m[0, 1] + m[1, 0]) / s, (m[0, 2] + m[2, 0]) / s]
    elif m[1, 1] > m[2, 2]:
        s = 2.0 * math.sqrt(1.0 + m[1, 1] - m[0, 0] - m[2, 2])
        q = [(m[0, 2] - m[2, 0]) / s, (m[0, 1] + m[1, 0]) / s, 0.25 * s, (m[1, 2] + m[2, 1]) / s]
    else:
        s = 2.0 * math.sqrt(1.0 + m[2, 2] - m[0, 0] - m[1, 1])
        q = [(m[1, 0] - m[0, 1]) / s, (m[0, 2] + m[2, 0]) / s, (m[1, 2] + m[2, 1]) / s, 0.25 * s]
    q = np.array(q)
    return q / np.linalg.norm(q)


def mat_of(q):
    w, x, y, z = q
    return np.array([[1 - 2 * (y * y + z * z), 2 * (x * y - w * z), 2 * (x * z + w * y)],
                     [2 * (x * y + w * z), 1 - 2 * (x * x + z * z), 2 * (y * z - w * x)],
                     [2 * (x * z - w * y), 2 * (y * z + w * x), 1 - 2 * (x * x + y * y)]])


def slerp(a, b, u):
    d = float(np.clip(np.dot(a, b), -1.0, 1.0))
    if d > 0.9995:
        q = a + (b - a) * u
        return q / np.linalg.norm(q)
    th = math.acos(d)
    return (math.sin((1 - u) * th) * a + math.sin(u * th) * b) / math.sin(th)


def particles_in_cup(obj, cup, ps, rng):
    """a liquid filling a cup to `level` at its first pose: particles on a jittered grid, the object's colour"""
    r = cup.ri - 0.75 * ps
    y0, y1 = cup.base + 0.5 * ps, float(obj["level"])
    g = np.arange(-r, r + 1e-9, ps)
    ys = np.arange(y0, y1, ps)
    X, Y, Z = np.meshgrid(g, ys, g, indexing="ij")
    loc = np.stack([X.ravel(), Y.ravel(), Z.ravel()], 1)
    loc = loc[loc[:, 0] ** 2 + loc[:, 2] ** 2 <= r * r]
    loc = loc + (rng.random(loc.shape) - 0.5) * 0.5 * ps
    R, c = cup.pose(0.0)
    x = loc @ R.T + c
    col = np.tile(np.array(srgb_hex(obj.get("color") or "#2a160b"), np.float32), (len(x), 1))
    return x.astype(np.float32), col


def particles_of(obj, glb, wm, ps, rng):
    from scipy import ndimage
    from scipy.spatial import cKDTree

    xf = np.array(obj.get("transform") or np.eye(4).T.ravel(), np.float64).reshape(4, 4).T
    prims = object_mesh(glb, wm, obj["nodes"], xf)
    pts, pc = sample_surface(prims, ps * 0.4, rng, srgb_hex(obj["color"]) if obj.get("color") else None)
    lo = pts.min(0) - 3 * ps
    hi = pts.max(0) + 3 * ps
    dims = np.ceil((hi - lo) / ps).astype(int) + 1
    ijk = np.floor((pts - lo) / ps).astype(int)
    surf = np.zeros(dims, bool)
    surf[ijk[:, 0], ijk[:, 1], ijk[:, 2]] = True
    fill = obj.get("fill", "solid")
    if fill == "shell":
        t = float(obj.get("thickness", 0.006))
        d = ndimage.distance_transform_edt(~surf) * ps
        inside = d <= max(t / 2 - ps / 2, 0.0)
    else:
        # close small holes, fill, then take the closing back off (keeps the surface voxels)
        dil = ndimage.binary_dilation(surf, iterations=1)
        filled = ndimage.binary_fill_holes(dil)
        inside = ndimage.binary_erosion(filled, iterations=1) | surf
        if inside.sum() < surf.sum() * 1.05:
            # an open mesh (nothing to fill): a shell of its thickness instead
            t = float(obj.get("thickness", 0.006))
            d = ndimage.distance_transform_edt(~surf) * ps
            inside = d <= max(t / 2 - ps / 2, 0.0)
    cells = np.argwhere(inside)
    x = lo + (cells + 0.5) * ps + (rng.random((len(cells), 3)) - 0.5) * 0.5 * ps
    tree = cKDTree(pts)
    _, nn = tree.query(x, k=1)
    return x.astype(np.float32), np.clip(pc[nn], 0, 1).astype(np.float32)


def wet_colour(c, wet):
    """a wet surface: darker and more saturated (sRGB 0..1)"""
    c = np.power(np.clip(c, 0, 1), 1.0 + 0.6 * wet)
    g = c.mean(1, keepdims=True)
    return np.clip(g + (c - g) * (1.0 + wet), 0, 1).astype(np.float32)


# ---------------------------------------------------------------------------------------------------------- solver
def main():
    args = sys.argv[1:]
    glb_dir, every, cpu = None, 10, False
    rest = []
    i = 0
    while i < len(args):
        if args[i] == "--glb":
            glb_dir = Path(args[i + 1]); i += 2
        elif args[i] == "--every":
            every = int(args[i + 1]); i += 2
        elif args[i] == "--cpu":
            cpu = True; i += 1
        else:
            rest.append(args[i]); i += 1
    if len(rest) != 2:
        raise SystemExit("usage: run.py SPEC.json OUT.rseq [--glb DIR --every N] [--cpu]")
    spec = json.loads(Path(rest[0]).read_text())
    out = Path(rest[1])
    if spec.get("solver") == "sph":  # a thin liquid: SPlisHSPlasH (sph.py)
        sys.path.insert(0, str(Path(__file__).parent))
        import sph

        return sph.main(rest)

    fps = float(spec.get("fps", 30))
    frames = int(spec["frames"])
    seed = int(spec.get("seed", 0))
    dx = float(spec.get("dx", 0.005))
    # the surface: the density's blur (grid nodes) and the iso level (of the rest density); a wider blur and a lower
    # level close a thin film (a spill) into one sheet instead of beads
    sf = spec.get("surface", {})
    skin = (float(sf.get("blur", 1.0)), float(sf.get("level", 0.33)), float(sf.get("flat", 1.0)))
    # RAEA_GOO_KEEP=DIR: each frame's live particles saved there too (DIR/NNNN.npz), to try surfaces without the sim
    keep = Path(os.environ["RAEA_GOO_KEEP"]) if os.environ.get("RAEA_GOO_KEEP") else None
    if keep:
        keep.mkdir(parents=True, exist_ok=True)
    grav = np.array(spec.get("gravity", [0, -9.81, 0]), np.float64)
    bmin = np.array(spec["bounds"]["min"], np.float64)
    bmax = np.array(spec["bounds"]["max"], np.float64)
    colliders = [c for c in spec.get("colliders", []) if "box" in c]
    cups = [Cup(c, dx) for c in spec.get("cups", [])]
    objs = spec["objects"]
    rng = np.random.default_rng(seed)
    ps = dx / 2  # 8 particles a cell

    # particles
    glbs = {}
    X, COL, OBJ, DELAY = [], [], [], []
    stats_obj = []
    for k, o in enumerate(objs):
        if "cup" in o:
            x, c = particles_in_cup(o, cups[int(o["cup"])], ps, rng)
        else:
            src = o["src"]
            if src not in glbs:
                g = Glb(src)
                glbs[src] = (g, g.world())
            g, wm = glbs[src]
            x, c = particles_of(o, g, wm, ps, rng)
        melt = max(float(o.get("melt", 2.0)), 1e-3)
        # top first, a little noise: each particle's softening starts up to half the melt later
        h = x[:, 1]
        top = (h.max() - h) / max(float(h.max() - h.min()), 1e-6)
        delay = (0.35 * top + 0.15 * rng.random(len(x))) * melt
        X.append(x); COL.append(c); OBJ.append(np.full(len(x), k, np.int32)); DELAY.append(delay.astype(np.float32))
        stats_obj.append({"name": o.get("name", f"object{k}"), "particles": int(len(x))})
        say(f"{o.get('name', k)}: {len(x)} particles ({o.get('fill', 'solid')})")
    wet = float(spec.get("wet", 0.3))
    COL = [wet_colour(c, wet) for c in COL]
    X = np.concatenate(X); COL = np.concatenate(COL); OBJ = np.concatenate(OBJ); DELAY = np.concatenate(DELAY)
    N = len(X)
    say(f"{N} particles, dx {dx}")

    import taichi as ti

    kw = {"cpu_max_num_threads": 1} if cpu else {}
    ti.init(arch=ti.cpu if cpu else ti.metal, default_fp=ti.f32, offline_cache=True, log_level=ti.WARN, random_seed=seed, **kw)
    dims = np.ceil((bmax - bmin) / dx).astype(int) + 1
    nx, ny, nz = (int(d) for d in dims)
    say(f"grid {nx}x{ny}x{nz}")
    inv_dx = 1.0 / dx
    BX, BY, BZ = (float(c) for c in bmin)
    GX, GY, GZ = (float(c) for c in grav)
    EX, EY, EZ = (float(c) for c in bmax)
    MS = float(1 << 20)  # mass fixed point
    VS = float(1 << 16)  # momentum fixed point (m/s)
    nob = len(objs)

    x = ti.Vector.field(3, ti.f32, N)
    v = ti.Vector.field(3, ti.f32, N)
    C = ti.Matrix.field(3, 3, ti.f32, N)
    F = ti.Matrix.field(3, 3, ti.f32, N)
    Jv = ti.field(ti.f32, N)
    obj = ti.field(ti.i32, N)
    dly = ti.field(ti.f32, N)
    state = ti.field(ti.i32, N)  # 0 frozen obstacle, 1 live, 2 gone, 3 not there (an obstacle no more, not yet live)
    gm = ti.field(ti.i32, (nx, ny, nz))
    gmv = ti.Vector.field(3, ti.i32, (nx, ny, nz))
    gsolid = ti.field(ti.i32, (nx, ny, nz))
    gv = ti.Vector.field(3, ti.f32, (nx, ny, nz))
    o_start = ti.field(ti.f32, nob)
    o_melt = ti.field(ti.f32, nob)
    o_mu = ti.field(ti.f32, nob)
    o_la = ti.field(ti.f32, nob)
    o_eta = ti.field(ti.f32, nob)
    o_rho = ti.field(ti.f32, nob)
    o_until = ti.field(ti.f32, nob)
    o_res = ti.field(ti.f32, nob)
    o_yield = ti.field(ti.f32, nob)
    o_runny = ti.field(ti.f32, nob)
    o_coh = ti.field(ti.f32, nob)
    ncol = max(len(colliders), 1)
    c_lo = ti.Vector.field(3, ti.f32, ncol)
    c_hi = ti.Vector.field(3, ti.f32, ncol)
    c_fr = ti.field(ti.f32, ncol)
    nup = max(len(cups), 1)
    # each cup's frame now (u_R, u_c) and a substep on (u_R2, u_c2): its walls' velocity is the difference
    u_R = ti.Matrix.field(3, 3, ti.f32, nup)
    u_c = ti.Vector.field(3, ti.f32, nup)
    u_R2 = ti.Matrix.field(3, 3, ti.f32, nup)
    u_c2 = ti.Vector.field(3, ti.f32, nup)
    u_dim = ti.Vector.field(4, ti.f32, nup)  # inner radius, outer radius, height, floor
    u_fr = ti.field(ti.f32, nup)

    BIG = 1e9
    mu_max, la_max, eta_max, rho_min = 0.0, 0.0, 0.0, 1e9
    for k, o in enumerate(objs):
        E = float(o.get("stiffness", 3e4)); nu = 0.3
        mu0 = E / (2 * (1 + nu)); la0 = E * nu / ((1 + nu) * (1 - 2 * nu))
        st = o.get("start")
        o_start[k] = BIG if st is None else float(st)
        o_melt[k] = max(float(o.get("melt", 2.0)), 1e-3)
        o_mu[k] = mu0; o_la[k] = la0
        o_eta[k] = float(o.get("viscosity", 12.0))
        o_rho[k] = float(o.get("density", 1000.0))
        un = o.get("obstacle_until")
        o_until[k] = BIG if un is None else float(un)
        o_res[k] = float(o.get("residual", 0.12))
        o_yield[k] = float(o.get("yield", 0.005))
        o_runny[k] = max(float(o.get("runny", 1.5)), 1e-3)
        o_coh[k] = float(o.get("cohesion", 0.6))
        mu_max = max(mu_max, mu0); la_max = max(la_max, la0); eta_max = max(eta_max, o_eta[k]); rho_min = min(rho_min, o_rho[k])
    for k, c in enumerate(colliders):
        c_lo[k] = c["box"]["min"]; c_hi[k] = c["box"]["max"]; c_fr[k] = float(c.get("friction", 0.4))
    n_col = len(colliders)
    for k, cu in enumerate(cups):
        u_dim[k] = [cu.ri, cu.ro, cu.h, cu.base]; u_fr[k] = cu.fr
    n_cup = len(cups)
    poseR = np.zeros((nup, 3, 3), np.float32); posec = np.zeros((nup, 3), np.float32)
    poseR2 = np.zeros((nup, 3, 3), np.float32); posec2 = np.zeros((nup, 3), np.float32)

    def set_cups(t0, t1):
        for k, cu in enumerate(cups):
            poseR[k], posec[k] = cu.pose(t0)
            poseR2[k], posec2[k] = cu.pose(t1)
        u_R.from_numpy(poseR); u_c.from_numpy(posec); u_R2.from_numpy(poseR2); u_c2.from_numpy(posec2)

    x.from_numpy(X); obj.from_numpy(OBJ); dly.from_numpy(DELAY)
    state.from_numpy(np.zeros(N, np.int32))

    @ti.kernel
    def reset():
        for p in x:
            v[p] = ti.Vector([0.0, 0.0, 0.0])
            C[p] = ti.Matrix.zero(ti.f32, 3, 3)
            F[p] = ti.Matrix.identity(ti.f32, 3)
            Jv[p] = 1.0

    @ti.kernel
    def set_states(t: ti.f32):
        for p in x:
            s = state[p]
            if s != 2:
                k = obj[p]
                if t >= o_start[k]:
                    state[p] = 1
                elif t >= o_until[k]:
                    state[p] = 3
                else:
                    state[p] = 0

    @ti.kernel
    def clear(r0x: ti.i32, r0y: ti.i32, r0z: ti.i32, r1x: ti.i32, r1y: ti.i32, r1z: ti.i32):
        for I in ti.grouped(ti.ndrange((r0x, r1x), (r0y, r1y), (r0z, r1z))):
            gm[I] = 0
            gmv[I] = ti.Vector([0, 0, 0])
            gsolid[I] = 0

    @ti.kernel
    def mark(r0x: ti.i32, r0y: ti.i32, r0z: ti.i32, r1x: ti.i32, r1y: ti.i32, r1z: ti.i32):
        for p in x:
            if state[p] == 0:
                g = (x[p] - ti.Vector([BX, BY, BZ])) * inv_dx
                b = ti.cast(ti.floor(g), ti.i32)
                for o in ti.static(ti.grouped(ti.ndrange(2, 2, 2))):
                    I = b + o
                    if r0x <= I[0] < r1x and r0y <= I[1] < r1y and r0z <= I[2] < r1z:
                        gsolid[I] = 1

    @ti.kernel
    def p2g(dt: ti.f32, t: ti.f32, r0x: ti.i32, r0y: ti.i32, r0z: ti.i32, r1x: ti.i32, r1y: ti.i32, r1z: ti.i32):
        for p in x:
            if state[p] == 1:
                g = (x[p] - ti.Vector([BX, BY, BZ])) * inv_dx
                base = ti.cast(ti.floor(g - 0.5), ti.i32)
                if base[0] < r0x or base[1] < r0y or base[2] < r0z or base[0] + 3 > r1x or base[1] + 3 > r1y or base[2] + 3 > r1z:
                    state[p] = 2  # left the simulated region (only past the bounds, by the margins)
                else:
                    k = obj[p]
                    fx = g - ti.cast(base, ti.f32)
                    w = [0.5 * (1.5 - fx) ** 2, 0.75 - (fx - 1.0) ** 2, 0.5 * (fx - 0.5) ** 2]
                    # the melt: 0 solid, 1 fluid
                    u = ti.math.clamp((t - o_start[k] - dly[p]) / o_melt[k], 0.0, 1.0)
                    s = u * u * (3.0 - 2.0 * u)
                    # Bingham: a little shear stiffness stays, and a yield strain the goo must pass to flow, so thin
                    # layers stop spreading and the goo keeps ridges and a rounded edge
                    # and once fluid, both halve every `runny` seconds: what held its shape starts to run and drip
                    age = ti.max(t - o_start[k] - dly[p] - o_melt[k], 0.0)
                    hold = ti.pow(0.5, age / o_runny[k])
                    eta = o_eta[k] * (0.15 + 0.85 * s) * (0.5 + 0.5 * hold)
                    mu = o_mu[k] * ((1.0 - s) ** 3 + o_res[k] * s * hold)
                    theta = 10.0
                    if s > 0.12:
                        theta = 0.05 * (1.0 - s) ** 2 + o_yield[k] * hold
                    I3 = ti.Matrix.identity(ti.f32, 3)
                    Fp = (I3 + dt * C[p]) @ F[p]
                    U, sig, V = ti.svd(Fp)
                    for d in ti.static(range(3)):
                        sig[d, d] = ti.math.clamp(sig[d, d], 1.0 - theta, 1.0 + theta)
                    Fp = U @ sig @ V.transpose()
                    F[p] = Fp
                    R = U @ V.transpose()
                    J = ti.math.clamp(Jv[p] * (1.0 + dt * C[p].trace()), 0.6, 1.4)
                    Jv[p] = J
                    bulk = o_la[k] + 2.0 / 3.0 * o_mu[k]
                    pr = bulk * (J - 1.0) * J
                    if J > 1.0:
                        pr *= o_coh[k]  # cohesion: goo holds together in strands; a liquid barely
                    tau = 2.0 * mu * (Fp - R) @ Fp.transpose() + pr * I3 + J * eta * (C[p] + C[p].transpose())
                    mrel = o_rho[k] / 1000.0
                    aff = (-dt * 4.0 * inv_dx * inv_dx / 1000.0) * tau + mrel * C[p]
                    for o in ti.static(ti.grouped(ti.ndrange(3, 3, 3))):
                        dpos = (ti.cast(o, ti.f32) - fx) * dx
                        wt = w[o[0]][0] * w[o[1]][1] * w[o[2]][2]
                        mv = wt * (mrel * v[p] + aff @ dpos)
                        I = base + o
                        mq = ti.math.clamp(mv * VS, -3.0e8, 3.0e8)
                        q = ti.Vector([ti.cast(ti.round(mq[0]), ti.i32), ti.cast(ti.round(mq[1]), ti.i32), ti.cast(ti.round(mq[2]), ti.i32)])
                        ti.atomic_add(gm[I], ti.cast(ti.round(wt * mrel * MS), ti.i32))
                        ti.atomic_add(gmv[I], q)

    @ti.kernel
    def grid(dt: ti.f32, r0x: ti.i32, r0y: ti.i32, r0z: ti.i32, r1x: ti.i32, r1y: ti.i32, r1z: ti.i32):
        for I in ti.grouped(ti.ndrange((r0x, r1x), (r0y, r1y), (r0z, r1z))):
            m = gm[I]
            vel = ti.Vector([0.0, 0.0, 0.0])
            if m > 0:
                vel = ti.cast(gmv[I], ti.f32) * (MS / VS) / ti.cast(m, ti.f32)
                vel += dt * ti.Vector([GX, GY, GZ])
                pos = ti.Vector([BX, BY, BZ]) + ti.cast(I, ti.f32) * dx
                for c in range(n_col):
                    lo = c_lo[c]; hi = c_hi[c]
                    if lo[0] <= pos[0] <= hi[0] and lo[1] <= pos[1] <= hi[1] and lo[2] <= pos[2] <= hi[2]:
                        # the nearest face's outward normal
                        dn = ti.Vector([pos[0] - lo[0], hi[0] - pos[0], pos[1] - lo[1], hi[1] - pos[1], pos[2] - lo[2], hi[2] - pos[2]])
                        best = 0
                        for f in ti.static(range(1, 6)):
                            if dn[f] < dn[best]:
                                best = f
                        n = ti.Vector([0.0, 0.0, 0.0])
                        for f in ti.static(range(6)):
                            if best == f:
                                n[f // 2] = -1.0 if f % 2 == 0 else 1.0
                        vn = vel.dot(n)
                        if vn < 0.0:
                            vt = vel - vn * n
                            lt = vt.norm()
                            if lt > 1e-9:
                                vel = vt * ti.max(0.0, 1.0 - c_fr[c] * (-vn) / lt)
                            else:
                                vel = ti.Vector([0.0, 0.0, 0.0])
                for c in range(n_cup):
                    # the node in the cup's frame: inside its wall or floor?
                    q = u_R[c].transpose() @ (pos - u_c[c])
                    dm = u_dim[c]
                    r = ti.sqrt(q[0] * q[0] + q[2] * q[2])
                    floor_lo = dm[3] - (dm[1] - dm[0])
                    solid = r <= dm[1] and q[1] >= floor_lo and q[1] <= dm[2] and not (r < dm[0] and q[1] > dm[3])
                    if solid:
                        e = ti.Vector([1.0, 0.0, 0.0])
                        if r > 1e-6:
                            e = ti.Vector([q[0] / r, 0.0, q[2] / r])
                        # the nearest of its surfaces: the inside (only above the floor), the outside, the rim, the
                        # floor's top (inside the bore), the foot
                        best = dm[1] - r
                        nl = e
                        if r >= dm[0] and q[1] > dm[3] and r - dm[0] < best:
                            best = r - dm[0]; nl = -e
                        if dm[2] - q[1] < best:
                            best = dm[2] - q[1]; nl = ti.Vector([0.0, 1.0, 0.0])
                        if r < dm[0] and dm[3] - q[1] < best:
                            best = dm[3] - q[1]; nl = ti.Vector([0.0, 1.0, 0.0])
                        if q[1] - floor_lo < best:
                            best = q[1] - floor_lo; nl = ti.Vector([0.0, -1.0, 0.0])
                        # under the bore's corner (in the wall, below the floor's top): the corner's edge, toward the
                        # bore, or what seeps along the floor goes through it
                        if r >= dm[0] and q[1] <= dm[3]:
                            dr = r - dm[0]; dy = dm[3] - q[1]
                            dc = ti.sqrt(dr * dr + dy * dy)
                            if dc < best:
                                best = dc
                                nl = ti.Vector([0.0, 1.0, 0.0])
                                if dc > 1e-6:
                                    nl = (-dr * e + ti.Vector([0.0, dy, 0.0])) / dc
                        n = u_R[c] @ nl
                        # the wall's velocity here: where this point of the cup is a substep on
                        vw = (u_R2[c] @ q + u_c2[c] - pos) / dt
                        rel = vel - vw
                        vn = rel.dot(n)
                        if vn < 0.0:
                            vt = rel - vn * n
                            lt = vt.norm()
                            if lt > 1e-9:
                                rel = vt * ti.max(0.0, 1.0 - u_fr[c] * (-vn) / lt)
                            else:
                                rel = ti.Vector([0.0, 0.0, 0.0])
                            vel = vw + rel
                if gsolid[I] == 1:
                    vel = ti.Vector([0.0, 0.0, 0.0])
            gv[I] = vel

    @ti.kernel
    def g2p(dt: ti.f32, vmax: ti.f32):
        for p in x:
            if state[p] == 1:
                g = (x[p] - ti.Vector([BX, BY, BZ])) * inv_dx
                base = ti.cast(ti.floor(g - 0.5), ti.i32)
                fx = g - ti.cast(base, ti.f32)
                w = [0.5 * (1.5 - fx) ** 2, 0.75 - (fx - 1.0) ** 2, 0.5 * (fx - 0.5) ** 2]
                nv = ti.Vector([0.0, 0.0, 0.0])
                nC = ti.Matrix.zero(ti.f32, 3, 3)
                for o in ti.static(ti.grouped(ti.ndrange(3, 3, 3))):
                    dpos = ti.cast(o, ti.f32) - fx
                    wt = w[o[0]][0] * w[o[1]][1] * w[o[2]][2]
                    gvel = gv[base + o]
                    nv += wt * gvel
                    nC += 4.0 * inv_dx * wt * gvel.outer_product(dpos)
                l = nv.norm()
                if l > vmax:
                    nv *= vmax / l
                v[p] = nv
                C[p] = nC
                x[p] += dt * nv
                q = x[p]
                if q[0] < BX or q[1] < BY or q[2] < BZ or q[0] > EX or q[1] > EY or q[2] > EZ:
                    state[p] = 2

    reset()
    fdt = 1.0 / fps
    cs = math.sqrt((la_max + 3 * mu_max) / rho_min)
    t = 0.0
    sim_s = 0.0
    surf_s = 0.0
    blobs = []
    tri_counts = []
    pos_lo, pos_hi = bmin, bmax
    COL8 = COL  # sRGB 0..1 per particle
    substeps_total = 0
    glb_written = []
    import multiprocessing as mp
    from concurrent.futures import ProcessPoolExecutor

    workers = int(spec.get("workers", 5))
    pool = ProcessPoolExecutor(workers, mp_context=mp.get_context("spawn"))
    pending = []

    def collect():
        b_, n_ = pending[len(blobs)].result()
        blobs.append(b_)
        tri_counts.append(n_)
    for f in range(frames):
        t = f * fdt
        # the surface of frame f: the state at time t
        t1 = time.time()
        set_states(t)
        st = state.to_numpy()
        xs = x.to_numpy()
        live = st == 1
        gp = str(glb_dir / f"goo_{f:04d}.glb") if glb_dir is not None and f % every == 0 else None
        if keep:
            np.savez(keep / f"{f:04d}.npz", x=xs[live], c=COL8[live], ps=ps, sk=np.array(skin))
        pending.append(pool.submit(surface_job, xs[live].copy(), COL8[live].copy(), ps, gp, skin, cup_bores(cups, t)))
        if gp:
            glb_written.append(f)
        while len(pending) - len(blobs) > 2 * workers:
            collect()
        surf_s += time.time() - t1
        if f == frames - 1:
            break
        # step to the next frame
        t2 = time.time()
        if live.any():
            vs = v.to_numpy()[live]
            vm = float(np.sqrt((vs * vs).sum(1)).max()) if len(vs) else 0.0
            margin = (vm * fdt * 1.5 + 0.5 * float(np.linalg.norm(grav)) * fdt * fdt * 1.5) + 4 * dx
            act = st <= 1
            lo = xs[live].min(0) - margin
            hi = xs[live].max(0) + margin
            r0 = np.clip(np.floor((lo - bmin) / dx).astype(int), 0, dims - 1)
            r1 = np.clip(np.ceil((hi - bmin) / dx).astype(int) + 1, 1, dims)
            dt_c = 0.35 * dx / (cs + vm)
            dt_v = 0.12 * rho_min * dx * dx / max(eta_max, 1e-6)
            dt = min(dt_c, dt_v, fdt)
            n = int(math.ceil(fdt / dt - 1e-9))
            dt = fdt / n
            vcap = 0.9 * dx / dt
            rr = (int(r0[0]), int(r0[1]), int(r0[2]), int(r1[0]), int(r1[1]), int(r1[2]))
            for s in range(n):
                ts = t + s * dt
                if cups:
                    set_cups(ts, ts + dt)
                clear(*rr)
                mark(*rr)
                p2g(dt, ts, *rr)
                grid(dt, *rr)
                g2p(dt, vcap)
            ti.sync()
            substeps_total += n
            n_sub = n
            del act
        sim_s += time.time() - t2
        if f % 10 == 0:
            say(f"frame {f}/{frames}: {int(live.sum())} live, {n_sub if live.any() else 0} substeps, sim {sim_s:.1f}s, surfaced {len(blobs)}")

    t1 = time.time()
    while len(blobs) < len(pending):
        collect()
    pool.shutdown()
    surf_s += time.time() - t1
    nbytes = write_rseq(out, fps, blobs)
    hide = []
    for k, o in enumerate(objs):
        st_ = o.get("start")
        hf = None if st_ is None else int(math.ceil((float(st_) + 0.2) * fps))
        stats_obj[k]["start"] = st_
        stats_obj[k]["hideFrame"] = hf
        hide.append(hf)
    stats = {"particles": int(N), "objects": stats_obj, "frames": frames, "fps": fps, "dx": dx,
             "substeps": substeps_total, "simSeconds": round(sim_s, 2), "surfaceSeconds": round(surf_s, 2),
             "seconds": round(time.time() - T0, 2), "bytes": nbytes, "maxTriangles": int(max(tri_counts) if tri_counts else 0),
             "glb": glb_written, "backend": "cpu" if cpu else "metal"}
    Path(str(out) + ".json").write_text(json.dumps(stats, indent=1))
    say(f"wrote {out} ({nbytes / 1e6:.1f} MB, {frames} frames, max {stats['maxTriangles']} triangles) in {stats['seconds']}s")


# ---------------------------------------------------------------------------------------------------------- surface
MAX_TRIS = 120000


def surface_job(xs, cols, ps, glb_path, skin=(1.0, 0.33, 1.0), cups=None):
    blob, ntri, mesh = surface(xs, cols, ps, skin, cups)
    if glb_path and mesh is not None:
        write_glb(Path(glb_path), mesh)
    return blob, ntri


def surface(xs, cols, ps, skin=(1.0, 0.33, 1.0), cups=None):
    """the live particles' surface: (blob, triangles, mesh or None). `cups`: each cup now, (R, c, inside radius, floor,
    rim), so what's inside one is skinned inside its bore (cup_bores)"""
    if len(xs) == 0:
        return frame_blob(None), 0, None
    # skin: blur, level, flat, and optionally its own triangle cap (a wide film one particle thick needs more than the
    # default; past the cap the grid coarsens, and a coarser grid loses the film)
    cap = int(skin[3]) if len(skin) > 3 else MAX_TRIS
    h = ps
    for _ in range(4):
        mesh = reconstruct(xs.astype(np.float64), cols.astype(np.float64), h, ps, *skin[:3], cups=cups)
        if mesh is None or len(mesh[3]) <= cap:
            break
        h *= math.sqrt(len(mesh[3]) / (cap * 0.85))
    if mesh is None:
        return frame_blob(None), 0, None
    return frame_blob(mesh), len(mesh[3]), mesh


def cup_bores(cups, t):
    """Each cup at time `t` as (R, c, inside radius, floor, rim): what the surface needs to keep a cup's contents
    inside it"""
    return [(*cu.pose(t), cu.ri, cu.base, cu.h) for cu in cups] if cups else None


def in_bores(p, bores):
    """which points `p` (n x 3, world) are in a cup's bore, between its floor and its rim"""
    m = np.zeros(len(p), bool)
    for R, c, ri, base, rim in bores or ():
        q = (p - c) @ R
        m |= (np.hypot(q[:, 0], q[:, 2]) < ri) & (q[:, 1] > base - 0.005) & (q[:, 1] < rim)
    return m


def reconstruct(xs, cols, h, ps, blur=1.0, iso=0.33, flat=1.0, cups=None):
    """The particles' density on a grid of h, blurred, its iso-surface. Sparse: the grid is cut into tiles and only
    tiles near particles are built, so a drop far from the rest never makes the grid coarser (a coarse grid turns the
    thin, invisible film a splash leaves into a thick sheet). Neighbouring tiles share their boundary nodes, whose
    values come out the same in both (the margin covers the splat and both blurs), so their vertices weld.

    `cups` (each (R, c, inside radius, floor, rim)): the particles in a cup's bore are blurred on their own and kept
    to the bore, so a blur wider across than up (`flat`, for a film on the stone) doesn't carry the coffee still in
    the mug out through its wall, where it showed as a gold band round the mug (2026-10-21)."""
    from scipy import ndimage
    from skimage import measure

    T = 40  # tile cells
    sig = (blur * flat, blur, blur * flat)  # wider across than up: a flat film closes, a stream stays thin
    M = int(math.ceil(6 * max(sig))) + 2  # margin nodes: the colour blur (1.5 sig, truncated at 4) and the splat
    rest = min((ps / h) ** 3, 1.0)
    level = iso * rest
    org = np.floor(xs.min(0) / h) * h - 2 * h
    inb = in_bores(xs, cups) if cups else None
    g = (xs - org) / h
    b = np.floor(g).astype(np.int64)
    fr = g - b
    tl = b // T
    occ = np.unique(tl, axis=0)
    near = np.unique((occ[:, None, :] + np.stack(np.meshgrid(*[[-1, 0, 1]] * 3, indexing="ij"), -1).reshape(-1, 3)[None]).reshape(-1, 3), axis=0)
    P, N, C, F = [], [], [], []
    nv = 0
    for t in near:
        n0 = t * T - M
        sel = np.all((b >= n0 - 1) & (b <= n0 + T + 2 * M), axis=1)
        if not sel.any():
            continue
        lb, lf, lc = b[sel] - n0, fr[sel], cols[sel]
        li = inb[sel] if inb is not None else None
        dims = np.array([T + 2 * M + 1] * 3)
        nt = int(np.prod(dims))
        dens = np.zeros(nt, np.float64)
        dens_in = np.zeros(nt, np.float64) if li is not None and li.any() else None
        csum = np.zeros((nt, 3), np.float64)
        for ox in (0, 1):
            wx = lf[:, 0] if ox else 1 - lf[:, 0]
            for oy in (0, 1):
                wy = lf[:, 1] if oy else 1 - lf[:, 1]
                for oz in (0, 1):
                    wz = lf[:, 2] if oz else 1 - lf[:, 2]
                    w = wx * wy * wz
                    q = lb + np.array([ox, oy, oz])
                    ok = np.all((q >= 0) & (q < dims), axis=1)
                    idx = (q[ok, 0] * dims[1] + q[ok, 1]) * dims[2] + q[ok, 2]
                    dens += np.bincount(idx, w[ok], nt)
                    if dens_in is not None:
                        dens_in += np.bincount(idx, (w * li)[ok], nt)
                    for c in range(3):
                        csum[:, c] += np.bincount(idx, w[ok] * lc[ok, c], nt)
        dens = dens.reshape(dims)
        if dens_in is None:
            dsm = ndimage.gaussian_filter(dens, sig, mode="constant")
        else:
            # what's in a bore, blurred and kept to it (to its rim; above the rim it's open); the rest as ever
            dens_in = dens_in.reshape(dims)
            nodes = org + (n0 + np.stack(np.meshgrid(*[np.arange(d) for d in dims], indexing="ij"), -1).reshape(-1, 3)) * h
            keep = np.zeros(len(nodes), bool)
            for R, c, ri, base, rim in cups:
                q = (nodes - c) @ R
                keep |= ((np.hypot(q[:, 0], q[:, 2]) < ri) & (q[:, 1] > base - 0.005)) | (q[:, 1] >= rim)
            dsm = ndimage.gaussian_filter(dens - dens_in, sig, mode="constant") + ndimage.gaussian_filter(dens_in, sig, mode="constant") * keep.reshape(dims)
        core = dsm[M:M + T + 1, M:M + T + 1, M:M + T + 1]
        if core.max() <= level or core.min() >= level:
            continue
        try:
            verts, faces, _, _ = measure.marching_cubes(core, level=level, allow_degenerate=False)
        except (ValueError, RuntimeError):
            continue
        if len(faces) == 0:
            continue
        csum = csum.reshape(tuple(dims) + (3,))
        lv = verts + M  # in the tile's padded grid
        csm = np.stack([ndimage.gaussian_filter(csum[..., c], [q * 1.5 for q in sig], mode="constant") for c in range(3)], -1)
        wsm = ndimage.gaussian_filter(dens, [q * 1.5 for q in sig], mode="constant")
        cw = ndimage.map_coordinates(wsm, lv.T, order=1, mode="nearest")
        cc = np.stack([ndimage.map_coordinates(csm[..., c], lv.T, order=1, mode="nearest") for c in range(3)], -1)
        # outward: skimage's winding is the same in every tile, but check against the density's slope per tile
        fn = np.cross(verts[faces[:, 1]] - verts[faces[:, 0]], verts[faces[:, 2]] - verts[faces[:, 0]])
        gradv = np.gradient(dsm)
        cen = (lv[faces[:, 0]] + lv[faces[:, 1]] + lv[faces[:, 2]]) / 3
        gc = np.stack([ndimage.map_coordinates(gradv[c], cen.T, order=1, mode="nearest") for c in range(3)], -1)
        if float((fn * gc).sum()) > 0:
            faces = faces[:, [0, 2, 1]]
        P.append(verts + t * T)
        C.append(np.clip(cc / np.maximum(cw, 1e-9)[:, None], 0, 1))
        F.append(faces + nv)
        nv += len(verts)
    if not F:
        return None
    verts = np.concatenate(P); vc = np.concatenate(C); faces = np.concatenate(F)
    # weld the tiles' shared boundary vertices
    _, first, inv = np.unique(np.round(verts * 4096).astype(np.int64), axis=0, return_index=True, return_inverse=True)
    inv = inv.ravel()
    verts, vc, faces = verts[first], vc[first], inv[faces]
    faces = faces[(faces[:, 0] != faces[:, 1]) & (faces[:, 1] != faces[:, 2]) & (faces[:, 2] != faces[:, 0])]
    verts = taubin(verts, faces, 6)
    pos = org + verts * h
    fn = np.cross(pos[faces[:, 1]] - pos[faces[:, 0]], pos[faces[:, 2]] - pos[faces[:, 0]])
    vn = np.zeros_like(pos)
    for c in range(3):
        np.add.at(vn, faces[:, c], fn)
    vn /= np.maximum(np.linalg.norm(vn, axis=1, keepdims=True), 1e-12)
    return pos.astype(np.float32), vn.astype(np.float32), vc.astype(np.float32), faces.astype(np.uint32)


def taubin(v, f, iters, lam=0.5, mu=-0.53):
    from scipy import sparse

    n = len(v)
    e = np.concatenate([f[:, [0, 1]], f[:, [1, 2]], f[:, [2, 0]]])
    e = np.concatenate([e, e[:, ::-1]])
    a = sparse.coo_matrix((np.ones(len(e)), (e[:, 0], e[:, 1])), shape=(n, n)).tocsr()
    a.data[:] = 1.0
    deg = np.asarray(a.sum(1)).ravel()
    deg[deg == 0] = 1
    w = sparse.diags(1.0 / deg) @ a
    for _ in range(iters):
        v = v + lam * (w @ v - v)
        v = v + mu * (w @ v - v)
    return v


# ---------------------------------------------------------------------------------------------------------- output
def frame_blob(mesh):
    import zstandard

    if mesh is None:
        raw = struct.pack("<II6f", 0, 0, 0, 0, 0, 0, 0, 0)
    else:
        pos, nrm, col, faces = mesh
        lo = pos.min(0)
        hi = pos.max(0)
        span = np.where(hi - lo > 0, hi - lo, 1.0)
        q = np.round((pos - lo) / span * 65535).clip(0, 65535).astype("<u2")
        nq = np.round(np.clip(nrm, -1, 1) * 32767).astype("<i2")
        c8 = np.concatenate([np.round(col * 255).clip(0, 255).astype(np.uint8), np.full((len(pos), 1), 255, np.uint8)], 1)
        raw = b"".join([struct.pack("<II", len(pos), len(faces)), lo.astype("<f4").tobytes(), hi.astype("<f4").tobytes(),
                        q.tobytes(), nq.tobytes(), c8.tobytes(), faces.astype("<u4").tobytes()])
    return zstandard.ZstdCompressor(level=6, write_checksum=False, write_content_size=True).compress(raw)


def write_rseq(out, fps, blobs):
    """b"RSEQ", u32 version 1, u32 header length, the header (JSON), the frames' zstd blobs"""
    def header(base):
        idx = []
        o = base
        for b in blobs:
            idx.append([o, len(b)])
            o += len(b)
        return json.dumps({"fps": fps, "frames": len(blobs), "codec": "zstd", "attrs": ["position", "normal", "color"],
                           "texture": None, "index": idx}, separators=(",", ":")).encode()

    # the index's offsets depend on the header's own length: settle it
    base = 12
    for _ in range(8):
        h = header(base + 0)
        nb = 12 + len(h)
        if nb == base:
            break
        base = nb
    h = header(base)
    assert 12 + len(h) == base, "header length didn't settle"
    out.parent.mkdir(parents=True, exist_ok=True)
    tmp = out.with_suffix(out.suffix + ".part")
    with open(tmp, "wb") as fh:
        fh.write(b"RSEQ" + struct.pack("<II", 1, len(h)) + h)
        for b in blobs:
            fh.write(b)
    tmp.replace(out)
    return out.stat().st_size


def write_glb(path, mesh):
    """one frame as a glb for looking at: vertex colours, a glossy clear-coated material"""
    pos, nrm, col, faces = mesh
    c8 = np.concatenate([np.round(col * 255).astype(np.uint8), np.full((len(pos), 1), 255, np.uint8)], 1)
    parts = [pos.astype("<f4").tobytes(), nrm.astype("<f4").tobytes(), c8.tobytes(), faces.astype("<u4").tobytes()]
    views, off = [], 0
    for b in parts:
        views.append({"buffer": 0, "byteOffset": off, "byteLength": len(b)})
        off += len(b)
    binb = b"".join(parts)
    j = {"asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [{"name": "goo", "mesh": 0}],
         "extensionsUsed": ["KHR_materials_clearcoat"],
         "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1, "COLOR_0": 2}, "indices": 3, "material": 0}]}],
         "materials": [{"name": "goo", "pbrMetallicRoughness": {"baseColorFactor": [1, 1, 1, 1], "metallicFactor": 0.0, "roughnessFactor": 0.22},
                        "extensions": {"KHR_materials_clearcoat": {"clearcoatFactor": 1.0, "clearcoatRoughnessFactor": 0.05}}}],
         "buffers": [{"byteLength": len(binb)}], "bufferViews": views,
         "accessors": [{"bufferView": 0, "componentType": 5126, "count": len(pos), "type": "VEC3", "min": pos.min(0).tolist(), "max": pos.max(0).tolist()},
                       {"bufferView": 1, "componentType": 5126, "count": len(pos), "type": "VEC3"},
                       {"bufferView": 2, "componentType": 5121, "normalized": True, "count": len(pos), "type": "VEC4"},
                       {"bufferView": 3, "componentType": 5125, "count": int(faces.size), "type": "SCALAR"}]}
    js = json.dumps(j).encode()
    js += b" " * (-len(js) % 4)
    binb += b"\0" * (-len(binb) % 4)
    body = struct.pack("<II", len(js), 0x4E4F534A) + js + struct.pack("<II", len(binb), 0x004E4942) + binb
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"glTF" + struct.pack("<II", 2, 12 + len(body)) + body)


def read_rseq(path):
    """(header, [(pos, nrm, rgba, idx), ...]) — for tests and for looking"""
    import zstandard

    b = Path(path).read_bytes()
    assert b[:4] == b"RSEQ", "not an rseq"
    ver, hl = struct.unpack_from("<II", b, 4)
    assert ver == 1
    hd = json.loads(b[12 : 12 + hl])
    out = []
    dz = zstandard.ZstdDecompressor()
    for off, ln in hd["index"]:
        raw = dz.decompress(b[off : off + ln])
        nv, nt = struct.unpack_from("<II", raw, 0)
        lo = np.frombuffer(raw, "<f4", 3, 8)
        hi = np.frombuffer(raw, "<f4", 3, 20)
        o = 32
        q = np.frombuffer(raw, "<u2", nv * 3, o).reshape(-1, 3); o += nv * 6
        nq = np.frombuffer(raw, "<i2", nv * 3, o).reshape(-1, 3); o += nv * 6
        c = np.frombuffer(raw, np.uint8, nv * 4, o).reshape(-1, 4); o += nv * 4
        idx = np.frombuffer(raw, "<u4", nt * 3, o).reshape(-1, 3); o += nt * 12
        assert o == len(raw), f"blob size {len(raw)} != {o}"
        pos = lo + (hi - lo) * q / 65535.0
        out.append((pos, nq / 32767.0, c, idx))
    return hd, out


if __name__ == "__main__":
    main()
