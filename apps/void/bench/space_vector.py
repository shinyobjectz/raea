#!/usr/bin/env python3
"""The Vector space, headless (void.feature: A space is a setup): the Vector space's starter drawing as the
object, switched to Vector. Checks:

  - draw:    Rect picked on the tools row (act panel_press), then a drag on the stage (the probe's /pointer, in the
             stage's box points, mapped into the drawing's pixels as the chrome's handlers are) draws a rect: the stage
             gains ink where it was dragged
  - layers:  the layers panel lists the drawing's shapes (the drawing publishes them; void hands them in as void:stage):
             its rows go from 3 to 4
  - agent:   the agent's way in, act panel_action layers.add, draws a red ellipse: the stage gains red
  - remove:  layers.remove row 0 (the topmost, the ellipse) takes it off again
  - pen:     Pen and Blue (the agent's tools.tool, tools.color), three presses on the stage and one on the first point
             close a filled triangle

  apps/void/bench/space_vector.py [--out DIR] [--mcp-port 7497] [--probe-port 7498]

Build first: void (lane build -- cargo build --release in void/).
"""
import argparse, json, os, shutil, signal, subprocess, sys, tempfile, time, urllib.parse

sys.path.insert(0, os.path.dirname(__file__))
from space_panels import ROOT, VOID, Void, http, read_png, switch  # noqa: E402

CW, CH = 1280, 720


def until(what, seconds, f):
    """f() until it returns without raising, for at most `seconds`; its value."""
    last = None
    for _ in range(int(seconds)):
        try:
            return f()
        except Exception as e:
            last = e
        time.sleep(1)
    raise RuntimeError(f"{what} in {seconds} s: {last}")


def count(png, test, box=None):
    """Samples (every 4th pixel) passing test(r, g, b), in box (x0, y0, x1, y1) of the drawing's pixels if given."""
    w, h, px = read_png(png)
    kx, ky = w / CW, h / CH
    x0, y0, x1, y1 = box or (0, 0, CW, CH)
    return sum(1 for y in range(int(y0 * ky), int(y1 * ky), 4) for x in range(int(x0 * kx), int(x1 * kx), 4) if test(*px(x, y)))


