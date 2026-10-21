"""Liquids for goo specs, simulated with SPlisHSPlasH (SPH: DFSPH pressure, Weiler 2018 viscosity, Jeske 2023 implicit
surface tension if asked for) instead of run.py's MPM: thin liquids that pour, splash and spread into a puddle a few
millimetres deep and must hold together doing it (coffee dumped from a mug, water from a tap), which MPM at a usable
grid breaks into beads. run.py hands a spec with "solver": "sph" here.

  ~/.cache/raea-goo/.venv/bin/python sph.py SPEC.json OUT.rseq

Reads the goo spec (fps, frames, bounds, colliders, cups, surface, wet, workers) plus "obstacles" [{src (a glTF),
nodes, transform, poses}] (meshes standing, or moved along poses like a cup's) and an "sph" block {radius (particle
radius, m: 0.001), viscosity and viscosityBoundary (override the presets'), tension (Jeske 2023 coefficient, 0: off),
tensionBoundary (0.5), tensionViscosity (0.05), step (largest time step, s: 0.001), cfl (0.5), threads (8; 1 is
repeatable), maxSpeed (m/s, 25), boundary ("volume" maps or sampled "particles")}. Each object is a liquid, its own
fluid: {name, liquid (a PRESETS name, "water"), color, density, viscosity, and one source: cup + level (a cup filled
to level m above its foot), block {min, max}, src (+ nodes, transform: a closed glTF mesh, filled) or emitter {at,
dir, radius, speed, start, end}}. Cups are hollow cylinders on their tracks, the wall at least 4.5 particle radii.
Writes OUT.rseq and its .json like run.py, the surface run.py's own (surface()).

What holds, learned on the coffee: viscosity 0.002 solved tight (0.1%) settles a spill 4-5 mm deep, as coffee does; at
the solver's default tolerance it blew up when the mug stopped hard, and with none the spill spreads one particle
thick. Surface tension beads millimetre-scale liquid into balls (30 already) and blows up from 100, so it's off. One
thread is byte-for-byte repeatable; OpenMP's threaded sums are not, even at a fixed count. A particle past maxSpeed
stops the bake with where it was, rather than bake garbage; a cup whose track comes down within 3 particle radii of a
collider stops it before it starts (the liquid caught between them is crushed: the coffee's mug, laid on its side 1.3 mm
over the stone, threw a particle at 58 m/s). Liquid leaving the bounds is parked outside the simulation. Emitters need a Python that isn't a framework build
(setup.sh uses uv's): SPlisHSPlasH loads their meshes from beside the running executable. Checked by test_sph.py."""
import atexit
import json
import math
import os
import shutil
import sys
import tempfile
import time
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).parent))
import run  # noqa: E402  (the shared pieces: Cup, surface, write_rseq, wet_colour, hex colours)


def say(m):
    print(f"sph {m}", flush=True)


def revolve(profile, n=48):
    """a closed solid of revolution about +y from a profile [(r, y)] running axis to axis: (vertices, faces)"""
    V, ring = [], []
    for r, y in profile:
        if r <= 1e-9:
            V.append((0.0, y, 0.0))
            ring.append([len(V) - 1] * n)
        else:
            ring.append([len(V) + j for j in range(n)])
            V.extend((r * math.cos(2 * math.pi * j / n), y, -r * math.sin(2 * math.pi * j / n)) for j in range(n))
    F = []
    for k in range(len(profile) - 1):
        a, b = ring[k], ring[k + 1]
        for j in range(n):
            j1 = (j + 1) % n
            for t in ((a[j], a[j1], b[j1]), (a[j], b[j1], b[j])):
                if len(set(t)) == 3:
                    F.append(t)
    V, F = np.array(V, np.float64), np.array(F, np.int64)
    # outward: a positive signed volume
    vol = np.einsum("ij,ij->i", V[F[:, 0]], np.cross(V[F[:, 1]], V[F[:, 2]])).sum()
    if vol < 0:
        F = F[:, ::-1]
    return V, F


