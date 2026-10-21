"""The floor shot's dagger at rest on its slab, and the blood that has run off it, from the meshes themselves.

  uv run --python 3.12 --with trimesh --with numpy --with scipy python -I tools/blood.py [--debug DIR]

1. The slab's top: ruins/008.glb as shots.tsx draws it (at the origin, turned 2.2 rad about y), its triangles
   rasterised from above onto a 2 mm grid (the first surface a ray straight down meets), joints and all. A joint is a
   cell lower than the stone on both sides of it; the sim gives the joints a packed floor 1 mm under the lower rim,
   which wicks, as do the 12 mm of stone along it (the corner draws blood along), and drinks a little: blood that
   reaches a joint runs along it, downhill, a rivulet down the stone beside it.
2. The dagger at rest: the bare dagger at scale 2.6, lying on its side at the same spot and heading as before. For a
   tilt (pitch and roll about its centre of mass) it is lowered until its lowest point touches the height field; the
   tilt kept is the one with the lowest centre of mass, a stable rest, which stands on three contacts.
3. The blood: a cellular thin-film flow on the height field (2 mm cells). Blood comes off the blade's lower edge and
   pools at the guard; where the edge bridges the joint, what runs along it drips straight in; each step a cell passes blood to its four neighbours by the difference in surface level,
   with mobility going as the film's thickness cubed (viscous film) and a seeded per-cell pinning thickness the front
   must exceed to wet a dry cell (the stone's roughness), so it stops in lobes. Seeded rng only, so it is the same
   every time.
4. Meshes, in media/props/blood/floor_blood.glb, one glTF scene each: scenes 0..N-1 are the run of blood at N growing
   stages (each a sheet over the 2 mm grid whose wet vertices sit on the stone plus the film's thickness and whose
   first dry ring is 0.8 mm under the stone, so the stone itself cuts the edge where the film thins out), and scene
   N is the film on the blade, in the dagger's own frame (the blade's triangles, subdivided, masked by a seeded
   streaky field, cut along its zero line so the edge is smooth, and lifted 0.12 mm along their normals). The material is wet arterial blood: dark red, glossy,
   clearcoated (KHR_materials_clearcoat, which Bevy's glTF loader reads), darker where it is thick.

It prints the dagger's Group position and rotation and the stages' thicknesses for shots.tsx.
"""
import json, struct, sys
from pathlib import Path

import numpy as np
import trimesh
from scipy import ndimage
from scipy.spatial.transform import Rotation

HERE = Path(__file__).resolve().parent.parent
SLAB = HERE / "media/ruins/008.glb"
DAGGER = HERE / "media/props/ornate_medieval_dagger/ornate_medieval_dagger_bare.gltf"
OUT = HERE / "media/props/blood/floor_blood.glb"
DEBUG = Path(sys.argv[sys.argv.index("--debug") + 1]) if "--debug" in sys.argv else None

SLAB_YAW = 2.2
DX = 0.002                       # m, the grid
X0, X1, Z0, Z1 = -0.62, 0.82, -0.02, 1.42
# the dagger as shots.tsx had it: Group at (x, ?, z), rotation [pi/2, 0, 0.9], scale 2.6
D_AT = (0.2, 0.5)
D_ROT = (np.pi / 2, 0.0, 0.9)
D_SCALE = 2.6
SEED = 1544                      # 15 March 44 BC
JOINT_DEPTH = 0.001              # m: the joint's packed floor under its lower rim

rng = np.random.default_rng(SEED)


