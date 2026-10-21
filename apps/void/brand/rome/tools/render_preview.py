"""Render one PNG tile per .glb (3/4 front view, framed to its bounds).
blender -b --factory-startup -P render_preview.py -- OUT_DIR SIZE a.glb [b.glb ...]"""
import bpy, sys, os, math
from mathutils import Vector

argv = sys.argv[sys.argv.index("--") + 1:]
out_dir, size, files = argv[0], int(argv[1]), argv[2:]
os.makedirs(out_dir, exist_ok=True)

for f in files:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    sc = bpy.context.scene
    try:
        sc.render.engine = "BLENDER_EEVEE"
    except TypeError:
        sc.render.engine = "BLENDER_EEVEE_NEXT"
    sc.render.resolution_x = sc.render.resolution_y = size
    sc.render.film_transparent = False
    sc.view_settings.view_transform = "AgX" if "AgX" in [i.identifier for i in sc.view_settings.bl_rna.properties["view_transform"].enum_items] else "Standard"
    world = bpy.data.worlds.new("w"); sc.world = world; world.use_nodes = True
    bg = next(n for n in world.node_tree.nodes if n.type == "BACKGROUND")
    bg.inputs[0].default_value = (0.32, 0.31, 0.30, 1); bg.inputs[1].default_value = 0.9

    bpy.ops.import_scene.gltf(filepath=f)
    meshes = [o for o in sc.objects if o.type == "MESH"]
    lo = Vector((1e18,) * 3); hi = Vector((-1e18,) * 3)
    for o in meshes:
        for c in o.bound_box:
            p = o.matrix_world @ Vector(c)
            lo = Vector(map(min, lo, p)); hi = Vector(map(max, hi, p))
    centre = (lo + hi) / 2
    radius = (hi - lo).length / 2

    cam_data = bpy.data.cameras.new("cam"); cam_data.lens = 50
    cam = bpy.data.objects.new("cam", cam_data); sc.collection.objects.link(cam); sc.camera = cam
    d = Vector((0.55, -1.0, 0.35)).normalized()
    fov = 2 * math.atan(18 / cam_data.lens)
    dist = radius / math.sin(fov / 2) * 1.02
    cam.location = centre + d * dist
    cam.rotation_euler = (-d).to_track_quat("-Z", "Y").to_euler()
    cam_data.clip_start = dist / 1000; cam_data.clip_end = dist * 10

    for rot, energy in (((math.radians(50), 0, math.radians(30)), 3.5), ((math.radians(70), 0, math.radians(-120)), 1.2)):
        ld = bpy.data.lights.new("sun", "SUN"); ld.energy = energy
        lo_ = bpy.data.objects.new("sun", ld); lo_.rotation_euler = rot; sc.collection.objects.link(lo_)

    name = os.path.splitext(os.path.basename(f))[0]
    sc.render.filepath = os.path.join(out_dir, name + ".png")
    bpy.ops.render.render(write_still=True)
    print("TILE", name)
