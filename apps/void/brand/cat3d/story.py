"""The cat discovers it can make music by jumping: the one timeline the animation, the picture and the score share.

    python3 apps/void/brand/cat3d/story.py      -> apps/void/brand/cat3d/story.json (cinematic.tsx reads it)
    build.py imports it and bakes perform(t) into the cat's Cinematic clip.

30 s at 30 fps, 120 bpm (a beat is 15 frames, a bar 60). Coordinates are raea's (glTF): x right, y up, z toward the
first camera; the cat starts on the middle tile facing +z. Tiles are TILE apart; a hop lands on a tile's centre, and
the landing is the note.

Beats (seconds):
   0    in the dark, standing; light comes up. Breathing, a blink.
   3    looks around: left, right, up (it heard something), down at its paws
   8.5  a small hop in place to test the ground: the tile lights and plays C. It freezes, stares down, then at us
  11    tries again, forward: E. Head tilt.  12.5  again: G, tail up, it likes this
  14    two quick bounces in place: A, G
  15    finds the beat: a hop on every beat across the floor, a melody, the kick comes in (16), the bass (18)
  22    stops, turns to the middle, crouches, wiggles (a cat about to pounce)
  23    the big jump to the middle: lands at 24.2 on the chord, every tile floods with colour
  24.4  looks round at what it made
  27    looks at us, blinks;  29 a last tiny hop: high C.  29.4-30 fades out
"""

import json
import math
import os

# where the legs hang from (build.py's numbers, fitted): the feet's homes are under them
_P = dict(front_x=0.165, front_y=-0.178, back_x=0.146, back_y=0.257)
try:
    _P.update({k: v for k, v in json.load(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "params.json"))).items() if k in _P})
except FileNotFoundError:
    pass
# each foot under its hip, in the cat's own frame: (across, forward); Blender's y is backward
FEET = {"FL": (_P["front_x"], -_P["front_y"]), "FR": (-_P["front_x"], -_P["front_y"]), "BL": (_P["back_x"], -_P["back_y"]), "BR": (-_P["back_x"], -_P["back_y"])}
# feet that mustn't be in the air together (the same side or the same end): the cat always stands on three
NEIGHBOURS = {"FL": ("FR", "BL"), "FR": ("FL", "BR"), "BL": ("BR", "FL"), "BR": ("BL", "FR")}

FPS = 30
DUR = 30.0
N = int(DUR * FPS)
TILE = 0.6
BPM = 120
BEAT = 60 / BPM

# a note's colour (the void lights up in the melody's colours)
COLOR = {
    "C4": "#ff5e57", "D4": "#ff9f43", "E4": "#ffd93d", "G4": "#4ade80", "A4": "#38bdf8", "C5": "#a78bfa",
    "G3": "#f472b6", "chord": "#ffffff",
}


def tile(i, j):
    return (i * TILE, j * TILE)


# ---------------------------------------------------------------- the hops: takeoff, landing, where to, how high, the note

HOPS = []


def hop(t_land, to, note, h=0.16, air=0.32, crouch=0.05, antic=0.22, big=False):
    HOPS.append(dict(t0=round(t_land - air, 4), t1=t_land, to=to, note=note, h=h, crouch=crouch, antic=antic, big=big))


# exploring: off the beat, as it tries things out
hop(8.9, (0, 0), "C4", h=0.10, air=0.26, crouch=0.04, antic=0.45)
hop(11.4, (1, 0), "E4", h=0.14, air=0.34, crouch=0.05, antic=0.35)
hop(12.9, (2, 0), "G4", h=0.18, air=0.34, crouch=0.05, antic=0.25)
hop(14.0, (2, 0), "A4", h=0.10, air=0.24, crouch=0.035, antic=0.16)
hop(14.5, (2, 0), "G4", h=0.10, air=0.24, crouch=0.035, antic=0.16)
# on the beat: a melody across the floor
PATH = [(3, 0), (4, 1), (3, 2), (2, 2), (1, 3), (0, 3), (-1, 2), (-2, 2), (-3, 1), (-3, 0), (-2, -1), (-1, -1), (0, -2), (1, -2)]
MELODY = ["C4", "D4", "E4", "G4", "A4", "G4", "E4", "G4", "A4", "C5", "A4", "G4", "E4", "D4"]
for k, (p, n) in enumerate(zip(PATH, MELODY)):
    hop(15.0 + k * BEAT + BEAT, p, n, h=0.2, air=0.3, crouch=0.04, antic=0.14)