# ------------------------------------------------------------------ the slab's height field
def height_field():
    s = trimesh.load(SLAB, force="scene")
    m = s.to_geometry()
    m.apply_transform(trimesh.transformations.rotation_matrix(SLAB_YAW, [0, 1, 0]))
    nx, nz = int(round((X1 - X0) / DX)) + 1, int(round((Z1 - Z0) / DX)) + 1
    xs, zs = X0 + np.arange(nx) * DX, Z0 + np.arange(nz) * DX
    H = np.full((nz, nx), -np.inf)
    tri = m.vertices[m.faces]
    lo, hi = tri.min(1), tri.max(1)
    keep = (hi[:, 0] >= X0) & (lo[:, 0] <= X1) & (hi[:, 2] >= Z0) & (lo[:, 2] <= Z1)
    for t in tri[keep]:
        (ax, ay, az), (bx, by, bz), (cx, cy, cz) = t
        det = (bz - cz) * (ax - cx) + (cx - bx) * (az - cz)
        if abs(det) < 1e-14:
            continue  # seen edge on from above: a vertical face
        i0 = max(0, int(np.ceil((min(ax, bx, cx) - X0) / DX)))
        i1 = min(nx - 1, int(np.floor((max(ax, bx, cx) - X0) / DX)))
        j0 = max(0, int(np.ceil((min(az, bz, cz) - Z0) / DX)))
        j1 = min(nz - 1, int(np.floor((max(az, bz, cz) - Z0) / DX)))
        if i1 < i0 or j1 < j0:
            continue
        X, Z = np.meshgrid(xs[i0:i1 + 1], zs[j0:j1 + 1])
        l1 = ((bz - cz) * (X - cx) + (cx - bx) * (Z - cz)) / det
        l2 = ((cz - az) * (X - cx) + (ax - cx) * (Z - cz)) / det
        l3 = 1 - l1 - l2
        inside = (l1 >= -1e-9) & (l2 >= -1e-9) & (l3 >= -1e-9)
        y = l1 * ay + l2 * by + l3 * cy
        sub = H[j0:j1 + 1, i0:i1 + 1]
        np.maximum(sub, np.where(inside, y, -np.inf), out=sub)
    H[~np.isfinite(H)] = 0.0  # nothing under it: the ground
    return xs, zs, H


def joints(H):
    """cells lower than the stone on both sides (within 3 cm) along some direction, and the floor the sim gives them"""
    k = 25
    gap = np.zeros(H.shape, bool)
    rim = np.full(H.shape, -np.inf)
    for dz, dx in ((0, 1), (1, 0), (1, 1), (1, -1)):
        a = np.full(H.shape, -np.inf)
        b = np.full(H.shape, -np.inf)
        for s in range(1, k + 1):
            a = np.maximum(a, shift(H, dz * s, dx * s))
            b = np.maximum(b, shift(H, -dz * s, -dx * s))
        low = np.minimum(a, b)               # the lower of the stones either side, across the joint this way
        g = low - H > 0.006
        gap |= g
        # across the joint this is the lower rim; along it (in the crack) it's lower still: keep the highest
        rim = np.where(g, np.maximum(rim, low), rim)
    gap = ndimage.binary_dilation(gap, iterations=1) & (H < ndimage.maximum_filter(H, 5) - 0.001) | gap
    # the joint's packed floor: JOINT_DEPTH under the lower rim beside it (cells the dilation added take their
    # neighbours' rim)
    near = ndimage.grey_dilation(rim, size=5)
    rim = np.where(np.isfinite(rim), rim, near)
    rim = np.where(np.isfinite(rim), rim, ndimage.minimum_filter(np.where(gap, np.inf, H), 41))
    B = np.where(gap, np.maximum(H, rim - JOINT_DEPTH), H)
    return gap, B


def shift(a, dz, dx, fill=-np.inf):
    out = np.full(a.shape, fill, a.dtype)
    nz, nx = a.shape
    out[max(0, dz):nz + min(0, dz), max(0, dx):nx + min(0, dx)] = a[max(0, -dz):nz + min(0, -dz), max(0, -dx):nx + min(0, -dx)]
    return out


def bilinear(F, x, z):
    fx, fz = (x - X0) / DX, (z - Z0) / DX
    i, j = np.clip(np.floor(fx).astype(int), 0, F.shape[1] - 2), np.clip(np.floor(fz).astype(int), 0, F.shape[0] - 2)
    u, v = np.clip(fx - i, 0, 1), np.clip(fz - j, 0, 1)
    return (F[j, i] * (1 - u) * (1 - v) + F[j, i + 1] * u * (1 - v) + F[j + 1, i] * (1 - u) * v + F[j + 1, i + 1] * u * v)


# ------------------------------------------------------------------ the dagger at rest
def euler_xyz(R):
    """Bevy's Quat::from_euler(EulerRot::XYZ, a, b, c) = Rx(a) Ry(b) Rz(c): the angles of R"""
    return Rotation.from_matrix(R).as_euler("XYZ")


