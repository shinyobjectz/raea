#!/usr/bin/env python3
"""Build sdk/motion/library/games/catalogue.jsonl (sdk/motion/docs/game-library.md).

    python3 -I sdk/motion/library/games/build.py [--cache DIR] [--refresh]

Sources, merged in this order (a later record with the same id replaces an earlier one):
  1. Bevy's examples at v0.19.1 (examples/README.md, pinned), one `example` record each, pointed at the raea-bevy topic
     that covers the same thing in TSX.
  2. bevyengine/bevy-assets (the successor of awesome-bevy): Apps/Games -> `game`, Learning -> `example`, a few plugin
     categories -> `plugin`. Each GitHub repo's licence, description and Bevy version come from the GitHub API through
     `gh` and are cached in sources/repos.json, so a rebuild needs no network for them.
  3. sources/*.jsonl, curated by hand: open-source games, game-design patterns, free 3D assets and animations.
  4. tsx/*.tsx: raea TSX equivalents. A file's first comment line `// record: ID` attaches it to that record.

Only records whose licence allows it carry `ship: "download"` and `files`; everything else is `ship: "link"`.
Then rebuild the vectors: `raea-meaning games-index`.
"""
import json
import os
import re
import subprocess
import sys
import urllib.parse
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
BEVY_TAG = "v0.19.1"
EXAMPLES_URL = f"https://raw.githubusercontent.com/bevyengine/bevy/{BEVY_TAG}/examples/README.md"
ASSETS_TREE = "repos/bevyengine/bevy-assets/git/trees/main?recursive=1"
ASSETS_RAW = "https://raw.githubusercontent.com/bevyengine/bevy-assets/main/"
DOWNLOADABLE = {"CC0-1.0", "CC-BY-3.0", "CC-BY-4.0", "MIT", "Apache-2.0", "Unlicense", "MIT OR Apache-2.0"}
KINDS = {"asset", "animation", "rig", "example", "game", "pattern", "plugin"}

# Bevy example categories worth searching for game work, and the raea-bevy topic each maps to by default.
CATEGORIES = {
    "2D Rendering": "motion/games",
    "3D Rendering": None,  # by keyword, below
    "Animation": "bevy/animation",
    "Assets": "bevy/assets",
    "Audio": "motion/sound",
    "Camera": "bevy/scene",
    "ECS (Entity Component System)": "bevy/games",
    "Games": "bevy/games",
    "Gizmos": "bevy/limits",
    "Input": "bevy/games",
    "Math": "bevy/scene",
    "Movement": "bevy/games",
    "Picking": "bevy/limits",
    "Scene": "bevy/assets",
    "Shaders": "bevy/limits",
    "State": "bevy/games",
    "Time": "bevy/animation",
    "Transforms": "bevy/scene",
    "UI (User Interface)": "bevy/scene",
    "glTF": "bevy/assets",
}
RENDER_3D = [
    (r"light|shadow|spot|point|directional|ambient|irradiance|lightmap", "bevy/lights"),
    (r"bloom|fog|tonemap|exposure|depth of field|motion blur|ssao|ambient occlusion|anti-?alias|post|color grading|chromatic", "bevy/post"),
    (r"pbr|material|texture|parallax|clearcoat|transmission|anisotropy|emissive|alpha|transparen|specular|normal map|deferred", "bevy/materials"),
    (r"shape|mesh|primitive|vertex|wireframe|generate|meshlet", "bevy/meshes"),
    (r"camera|orthographic|projection|viewport|render to texture|split", "bevy/scene"),
]
# topics whose features raea has; the rest are pointers to what isn't there yet
NOTE = {
    "bevy/lights": "In raea: AmbientLight, DirectionalLight, PointLight, SpotLight with shadows (raea craft bevy/lights).",
    "bevy/post": "In raea: Bloom, Fog, ToneMapping and exposure; no SSAO, depth of field or motion blur yet (raea craft bevy/post).",
    "bevy/materials": "In raea: MeshStandardMaterial and MeshPhysicalMaterial props, picture maps; no normal or roughness maps or transmission yet (raea craft bevy/materials).",
    "bevy/meshes": "In raea: Box, Sphere, Plane, Cylinder, Cone, Torus, Text3D and Manifold solids (raea craft bevy/meshes).",
    "bevy/scene": "In raea: Canvas3D, one PerspectiveCamera, Group transforms, 2D page over the 3D (raea craft bevy/scene).",
    "bevy/animation": "In raea: every value from the frame (interpolate, spring, Sequence), glTF clips posed by time (raea craft bevy/animation).",
    "bevy/games": "In raea: a pure game step with useGameState, useInput and useGameEvents, an ECS world drawn by Entities (raea craft bevy/games).",
    "bevy/assets": "In raea: <Model src={staticFile('x.glb')} animation='Run'> and ImagePlane (raea craft bevy/assets).",
    "bevy/limits": "Not in raea's SDK yet (custom shaders, picking, gizmos, physics engines); see raea craft bevy/limits for the workaround.",
    "motion/games": "In raea: 2D games are drawn on the page in TSX (raea craft motion/games).",
    "motion/sound": "In raea: sound comes from the composition's Audio and raea's mixer, not Bevy audio (raea craft motion/sound).",
}


