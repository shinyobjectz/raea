#!/usr/bin/env python3
"""void's terminal dock, headless (apps/void/shell.feature P2, P2b): the dock between the files and the chat, its terminals
as tabs (each a terminal.tsx laying out controllers, or a controller alone), the cli a real shell. On a test instance started as a person's void starts (spaces/void). Checks:

  - start:   the dock's bar (shell-dock) and a cli (cli:1.0: tab 1's controller 0) open under the stage, between the files and the chat (its box
             starts at the files' width and ends at the chat's, 240 points deep, the cli under the bar's 30)
  - cli:     the login shell starts in the workspace folder, sized in cells to its panel (pty.rs: 7.23 by 16 points, 8 of
             padding), and draws its prompt
  - keys:    real keys after a real click on the cli (act key) run a command (expr 50 - 8 prints 42); Control-C stops a
             running one (sleep 30) and the next runs at once
  - scroll:  the wheel over the cli (act scroll) scrolls its screen back (seq 1 120's early lines show), a key brings it
             back to the bottom
  - idle:    nothing reaching it for 3 s, no dock panel paints more than 4 times or steps more than 15
  - tabs:    + shows the picker (the terminals there are, edit and shell, then the controllers: audio, cli, timeline), a
             press on timeline opens it alone as a tab and
             shows it (painted); the dock's open action opens a second cli (its own shell); a press on the first tab goes
             back to it; the shell's panels weren't opened again (space_ui reuses a set's panels); × on the second cli
             closes its tab and its shell
  - media:   a cut of two Sequences (Title, then Body holding an <Audio> bed) opened on the stage, and the edit terminal
             opened (apps/void/terminals/edit: the timeline over audio, 3 to 2): its two controllers share the terminal's
             box, the timeline over audio; the timeline lays the cut's clips out in colour under the ruler
             (void:timeline: Live::clips from the bake, heard by @raea/terminal's useTimeline), and with the playhead
             in Body audio lists the bed (void:sounds: Live::sounds, useSounds)
  - edges:   ⇅ moves the dock to the left (beside the files, under the header, 240 wide; the stage narrower), the move
             action back to the bottom; Control-` hides the dock (the stage taller) and shows it; its edge dragged 100
             points up deepens it; all kept in settings.json ("shell", "dock")

  apps/void/bench/dock.py [--out DIR] [--mcp-port 7527] [--probe-port 7528]

Build first: void (cargo build --release in void/).
"""
import argparse, json, math, os, shutil, signal, struct, subprocess, sys, tempfile, time, wave

sys.path.insert(0, os.path.dirname(__file__))
from space_panels import ROOT, VOID, Void  # noqa: E402
from shell import colour, ink, until  # noqa: E402

CUT = """import { AbsoluteFill, Audio, Composition, Sequence, registerRoot } from "@raea/motion";
import bed from "./bed.wav";

function Cut() {
  return (
    <AbsoluteFill style={{ background: "#202024", color: "white", fontFamily: "Inter", fontSize: 72 }}>
      <Sequence name="Title" durationInFrames={60}>
        <AbsoluteFill style={{ alignItems: "center", justifyContent: "center" }}>Title</AbsoluteFill>
      </Sequence>
      <Sequence name="Body" from={60} durationInFrames={90}>
        <Audio src={bed} volume={0.5} />
        <AbsoluteFill style={{ alignItems: "center", justifyContent: "center" }}>Body</AbsoluteFill>
      </Sequence>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="edit" component={Cut} width={640} height={360} fps={30} durationInFrames={150} />);
"""