def rows(png):
    """How many of the layers panel's rows are drawn (a row's name in light text right of its swatch)."""
    w, h, px = read_png(png)
    k = w / 220
    n = 0
    for i in range(12):
        y0 = int((36 + i * 28 + 8) * k)
        if y0 + int(12 * k) >= h:
            break
        if any(sum(px(int(x * k), y)) > 450 for y in range(y0, y0 + int(12 * k)) for x in range(36, 100)):
            n += 1
    return n


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=None)
    ap.add_argument("--mcp-port", type=int, default=7497)
    ap.add_argument("--probe-port", type=int, default=7498)
    a = ap.parse_args()
    out = a.out or tempfile.mkdtemp(prefix="space-vector-")
    os.makedirs(out, exist_ok=True)
    home, proj = os.path.join(out, "home"), os.path.join(out, "workspace")
    shutil.copytree(os.path.join(ROOT, "apps", "void", "spaces", "vector", "starter"), os.path.join(proj, "objects", "drawing"))
    env = {**os.environ, "RAEA_VOID_HOME": home, "RAEA_VOID_MCP_PORT": str(a.mcp_port), "RAEA_VOID_PROBE": "1",
           "RAEA_VOID_PROBE_PORT": str(a.probe_port), "RAEA_VOID_HEADLESS": "1", "RAEA_VOID_BACKGROUND": "1", "RAEA_AUDIO_MUTE": "1"}
    log = open(os.path.join(out, "void.log"), "w")
    proc = subprocess.Popen(["nice", "-n", "10", VOID, proj], env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    v, results, ok = Void(a.mcp_port), {}, True
    probe = f"http://127.0.0.1:{a.probe_port}"

    def stage(name):
        path = os.path.join(out, name)
        http(f"{probe}/snapshot?path={urllib.parse.quote(path)}&as=page", timeout=60)
        return path

    def layers(name):
        path = os.path.join(out, name)
        v.act("panel_snapshot", {"i": 1, "path": path})
        return path

    def pointer(x, y, down):
        return http(f"{probe}/pointer?x={x}&y={y}&down={1 if down else 0}")

    try:
        until("void's MCP answering", 90, lambda: v.call("state", {}))
        v.act("open_object", "drawing")
        time.sleep(3)
        switch(v, "vector", "Vector")
        until("both panels painting", 60, lambda: [v.act("panel_snapshot", {"i": i, "path": os.path.join(out, f"panel-{i}-open.png")}) for i in (0, 1)])
        time.sleep(1)
        rows_before = rows(layers("layers-before.png"))
        # Rect on the tools row: its second button (12 px in, 78 wide, 6 apart), halfway down
        v.act("panel_press", {"i": 0, "x": 12 + 84 + 39, "y": 26})
        time.sleep(0.5)
        # where the drawing's pixels are in the stage's box: fitted whole and centred, as the chrome shows it
        b = pointer(0, 0, False)
        bw, bh = b["box"]
        f = min(bw / CW, bh / CH)
        to_box = lambda x, y: (round((bw - CW * f) / 2 + x * f, 2), round((bh - CH * f) / 2 + y * f, 2))
        rect = (200, 120, 520, 330)
        ink = lambda r, g, b_: r + g + b_ < 150
        before = count(stage("stage-before.png"), ink, rect)
        pointer(*to_box(rect[0], rect[1]), True)
        for t in range(1, 6):
            time.sleep(0.05)
            pointer(*to_box(rect[0] + (rect[2] - rect[0]) * t / 5, rect[1] + (rect[3] - rect[1]) * t / 5), True)
        time.sleep(0.1)
        pointer(*to_box(rect[2], rect[3]), False)
        time.sleep(1)
        after = count(stage("stage-drawn.png"), ink, rect)
        results["draw"] = {"box": [bw, bh], "ink_before": before, "ink_after": after}
        ok &= after > before + 50
        time.sleep(0.5)
        rows_after = rows(layers("layers-drawn.png"))
        results["layers"] = {"rows_before": rows_before, "rows_after": rows_after}
        ok &= rows_before == 3 and rows_after == 4
        red = lambda r, g, b_: r > 200 and g < 110 and b_ < 110
        red_before = count(stage("stage-before-agent.png"), red)
        said = v.act("panel_action", {"panel": "layers", "action": "add", "args": {"shape": {"kind": "ellipse", "cx": 820, "cy": 420, "rx": 120, "ry": 80, "color": "#e5484d"}}})
        time.sleep(1)
        red_after = count(stage("stage-agent.png"), red)
        layers("layers-agent.png")
        results["agent"] = {"said": said, "red_before": red_before, "red_after": red_after}
        ok &= red_after > red_before + 100
        v.act("panel_action", {"panel": "layers", "action": "remove", "args": {"row": 0}})
        time.sleep(1)
        red_gone = count(stage("stage-removed.png"), red)
        results["remove"] = {"red_after": red_gone, "rows": rows(layers("layers-removed.png"))}
        ok &= red_gone < red_before + 20
        # the pen: Blue, three presses, then a press on the first point closes a filled triangle
        v.act("panel_action", {"panel": "tools", "action": "tool", "args": {"name": "Pen"}})
        v.act("panel_action", {"panel": "tools", "action": "color", "args": {"name": "Blue"}})
        time.sleep(0.5)
        blue = lambda r, g, b_: b_ > 200 and r < 90 and 80 < g < 140
        tri = (620, 80, 860, 300)
        blue_before = count(stage("stage-before-pen.png"), blue, tri)
        for x, y in ((620, 300), (740, 80), (860, 300), (622, 298)):
            pointer(*to_box(x, y), True)
            time.sleep(0.1)
            pointer(*to_box(x, y), False)
            time.sleep(0.1)
        time.sleep(1)
        blue_after = count(stage("stage-pen.png"), blue, tri)
        layers("layers-pen.png")
        results["pen"] = {"blue_before": blue_before, "blue_after": blue_after}
        ok &= blue_after > blue_before + 200
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
