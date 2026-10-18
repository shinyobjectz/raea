#!/usr/bin/env python3
"""G4's pass test: find-or-make assets as snippets. find() tries the footage library first and falls back to
generation; the edit stores the asset's content hash, never the prompt; re-renders reuse identical bytes (even with
retrieval and generation switched off); regenerating is an explicit, recorded move that undo reverses."""
import hashlib, json, os, re, shutil, subprocess, sys, tempfile

B = os.path.expanduser("~/raea/rdb/target/release/rdb")
HOOK = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "hook")
work = tempfile.mkdtemp(prefix="g4-")
edit = os.path.join(work, "edit.duckdb")


def run(*a, env=None):
    r = subprocess.run([B, *a], capture_output=True, text=True, env={**os.environ, **(env or {})})
    return r


def frame(f, env=None):
    out = os.path.join(work, f"render-{len(os.listdir(work))}")
    r = run("render", edit, "--at", str(f), "-o", out, env=env)
    assert r.returncode == 0, r.stderr[-400:]
    return hashlib.sha256(open(os.path.join(out, f"frame-{f:05}.png"), "rb").read()).hexdigest()


def q(sql):
    r = run("query", edit, sql)
    assert r.returncode == 0, r.stderr
    return json.loads(r.stdout)


run("new", edit)
for block in open(os.path.join(HOOK, "snippets.sql")).read().split("\n--\n"):
    b = block.strip()
    if b and not all(l.strip().startswith("--") for l in b.splitlines()):
        assert run("apply", edit, b).returncode == 0

checks = []
r = run("apply", edit, "INSERT INTO image (id, src, x, y, width, height) VALUES "
        "('broll', find('a person talking to the camera'), 90, 1100, 420, 420), "
        "('product', find('hands holding a skincare bottle'), 570, 1100, 420, 420)")
assert r.returncode == 0, r.stderr
imgs = {x["id"]: x["src"] for x in q("SELECT id, src FROM image ORDER BY seq")}
prov = {x["hash"]: json.loads(x["origin"]) if isinstance(x["origin"], str) else x["origin"] for x in q("SELECT hash, origin FROM asset")}
how = {i: prov[s.split(":")[1]]["how"] for i, s in imgs.items()}
print("rows hold:", imgs)
print("provenance:", {i: (prov[s.split(':')[1]]['how'], round(prov[s.split(':')[1]].get('score', 0), 3)) for i, s in imgs.items()})
checks.append(("find() used the library for a phrase it has, generation for one it lacks", how == {"broll": "found", "product": "made"}))
checks.append(("rows store a content hash, never the prompt", all(re.fullmatch(r"asset:[0-9a-f]{64}", s) for s in imgs.values())
               and not any("skincare" in json.dumps(x) for x in q("SELECT * FROM image"))))
stored = sorted(os.listdir(os.path.join(work, "assets")))
checks.append(("the bytes are stored once, under their hash", all(f.split(".")[0] == hashlib.sha256(open(os.path.join(work, "assets", f), "rb").read()).hexdigest() for f in stored)))

f1 = frame(60)
off = {"RAEA_LIBRARY": os.path.join(work, "empty-library"), "RAEA_MEANING": "/usr/bin/false"}
os.makedirs(off["RAEA_LIBRARY"], exist_ok=True)
f2 = frame(60, env=off)
checks.append(("a re-render in a new process, with retrieval and generation switched off, is byte-identical", f1 == f2))

moves_before = len(q("SELECT n FROM move"))
r = run("apply", edit, "UPDATE image SET src = remake(src) WHERE id = 'product'")
assert r.returncode == 0, r.stderr
new = {x["id"]: x["src"] for x in q("SELECT id, src FROM image")}
newprov = {x["hash"]: json.loads(x["origin"]) if isinstance(x["origin"], str) else x["origin"] for x in q("SELECT hash, origin FROM asset")}
p_new = newprov[new["product"].split(":")[1]]
f3 = frame(60)
checks.append(("regenerate is an explicit, recorded move with the next seed", new["product"] != imgs["product"] and p_new["seed"] == 2
               and len(q("SELECT n FROM move")) == moves_before + 1 and f3 != f1))
r = run("apply", edit, "UPDATE image SET src = remake(src) WHERE id = 'broll'")
nb = {x["id"]: x["src"] for x in q("SELECT id, src FROM image")}
pb = {x["hash"]: json.loads(x["origin"]) if isinstance(x["origin"], str) else x["origin"] for x in q("SELECT hash, origin FROM asset")}[nb["broll"].split(":")[1]]
print("remade broll:", pb.get("how"), "rank", pb.get("rank"), "score", round(pb.get("score", 0), 3))
checks.append(("remaking a found asset takes the next-ranked match", r.returncode == 0 and pb.get("how") == "found" and pb.get("rank") == 1))
assert run("undo", edit, "2").returncode == 0
back = {x["id"]: x["src"] for x in q("SELECT id, src FROM image")}
checks.append(("undo puts the earlier assets and frames back", back == imgs and frame(60) == f1))

for name, ok in checks:
    print(("PASS " if ok else "FAIL ") + name)
print(f"G4: {sum(ok for _, ok in checks)}/{len(checks)}")
sys.exit(0 if all(ok for _, ok in checks) else 1)