def box(lo, hi):
    lo, hi = np.array(lo, np.float64), np.array(hi, np.float64)
    V = np.array([[x, y, z] for x in (lo[0], hi[0]) for y in (lo[1], hi[1]) for z in (lo[2], hi[2])])
    F = np.array([[0, 1, 3], [0, 3, 2], [4, 6, 7], [4, 7, 5], [0, 4, 5], [0, 5, 1], [2, 3, 7], [2, 7, 6], [0, 2, 6], [0, 6, 4], [1, 5, 7], [1, 7, 3]])
    vol = np.einsum("ij,ij->i", V[F[:, 0]], np.cross(V[F[:, 1]], V[F[:, 2]])).sum()
    return V, (F[:, ::-1] if vol < 0 else F)


def write_obj(path, V, F):
    with open(path, "w") as f:
        f.writelines(f"v {x:.6f} {y:.6f} {z:.6f}\n" for x, y, z in V)
        f.writelines(f"f {a + 1} {b + 1} {c + 1}\n" for a, b, c in F)


def axis_angle(R):
    q = run.quat_of(R)  # w x y z
    w = float(np.clip(q[0], -1.0, 1.0))
    a = 2.0 * math.acos(w)
    s = math.sqrt(max(1.0 - w * w, 0.0))
    return ([1.0, 0.0, 0.0] if s < 1e-8 else (q[1:] / s).tolist()), a


class Track:
    """a pose track [[t, 16 numbers: the frame to world, column-major], ...]: positions lerp, turns slerp (as cups')"""

    def __init__(self, poses):
        poses = sorted(poses, key=lambda q: q[0])
        self.ts = np.array([q[0] for q in poses], np.float64)
        ms = [np.array(q[1:17], np.float64).reshape(4, 4).T for q in poses]
        self.ps = np.array([m[:3, 3] for m in ms])
        self.qs = np.array([run.quat_of(m[:3, :3]) for m in ms])
        for k in range(1, len(self.qs)):
            if np.dot(self.qs[k], self.qs[k - 1]) < 0:
                self.qs[k] = -self.qs[k]

    def pose(self, t):
        return run.Cup.pose(self, t)


def sinks(what, track, V, colliders, seconds, pr):
    """stop before simulating if a moving body comes down within 3 particle radii of a collider box (or into it): the
    liquid caught between two boundaries closer than a particle or two can't get out, and is crushed until it blows up
    (the coffee's mug laid on its side, landing 1.3 mm over the stone, threw a particle at 58 m/s). Only closing in
    counts: a cup standing where it starts, or lifting off, is fine."""
    gap = 3 * pr
    last = {}
    for t in np.arange(0.0, seconds + 1e-9, 1 / 240):
        R, c = track.pose(t)
        W = V @ R.T + c
        for j, cb in enumerate(colliders):
            lo, hi = np.asarray(cb["box"]["min"]) - gap, np.asarray(cb["box"]["max"]) + gap
            inside = np.all((W > lo) & (W < hi), axis=1)
            d = float(np.minimum(W[inside] - lo, hi - W[inside]).min(axis=1).max()) if inside.any() else 0.0
            if d > 0.5 * pr and d > last.get(j, d) + 1e-6:  # (where it starts is where it rests)
                raise SystemExit(f"sph: {what} closes to {(gap - d) * 1000:.1f} mm of collider {j} at t {t:.3f} s (under {gap * 1000:.1f} mm, 3 particle "
                                 f"radii, or inside it); liquid caught between them would blow up. Keep it clear: a cup's outside is its radius + "
                                 f"wall, the wall at least 4.5 particle radii")
            last[j] = d


def omega(track, t, e=1.0 / 480):
    """a track's angular velocity at t (world)"""
    R0, _ = track.pose(t - e)
    R1, _ = track.pose(t + e)
    W = (R1 - R0) / (2 * e) @ track.pose(t)[0].T
    return np.array([W[2, 1] - W[1, 2], W[0, 2] - W[2, 0], W[1, 0] - W[0, 1]]) * 0.5


