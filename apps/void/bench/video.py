#!/usr/bin/env python3
"""Video measured inside void (the no-V8 goal's video gate, sdk/react/bench/video/gate.sh, and the re-bake of gate 2,
sdk/react/bench/timelines/check.sh, through void's own stage), against a headless test instance (apps/void/bench/video.sh starts
one: RAEA_VOID_HEADLESS=1, no window on screen; the stage paints into its texture and plays).

  video.py --proj P --out OUT.json [--shots DIR] video:SLUG [video:SLUG ...] [rebake:SLUG]

video:SLUG, for each edit (objects/SLUG/edit.tsx), opened through void's MCP (act open_object):
  - play: from frame 0 through the edit's length at its rate (probe /reset, then /report): frames missed (a gap between
    the stage's paints over 1.5 frames), the mixer's drift max, RSS max;
  - scrub: 60 seeks over the edit in play.rs's shuffled order, each through the stage's own seek (Control::Seek, the
    probe's /seek: from the call to the paint of that frame), paused; p99;
  - seeks against render: frames 3/4, 1/2 and 1/4 of the way (each a jump backwards: the keyframe, then the frames
    after it), void's frame after the seek against `raea-motion render --frame N` (RAEA_READBACK=rgba): the page at
    its own size composited on the GPU as the stage does (as=page), and the stage's texture itself (as=stage) against
    the render shrunk to the stage's size by ffmpeg's area scale, the stage's own filter (quads shrink_into).
  PASS: scrub p99 <= 50 ms, drift max <= 1 ms, 0 missed, RSS <= 1.5 GB, every seek SSIM >= 0.999 at the stage's
  scale (the full-size page's SSIM is reported beside it).

rebake:SLUG (a copy of sdk/react/bench/timelines/titles; it's edited): opened and playing, then edited as an agent edits
it, each written at once:
  1. a component's spring (Cards' damping 12 -> 5, as check.sh retunes it): only the frames where it renders are
     stepped (bake_incremental's count);
  2. a constant outside every component (GLOW, the glow's colour): every frame baked again.
  Each: void's re-bake within 1 s of the write (the probe's /rebakes: when the new bake was swapped in), the playhead
  where it was (the swap's frame = the frame before it) and playing on, and frames after it (seeks) equal to the same
  frames of the edited file opened fresh in this void (baked from nothing; SSIM >= 0.99999). Those frames, and the
  frame playing, against a fresh render of the edited file are reported beside it, not gated: void's text edges
  differ from a render's at every frame, edited or not (titles: 0.9951-0.9987 at the page's size, 0.9985-0.9999 at
  the stage's), which is a gap of void's paint, not of the re-bake.

Timing runs under the shared lock (/tmp/raea-measure.lock, each hold under 10 minutes), the load recorded beside it.
"""
import argparse, json, os, re, subprocess, sys, time, urllib.parse, urllib.request

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
MOTION = os.path.join(ROOT, "sdk", "react", "target", "release", "raea-motion")
LOCK = "/tmp/raea-measure.lock"


def http(url, body=None, timeout=60):
    req = urllib.request.Request(url, data=json.dumps(body).encode() if body is not None else None,
                                 headers={"Content-Type": "application/json"} if body is not None else {})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        t = r.read().decode()
    return json.loads(t) if t else None


class Void:
    def __init__(self, mcp_port, probe_port):
        self.mcp = f"http://127.0.0.1:{mcp_port}/mcp"
        self.probe = f"http://127.0.0.1:{probe_port}"
        self.id = 0

    def act(self, action, value=None):
        self.id += 1
        r = http(self.mcp, {"jsonrpc": "2.0", "id": self.id, "method": "tools/call",
                            "params": {"name": "act", "arguments": {"action": action, "value": value}}})
        res = r.get("result", {})
        if res.get("isError"):
            raise RuntimeError(f"act {action} {value}: {res['content'][0]['text']}")
        return res

    def p(self, route, **q):
        qs = "&".join(f"{k}={urllib.parse.quote(str(v), safe='')}" for k, v in q.items())
        r = http(f"{self.probe}{route}{'?' + qs if qs else ''}", timeout=120)
        if isinstance(r, dict) and "error" in r and route != "/report":
            raise RuntimeError(f"{route} {q}: {r['error']}")
        return r


def load1():
    return round(float(os.getloadavg()[0]), 2)


