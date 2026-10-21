"""Fit void's 3D cat to the logo: its proportions, the logo's pose and the angle the logo sees it from (build.py's P),
chosen so the cat's silhouette and eyes, rendered flat from that angle, cover the logo's 32 x 32 pixels
(apps/site/src/components/brand/cat-path.ts) as closely as they can.

    blender -b -P apps/void/brand/cat3d/fit.py -- [EVALS] [OUTDIR] [--again]

It carries on from params.json when there is one (--again starts over from build.py's numbers). FIT_ROUNDS
sets the rounds (6), FIT_KEYS=ear,head_ fits only the numbers named so, FIT_EARS=1 adds the ears' strip of the logo
(rows 0-6) to the score at that weight.

Each try rebuilds the cat from its numbers, poses it, and renders it from the camera at 4x the logo's grid with
Workbench (flat: black body, white eyes, white ground); the score is the overlap (intersection over union) of the
body with the logo's body, and of the eyes with its eyes. A (1+1) evolution strategy with a step per number and the
1/5th success rule moves the numbers; the camera first on its own, then everything. Writes params.json beside this
file (build.py reads it) and, in OUTDIR, overlay.png: the logo's pixels over the fitted render (black both, red the
cat only, blue the logo only, white/green eyes).
"""

import importlib.util
import json
import math
import os
import random
import re
import sys
import time

import bpy
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("catbuild", os.path.join(HERE, "build.py"))
cb = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cb)

argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
EVALS = int(argv[0]) if argv else 1200
OUTDIR = argv[1] if len(argv) > 1 else HERE
R = 128  # the render: 4 px a logo pixel
ROUNDS = int(os.environ.get("FIT_ROUNDS", 6))
VIEW = (-1.5, 0.0, 32.0)  # the logo's viewBox: x0, y0, size

# ---------------------------------------------------------------- the logo as masks

BODY = "M6 0h3v2h1v4h1v4h1v-4h1v-2h1v-1h3v1h1v2h4v-1h1v-1h3v7h1v7h-1v1h-1v1h1v2h1v2h1v2h1v2h-1v1h-2v-1h-2v-1h-1v-1h-1v-1h-3v1h-3v2h-1v3h-1v1h-3v-1h-1v-6h-1v-1h-2v-1h-1v1h-1v2h-1v2h-1v1h-2v-1h-1v-5h1v-3h1v-4h1v-2h1v-2h1v-1h1z"
EYES = [(18, 11, 1, 3), (23, 11, 1, 3)]  # x y w h


def polygon(d):
    m = re.match(r"M(-?\d+) (-?\d+)", d)
    x, y = int(m.group(1)), int(m.group(2))
    pts = [(x, y)]
    for c, n in re.findall(r"([hv])(-?\d+)", d):
        if c == "h":
            x += int(n)
        else:
            y += int(n)
        pts.append((x, y))
    return pts


def logo_masks():
    s = VIEW[2] / R
    xs = VIEW[0] + (np.arange(R) + 0.5) * s
    ys = VIEW[1] + (np.arange(R) + 0.5) * s
    X, Y = np.meshgrid(xs, ys)
    inside = np.zeros((R, R), bool)
    pts = polygon(BODY)
    for (x1, y1), (x2, y2) in zip(pts, pts[1:] + pts[:1]):
        if y1 == y2:
            continue
        cross = ((y1 > Y) != (y2 > Y)) & (X < x1 + (Y - y1) * (x2 - x1) / (y2 - y1))
        inside ^= cross
    eyes = np.zeros((R, R), bool)
    for x, y, w, h in EYES:
        eyes |= (X >= x) & (X < x + w) & (Y >= y) & (Y < y + h)
    return inside, eyes  # the eyes are holes in the body: cat = body | eyes


LOGO_BODY, LOGO_EYES = logo_masks()
LOGO_CAT = LOGO_BODY | LOGO_EYES

# ---------------------------------------------------------------- the render

scene = bpy.context.scene
bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
scene.render.engine = "BLENDER_WORKBENCH"
scene.display.shading.light = "FLAT"
scene.display.shading.color_type = "MATERIAL"
scene.render.resolution_x = scene.render.resolution_y = R
scene.render.resolution_percentage = 100
scene.render.film_transparent = False
scene.view_settings.view_transform = "Standard"
scene.render.image_settings.file_format = "PNG"
scene.render.use_freestyle = False
world = bpy.data.worlds.new("White")
world.color = (1, 1, 1)
scene.world = world
cam = bpy.data.objects.new("Cam", bpy.data.cameras.new("Cam"))
cam.data.type = "ORTHO"
scene.collection.objects.link(cam)
scene.camera = cam
TMP = os.path.join(OUTDIR, "_fit.png")


def clear():
    for o in list(bpy.data.objects):
        if o is not cam:
            bpy.data.objects.remove(o, do_unlink=True)
    for coll in (bpy.data.meshes, bpy.data.armatures):
        for x in list(coll):
            coll.remove(x)


