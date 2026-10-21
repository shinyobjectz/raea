"""score: a .brief compiled through an intermediate form to a target, answered with its exposure sheet.

  front end   compiler/front.py   the brief's intent graph (raea-brief parse) read, then placed on the space's takes
                                  (clips, framing, parts, sound; open beats bound to footage by compiler/moments.py)
  form        compiler/ir.py      Script and Timeline: no target in them
  sheet       compiler/sheet.py   the script timed against the takes, its checks, what would bring it to length
  backend     compiler/sdk.py     the edit as an @raea/motion composition (the only target: rrl the language and its
                                  backend went on 2026-10-17)

    uv run -q python ~/raea/ai/score/scorec.py SPACE [NAME.brief] [--o EDIT] [--no-sound] [--json IR.json]
                                            [--graph GRAPH.json] [--cut RATIO]

Usually run by `raea-brief compile` (`raea script`), which parses the brief and passes --graph. A COMPILED line, the
sheet, then what to fix. The notation is sdk/brief/format.md; a script.fountain is refused with the command that converts
it (`raea-brief convert`). Score's Fountain reader is kept in bench/fountain.py for the recorded comparisons.
"""
import argparse, json, os, subprocess, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from compiler import front, ir, sdk, sheet  # noqa: E402

BRIEF = Path(os.environ.get("RAEA_BRIEF", Path.home() / "raea/brief/target/release/raea-brief"))


def main():
    ap = argparse.ArgumentParser(); ap.add_argument("space"); ap.add_argument("script", nargs="?"); ap.add_argument("--o")
    ap.add_argument("--target", default="sdk", choices=["sdk"], help="the SDK, the only target (kept so older callers still parse)")
    ap.add_argument("--no-sound", action="store_true"); ap.add_argument("--json")
    ap.add_argument("--graph", help="the script's intent graph (raea-brief parse), read instead of the script's text")
    ap.add_argument("--cut", help="the deliverable to compile, by its ratio (a .brief's Format: may list several)")
    ap.add_argument("--variant", help="the variant to compile, by its name (`# hook {A}`)")
    a = ap.parse_args()
    space = Path(a.space).resolve(); script = Path(a.script).resolve() if a.script else space / "brief.brief"
    edit = Path(a.o).resolve() if a.o else space / "edit.tsx"
    if not script.exists():
        print(f"REFUSED  no script at {script}: write the script first (the notation is in the score procedure)"); return 2
    tk = front.takes(space / "media")
    work = space / ".score"; work.mkdir(exist_ok=True); edit.parent.mkdir(parents=True, exist_ok=True)
    if script.suffix == ".fountain":
        print(f"REFUSED  {script.name}: the compiler reads .brief now (sdk/brief/format.md); `raea-brief convert {script.name} > brief.brief` writes it as one"); return 2
    if a.graph:
        g = json.loads(Path(a.graph).read_text())
    else:
        r = subprocess.run([str(BRIEF), "parse", str(script)], capture_output=True, text=True)
        if r.returncode:
            print("REFUSED  " + r.stderr.strip()); return 2
        g = json.loads(r.stdout)
    g["source"] = script.read_text()
    sc = front.from_graph(g, a.cut, a.variant)
    if not tk and not sc.beats:
        # a script of quoted lines needs the takes' words; a .brief of open beats alone can bind silent footage
        print(f"REFUSED  no transcripts in {space / 'media'} (each take needs its words.json: import_media writes it)"); return 2
    sheet.pick_alternatives(sc, tk)  # `a || b` lines settled before they're placed
    tl = front.resolve(sc, space, tk, work)
    if not tl.clips:
        # nothing to cut: an empty composition renders nothing and the engine times out on it (an agent session, 2026-10-17:
        # both Arock scripts, a silent screen recording no quoted line can come from)
        files = front.media_files(space / "media")
        silent = sorted(k for k in files if k not in tk)
        print(f"REFUSED  no line of the script is said in a take with picture ({len(sc.lines)} lines, none found)"
              + (f"; takes with no words: {', '.join(silent)} (a line can't come from them: place them with an open beat, `~ screen: ...`)" if silent else ""))
        if sc.beats:
            print(f"open beats waiting for the footage search (P3): {len(sc.beats)}")
        print(sheet.sheet(sc, tk))
        return 2
    if a.json:
        Path(a.json).write_text(json.dumps({"script": ir.to_json(sc), "timeline": ir.to_json(tl)}, indent=1, default=str))
    refused, unsounded = sdk.write(tl, space, edit, work, sound=not a.no_sound)
    if sc.beats:
        # what the footage can't give, as items to act on (reshoot, screen recording, stock, a part), beside the edit
        from compiler import moments
        need = moments.needs(sc.beats, tl, space)
        sdk.sidecar(edit, "needs.json").write_text(json.dumps(need, indent=1, default=str))
        if need:
            print("Needs (" + sdk.sidecar(edit, "needs.json").name + "):")
            print("\n".join(f"- {n['kind']}: {n.get('do') or n['why']}" for n in need))
    if sc.checks:
        # a .brief's ! rules beside the edit, answered where the compiler can and the rest for `raea check` on the render
        sdk.sidecar(edit, "checks.json").write_text(json.dumps(sheet.checks(sc, tk), indent=1, default=str))
    print(f"COMPILED  {edit.name}: {tl.frames / tl.fps:.1f} s, {len(tl.clips)} clips, {len(tl.parts)} parts"
          f"{f' ({len(refused)} refused)' if refused else ''}")
    print(sheet.sheet(sc, tk))
    if refused:
        print("Parts refused:")
        for r in refused:
            print(f"- {r['error']}: {r['part'][:140]}")
    if tl.notes:
        print("Footage:"); print("\n".join(f"- {n}" for n in tl.notes))
    if unsounded:
        print("Sound not added:"); print("\n".join(unsounded))
    issues, _ = sdk.check(space, edit)
    if issues:
        print("engine:"); print("\n".join("  " + l for l in issues))
    return 0


if __name__ == "__main__":
    sys.exit(main())
