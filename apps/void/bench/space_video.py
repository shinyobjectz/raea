#!/usr/bin/env python3
"""The Video space, headless (void.feature: A space is a setup): today's way of working on a video rebuilt as an
ordinary space, its header and player in TSX. A five-second motion object on the stage, switched to Video. Checks:

  - open:    the header and the player painted, and the player knows the video's length (it draws 0:05)
  - pause:   the agent's player.pause stops the playhead (the probe's /report: stage_frame holds)
  - seek:    player.seek {seconds: 2} puts it at frame 60
  - scrub:   a press three quarters along the player's track (act panel_press) puts it three quarters in
  - play:    a press on the play button starts it again (the playhead moves)
  - header:  under void's shell (apps/void/shell.feature) the space's header is a toolbar over the stage, under the shell's
             header, as Vector's tools are (the stage's box 8 points lower under Vector's 52-point tools than under
             this header's 44), and a press on its Vector chip switches the space

  apps/void/bench/space_video.py [--out DIR] [--mcp-port 7497] [--probe-port 7498]

Build first: void (lane build -- cargo build --release in void/).
"""
import argparse, json, os, signal, subprocess, sys, tempfile, time

sys.path.insert(0, os.path.dirname(__file__))
from space_panels import VOID, Void, http, switch  # noqa: E402

FPS, FRAMES = 30, 150
CLIP = """import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "@raea/motion";
function Clip() {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "#101828" }}>
      <div style={{ position: "absolute", left: 80 + f * 7, top: 300, width: 120, height: 120, borderRadius: 24, background: "#2b6cff" }} />
    </AbsoluteFill>
  );
}
registerRoot(() => <Composition id="edit" component={Clip} width={1280} height={720} fps={%d} durationInFrames={%d} />);
""" % (FPS, FRAMES)


def until(what, seconds, f):
    """f() until it returns without raising, for at most `seconds`; its value."""
    last = None
    for _ in range(int(seconds * 4)):
        try:
            return f()
        except Exception as e:
            last = e
        time.sleep(0.25)
    raise RuntimeError(f"{what} in {seconds} s: {last}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=None)
    ap.add_argument("--mcp-port", type=int, default=7497)
    ap.add_argument("--probe-port", type=int, default=7498)
    a = ap.parse_args()
    out = a.out or tempfile.mkdtemp(prefix="space-video-")
    home, proj = os.path.join(out, "home"), os.path.join(out, "workspace")
    obj = os.path.join(proj, "objects", "clip")
    os.makedirs(obj, exist_ok=True)
    open(os.path.join(obj, "edit.tsx"), "w").write(CLIP)
    json.dump({"name": "Clip", "slug": "clip", "created": 0, "kind": "motion"}, open(os.path.join(obj, "space.json"), "w"))
    env = {**os.environ, "RAEA_VOID_HOME": home, "RAEA_VOID_MCP_PORT": str(a.mcp_port), "RAEA_VOID_PROBE": "1",
           "RAEA_VOID_PROBE_PORT": str(a.probe_port), "RAEA_VOID_HEADLESS": "1", "RAEA_VOID_BACKGROUND": "1", "RAEA_AUDIO_MUTE": "1"}
    log = open(os.path.join(out, "void.log"), "w")
    proc = subprocess.Popen(["nice", "-n", "10", VOID, proj], env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    v, results, ok = Void(a.mcp_port), {}, True
    report = lambda: http(f"http://127.0.0.1:{a.probe_port}/report")
    frame = lambda: report()["stage_frame"]
    try:
        until("void's MCP answering", 90, lambda: v.call("state", {}))
        v.act("open_object", "clip")
        until("the clip playing", 30, lambda: report()["stage_frames"] == FRAMES or (_ for _ in ()).throw(RuntimeError(report().get("stage_frames"))))
        switch(v, "video", "Video")
        snaps = until("both panels painting", 60, lambda: [json.loads(v.act("panel_snapshot", {"i": i, "path": os.path.join(out, f"panel-{i}-open.png")})) for i in (0, 1)])
        time.sleep(0.5)
        width = snaps[1]["w"]  # a headless window's scale is 1: the player's width in points
        results["open"] = {"panels": [(s["w"], s["h"]) for s in snaps]}
        v.act("panel_action", {"panel": "player", "action": "pause"})
        time.sleep(0.5)
        f1 = frame()
        time.sleep(0.5)
        f2 = frame()
        results["pause"] = [f1, f2]
        ok &= f1 == f2
        v.act("panel_action", {"panel": "player", "action": "seek", "args": {"seconds": 2}})
        time.sleep(0.5)
        results["seek"] = frame()
        ok &= results["seek"] == 2 * FPS
        # the player's width now (the box settles after the panels first paint)
        width = json.loads(v.act("panel_snapshot", {"i": 1, "path": os.path.join(out, "player-before-scrub.png")}))["w"]
        left, right = 12 + 44 + 56, width - 12 - 56
        x = left + 0.75 * (right - left)
        v.act("panel_press", {"i": 1, "x": x, "y": 24})
        time.sleep(0.5)
        want = round(0.75 * (FRAMES - 1))
        results["scrub"] = {"width": width, "x": round(x, 1), "frame": frame(), "want": want}
        ok &= abs(results["scrub"]["frame"] - want) <= 1
        v.act("panel_snapshot", {"i": 1, "path": os.path.join(out, "player-scrubbed.png")})
        http(f"http://127.0.0.1:{a.probe_port}/snapshot?path={os.path.join(out, 'stage-scrubbed.png')}&as=page", timeout=60)
        v.act("panel_press", {"i": 1, "x": 12 + 22, "y": 24})
        time.sleep(0.2)
        p1 = frame()
        time.sleep(0.6)
        p2 = frame()
        results["play"] = [p1, p2]
        ok &= p2 > p1
        # the stage's box under the space's own header, then under Vector's tools: both under the shell's header, so
        # Vector's 52-point toolbar for this header's 44
        def stage_top():
            rects = [l for l in v.call("logs", {"n": 200}).splitlines() if "native stage: rect" in l]
            return int(rects[-1].split("rect ")[1].split(",")[1].split(" ")[0])
        top_video = stage_top()
        v.act("panel_press", {"i": 0, "x": 8 + 32 + 260 + 3 * 96 + 46, "y": 22})
        heard = until("the header's switch", 20, lambda: [l for l in v.call("logs", {"n": 60}).splitlines() if "Space · Vector" in l] or (_ for _ in ()).throw(RuntimeError("not yet")))
        time.sleep(1)
        top_vector = stage_top()
        results["header"] = {"heard": heard[-1:], "stage_top_video": top_video, "stage_top_vector": top_vector}
        ok &= top_vector - top_video == 52 - 44
    except Exception as e:
        ok = False
        results["error"] = str(e)
    finally:
        if proc.poll() is None:
            os.killpg(proc.pid, signal.SIGTERM)
            try:
                proc.wait(10)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
    results["pass"] = bool(ok)
    json.dump(results, open(os.path.join(out, "result.json"), "w"), indent=2)
    print(json.dumps(results, indent=2))
    print(("PASS" if ok else "FAIL"), out)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