def flat_toml(text):
    """bevy-assets records are flat `key = "value"` files; Python 3.9 has no tomllib."""
    out = {}
    for line in text.splitlines():
        m = re.match(r'^\s*([A-Za-z_]+)\s*=\s*"((?:[^"\\]|\\.)*)"', line)
        if m:
            out[m.group(1)] = m.group(2).replace('\\"', '"')
    return out


def gh(path):
    out = subprocess.run(["gh", "api", path], capture_output=True, text=True)
    return json.loads(out.stdout) if out.returncode == 0 else None


def get(url, cache, name):
    p = cache / name
    if not p.exists():
        p.parent.mkdir(parents=True, exist_ok=True)
        with urllib.request.urlopen(url, timeout=60) as r:
            p.write_bytes(r.read())
    return p.read_text()


def slug(t):
    return re.sub(r"[^a-z0-9]+", "-", t.lower()).strip("-")


def bevy_examples(cache):
    text = get(EXAMPLES_URL, cache, "bevy-examples-README.md")
    out, cat = [], None
    for line in text.splitlines():
        if line.startswith("## Platform-Specific") or line.startswith("## Tests"):
            cat = None
        m = re.match(r"^### (.+)$", line)
        if m:
            cat = m.group(1).strip() if m.group(1).strip() in CATEGORIES else None
            continue
        m = re.match(r"^\[(.+?)\]\(\.\./examples/(.+?)\) \| (.+)$", line)
        if not (cat and m):
            continue
        name, path, desc = m.group(1), m.group(2), m.group(3).strip()
        topic = CATEGORIES[cat]
        if topic is None:
            hay = f"{name} {desc} {path}".lower()
            topic = next((t for pat, t in RENDER_3D if re.search(pat, hay)), "bevy/limits")
        stem = Path(path).stem if not path.endswith("/main.rs") else Path(path).parent.name
        cat_words = re.sub(r"\(.*?\)", "", cat).strip().lower()
        out.append({
            "id": f"bevy-{slug(cat_words)}-{slug(stem)}",
            "kind": "example",
            "title": f"Bevy example: {name}",
            "description": f"{desc.rstrip('.')}. Bevy {BEVY_TAG[1:]} {cat_words} example ({path}). {NOTE[topic]}",
            "tags": ["bevy", "example", cat_words] + [w for w in re.findall(r"[a-z0-9]+", stem.lower()) if len(w) > 2],
            "licence": "MIT OR Apache-2.0",
            "attribution": "Bevy contributors",
            "ship": "link",
            "source": f"https://github.com/bevyengine/bevy/blob/{BEVY_TAG}/examples/{path}",
            "topic": topic,
        })
    return out


