#!/usr/bin/env python3
"""A space's panels in void, headless (void.feature: A space is a setup): a test instance of void with no
window on screen, its own home, ports and workspace, Build 3D's starter (a rock garden, live 3D) as the object,
switched to the Build 3D space. Checks:

  - open:   both of Build 3D's panels opened and painted (their textures read back through act panel_snapshot)
  - click:  a press on the outliner's fourth row (act panel_press) picks it (the row's blue moves from row 3 to row 4)
  - action: the agent's way in, act panel_action outliner.pick {row: 5}, picks the sixth row the same way
  - object: the panels reach the object on the stage: Rock 3 picked (the action above), then the agent picks the
            material Ember (materials.pick), and the stage's picture gains orange where Rock 3 is
  - back:   /space void puts void's own space back in Build 3D's place (there's no space-less chrome to go back to)

  apps/void/bench/space_panels.py [--out DIR] [--mcp-port 7497] [--probe-port 7498]

Build first: void (lane build -- cargo build --release in void/).
"""
import argparse, json, os, shutil, signal, subprocess, sys, tempfile, time, urllib.parse, urllib.request

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
VOID = os.path.join(ROOT, "apps", "void", "target", "release", "void")


def http(url, body=None, timeout=60):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, headers={"content-type": "application/json", "accept": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read() or b"null")


class Void:
    def __init__(self, port):
        self.mcp, self.id = f"http://127.0.0.1:{port}/mcp", 0

    def call(self, name, args):
        self.id += 1
        r = http(self.mcp, {"jsonrpc": "2.0", "id": self.id, "method": "tools/call", "params": {"name": name, "arguments": args}})
        res = r.get("result", {})
        text = (res.get("content") or [{}])[0].get("text", "")
        if res.get("isError"):
            raise RuntimeError(f"{name} {args}: {text}")
        return text

    def act(self, action, value=None):
        return self.call("act", {"action": action, "value": value})


def switch(v, name, title, seconds=60):
    """/space NAME (act space), and wait until void says the space titled TITLE opened: a void always runs a space, so
    until then the panels it has are the last space's (void's own, at launch). TITLE must differ from the last's."""
    v.act("space", name)
    last = None
    for _ in range(int(seconds * 4)):
        said = [l for l in v.call("logs", {"n": 300}).splitlines() if "Space ·" in l or "didn't load" in l]
        last = said[-1] if said else None
        if last and last.endswith(f"Space · {title}"):
            return last
        if last and "didn't load" in last:
            raise RuntimeError(f"the space {name} didn't load: {last}")
        time.sleep(0.25)
    raise RuntimeError(f"the space {name} never opened in {seconds} s (last: {last})")


