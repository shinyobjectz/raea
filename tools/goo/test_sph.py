"""tools/goo's SPH liquids (sph.py), coarse scenes in a minute or two each, checked for what goes wrong with liquids:
a puddle that doesn't settle or spreads one particle thick, liquid falling on past the bounds, a cup that leaks or a moving one that blows up, a stream
through an obstacle, an emitter that pours the wrong amount, a blow-up not caught, a cup set down onto the floor, and a one-thread bake that isn't
the same twice.

    $RAEA_GOO/.venv/bin/python -I tools/goo/test_sph.py [names...]
"""

import json
import math
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import run  # noqa: E402
import test_run  # noqa: E402

R = 0.002  # particle radius: coarse, quick
FLOOR = {"box": {"min": [-0.5, -0.04, -0.5], "max": [0.5, 0.0, 0.5]}}
BOUNDS = {"min": [-0.45, -0.05, -0.45], "max": [0.45, 0.6, 0.45]}


def bake(d, name, spec, check=True):
    sp = d / f"{name}.json"
    sp.write_text(json.dumps(spec))
    out = d / f"{name}.rseq"
    r = subprocess.run([sys.executable, "-I", str(HERE / "sph.py"), str(sp), str(out)], capture_output=True, text=True)
    if check and r.returncode != 0:
        raise SystemExit(f"{name}: sph.py failed ({r.returncode}):\n{r.stdout[-1500:]}\n{r.stderr[-1500:]}")
    return r, out


def frames(out):
    _, fr = run.read_rseq(out)
    return [np.asarray(f["position"] if isinstance(f, dict) else f[0]).reshape(-1, 3) for f in fr]


def spec(frames_, objects, **kw):
    s = {"solver": "sph", "fps": 30, "frames": frames_, "bounds": BOUNDS, "colliders": [FLOOR], "objects": objects,
         "sph": {"radius": R, "threads": 4}, "workers": 2}
    s.update(kw)
    return s


def mug(t0, t1, lift=0.0, tip=0.0):
    """a cup's pose track: standing at the origin, lifted from t0 to t1, then tipped about x over as long again (tipped
    about its foot as it rose, the foot's edge would dip into the floor)"""
    def ease(u):
        u = min(max(u, 0.0), 1.0)
        return u * u * (3 - 2 * u)

    poses = []
    for i in range(0, 121):
        t = i / 60
        u = ease((t - t0) / (t1 - t0))
        a = tip * ease((t - t1) / (t1 - t0))
        c, s_ = math.cos(a), math.sin(a)
        poses.append([t, 1, 0, 0, 0, 0, c, s_, 0, 0, -s_, c, 0, 0, lift * u, 0, 1])
    return poses


def test_dam(d):
    """a block of water collapses onto the floor and settles into a sheet, a few mm deep, without blowing up"""
    _, out = bake(d, "dam", spec(45, [{"name": "water", "liquid": "water", "block": {"min": [-0.1, 0.004, -0.05], "max": [0.0, 0.08, 0.05]}}]))
    f = frames(out)
    last = f[-1]
    assert len(last), "dam: nothing at the end"
    top = np.percentile(last[:, 1], 95)
    assert top < 0.03, f"dam: still {top * 1000:.0f} mm tall after 1.5 s (no collapse?)"
    assert last[:, 1].min() > -0.006, "dam: went through the floor"
    w = np.ptp(last[:, 0])
    assert 0.1 < w < 0.85, f"dam: spread {w:.2f} m across (none, or one particle thick everywhere)"
    return f"settled {top * 1000:.0f} mm deep, {w * 100:.0f} cm across"


def test_runoff(d):
    """liquid running off the edge of a small floor leaves the bounds and the simulation: still falling, it would pass a
    6 m/s maxSpeed and shrink the time step"""
    s = spec(45, [{"name": "water", "liquid": "water", "block": {"min": [0.04, 0.004, -0.03], "max": [0.1, 0.06, 0.03]}}],
             colliders=[{"box": {"min": [-0.1, -0.04, -0.1], "max": [0.1, 0.0, 0.1]}}], bounds={"min": [-0.15, -0.05, -0.15], "max": [0.3, 0.2, 0.15]})
    s["sph"]["maxSpeed"] = 6
    r, out = bake(d, "runoff", s)
    last = frames(out)[-1]
    assert (last[:, 0] > 0.1).sum() < 0.2 * len(last) + 1, "runoff: the stream off the edge is still there at the end"
    return "ran off and out without tripping 6 m/s"


def test_cup(d):
    """a cup of liquid lifted 15 cm keeps it in; tipped over, it pours it out and the cup ends empty"""
    cup = {"radius": 0.04, "height": 0.1, "base": 0.01, "wall": 0.01, "poses": mug(0.3, 0.8, lift=0.15)}
    _, out = bake(d, "cup", spec(36, [{"name": "c", "liquid": "coffee", "cup": 0, "level": 0.06}], cups=[cup]))
    f = frames(out)
    # mesh vertices sit a little outside the particles: allow the wall
    for k in (12, 24, 35):
        p = f[k]
        assert np.hypot(p[:, 0], p[:, 2]).max() < 0.05 + 3 * R, f"cup: liquid outside the cup at frame {k}"
    for k in (24, 35):  # fully lifted from 0.8 s: everything rides 15 cm up, above the cup's foot
        assert f[k][:, 1].min() > 0.15 - 3 * R, f"cup: liquid left behind or fell out of the bottom at frame {k}"
    cup2 = dict(cup, poses=mug(0.2, 0.7, lift=0.15, tip=2.4))
    _, out = bake(d, "pour", spec(60, [{"name": "c", "liquid": "coffee", "cup": 0, "level": 0.06}], cups=[cup2]))
    last = frames(out)[-1]
    on_floor = (last[:, 1] < 0.03).mean()
    assert on_floor > 0.85, f"cup: tipped over, only {on_floor:.0%} of the surface is on the floor"
    return f"held through the lift; tipped, {on_floor:.0%} on the floor"