def dagger():
    d = trimesh.load(DAGGER, force="scene")
    m = d.to_geometry()
    fine = m.subdivide_to_size(0.0008)
    v = fine.vertices if isinstance(fine, trimesh.Trimesh) else fine[0]
    # centre of mass from the slices' cross sections (the mesh isn't closed): an ellipse of each slice's extents
    ys = np.arange(m.vertices[:, 1].min(), m.vertices[:, 1].max(), 0.002)
    mass, my = 0.0, 0.0
    for y in ys:
        s = m.vertices[(m.vertices[:, 1] >= y - 0.003) & (m.vertices[:, 1] < y + 0.003)]
        if len(s) < 3:
            continue
        a = np.pi / 4 * np.ptp(s[:, 0]) * np.ptp(s[:, 2])
        mass += a
        my += a * y
    com = np.array([0.0, my / mass, 0.0])
    return m, np.asarray(v), com


def rest(H, pts, com):
    R0 = Rotation.from_euler("XYZ", D_ROT).as_matrix()
    P0 = (pts * D_SCALE) @ R0.T              # in the world's axes, about the Group's origin
    C0 = (com * D_SCALE) @ R0.T

    def drop(a, b):
        Q = Rotation.from_euler("xz", [a, b]).as_matrix()   # pitch and roll in the world's x and z
        P = (P0 - C0) @ Q.T + C0
        x, z = P[:, 0] + D_AT[0], P[:, 2] + D_AT[1]
        lift = np.max(bilinear(H, x, z) - P[:, 1])
        return lift + C0[1], Q, lift, P

    best = None
    for step, span, c in ((0.01, 0.2, (0.0, 0.0)), (0.002, 0.02, None), (0.0004, 0.004, None)):
        c = c or best[1:]
        for a in np.arange(c[0] - span, c[0] + span + 1e-9, step):
            for b in np.arange(c[1] - span, c[1] + span + 1e-9, step):
                y = drop(a, b)[0]
                if best is None or y < best[0]:
                    best = (y, a, b)
    _, a, b = best
    _, Q, lift, P = drop(a, b)
    R = Q @ R0
    # the Group's origin: C0 + Q (0 - C0) moved by the drop
    origin = np.array([D_AT[0], lift, D_AT[1]]) + (C0 - Q @ C0)
    world = (pts * D_SCALE) @ R.T + origin
    gapv = world[:, 1] - bilinear(H, world[:, 0], world[:, 2])
    touch = world[gapv < 0.0004]
    return origin, R, euler_xyz(R), world, touch


# ------------------------------------------------------------------ the blood
def smooth_noise(shape, sigma, seed_rng):
    n = ndimage.gaussian_filter(seed_rng.standard_normal(shape), sigma, mode="wrap")
    return n / n.std()


def sources(R, origin, H, gap):
    """where blood comes off: the blade's lower edge (where the film on it runs to), and the guard"""
    out = []
    y = np.linspace(0.075, 0.245, 12)
    for side in (-1, 1):
        loc = np.stack([np.full_like(y, side * 0.0108), y, np.zeros_like(y)], 1) * D_SCALE
        w = loc @ R.T + origin
        out.append(w)
    sides = [o[:, 1].mean() for o in out]
    low = out[int(np.argmin(sides))]
    high = out[int(np.argmax(sides))]
    guard = (np.array([[0.0, 0.05, 0.0], [0.03, 0.05, 0.0], [-0.03, 0.05, 0.0]]) * D_SCALE) @ R.T + origin
    pts, rates = [], []
    for i, p in enumerate(low):
        # where the edge bridges the joint there's no stone under it: what runs to there drips straight in
        j, k = int(round((p[2] - Z0) / DX)), int(round((p[0] - X0) / DX))
        over = gap[max(0, j - 6):j + 7, max(0, k - 6):k + 7].any()
        pts.append(p); rates.append((1.0 + 0.6 * rng.random()) * (12.0 if over else 1.0))
    for i, p in enumerate(high[::3]):
        pts.append(p); rates.append(0.25 + 0.2 * rng.random())
    for p in guard:
        pts.append(p); rates.append(2.2)
    pts = np.array(pts)
    rates = np.array(rates)
    return pts, rates / rates.sum()