def read_png(path):
    """(width, height, pixel(x, y) -> (r, g, b)) of an 8-bit RGBA PNG as void writes them (no Pillow needed)."""
    import struct, zlib
    data = open(path, "rb").read()
    pos, idat, w, h = 8, b"", 0, 0
    while pos < len(data):
        n, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        if kind == b"IHDR":
            w, h, depth, color = struct.unpack(">IIBB", body[:10])
            assert depth == 8 and color == 6, "8-bit RGBA only"
        elif kind == b"IDAT":
            idat += body
        pos += 12 + n
    raw, stride, rows, prev = zlib.decompress(idat), w * 4, [], bytearray(w * 4)
    for y in range(h):
        f, line = raw[y * (stride + 1)], bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for x in range(stride):
            a = line[x - 4] if x >= 4 else 0
            b, c = prev[x], prev[x - 4] if x >= 4 else 0
            if f == 1: line[x] = (line[x] + a) & 255
            elif f == 2: line[x] = (line[x] + b) & 255
            elif f == 3: line[x] = (line[x] + (a + b) // 2) & 255
            elif f == 4:
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(line)
        prev = line
    return w, h, lambda x, y: tuple(rows[y][x * 4:x * 4 + 3])


def picked_row(png, top=36, row=30, width=240):
    """Which outliner row is blue (#2b4bff), read down the panel's left third; None if none is."""
    w, h, px = read_png(png)
    k = w / width
    for i in range(6):
        y = int((10 + top - 10 + i * row + row / 2) * k)
        if y >= h:
            break
        r, g, b = px(int(w * 0.15), y)
        if b > 200 and r < 90 and 50 < g < 110:
            return i
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=None)
    ap.add_argument("--mcp-port", type=int, default=7497)
    ap.add_argument("--probe-port", type=int, default=7498)
    a = ap.parse_args()
    out = a.out or tempfile.mkdtemp(prefix="space-panels-")
    os.makedirs(out, exist_ok=True)
    for port in (a.mcp_port, a.probe_port):
        if subprocess.run(["lsof", f"-iTCP:{port}", "-sTCP:LISTEN"], capture_output=True).returncode == 0:
            sys.exit(f"port {port} is taken: pass --mcp-port/--probe-port")
    home, proj = os.path.join(out, "home"), os.path.join(out, "workspace")
    obj = os.path.join(proj, "objects", "rock-garden")
    shutil.copytree(os.path.join(ROOT, "apps", "void", "spaces", "build-3d", "starter"), obj)
    env = {**os.environ, "RAEA_VOID_HOME": home, "RAEA_VOID_MCP_PORT": str(a.mcp_port), "RAEA_VOID_PROBE": "1",
           "RAEA_VOID_PROBE_PORT": str(a.probe_port), "RAEA_VOID_HEADLESS": "1", "RAEA_VOID_BACKGROUND": "1", "RAEA_AUDIO_MUTE": "1"}
    log = open(os.path.join(out, "void.log"), "w")
    proc = subprocess.Popen(["nice", "-n", "10", VOID, proj], env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    v = Void(a.mcp_port)
    results, ok = {}, True
    try:
        t = time.time()
        while True:
            try:
                v.call("state", {})
                break
            except Exception:
                if time.time() - t > 90 or proc.poll() is not None:
                    raise RuntimeError("void's MCP never answered (see void.log)")
                time.sleep(1)
        v.act("open_object", "rock-garden")
        time.sleep(3)
        t0 = time.time()
        switch(v, "build 3d", "Build 3D")
        shots = []
        while time.time() - t0 < 60:
            time.sleep(1)
            try:
                shots = [json.loads(v.act("panel_snapshot", {"i": i, "path": os.path.join(out, f"panel-{i}-open.png")})) for i in (0, 1)]
                break
            except Exception as e:
                last = str(e)
        else:
            raise RuntimeError(f"the panels never painted: {last}")
        results["open"] = {"seconds": round(time.time() - t0, 1), "panels": [(s["w"], s["h"]) for s in shots]}
        before = picked_row(os.path.join(out, "panel-1-open.png"))
        logs = v.call("logs", {"n": 400})
        results["logs"] = [l for l in logs.splitlines() if "panels:" in l or "space:" in l][-12:]
        # the outliner's fourth row, in the panel's points (its rows start under its 10 px padding and 26 px header). A
        # headless window gets no mouse events from AppKit, so the press goes in through void's pointer path
        # (act panel_press: what the chrome's handlers hand the panel), not through the window
        px, py = 60, 10 + 26 + 3 * 30 + 15
        v.act("panel_press", {"i": 1, "x": px, "y": py})
        time.sleep(1)
        v.act("panel_snapshot", {"i": 1, "path": os.path.join(out, "panel-1-click.png")})
        after = picked_row(os.path.join(out, "panel-1-click.png"))
        results["click"] = {"at": [px, py], "picked_before": before, "picked_after": after}
        if not (before == 2 and after == 3):
            ok = False
        said = v.act("panel_action", {"panel": "outliner", "action": "pick", "args": {"row": 5}})
        time.sleep(1)
        v.act("panel_snapshot", {"i": 1, "path": os.path.join(out, "panel-1-action.png")})
        acted = picked_row(os.path.join(out, "panel-1-action.png"))
        results["action"] = {"said": said, "picked_after": acted}
        if acted != 5:
            ok = False
        # the object hears the panels: Rock 3 is picked; the agent paints it Ember
        probe = f"http://127.0.0.1:{a.probe_port}"
        def orange(name):
            path = os.path.join(out, name)
            http(f"{probe}/snapshot?path={urllib.parse.quote(path)}&as=page", timeout=60)
            w, h, px = read_png(path)
            n = 0
            for y in range(0, h, 4):
                for x in range(0, w, 4):
                    r, g, b = px(x, y)
                    n += r > 150 and 60 < g < 130 and b < 80
            return n
        before_o = orange("stage-before.png")
        said = v.act("panel_action", {"panel": "materials", "action": "pick", "args": {"name": "Ember"}})
        time.sleep(1.5)
        after_o = orange("stage-after.png")
        v.act("panel_snapshot", {"i": 0, "path": os.path.join(out, "panel-0-after.png")})
        results["object"] = {"said": said, "orange_before": before_o, "orange_after": after_o}
        if not after_o > before_o + 20:
            ok = False
        results["back"] = switch(v, "void", "void", 30)
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
    results["pass"] = ok
    json.dump(results, open(os.path.join(out, "result.json"), "w"), indent=2)
    print(json.dumps(results, indent=2))
    print(("PASS" if ok else "FAIL"), out)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
