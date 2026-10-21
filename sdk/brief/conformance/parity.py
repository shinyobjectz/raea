"""P1's parity: every score script the bench wrote, converted to a .brief (raea-brief convert) and read back by the
Rust parser (raea-brief parse), gives the compiler the same Script front.parse reads from the Fountain, in all the
notation defines; and what the notation doesn't define, which front.parse reads as unknown or drops, is in the .brief
as a comment, so nothing is lost.

    uv run -q python ~/raea/sdk/brief/conformance/parity.py [--keep DIR]

The corpus is ai/score/bench/out (not committed): s2b's ten compiled scripts and the 100 written cold in stage 1
(a2, a2a, a3, a3a, c3), read where they are.
"""
import json, re, subprocess, sys, tempfile
from pathlib import Path

RAEA = Path.home() / "raea"
sys.path.insert(0, str(RAEA / "ai/score")); sys.path.insert(0, str(RAEA / "ai/score/bench"))
from compiler import front  # noqa: E402
import fountain  # noqa: E402  score's Fountain reader, out of the compiler since P5
from compiler.sheet import MUSTS, notes_in  # noqa: E402

BIN = RAEA / "sdk/brief/target/release/raea-brief"
OUT = RAEA / "ai/score/bench/out"
READ = {":root": {"background", "music", "music-gain", "sfx-gain", "platform", "eyes"}, "captions": {"place", "keys"}}


def corpus():
    fs = sorted(OUT.glob("s2b/*/script.fountain"))
    for arm in ("a2_fountain_css", "a2a_fountain", "a3_refs", "a3a_refs_plain", "c3_xsheet"):
        fs += sorted((OUT / arm).glob("*.txt"))
    return fs


def shape(sc):
    """what the compiler reads from a Script: everything the notation defines"""
    note = lambda n: (n.kind, n.props, [u for u in n.utils if front.UTIL.match(u)], n.anchor)
    # a note left open is words of the line to front.parse (a misreading: no take says them); a note with nothing the
    # notation reads in it does nothing
    lines = [(l.role, l.take, re.sub(r"\s*\[\[[^\]]*\]?$", "", l.text), l.delivery, l.punch,
              [note(n) for n in l.notes if n.kind or note(n)[2]]) for l in sc.lines]
    style = {s: {k: v for k, v in d.items() if k in READ[s]} for s, d in sc.style.items() if s in READ}
    style = {s: d for s, d in style.items() if d or s == "captions"}
    return {"title": sc.title, "format": sc.format, "secs": sc.secs, "sections": sc.sections, "lines": lines,
            "screen_text": sc.screen_text, "style": style}


def unknowns(text, sc):
    """what front.parse reads but the notation doesn't define: unknown utilities, title keys, stylesheet properties"""
    out = [u for n in notes_in(text) for u in n.utils if not front.UTIL.match(u)]
    body, css = (front.strip_fence(text).split("\n---\n", 1) + [""])[:2]
    for sel, b in re.findall(r"([^{}]+)\{([^}]*)\}", css):
        for d in [x.strip() for x in b.split(";") if x.strip()]:
            if sel.strip() == "@must":
                if not any(d.lower().rstrip(".") == m for m in MUSTS): out.append(d)
            elif sel.strip() not in READ or d.split(":", 1)[0].strip() not in READ[sel.strip()]:
                out.append(d)
    return out


def main():
    keep = Path(sys.argv[sys.argv.index("--keep") + 1]) if "--keep" in sys.argv else Path(tempfile.mkdtemp())
    keep.mkdir(parents=True, exist_ok=True)
    rows, fails = [], []
    for f in corpus():
        text = f.read_text()
        name = f"{f.parent.name}__{f.stem}" if f.name == "script.fountain" else f"{f.parent.name}__{f.stem}"
        b = keep / f"{name}.brief"
        b.write_text(subprocess.run([BIN, "convert", f], capture_output=True, text=True, check=True).stdout)
        p = subprocess.run([BIN, "parse", b], capture_output=True, text=True)
        if p.returncode:
            fails.append((name, "refused", p.stderr.strip().splitlines()[:3])); rows.append((name, False, False, False)); continue
        a = fountain.parse(text)
        g = json.loads(p.stdout)
        same = shape(a) == shape(front.from_graph(dict(g, source=b.read_text())))
        if not same:
            sa, sb = shape(a), shape(front.from_graph(g))
            diff = [k for k in sa if sa[k] != sb[k]]
            detail = []
            for k in diff:
                if k == "lines":
                    for i, (x, y) in enumerate(zip(sa[k], sb[k])):
                        if x != y:
                            detail.append(f"line {i + 1}: {x} != {y}"); break
                    if len(sa[k]) != len(sb[k]):
                        detail.append(f"{len(sa[k])} lines != {len(sb[k])}")
                else:
                    detail.append(f"{k}: {sa[k]} != {sb[k]}")
            fails.append((name, "differs", detail[:2]))
        bt = b.read_text()
        kept = all(u in bt for u in unknowns(text, a))
        if not kept:
            fails.append((name, "dropped", [u for u in unknowns(text, a) if u not in bt][:3]))
        fmt = subprocess.run([BIN, "fmt", b], capture_output=True, text=True)
        b2 = keep / f"{name}.fmt.brief"; b2.write_text(fmt.stdout)
        p2 = subprocess.run([BIN, "parse", b2], capture_output=True, text=True)
        g2 = json.loads(p2.stdout) if not p2.returncode else None
        strip = lambda g: json.dumps({k: v for k, v in g.items() if k != "file"}, sort_keys=True, default=str) if g else None
        drop_at = lambda g: re.sub(r'"from": "[^"]*"', '"from": 0', re.sub(r'"at": \[\s*\d+,\s*\d+\s*\]', '"at": 0', strip(g))) if g else None
        roundtrip = g2 is not None and drop_at(g) == drop_at(g2)
        if not roundtrip:
            fails.append((name, "fmt", (p2.stderr.strip().splitlines() or ["the graph changed"])[:2]))
        rows.append((name, same, kept, roundtrip))
    n = len(rows)
    print(f"{n} scripts: same Script {sum(r[1] for r in rows)}/{n}, nothing dropped {sum(r[2] for r in rows)}/{n}, fmt reads back the same {sum(r[3] for r in rows)}/{n}")
    print(f"converted .brief files in {keep}")
    for name, why, detail in fails[:30]:
        print(f"- {name}: {why}: {detail}")
    return 0 if all(all(r[1:]) for r in rows) else 1


if __name__ == "__main__":
    sys.exit(main())
