"""What each motion utility is, for every backend: a part's utilities as tracks of (column, {frame: value}, ease), with
x and y as offsets from where the part is placed (each backend adds its own base).

  enter-fade    opacity 0 to 1
  enter-rise    up from 3% of the frame's height below, fading in
  enter-slide   in from 6% of the frame's width to the left, fading in
  enter-pop     scale 0.85, 1.05, 1
  enter-bounce  scale 0.8, 1.12, 0.96, 1
  settle        a rise or slide overshoots by a sixth and comes back
  exit-fade     opacity 1 to 0 by the part's end
  dur-sm|md|lg  Material 3's short4, medium2, long2 (200, 300, 500 ms); md when none is said
  ease-*        Material 3's curves: standard, emphasized (its decelerate), decelerate, accelerate
"""
DUR = {"sm": 0.2, "md": 0.3, "lg": 0.5}
# Material 3's easing curves, as cubic beziers
BEZIER = {"standard": (0.2, 0.0, 0.0, 1.0), "emphasized": (0.05, 0.7, 0.1, 1.0), "decelerate": (0.0, 0.0, 0.0, 1.0), "accelerate": (0.3, 0.0, 1.0, 1.0)}


def plan(tl, p):
    """the part's motion tracks, or [] when it asks for none"""
    u = p.motion or []
    enter = next((x[6:] for x in u if x.startswith("enter-")), None)
    exitf = "exit-fade" in u
    if not enter and not exitf:
        return []
    d = max(2, round(DUR.get(next((x[4:] for x in u if x.startswith("dur-")), "md")) * tl.fps))
    ease = next((x[5:] for x in u if x.startswith("ease-")), "emphasized")
    settle = "settle" in u
    s0, e = p.start, p.end
    tracks = []
    if enter in ("fade", "rise", "slide"):
        tracks.append(["opacity", {s0: 0, s0 + d: 1}])
    if enter == "rise":
        dy = round(tl.height * 0.03)
        tracks.append(["y", {s0: dy, s0 + round(d * 0.7): -dy / 6, s0 + d: 0} if settle else {s0: dy, s0 + d: 0}])
    if enter == "slide":
        dx = round(tl.width * 0.06)
        tracks.append(["x", {s0: -dx, s0 + round(d * 0.7): dx / 6, s0 + d: 0} if settle else {s0: -dx, s0 + d: 0}])
    if enter == "pop":
        tracks.append(["scale", {s0: 0.85, s0 + round(d * 0.6): 1.05, s0 + d: 1}])
    if enter == "bounce":
        tracks.append(["scale", {s0: 0.8, s0 + round(d * 0.4): 1.12, s0 + round(d * 0.7): 0.96, s0 + d: 1}])
    if exitf:
        out = {max(s0 + d, e - d): 1, e: 0}
        op = next((t for t in tracks if t[0] == "opacity"), None)
        if op:
            op[1] = {**op[1], **out}
        else:
            tracks.append(["opacity", out])
    return [(c, k, ease) for c, k in tracks]
