"""A workspace's shared asks against its pieces (P3b, brief/format.feature): each beat of a brief the pieces `Uses:`
(the client's ask) is covered by a piece when the footage that piece cuts shows it: among the frames of its clips
where the ask's finds hold, the ask's phrase scores FIND_THRESHOLD or more (moments.py). Run by `raea-brief compile
--all SPACE` after every piece is compiled, from each piece's graph and IR in SPACE/.score.

    uv run -q python ~/raea/score/cover.py SPACE
"""
import json, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from compiler import front, moments  # noqa: E402


def main():
    space = Path(sys.argv[1]).resolve()
    work = space / ".score"
    pieces = {}
    for g in sorted(work.glob("*.graph.json")):
        name = g.name[: -len(".graph.json")]
        # one IR per deliverable: NAME.ir.json, or NAME-9x16.ir.json and the rest when the piece has several
        for ir in [work / f"{name}.ir.json"] + sorted(work.glob(f"{name}-*.ir.json")):
            if ir.exists():
                pieces[ir.name[: -len(".ir.json")]] = (json.loads(g.read_text()), json.loads(ir.read_text())["timeline"])
    asks = {}
    for name, (g, _) in pieces.items():
        for a in g.get("asks", []):
            key = (str(a["from"]), a["at"][0])
            asks.setdefault(key, (a, []))[1].append(name)
    if not asks:
        return 0
    tk = front.takes(space / "media")
    by_file = {}
    for (src, _), (a, users) in sorted(asks.items()):
        by_file.setdefault(src, []).append((a, users))
    for src, items in by_file.items():
        print(f"{Path(src).name}: its beats across the pieces that use it")
        for a, users in items:
            q = moments.embed_text([a["phrase"]])[0]
            finds = [f for f in a.get("find", []) if isinstance(f, str) and (f in moments.DETECTED or ":" in f)]
            got = []
            for name in users:
                best = (0.0, None)
                for c in pieces[name][1]["clips"]:
                    r = moments.rows(space, c["file"], tk.get(c["take"]))
                    if not r or q is None:
                        continue
                    ms, col, dim = r
                    for i, m in enumerate(ms):
                        if c["src_in"] <= m["t"] <= c["src_out"] and all(moments.holds(f, m) for f in finds):
                            s = sum(x * y for x, y in zip(q, col[i * dim:(i + 1) * dim]))
                            if s > best[0]:
                                best = (s, f"{c['take']} {m['t']:.1f} s")
                got.append((name, best))
            hit = [(n, b) for n, b in got if b[0] >= moments.FIND_THRESHOLD]
            head = f"- ~ {a['kind']}: {a['phrase']}"
            if hit:
                print(head + ": covered by " + ", ".join(f"{n} ({b[0]:.3f}, {b[1]})" for n, b in hit))
            else:
                n, b = max(got, key=lambda x: x[1][0]) if got else ("", (0.0, None))
                print(head + ": by NO piece" + (f" (closest {n}, {b[0]:.3f} at {b[1]})" if b[1] else " (no ingested footage in any piece)"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
