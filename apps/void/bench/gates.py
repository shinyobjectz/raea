#!/usr/bin/env python3
"""The no-V8 goal's gates measured in void's own window (apps/void/bench/probe.rs answers; the offscreen gates are
sdk/react/bench/games/gate.sh, timelines/check.sh and video/gate.sh).

Runs against a void already launched as a test instance beside the person's (its own home, ports and workspace, the
window behind and never the key window), built with the probe and started with RAEA_VOID_PROBE=1:

  RAEA_VOID_HOME=$S/home RAEA_VOID_MCP_PORT=7467 RAEA_VOID_PROBE=1 RAEA_VOID_PROBE_PORT=7468 \\
    RAEA_VOID_BACKGROUND=1 RAEA_AUDIO_MUTE=1 apps/void/target/release/void $S/proj &

The founder, 2026-10-18: test windows must never show in front of them. RAEA_VOID_BACKGROUND no longer floats the window
(RAEA_VOID_PROBE_FLOAT=1 does, for a spare screen only); a window macOS doesn't draw (covered, minimized, off screen)
pauses the stage (stage_native.rs), so the in-window numbers need a visible window the person agreed to.

$S/proj/objects/<slug>/ holds each object (edit.tsx and a space.json with its kind). Then:

  apps/void/bench/gates.py --out results.json [--seconds 300] [--timeline-seconds 30] game:pong game:platformer \\
    timeline:titles timeline:cj-reel

For each object: opened through void's MCP (act open_object), played SECONDS with the probe's reset before and report
after (games with random key changes from the probe's input thread), under the shared measurement lock
(/tmp/raea-measure.lock, held under 10 minutes) once the 1-minute load average is under 6 (waiting up to 4 minutes; the load is recorded next
to every number either way). Then render equals play: the stage's page read back at frames (a timeline paused and
sought there; a game where it is, with the keys each frame carried) against `raea-motion render --frame N` with
RAEA_READBACK=rgba, compared by ffmpeg's SSIM.
"""
import argparse, json, os, re, subprocess, sys, time, urllib.request

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
        return http(f"{self.probe}{route}{'?' + qs if qs else ''}", timeout=120)


def load1():
    return float(os.getloadavg()[0])


def lock(log):
    while True:
        try:
            os.mkdir(LOCK)
            break
        except FileExistsError:
            time.sleep(5)
    t = time.time()
    while load1() >= 6 and time.time() - t < 240:
        log(f"  load {load1():.1f}, waiting for < 6")
        time.sleep(15)
    return load1()


def unlock():
    try:
        os.rmdir(LOCK)
    except OSError:
        pass
    # holds are capped at 10 minutes, and a hold is followed by 60 s before the next (the shared protocol)
    time.sleep(60)


def ssim(a, b):
    out = subprocess.run(["ffmpeg", "-v", "error", "-i", a, "-i", b, "-filter_complex",
                          "[0]format=rgb24[a];[1]format=rgb24[b];[a][b]ssim=stats_file=-", "-f", "null", "-"],
                         capture_output=True, text=True).stdout + ""
    m = re.findall(r"All:([0-9.]+)", out)
    if not m:
        r = subprocess.run(["ffmpeg", "-i", a, "-i", b, "-lavfi", "ssim", "-f", "null", "-"], capture_output=True, text=True)
        m = re.findall(r"All:([0-9.]+)", r.stderr)
    return float(m[-1]) if m else None


def size_of(png):
    r = subprocess.run(["sips", "-g", "pixelWidth", "-g", "pixelHeight", png], capture_output=True, text=True).stdout
    w = re.search(r"pixelWidth: (\d+)", r)
    h = re.search(r"pixelHeight: (\d+)", r)
    return (int(w.group(1)), int(h.group(1))) if w and h else None


def comp_fps(edit):
    t = open(edit).read()
    m = re.search(r"fps=\{(\d+)\}", t) or re.search(r"const FPS = (\d+)", t)
    return int(m.group(1))


