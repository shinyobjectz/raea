#!/usr/bin/env python3
"""Render a downloadable model's thumbnail in raea (sdk/motion/docs/game-library.md).

    python3 -I sdk/motion/library/games/thumb.py ID [--clip NAME] [--time S] [--work DIR]

Fetches the record with `raea-meaning games-fetch`, frames the model from its glTF bounds (node transforms applied),
renders one 512x512 frame with raea-motion and writes thumbs/ID.jpg (256x256). A picture rendered from a CC0 or CC-BY
model carries the model's licence; the record then needs "thumbnail" and "thumb_licence" (the script prints them).
A render that fails (JPEG textures, for one) is reported, which is worth recording on the record as `raea`.
"""
import json
import math
import os
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
MEANING = os.environ.get("RAEA_MEANING", str(ROOT / "ai/meaning/target/release/raea-meaning"))
MOTION = os.environ.get("RAEA_MOTION_BIN", str(ROOT / "sdk/react/target/release/raea-motion"))


def gltf_json(path):
    b = path.read_bytes()
    if b[:4] == b"glTF":
        n = struct.unpack("<I", b[12:16])[0]
        return json.loads(b[20:20 + n])
    return json.loads(b)


def mat_mul(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(4)) for j in range(4)] for i in range(4)]


def trs(node):
    if "matrix" in node:
        m = node["matrix"]
        return [[m[c * 4 + r] for c in range(4)] for r in range(4)]
    t = node.get("translation", [0, 0, 0])
    x, y, z, w = node.get("rotation", [0, 0, 0, 1])
    s = node.get("scale", [1, 1, 1])
    r = [[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
         [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
         [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]]
    return [[r[i][0] * s[0], r[i][1] * s[1], r[i][2] * s[2], t[i]] for i in range(3)] + [[0, 0, 0, 1]]


def bounds(j, scene=0):
    lo, hi = [math.inf] * 3, [-math.inf] * 3
    eye = [[1 if i == k else 0 for k in range(4)] for i in range(4)]

    def walk(n, parent):
        node = j["nodes"][n]
        m = mat_mul(parent, trs(node))
        if "mesh" in node:
            # glTF ignores a skinned mesh's own transform when drawing, but its joints' bind pose (joint world x
            # inverse bind) is, in nearly every export, that same transform: so the node's world matrix frames it
            mm = m
            for p in j["meshes"][node["mesh"]]["primitives"]:
                a = j["accessors"][p["attributes"]["POSITION"]]
                if "min" not in a:
                    continue
                for cx in (a["min"][0], a["max"][0]):
                    for cy in (a["min"][1], a["max"][1]):
                        for cz in (a["min"][2], a["max"][2]):
                            v = [sum(mm[i][k] * c for k, c in enumerate((cx, cy, cz, 1))) for i in range(3)]
                            for i in range(3):
                                lo[i], hi[i] = min(lo[i], v[i]), max(hi[i], v[i])
        for c in node.get("children", []):
            walk(c, m)

    scenes = j.get("scenes") or [{"nodes": list(range(len(j["nodes"])))}]
    for n in scenes[j.get("scene", scene)]["nodes"]:
        walk(n, eye)
    return lo, hi


def main():
    args = sys.argv[1:]
    if not args:
        sys.exit(__doc__)
    rid = args[0]
    flag = lambda n, d=None: args[args.index(n) + 1] if n in args else d
    work = Path(flag("--work") or tempfile.mkdtemp(prefix="raea-thumb-"))
    (work / "media").mkdir(parents=True, exist_ok=True)
    got = json.loads(subprocess.run([MEANING, "games-fetch", rid], check=True, capture_output=True, text=True).stdout)
    main_file = Path(got["main"])
    if main_file.is_dir() or not main_file.exists():
        sys.exit(f"{rid}: the record names no single model file (main); pick one from {got['dir']}")
    # the model and whatever it references, beside the edit
    for f in Path(got["dir"]).rglob("*"):
        if f.is_file() and f.suffix.lower() in (".glb", ".gltf", ".bin", ".png", ".jpg", ".jpeg", ".ktx2"):
            dst = work / "media" / f.relative_to(got["dir"])
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(f, dst)
    rel = main_file.relative_to(got["dir"]).as_posix()
    lo, hi = bounds(gltf_json(main_file))
    size = max(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2], 1e-6)
    k = 1.6 / size
    cx, cz = (lo[0] + hi[0]) / 2, (lo[2] + hi[2]) / 2
    clip = flag("--clip")
    anim = f' animation={json.dumps(clip)} time={{{float(flag("--time", "0.5"))}}}' if clip else ""
    (work / "edit.tsx").write_text(f'''import {{ AbsoluteFill, Composition, registerRoot, staticFile }} from "@raea/motion";
import {{ AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, Model, PerspectiveCamera }} from "@raea/motion/3d";
function Shot() {{
  return (
    <AbsoluteFill style={{{{ background: "#2a3242" }}}}>
      <Canvas3D>
        <PerspectiveCamera position={{[1.5, 1.5, 3.6]}} lookAt={{[0, {(hi[1] - lo[1]) * k / 2:.3f}, 0]}} fov={{36}} />
        <AmbientLight intensity={{0.55}} />
        <DirectionalLight position={{[3, 6, 4]}} intensity={{1.3}} castShadow />
        <Mesh position={{[0, -0.05, 0]}}><Box args={{[8, 0.1, 8]}} /><MeshStandardMaterial color="#d9d4c7" roughness={{0.9}} /></Mesh>
        <Model src={{staticFile({json.dumps(rel)})}}{anim} scale={{{k:.5f}}} position={{[{-cx * k:.4f}, {-lo[1] * k:.4f}, {-cz * k:.4f}]}} />
      </Canvas3D>
    </AbsoluteFill>
  );
}}
registerRoot(() => <Composition id="thumb" component={{Shot}} width={{512}} height={{512}} fps={{30}} durationInFrames={{1}} />);
''')
    png = work / "t.png"
    r = subprocess.run(["nice", "-n", "15", MOTION, "render", str(work / "edit.tsx"), "--frame", "0", "-o", str(png)], capture_output=True, text=True)
    if r.returncode != 0 or not png.exists():
        lines = (r.stdout + r.stderr).splitlines()
        why = [l for l in lines if "REFUSED" in l] or [l for l in lines if " ERROR " in l or "error:" in l]
        print(json.dumps({"id": rid, "raea": "fails: " + (why[-1] if why else "render failed")[:300]}))
        if not flag("--work"):
            shutil.rmtree(work)
        sys.exit(1)
    out = HERE / "thumbs" / f"{rid}.jpg"
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", str(png), "-vf", "scale=256:256", "-q:v", "5", str(out)], check=True)
    shutil.rmtree(work)
    print(json.dumps({"id": rid, "thumbnail": f"thumbs/{rid}.jpg", "thumb_licence": f"{got['licence']} (rendered by raea from the model)", "raea": "renders", "fit_scale": round(k, 5)}))


if __name__ == "__main__":
    main()
