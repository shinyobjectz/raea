"""The ink profile's parity: every .brief we have, written as an ink story (raea-brief ink) and lowered back
(raea-brief lower, then parse), gives the compiler the same graph: the same lines with their sections, takes,
speakers, delivery, punch, parts and variants, the same open beats, checks, stylesheet, title and format.

    uv run -q python ~/raea/brief/conformance/ink_parity.py DIR...    every *.brief in each DIR (and DIR/*.txt answers)
"""
import json, re, subprocess, sys, tempfile
from pathlib import Path

BIN = Path.home() / "raea/brief/target/release/raea-brief"


def graph(p):
    r = subprocess.run([str(BIN), "parse", str(p)], capture_output=True, text=True)
    return (json.loads(r.stdout), "") if r.returncode == 0 else (None, r.stderr.strip())


def shape(g):
    # ink collapses a run of spaces inside text to one (its own rule; quoted lines match transcripts by word, so it's moot)
    ws = lambda t: " ".join(t.split()) if isinstance(t, str) else t
    items = [(i["type"], i.get("section"), i.get("take"), i.get("speaker"), i.get("delivery"), i.get("punch"), ws(i.get("text") or i.get("phrase")),
              [(n["kind"], n["props"], n["utils"]) for n in i.get("notes", [])], i.get("find"), i.get("variants")) for i in g["items"] if i["type"] != "transition"]
    return {"items": items, "checks": sorted(c["rule"] for c in g["checks"]), "style": g["style"], "title": g["title"], "format": g["format"],
            "sections": [s["name"] for s in g["sections"]], "synopsis": [s["synopsis"] for s in g["sections"]]}


def main():
    files = []
    for d in map(Path, sys.argv[1:]):
        files += sorted(d.glob("*.brief")) if d.is_dir() else [d]
        files += sorted(d.glob("*.txt")) if d.is_dir() else []
    work = Path(tempfile.mkdtemp())
    n = same = skipped = 0
    fails = []
    for f in files:
        text = f.read_text()
        m = re.search(r"```[\w-]*\n(.*?)```", text, re.S)
        b = work / f"{f.parent.name}__{f.stem}.brief"; b.write_text(m.group(1) if m else text)
        g, err = graph(b)
        if g is None:
            skipped += 1; continue  # a brief that's refused has no graph to compare
        n += 1
        ink = work / (b.stem + ".ink")
        r = subprocess.run([str(BIN), "ink", str(b)], capture_output=True, text=True); ink.write_text(r.stdout)
        g2, err2 = graph(ink)
        if g2 is None:
            fails.append((b.name, "refused as ink", err2.splitlines()[:2])); continue
        a, c = shape(g), shape(g2)
        if a == c:
            same += 1
        else:
            k = next(k for k in a if a[k] != c[k])
            if k == "items":
                i = next((i for i, (x, y) in enumerate(zip(a[k], c[k])) if x != y), min(len(a[k]), len(c[k])))
                fails.append((b.name, f"item {i + 1}", [a[k][i] if i < len(a[k]) else None, c[k][i] if i < len(c[k]) else None]))
            else:
                fails.append((b.name, k, [str(a[k])[:200], str(c[k])[:200]]))
    print(f"{n} briefs that parse ({skipped} refused, left out): the same graph through ink {same}/{n}")
    print(f"the ink stories are in {work}")
    for x in fails[:15]:
        print("-", x[0], x[1], str(x[2])[:400])
    return 0 if same == n else 1


if __name__ == "__main__":
    sys.exit(main())