def simulate(B, gap, src, rates, steps=26000, snap_every=100):
    nz, nx = B.shape
    rough = 0.0001 * smooth_noise(B.shape, 3, rng)            # the stone's grain, for the flow only
    bed = B + rough
    pin = 0.0017 + 0.0004 * smooth_noise(B.shape, 6, rng) + 0.00015 * smooth_noise(B.shape, 1.5, rng)    # the thickness a front needs to wet a dry cell
    pin = np.clip(pin, 0.0009, 0.0028)
    pin[gap] = 0.0004                                            # a joint wicks: blood runs on along it,
    edge = ndimage.binary_dilation(gap, iterations=5) & ~gap     # and along the corner where its edges meet it
    pin[edge] = np.minimum(pin[edge], 0.0005)
    h = np.zeros(B.shape)
    ever = np.zeros(B.shape, bool)
    VOL = 3.3e-4                                                 # m3: a third of a litre
    cell = DX * DX
    emit = int(steps * 0.45)
    prof = np.exp(-np.arange(emit) / (emit * 0.45))
    prof /= prof.sum()
    si = [(int(round((p[2] - Z0) / DX)), int(round((p[0] - X0) / DX))) for p in src]
    disk = [(dz, dx) for dz in range(-2, 3) for dx in range(-2, 3) if dz * dz + dx * dx <= 4]
    C, H0 = 0.12, 0.002
    snaps = []
    for t in range(steps):
        if t < emit:
            for (j, i), r in zip(si, rates):
                dv = VOL * r * prof[t] / cell / len(disk)
                for dz, dx in disk:
                    h[j + dz, i + dx] += dv
        wet = h > 1e-6
        ys, xs = np.nonzero(wet)
        j0, j1 = max(1, ys.min() - 2), min(nz - 1, ys.max() + 3)
        i0, i1 = max(1, xs.min() - 2), min(nx - 1, xs.max() + 3)
        hh = h[j0 - 1:j1 + 1, i0 - 1:i1 + 1]
        W = bed[j0 - 1:j1 + 1, i0 - 1:i1 + 1] + hh
        pn = pin[j0 - 1:j1 + 1, i0 - 1:i1 + 1]
        mob = np.minimum(C * (hh / H0) ** 3, 0.2)
        flows = []
        out = np.zeros_like(hh)
        for ax in (0, 1):
            a = (slice(None, -1), slice(None)) if ax == 0 else (slice(None), slice(None, -1))
            b = (slice(1, None), slice(None)) if ax == 0 else (slice(None), slice(1, None))
            dW = W[a] - W[b]
            up_a = dW > 0                                   # a flows into b
            hu = np.where(up_a, hh[a], hh[b])
            hd = np.where(up_a, hh[b], hh[a])
            pd = np.where(up_a, pn[b], pn[a])
            m = np.where(up_a, mob[a], mob[b])
            f = m * np.abs(dW)
            f = np.where((hd < 1e-6) & (hu < pd), 0.0, f)
            f = np.minimum(f, 0.5 * np.abs(dW))
            flows.append((a, b, up_a, f))
            out[a] += np.where(up_a, f, 0)
            out[b] += np.where(up_a, 0, f)
        scale = np.where(out > 0, np.minimum(1.0, 0.45 * hh / np.maximum(out, 1e-30)), 1.0)
        dh = np.zeros_like(hh)
        for a, b, up_a, f in flows:
            g = f * np.where(up_a, scale[a], scale[b])
            s = np.where(up_a, g, -g)
            dh[a] -= s
            dh[b] += s
        hh += dh
        gp = gap[j0 - 1:j1 + 1, i0 - 1:i1 + 1]
        hh[gp] *= 1 - 4e-6                                     # the joint's packed dirt drinks a little
        np.maximum(hh, 0, out=hh)
        ever |= h > 0.00012
        if t % snap_every == snap_every - 1:
            snaps.append((t, h.copy(), ever.copy()))
    return snaps


# ------------------------------------------------------------------ meshes
def blood_color(th):
    """linear RGB by thickness: a thin film is a brighter red, a pool nearly black-red"""
    k = np.clip((th - 0.0002) / 0.0025, 0, 1)[:, None]
    thin = np.array([0.1022, 0.0024, 0.0018])     # #5a0806
    thick = np.array([0.0232, 0.0006, 0.0006])    # #2a0202
    return thin * (1 - k) + thick * k


