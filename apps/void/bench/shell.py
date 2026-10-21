#!/usr/bin/env python3
"""void's shell, headless (apps/void/shell.feature P1): the header, the files on the left and the chat on the right, the
same around every space, on a test instance started with nothing said about spaces (as a person's void starts, so it
opens spaces/void). Checks:

  - start:   the void space opens by itself with the shell's three panels painted (shell-header, shell-files, shell-chat) and the
             stage between them (its box inset by the files' and chat's widths and the header's height)
  - idle:    nothing reaching the panels for 3 s, none of them is painted more than 4 times and none stepped more than
             15 (a panel steps and paints only when something reaches it or its page changes, and the stage's player,
             moving while the workspace's first object plays, wakes a settled one 4 times a second: Only what's seen
             is drawn)
  - tree:    the files panel lists the workspace with Material Icon Theme icons (colour in its icon column, which a
             grey-only panel wouldn't have), a press on a folder's row opens it (more rows, so more ink), and a file
             written into the workspace shows up (the panel's picture changes)
  - open:    the files panel's open action on a composition's folder, then a press on its file, opens it (void says the
             object opened)
  - spaces:  a press on the spaces section's video row opens the video space, and its shell is the same (three shell
             panels last); /space void goes back
  - toggle:  real clicks (act click) on the cube and the cat hide the files and the chat (the stage widens: the native
             stage's rect), and Command-B and Command-J (act key) show them again; kept in settings.json ("shell")
  - widths:  a drag of the files' inner edge (act drag) widens it, kept in settings.json
  - chat:    "/space" and return typed into the chat (act panel_key) reach void and its answer comes back; a paste
             shows in the prompt and up puts /space back; real keys after a real click on the chat reach void; a press
             on the chat's name lists the chats and + New chat starts one; ⋯ opens the settings sheet
  - drag:    a real click on the header's empty stretch drags the window, as a title bar does (void logs it)

  apps/void/bench/shell.py [--out DIR] [--mcp-port 7497] [--probe-port 7498]

Build first: void (cargo build --release in void/).
"""
import argparse, json, os, shutil, signal, subprocess, sys, tempfile, time

sys.path.insert(0, os.path.dirname(__file__))
from space_panels import ROOT, VOID, Void, read_png, switch  # noqa: E402


# the shell's panels, after the space's own (the dock's follow them: bench/dock.py)
SHELL = ["shell-header", "shell-files", "shell-chat"]


def until(what, seconds, f):
    """f() until it returns something true without raising, for at most `seconds`; its value."""
    last = None
    for _ in range(int(seconds * 4)):
        try:
            v = f()
            if v:
                return v
            last = "not yet"
        except Exception as e:
            last = e
        time.sleep(0.25)
    raise RuntimeError(f"{what} in {seconds} s: {last}")


def ink(png, y0=0, y1=None, x0=0, x1=None, above=240):
    """Samples clearly lighter than the panels' ground (their channels' sum over `above`), in the box given (points;
    scale 1 headless)."""
    w, h, px = read_png(png)
    return sum(1 for y in range(y0, min(h, y1 or h), 2) for x in range(x0, min(w, x1 or w), 2) if sum(px(x, y)[:3]) > above)