# the big one, to the middle, on the downbeat of bar 13
hop(24.2, (0, 0), "chord", h=1.1, air=1.0, crouch=0.13, antic=0.9, big=True)
# the last
hop(29.0, (0, 0), "C5", h=0.08, air=0.22, crouch=0.03, antic=0.2)


def landings():
    """Every landing as the picture and the score want it: frame, time, tile, note, colour."""
    out = []
    for h in HOPS:
        out.append(dict(t=h["t1"], frame=round(h["t1"] * FPS), x=tile(*h["to"])[0], z=tile(*h["to"])[1], note=h["note"], color=COLOR[h["note"]], big=h["big"]))
    return out


# ---------------------------------------------------------------- where the cat is: root path and facing


def _smooth(u):
    u = min(1.0, max(0.0, u))
    return u * u * (3 - 2 * u)


def root(t):
    """(x, y, z) of the cat's root: on a tile, or on a hop's arc (level speed across, a parabola up)."""
    pos = (0.0, 0.0)
    for h in HOPS:
        dst = tile(*h["to"])
        if t < h["t0"]:
            return (pos[0], 0.0, pos[1])
        if t < h["t1"]:
            u = (t - h["t0"]) / (h["t1"] - h["t0"])
            x = pos[0] + (dst[0] - pos[0]) * u
            z = pos[1] + (dst[1] - pos[1]) * u
            return (x, 4 * h["h"] * u * (1 - u), z)
        pos = dst
    return (pos[0], 0.0, pos[1])


def lift(t):
    """The root's height and its vertical speed (m/s): the hop's parabola, or 0 on the floor."""
    for h in HOPS:
        if h["t0"] <= t < h["t1"]:
            d = h["t1"] - h["t0"]
            u = (t - h["t0"]) / d
            return 4 * h["h"] * u * (1 - u), 4 * h["h"] * (1 - 2 * u) / d
    return 0.0, 0.0


def hop_at(t):
    """The hop the cat is in the air for at t, or None."""
    for h in HOPS:
        if h["t0"] <= t < h["t1"]:
            return h
    return None


def antic(t):
    """The scripted crouch before a takeoff (m, + down) and how far into it (0..1)."""
    for h in HOPS:
        a0 = h["t0"] - h["antic"]
        if a0 <= t < h["t0"]:
            u = (t - a0) / h["antic"]
            return h["crouch"] * _smooth(u) ** 1.5, u
    return 0.0, 0.0


_HEADS = None


def _heading_targets():
    global _HEADS
    if _HEADS is None:
        _HEADS = _heading_keys()
    return _HEADS


def _heading_keys():
    """Where the cat means to face: toward each hop that goes somewhere, from its crouch; scripted turns besides."""
    keys = [(0.0, 0.0)]
    pos = (0.0, 0.0)
    for h in HOPS:
        dst = tile(*h["to"])
        dx, dz = dst[0] - pos[0], dst[1] - pos[1]
        if abs(dx) + abs(dz) > 1e-6:
            keys.append((h["t0"] - h["antic"] - 0.18, math.atan2(dx, dz)))
        pos = dst
    keys.append((24.5, 0.35))  # after the big landing, three-quarters to us
    keys.append((27.0, 0.0))  # square to us for the end
    # each heading the nearest turn from the one before (atan2 wraps at a half turn, and the long way round is a spin)
    out, prev = [], 0.0
    for at, a in sorted(keys):
        a += 2 * math.pi * round((prev - a) / (2 * math.pi))
        out.append((at, a))
        prev = a
    return out


# ---------------------------------------------------------------- the performance: every channel, springs integrated