def glb_triangles(src, nodes=None, transform=None):
    """a glTF's nodes (all its meshes when none are named) as world-space triangles, moved by `transform`"""
    g = run.Glb(src)
    wm = g.world()
    if not nodes:
        nodes = [n.get("name") for n in g.j["nodes"] if "mesh" in n and n.get("name")]
    xf = np.array(transform or np.eye(4).T.ravel(), np.float64).reshape(4, 4).T
    tris = [t for prim in run.object_mesh(g, wm, nodes, xf) for t in prim[0]]
    if not tris:
        raise SystemExit(f"sph: no triangles in {src} {nodes or ''}")
    T = np.array(tris, np.float64).reshape(-1, 3, 3)
    V = T.reshape(-1, 3)
    return V, np.arange(len(V)).reshape(-1, 3)


# Liquids by name: colour (sRGB), density, and viscosity (the Weiler 2018 solver's, liquid and against what it lies
# on). At 0.002 a spill settles 4-5 mm deep (48 ml over ~100 cm2), as coffee does; with none it spreads one particle
# thick like a gas, and the wall term alone isn't stable. Thick liquids (oil, syrup, honey) take more.
PRESETS = {
    "water": {"color": "#d8e8f0", "density": 1000, "viscosity": 0.002, "viscosityBoundary": 0.002},
    "coffee": {"color": "#2a160b", "density": 1000, "viscosity": 0.002, "viscosityBoundary": 0.002},
    "tea": {"color": "#8a4a12", "density": 1000, "viscosity": 0.002, "viscosityBoundary": 0.002},
    "milk": {"color": "#f4f1ea", "density": 1030, "viscosity": 0.003, "viscosityBoundary": 0.003},
    "juice": {"color": "#f29a1c", "density": 1040, "viscosity": 0.003, "viscosityBoundary": 0.003},
    # thick ones in order but far under their real ratios to water: at 5 (honey's old preset, ~700x real honey) the
    # viscous length (nu^2/g)^(1/3) is 1.4 m, so a 25 cm drizzle can't speed up and falls at the emitter's speed
    "oil": {"color": "#d9b13b", "density": 920, "viscosity": 0.01, "viscosityBoundary": 0.01},
    "syrup": {"color": "#6b2f0c", "density": 1330, "viscosity": 0.02, "viscosityBoundary": 0.02},
    "honey": {"color": "#d18b16", "density": 1420, "viscosity": 0.05, "viscosityBoundary": 0.05},
}