def test_obstacle(d):
    """a stream onto a glTF block runs off it: nothing inside the block"""
    glb = d / "block.glb"
    test_run.box_glb(glb, [-0.05, 0.0, -0.05], [0.05, 0.05, 0.05])
    _, out = bake(d, "obstacle", spec(30, [{"name": "w", "liquid": "water", "emitter": {"at": [0.0, 0.2, 0.0], "dir": [0, -1, 0], "radius": 0.01, "speed": 1.0}}],
                                         obstacles=[{"src": str(glb)}]))
    f = frames(out)
    p = f[-1]
    inside = ((np.abs(p[:, 0]) < 0.045) & (np.abs(p[:, 2]) < 0.045) & (p[:, 1] > 0.005) & (p[:, 1] < 0.045)).sum()
    assert inside == 0, f"obstacle: {inside} surface vertices inside the block"
    assert (p[:, 1] < 0.02).any(), "obstacle: nothing ran off onto the floor"
    return "ran off the block, none inside"


def test_emitter(d):
    """an emitter pours its volume: 10 mm across at 1 m/s for 0.5 s is ~39 ml"""
    r, out = bake(d, "emit", spec(24, [{"name": "w", "liquid": "water", "emitter": {"at": [0.0, 0.1, 0.0], "dir": [0, -1, 0], "radius": 0.005, "speed": 1.0, "start": 0.0, "end": 0.5}}]))
    stats = json.loads(Path(str(out) + ".json").read_text())
    vol = stats["particles"] * (2 * R) ** 3
    want = math.pi * 0.005 ** 2 * 1.0 * 0.5
    assert 0.4 * want < vol < 2.5 * want, f"emitter: {vol * 1e6:.1f} ml poured, want ~{want * 1e6:.1f}"
    return f"poured {vol * 1e6:.1f} ml (ideal {want * 1e6:.1f})"


def test_honey(d):
    """a honey drizzle from 25 cm speeds up as it falls and lands in about half a second (it hung at the tap when the
    emitter's own wall held it, and fell at the tap's speed at 700x real honey's viscosity), then piles"""
    _, out = bake(d, "honey", spec(24, [{"name": "h", "liquid": "honey", "emitter": {"at": [0.0, 0.25, 0.0], "dir": [0, -1, 0], "radius": 0.005, "speed": 0.3, "start": 0.0, "end": 0.6}}]))
    f = frames(out)
    land = next((k for k, p in enumerate(f) if len(p) and p[:, 1].min() < 0.01), None)
    assert land is not None and land / 30 < 0.6, f"honey: landed at {land and land / 30} s, not by 0.6 (free fall from 25 cm is 0.23 s)"
    return f"landed at {land / 30:.2f} s"


def test_guard(d):
    """a stream at 60 m/s is past maxSpeed: the bake stops and says so, instead of baking garbage"""
    r, _ = bake(d, "guard", spec(6, [{"name": "w", "liquid": "water", "emitter": {"at": [0.0, 0.3, 0.0], "dir": [0, -1, 0], "radius": 0.005, "speed": 60.0}}]), check=False)
    assert r.returncode != 0 and "blew up" in (r.stdout + r.stderr), f"guard: didn't stop ({r.returncode})"
    return "stopped with where"


def test_landing(d):
    """a cup set down hard onto the floor is refused before simulating (the liquid under it would be crushed); the same
    cup lifting off from the floor is not"""
    up = mug(1.5, 1.8, lift=0.1)  # lifted off over 1.5-1.8 s; played backward, set down over 0.2-0.5 s
    down = [[q[0], *p[1:]] for q, p in zip(up, reversed(up))]
    cup = {"radius": 0.04, "height": 0.1, "base": 0.01, "wall": 0.01, "poses": down}
    r, _ = bake(d, "landing", spec(18, [{"name": "c", "liquid": "water", "cup": 0, "level": 0.05}], cups=[cup]), check=False)
    assert r.returncode != 0 and "closes to" in r.stdout + r.stderr, f"landing: a cup set down onto the floor wasn't refused ({r.returncode})"
    return "set down: refused before simulating"


def test_repeatable(d):
    """one thread: two bakes, the same bytes"""
    s = spec(12, [{"name": "water", "liquid": "water", "block": {"min": [-0.05, 0.004, -0.03], "max": [0.0, 0.05, 0.03]}}])
    s["sph"]["threads"] = 1
    _, a = bake(d, "rep_a", s)
    _, b = bake(d, "rep_b", s)
    assert a.read_bytes() == b.read_bytes(), "repeatable: two one-thread bakes differ"
    return "same bytes"


TESTS = {"dam": test_dam, "runoff": test_runoff, "cup": test_cup, "obstacle": test_obstacle, "emitter": test_emitter, "honey": test_honey, "guard": test_guard, "landing": test_landing, "repeatable": test_repeatable}


def main():
    names = sys.argv[1:] or list(TESTS)
    failed = 0
    with tempfile.TemporaryDirectory(prefix="raea-sph-test-") as td:
        for n in names:
            try:
                print(f"ok   {n}: {TESTS[n](Path(td))}", flush=True)
            except (AssertionError, SystemExit) as e:
                failed += 1
                print(f"FAIL {n}: {e}", flush=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
