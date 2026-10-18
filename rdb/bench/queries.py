"""Phase 4's query check: the live hook edit built from its snippets, then each query in bench/queries.sql answered from
our columns against DuckDB's recorded answers (bench/ref/queries.json).   python3 bench/queries.py BIN EDIT"""
import subprocess, sys, json
B, E = sys.argv[1], sys.argv[2]
for b in [b.strip() for b in open("examples/hook/snippets.sql").read().split("\n--\n")]:
    if b and not all(l.strip().startswith("--") for l in b.splitlines()):
        r = subprocess.run([B, "apply", E, b], capture_output=True, text=True, cwd="examples/hook")
        assert r.returncode == 0, r.stderr
ref = json.load(open("bench/ref/queries.json"))
agg = False
def same(a, b):
    if isinstance(a, (int, float)) and isinstance(b, (int, float)): return float(a) == float(b) or (agg and abs(a - b) <= 1e-12 * max(abs(a), abs(b)))
    if isinstance(b, str) and isinstance(a, str):
        try: return json.loads(a) == json.loads(b)
        except Exception: return a == b
    return a == b
bad = 0
for r in ref:
    p = subprocess.run([B, "query", E, r["q"]], capture_output=True, text=True)
    if p.returncode: print("ERR", r["q"], p.stderr.strip()[:300]); bad += 1; continue
    agg = any(f in r["q"] for f in ("sum(", "avg("))
    got = [list(x.values()) for x in json.loads(p.stdout)]
    ok = len(got) == len(r["rows"]) and all(len(g) == len(w) and all(same(x, y) for x, y in zip(g, w)) for g, w in zip(got, r["rows"]))
    if not ok:
        bad += 1; print("DIFF", r["q"]); print("  want", str(r["rows"])[:300]); print("  got ", str(got)[:300])
print(f"{len(ref)-bad}/{len(ref)} queries answer as DuckDB did")