def render(P):
    clear()
    arm, _ = cb.build(P, scene)
    cb.logo_pose(arm, P)
    # the camera on a sphere round the cat's middle, looking at it
    target = np.array([0.0, 0.0, 0.45])
    yaw, pitch = P["cam_yaw"], P["cam_pitch"]
    d = np.array([math.sin(yaw) * math.cos(pitch), -math.cos(yaw) * math.cos(pitch), math.sin(pitch)])
    cam.location = tuple(target + d * 5)
    from mathutils import Vector

    cam.rotation_euler = (Vector(tuple(-d))).to_track_quat("-Z", "Y").to_euler()
    cam.data.ortho_scale = P["cam_scale"]
    cam.data.shift_x = P["cam_dx"]
    cam.data.shift_y = P["cam_dy"]
    scene.render.filepath = TMP
    bpy.ops.render.render(write_still=True)
    img = bpy.data.images.load(TMP, check_existing=False)
    px = np.array(img.pixels[:], dtype=np.float32).reshape(R, R, 4)[::-1]  # top row first, as the logo
    bpy.data.images.remove(img)
    lum = px[..., :3].mean(-1)
    eyes = lum > 0.5
    # white is the ground or an eye: an eye is white with the cat around it (not reaching the frame's edge)
    cat = lum < 0.5
    from_edge = flood_outside(~cat)
    eyes = eyes & ~from_edge
    return cat | eyes, eyes


def flood_outside(free):
    """The free pixels joined to the frame's edge."""
    out = np.zeros_like(free)
    out[0, :] = free[0, :]
    out[-1, :] = free[-1, :]
    out[:, 0] = free[:, 0]
    out[:, -1] = free[:, -1]
    while True:
        grown = out.copy()
        grown[1:, :] |= out[:-1, :]
        grown[:-1, :] |= out[1:, :]
        grown[:, 1:] |= out[:, :-1]
        grown[:, :-1] |= out[:, 1:]
        grown &= free
        if (grown == out).all():
            return out
        out = grown


def iou(a, b):
    u = (a | b).sum()
    return (a & b).sum() / u if u else 1.0


# the strip above the head's top (logo rows 0-6, less the tail's columns): where the ears are
EARS_ROWS = slice(0, int(6.5 * R / 32))
EARS_COLS = slice(int((11 - VIEW[0]) * R / 32), R)
FOCUS = float(os.environ.get("FIT_EARS", 0))


def score(P):
    cat, eyes = render(P)
    s = iou(cat, LOGO_CAT) + 0.25 * iou(eyes, LOGO_EYES)
    if FOCUS:
        s += FOCUS * iou(cat[EARS_ROWS, EARS_COLS], LOGO_CAT[EARS_ROWS, EARS_COLS])
    return s, cat, eyes


# ---------------------------------------------------------------- the search

# each number's first step (its scale), and its bounds
STEP = {k: 0.02 for k in cb.P}
STEP.update({k: 0.12 for k in cb.P if re.match(r"^(FL|FR|BL|BR)_", k) or k in ("head_yaw", "head_nod", "tail_bend")})
STEP.update({"ear_top": 0.08, "cam_yaw": 0.15, "cam_pitch": 0.08, "cam_scale": 0.06, "cam_dx": 0.03, "cam_dy": 0.03, "cheek": 0.04})


def clamp(P):
    for k, v in P.items():
        if k.startswith(("body_r", "head_r", "ear_h", "ear_r", "eye_w", "eye_h", "tail_r", "front_t", "back_t", "tail_h")):
            P[k] = max(0.01, v)
        if k in ("tail_r1", "tail_r0"):
            P[k] = min(P[k], 0.12)
    # a cat from every other angle too, not only the logo's: the tail as thick at its tip as the logo draws it, ears that
    # stand clear of the head, legs of a kind, the eyes on the face
    P["tail_r1"] = max(P["tail_r1"], 0.8 * P["tail_r0"])
    # ears whose base sits on the head, not sunk in it showing only their tip
    top = P["head_rz"] * math.sqrt(max(0.0, 1 - (P["ear_x"] / max(P["head_rx"], 1e-3)) ** 2))
    P["ear_z"] = max(P["ear_z"], top - 0.05)
    P["ear_h"] = max(P["ear_h"], top + 0.07 - P["ear_z"])
    P["ear_r"] = max(P["ear_r"], 0.095)
    P["ear_top"] = min(max(P["ear_top"], 0.15), 0.9)
    P["front_t"] = max(P["front_t"], 0.75 * P["back_t"])
    P["eye_x"] = min(P["eye_x"], 0.6 * P["head_rx"])
    P["cam_scale"] = max(0.3, P["cam_scale"])
    P["hip_z"] = max(0.2, P["hip_z"])
    return P