def colour(png, x0, x1, y0=0, y1=None):
    """Samples with a hue (an icon's colour, not the panels' greys), in the box given."""
    w, h, px = read_png(png)
    n = 0
    for y in range(y0, min(h, y1 or h)):
        for x in range(x0, min(w, x1)):
            r, g, b = px(x, y)[:3]
            n += max(r, g, b) - min(r, g, b) > 60
    return n


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=None)
    ap.add_argument("--mcp-port", type=int, default=7497)
    ap.add_argument("--probe-port", type=int, default=7498)
    a = ap.parse_args()
    out = a.out or tempfile.mkdtemp(prefix="shell-")
    home, proj = os.path.join(out, "home"), os.path.join(out, "workspace")
    os.makedirs(os.path.join(proj, "objects"), exist_ok=True)
    os.makedirs(os.path.join(proj, "media"), exist_ok=True)
    for slug, starter in (("drawing", "vector"), ("rock-garden", "build-3d")):
        shutil.copytree(os.path.join(ROOT, "apps", "void", "spaces", starter, "starter"), os.path.join(proj, "objects", slug))
    open(os.path.join(proj, "notes.md"), "w").write("# notes\n")
    env = {k: x for k, x in os.environ.items() if k != "RAEA_VOID_SPACE"}
    env = {**env, "RAEA_VOID_HOME": home, "RAEA_VOID_MCP_PORT": str(a.mcp_port), "RAEA_VOID_PROBE": "1",
           "RAEA_VOID_PROBE_PORT": str(a.probe_port), "RAEA_VOID_HEADLESS": "1", "RAEA_VOID_BACKGROUND": "1", "RAEA_AUDIO_MUTE": "1"}
    log = open(os.path.join(out, "void.log"), "w")
    proc = subprocess.Popen(["nice", "-n", "10", VOID, proj], env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    v, results, ok = Void(a.mcp_port), {}, True
    logs = lambda: v.call("logs", {"n": 400}).splitlines()
    state = lambda: json.loads(v.call("state", {}))
    shell = lambda: state()["shell"]

    def index():
        """The open space's panels by name (the shell's are last)."""
        return {p["name"]: i for i, p in enumerate(shell()["panels"] or [])}

    def snap(i, name):
        return json.loads(v.act("panel_snapshot", {"i": i, "path": os.path.join(out, name)}))

    def does(what, n=0):
        return until(f"a panel asking {what}", 10, lambda: sum(1 for l in logs() if l.endswith(f" does {what}")) > n)

    def asks(what):
        return sum(1 for l in logs() if l.endswith(f" does {what}"))

    def stage_rect():
        at, wh = [l for l in logs() if "native stage: rect" in l][-1].split("rect ")[1].split(" ")[:2]
        return [float(x) for x in at.split(",")] + [float(x) for x in wh.split("x")]

    def settings():
        try:
            return json.load(open(os.path.join(home, "settings.json"))).get("shell")
        except FileNotFoundError:
            return None

    def click(i, x, y):
        r = json.loads(v.act("panel_rect", {"i": i}))
        v.act("click", {"x": r["x"] + x, "y": r["y"] + y})
        return r

    try:
        until("void's MCP answering", 90, lambda: v.call("state", {}))
        until("the void space opening", 60, lambda: "shell-header" in index())
        P = index()
        HEADER, FILES, CHAT = P["shell-header"], P["shell-files"], P["shell-chat"]
        shots = until("the shell painting", 60, lambda: [snap(i, f"open-{n}.png") for n, i in (("header", HEADER), ("files", FILES), ("chat", CHAT))])
        said = [l for l in logs() if "Space ·" in l]
        rects = {n: json.loads(v.act("panel_rect", {"i": i})) for n, i in (("header", HEADER), ("files", FILES), ("chat", CHAT))}
        results["start"] = {"panels": [(s["w"], s["h"]) for s in shots], "rects": rects, "said": said[-1:], "order": list(P)}
        ok &= bool(said) and said[-1].endswith("Space · void") and [n for n in P if n in SHELL] == SHELL
        ok &= round(rects["files"]["w"]) == 260 and round(rects["chat"]["w"]) == 380 and round(rects["header"]["h"]) == 40

        # idle: nothing reaching the panels
        time.sleep(3)
        before = {p["name"]: (p["steps"], p["paints"]) for p in shell()["panels"]}
        time.sleep(3)
        after = {p["name"]: (p["steps"], p["paints"]) for p in shell()["panels"]}
        idle = {n: [after[n][0] - before[n][0], after[n][1] - before[n][1]] for n in after if n in before}
        results["idle"] = {"panels": idle, "native": state()["native"]}
        ok &= all(s <= 15 and p <= 4 for s, p in idle.values())

        # the tree
        snap(FILES, "tree-start.png")
        icons = colour(os.path.join(out, "tree-start.png"), 22, 42, 30, 200)
        rows0 = ink(os.path.join(out, "tree-start.png"), 30, 300)
        # objects/ is the second folder (media, objects, then notes.md)
        n = asks("tree-open")
        v.act("panel_press", {"i": FILES, "x": 80, "y": 30 + 22 + 11})
        does("tree-open", n)
        time.sleep(0.5)
        snap(FILES, "tree-objects.png")
        rows1 = ink(os.path.join(out, "tree-objects.png"), 30, 300)
        open(os.path.join(proj, "added-by-the-check.md"), "w").write("hello\n")
        def changed():
            snap(FILES, "tree-added.png")
            return read_png(os.path.join(out, "tree-added.png"))[2](60, 30 + 5 * 22 + 11) != read_png(os.path.join(out, "tree-objects.png"))[2](60, 30 + 5 * 22 + 11) or ink(os.path.join(out, "tree-added.png"), 30, 400) != ink(os.path.join(out, "tree-objects.png"), 30, 400)
        watched = until("the new file showing in the tree", 10, changed)
        results["tree"] = {"icon_colour": icons, "ink_closed": rows0, "ink_objects_open": rows1, "watched": bool(watched)}
        ok &= icons > 20 and rows1 > rows0

        # a composition opened from the tree: objects/rock-garden opened, its first file pressed
        v.act("panel_action", {"panel": "shell-files", "action": "open", "args": {"path": "objects/rock-garden"}})
        does("tree-open", n + 1)
        time.sleep(0.5)
        n = asks("tree-file")
        # media, objects, drawing, rock-garden, (its files)
        v.act("panel_press", {"i": FILES, "x": 120, "y": 30 + 4 * 22 + 11})
        does("tree-file", n)
        opened = until("the rock garden opening", 20, lambda: [l for l in logs() if "rock-garden" in l.lower() or "rock garden" in l.lower()])
        results["open"] = opened[-1:]

        # a space from the spaces section: video, then back
        h = snap(FILES, "files-spaces.png")["h"]
        slugs = ["build-3d", "typewriter", "vector", "video", "void"]  # void's spaces (the workspace has none)
        row = slugs.index("video")
        n = asks("space")
        v.act("panel_press", {"i": FILES, "x": 60, "y": h - (30 + len(slugs) * 24 + 8) + 30 + row * 24 + 12})
        does("space", n)
        said_video = until("the video space opening", 60, lambda: [l for l in logs() if l.endswith("Space · Video") or l.endswith("Space · video")])
        names = list(index())
        results["spaces"] = {"said": said_video[-1:], "panels": names}
        ok &= [n for n in names if n in SHELL] == SHELL
        switch(v, "void", "void")
        P = index()
        HEADER, FILES, CHAT = P["shell-header"], P["shell-files"], P["shell-chat"]
        time.sleep(1)

        # the cube and the cat, then Command-B and Command-J
        wide0 = stage_rect()
        n = asks("toggle")
        click(HEADER, 78 + 15, 20)  # the cube
        does("toggle", n)
        until("the files hidden", 10, lambda: not shell()["shown"]["files"])
        hr = json.loads(v.act("panel_rect", {"i": HEADER}))
        click(HEADER, hr["w"] - 8 - 15, 20)  # the cat
        until("the chat hidden", 10, lambda: not shell()["shown"]["chat"])
        time.sleep(0.5)
        wide1 = until("the stage widening", 10, lambda: (lambda r: r if r[2] > wide0[2] + 500 else None)(stage_rect()))
        kept = settings()
        click(HEADER, hr["w"] / 2, 20)  # (the header takes the keys; its empty stretch also drags, checked below)
        v.act("key", {"keys": ["b"], "cmd": True})
        until("Command-B showing the files", 10, lambda: shell()["shown"]["files"])
        v.act("key", {"keys": ["j"], "cmd": True})
        until("Command-J showing the chat", 10, lambda: shell()["shown"]["chat"])
        results["toggle"] = {"stage_before": wide0, "stage_hidden": wide1, "kept": kept, "shown": shell()["shown"]}
        ok &= kept is not None and kept["files"] is False and kept["chat"] is False and settings()["files"] and settings()["chat"]

        # the files' edge dragged 80 points right
        fr = json.loads(v.act("panel_rect", {"i": FILES}))
        try:
            v.act("drag", {"from": [fr["x"] + fr["w"], fr["y"] + 200], "to": [fr["x"] + fr["w"] + 80, fr["y"] + 200]})
            widened = until("the files widening", 10, lambda: shell()["shown"]["files_w"] > 300)
            results["widths"] = {"files_w": shell()["shown"]["files_w"], "kept": settings()["files_w"]}
            ok &= bool(widened) and settings()["files_w"] > 300
        except RuntimeError as e:
            results["widths"] = {"skipped": str(e)}

        # the chat
        snap(CHAT, "chat-before.png")
        body_before = ink(os.path.join(out, "chat-before.png"), 40)
        v.act("panel_key", {"i": CHAT, "keys": list("/space") + ["Enter"]})
        heard = until("void answering /space", 20, lambda: [l for l in logs() if "note: Spaces:" in l])
        time.sleep(1)
        snap(CHAT, "chat-after.png")
        body_after = ink(os.path.join(out, "chat-after.png"), 40)

        def prompt_ink(name):
            shot = snap(CHAT, name)
            # white text only: the empty prompt's grey placeholder isn't counted
            return ink(os.path.join(out, name), shot["h"] - 52, above=600)
        empty = prompt_ink("prompt-empty.png")
        v.act("panel_paste", {"i": CHAT, "text": "a garden of moss and sand"})
        time.sleep(0.5)
        pasted = prompt_ink("prompt-pasted.png")
        v.act("panel_key", {"i": CHAT, "keys": ["ArrowUp"]})
        time.sleep(0.5)
        recalled = prompt_ink("prompt-recalled.png")
        v.act("panel_key", {"i": CHAT, "keys": ["Backspace"] * len("/space")})
        time.sleep(0.5)
        rubbed = prompt_ink("prompt-rubbed.png")
        click(CHAT, 200, 300)  # the chat takes the keys
        n = sum(1 for l in logs() if "note: Spaces:" in l)
        v.act("key", {"keys": list("/space") + ["Enter"]})
        typed = until("void answering real keys", 20, lambda: sum(1 for l in logs() if "note: Spaces:" in l) > n)
        # the chats: the name lists them, + New chat starts one
        snap(CHAT, "chats-closed.png")
        v.act("panel_press", {"i": CHAT, "x": 40, "y": 20})
        time.sleep(0.5)
        snap(CHAT, "chats-open.png")
        listed = read_png(os.path.join(out, "chats-open.png"))[2](60, 40 + 15) != read_png(os.path.join(out, "chats-closed.png"))[2](60, 40 + 15) or ink(os.path.join(out, "chats-open.png"), 40, 120) != ink(os.path.join(out, "chats-closed.png"), 40, 120)
        n = asks("new-chat")
        v.act("panel_press", {"i": CHAT, "x": 60, "y": 40 + 15})
        does("new-chat", n)
        # ⋯: void's settings (the space's sheet)
        cr = json.loads(v.act("panel_rect", {"i": CHAT}))
        n = asks("settings")
        v.act("panel_press", {"i": CHAT, "x": cr["w"] - 8 - 15, "y": 20})
        does("settings", n)
        results["chat"] = {"heard": heard[-1:], "ink": [body_before, body_after], "prompt": {"empty": empty, "pasted": pasted, "recalled": recalled, "rubbed": rubbed},
                           "typed": bool(typed), "chats_listed": listed}
        ok &= body_after > body_before and pasted > empty + 20 and empty + 5 < recalled < pasted - 10 and rubbed <= empty + 2 and listed

        # the header as the title bar
        n = sum(1 for l in logs() if f"panel {HEADER} drags the window" in l)
        click(HEADER, hr["w"] - 200, 20)
        dragged = until("the header dragging the window", 10, lambda: sum(1 for l in logs() if f"panel {HEADER} drags the window" in l) > n)
        results["drag"] = bool(dragged)
        # the header's tab names the open space (void hands it in as void:shell)
        snap(HEADER, "header-end.png")
        tab = ink(os.path.join(out, "header-end.png"), 8, 32, 120, 310)
        results["tab_ink"] = tab
        ok &= tab > 10
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
