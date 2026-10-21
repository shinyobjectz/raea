"""void's cat in 3D: the pixel cat of apps/void/brand/logo (black, tail up, two ears, two white slit eyes) as a rigged glTF
with its clips, for raea's <Model> (and its looks: solid, pixel, site).

    blender -b -P apps/void/brand/cat3d/build.py          -> apps/void/brand/cat3d/media/cat.glb

Its proportions, the logo's pose and the angle the logo sees it from are the numbers in P, overridden by
params.json beside this file, which fit.py writes by fitting the cat's silhouette, seen from that angle, to the
logo's 32 x 32 pixels (fit.py has how).

Blender coordinates (Z up, the cat facing -Y, which glTF turns into +Z, toward a camera at +Z). The body is built from
rounded parts (body, a head sunk into it with no neck and no nose, ears, eyes, four pawless two-part legs, a tail
tube), joined into one mesh and skinned to a 21-bone armature by distance to each part's own bones, so a joint bends
smoothly and nothing pulls on a part it isn't part of. media/cat-white.glb is the same cat white with black eyes
(the logo on void's black), and the canonical cat: shown look="site" theme="dark" on black. Clips (seconds, looping unless said): Cinematic 30 (story.py's performance, its root
moving; play once), Logo (the logo's pose, held), Idle 3, Walk 1, Trot 0.6, Sit 3 (sat down, breathing), LookAround 4, Pounce 1.6 (play once), Blink 0.4 (the
eyes alone, to play under anything).
"""

import json
import math
import os

import bmesh
import bpy
from mathutils import Matrix, Vector

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "media", "cat.glb")
FPS = 30

# the cat's numbers (metres, Blender axes); params.json overrides any of them
P = {
    # body: a bean
    "body_y": 0.05, "body_z": 0.43, "body_rx": 0.21, "body_ry": 0.33, "body_rz": 0.20,
    # head, sunk into the body's front
    "head_y": -0.24, "head_z": 0.58, "head_rx": 0.205, "head_ry": 0.18, "head_rz": 0.165, "cheek": 0.18,
    # ears: base across from the middle, above the head's centre; height, base radius, lean out, top width (of the base)
    "ear_x": 0.11, "ear_z": 0.10, "ear_h": 0.18, "ear_r": 0.085, "ear_out": 0.04, "ear_top": 0.5,
    # eyes: across from the middle, above the head's centre, half width, half height
    "eye_x": 0.074, "eye_z": 0.02, "eye_w": 0.027, "eye_h": 0.062,
    # tail: root, height, lean back, curl at the tip, radius at root and tip
    "tail_y": 0.30, "tail_z": 0.47, "tail_h": 0.62, "tail_back": 0.18, "tail_hook": -0.10, "tail_r0": 0.062, "tail_r1": 0.038,
    # legs: where they hang from, how thick, how high the hip is
    "front_x": 0.11, "front_y": -0.16, "back_x": 0.12, "back_y": 0.25, "hip_z": 0.42, "front_t": 0.075, "back_t": 0.095,
    # the logo's pose: each leg's swing (x) and splay (z) at the hip, its knee; head turn and nod; tail bend
    "FL_x": 0.0, "FL_z": 0.0, "FL_k": 0.0, "FR_x": 0.0, "FR_z": 0.0, "FR_k": 0.0,
    "BL_x": 0.0, "BL_z": 0.0, "BL_k": 0.0, "BR_x": 0.0, "BR_z": 0.0, "BR_k": 0.0,
    "head_yaw": 0.0, "head_nod": 0.0, "tail_bend": 0.0,
    # the angle the logo sees it from (fit.py): turn about Z, tilt down, orthographic size, frame offsets
    "cam_yaw": 0.6, "cam_pitch": 0.2, "cam_scale": 1.4, "cam_dx": 0.0, "cam_dy": 0.0,
    # the body's front: how much higher and wider than its back it is
    "chest": 0.12,
    # the fur's one surface: its voxel (metres) and how far its creases are rounded (voxel steps)
    "voxel": 0.007, "fillet": 8, "keep": 0.25,
}
_pj = os.path.join(HERE, "params.json")
if os.path.exists(_pj):
    P.update(json.load(open(_pj)))