def liquid_of(ob, o):
    """an object's liquid: its preset, then the spec's sph block, then the object's own keys"""
    lq = dict(PRESETS.get(ob.get("liquid", "water"), PRESETS["water"]))
    for k in ("viscosity", "viscosityBoundary"):
        if k in o:
            lq[k] = float(o[k])
    for k in ("color", "density", "viscosity", "viscosityBoundary"):
        if k in ob:
            lq[k] = ob[k]
    return lq


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if len(argv) != 2:
        raise SystemExit("usage: sph.py SPEC.json OUT.rseq")
    T0 = time.time()
    spec = json.loads(Path(argv[0]).read_text())
    out = Path(argv[1])
    fps = float(spec.get("fps", 30))
    frames = int(spec["frames"])
    o = dict(spec.get("sph", {}))
    pr = float(o.get("radius", 0.001))
    bmin = np.array(spec["bounds"]["min"], np.float64); bmax = np.array(spec["bounds"]["max"], np.float64)
    tension = float(o.get("tension", 0.0))
    # threads: 8 by default. OpenMP's sums land in a different order run to run, so a threaded bake isn't byte for byte
    # the same twice (even at a fixed count); a bake is kept once made, so a shot doesn't change, and threads 1 (~2.5x
    # slower) is repeatable
    threads = int(o.get("threads", min(8, os.cpu_count() or 1)))
    os.environ["OMP_NUM_THREADS"] = str(threads)
    max_speed = float(o.get("maxSpeed", 25.0))
    sf = spec.get("surface", {})
    skin = (float(sf.get("blur", 1.0)), float(sf.get("level", 0.33)), float(sf.get("flat", 1.0)), int(sf.get("maxTris", run.MAX_TRIS)))
    seconds = (frames - 1) / fps
    keep = Path(os.environ["RAEA_GOO_PARTICLES"]) if os.environ.get("RAEA_GOO_PARTICLES") else None
    if keep:
        keep.mkdir(parents=True, exist_ok=True)

    import pysplishsplash as sph

    tmp = Path(tempfile.mkdtemp(prefix="raea-sph-"))
    atexit.register(shutil.rmtree, tmp, True)  # the scene, its meshes and SPlisHSPlasH's own output, gone however it ends
    bodies, moving = [], []  # moving: (body index, track)

    def body(path, V, F, track=None, res=None):
        write_obj(path, V, F)
        ext = V.max(0) - V.min(0)
        if res is None:
            res = np.clip(np.ceil(ext / (pr * 1.25)), 8, 160).astype(int).tolist()
        b = {"geometryFile": str(path), "translation": [0, 0, 0], "rotationAxis": [1, 0, 0], "rotationAngle": 0, "scale": [1, 1, 1],
             "isDynamic": False, "isWall": False, "mapInvert": False, "mapThickness": 0.0, "mapResolution": res, "samplingMode": 1}
        if track is not None:
            R0, c0 = track.pose(0.0)
            ax, an = axis_angle(R0)
            b.update({"translation": c0.tolist(), "rotationAxis": ax, "rotationAngle": an, "isAnimated": True})
            moving.append((len(bodies), track))
        bodies.append(b)

    # cups: hollow cylinders on their tracks, the wall thicker than a particle's reach (its kernel's support, 4 radii),
    # outward, or a surge punches through and the pressure blows up (a 5 mm wall at 1.5 mm particles did)
    cups = []
    for k, cs in enumerate(spec.get("cups", [])):
        c = run.Cup(cs, 0.0)
        c.ro = c.ri + max(float(cs.get("wall", 0.005)), 4.5 * pr)
        cups.append(c)
        V, F = revolve([(0, 0), (c.ro, 0), (c.ro, c.h), (c.ri, c.h), (c.ri, c.base), (0, c.base)])
        sinks(f"cup {k}", c, V, spec.get("colliders", []), seconds, pr)
        body(tmp / f"cup{k}.obj", V, F, track=c)
    # meshes from glTFs, standing or on a pose track (the track moves the mesh from where the glTF has it)
    for k, ob in enumerate(spec.get("obstacles", [])):
        V, F = glb_triangles(ob["src"], ob.get("nodes"), ob.get("transform"))
        body(tmp / f"mesh{k}.obj", V, F, track=Track(ob["poses"]) if ob.get("poses") else None)
    # boxes, cropped to the bounds (the floor runs on past them), with a coarse map: a box's faces are flat, so its
    # volume map interpolates well from few cells, and a fine one over a whole floor takes minutes to build
    for k, c in enumerate(spec.get("colliders", [])):
        lo = np.maximum(c["box"]["min"], bmin - 0.05); hi = np.minimum(c["box"]["max"], bmax + 0.05)
        body(tmp / f"box{k}.obj", *box(lo, hi), res=np.clip(np.ceil((hi - lo) / 0.02), 6, 64).astype(int).tolist())

    # the liquids: each object its own fluid model and material, from a cup filled to a level, a box, a closed mesh
    # filled, or an emitter (a tap, a jug's lip) pouring a round stream
    materials, models, blocks, emitters, colours = [], [], [], [], {}
    for k, ob in enumerate(spec["objects"]):
        lid = f"liquid{k}"
        lq = liquid_of(ob, o)
        colours[lid] = run.wet_colour(np.array([run.srgb_hex(lq["color"])], np.float32), float(spec.get("wet", 0.3)))[0]
        m = {"id": lid, "density0": float(lq["density"]), "viscosityMethod": 6,

             "Weiler et al. 2018": {"viscosity": float(lq["viscosity"]), "viscosityBoundary": float(lq["viscosityBoundary"]),
                                    # solved tight: at the default tolerance (1%) it blew up when the mug stopped hard (a particle
                                    # at 38 m/s); at 0.5% a particle spiked to 5.6 m/s; at 0.2% and below it holds (0.2 m/s)
                                    "viscoMaxIter": int(o.get("viscoMaxIter", 300)), "viscoMaxError": float(o.get("viscoMaxError", 0.001))},
             "surfaceTensionMethod": 4 if tension > 0 else 0,
             "Jeske et al. 2023": {"surfaceTension": tension, "surfaceTensionBoundary": float(o.get("tensionBoundary", 0.5)),
                                   "surfaceTensionViscosity": float(o.get("tensionViscosity", 0.05)), "surfaceTensionMaxIter": 100,
                                   "surfaceTensionMaxError": 0.001}}
        if "cup" in ob:
            c = cups[int(ob["cup"])]
            lv, lf = revolve([(0, c.base + pr), (c.ri - pr, c.base + pr), (c.ri - pr, float(ob["level"])), (0, float(ob["level"]))])
            R0, c0 = c.pose(0.0)
            write_obj(tmp / f"{lid}.obj", lv @ R0.T + c0, lf)
            models.append({"id": lid, "particleFile": str(tmp / f"{lid}.obj"), "translation": [0, 0, 0], "scale": [1, 1, 1], "resolutionSDF": [40, 40, 40]})
        elif "block" in ob:
            blocks.append({"id": lid, "denseMode": 0, "start": ob["block"]["min"], "end": ob["block"]["max"], "translation": [0, 0, 0], "scale": [1, 1, 1]})
        elif "src" in ob:
            V, F = glb_triangles(ob["src"], ob.get("nodes"), ob.get("transform"))
            write_obj(tmp / f"{lid}.obj", V, F)
            models.append({"id": lid, "particleFile": str(tmp / f"{lid}.obj"), "translation": [0, 0, 0], "scale": [1, 1, 1], "resolutionSDF": [64, 64, 64]})
        elif "emitter" in ob:
            e = ob["emitter"]
            d = np.array(e.get("dir", [0, -1, 0]), np.float64); d /= np.linalg.norm(d)
            # the emitter shoots along its local x: turn x onto `dir`
            x = np.array([1.0, 0, 0]); ax = np.cross(x, d); s_ = np.linalg.norm(ax); an = math.atan2(s_, float(np.dot(x, d)))
            ax = (ax / s_).tolist() if s_ > 1e-8 else [0.0, 0.0, 1.0]
            radius = float(e.get("radius", 0.006))
            t0, t1 = float(e.get("start", 0.0)), float(e.get("end", seconds))
            speed = float(e.get("speed", 1.0))
            m["maxEmitterParticles"] = int(math.pi * radius ** 2 * speed * (t1 - t0) / (2 * pr) ** 3 * 1.3) + 1000
            m["emitterReuseParticles"] = False
            emitters.append({"id": lid, "type": 1, "width": 2 * radius / (2 * pr), "translation": list(map(float, e["at"])), "rotationAxis": ax,
                             "rotationAngle": an, "velocity": speed, "emitStartTime": t0, "emitEndTime": t1})
        else:
            raise SystemExit(f"sph: object {ob.get('name', k)} needs a source: cup+level, block, src or emitter")
        materials.append(m)

    scene = {
        "Configuration": {"particleRadius": pr, "timeStepSize": 0.0002, "simulationMethod": 4,
                          "boundaryHandlingMethod": {"volume": 2, "particles": 0}[o.get("boundary", "volume")],
                          "gravitation": spec.get("gravity", [0, -9.81, 0]), "cflMethod": 1, "cflFactor": float(o.get("cfl", 0.5)),
                          "cflMaxTimeStepSize": float(o.get("step", 0.001)), "cflMinTimeStepSize": 1e-5, "stopAt": seconds + 0.5 / fps,
                          "enablePartioExport": False, "enableVTKExport": False, "enableRigidBodyExport": False},
        "Materials": materials, "RigidBodies": bodies, "FluidModels": models, "FluidBlocks": blocks, "Emitters": emitters,
    }
    (tmp / "scene.json").write_text(json.dumps(scene, indent=1))

    from concurrent.futures import ProcessPoolExecutor
    import multiprocessing as mp

    pool = ProcessPoolExecutor(int(spec.get("workers", 5)), mp_context=mp.get_context("spawn"))
    pending = []
    st = {"f": 0, "n": 0, "t0": time.time(), "k": 0, "vmax": 0.0}

    def fluids():
        sim = sph.Simulation.getCurrent()
        return [sim.getFluidModel(i) for i in range(sim.numberOfFluidModels())]

    def field(fm, name):
        # the solver's own array for a field (live: writing it writes the particles), the active particles' rows
        return np.asarray(fm.getFieldBuffer(name))[: fm.numActiveParticles()]

    def park():
        # liquid leaving the bounds leaves the simulation: falling on past them it would only get faster, shrinking
        # every step (the CFL step follows the fastest particle) and at last tripping maxSpeed. Its particles stop
        # being integrated (an emitter's state: SPlisHSPlasH moves only active ones) and wait, still, a metre below
        # the bounds, spaced past each other's reach. Found by where they are, every step: the solver re-sorts its
        # particles now and then, so an index doesn't stay one particle. (The emitters' own reuse runs only from one.)
        g, floor = 6 * pr, bmin[1] - 0.5
        for fm in fluids():
            x, v, sta = field(fm, "position"), field(fm, "velocity"), field(fm, "state")
            out = np.nonzero(np.any((x < bmin - 2 * pr) | (x > bmax + 2 * pr), axis=1) & (x[:, 1] > floor))[0]
            if len(out):
                x[out] = np.stack([bmin[0] + (out % 200) * g, bmin[1] - 1.0 - (out // 40000) * g, bmin[2] + (out // 200 % 200) * g], 1)
            gone = x[:, 1] < floor
            sta[gone] = 1
            v[gone] = 0

    def grab(t):
        # each frame as the sim passes its time: the particles still in the bounds, surfaced off the main thread
        xs, cs = [], []
        for fm in fluids():
            x = field(fm, "position").astype(np.float64)
            xs.append(x); cs.append(np.tile(colours[fm.getId()], (len(x), 1)))
        xs = np.concatenate(xs) if xs else np.zeros((0, 3)); cs = np.concatenate(cs) if cs else np.zeros((0, 3), np.float32)
        st["n"] = max(st["n"], len(xs))
        live = np.all((xs >= bmin) & (xs <= bmax), axis=1)
        if keep:  # RAEA_GOO_PARTICLES: each frame's particles, to tune the skin (`surface`) without a rebake
            np.savez_compressed(keep / f"{st['f']:04d}.npz", x=xs[live].astype(np.float32), c=cs[live])
        pending.append(pool.submit(run.surface_job, xs[live], cs[live], 2 * pr, None, skin, run.cup_bores(cups, t)))
        f = st["f"]
        if f % 10 == 0:
            tm = sph.TimeManager.getCurrent()
            say(f"frame {f}/{frames}: t {t:.3f}, {int(live.sum())} live, dt {tm.getTimeStepSize() * 1000:.2f} ms, top speed {st['vmax']:.1f} m/s, {time.time() - st['t0']:.0f}s")
        st["f"] = f + 1

    def guard(t):
        # a blow-up: a particle far faster than anything here moves (a fall of a metre is 4.4 m/s) means the solve has
        # gone unstable; stop with where, rather than bake a spray of garbage or crawl at a vanishing time step
        vm = 0.0
        for fm in fluids():
            v = field(fm, "velocity")
            if len(v):
                sp = np.linalg.norm(v, axis=1)
                j = int(np.argmax(sp))
                if sp[j] > max_speed:
                    p = field(fm, "position")[j]
                    raise SystemExit(f"sph: blew up at t {t:.3f} s: a particle of {fm.getId()} at {np.round(p, 3).tolist()} moving {sp[j]:.0f} m/s "
                                     f"(limit sph.maxSpeed {max_speed}); try a smaller radius, thicker walls or a gentler move")
                vm = max(vm, float(sp[j]))
        st["vmax"] = vm

    def drive():
        sim = sph.Simulation.getCurrent()
        t = sph.TimeManager.getCurrent().getTime()
        if not st.get("boxes"):
            # SPlisHSPlasH walls each emitter in a box of its own, an ordinary boundary after ours: a viscous liquid
            # sticks to it (a honey stream hung at the tap), so they go far away before the first step
            for bi in range(len(bodies), sim.numberOfBoundaryModels()):
                rb = sim.getBoundaryModel(bi).getRigidBodyObject()
                rb.setPosition(np.asarray([0, -50, 0], np.float32))
                rb.updateMeshTransformation()
            st["boxes"] = True
        park()
        while st["f"] < frames and t >= st["f"] / fps - 1e-7:
            grab(t)
        guard(t)
        # the moving bodies where their tracks have them now, moving as they move (the boundary takes their velocity)
        e = 1.0 / 480
        for bi, track in moving:
            rb = sim.getBoundaryModel(bi).getRigidBodyObject()
            R, c = track.pose(t)
            v = (track.pose(t + e)[1] - track.pose(t - e)[1]) / (2 * e)
            q = run.quat_of(R)
            rb.setPosition(np.asarray(c, np.float32))
            rb.setRotation(np.asarray([q[1], q[2], q[3], q[0]], np.float32))  # Eigen's coefficient order: x y z w
            rb.setVelocity(np.asarray(v, np.float32))
            rb.setAngularVelocity(np.asarray(omega(track, t), np.float32))
            rb.updateMeshTransformation()

    base = sph.Exec.SimulatorBase()
    base.init(sceneFile=str(tmp / "scene.json"), useCache=False, outputDir=str(tmp / "out"), initialPause=False, useGui=False,
              stopAt=seconds + 0.5 / fps)
    base.setTimeStepCB(drive)
    base.run()
    while st["f"] < frames:  # stopped a hair early: the last frame as it stands
        grab(sph.TimeManager.getCurrent().getTime())
    sim_s = time.time() - st["t0"]
    blobs, tris = [], []
    for p in pending:
        b, k = p.result()
        blobs.append(b); tris.append(k)
    pool.shutdown()
    nbytes = run.write_rseq(out, fps, blobs)
    stats = {"particles": int(st["n"]), "objects": [{"name": ob.get("name", f"liquid{k}"), "start": ob.get("start"), "hideFrame": None} for k, ob in enumerate(spec["objects"])],
             "frames": frames, "fps": fps, "radius": pr, "threads": threads, "simSeconds": round(sim_s, 2),
             "seconds": round(time.time() - T0, 2), "bytes": nbytes, "maxTriangles": int(max(tris) if tris else 0), "glb": [], "backend": "sph"}
    Path(str(out) + ".json").write_text(json.dumps(stats, indent=1))
    say(f"wrote {out} ({nbytes / 1e6:.1f} MB, {frames} frames, max {stats['maxTriangles']} triangles) in {stats['seconds']}s")


if __name__ == "__main__":
    main()