def sheet(xs, zs, B, th, wet):
    """the 2 mm grid's cells with a wet corner: wet vertices at B + th, the dry ring 0.8 mm under the stone"""
    T = np.where(wet, th, 0.0)
    # a dry vertex sits 0.8 mm under the stone, or level with the lowest wet neighbour if that is lower (blood down in
    # a joint): there it runs into the joint's wall, inside the stone, instead of climbing out of the joint
    Yw = np.where(wet, B + T, np.inf)
    low = Yw.copy()
    for dz in (-1, 0, 1):
        for dx in (-1, 0, 1):
            low = np.minimum(low, shift(Yw, dz, dx, np.inf))
    Y = np.where(wet, B + T, np.minimum(B - 0.0008, low))
    nz, nx = B.shape
    cellw = wet[:-1, :-1] | wet[1:, :-1] | wet[:-1, 1:] | wet[1:, 1:]
    used = np.zeros(B.shape, bool)
    used[:-1, :-1] |= cellw; used[1:, :-1] |= cellw; used[:-1, 1:] |= cellw; used[1:, 1:] |= cellw
    idx = -np.ones(B.shape, np.int64)
    idx[used] = np.arange(used.sum())
    X, Z = np.meshgrid(xs, zs)
    pos = np.stack([X[used], Y[used], Z[used]], 1)
    gz, gx = np.gradient(Y, DX)
    nrm = np.stack([-gx, np.ones_like(gx), -gz], -1)[used]
    nrm /= np.linalg.norm(nrm, axis=1, keepdims=True)
    j, i = np.nonzero(cellw)
    a, b, c, d = idx[j, i], idx[j, i + 1], idx[j + 1, i], idx[j + 1, i + 1]
    # wound counter-clockwise seen from above (+y): x right, z toward the viewer
    tris = np.concatenate([np.stack([a, c, b], 1), np.stack([b, c, d], 1)])
    col = blood_color(T[used])
    return pos.astype(np.float32), nrm.astype(np.float32), col.astype(np.float32), tris.astype(np.uint32)