# head: (time, yaw relative to the body (+ toward the cat's left, our right), pitch (+ down), roll (+ tilt), speed)
LOOKS = [
    (0.0, 0.0, 0.05, 0.0, 4), (3.2, 0.7, 0.0, 0.0, 5), (4.6, 0.7, -0.05, 0.08, 4), (4.9, -0.75, 0.0, 0.0, 7),
    (6.1, -0.25, -0.45, -0.1, 6), (7.1, 0.05, 0.45, 0.0, 5), (8.4, 0.0, 0.35, 0.0, 4),
    (9.0, 0.0, 0.65, 0.0, 16), (10.1, 0.1, -0.05, 0.12, 8), (10.8, 0.0, 0.25, 0.0, 6),
    (11.5, 0.0, 0.5, 0.0, 12), (11.85, 0.15, 0.25, 0.4, 7), (12.4, 0.0, 0.2, 0.0, 6),
    (13.0, 0.0, 0.45, 0.0, 12), (13.4, 0.5, -0.05, 0.1, 7), (14.0, 0.0, 0.15, 0.0, 8),
    (15.0, 0.0, 0.12, 0.0, 8), (22.0, 0.0, 0.1, 0.0, 6), (22.3, 0.0, 0.2, 0.0, 6),
    (23.2, 0.0, -0.15, 0.0, 6), (23.9, 0.0, 0.3, 0.0, 6),
    (24.6, 0.0, 0.2, 0.0, 6), (25.0, 0.85, -0.15, 0.0, 4), (26.2, -0.85, -0.25, 0.0, 3.5), (27.2, 0.0, -0.02, 0.0, 5),
    (28.6, 0.0, 0.05, 0.15, 3), (29.6, 0.0, 0.0, 0.0, 4),
]
BLINKS = [2.2, 5.6, 7.6, 10.4, 13.6, 20.1, 25.6, 28.3]
SURPRISE = [(8.95, 1.4), (11.45, 0.6), (24.25, 1.0)]  # (time, how much): eyes wide, ears up
EAR_FLICKS = [(3.9, 1), (6.0, -1), (12.0, 1), (26.8, -1)]
HAPPY = [(12.95, 0.6), (14.6, 1.0), (22.0, 0.3), (24.4, 1.0), (27.5, 0.7)]  # (time, tail up and curled, held)
WIGGLE = (22.6, 23.15)  # the pounce wiggle: hips and tail


def _keyed(keys, t, i):
    """keys[(time, v...)] held, stepped: the target at t."""
    v = keys[0][i]
    for k in keys:
        if k[0] <= t:
            v = k[i]
    return v


def _bump(t, at, rise, fall):
    if t < at - rise or t > at + fall:
        return 0.0
    return _smooth((t - (at - rise)) / rise) if t < at else 1 - _smooth((t - at) / fall)


