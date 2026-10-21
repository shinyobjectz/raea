#!/usr/bin/env python3
"""A terminal of a space's own, headless (void.feature: A space is a setup): the Typewriter space, whose
terminal is an ordinary panel. Types "/space" and return into it (act panel_key) and checks that void got the line
(the note /space answers with is in its log) and that the panel was handed the conversation (its sheet gains ink).
Nothing goes to the model: /space runs in void.

  apps/void/bench/space_terminal.py [--out DIR] [--mcp-port 7497] [--probe-port 7498]
"""
import argparse, json, os, signal, subprocess, sys, tempfile, time

sys.path.insert(0, os.path.dirname(__file__))
from space_panels import VOID, Void, read_png, switch  # noqa: E402


def ink(png):
    """Samples clearly darker than the panel's paper (#f3eee2, 707 summed): how much is written on it."""
    w, h, px = read_png(png)
    return sum(1 for y in range(0, h, 2) for x in range(0, w, 2) if sum(px(x, y)) < 600)


def until(what, seconds, f):
    """f() until it returns without raising, for at most `seconds`; its value."""
    t, last = time.time(), None
    for _ in range(int(seconds)):
        try:
            return f()
        except Exception as e:
            last = e
        time.sleep(1)
    raise RuntimeError(f"{what} in {seconds} s: {last}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=None)
    ap.add_argument("--mcp-port", type=int, default=7497)
    ap.add_argument("--probe-port", type=int, default=7498)
    a = ap.parse_args()
    out = a.out or tempfile.mkdtemp(prefix="space-terminal-")
    home, proj = os.path.join(out, "home"), os.path.join(out, "workspace")
    os.makedirs(os.path.join(proj, "objects"), exist_ok=True)
    env = {**os.environ, "RAEA_VOID_HOME": home, "RAEA_VOID_MCP_PORT": str(a.mcp_port), "RAEA_VOID_PROBE": "1",
           "RAEA_VOID_PROBE_PORT": str(a.probe_port), "RAEA_VOID_HEADLESS": "1", "RAEA_VOID_BACKGROUND": "1", "RAEA_AUDIO_MUTE": "1"}
    log = open(os.path.join(out, "void.log"), "w")
    proc = subprocess.Popen(["nice", "-n", "10", VOID, proj], env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    v, results, ok = Void(a.mcp_port), {}, True
    try:
        until("void's MCP answering", 90, lambda: v.call("state", {}))
        switch(v, "typewriter", "Typewriter")
        until("the terminal painting", 60, lambda: v.act("panel_snapshot", {"i": 0, "path": os.path.join(out, "terminal-before.png")}))
        before = ink(os.path.join(out, "terminal-before.png"))
        v.act("panel_key", {"i": 0, "keys": list("/space") + ["Enter"]})
        time.sleep(2)
        v.act("panel_snapshot", {"i": 0, "path": os.path.join(out, "terminal-after.png")})
        after = ink(os.path.join(out, "terminal-after.png"))
        logs = v.call("logs", {"n": 200}).splitlines()
        results.update({"ink_before": before, "ink_after": after, "void_heard": [l for l in logs if "note: Spaces" in l][-2:]})
        if not results["void_heard"] or after <= before:
            ok = False
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