def write_cut(folder):
    os.makedirs(folder, exist_ok=True)
    open(os.path.join(folder, "edit.tsx"), "w").write(CUT)
    json.dump({"name": "Cut", "slug": "cut", "created": 0, "kind": "video"}, open(os.path.join(folder, "space.json"), "w"))
    with wave.open(os.path.join(folder, "bed.wav"), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(48000)
        w.writeframes(b"".join(struct.pack("<h", int(8000 * math.sin(2 * math.pi * 220 * i / 48000))) for i in range(48000 * 6)))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=None)
    ap.add_argument("--mcp-port", type=int, default=7527)
    ap.add_argument("--probe-port", type=int, default=7528)
    a = ap.parse_args()
    out = a.out or tempfile.mkdtemp(prefix="dock-")
    home, proj = os.path.join(out, "home"), os.path.join(out, "workspace")
    os.makedirs(os.path.join(proj, "objects"), exist_ok=True)
    shutil.copytree(os.path.join(ROOT, "apps", "void", "spaces", "build-3d", "starter"), os.path.join(proj, "objects", "rock-garden"))
    write_cut(os.path.join(proj, "objects", "cut"))
    env = {k: x for k, x in os.environ.items() if k != "RAEA_VOID_SPACE"}
    env = {**env, "RAEA_VOID_HOME": home, "RAEA_VOID_MCP_PORT": str(a.mcp_port), "RAEA_VOID_PROBE": "1",
           "RAEA_VOID_PROBE_PORT": str(a.probe_port), "RAEA_VOID_HEADLESS": "1", "RAEA_VOID_BACKGROUND": "1", "RAEA_AUDIO_MUTE": "1"}
    log = open(os.path.join(out, "void.log"), "w")
    proc = subprocess.Popen(["nice", "-n", "10", VOID, proj], env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    v, results, ok = Void(a.mcp_port), {}, True
    logs = lambda: v.call("logs", {"n": 600}).splitlines()
    state = lambda: json.loads(v.call("state", {}))
    shell = lambda: state()["shell"]
    rect = lambda i: json.loads(v.act("panel_rect", {"i": i}))

    def index():
        return {p["name"]: i for i, p in enumerate(shell()["panels"] or [])}

    def snap(i, name):
        return json.loads(v.act("panel_snapshot", {"i": i, "path": os.path.join(out, name)}))

    def asks(what):
        return sum(1 for l in logs() if l.endswith(f" does {what}"))

    def does(what, n=0):
        return until(f"a panel asking {what}", 10, lambda: asks(what) > n)

    def cli(id_=None):
        """The text of cli id_ (the first running, if None)."""
        cs = shell()["clis"]
        c = [c for c in cs if id_ is None or c["id"] == id_]
        return c[0]["text"] if c else ""

    def stage_rect():
        at, wh = [l for l in logs() if "native stage: rect" in l][-1].split("rect ")[1].split(" ")[:2]
        return [float(x) for x in at.split(",")] + [float(x) for x in wh.split("x")]

    def settings():
        try:
            return json.load(open(os.path.join(home, "settings.json")))
        except FileNotFoundError:
            return {}

    def opened(src):
        return sum(1 for l in logs() if "panels: " in l and src in l and " open in " in l)

    try:
        until("void's MCP answering", 90, lambda: v.call("state", {}))
        P = until("the dock opening", 60, lambda: (lambda p: p if "shell-dock" in p and "cli:1.0" in p else None)(index()))
        DOCK, CLI = P["shell-dock"], P["cli:1.0"]
        until("the dock painting", 60, lambda: snap(DOCK, "dock-start.png"))
        files, chat, bar, term = rect(P["shell-files"]), rect(P["shell-chat"]), rect(DOCK), rect(CLI)
        results["start"] = {"order": list(P), "files": files, "chat": chat, "dock": bar, "cli": term}
        ok &= round(bar["x"]) == round(files["x"] + files["w"]) and round(bar["x"] + bar["w"]) == round(chat["x"]) and round(bar["h"]) == 240
        ok &= round(term["h"]) == 210 and round(term["y"]) == round(bar["y"] + 30) and round(term["w"]) == round(bar["w"])

        # the shell: started in the workspace, sized to the panel in cells, its prompt drawn
        started = until("the cli's shell starting", 20, lambda: [l for l in logs() if "cli 100: " in l])
        cols, rows = int((term["w"] - 16) // 7.23), int((term["h"] - 16) // 16)
        prompt = until("the shell's prompt", 20, lambda: cli(100).strip())
        until("the cli painting its prompt", 20, lambda: ink(os.path.join(out, "cli-prompt.png"), 0, 60) > 5 if snap(CLI, "cli-prompt.png") else False)
        results["cli"] = {"started": started[-1], "cells": [cols, rows], "prompt": prompt.splitlines()[-1:]}
        ok &= os.path.realpath(proj) in started[-1] or proj in started[-1]
        cells = until("the shell sized to its panel", 10, lambda: (lambda c: c if c == [cols, rows] else None)(shell()["clis"][0]["cells"]))
        results["cli"]["sized"] = cells
        ok &= bool(cells)

        # real keys: a click on the cli, then a command; Control-C stops a long one
        v.act("click", {"x": term["x"] + term["w"] / 2, "y": term["y"] + term["h"] / 2})
        time.sleep(0.3)
        v.act("key", {"keys": list("expr 50 - 8") + ["Enter"]})
        ran = until("expr's answer", 10, lambda: "42" in [l.strip() for l in cli(100).splitlines()])
        v.act("key", {"keys": list("sleep 30") + ["Enter"]})
        time.sleep(0.5)
        v.act("key", {"keys": ["c"], "ctrl": True})
        time.sleep(0.3)
        v.act("key", {"keys": list("expr 3 - 1") + ["Enter"]})
        stopped = until("Control-C stopping sleep", 8, lambda: "2" in [l.strip() for l in cli(100).splitlines()])
        snap(CLI, "cli-typed.png")
        results["keys"] = {"ran": bool(ran), "interrupted": bool(stopped), "screen": cli(100).splitlines()[-6:]}

        # the wheel scrolls it back, a key brings it down
        v.act("key", {"keys": list("seq 1 120") + ["Enter"]})
        until("seq's last line", 10, lambda: "120" in [l.strip() for l in cli(100).splitlines()])
        bottom = cli(100)
        # the stage paused first: while it plays, the window takes the real pointer's place as it redraws (the
        # person's own mouse, wherever it is on their screen), and a scroll posted to a hidden window follows it
        v.act("pause")
        time.sleep(0.3)
        # posted again once if it didn't land: the synthetic scroll can still follow a stray move of the real pointer
        # (a harness artefact; a person's wheel goes where their pointer is)
        scrolled = lambda: (lambda t: t if t != bottom and "120" not in [l.strip() for l in t.splitlines()] else None)(cli(100))
        back = None
        for attempt in range(2):
            v.act("scroll", {"x": term["x"] + term["w"] / 2, "y": term["y"] + term["h"] / 2, "dy": -400})
            try:
                back = until("the cli scrolled back", 5, scrolled)
                break
            except RuntimeError:
                results["scroll_reposted"] = attempt + 1
        if back is None:
            raise RuntimeError("the cli scrolled back: not after two scrolls")
        snap(CLI, "cli-back.png")
        v.act("key", {"keys": [" "]})
        down = until("the cli back at the bottom", 10, lambda: "120" in [l.strip() for l in cli(100).splitlines()])
        v.act("key", {"keys": ["Backspace"]})
        results["scroll"] = {"back": back.splitlines()[:2], "down": bool(down)}

        # idle
        time.sleep(2)
        before = {p["name"]: (p["steps"], p["paints"]) for p in shell()["panels"]}
        time.sleep(3)
        after = {p["name"]: (p["steps"], p["paints"]) for p in shell()["panels"]}
        idle = {n: [after[n][0] - before[n][0], after[n][1] - before[n][1]] for n in after if n in before}
        results["idle"] = idle
        ok &= all(s <= 15 and p <= 4 for n, (s, p) in idle.items() if n in ("shell-dock", "cli:1.0"))

        # tabs: + shows the picker, timeline opens from it
        header_src = os.path.join("shell", "header.tsx")
        headers = opened(header_src)
        n = asks("dock-tab")
        v.act("panel_press", {"i": DOCK, "x": 8 + 1 * (132 + 4) + 12, "y": 15})
        does("dock-tab", n)
        until("the picker", 10, lambda: shell()["shown"]["dock_active"] == 0)
        time.sleep(0.5)
        snap(DOCK, "dock-picker.png")
        picker = ink(os.path.join(out, "dock-picker.png"), 36, 200)
        n = asks("dock-new")
        # the terminals, then the controllers: edit, shell, audio, cli, timeline (void's, by slug)
        v.act("panel_press", {"i": DOCK, "x": 40, "y": 30 + 6 + 4 * 28 + 14})
        does("dock-new", n)
        tabs = until("the timeline's tab", 10, lambda: (lambda t: t if len(t) == 2 and t[1]["terminal"] == "timeline" else None)(shell()["tabs"]))
        P = until("the timeline's panel", 10, lambda: (lambda p: p if f"dock:{tabs[1]['id']}.0" in p else None)(index()))
        TL = P[f"dock:{tabs[1]['id']}.0"]
        until("the timeline painting", 20, lambda: snap(TL, "timeline.png"))
        tl_ink = ink(os.path.join(out, "timeline.png"), 0, 40)
        # a second cli, by the dock's action
        v.act("panel_action", {"panel": "shell-dock", "action": "open", "args": {"terminal": "cli"}})
        tabs = until("a second cli", 10, lambda: (lambda t: t if len(t) == 3 and t[2]["terminal"] == "cli" else None)(shell()["tabs"]))
        second = tabs[2]["id"]
        until("its own shell", 20, lambda: len(shell()["clis"]) == 2 and cli(second * 100).strip())
        # the first tab again
        v.act("panel_press", {"i": DOCK, "x": 8 + 40, "y": 15})
        until("the first cli shown", 10, lambda: shell()["shown"]["dock_active"] == 1)
        reopened = opened(header_src) - headers
        # × on the second cli
        P = index()
        v.act("panel_press", {"i": P["shell-dock"], "x": 8 + 2 * (132 + 4) + 132 - 9, "y": 15})
        until("the second cli closed", 10, lambda: [t["id"] for t in shell()["tabs"]] == [1, tabs[1]["id"]] and len(shell()["clis"]) == 1)
        results["tabs"] = {"picker_ink": picker, "timeline_ink": tl_ink, "tabs": shell()["tabs"], "header_reopened": reopened, "kept": settings().get("dock")}
        ok &= picker > 20 and tl_ink > 10 and reopened == 0 and [t["terminal"] for t in settings().get("dock", [])] == ["cli", "timeline"]

        # media: the cut on the stage, its clips in the timeline, its bed in audio
        v.act("open_object", "cut")
        until("the cut on the stage", 60, lambda: [l for l in logs() if "stage: opened" in l and "baked" in l])
        P = index()
        TL = P[f"dock:{tabs[1]['id']}.0"]
        v.act("panel_action", {"panel": "shell-dock", "action": "show", "args": {"id": tabs[1]["id"]}})
        until("the timeline shown", 10, lambda: shell()["shown"]["dock_active"] == tabs[1]["id"])
        v.act("pause")
        v.act("scrub", 90)
        def clips_drawn():
            snap(TL, "timeline-cut.png")
            return colour(os.path.join(out, "timeline-cut.png"), 14, 520, 36 + 22 + 8, 36 + 22 + 8 + 20)
        clipped = until("the cut's clips in the timeline", 20, lambda: clips_drawn() > 200)
        # the edit terminal: the timeline over audio in one tab
        v.act("panel_action", {"panel": "shell-dock", "action": "open", "args": {"terminal": "edit"}})
        tabs3 = until("the edit tab", 10, lambda: (lambda t: t if len(t) == 3 and t[2]["terminal"] == "edit" else None)(shell()["tabs"]))
        E = tabs3[2]["id"]
        P = until("the edit terminal's controllers", 10, lambda: (lambda p: p if f"dock:{E}.0" in p and f"dock:{E}.1" in p else None)(index()))
        ETL, AU = P[f"dock:{E}.0"], P[f"dock:{E}.1"]
        time.sleep(1)
        bar, top_, low = rect(P["shell-dock"]), rect(ETL), rect(AU)
        def edit_clips():
            snap(ETL, "edit-timeline.png")
            return colour(os.path.join(out, "edit-timeline.png"), 14, 520, 36 + 22 + 8, 36 + 22 + 8 + 20)
        edit_clipped = until("the cut's clips in the edit terminal's timeline", 20, lambda: edit_clips() > 200)
        def bed_listed():
            snap(AU, "audio-cut.png")
            return ink(os.path.join(out, "audio-cut.png"), 34, 60)
        listed = until("the bed listed in audio", 20, lambda: bed_listed() > 20)
        results["media"] = {"clip_colour": clipped, "edit": {"bar": bar, "timeline": top_, "audio": low, "clip_colour": edit_clipped}, "audio_row_ink": listed}
        # 210 under the bar, 3 to 2: the timeline 126 less the point between, audio the 84 under it, both the dock's width
        ok &= round(top_["y"]) == round(bar["y"] + 30) and round(top_["h"]) == 125 and round(low["y"]) == round(bar["y"] + 30 + 126) and round(low["h"]) == 84
        ok &= round(top_["w"]) == round(bar["w"]) == round(low["w"])
        v.act("panel_action", {"panel": "shell-dock", "action": "close", "args": {"id": tabs3[2]["id"]}})
        until("edit closed", 10, lambda: len(shell()["tabs"]) == 2 and f"dock:{E}.1" not in index())
        v.act("panel_action", {"panel": "shell-dock", "action": "show", "args": {"id": 1}})
        until("the cli shown", 10, lambda: shell()["shown"]["dock_active"] == 1)

        # edges: ⇅ to the left, back to the bottom by the action, Control-` hides and shows, the edge dragged
        P = index()
        DOCK = P["shell-dock"]
        wide0 = stage_rect()
        bar = rect(DOCK)
        v.act("panel_press", {"i": DOCK, "x": bar["w"] - 6 - 2 * 24 - 2 + 12, "y": 15})
        until("the dock on the left", 10, lambda: shell()["shown"]["dock_side"] == "left")
        time.sleep(1)
        left = rect(index()["shell-dock"])
        wide1 = stage_rect()
        v.act("panel_action", {"panel": "shell-dock", "action": "move", "args": {"side": "bottom"}})
        until("the dock at the bottom", 10, lambda: shell()["shown"]["dock_side"] == "bottom")
        time.sleep(1)
        term = rect(index()["cli:1.0"])
        v.act("click", {"x": term["x"] + term["w"] / 2, "y": term["y"] + term["h"] / 2})
        time.sleep(0.3)
        v.act("key", {"keys": ["`"], "ctrl": True})
        until("Control-` hiding the dock", 10, lambda: not shell()["shown"]["dock"])
        time.sleep(1)
        tall = stage_rect()
        v.act("key", {"keys": ["`"], "ctrl": True})
        shown_again = until("Control-` showing it", 10, lambda: shell()["shown"]["dock"])
        time.sleep(1)
        bar = rect(index()["shell-dock"])
        try:
            v.act("drag", {"from": [bar["x"] + bar["w"] / 2, bar["y"]], "to": [bar["x"] + bar["w"] / 2, bar["y"] - 100]})
            deeper = until("the dock deepening", 10, lambda: shell()["shown"]["dock_size"] >= 320)
        except RuntimeError as e:
            deeper = None
            results["drag_error"] = str(e)
        kept = settings().get("shell", {})
        results["edges"] = {"left": left, "stage": {"bottom": wide0, "left": wide1, "hidden": tall}, "shown_again": bool(shown_again), "size": shell()["shown"]["dock_size"], "kept": kept}
        ok &= round(left["x"]) == round(files["x"] + files["w"]) and round(left["w"]) == 240 and round(left["y"]) == 40
        ok &= wide1[2] < wide0[2] - 200 and tall[3] > wide0[3] + 200 and bool(deeper) and kept.get("dock_size", 0) >= 320
        errors = [p for p in shell()["panels"] if p["error"]]
        results["errors"] = errors
        ok &= not errors
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