class Lock:
    """The shared measurement lock, held under 10 minutes (the caller keeps each hold short)."""

    def __enter__(self):
        t = time.time()
        while True:
            try:
                os.mkdir(LOCK)
                break
            except FileExistsError:
                if time.time() - t > 900:
                    raise RuntimeError("the measurement lock stayed held 15 minutes")
                time.sleep(5)
        # once the 1-minute load is under 6 (waiting up to 4 minutes; the load is recorded beside every number)
        w = time.time()
        while load1() >= 6 and time.time() - w < 240:
            time.sleep(10)
        self.t = time.time()
        return self

    def __exit__(self, *a):
        try:
            os.rmdir(LOCK)
        except OSError:
            pass
        if time.time() - self.t > 600:
            print("  (warning: held the lock over 10 minutes)", flush=True)


def ssim(a, b):
    out = subprocess.run(["ffmpeg", "-v", "error", "-i", a, "-i", b, "-filter_complex",
                          "[0]format=rgb24[a];[1]format=rgb24[b];[a][b]ssim=stats_file=-", "-f", "null", "-"],
                         capture_output=True, text=True).stdout
    m = re.findall(r"All:([0-9.]+)", out)
    return float(m[-1]) if m else None


def shrink(src, w, h, out):
    """The render shrunk to the stage's size by area, as the stage shrinks its page (quads shrink_into)."""
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", src, "-vf", f"scale={w}:{h}:flags=area", out], check=True)
    return out


def render(edit, f, out):
    r = subprocess.run(["nice", "-n", "15", MOTION, "render", edit, "-o", out, "--frame", str(f)],
                       env={**os.environ, "RAEA_READBACK": "rgba"}, capture_output=True, text=True)
    if r.returncode != 0 or not os.path.exists(out):
        raise RuntimeError(f"render {edit} --frame {f}: {(r.stderr or r.stdout)[-300:]}")
    return out


def wait_open(v, slug, timeout=300):
    """Until the stage plays this object (a timeline bakes first): its frame moving past 10."""
    t = time.time()
    last = None
    while time.time() - t < timeout:
        r = v.p("/report")
        f = r.get("stage_frame", -1)
        # the object before plays on until the stage takes this one
        if not (r.get("stage_showing") or "").rstrip("/").endswith("/" + slug):
            time.sleep(0.2)
            continue
        if r.get("stage_error") and time.time() - t > 15:
            raise RuntimeError(f"{slug}: {r['stage_error']}")
        if last is not None and f > last and f > 10:
            return round(time.time() - t, 1), r
        last = f
        time.sleep(0.5)
    raise RuntimeError(f"{slug}: the stage never played")