def _simulate():
    """Every channel at every frame, the springs (facing, head, the landing's give, nod, ears, tail) integrated at 240 Hz."""
    sub = 8
    dt = 1 / (FPS * sub)
    heads = _heading_targets()
    st = dict(head=0.0, headv=0.0, yaw=0.0, yawv=0.0, hy=0.0, hyv=0.0, hp=0.0, hpv=0.0, hr=0.0, hrv=0.0, give=0.0, givev=0.0, ear=0.0, earv=0.0, tail=0.0, tailv=0.0, tailz=0.0, tailzv=0.0, nod=0.0, nodv=0.0,
              lag=0.0, lagv=0.0, sq=0.0, sqv=0.0)
    frames = []
    landed = set()
    vy_was = 0.0
    for f in range(N + 1):
        for s in range(sub):
            t = (f * sub + s) * dt
            # facing: a critically damped spring to the heading, which itself eases to each new heading (a stepped
            # target starts the turn with a lurch)
            st["headv"] += (196.0 * (_keyed(heads, t, 1) - st["head"]) - 28.0 * st["headv"]) * dt
            st["head"] += st["headv"] * dt
            target = st["head"]
            # in the air it can twist round quickly; on the floor it turns only as fast as its feet can step round
            air = hop_at(t) is not None
            w = 9.0 if air else 6.0
            st["yawv"] += (w * w * (target - st["yaw"]) - 2 * w * st["yawv"]) * dt
            if not air and abs(st["yawv"]) > 2.2:
                # over the feet's pace: braked hard but smoothly (a clamp is a jolt when it lands spinning)
                st["yawv"] -= math.copysign(30.0 * (abs(st["yawv"]) - 2.2), st["yawv"]) * dt
            st["yaw"] += st["yawv"] * dt
            # head: to the look, at the look's speed
            ly, lp, lr, lw = _keyed(LOOKS, t, 1), _keyed(LOOKS, t, 2), _keyed(LOOKS, t, 3), _keyed(LOOKS, t, 4)
            for k, tv in (("hy", ly), ("hp", lp), ("hr", lr)):
                st[k + "v"] += (lw * lw * (tv - st[k]) - 2 * lw * st[k + "v"]) * dt
                st[k] += st[k + "v"] * dt
            # a landing kicks the springs: the give in the legs, the nod, the ears, the tail
            for h in HOPS:
                if h["t1"] <= t and id(h) not in landed:
                    landed.add(id(h))
                    v = 4 * h["h"] / (h["t1"] - h["t0"])  # speed coming down
                    st["givev"] += v * (0.55 if h["big"] else 0.4)
                    st["nodv"] += v * 2.2
                    st["earv"] += v * 1.0
                    st["tailv"] += v * 0.8
            # inertia: every change in the body's vertical speed (the push off, the landing, the drop into a crouch) is
            # felt late by what hangs off it. The head lags and catches up (follow-through), the ears and tail more so
            # and slower (overlapping action), and the body squashes or stretches with it.
            _, vy = lift(t)
            c0, _u = antic(t)
            c1, _u = antic(t + dt)
            vy -= (c1 - c0) / dt  # the crouch is the body going down
            dv = vy - vy_was
            vy_was = vy
            st["lagv"] += dv * 2.4
            st["earv"] += dv * 3.0
            st["tailv"] += dv * 2.2
            st["sqv"] += dv * 0.9
            for k, kk, cc in (("give", 260, 22), ("nod", 120, 9), ("ear", 90, 6), ("tail", 40, 4.5), ("tailz", 30, 3), ("lag", 110, 11), ("sq", 220, 15)):
                st[k + "v"] += (-kk * st[k] - cc * st[k + "v"]) * dt
                st[k] += st[k + "v"] * dt
        frames.append(dict(st))
    _feet(frames)
    return frames


def home(x, z, yaw, foot, ahead=0.0):
    """Where a foot belongs on the floor (raea x, z) under its hip, for the cat at (x, z) facing yaw; `ahead` m further
    along the way it faces."""
    lx, fw = FEET[foot]
    fw += ahead
    return (x + lx * math.cos(yaw) + fw * math.sin(yaw), z - lx * math.sin(yaw) + fw * math.cos(yaw))


STEP = 0.12  # s, a step's length in time
REACH = 0.045  # m a foot may be from where it belongs before it steps