def wait_playing(v, slug, log, timeout=180):
    """Until the stage shows the object and its frame moves (a timeline bakes first)."""
    t = time.time()
    last = None
    while time.time() - t < timeout:
        r = v.p("/report")
        f = r.get("stage_frame", -1)
        # the object before may have left its error until the stage takes the new one
        if r.get("stage_error") and time.time() - t > 15:
            raise RuntimeError(f"{slug}: {r['stage_error']}")
        if last is not None and f > last and f > 10:
            return time.time() - t
        last = f
        time.sleep(1)
    raise RuntimeError(f"{slug}: the stage never played")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("objects", nargs="+", help="game:SLUG or timeline:SLUG")
    ap.add_argument("--proj", required=True, help="the test instance's workspace (objects/<slug>/edit.tsx)")
    ap.add_argument("--out", required=True)
    ap.add_argument("--shots", default=None, help="folder for the snapshots and renders")
    ap.add_argument("--seconds", type=float, default=300)
    ap.add_argument("--timeline-seconds", type=float, default=30)
    ap.add_argument("--mcp-port", type=int, default=7467)
    ap.add_argument("--probe-port", type=int, default=7468)
    ap.add_argument("--pid", type=int, default=None, help="the test instance's pid, for RSS from outside too")
    ap.add_argument("--no-measure", action="store_true", help="render equals play only")
    a = ap.parse_args()
    shots = a.shots or os.path.join(os.path.dirname(os.path.abspath(a.out)), "shots")
    os.makedirs(shots, exist_ok=True)
    v = Void(a.mcp_port, a.probe_port)
    log = lambda s: print(s, flush=True)
    results = {"started": time.strftime("%Y-%m-%d %H:%M:%S"), "objects": {}}
    if os.path.exists(a.out):
        results = json.load(open(a.out))
    for spec in a.objects:
        kind, slug = spec.split(":", 1)
        edit = os.path.join(a.proj, "objects", slug, "edit.tsx")
        res = results["objects"].setdefault(slug, {"kind": kind})
        log(f"{slug}: opening")
        v.act("open_object", slug)
        res["opened_s"] = round(wait_playing(v, slug, log), 1)
        time.sleep(3)
        if not a.no_measure:
            secs = a.seconds if kind == "game" else a.timeline_seconds
            load_before = lock(log)
            try:
                load_start = load1()
                v.p("/reset")
                if kind == "game":
                    keys = "ArrowRight,ArrowRight+ArrowUp,ArrowRight," if slug == "platformer" else ",ArrowUp,ArrowDown"
                    v.p("/input", keys=keys, seconds=secs)
                ps = []
                t = time.time()
                while time.time() - t < secs:
                    time.sleep(1)
                    if a.pid:
                        r = subprocess.run(["ps", "-o", "rss=", "-p", str(a.pid)], capture_output=True, text=True).stdout.strip()
                        ps.append(int(r) // 1024 if r else None)
                rep = v.p("/report")
                rep["load1"] = {"at_lock": round(load_before, 2), "start": round(load_start, 2), "end": round(load1(), 2)}
                if ps:
                    rep["ps_rss_mb_each_second"] = ps
            finally:
                unlock()
            res["measured"] = rep
            st, w, ip = rep["stage"], rep["window"], rep["input_to_present_ms"]
            mem = [m[1] for m in rep["mem_each_second"]]
            log(f"  {secs:.0f} s at {rep['display_hz']:.0f} Hz, comp {rep['composition_fps']:.0f} fps, load {rep['load1']}: "
                f"stage work p99 {st['work_ms']['p99']:.2f} ms, missed {st['missed']}/{st['painted']}; "
                f"window render p99 {w['render_ms']['p99']:.2f} ms, gap p99 {w['gap_ms']['p99']:.2f}, missed {w['missed']}/{w['rendered']}; "
                f"input p99 {ip['p99']:.1f} ms ({ip['p99_frames']:.2f} frames, {ip['timed']}/{ip['sent']}); "
                f"input to stage paint p99 {ip.get('to_paint_p99', 0):.1f} ms; "
                f"rss {mem[0] if mem else 0:.0f} -> {mem[-1] if mem else 0:.0f} MB (max {max(mem) if mem else 0:.0f})")
        # render equals play
        pairs = []
        if kind == "timeline":
            n = None
            for f in [0, 45, 120, 300, 450, 599]:
                play_png = os.path.join(shots, f"{slug}-void-{f}.png")
                s = v.p("/snapshot", path=play_png, frame=f)
                if "error" in s:
                    pairs.append({"frame": f, "error": s["error"]})
                    continue
                pairs.append({"frame": s["frame"], "play": play_png})
            v.act("play")
            inputs = None
        else:
            # a game where it is: its frame, and the keys every frame carried, for the render
            play_png = os.path.join(shots, f"{slug}-void-now.png")
            s = v.p("/snapshot", path=play_png)
            inputs = os.path.join(shots, f"{slug}-input.json")
            v.p("/inputlog", path=inputs)
            pairs.append({"frame": s.get("frame"), "play": play_png, "error": s.get("error")})
        for pr in pairs:
            if pr.get("error") or pr.get("play") is None:
                continue
            f = pr["frame"]
            ren = os.path.join(shots, f"{slug}-render-{f}.png")
            cmd = [MOTION, "render", edit, "-o", ren, "--frame", str(f)] + (["--input", inputs] if inputs else [])
            r = subprocess.run(cmd, env={**os.environ, "RAEA_READBACK": "rgba"}, capture_output=True, text=True)
            if r.returncode != 0 or not os.path.exists(ren):
                pr["error"] = f"render failed: {(r.stderr or r.stdout)[-300:]}"
                continue
            pr["render"] = ren
            ps_, rs_ = size_of(pr["play"]), size_of(ren)
            pr["sizes"] = {"void": ps_, "render": rs_}
            if ps_ != rs_:
                pr["note"] = "sizes differ (a responsive game laid out at void's stage box); compared after scaling void's page"
                scaled = pr["play"].replace(".png", f"-scaled.png")
                subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", pr["play"], "-vf", f"scale={rs_[0]}:{rs_[1]}", scaled])
                pr["ssim"] = ssim(ren, scaled)
            else:
                pr["ssim"] = ssim(ren, pr["play"])
        res["render_vs_play"] = pairs
        # void against the offscreen playtest at void's own page size, the same engine path outside void: a timeline's
        # frames from its bake; a game opened again and played with no keys, its frame there against the same frame of
        # `raea-motion play --size` (a responsive game's page is void's stage box, which a render can't be laid out at)
        same = []
        if kind == "timeline":
            fps = comp_fps(edit)
            ok = [p for p in pairs if p.get("play")]
            snaps = []
            for p in ok:
                p["offscreen"] = os.path.join(shots, f"{slug}-offscreen-{p['frame']}.png")
                snaps += ["--snapshot", str(p["frame"] / fps), p["offscreen"]]
            last = max(p["frame"] for p in ok) / fps + 0.2 if ok else 1
            subprocess.run([MOTION, "play", edit, "--baked", "--offscreen", "--seconds", str(last)] + snaps,
                           env={**os.environ, "RAEA_AUDIO_MUTE": "1"}, capture_output=True, text=True)
            for p in ok:
                same.append({"frame": p["frame"], "ssim": ssim(p["play"], p["offscreen"]) if os.path.exists(p["offscreen"]) else None})
        else:
            other = next((o.split(":", 1)[1] for o in a.objects if o.startswith("game:") and not o.endswith(":" + slug)), None)
            if other:
                v.act("open_object", other)
                wait_playing(v, other, log)
            v.act("open_object", slug)
            t = time.time()
            while v.p("/report").get("stage_frame", 0) < 240 and time.time() - t < 60:
                time.sleep(0.5)
            vp = os.path.join(shots, f"{slug}-void-nokeys.png")
            s = v.p("/snapshot", path=vp)
            if "error" not in s:
                fps = comp_fps(edit)
                op = os.path.join(shots, f"{slug}-offscreen-{s['frame']}.png")
                subprocess.run([MOTION, "play", edit, "--offscreen", "--size", f"{s['w']}x{s['h']}", "--seconds",
                                str(s["frame"] / fps + 0.2), "--snapshot", str(s["frame"] / fps), op],
                               env={**os.environ, "RAEA_AUDIO_MUTE": "1"}, capture_output=True, text=True)
                same.append({"frame": s["frame"], "page": [s["w"], s["h"]], "void": vp, "offscreen": op,
                             "ssim": ssim(vp, op) if os.path.exists(op) else None})
            else:
                same.append({"error": s["error"]})
        res["void_vs_offscreen_play"] = same
        log(f"  void vs offscreen play: " + ", ".join(f"{p.get('frame')}:{p.get('ssim')}" for p in same))
        log(f"  render vs void: " + ", ".join(f"{p['frame']}:{p.get('ssim') if p.get('ssim') is not None else p.get('error', '?')[:60]}" for p in pairs))
        json.dump(results, open(a.out, "w"), indent=1)
    results["finished"] = time.strftime("%Y-%m-%d %H:%M:%S")
    json.dump(results, open(a.out, "w"), indent=1)


if __name__ == "__main__":
    import urllib.parse
    main()