def scrub_order(total, n=60):
    """play.rs --scrub's order: n frames spread over the timeline, shuffled by its xorshift."""
    order = [k * total // n for k in range(n)]
    x = 0x9e3779b97f4a7c15
    M = (1 << 64) - 1
    for i in range(len(order) - 1, 0, -1):
        x ^= (x << 13) & M
        x ^= x >> 7
        x ^= (x << 17) & M
        j = x % (i + 1)
        order[i], order[j] = order[j], order[i]
    return order


def pct(v, p):
    s = sorted(v)
    return s[round((len(s) - 1) * p)] if s else None


def compare(v, edit, f, shots, tag):
    """void's frame f (already on the stage, paused) against the render: the page at its size, the stage's texture
    at its size against the render shrunk by area."""
    page = os.path.join(shots, f"{tag}-void-page-{f}.png")
    stage = os.path.join(shots, f"{tag}-void-stage-{f}.png")
    sp = v.p("/snapshot", path=page, **{"as": "page"})
    ss = v.p("/snapshot", path=stage, **{"as": "stage"})
    if sp["frame"] != f or ss["frame"] != f:
        raise RuntimeError(f"{tag}: the stage showed frame {sp['frame']}/{ss['frame']}, not {f}")
    ren = render(edit, f, os.path.join(shots, f"{tag}-render-{f}.png"))
    small = shrink(ren, ss["w"], ss["h"], os.path.join(shots, f"{tag}-render-{f}-{ss['w']}x{ss['h']}.png"))
    return {"frame": f, "page": [sp["w"], sp["h"]], "stage": [ss["w"], ss["h"]], "ssim_page": ssim(ren, page), "ssim_stage": ssim(small, stage)}


def video(v, proj, slug, shots, log):
    edit = os.path.join(proj, "objects", slug, "edit.tsx")
    res = {}
    log(f"{slug}: opening")
    v.act("open_object", slug)
    res["opened_s"], r = wait_open(v, slug)
    total, fps = int(r["stage_frames"]), float(r["stage_rate"])
    res.update(frames=total, fps=fps, page=r["page"])
    time.sleep(2)
    with Lock():
        res["load"] = {"start": load1()}
        log(f"  {slug}: measuring (load {res['load']['start']})")
        # play: from frame 0 through the edit at its rate
        v.p("/pause")
        v.p("/seek", frame=0)
        v.p("/reset")
        v.p("/play")
        # to its last frames (not past them: the loop back to the start is a jump, which the mixer places again; it's
        # measured apart below)
        time.sleep(max(0.0, (total - 3) / fps - 0.5))
        t0 = time.time()
        while v.p("/report")["stage_frame"] < total - 3 and time.time() - t0 < 5:
            time.sleep(0.05)
        rep = v.p("/report")
        st = rep["stage"]
        res["play"] = {"seconds": rep["seconds"], "painted": st["painted"], "stepped": st["stepped"], "missed": st["missed"],
                       "gap_ms": st["gap_ms"], "work_ms": st["work_ms"], "drift_ms": rep["drift_ms"], "rss_max_mb": rep["rss_max_mb"]}
        # the loop back to the start, and a second after it: the jump's sound placed again (reported, not gated)
        v.p("/reset")
        time.sleep(4 / fps + 1.0)
        rl = v.p("/report")
        res["after_loop"] = {"drift_ms": rl["drift_ms"], "missed": rl["stage"]["missed"]}
        # scrub: 60 shuffled seeks through the stage's own seek, paused
        v.p("/pause")
        time.sleep(0.3)
        ms = []
        for f in scrub_order(total):
            ms.append((f, v.p("/seek", frame=f)["ms"]))
        t = [m[1] for m in ms]
        res["scrub_ms"] = {"seeks": len(t), "p50": pct(t, 0.5), "p99": pct(t, 0.99), "max": max(t),
                           "slowest": sorted(ms, key=lambda m: -m[1])[:5]}
        # seeks against render: backwards jumps; the snapshots now, the renders after the lock
        seeks = []
        for f in (total * 3 // 4, total // 2, total // 4):
            v.p("/seek", frame=f)
            page = os.path.join(shots, f"{slug}-void-page-{f}.png")
            stage = os.path.join(shots, f"{slug}-void-stage-{f}.png")
            sp = v.p("/snapshot", path=page, **{"as": "page"})
            ss = v.p("/snapshot", path=stage, **{"as": "stage"})
            seeks.append((f, page, stage, sp, ss))
        res["load"]["end"] = load1()
    res["seeks"] = []
    for f, page, stage, sp, ss in seeks:
        ren = render(edit, f, os.path.join(shots, f"{slug}-render-{f}.png"))
        small = shrink(ren, ss["w"], ss["h"], os.path.join(shots, f"{slug}-render-{f}-{ss['w']}x{ss['h']}.png"))
        res["seeks"].append({"frame": f, "shown": [sp["frame"], ss["frame"]], "page": [sp["w"], sp["h"]], "stage": [ss["w"], ss["h"]],
                             "ssim_page": ssim(ren, page), "ssim_stage": ssim(small, stage)})
    v.p("/play")
    p, sc = res["play"], res["scrub_ms"]
    drift = p["drift_ms"]["max"] if p["drift_ms"]["samples"] else None
    # gated at the stage's scale (what void shows); the full-size page is reported beside it
    worst = min(s["ssim_stage"] or 0 for s in res["seeks"])
    checks = {
        "scrub p99": (sc["p99"] <= 50, f"{sc['p99']:.1f} ms"),
        "missed": (p["missed"] == 0, f"{p['missed']} of {p['painted']}"),
        "drift max": (drift is not None and drift <= 1, f"{drift:.2f} ms ({p['drift_ms']['samples']} samples)" if drift is not None else "none measured"),
        "rss max": (p["rss_max_mb"] <= 1536, f"{p['rss_max_mb']:.0f} MB"),
        "seeks vs render": (worst >= 0.999 and all(s["shown"] == [s["frame"]] * 2 for s in res["seeks"]),
                            " ".join(f"{s['frame']}:{s['ssim_page']:.6f}/{s['ssim_stage']:.6f}" for s in res["seeks"]) + " (page/stage)"),
    }
    res["pass"] = all(c[0] for c in checks.values())
    log(f"{slug}: {'PASS' if res['pass'] else 'FAIL'}  " + "; ".join(f"{k} {c[1]}{'' if c[0] else ' (over)'}" for k, c in checks.items())
        + f"; after the loop back (not gated): drift max {res['after_loop']['drift_ms']['max']:.2f} ms, missed {res['after_loop']['missed']}"
        + f"  [load {res['load']['start']}-{res['load']['end']}]")
    return res


GLOW = 'const GLOW = "#4b5cff";\n'


def rebake(v, proj, slug, shots, log):
    """An agent's edits to an open timeline, re-baked by void where it plays."""
    folder = os.path.join(proj, "objects", slug)
    edit = os.path.join(folder, "edit.tsx")
    fixture = open(os.path.join(ROOT, "sdk", "react", "bench", "timelines", "titles", "edit.tsx")).read()
    # the glow's colour a constant outside the components (the second edit changes it)
    base = fixture.replace("const FPS = 30;\n", "const FPS = 30;\n" + GLOW).replace('background: "#4b5cff"', "background: GLOW")
    assert GLOW in base and "background: GLOW" in base
    open(edit, "w").write(base)
    log(f"{slug}: opening")
    v.act("open_object", slug)
    res = {"opened_s": wait_open(v, slug)[0], "edits": []}
    time.sleep(1)
    # (the spring: the last `damping: N`, Cards', as check.sh retunes it)
    lines = base.split("\n")
    i = max(k for k, l in enumerate(lines) if re.search(r"damping: \d+", l))
    lines[i] = re.sub(r"damping: \d+", "damping: 5", lines[i])
    spring = "\n".join(lines)
    edits = [("component spring (Cards damping 12 -> 5)", spring, "incremental"),
             ("constant outside the components (GLOW)", spring.replace(GLOW, 'const GLOW = "#ff5c8a";\n'), "full")]
    for name, text, want in edits:
        with Lock():
            load = load1()
            n0 = len(v.p("/rebakes"))
            before = v.p("/report")
            t = time.time()
            with open(edit, "w") as fh:
                fh.write(text)
            got = None
            while time.time() - t < 15:
                rb = v.p("/rebakes")
                if len(rb) > n0:
                    got = rb[-1]
                    break
                time.sleep(0.02)
            if got is None or "error" in got:
                raise RuntimeError(f"{slug}: {name}: no re-bake ({got})")
            write_to_swap = got["at_unix_ms"] - t * 1000
            time.sleep(1.0)
            after = v.p("/report")
        e = {"edit": name, "load": load, "write_to_swap_ms": round(write_to_swap, 1), **got,
             "frame_at_write": before["stage_frame"], "frame_1s_after": after["stage_frame"], "playing_after": after["stage_playing"]}
        # a frame while it plays, then frames reached by seeks, against renders of the edited file
        shown = []
        tag = f"{slug}-e{len(res['edits'])}"
        pp, ps = os.path.join(shots, f"{tag}-playing-page.png"), os.path.join(shots, f"{tag}-playing-stage.png")
        sp = v.p("/snapshot", path=pp, **{"as": "page"})
        ss = v.p("/snapshot", path=ps, **{"as": "stage"})
        ren = render(edit, sp["frame"], os.path.join(shots, f"{tag}-render-{sp['frame']}.png"))
        ren2 = ren if ss["frame"] == sp["frame"] else render(edit, ss["frame"], os.path.join(shots, f"{tag}-render-{ss['frame']}.png"))
        small = shrink(ren2, ss["w"], ss["h"], os.path.join(shots, f"{tag}-render-{ss['frame']}-small.png"))
        shown.append({"frame": sp["frame"], "playing": True, "stage_frame": ss["frame"], "ssim_page": ssim(ren, pp), "ssim_stage": ssim(small, ps)})
        v.p("/pause")
        for f in (599, 450, 300, 120, 45, 0):
            v.p("/seek", frame=f)
            shown.append(compare(v, edit, f, shots, f"{slug}-e{len(res['edits'])}"))
        v.p("/play")
        e["shown"] = shown
        # the same frames from a fresh open of the edited file in this void (baked from nothing): the re-bake must equal it
        fslug = slug + "-fresh"
        fresh = os.path.join(proj, "objects", fslug)
        os.makedirs(fresh, exist_ok=True)
        open(os.path.join(fresh, "edit.tsx"), "w").write(text)
        open(os.path.join(fresh, "space.json"), "w").write(json.dumps({"name": fslug, "slug": fslug, "created": 0, "opened": 0, "hue": 200, "kind": "video"}))
        v.act("open_object", fslug)
        wait_open(v, fslug)
        v.p("/pause")
        same = []
        for s_ in shown[1:]:
            f = s_["frame"]
            v.p("/seek", frame=f)
            p2 = os.path.join(shots, f"{tag}-fresh-page-{f}.png")
            v.p("/snapshot", path=p2, **{"as": "page"})
            same.append((f, ssim(os.path.join(shots, f"{tag}-void-page-{f}.png"), p2)))
        e["rebake_vs_fresh"] = same
        v.act("open_object", slug)
        wait_open(v, slug)
        # at the stage's scale (the full-size page beside it)
        worst = min(s["ssim_stage"] if "ssim_stage" in s else s["ssim_page"] or 0 for s in shown)
        stepped_ok = got["stepped"] < got["frames"] if want == "incremental" else got["stepped"] == got["frames"]
        checks = {
            "re-baked": (write_to_swap <= 1000, f"{write_to_swap:.0f} ms from the write (bundle {got['bundle_ms']:.0f}, bake {got['bake_ms']:.0f}, swap {got['swap_ms']:.1f} ms)"),
            "stepped": (stepped_ok, f"{got['stepped']} of {got['frames']} frames ({want})"),
            "playhead": (got["frame_after"] == got["frame_before"] and after["stage_playing"] and after["stage_frame"] != got["frame_after"],
                         f"swapped at frame {got['frame_before']} -> {got['frame_after']}, {after['stage_frame']} a second later"),
            "equals a fresh bake": (min(x[1] or 0 for x in same) >= 0.99999, " ".join(f"{f}:{x:.6f}" for f, x in same)),
            # (reported, not gated: void's text edges differ from the render's at every frame, re-baked or not)
            "frames vs render": (True, " ".join(f"{s['frame']}:{s['ssim_page']:.6f}" + (f"/{s['ssim_stage']:.6f}" if "ssim_stage" in s else "") for s in shown) + f" (page/stage; at the stage's scale worst {worst:.6f})"),
        }
        e["pass"] = all(c[0] for c in checks.values())
        log(f"{slug}, {name}: {'PASS' if e['pass'] else 'FAIL'}  " + "; ".join(f"{k} {c[1]}{'' if c[0] else ' (over)'}" for k, c in checks.items()) + f"  [load {load}]")
        res["edits"].append(e)
        time.sleep(1)
    open(edit, "w").write(base)
    res["pass"] = all(e["pass"] for e in res["edits"])
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("objects", nargs="+", help="video:SLUG or rebake:SLUG")
    ap.add_argument("--proj", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--shots", default=None)
    ap.add_argument("--mcp-port", type=int, default=7477)
    ap.add_argument("--probe-port", type=int, default=7478)
    a = ap.parse_args()
    # killed (a harness's timeout): the lock is let go on the way out
    import signal
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
    shots = a.shots or os.path.join(os.path.dirname(os.path.abspath(a.out)), "shots")
    os.makedirs(shots, exist_ok=True)
    v = Void(a.mcp_port, a.probe_port)
    log = lambda s: print(s, flush=True)
    results = {"started": time.strftime("%Y-%m-%d %H:%M:%S"), "objects": {}}
    ok = True
    for spec in a.objects:
        kind, slug = spec.split(":", 1)
        try:
            r = video(v, a.proj, slug, shots, log) if kind == "video" else rebake(v, a.proj, slug, shots, log)
        except Exception as e:
            log(f"{slug}: FAIL  {e}")
            r = {"pass": False, "error": str(e)}
        results["objects"][spec] = r
        ok &= r["pass"]
        json.dump(results, open(a.out, "w"), indent=1)
    results["finished"] = time.strftime("%Y-%m-%d %H:%M:%S")
    json.dump(results, open(a.out, "w"), indent=1)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