def bevy_assets(cache, refresh):
    repos_file = HERE / "sources" / "repos.json"
    repos = json.loads(repos_file.read_text()) if repos_file.exists() else {}
    tree_p = cache / "bevy-assets-tree.json"
    if refresh or not tree_p.exists():
        tree_p.write_text(json.dumps(gh(ASSETS_TREE)))
    tree = json.loads(tree_p.read_text())["tree"]
    wanted = {
        "Apps/Games/": "game",
        "Learning/": "example",
        "Assets/Templates/": "plugin",
        "Assets/3D/": "plugin",
        "Assets/Animation/": "plugin",
        "Assets/Physics/": "plugin",
        "Assets/Camera/": "plugin",
        "Assets/Input/": "plugin",
    }
    out = []
    for item in tree:
        p = item["path"]
        kind = next((k for pre, k in wanted.items() if p.startswith(pre)), None)
        if not kind or not p.endswith(".toml") or p.endswith("_category.toml"):
            continue
        raw = get(ASSETS_RAW + urllib.parse.quote(p), cache / "bevy-assets", p.replace("/", "_"))
        t = flat_toml(raw)
        link = t.get("link", "")
        gm = re.match(r"https?://github\.com/([^/]+)/([^/#?]+)", link)
        info = {}
        if gm:
            full = f"{gm.group(1)}/{gm.group(2).removesuffix('.git')}"
            if full not in repos or refresh:
                d = gh(f"repos/{full}") or {}
                bv = None
                if kind == "game" and d:
                    c = gh(f"repos/{full}/contents/Cargo.toml")
                    if c and c.get("content"):
                        import base64
                        cargo = base64.b64decode(c["content"]).decode("utf-8", "replace")
                        m = re.search(r'^bevy\s*=\s*(?:\{[^}]*version\s*=\s*)?"[=^~]?([0-9.]+)', cargo, re.M)
                        bv = m.group(1) if m else None
                repos[full] = {
                    "licence": ((d.get("license") or {}).get("spdx_id")) or "unknown",
                    "description": d.get("description") or "",
                    "stars": d.get("stargazers_count"),
                    "pushed": (d.get("pushed_at") or "")[:10],
                    "topics": d.get("topics") or [],
                    "archived": d.get("archived"),
                    "bevy": bv,
                    "missing": not d,
                }
            info = repos[full]
        section = p.split("/")[1] if p.startswith("Assets/") else p.split("/")[0]
        name = t.get("name", Path(p).stem)
        desc = t.get("description", "").strip()
        extra = info.get("description", "").strip()
        if extra and extra.lower()[:24] not in desc.lower() and desc.lower()[:24] not in extra.lower():
            desc = f"{desc.rstrip('.')}. {extra}"
        what = {"game": "An open-source game made with Bevy", "example": "A Bevy tutorial or learning resource", "plugin": f"A Bevy {section.lower()} crate"}[kind]
        rec = {
            "id": {"game": "game", "example": "learn", "plugin": "crate"}[kind] + "-" + slug(name),
            "kind": kind,
            "title": name,
            "description": f"{what}: {desc.rstrip('.')}." + (f" Bevy {info['bevy']}." if info.get("bevy") else ""),
            "tags": ["bevy", section.lower()] + [x for x in info.get("topics", []) if x not in ("bevy", "rust", "bevy-engine", "game", "rust-lang")][:6],
            "licence": {"NOASSERTION": "custom or dual (see repo)"}.get(info.get("licence"), info.get("licence", "unknown")) if gm else "unknown",
            "attribution": gm.group(1) if gm else "",
            "ship": "link",
            "source": link,
            "listed": f"https://github.com/bevyengine/bevy-assets/blob/main/{urllib.parse.quote(p)}",
        }
        if gm:
            rec["repo"] = f"https://github.com/{gm.group(1)}/{gm.group(2)}"
        if info.get("bevy"):
            rec["bevy_version"] = info["bevy"]
        if info.get("stars") is not None:
            rec["stars"] = info["stars"]
        if info.get("archived") or info.get("missing"):
            rec["tags"].append("archived" if info.get("archived") else "repo-gone")
        if kind == "plugin":
            rec["topic"] = "rows"  # extending the SDK: sdk/skills/raea-bevy/rows.md
        out.append(rec)
    repos_file.write_text(json.dumps(dict(sorted(repos.items())), indent=1) + "\n")
    return out