def search(P, keys, evals, log):
    P = clamp(dict(P))
    best, cat, eyes = score(P)
    steps = {k: STEP[k] for k in keys}
    wins = 0
    for i in range(evals):
        # a few numbers at a time: with forty-odd, moving them all at once rarely finds a better cat
        Q = dict(P)
        for k in random.sample(keys, min(len(keys), random.choice((1, 1, 2, 3, 4)))):
            Q[k] = P[k] + random.gauss(0, steps[k])
        clamp(Q)
        s, c, e = score(Q)
        if s > best:
            P, best, cat, eyes = Q, s, c, e
            wins += 1
        # 1/5th rule every 20 tries, never below a tenth of the first step
        if (i + 1) % 20 == 0:
            f = 1.22 if wins > 4 else 0.82
            for k in steps:
                steps[k] = max(steps[k] * f, STEP[k] * 0.1)
            wins = 0
            if (i + 1) % 200 == 0:
                log(f"{i + 1:5d} score {best:.4f} (body {iou(cat, LOGO_CAT):.3f}, eyes {iou(eyes, LOGO_EYES):.3f}) step x{f}")
    return P, best, cat, eyes


def overlay(cat, eyes, path):
    img = np.ones((R, R, 3), np.float32)
    img[cat & LOGO_CAT] = 0
    img[cat & ~LOGO_CAT] = (0.9, 0.2, 0.2)
    img[~cat & LOGO_CAT] = (0.2, 0.4, 0.95)
    img[eyes & LOGO_EYES] = (1, 1, 1)
    img[eyes & ~LOGO_EYES] = (0.3, 0.85, 0.3)
    img[LOGO_EYES & ~eyes] = (0.95, 0.75, 0.2)
    out = bpy.data.images.new("overlay", R, R)
    rgba = np.concatenate([img[::-1], np.ones((R, R, 1), np.float32)], -1)
    out.pixels = rgba.ravel()
    out.filepath_raw = path
    out.file_format = "PNG"
    out.save()


if __name__ == "__main__":
    random.seed(7)
    t0 = time.time()
    logf = open(os.path.join(OUTDIR, "fit.log"), "w")

    def log(s):
        line = f"[{time.time() - t0:6.1f}s] {s}"
        print(line, flush=True)
        logf.write(line + "\n")
        logf.flush()

    P = dict(cb.P)
    if EVALS == 0:
        s, cat, eyes = score(P)
        log(f"one try: score {s:.4f}")
        overlay(cat, eyes, os.path.join(OUTDIR, "overlay.png"))
        sys.exit(0)
    # where to start, read off the logo: seen from the cat's side and a little in front, its forward to the right; the
    # head turned to the camera (both eyes show), a front leg reaching forward and a back leg stretched back, chunky
    if os.path.exists(os.path.join(HERE, "params.json")) and "--again" not in sys.argv:
        SKIP_SWEEP = True
    else:
        SKIP_SWEEP = False
    if not SKIP_SWEEP:
        P.update(cam_pitch=0.15, head_rx=0.24, head_ry=0.2, head_rz=0.2, front_t=0.09, back_t=0.11, FL_x=-0.45, FR_x=0.1, BL_x=0.5, BR_x=-0.1)
        best = (-1, None)
        for yaw in np.linspace(-1.6, -0.4, 13):
            for hy in (-0.9, -0.5, 0.0, 0.5, 0.9):
                for flip in (1, -1):
                    Q = dict(P, cam_yaw=float(yaw), head_yaw=hy)
                    if flip < 0:  # which pair reaches: the near legs or the far ones
                        Q.update(FL_x=P["FR_x"], FR_x=P["FL_x"], BL_x=P["BR_x"], BR_x=P["BL_x"])
                    s, _, _ = score(Q)
                    if s > best[0]:
                        best = (s, Q)
        P = best[1]
        log(f"sweep: yaw {P['cam_yaw']:.2f} head_yaw {P['head_yaw']:.2f} score {best[0]:.4f}")
    cams = [k for k in P if k.startswith("cam_")]
    if not SKIP_SWEEP:
        P, s, cat, eyes = search(P, cams, 300, log)
    # everything, restarting the steps a few times so it can climb out of where it settles
    # FIT_KEYS=ear,head_yaw fits only the numbers whose names start so (the rest held)
    keys = [k for k in P if any(k.startswith(p) for p in os.environ.get("FIT_KEYS", "").split(",") if p)] or list(P)
    for r in range(ROUNDS):
        P, s, cat, eyes = search(P, keys, EVALS, log)
        log(f"round {r}: score {s:.4f} body {iou(cat, LOGO_CAT):.3f} eyes {iou(eyes, LOGO_EYES):.3f}")
        json.dump({k: round(v, 5) for k, v in P.items()}, open(os.path.join(HERE, "params.json"), "w"), indent=1)
        overlay(cat, eyes, os.path.join(OUTDIR, "overlay.png"))
    log(f"done: score {s:.4f} body {iou(cat, LOGO_CAT):.3f} eyes {iou(eyes, LOGO_EYES):.3f}")
