"""The EID MAR denarius (RRC 508/3) as a textured silver coin, built from one public-domain photograph of both faces
(Münzkabinett, Staatliche Museen zu Berlin, photo Reinhard Saczewski; Wikimedia Commons). No open 3D scan of an EID MAR
exists that we can download (the one CC BY model on Sketchfab is 704 faces with a blurred texture).

  blender -b --factory-startup -P tools/coin.py -- PHOTO.jpg OUT.glb [--maps DIR]

Each face's outline is read from the photo (white background), so the flan keeps the real coin's shape. The colour is
the photo with its broad studio light divided out; the height comes from the photo's relief (lighter-than-field masses
and lines raised, the field flat, the rim rolled off); the mesh is displaced to the coarse relief, a normal map carries
the fine; roughness is low on the worn high points and high in the recesses. Obverse (Brutus) faces +Z, reverse
(pileus, daggers, EID MAR) faces -Z, flat in XY, upright along +Y, 19 mm across, metres, centred.
"""
import bpy, bmesh, sys, os, math
import numpy as np

argv = sys.argv[sys.argv.index("--") + 1:]
src, out = argv[0], argv[1]
maps = argv[argv.index("--maps") + 1] if "--maps" in argv else None

DIAM = 0.019        # m across
THICK = 0.0021      # m, field to field
RELIEF = 0.00045    # m, the highest relief above the field
TEX = 1024          # px per face
NR, NS = 150, 540   # rings and segments per face

# ------------------------------------------------------------------ image helpers (numpy only: Blender has no scipy)
def load(path):
    im = bpy.data.images.load(path)
    w, h = im.size
    px = np.empty(w * h * 4, np.float32)
    im.pixels.foreach_get(px)
    return px.reshape(h, w, 4)[::-1, :, :3].copy()  # top row first, sRGB-encoded floats

def blur(a, s):
    if s <= 0:
        return a
    h, w = a.shape
    ph, pw = h + 4 * int(s), w + 4 * int(s)
    pad = np.pad(a, ((0, ph - h), (0, pw - w)), mode="edge")
    fy = np.fft.fftfreq(ph)[:, None]
    fx = np.fft.rfftfreq(pw)[None, :]
    g = np.exp(-2 * (math.pi * s) ** 2 * (fx ** 2 + fy ** 2))
    return np.fft.irfft2(np.fft.rfft2(pad) * g, s=(ph, pw))[:h, :w]

def sample(img, x, y):
    """bilinear, img HxW or HxWxC, x/y pixel coordinates (any shape)"""
    h, w = img.shape[:2]
    x = np.clip(x, 0, w - 1.001); y = np.clip(y, 0, h - 1.001)
    x0 = np.floor(x).astype(int); y0 = np.floor(y).astype(int)
    fx = x - x0; fy = y - y0
    if img.ndim == 3:
        fx = fx[..., None]; fy = fy[..., None]
    a = img[y0, x0] * (1 - fx) + img[y0, x0 + 1] * fx
    b = img[y0 + 1, x0] * (1 - fx) + img[y0 + 1, x0 + 1] * fx
    return a * (1 - fy) + b * fy

def smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)

def lin(c):
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)

# ------------------------------------------------------------------ the two faces out of the photo
photo = load(src)
H, W, _ = photo.shape
lum_all = photo @ np.array([0.2126, 0.7152, 0.0722])

ANG = 1440
def face(x0, x1):
    """centre, outline radius per angle (pixels), from one half of the photo"""
    lum = lum_all[:, x0:x1]
    mask = blur((lum < 0.9).astype(float), 1.5) > 0.5
    ys, xs = np.nonzero(mask)
    cx, cy = xs.mean() + x0, ys.mean()
    th = np.linspace(0, 2 * math.pi, ANG, endpoint=False)
    rmax = min(x1 - x0, H) * 0.6
    rs = np.linspace(0, rmax, int(rmax * 2))
    xx = cx - x0 + np.cos(th)[:, None] * rs[None, :]
    yy = cy - np.sin(th)[:, None] * rs[None, :]   # image y runs down; angle runs counter-clockwise in the world
    inside = sample(mask.astype(float), xx, yy) > 0.5
    # the outline: the last radius still inside before the first long run outside
    r = np.array([rs[np.nonzero(row)[0].max()] if row.any() else rmax * 0.5 for row in inside])
    # smooth the outline a little (the background's JPEG noise)
    k = np.exp(-0.5 * (np.arange(-6, 7) / 2.5) ** 2); k /= k.sum()
    r = np.convolve(np.r_[r[-6:], r, r[:6]], k, mode="same")[6:-6]
    return cx, cy, r