def blade_film(m):
    """blood on the blade: its triangles (beyond the guard), subdivided, masked by a streaky seeded field and lifted
    along their normals, in the dagger's own frame"""
    v, f = m.vertices, m.faces
    c = v[f].mean(1)
    blade = (c[:, 1] > 0.062) & (np.abs(c[:, 2]) < 0.0045)
    sub = trimesh.Trimesh(v, f[blade], process=False).subdivide_to_size(0.0012)
    if not isinstance(sub, trimesh.Trimesh):
        sub = trimesh.Trimesh(*sub, process=False)
    sub.merge_vertices()
    V, F, VN = sub.vertices, sub.faces, sub.vertex_normals
    # streaks run along the blade (y), coarse across it (x): a noise on a stretched grid, read bilinearly per vertex
    gx, gy = 64, 256
    n = ndimage.gaussian_filter(rng.standard_normal((gy, gx)), (10, 2.2), mode="wrap")
    n /= n.std()
    fx = (V[:, 0] + 0.012) / 0.024 * (gx // 2 - 1)
    fy = (V[:, 1] - 0.06) / 0.21 * (gy - 1)
    off = np.where(V[:, 2] >= 0, 0, gx // 2)          # each face of the blade its own half of the noise
    i0 = np.clip(np.floor(fx).astype(int), 0, gx // 2 - 2)
    j0 = np.clip(np.floor(fy).astype(int), 0, gy - 2)
    tx, ty = np.clip(fx - i0, 0, 1), np.clip(fy - j0, 0, 1)
    i0 = i0 + off
    nv = (n[j0, i0] * (1 - tx) * (1 - ty) + n[j0, i0 + 1] * tx * (1 - ty) + n[j0 + 1, i0] * (1 - tx) * ty
          + n[j0 + 1, i0 + 1] * tx * ty)
    u = (V[:, 1] - 0.06) / 0.207                        # 0 at the guard, 1 at the tip
    # thickest toward the tip (the part that went in), wiped thinner near the guard, streaked
    sc = 1.1 * u + 0.45 * nv - 0.45
    # the film is where sc > 0: whole triangles inside, and the inside part of each one the zero line crosses, cut
    # along it (marching triangles), so the edge is a smooth line rather than steps
    pos, nrm, tris = [V], [VN], []
    s = sc[F] > 0
    tris.append(F[s.all(1)])
    extra = len(V)
    newp, newn, newt = [], [], []

    def cut(a, b):
        t = sc[a] / (sc[a] - sc[b])
        newp.append(V[a] + (V[b] - V[a]) * t)
        nn = VN[a] + (VN[b] - VN[a]) * t
        newn.append(nn / np.linalg.norm(nn))
        return extra + len(newp) - 1

    for f, m in zip(F[s.any(1) & ~s.all(1)], s[s.any(1) & ~s.all(1)]):
        k = int(np.argmax(m != (m.sum() == 2)))        # the odd vertex out
        a, b, c = f[k], f[(k + 1) % 3], f[(k + 2) % 3]
        ab, ac = cut(a, b), cut(a, c)
        if m.sum() == 1:                               # a inside: one triangle
            newt.append([a, ab, ac])
        else:                                          # a outside: a quad of b, c and the two cuts
            newt += [[ab, b, c], [ab, c, ac]]
    P = np.concatenate([V, np.array(newp)]) if newp else V
    N = np.concatenate([VN, np.array(newn)]) if newn else VN
    T = np.concatenate([tris[0], np.array(newt, np.int64).reshape(-1, 3)])
    keep = np.unique(T)
    remap = -np.ones(len(P), np.int64)
    remap[keep] = np.arange(len(keep))
    P, N, T = P[keep], N[keep], remap[T]
    lift = 0.00012
    pos = P + N * lift
    th = np.full(len(pos), 0.0022)
    return pos.astype(np.float32), N.astype(np.float32), blood_color(th).astype(np.float32), T.astype(np.uint32)


def write_glb(path, parts):
    """one glTF scene per part (pos, normal, colour, tris), all with the wet-blood material"""
    blob = bytearray()
    views, accs, meshes, nodes, scenes = [], [], [], [], []

    def add(arr, target, comp, typ, minmax=False):
        nonlocal blob
        while len(blob) % 4:
            blob += b"\0"
        off = len(blob)
        blob += arr.tobytes()
        views.append({"buffer": 0, "byteOffset": off, "byteLength": arr.nbytes, "target": target})
        a = {"bufferView": len(views) - 1, "componentType": comp, "count": len(arr), "type": typ}
        if minmax:
            a["min"] = arr.min(0).tolist()
            a["max"] = arr.max(0).tolist()
        accs.append(a)
        return len(accs) - 1

    for k, (pos, nrm, col, tris) in enumerate(parts):
        p = add(pos, 34962, 5126, "VEC3", True)
        n = add(nrm, 34962, 5126, "VEC3")
        c = add(col, 34962, 5126, "VEC3")
        i = add(tris.reshape(-1), 34963, 5125, "SCALAR")
        meshes.append({"primitives": [{"attributes": {"POSITION": p, "NORMAL": n, "COLOR_0": c}, "indices": i, "material": 0}]})
        nodes.append({"mesh": k, "name": f"blood{k}"})
        scenes.append({"nodes": [k]})
    while len(blob) % 4:
        blob += b"\0"
    gl = {
        "asset": {"version": "2.0", "generator": "rome/tools/blood.py"},
        "extensionsUsed": ["KHR_materials_clearcoat", "KHR_materials_specular"],
        "scene": 0, "scenes": scenes, "nodes": nodes, "meshes": meshes,
        "materials": [{
            "name": "wet_blood",
            "pbrMetallicRoughness": {"baseColorFactor": [1, 1, 1, 1], "metallicFactor": 0.0, "roughnessFactor": 0.45},
            # the wet sheen: a clearcoat, held to 0.55 so that in shadow, lit by the night sky alone, it reads as dark
            # red rather than the sky's blue-grey; the base's own specular warmed toward the blood's red
            "extensions": {"KHR_materials_clearcoat": {"clearcoatFactor": 0.55, "clearcoatRoughnessFactor": 0.05},
                           "KHR_materials_specular": {"specularFactor": 0.4, "specularColorFactor": [1.0, 0.4, 0.35]}},
        }],
        "accessors": accs, "bufferViews": views, "buffers": [{"byteLength": len(blob)}],
    }
    js = json.dumps(gl, separators=(",", ":")).encode()
    while len(js) % 4:
        js += b" "
    out = struct.pack("<III", 0x46546C67, 2, 12 + 8 + len(js) + 8 + len(blob))
    out += struct.pack("<II", len(js), 0x4E4F534A) + js + struct.pack("<II", len(blob), 0x004E4942) + bytes(blob)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(out)


def png(path, a, lo=None, hi=None):
    from PIL import Image
    lo = np.nanmin(a) if lo is None else lo
    hi = np.nanmax(a) if hi is None else hi
    v = np.clip((a - lo) / max(hi - lo, 1e-12), 0, 1)
    Image.fromarray((v[::-1] * 255).astype(np.uint8)).save(path)


def main():
    xs, zs, H = height_field()
    gap, B = joints(H)
    m, pts, com = dagger()
    origin, R, eul, world, touch = rest(H, pts, com)
    # the contacts, grouped by where along the dagger they are
    loc = ((touch - origin) @ R) / D_SCALE
    groups = []
    for p in loc[np.argsort(loc[:, 1])]:
        if groups and abs(p[1] - groups[-1][-1][1]) < 0.01:
            groups[-1].append(p)
        else:
            groups.append([p])
    contacts = [np.mean(g, 0).round(4).tolist() for g in groups]
    src, rates = sources(R, origin, H, gap)
    snaps = simulate(B, gap, src, rates)
    final = snaps[-1][2].sum()
    # stages by wetted area: the first is how the pool stands when the shot opens, the last where it stops
    fr = [0.62, 0.72, 0.8, 0.87, 0.93, 0.97, 1.0]
    picks = []
    for f in fr:
        picks.append(next(s for s in snaps if s[2].sum() >= f * final - 1e-9))
    parts, deltas = [], []
    prev = None
    for t, h, ever in picks:
        # pin holes a few cells across (dry islands the front closed round) are wetted by the film
        lab, n = ndimage.label(ndimage.binary_fill_holes(ever) & ~ever)
        if n:
            size = ndimage.sum(np.ones_like(h), lab, np.arange(1, n + 1))
            ever = ever | np.isin(lab, 1 + np.nonzero(size < 60)[0])
        th = ndimage.gaussian_filter(h, 1.2)
        th = np.where(ever, np.maximum(th, 0.00022), 0.0)
        parts.append(sheet(xs, zs, B, th, ever))
        new = ever & ~prev if prev is not None else ever
        deltas.append(float(th[new].max()) if new.any() else 0.0)
        prev = ever
    parts.append(blade_film(m))
    write_glb(OUT, parts)
    print(json.dumps({
        "group_position": (origin).round(5).tolist(),
        "group_rotation": eul.round(5).tolist(),
        "contacts_dagger_frame": contacts,
        "lowest_gap_mm": float((world[:, 1] - bilinear(H, world[:, 0], world[:, 2])).min() * 1000),
        "stages": len(picks), "stage_steps": [p[0] for p in picks],
        "stage_rise_m": [round(d, 5) for d in deltas],
        "max_mm_on_stone": round(float(picks[-1][1][~gap].max() * 1000), 2), "max_mm_in_joint": round(float(picks[-1][1][gap].max() * 1000), 2),
        "deepest_at_xz": [round(float(xs[np.unravel_index(np.argmax(picks[-1][1]), H.shape)[1]]), 3), round(float(zs[np.unravel_index(np.argmax(picks[-1][1]), H.shape)[0]]), 3)],
        "area_m2": float(final * DX * DX), "volume_l": float(picks[-1][1].sum() * DX * DX * 1000),
        "tris": [len(p[3]) for p in parts],
        "out": str(OUT.relative_to(HERE)),
    }, indent=1))
    if DEBUG:
        DEBUG.mkdir(parents=True, exist_ok=True)
        top = np.percentile(H[~gap], 99)
        png(DEBUG / "height.png", np.where(gap, np.nan, H), top - 0.03, top)
        png(DEBUG / "gap.png", gap.astype(float))
        for k, (t, h, ever) in enumerate(picks):
            img = np.where(gap, 0.15, 0.4) + 0.6 * np.clip(h / 0.004, 0, 1)
            img = np.where(ever & (h < 1e-5), 0.3, img)
            dm = np.zeros(H.shape, bool)
            ii = ((world[:, 0] - X0) / DX).astype(int).clip(0, H.shape[1] - 1)
            jj = ((world[:, 2] - Z0) / DX).astype(int).clip(0, H.shape[0] - 1)
            dm[jj, ii] = True
            png(DEBUG / f"stage{k}.png", np.where(dm, 1.0, img), 0, 1)


if __name__ == "__main__":
    main()
