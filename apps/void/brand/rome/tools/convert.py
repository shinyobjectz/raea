"""Convert one downloaded model into a clean single .glb for raea.

blender -b --factory-startup -P convert.py -- SRC OUT.glb [--height M | --length M | --keep-scale] [--rot X,Y,Z] [--marble] [--max-tris N] [--survey]

- imports SRC (.glb/.gltf/.stl/.obj) into an empty scene
- optional extra rotation in degrees (fix sideways scans), applied before measuring
- joins nothing, but applies every transform so the exported nodes are clean
- decimates (collapse) so the whole asset is <= max-tris triangles (default 150k)
- downsizes every image to <= 2048 px on its longest side
- scales so the bounding-box height (Z) or longest horizontal side equals the given metres
- centres X/Y on the origin and puts the lowest point on the floor (Z=0, glTF Y=0)
- --marble gives untextured scans (STL) a plain off-white marble-ish material
- exports GLB, +Y up, textures embedded (JPEG unless an image needs alpha)
--survey only prints triangle count, bbox and image sizes after import (no export).
"""
import bpy, sys, math, json, os
from mathutils import Vector, Matrix

argv = sys.argv[sys.argv.index("--") + 1:]
src, out = argv[0], argv[1]
opts = argv[2:]
def opt(name, default=None, cast=str):
    if name in opts:
        return cast(opts[opts.index(name) + 1])
    return default
height = opt("--height", None, float)
length = opt("--length", None, float)
rot = opt("--rot", None, lambda s: [float(v) for v in s.split(",")])
max_tris = opt("--max-tris", 150000, int)
marble = "--marble" in opts
survey = "--survey" in opts

bpy.ops.wm.read_factory_settings(use_empty=True)
ext = os.path.splitext(src)[1].lower()
if ext in (".glb", ".gltf"):
    bpy.ops.import_scene.gltf(filepath=src)
elif ext == ".stl":
    bpy.ops.wm.stl_import(filepath=src)
elif ext == ".obj":
    bpy.ops.wm.obj_import(filepath=src)
else:
    raise SystemExit(f"unknown type {ext}")

scene = bpy.context.scene
meshes = [o for o in scene.objects if o.type == "MESH"]
# Drop animation/armatures only if none are skinned (we want static props here); keep skinned rigs intact.
skinned = any(any(m.type == "ARMATURE" for m in o.modifiers) for o in meshes)

def tri_count(objs):
    dg = bpy.context.evaluated_depsgraph_get()
    n = 0
    for o in objs:
        me = o.evaluated_get(dg).to_mesh()
        me.calc_loop_triangles()
        n += len(me.loop_triangles)
        o.evaluated_get(dg).to_mesh_clear()
    return n

def world_bbox(objs):
    lo = Vector((1e18,) * 3); hi = Vector((-1e18,) * 3)
    dg = bpy.context.evaluated_depsgraph_get()
    for o in objs:
        oe = o.evaluated_get(dg)
        me = oe.to_mesh()
        mw = o.matrix_world
        for v in me.vertices:
            p = mw @ v.co
            lo = Vector(map(min, lo, p)); hi = Vector(map(max, hi, p))
        oe.to_mesh_clear()
    return lo, hi

if survey:
    lo, hi = world_bbox(meshes)
    imgs = [(i.name, tuple(i.size)) for i in bpy.data.images]
    print("SURVEY", json.dumps(dict(src=src, objects=len(meshes), tris=tri_count(meshes), skinned=skinned,
          bbox_min=list(lo), bbox_max=list(hi), size=list(hi - lo), images=imgs,
          materials=len(bpy.data.materials))))
    raise SystemExit(0)

# 1. apply all transforms: un-parent every mesh keeping its world transform, then bake it into the mesh
if not skinned:
    for o in meshes:
        mw = o.matrix_world.copy()
        o.parent = None
        o.matrix_world = mw
    for o in list(scene.objects):
        if o.type != "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    meshes = [o for o in scene.objects if o.type == "MESH"]
    for o in meshes:
        if o.data.users > 1:
            o.data = o.data.copy()
        o.data.transform(o.matrix_world)
        o.matrix_world = Matrix.Identity(4)
    if rot:
        R = (Matrix.Rotation(math.radians(rot[2]), 4, "Z") @ Matrix.Rotation(math.radians(rot[1]), 4, "Y")
             @ Matrix.Rotation(math.radians(rot[0]), 4, "X"))
        for o in meshes:
            o.data.transform(R)

# 2. decimate to the triangle budget
tris = tri_count(meshes)
tris_before = tris
if tris > max_tris and not skinned:
    ratio = max_tris / tris * 0.97
    for o in meshes:
        bpy.context.view_layer.objects.active = o
        m = o.modifiers.new("dec", "DECIMATE")
        m.decimate_type = "COLLAPSE"
        m.ratio = ratio
        m.use_collapse_triangulate = True
        with bpy.context.temp_override(object=o, active_object=o):
            bpy.ops.object.modifier_apply(modifier=m.name)
    tris = tri_count(meshes)

# 3. downsize images
for img in bpy.data.images:
    w, h = img.size
    if max(w, h) > 2048:
        s = 2048 / max(w, h)
        img.scale(max(1, int(w * s)), max(1, int(h * s)))

# 4. marble for untextured scans
if marble:
    mat = bpy.data.materials.new("marble")
    mat.use_nodes = True
    bsdf = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    bsdf.inputs["Base Color"].default_value = (0.86, 0.84, 0.80, 1.0)
    bsdf.inputs["Roughness"].default_value = 0.42
    for o in meshes:
        o.data.materials.clear()
        o.data.materials.append(mat)
        for p in o.data.polygons:
            p.use_smooth = True

# 5. scale + centre on the floor (Blender Z-up; glTF export makes it Y-up)
lo, hi = world_bbox(meshes)
size = hi - lo
s = 1.0
if height:
    s = height / size.z
elif length:
    s = length / max(size.x, size.y)
T = Matrix.Scale(s, 4) @ Matrix.Translation(Vector((-(lo.x + hi.x) / 2, -(lo.y + hi.y) / 2, -lo.z)))
if not skinned:
    for o in meshes:
        o.data.transform(T)
lo, hi = world_bbox(meshes)

# 6. export
needs_alpha = False
for m in bpy.data.materials:
    if m.node_tree:
        for n in m.node_tree.nodes:
            if n.type == "BSDF_PRINCIPLED" and n.inputs["Alpha"].is_linked:
                needs_alpha = True
os.makedirs(os.path.dirname(out), exist_ok=True)
bpy.ops.export_scene.gltf(filepath=out, export_format="GLB", export_yup=True, export_apply=True,
                          export_image_format="AUTO" if needs_alpha else "JPEG", export_jpeg_quality=88,
                          export_animations=skinned, use_selection=False)
print("RESULT", json.dumps(dict(out=out, tris_before=tris_before, tris=tris, scale=s, size_m=list(hi - lo),
                                bytes=os.path.getsize(out), images=[(i.name, tuple(i.size)) for i in bpy.data.images])))
