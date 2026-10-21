"""tools/goo: a box of goo on a table box, coarse, in seconds. The .rseq parses as the format says, it's empty before
the box's start and has triangles after, and two runs write the same bytes.

    $RAEA_GOO/.venv/bin/python -I tools/goo/test_run.py
"""

import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import run  # noqa: E402


def box_glb(path, lo, hi):
    """a closed box as a one-node glb named 'box', base colour orange"""
    import numpy as np

    x0, y0, z0 = lo
    x1, y1, z1 = hi
    p = np.array([[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]], "<f4")
    f = np.array([[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [3, 7, 6], [3, 6, 2], [0, 4, 7], [0, 7, 3], [1, 2, 6], [1, 6, 5]], "<u4")
    binb = p.tobytes() + f.tobytes()
    j = {"asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [{"name": "box", "mesh": 0}],
         "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1, "material": 0}]}],
         "materials": [{"pbrMetallicRoughness": {"baseColorFactor": [1.0, 0.3, 0.05, 1.0]}}],
         "buffers": [{"byteLength": len(binb)}],
         "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": p.nbytes}, {"buffer": 0, "byteOffset": p.nbytes, "byteLength": f.nbytes}],
         "accessors": [{"bufferView": 0, "componentType": 5126, "count": 8, "type": "VEC3", "min": list(map(float, lo)), "max": list(map(float, hi))},
                       {"bufferView": 1, "componentType": 5125, "count": 36, "type": "SCALAR"}]}
    js = json.dumps(j).encode()
    js += b" " * (-len(js) % 4)
    binb += b"\0" * (-len(binb) % 4)
    body = struct.pack("<II", len(js), 0x4E4F534A) + js + struct.pack("<II", len(binb), 0x004E4942) + binb
    Path(path).write_bytes(b"glTF" + struct.pack("<II", 2, 12 + len(body)) + body)


def main():
    d = Path(tempfile.mkdtemp(prefix="goo-test-"))
    box_glb(d / "box.glb", (-0.05, 0.751, -0.05), (0.05, 0.85, 0.05))
    spec = {"fps": 30, "frames": 24, "seed": 0, "dx": 0.01, "gravity": [0, -9.81, 0],
            "bounds": {"min": [-0.3, 0.6, -0.3], "max": [0.3, 1.0, 0.3]},
            "colliders": [{"box": {"min": [-0.2, 0.0, -0.2], "max": [0.2, 0.75, 0.2]}, "friction": 0.4}],
            "objects": [{"name": "box", "src": str(d / "box.glb"), "nodes": ["box"], "start": 0.2, "melt": 0.4,
                         "stiffness": 2e4, "viscosity": 2.0, "fill": "solid"}]}
    (d / "spec.json").write_text(json.dumps(spec))
    outs = []
    for k in range(2):
        out = d / f"o{k}.rseq"
        subprocess.run([sys.executable, "-I", str(HERE / "run.py"), str(d / "spec.json"), str(out)], check=True)
        outs.append(out)
    hd, frames = run.read_rseq(outs[0])
    assert hd["frames"] == 24 and len(frames) == 24, hd
    assert hd["attrs"] == ["position", "normal", "color"] and hd["codec"] == "zstd"
    start = 6  # 0.2 s at 30
    for k, (pos, nrm, col, idx) in enumerate(frames):
        if k < start:
            assert len(idx) == 0, f"frame {k} should be empty before the start"
        else:
            assert len(idx) > 0, f"frame {k} has no triangles"
            assert idx.max() < len(pos)
            assert abs(float((nrm ** 2).sum(1).mean()) - 1) < 0.05
            assert col[:, 0].mean() > col[:, 2].mean(), "the box's orange should carry"
    first, last = frames[start][0], frames[-1][0]
    assert last[:, 1].max() < first[:, 1].max() - 0.01, "the goo should slump"
    assert last[:, 1].min() > 0.74, "the goo should stay on the table"
    a, b = outs[0].read_bytes(), outs[1].read_bytes()
    assert a == b, "two runs wrote different bytes"
    st = json.loads(Path(str(outs[0]) + ".json").read_text())
    assert st["objects"][0]["hideFrame"] == 12, st
    print(f"ok: {len(a)} bytes, top {first[:, 1].max():.3f} -> {last[:, 1].max():.3f}, {st['seconds']}s", file=sys.stderr)
    shutil.rmtree(d, True)


if __name__ == "__main__":
    main()