obv = face(0, W // 2)
rev = face(W // 2, W)
print(f"COIN obverse centre ({obv[0]:.0f},{obv[1]:.0f}) r {obv[2].mean():.1f}px; reverse centre ({rev[0]:.0f},{rev[1]:.0f}) r {rev[2].mean():.1f}px")

def unwarp(f, mirror):
    """the face resampled to a TEX square in normalised polar terms: pixel (i,j) at radius fraction and angle, so the
    outline lands on the unit circle. mirror: viewed from behind (the reverse), x runs the other way."""
    cx, cy, r = f
    g = (np.arange(TEX) + 0.5) / TEX * 2 - 1
    X, Y = np.meshgrid(g, -g)  # Y up
    rho = np.hypot(X, Y)
    th = np.mod(np.arctan2(Y, -X if mirror else X), 2 * math.pi)
    R = sample(r[None, :], th / (2 * math.pi) * ANG, np.zeros_like(th))
    px = cx + np.cos(th) * rho * R
    py = cy - np.sin(th) * rho * R
    rgb = sample(photo, px, py)
    return rgb, rho

# The smooth masses the photo can't show as relief (no edges inside them), traced by hand on the unwarped faces, as
# fractions of the face square (x right, y down; the reverse square is already mirrored, as it's stored).
MASSES = {
    # Brutus's head and neck, the truncation at the bottom
    "obv": [[(0.41, 0.29), (0.52, 0.26), (0.63, 0.27), (0.68, 0.33), (0.69, 0.41), (0.70, 0.46), (0.74, 0.49), (0.70, 0.51),
             (0.71, 0.55), (0.70, 0.61), (0.66, 0.65), (0.59, 0.67), (0.60, 0.75), (0.52, 0.77), (0.41, 0.76), (0.42, 0.65),
             (0.39, 0.56), (0.37, 0.46), (0.35, 0.37), (0.37, 0.31)]],
    # the pileus: its dome and brim
    "rev": [[(0.41, 0.62), (0.41, 0.56), (0.43, 0.53), (0.47, 0.51), (0.52, 0.51), (0.56, 0.53), (0.58, 0.57), (0.59, 0.61),
             (0.66, 0.61), (0.66, 0.65), (0.41, 0.66)]],
}

def polygon_mask(polys):
    g = (np.arange(TEX) + 0.5) / TEX
    X, Y = np.meshgrid(g, g)
    m = np.zeros((TEX, TEX), bool)
    for poly in polys:
        inside = np.zeros((TEX, TEX), bool)
        for (x0, y0), (x1, y1) in zip(poly, poly[1:] + poly[:1]):
            cross = ((y0 > Y) != (y1 > Y)) & (X < (x1 - x0) * (Y - y0) / ((y1 - y0) or 1e-9) + x0)
            inside ^= cross
        m |= inside
    return m.astype(float)

maps_out = {}
def build_maps(f, mirror, tag):
    rgb, rho = unwarp(f, mirror)
    lum = rgb @ np.array([0.2126, 0.7152, 0.0722])
    inside = rho < 1.0
    field = np.median(lum[(rho > 0.55) & (rho < 0.8)])
    lumf = np.where(inside, lum, field)
    # the studio light, broad, divided out of the colour
    light = blur(lumf, TEX / 14)
    flat = np.clip(lumf / np.maximum(light, 1e-3) * field, 0, 1)
    # relief: the design's masses (head, daggers, cap, letters, beads: lighter than the field at a middle scale) stand
    # up, the lines inside them add detail, and the field stays flat (its scratches are colour, not relief)
    # The design is where the photo has local contrast (struck relief throws edges and shadows; the field is smooth);
    # one photo's shading can't say which way a slope faces, so the relief is built from that design mask: big masses
    # (the head, the cap, the daggers) filled and domed, thin ones (letters, beads, hair) kept at a finer scale.
    energy = blur(np.abs(flat - blur(flat, 3)), 2)
    e = energy[rho < 0.95]
    lo, hi_ = np.percentile(e, 55), np.percentile(e, 80)  # about a quarter of a face is struck design
    print(f"COIN {tag} energy p55 {lo:.4f} p80 {hi_:.4f}")
    fine = smoothstep(lo, hi_, energy)
    fine = smoothstep(0.4, 0.7, blur(fine, 2.5))          # specks fall away
    big = smoothstep(0.3, 0.55, blur(fine, 9))            # masses filled
    traced = blur(polygon_mask(MASSES[tag]), 2.5)
    inner = 1 - smoothstep(0.86, 0.92, rho)               # the photo's own edge is not design
    fine *= inner
    big = np.maximum(big * inner, traced)
    fine *= smoothstep(0.1, 0.35, blur(big, 4))           # detail only in or beside the design: the field is smooth
    # inside a traced mass (a cheek, a neck) lone specks are pores in the photo, not relief: keep only dense detail (hair)
    fine = fine * (1 - traced) + traced * fine * smoothstep(0.3, 0.55, blur(fine, 4))
    dome = blur(big, 16) * big                            # higher in the middle of a mass, like a struck portrait
    h = blur(0.6 * dome + 0.3 * fine * (0.4 + 0.6 * big) + 0.12 * fine, 1.2)
    if maps:
        maps_out[tag + "_dbg"] = np.stack([fine, big, dome], -1)
    # the rim: the field rolls off over the last 5% to the edge
    h = h * (1 - smoothstep(0.93, 1.0, rho)) - 0.6 * smoothstep(0.95, 1.02, rho)
    h = np.clip(h, -1, 1)
    hi = np.clip(h / max(np.percentile(h[inside], 99.5), 1e-3), -1, 1)
    # colour: the de-lit photo, its hue kept faintly, set to bright silver for a metal's albedo, darker in recesses
    grey = flat[..., None]
    tint = np.clip(rgb / np.maximum(lum[..., None], 1e-3), 0.85, 1.15)
    col = np.clip(grey * (0.55 + 0.45 * tint), 0, 1)
    col = col / max(np.median(col[(rho > 0.55) & (rho < 0.8)]), 1e-3) * 0.72
    cav = np.clip(blur(hi, 3) - hi, 0, 1)
    col = np.clip(col * (1 - 0.6 * cav[..., None]) * np.array([1.0, 0.985, 0.95]), 0, 1)
    # roughness: polished where worn (high), dull in the recesses and the field
    rough = np.clip(0.52 - 0.24 * np.clip(hi, 0, 1) + 0.25 * cav + 0.04 * (blur(np.random.default_rng(1).random(hi.shape), 2) - 0.5), 0.18, 0.75)
    maps_out[tag] = dict(h=hi, col=col, rough=rough)

build_maps(obv, False, "obv")
build_maps(rev, True, "rev")

# ------------------------------------------------------------------ the textures: an atlas, obverse left, reverse right
def atlas(key, chans):
    a = np.concatenate([maps_out["obv"][key], maps_out["rev"][key]], axis=1)
    return a if a.ndim == 3 else a[..., None].repeat(chans, 2)

def normal_from(hh, strength):
    fine = blur(hh, 0.8) - blur(hh, 5)           # the mesh carries the coarse relief; the map the fine, without ringing
    dx = np.gradient(fine, axis=1) * strength
    dy = -np.gradient(fine, axis=0) * strength   # image rows run down, v runs up
    n = np.stack([-dx, -dy, np.ones_like(dx)], -1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    return n * 0.5 + 0.5

col = atlas("col", 3)
# the edge's own patch: the atlas's bottom-left corner (outside the coin) in the field's colour, mid roughness, flat
P = TEX // 12
field_col = np.median(maps_out["obv"]["col"][TEX // 2 - 40:TEX // 2 + 40, int(TEX * 0.86):int(TEX * 0.9)].reshape(-1, 3), axis=0)
col[-P:, :P] = field_col * (0.92 + 0.08 * np.random.default_rng(2).random((P, P, 1)))
nrm = np.concatenate([normal_from(maps_out["obv"]["h"], 5), normal_from(maps_out["rev"]["h"], 5)], axis=1)
rough = atlas("rough", 1)[..., 0]
rough[-P:, :P] = 0.4
nrm[-P:, :P] = (0.5, 0.5, 1.0)
mr = np.stack([np.zeros_like(rough), rough, np.ones_like(rough)], -1)  # glTF: G roughness, B metallic

def to_image(name, rgb, srgb):
    h, w = rgb.shape[:2]
    im = bpy.data.images.new(name, w, h, alpha=False, float_buffer=False)
    if not srgb:
        im.colorspace_settings.name = "Non-Color"
    rgba = np.concatenate([rgb, np.ones((h, w, 1))], -1)[::-1].astype(np.float32)
    im.pixels.foreach_set(rgba.ravel())
    im.file_format = "PNG"
    im.pack()
    if maps:
        os.makedirs(maps, exist_ok=True)
        im.filepath_raw = os.path.join(maps, name + ".png")
        im.save()
    return im

img_col = to_image("eid_mar_color", col, True)
img_nrm = to_image("eid_mar_normal", nrm, False)
img_mr = to_image("eid_mar_mr", mr, False)
if maps:
    to_image("eid_mar_dbg", np.concatenate([maps_out["obv_dbg"], maps_out["rev_dbg"]], axis=1), False)
    to_image("eid_mar_height", (atlas("h", 3) * 0.5 + 0.5), False)

# ------------------------------------------------------------------ the mesh
R0 = DIAM / 2
outline = obv[2] / obv[2].mean() * R0               # metres per angle, the obverse's own flan
def R_at(th):
    return sample(outline[None, :], np.mod(th, 2 * math.pi) / (2 * math.pi) * ANG, np.zeros_like(th))

verts, uvs, faces = [], [], []
def add_face(tag, sign, u0):
    """a polar grid: centre vertex, then NR rings of NS; z = sign*(T/2 + relief*h)"""
    hh = maps_out[tag]["h"]
    base = len(verts)
    th = np.linspace(0, 2 * math.pi, NS, endpoint=False)
    def push(x, y, rho_img, th_img):
        # the atlas pixel for this point: polar in the face's own square (the reverse mirrored in x)
        # (the reverse's square is already mirrored, by unwarp: seen from -Z, world +x is the viewer's left)
        gx = rho_img * np.cos(th_img); gy = rho_img * np.sin(th_img)
        pu = (gx + 1) / 2; pv = (gy + 1) / 2
        hval = float(sample(hh, np.array(pu * TEX - 0.5), np.array((1 - pv) * TEX - 0.5)))
        z = sign * (THICK / 2 + RELIEF * hval)
        verts.append((x, y, z))
        uvs.append((u0 + pu * 0.5, pv))
    push(0.0, 0.0, 0.0, 0.0)
    for i in range(1, NR + 1):
        f = (i / NR) ** 0.85                          # rings a little closer near the rim
        for t in th:
            R = float(R_at(np.array(t)))
            push(f * R * math.cos(t), f * R * math.sin(t), f, t)
    # triangles: the centre fan, then quads ring to ring (wound so the face looks out along sign*Z)
    def vid(i, j):
        return base + 1 + (i - 1) * NS + (j % NS)
    for j in range(NS):
        tri = (base, vid(1, j), vid(1, j + 1))
        faces.append(tri if sign > 0 else tri[::-1])
    for i in range(1, NR):
        for j in range(NS):
            q = (vid(i, j), vid(i + 1, j), vid(i + 1, j + 1), vid(i, j + 1))
            faces.append(q if sign > 0 else q[::-1])
    return lambda j: vid(NR, j)

ob_ring = add_face("obv", +1, 0.0)
rv_ring = add_face("rev", -1, 0.5)

# the edge: rounded, from the obverse's last ring to the reverse's, through a few rings bulging out a touch
EDGE = 6
th = np.linspace(0, 2 * math.pi, NS, endpoint=False)
edge_rows = [[ob_ring(j) for j in range(NS)]]
for k in range(1, EDGE):
    phi = k / EDGE * math.pi
    row = []
    for j, t in enumerate(th):
        R = float(R_at(np.array(t))) * (1.0 + 0.012 * math.sin(phi))
        z = (THICK / 2) * math.cos(phi)
        row.append(len(verts))
        verts.append((R * math.cos(t), R * math.sin(t), z))
        # the edge's colour: the obverse's outermost field, along the angle
        uvs.append(((0.3 + 0.4 * (j % 7) / 7) / 12 * 0.5, (0.3 + 0.4 * k / EDGE) / 12))   # the edge patch
    edge_rows.append(row)
edge_rows.append([rv_ring(j) for j in range(NS)])
for a, b in zip(edge_rows, edge_rows[1:]):
    for j in range(NS):
        faces.append((a[j], a[(j + 1) % NS], b[(j + 1) % NS], b[j]))

mesh = bpy.data.meshes.new("eid_mar")
mesh.from_pydata(verts, [], faces)
mesh.update()
uvl = mesh.uv_layers.new(name="UVMap")
loop_vi = np.empty(len(mesh.loops), np.int32)
mesh.loops.foreach_get("vertex_index", loop_vi)
uv_arr = np.array(uvs, np.float32)[loop_vi]
uvl.data.foreach_set("uv", uv_arr.ravel())
for p in mesh.polygons:
    p.use_smooth = True

ob = bpy.data.objects.new("eid_mar", mesh)
bpy.context.scene.collection.objects.link(ob)

mat = bpy.data.materials.new("eid_mar_silver")
mat.use_nodes = True
nt = mat.node_tree
bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
tc = nt.nodes.new("ShaderNodeTexImage"); tc.image = img_col
nt.links.new(tc.outputs["Color"], bsdf.inputs["Base Color"])
tm = nt.nodes.new("ShaderNodeTexImage"); tm.image = img_mr
sep = nt.nodes.new("ShaderNodeSeparateColor")
nt.links.new(tm.outputs["Color"], sep.inputs["Color"])
nt.links.new(sep.outputs[1], bsdf.inputs["Roughness"])
nt.links.new(sep.outputs[2], bsdf.inputs["Metallic"])
tn = nt.nodes.new("ShaderNodeTexImage"); tn.image = img_nrm
nm = nt.nodes.new("ShaderNodeNormalMap")
nt.links.new(tn.outputs["Color"], nm.inputs["Color"])
nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
ob.data.materials.append(mat)

for o in list(bpy.context.scene.objects):
    if o is not ob:
        bpy.data.objects.remove(o, do_unlink=True)

# Blender Z up -> glTF Y up: the exporter turns +Z (obverse) into glTF +Y. We want the obverse along glTF +Z and the
# coin upright along +Y, so the coin is turned +90 deg about X first (Blender +Z -> -Y, Blender +Y -> +Z): after the
# exporter's own conversion (glTF = (x, z, -y)) Blender -Y becomes glTF +Z and Blender +Z becomes glTF +Y.
ob.rotation_euler = (math.radians(90), 0, 0)
bpy.context.view_layer.objects.active = ob
ob.select_set(True)
bpy.ops.object.transform_apply(location=False, rotation=True, scale=False)

bpy.ops.export_scene.gltf(filepath=out, export_format="GLB", use_selection=True, export_tangents=True,
                          export_image_format="AUTO", export_yup=True, export_apply=True)
print(f"COIN wrote {out}: {len(verts)} verts, {len(faces)} faces")