def material(name, color, rough, emit=0.0):
    m = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = next(n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    bsdf.inputs["Base Color"].default_value = (*color, 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    if emit > 0:
        bsdf.inputs["Emission Color"].default_value = (*color, 1.0)
        bsdf.inputs["Emission Strength"].default_value = emit
    m.diffuse_color = (*color, 1.0)  # what the fit's flat render sees
    return m


def srgb(h):
    c = [int(h[i : i + 2], 16) / 255 for i in (0, 2, 4)]
    return tuple(x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c)


def materials():
    fur = material("Fur", srgb("1c1c20"), 0.6)
    inner = material("EarInner", srgb("1c1c20"), 0.7)  # the body's black: the logo is one shape
    eye = material("Eye", srgb("f4f4f0"), 0.25, emit=1.5)
    return [fur, inner, eye]


def seg_dist(p, a, b):
    ab = b - a
    t = max(0.0, min(1.0, (p - a).dot(ab) / max(ab.length_squared, 1e-9)))
    return (p - (a + ab * t)).length


def build(P, scene):
    """The armature and its skinned body from the numbers, in scene; returns (armature, body)."""
    TAIL = []
    for i in range(7):
        u = i / 6
        TAIL.append(Vector((0, P["tail_y"] + P["tail_back"] * math.sin(u * math.pi / 2) + P["tail_hook"] * u ** 3, P["tail_z"] + P["tail_h"] * u)))
    hy, hz = P["head_y"], P["head_z"]
    neck_top = Vector((0, hy + 0.03, hz - 0.08))
    BONES = [
        ("root", (0, 0, 0), (0, 0.2, 0), None),
        ("hips", (0, P["back_y"] + 0.03, P["hip_z"]), (0, P["body_y"] - 0.01, P["hip_z"] + 0.02), "root"),
        ("chest", (0, P["body_y"] - 0.01, P["hip_z"] + 0.02), (0, P["front_y"] - 0.03, P["hip_z"] + 0.06), "hips"),
        ("neck", (0, P["front_y"] - 0.03, P["hip_z"] + 0.06), tuple(neck_top), "chest"),
        ("head", tuple(neck_top), (0, hy + 0.02, hz + P["head_rz"] + 0.05), "neck"),
    ]
    for side, x in (("L", 1), ("R", -1)):
        BONES.append((f"ear.{side}", (P["ear_x"] * x, hy + 0.02, hz + P["ear_z"]), ((P["ear_x"] + P["ear_out"]) * x, hy + 0.04, hz + P["ear_z"] + P["ear_h"]), "head"))
    for side, x in (("L", 1), ("R", -1)):
        ey = eye_y(P, P["eye_x"])
        BONES.append((f"eye.{side}", (P["eye_x"] * x, ey + 0.01, hz + P["eye_z"]), (P["eye_x"] * x, ey - 0.06, hz + P["eye_z"]), "head"))
    for i in range(6):
        BONES.append((f"tail.{i + 1}", tuple(TAIL[i]), tuple(TAIL[i + 1]), "hips" if i == 0 else f"tail.{i}"))
    LEGS = {"FL": (P["front_x"], P["front_y"], "chest"), "FR": (-P["front_x"], P["front_y"], "chest"), "BL": (P["back_x"], P["back_y"], "hips"), "BR": (-P["back_x"], P["back_y"], "hips")}
    knee_z = P["hip_z"] * 0.5
    for k, (x, y, parent) in LEGS.items():
        ky = y + (0.02 if k[0] == "F" else -0.03)
        BONES.append((f"leg.{k}.upper", (x, y, P["hip_z"]), (x, ky, knee_z), parent))
        BONES.append((f"leg.{k}.lower", (x, ky, knee_z), (x, y, 0.035), f"leg.{k}.upper"))

    arm_data = bpy.data.armatures.new("CatRig")
    arm = bpy.data.objects.new("Cat", arm_data)
    scene.collection.objects.link(arm)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    for name, h, t, parent in BONES:
        b = arm_data.edit_bones.new(name)
        b.head, b.tail = Vector(h), Vector(t)
        b.roll = 0
        if parent:
            b.parent = arm_data.edit_bones[parent]
            b.use_connect = (b.head - arm_data.edit_bones[parent].tail).length < 1e-6
    bpy.ops.object.mode_set(mode="OBJECT")
    SEG = {name: (Vector(h), Vector(t)) for name, h, t, _ in BONES}

    bm = bmesh.new()
    deform = bm.verts.layers.deform.verify()
    parts = []  # (verts, bones)

    def add(vs, faces, mat, bones):
        for f in faces:
            f.material_index = mat
            f.smooth = True
        parts.append((vs, bones))

    def ellipsoid(center, radii, mat, bones, segs=64, rings=32, squash=None):
        r = bmesh.ops.create_uvsphere(bm, u_segments=segs, v_segments=rings, radius=1.0)
        vs = r["verts"]
        for v in vs:
            p = Vector((v.co.x * radii[0], v.co.y * radii[1], v.co.z * radii[2]))
            if squash:
                p = squash(p)
            v.co = p + Vector(center)
        add(vs, list({f for v in vs for f in v.link_faces}), mat, bones)

    def tube(points, radii, mat, bones, segs=32):
        """A tube along points, radius per point, closed at both ends."""
        rings = []
        for i, p in enumerate(points):
            d = (points[min(i + 1, len(points) - 1)] - points[max(i - 1, 0)]).normalized()
            side = d.cross(Vector((1, 0, 0)))
            if side.length < 1e-4:
                side = d.cross(Vector((0, 1, 0)))
            side.normalize()
            up = d.cross(side).normalized()
            rings.append([bm.verts.new(p + (side * math.cos(2 * math.pi * s / segs) + up * math.sin(2 * math.pi * s / segs)) * radii[i]) for s in range(segs)])
        faces = []
        for a, b in zip(rings, rings[1:]):
            for s in range(segs):
                faces.append(bm.faces.new((a[s], a[(s + 1) % segs], b[(s + 1) % segs], b[s])))
        # round ends (half a ball each), not points
        def cap(ring, p, d, r):
            prev = ring
            for k in (1, 2, 3):
                a = k * math.pi / 8
                c = p + d * r * math.sin(a)
                cur = [c + (v.co - p) * math.cos(a) for v in ring]
                cur = [bm.verts.new(q) for q in cur]
                for s in range(segs):
                    faces.append(bm.faces.new((prev[s], prev[(s + 1) % segs], cur[(s + 1) % segs], cur[s])))
                extra.extend(cur)
                prev = cur
            tip = bm.verts.new(p + d * r)
            extra.append(tip)
            for s in range(segs):
                faces.append(bm.faces.new((prev[s], prev[(s + 1) % segs], tip)))

        extra = []
        cap(rings[-1], points[-1], (points[-1] - points[-2]).normalized(), radii[-1])
        cap(list(reversed(rings[0])), points[0], (points[0] - points[1]).normalized(), radii[0])
        add([v for r in rings for v in r] + extra, faces, mat, bones)

    def cone(base, tip, radius, top, mat, bones, segs=32, flat=0.55):
        """An ear: a blunt cone flattened front to back, its top `top` of its base wide and flat, as the logo's
        are."""
        axis = (Vector(tip) - Vector(base)).normalized()
        side = axis.cross(Vector((0, 1, 0))).normalized()
        fwd = side.cross(axis).normalized()

        def ring(at, r):
            return [bm.verts.new(Vector(at) + side * math.cos(2 * math.pi * s / segs) * r + fwd * math.sin(2 * math.pi * s / segs) * r * flat) for s in range(segs)]

        lo, hi = ring(base, radius), ring(tip, radius * top)
        t = bm.verts.new(Vector(tip))
        c = bm.verts.new(Vector(base) - axis * radius * 0.4)
        faces = []
        for s in range(segs):
            n = (s + 1) % segs
            faces.append(bm.faces.new((lo[s], lo[n], hi[n], hi[s])))
            faces.append(bm.faces.new((hi[s], hi[n], t)))
            faces.append(bm.faces.new((lo[n], lo[s], c)))
        add(lo + hi + [t, c], faces, mat, bones)

    # body: a bean, the chest a little higher and rounder than the hips
    def bean(p):
        k = max(-1.0, min(1.0, p.y / P["body_ry"]))
        p.z *= 1.0 + P["chest"] * (-k)
        p.x *= 1.0 + P["chest"] * 0.67 * (-k)
        return p

    ellipsoid((0, P["body_y"], P["body_z"]), (P["body_rx"], P["body_ry"], P["body_rz"]), 0, ["hips", "chest", "neck"], squash=bean)

    # head: wider than tall, cheeks out, sunk into the body (no neck, no nose)
    def cheeks(p):
        if p.z < 0:
            p.x *= 1.0 + P["cheek"] * min(1.0, -p.z / 0.12)
        return p

    ellipsoid((0, hy, hz), (P["head_rx"], P["head_ry"], P["head_rz"]), 0, ["neck", "head"], squash=cheeks)
    for side, x in (("L", 1), ("R", -1)):
        cone((P["ear_x"] * x, hy + 0.02, hz + P["ear_z"]), ((P["ear_x"] + P["ear_out"]) * x, hy + 0.04, hz + P["ear_z"] + P["ear_h"]), P["ear_r"], P["ear_top"], 0, ["head", f"ear.{side}"])
        # eyes: tall white slits, as on the logo, on the face
        ellipsoid((P["eye_x"] * x, eye_y(P, P["eye_x"]), hz + P["eye_z"]), (P["eye_w"], 0.018, P["eye_h"]), 2, [f"eye.{side}"], segs=10, rings=8)
    # tail: tapering tube up the back
    pts = []
    for i in range(6):
        for j in range(8):
            pts.append(TAIL[i].lerp(TAIL[i + 1], j / 8))
    pts.append(TAIL[-1])
    tube(pts, [P["tail_r0"] + (P["tail_r1"] - P["tail_r0"]) * i / (len(pts) - 1) for i in range(len(pts))], 0, [f"tail.{i + 1}" for i in range(6)] + ["hips"])
    # legs: a thigh blended into the body, then a shin to the floor, no paws
    for k, (x, y, parent) in LEGS.items():
        up, lo = f"leg.{k}.upper", f"leg.{k}.lower"
        knee = SEG[up][1]
        top = Vector((x * 0.6, y + (0.03 if k[0] == "F" else -0.02), P["hip_z"] + 0.05))
        foot = Vector((x, y, 0.045))
        t = P["back_t"] if k[0] == "B" else P["front_t"]
        ctl, rad = [top, top.lerp(knee, 0.5), knee, knee.lerp(foot, 0.5), foot], [t * 1.05, t, t * 0.85, t * 0.82, t * 0.8]
        # rings every centimetre or so: the fur's surface keeps whatever facets its parts have
        pts, rs = [], []
        for i in range(4):
            for j in range(6):
                pts.append(ctl[i].lerp(ctl[i + 1], j / 6))
                rs.append(rad[i] + (rad[i + 1] - rad[i]) * j / 6)
        tube(pts + [foot], rs + [rad[-1]], 0, [parent, up, lo])

    # the fur is one surface: the parts' union as a distance field, its creases filleted, meshed once (overlapping
    # shells left as they are show every seam). The eyes stay their own shapes on it.
    eye_parts = [i for i, (vs, _) in enumerate(parts) if any(f.material_index == 2 for f in vs[0].link_faces)]
    fur_bm = bm.copy()
    bmesh.ops.delete(fur_bm, geom=[v for v in fur_bm.verts if any(f.material_index == 2 for f in v.link_faces)], context="VERTS")
    fur = bpy.data.objects.new("CatFur", bpy.data.meshes.new("CatFur"))
    fur_bm.to_mesh(fur.data)
    fur_bm.free()
    scene.collection.objects.link(fur)
    ng = bpy.data.node_groups.new("Blend", "GeometryNodeTree")
    ng.interface.new_socket("Geometry", in_out="INPUT", socket_type="NodeSocketGeometry")
    ng.interface.new_socket("Geometry", in_out="OUTPUT", socket_type="NodeSocketGeometry")
    gi, go = ng.nodes.new("NodeGroupInput"), ng.nodes.new("NodeGroupOutput")
    sdf, fil, out, smooth = (ng.nodes.new(t) for t in ("GeometryNodeMeshToSDFGrid", "GeometryNodeSDFGridFillet", "GeometryNodeGridToMesh", "GeometryNodeSetShadeSmooth"))
    sdf.inputs["Voxel Size"].default_value = P["voxel"]
    sdf.inputs["Band Width"].default_value = 3 + P["fillet"]
    fil.inputs["Iterations"].default_value = P["fillet"]
    out.inputs["Threshold"].default_value = 0.0
    for a, b in ((gi.outputs[0], sdf.inputs["Mesh"]), (sdf.outputs[0], fil.inputs["Grid"]), (fil.outputs[0], out.inputs["Grid"]), (out.outputs[0], smooth.inputs["Mesh"]), (smooth.outputs[0], go.inputs[0])):
        ng.links.new(a, b)
    fur.modifiers.new("Blend", "NODES").node_group = ng
    fur.modifiers.new("Fewer", "DECIMATE").ratio = P["keep"]
    blended = bpy.data.meshes.new_from_object(fur.evaluated_get(bpy.context.evaluated_depsgraph_get()))
    bpy.data.objects.remove(fur)
    bpy.data.node_groups.remove(ng)

    # each part's surface, sampled (its corners and its faces' middles), to find which parts a vertex of the blend is near
    from mathutils.kdtree import KDTree

    samples = []
    for i, (vs, _) in enumerate(parts):
        if i in eye_parts:
            continue
        samples += [(v.co.copy(), i) for v in vs]
        samples += [(f.calc_center_median(), i) for f in {f for v in vs for f in v.link_faces}]
    kd = KDTree(len(samples))
    for j, (co, _) in enumerate(samples):
        kd.insert(co, j)
    kd.balance()

    out_bm = bmesh.new()
    out_bm.from_mesh(blended)
    bpy.data.meshes.remove(blended)
    # the fur is the one big piece: a crumb the field meshed on its own (where two parts nearly touch) goes
    islands, seen = [], set()
    for v in out_bm.verts:
        if v in seen:
            continue
        seen.add(v)
        isl, todo = [], [v]
        while todo:
            u = todo.pop()
            isl.append(u)
            for e in u.link_edges:
                w = e.other_vert(u)
                if w not in seen:
                    seen.add(w)
                    todo.append(w)
        islands.append(isl)
    islands.sort(key=len)
    bmesh.ops.delete(out_bm, geom=[v for isl in islands[:-1] for v in isl], context="VERTS")
    for f in out_bm.faces:
        f.material_index = 0
        f.smooth = True
    deform2 = out_bm.verts.layers.deform.verify()
    names = [b[0] for b in BONES]

    def skin(v, bones, layer):
        ws = sorted(((b, 1.0 / (seg_dist(v.co, *SEG[b]) + 0.015) ** 4) for b in bones), key=lambda w: -w[1])[:4]
        s = sum(w for _, w in ws)
        for b, w in ws:
            if w / s > 0.01:
                v[layer][names.index(b)] = w / s

    # a vertex of the blend takes the bones of every part within a few centimetres of its nearest, so a fillet between
    # two parts bends with both
    for v in out_bm.verts:
        near = kd.find_n(v.co, 24)
        d0 = near[0][2]
        bones = []
        for _, j, d in near:
            if d < d0 + 0.04:
                for b in parts[samples[j][1]][1]:
                    if b not in bones:
                        bones.append(b)
        skin(v, bones, deform2)
    # the eyes, as they were, each to its own bone
    for i in eye_parts:
        vs, bones = parts[i]
        new = {v: out_bm.verts.new(v.co) for v in vs}
        for f in {f for v in vs for f in v.link_faces}:
            g = out_bm.faces.new([new[v] for v in f.verts])
            g.material_index, g.smooth = 2, True
        for v in new.values():
            skin(v, bones, deform2)
    bm.free()
    bm = out_bm

    mesh = bpy.data.meshes.new("CatBody")
    bm.to_mesh(mesh)
    bm.free()
    for m in materials():
        mesh.materials.append(m)
    body = bpy.data.objects.new("CatBody", mesh)
    scene.collection.objects.link(body)
    for n in names:
        body.vertex_groups.new(name=n)
    body.parent = arm
    body.modifiers.new("Armature", "ARMATURE").object = arm
    for p in arm.pose.bones:
        p.rotation_mode = "XYZ"
    return arm, body


def eye_y(P, x):
    """Where the face is, x across and eye_z up from the head's centre: the eyes sit on it."""
    u = 1 - (x / P["head_rx"]) ** 2 - (P["eye_z"] / P["head_rz"]) ** 2
    return P["head_y"] - P["head_ry"] * math.sqrt(max(0.05, u)) + 0.006


def logo_pose(arm, P):
    """The pose the logo draws the cat in."""
    pb = arm.pose.bones
    for k in ("FL", "FR", "BL", "BR"):
        pb[f"leg.{k}.upper"].rotation_euler = (P[f"{k}_x"], 0, P[f"{k}_z"])
        pb[f"leg.{k}.lower"].rotation_euler = (P[f"{k}_k"], 0, 0)
    pb["head"].rotation_euler = (P["head_nod"], P["head_yaw"], 0)
    for i in range(6):
        pb[f"tail.{i + 1}"].rotation_euler = (P["tail_bend"], 0, 0)


if __name__ == "__main__":
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.fps = FPS
    arm, body = build(P, scene)

    # ---------------------------------------------------------------- clips


    def pb(n):
        return arm.pose.bones[n]


    def rest():
        for p in arm.pose.bones:
            p.location = (0, 0, 0)
            p.rotation_euler = (0, 0, 0)
            p.scale = (1, 1, 1)


    def clip(name, seconds, pose, every=1):
        """An action keyed every `every` frames from pose(t in seconds), on every bone."""
        act = bpy.data.actions.new(name)
        act.use_fake_user = True
        arm.animation_data_create()
        arm.animation_data.action = act
        n = round(seconds * FPS)
        for f in list(range(0, n, every)) + [n]:
            rest()
            pose(f / FPS)
            for p in arm.pose.bones:
                p.keyframe_insert("location", frame=f + 1)
                p.keyframe_insert("rotation_euler", frame=f + 1)
                p.keyframe_insert("scale", frame=f + 1)
        track = arm.animation_data.nla_tracks.new()
        track.name = name
        track.strips.new(name, 1, act)
        arm.animation_data.action = None
        return act


    TAU = 2 * math.pi


    # Bone axes: a bone's local Y runs head to tail; X is to its side. For the spine bones (pointing forward, -Y), a
    # positive X rotation tips the tail end down; for the legs (pointing down), a positive X rotation swings the foot back.


    def tail_wave(t, amp=0.18, speed=1.0, lift=0.0):
        for i in range(6):
            ph = t * TAU * speed - i * 0.55
            pb(f"tail.{i + 1}").rotation_euler = (lift * (i + 1) / 6, 0, amp * math.sin(ph) * (0.5 + i / 6))


    def blink(t, at):
        d = abs(t - at)
        s = 1.0 if d > 0.09 else max(0.08, d / 0.09)
        for e in ("eye.L", "eye.R"):
            # an eye bone points forward (local Y = -Y world); local X is world X, local Z is world Z: squash along Z
            pb(e).scale = (1, 1, s)


    def idle(t):
        br = math.sin(t * TAU / 1.5)
        pb("chest").scale = (1 + 0.02 * br, 1, 1 + 0.025 * br)
        pb("neck").rotation_euler = (0.03 * br, 0, 0)
        pb("head").rotation_euler = (0.04 * math.sin(t * TAU / 3), 0, 0.06 * math.sin(t * TAU / 3 + 1))
        tail_wave(t, 0.16, 1 / 1.5)
        blink(t, 1.9)
        # one ear flicks
        fl = max(0.0, 1 - abs(t - 0.8) / 0.12)
        pb("ear.L").rotation_euler = (0, -0.35 * fl, 0)


    def walk(t, period=1.0, stride=0.45, bob=0.02):
        ph = t / period * TAU
        # diagonal pairs: FL with BR, FR with BL
        for k, off in (("FL", 0), ("BR", 0.08), ("FR", math.pi), ("BL", math.pi + 0.08)):
            s = math.sin(ph + off)
            lift = max(0.0, math.cos(ph + off))
            pb(f"leg.{k}.upper").rotation_euler = (stride * s, 0, 0)
            knee = (0.7 if k[0] == "F" else -0.7) * lift
            pb(f"leg.{k}.lower").rotation_euler = (knee - stride * s * 0.3, 0, 0)
        pb("root").location = (0, 0, 0)
        pb("hips").location = (0, bob * math.cos(2 * ph), 0)  # hips' local Y is the body's length; bob via chest below
        pb("hips").location = (0, 0, 0)
        pb("hips").rotation_euler = (0.03 * math.cos(2 * ph), 0, 0.04 * math.sin(ph))
        pb("chest").rotation_euler = (0, 0, -0.05 * math.sin(ph))
        pb("head").rotation_euler = (0.04 * math.cos(2 * ph), 0, 0.05 * math.sin(ph))
        tail_wave(t, 0.12, 1 / period, 0.05)


    def walk_bob(t, period, amp):
        # the whole cat rises a little at each footfall: root in world Z (root's local Y is world +Y, local Z is world -Y?)
        # root points along +Y with roll 0: local X = world X, local Y = world Y, local Z = world Z
        pb("root").location = (0, 0, amp * abs(math.cos(t / period * TAU)))


    def walk_clip(t):
        walk(t)
        walk_bob(t, 1.0, 0.012)


    def trot_clip(t):
        walk(t, 0.6, 0.6, 0.03)
        walk_bob(t, 0.6, 0.02)


    SIT = 0.7  # how far the hips tip back


    def sit(t):
        # body pitched up about the hips: tail end of hips goes up (negative X on a forward-pointing bone lifts its tail)
        pb("root").location = (0, 0.0, -0.13)
        pb("hips").rotation_euler = (-SIT, 0, 0)
        pb("chest").rotation_euler = (-0.25, 0, 0)
        pb("neck").rotation_euler = (SIT * 0.45, 0, 0)
        pb("head").rotation_euler = (0.35 + 0.03 * math.sin(t * TAU / 3), 0, 0.05 * math.sin(t * TAU / 3))
        # front legs straight down again (undo the body's pitch), back legs folded forward under the haunch
        for k in ("FL", "FR"):
            pb(f"leg.{k}.upper").rotation_euler = (SIT + 0.25 - 0.05, 0, 0)
            pb(f"leg.{k}.lower").rotation_euler = (0.05, 0, 0)
        for k in ("BL", "BR"):
            pb(f"leg.{k}.upper").rotation_euler = (SIT - 1.45, 0, 0)
            pb(f"leg.{k}.lower").rotation_euler = (1.9, 0, 0)
        # tail down along the ground and curled round the side
        for i in range(6):
            pb(f"tail.{i + 1}").rotation_euler = (0.55 if i < 2 else 0.1, 0, 0.35 + 0.05 * math.sin(t * TAU / 3 - i * 0.5))
        br = math.sin(t * TAU / 1.5)
        pb("chest").scale = (1 + 0.02 * br, 1, 1 + 0.025 * br)
        blink(t, 2.2)


    def look_around(t):
        idle(t)
        # left, hold, right, hold, back
        keys = [(0, 0), (0.5, 0.6), (1.4, 0.6), (2.0, -0.6), (2.9, -0.6), (3.5, 0), (4.0, 0)]
        for (t0, a0), (t1, a1) in zip(keys, keys[1:]):
            if t0 <= t <= t1:
                u = (t - t0) / (t1 - t0)
                u = u * u * (3 - 2 * u)
                yaw = a0 + (a1 - a0) * u
        # yaw about world Z: the head bone points up (local Y = world Z), so its local Y rotation is a turn
        pb("head").rotation_euler = (0.05, yaw, 0)
        pb("neck").rotation_euler = (0, yaw * 0.35, 0)
        blink(t, 1.45)


    def pounce(t):
        # crouch (0-0.6), leap (0.6-1.1), land (1.1-1.6)
        def ease(a, b, x):
            u = max(0.0, min(1.0, (x - a) / (b - a)))
            return u * u * (3 - 2 * u)

        crouch = ease(0.0, 0.55, t) * (1 - ease(0.6, 0.7, t)) + ease(1.15, 1.25, t) * (1 - ease(1.3, 1.6, t)) * 0.6
        air = math.sin(math.pi * max(0.0, min(1.0, (t - 0.6) / 0.55))) if 0.6 <= t <= 1.15 else 0.0
        fwd = ease(0.6, 1.15, t) * 0.6
        pb("root").location = (0, -fwd, -0.1 * crouch + 0.45 * air)
        pb("hips").rotation_euler = (-0.15 * crouch + 0.25 * air * (1 if t < 0.85 else -1), 0, 0)
        for k in ("FL", "FR"):
            pb(f"leg.{k}.upper").rotation_euler = (-0.5 * crouch - 0.9 * air, 0, 0)
            pb(f"leg.{k}.lower").rotation_euler = (0.9 * crouch + 0.2 * air, 0, 0)
        for k in ("BL", "BR"):
            pb(f"leg.{k}.upper").rotation_euler = (0.6 * crouch + 0.9 * air, 0, 0)
            pb(f"leg.{k}.lower").rotation_euler = (-1.0 * crouch - 0.3 * air, 0, 0)
        pb("head").rotation_euler = (0.25 * crouch, 0, 0)
        for e in ("ear.L", "ear.R"):
            pb(e).rotation_euler = (0.5 * crouch, 0, 0)
        # the tail lashes while crouched, streams behind in the air
        for i in range(6):
            pb(f"tail.{i + 1}").rotation_euler = (0.5 * air, 0, 0.25 * crouch * math.sin(t * TAU * 3 - i * 0.6))


    def blink_only(t):
        blink(t, 0.2)


    clip("Logo", 0.1, lambda t: logo_pose(arm, P))
    clip("Idle", 3.0, idle)
    clip("Walk", 1.0, walk_clip)
    clip("Trot", 0.6, trot_clip)
    clip("Sit", 3.0, sit)
    clip("LookAround", 4.0, look_around)
    clip("Pounce", 1.6, pounce)
    clip("Blink", 0.4, blink_only)

    # the cinematic (story.py): the whole 30 s performance as one clip, its root moving across the floor
    import sys

    sys.path.insert(0, HERE)
    import story

    L1 = P["hip_z"] - P["hip_z"] * 0.5  # hip to knee
    L2 = P["hip_z"] * 0.5 - 0.035  # knee to foot

    def leg_ik(span, back, knee_forward):
        """A two-bone leg in the body's plane reaching a foot `span` below its hip and `back` behind it: the upper
        and lower bones' X rotations (+ swings the foot back)."""
        r = max(0.3 * (L1 + L2), min((L1 + L2) * 0.999, math.hypot(span, back)))
        hip = math.acos(max(-1.0, min(1.0, (L1 * L1 + r * r - L2 * L2) / (2 * L1 * r))))
        bend = math.pi - math.acos(max(-1.0, min(1.0, (L1 * L1 + L2 * L2 - r * r) / (2 * L1 * L2))))
        phi = math.atan2(back, span)
        return (phi - hip, bend) if knee_forward else (phi + hip, -bend)

    def posed(name):
        """A bone's posed matrix in the armature's space, from its rest and the pose set so far (Blender's own sum:
        parent's posed matrix, the rest offset from the parent, then location, rotation, scale)."""
        b, p = arm.data.bones[name], pb(name)
        basis = Matrix.Translation(p.location) @ p.rotation_euler.to_matrix().to_4x4() @ Matrix.Diagonal((*p.scale, 1.0))
        if b.parent is None:
            return b.matrix_local @ basis
        return posed(b.parent.name) @ (b.parent.matrix_local.inverted() @ b.matrix_local) @ basis

    AIMED = {}

    def aim(name, d):
        """Point a bone along d (armature space), its length set so it reaches |d| whatever its parents' squash: the
        rotation that turns its rest Y onto d, the least twist, as XYZ Euler; its Y scale to the length."""
        b = arm.data.bones[name]
        base = posed(b.parent.name) @ (b.parent.matrix_local.inverted() @ b.matrix_local)
        local = base.to_3x3().inverted() @ d
        q = Vector((0.0, 1.0, 0.0)).rotation_difference(local.normalized())
        # the Euler nearest last frame's, so keys never flip to an equal angle the long way round
        e = q.to_euler("XYZ", AIMED[name]) if name in AIMED else q.to_euler("XYZ")
        AIMED[name] = e.copy()
        pb(name).rotation_euler = e
        pb(name).scale = (1.0, local.length / b.length, 1.0)

    def leg_to(k, foot, pole):
        """Two-bone IK: leg k's foot to `foot` (armature space), the knee bent toward `pole`."""
        up, lo = f"leg.{k}.upper", f"leg.{k}.lower"
        pb(up).rotation_euler = (0, 0, 0)
        pb(up).scale = pb(lo).scale = (1, 1, 1)
        pb(lo).rotation_euler = (0, 0, 0)
        hip = posed(up).to_translation()
        l1, l2 = arm.data.bones[up].length, arm.data.bones[lo].length
        d = foot - hip
        # soft IK: past 90% of the leg's length the reach eases toward full instead of hitting it (a hard limit pops
        # the knee straight, since the knee's height changes ever faster as the leg nears full length)
        L, soft = l1 + l2, 0.9 * (l1 + l2)
        r = d.length
        if r > soft:
            span = 0.995 * L - soft
            r = soft + span * (1 - math.exp(-(r - soft) / span))
        r = max(0.35 * L, r)
        u = d.normalized()
        a = (l1 * l1 - l2 * l2 + r * r) / (2 * r)
        hgt = math.sqrt(max(0.0, l1 * l1 - a * a))
        side = (pole - u * pole.dot(u)).normalized()
        knee = hip + u * a + side * hgt
        aim(up, knee - hip)
        aim(lo, hip + u * r - knee)

    def cinematic(t):
        c = story.perform(t)
        # root: the floor path (raea's x, y, z is Blender's x, -z, y), turned to its heading; lowered by the crouch
        pb("root").location = (c["x"], -c["z"], c["y"] - c["crouch"])
        pb("root").rotation_euler = (0, 0, c["yaw"])
        # squash and stretch, the volume kept: taller and thinner stretched, flatter and wider squashed (local Z is
        # the body's up and down, Y its length)
        s_ = c["stretch"]
        v, w = 1 + s_, 1 / math.sqrt(1 + s_)
        pb("hips").rotation_euler = (c["pitch"], 0, c["wiggle"] * 2.5 - c["spine"] * 0.4)
        pb("hips").scale = (w, 1 + s_ * 0.5, v)
        # the chest leads the turn; it breathes
        pb("chest").rotation_euler = (0, 0, c["spine"])
        pb("chest").scale = (w * (1 + 0.02 * c["breath"]), 1, v * (1 + 0.025 * c["breath"]))
        # legs: each foot where the story puts it in the world (planted, stepping, or in the air), reached by IK;
        # elbows bend back, knees forward
        fwd = (posed("hips").to_3x3() @ Vector((0.0, 1.0, 0.0))).normalized()
        for k, (fx, fy, fz) in c["feet"].items():
            leg_to(k, Vector((fx, -fz, fy + 0.035)), -fwd if k[0] == "F" else fwd)
        # head and neck: the neck takes a third of a turn
        pb("neck").rotation_euler = (0, c["head_yaw"] * 0.3, 0)
        pb("head").rotation_euler = (c["head_pitch"], c["head_yaw"] * 0.7, c["head_roll"])
        for e, sgn in (("ear.L", 1), ("ear.R", -1)):
            pb(e).rotation_euler = (c["ears"] + (c["ear_flick"] if sgn > 0 else 0.0), 0, 0)
        for e in ("eye.L", "eye.R"):
            w = 1 + 0.18 * c["wide"]
            pb(e).scale = (w, 1, max(0.08, 1 - c["blink"]) * w)
        # tail: each segment a little behind the one before it (overlap), dragged by the body's moves
        for i in range(6):
            lag = story.perform(max(0.0, t - 0.07 * i))
            curl = c["tail_lift"] * (-0.05 if i < 4 else -0.35)
            pb(f"tail.{i + 1}").rotation_euler = (lag["tail_bend"] * 0.25 + curl, 0, lag["tail_sway"] * (0.45 + i / 6))

    clip("Cinematic", story.DUR, cinematic)
    rest()

    def export(path):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        bpy.ops.export_scene.gltf(
            filepath=path,
            export_format="GLB",
            export_animation_mode="NLA_TRACKS",
            export_anim_single_armature=True,
            export_force_sampling=True,
            export_skins=True,
            export_yup=True,
            export_apply=False,
        )
        print("wrote", path, os.path.getsize(path), "bytes")

    export(OUT)
    # the white cat (the logo on void's black): white fur, eyes black
    for name, hexc in (("Fur", "f2f2ee"), ("EarInner", "f2f2ee"), ("Eye", "0c0c0e")):
        m = bpy.data.materials[name]
        bsdf = next(n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
        bsdf.inputs["Base Color"].default_value = (*srgb(hexc), 1.0)
        bsdf.inputs["Emission Strength"].default_value = 0.0
    export(os.path.join(HERE, "media", "cat-white.glb"))