def _feet(frames):
    """The feet in the world, every frame: planted until the body has moved off them, then a step (lifted on an arc,
    slow out of the old spot and slow into the new, landing a little past where it belongs, where the body is going);
    in the air, under their hips, reaching and tucking with the hop; on landing, planted where they come down."""
    planted = {}
    stepping = {}  # foot: (from, to, t0)
    rested = {}  # foot: when it last came down (a foot doesn't step again at once: the others get their turn)
    last = {}  # foot: where it was the frame before
    launch = {}  # foot: where it left from, this hop (planted, or partway through a step)
    for f, st in enumerate(frames):
        t = f / FPS
        x, _, z = root(t)
        yaw = st["yaw"]
        h = hop_at(t)
        feet = {}
        # the foot furthest from where it belongs goes first
        order = sorted(FEET, key=lambda k: -math.hypot(*(a - b for a, b in zip(home(x, z, yaw, k), planted.get(k, home(x, z, yaw, k))))))
        for k in order:
            want = home(x, z, yaw, k)
            if k not in planted:
                planted[k] = want
            if h is not None:
                stepping.pop(k, None)
                launch.setdefault(k, last.get(k, (planted[k][0], 0.0, planted[k][1])))
                # in the air: under its hip, the front feet reaching ahead and the hind trailing, tucked up at the top
                u = (t - h["t0"]) / (h["t1"] - h["t0"])
                y, _ = lift(t)
                reach = math.sin(math.pi * u) ** 0.7 * (1.0 if h["big"] else 0.6)
                tuck = math.sin(math.pi * min(1, u * 1.4)) * 0.5
                fw = (0.07 if k[0] == "F" else -0.09) * reach
                hx, hz = home(x, z, yaw, k, fw)
                air = (hx, y + 0.06 * tuck, hz)
                # pushing off, a foot leaves the floor as the legs straighten, not at once: the front feet first, the
                # hind feet last (they do the pushing)
                w = _smooth((t - h["t0"]) / (0.14 if k[0] == "F" else 0.2))
                p = launch[k]
                feet[k] = tuple(a + (b - a) * w for a, b in zip(p, air))
                # it comes down where it is: planted there
                planted[k] = (air[0], air[2]) if u > 0.5 else planted[k]
                continue
            if k in stepping:
                a, b, s0 = stepping[k]
                u = (t - s0) / STEP
                if u >= 1:
                    planted[k] = b
                    rested[k] = t
                    del stepping[k]
                else:
                    e = _smooth(u)
                    feet[k] = (a[0] + (b[0] - a[0]) * e, 0.035 * math.sin(math.pi * u) ** 0.8, a[1] + (b[1] - a[1]) * e)
                    continue
            p = planted[k]
            off = math.hypot(want[0] - p[0], want[1] - p[1])
            if off > REACH and t - rested.get(k, -1.0) > 0.05 and not any(n in stepping for n in NEIGHBOURS[k]):
                # step to where it will belong, the way the body is turning: past the home by half the miss
                to = (want[0] + (want[0] - p[0]) * 0.5, want[1] + (want[1] - p[1]) * 0.5)
                stepping[k] = (p, to, t)
            feet[k] = (p[0], 0.0, p[1])
        if h is None:
            launch.clear()
        last.update(feet)
        st["feet"] = feet


_FRAMES = None


# channels whose targets step (a new heading, the push off, the landing), each through a critically damped spring
# (rad/s): the pose gets there in a few frames instead of in one
EASED = dict(turn_look=9.0, spine=7.0, head_pitch=22.0, pitch=28.0, crouch=32.0, stretch=30.0)
_EASED = None


def perform(t):
    """The cat's channels at t: root, facing, crouch, body pitch, legs, head, ears, tail, eyes, breath."""
    global _EASED
    if _EASED is None:
        sub = 8
        dt = 1 / (FPS * sub)
        cur = {k: _raw(0.0)[k] for k in EASED}
        vel = {k: 0.0 for k in EASED}
        _EASED = []
        for f in range(N + 1):
            for s in range(sub):
                raw = _raw((f * sub + s) * dt)
                for k, w in EASED.items():
                    vel[k] += (w * w * (raw[k] - cur[k]) - 2 * w * vel[k]) * dt
                    cur[k] += vel[k] * dt
            _EASED.append(dict(cur))
    raw = _raw(t)
    out = dict(raw, **_EASED[max(0, min(N, round(t * FPS)))])
    out["head_yaw"] += out["turn_look"] - raw["turn_look"]
    return out