def curated():
    out = []
    for f in sorted((HERE / "sources").glob("*.jsonl")):
        for n, line in enumerate(f.read_text().splitlines(), 1):
            if line.strip():
                r = json.loads(line)
                r.setdefault("ship", "link")
                out.append(r)
    return out


def attach_tsx(recs):
    by = {r["id"]: r for r in recs}
    for f in sorted((HERE / "tsx").glob("*.tsx")):
        m = re.search(r"^// record: ([a-z0-9-]+(?:, *[a-z0-9-]+)*)", f.read_text(), re.M)
        if not m:
            sys.exit(f"{f}: needs a `// record: ID` line")
        for rid in [x.strip() for x in m.group(1).split(",")]:
            if rid not in by:
                sys.exit(f"{f}: no record {rid}")
            by[rid]["tsx"] = f"tsx/{f.name}"


def check(recs):
    seen = set()
    for r in recs:
        assert r["id"] not in seen, f"duplicate id {r['id']}"
        seen.add(r["id"])
        assert r["kind"] in KINDS, f"{r['id']}: kind {r['kind']}"
        for k in ("title", "description", "licence", "source"):
            assert r.get(k), f"{r['id']}: no {k}"
        if r.get("ship") == "download":
            assert r["licence"] in DOWNLOADABLE, f"{r['id']}: {r['licence']} can't ship"
            assert r.get("files"), f"{r['id']}: download without files"
            if r["licence"].startswith("CC-BY"):
                assert r.get("attribution"), f"{r['id']}: CC-BY without attribution"
        if r.get("thumbnail"):
            assert r.get("thumb_licence"), f"{r['id']}: a thumbnail needs thumb_licence"
            if not (HERE / r["thumbnail"]).exists():
                print(f"warning: {r['id']}: {r['thumbnail']} isn't made yet; left out", file=sys.stderr)
                del r["thumbnail"], r["thumb_licence"]


def main():
    args = sys.argv[1:]
    cache = Path(args[args.index("--cache") + 1]) if "--cache" in args else Path(os.environ.get("TMPDIR", "/tmp")) / "raea-games-build"
    refresh = "--refresh" in args
    recs = {}
    harvested = bevy_examples(cache) + bevy_assets(cache, refresh)
    mine = curated()
    for r in harvested + mine:
        if r["id"] in recs:
            recs[r["id"]] = {**recs[r["id"]], **r}  # a curated record refines a harvested one
        else:
            recs[r["id"]] = r
    # a curated record also replaces a harvested one for the same repo under another id
    key = lambda r: (r.get("repo") or r.get("source") or "").lower().rstrip("/").removesuffix(".git").replace("http://", "https://").replace("www.", "")
    shared = lambda k: k.startswith("https://github.com/bevyengine/bevy") and not k.endswith("bevy-assets")
    mine_ids = {r["id"] for r in mine}
    mine_keys = {key(recs[i]) for i in mine_ids if not shared(key(recs[i]))}
    recs = [r for r in recs.values() if r["id"] in mine_ids or key(r) not in mine_keys]
    for r in recs:
        if r.get("files"):
            r["download_bytes"] = sum(f.get("bytes") or f.get("bytes_estimate") or 0 for f in r["files"])
    attach_tsx(recs)
    check(recs)
    kind_order = ["asset", "animation", "rig", "pattern", "game", "example", "plugin"]
    recs.sort(key=lambda r: (kind_order.index(r["kind"]), r["id"]))
    (HERE / "catalogue.jsonl").write_text("".join(json.dumps(r, ensure_ascii=False, separators=(",", ":")) + "\n" for r in recs))
    counts = {k: sum(r["kind"] == k for r in recs) for k in kind_order}
    print(f"{len(recs)} records: {counts}; downloadable {sum(r.get('ship') == 'download' for r in recs)}")


if __name__ == "__main__":
    main()