def _raw(t):
    global _FRAMES
    if _FRAMES is None:
        _FRAMES = _simulate()
    f = max(0, min(N, round(t * FPS)))
    st = _FRAMES[f]
    x, y, z = root(t)
    out = dict(x=x, y=y, z=z, yaw=st["yaw"])
    # crouch: before each takeoff, down (slow then quick); pushing off, up past standing; the landing's give
    crouch, pitch, front, hind, tuck, reach = 0.0, 0.0, 0.0, 0.0, 0.0, 0.0
    for h in HOPS:
        a0, t0, t1 = h["t0"] - h["antic"], h["t0"], h["t1"]
        if a0 <= t < t0:
            u = (t - a0) / h["antic"]
            crouch = h["crouch"] * _smooth(u) ** 1.5
            pitch = 0.08 * _smooth(u) * (2.0 if h["big"] else 1.0)  # leaning in
            if h["big"]:
                pitch = -0.1 * _smooth(u)  # haunches up, chest down: a pounce
            hind = -0.04 * _smooth(u)  # hind feet come under
        elif t0 <= t < t1:
            u = (t - t0) / (t1 - t0)
            crouch = -0.03 * (1 - _smooth(u / 0.25))  # the push: legs long
            pitch = (-0.28 if h["big"] else -0.15) * (1 - u) + (0.25 if h["big"] else 0.12) * u  # nose up, then down
            reach = math.sin(math.pi * u) ** 0.7 * (1.0 if h["big"] else 0.6)  # front reach forward, hind trail
            tuck = math.sin(math.pi * min(1, u * 1.4)) * 0.5
    # squash and stretch: squashed into the crouch and by the landing, stretched by speed up or down (the spring
    # carries it through; volume kept by the rig)
    _, vy = lift(t)
    out["stretch"] = max(-0.14, min(0.14, 0.05 * abs(vy) - 0.9 * crouch - 0.6 * max(0.0, st["give"]) + st["sq"] * 0.25))
    out["feet"] = st["feet"]
    # turning: the head looks into the turn first, the chest leads the hips (the spine bends into it)
    ahead = _keyed(_heading_targets(), t + 0.2, 1) - st["yaw"]
    out["turn_look"] = max(-0.6, min(0.6, ahead * 0.55))
    out["spine"] = max(-0.3, min(0.3, (_keyed(_heading_targets(), t, 1) - st["yaw"]) * 0.45))
    out["crouch"] = crouch + max(-0.04, st["give"]) * 1.0
    out["pitch"] = pitch + st["give"] * 0.8
    out["reach"], out["tuck"], out["hind"] = reach, tuck, hind
    # head
    out["head_yaw"], out["head_pitch"], out["head_roll"] = st["hy"] + out["turn_look"], st["hp"] + st["nod"] * 0.6 + st["lag"] * 0.35, st["hr"]
    # eyes: blinks, wide on a surprise
    blink = max([_bump(t, b, 0.06, 0.1) for b in BLINKS] + [0.0])
    wide = max([s * _bump(t, at, 0.05, 1.4) for at, s in SURPRISE] + [0.0])
    out["blink"], out["wide"] = blink, wide
    # ears: alert with a surprise, flat in a crouch, a flick now and then, the landing's bounce
    flick = sum(sgn * _bump(t, at, 0.05, 0.18) for at, sgn in EAR_FLICKS)
    # the ears stay up: a twitch back as it crouches or is startled, never flat
    out["ears"] = -0.12 * wide + 0.35 * (crouch if crouch > 0 else 0) + st["ear"] * 0.18
    out["ear_flick"] = flick * 0.45
    # tail: idle sway, up and curled when happy, lashing in the wiggle, the spring
    happy = 0.0
    for at, v in HAPPY:
        if t >= at:
            happy = v * _smooth((t - at) / 0.6)
    lash = 1.0 if WIGGLE[0] <= t < WIGGLE[1] else 0.0
    out["tail_lift"] = happy
    out["tail_sway"] = 0.14 * math.sin(t * 2 * math.pi / 2.4) * (1 - 0.5 * happy) + lash * 0.35 * math.sin(t * 2 * math.pi * 3.2)
    out["tail_bend"] = st["tail"] * 0.8 + 0.25 * (crouch > 0.02) * (1 - happy)
    out["wiggle"] = lash * 0.06 * math.sin(t * 2 * math.pi * 3.2)
    out["breath"] = math.sin(t * 2 * math.pi / 1.6)
    # feet shuffle while it turns on the spot
    out["shuffle"] = min(1.0, abs(st["yawv"]) * 0.25)
    out["shuffle_phase"] = st["yaw"] * 6.0
    return out


def light(t):
    """The key light coming up out of the dark, and the fade at the end (0..1)."""
    return _smooth(t / 2.5) * (1 - _smooth((t - 29.3) / 0.7))


if __name__ == "__main__":
    here = os.path.dirname(os.path.abspath(__file__))
    data = dict(
        fps=FPS, frames=N, bpm=BPM, tile=TILE,
        landings=landings(),
        # the cat's root and facing at every frame, for the camera to follow
        root=[[round(v, 4) for v in (lambda p: (p["x"], p["y"], p["z"], p["yaw"]))(perform(f / FPS))] for f in range(N + 1)],
    )
    json.dump(data, open(os.path.join(here, "story.json"), "w"), separators=(",", ":"))
    print("story.json:", len(data["landings"]), "landings,", N + 1, "frames")
