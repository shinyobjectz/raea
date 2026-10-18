#!/usr/bin/env python3
"""Writes media/spinner.glb: a small animated glTF made here (no third-party asset), for the Model fixture.

A red box on a blue post under a root node, two clips: "Spin" (the root turns once around y in 2 s) and "Bob" (the
box rises and falls over 1 s). Standard library only.
"""
import json, math, struct, sys
from pathlib import Path


def box(w, h, d, y0):
    """Flat-shaded box: 24 vertices (4 per face) with normals, 36 indices."""
    pos, nor, idx = [], [], []
    faces = [((1, 0, 0), (0, 1, 0), (0, 0, 1)), ((-1, 0, 0), (0, 1, 0), (0, 0, -1)), ((0, 1, 0), (0, 0, 1), (1, 0, 0)),
             ((0, -1, 0), (0, 0, -1), (1, 0, 0)), ((0, 0, 1), (0, 1, 0), (-1, 0, 0)), ((0, 0, -1), (0, 1, 0), (1, 0, 0))]
    half = (w / 2, h / 2, d / 2)
    for n, u, v in faces:
        base = len(pos)
        for su, sv in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
            p = [n[i] * half[i] + su * u[i] * half[i] + sv * v[i] * half[i] for i in range(3)]
            p[1] += y0
            pos.append(p)
            nor.append(list(n))
        # wind counter-clockwise seen from outside
        a, b, c, d_ = base, base + 1, base + 2, base + 3
        cross = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
        if sum(cross[i] * n[i] for i in range(3)) > 0:
            idx += [a, b, c, a, c, d_]
        else:
            idx += [a, c, b, a, d_, c]
    return pos, nor, idx


class Buf:
    def __init__(self):
        self.data = bytearray()
        self.views, self.accessors = [], []

    def add(self, fmt, values, comp, typ, target=None, minmax=False, count=None):
        while len(self.data) % 4:
            self.data.append(0)
        off = len(self.data)
        flat = [x for v in values for x in (v if isinstance(v, (list, tuple)) else [v])]
        self.data += struct.pack("<%d%s" % (len(flat), fmt), *flat)
        view = {"buffer": 0, "byteOffset": off, "byteLength": len(self.data) - off}
        if target:
            view["target"] = target
        self.views.append(view)
        acc = {"bufferView": len(self.views) - 1, "componentType": comp, "count": count or len(values), "type": typ}
        if minmax:
            dims = len(values[0]) if isinstance(values[0], (list, tuple)) else 1
            cols = [[(v if dims > 1 else [v])[i] for v in values] for i in range(dims)]
            acc["min"] = [min(c) for c in cols]
            acc["max"] = [max(c) for c in cols]
        self.accessors.append(acc)
        return len(self.accessors) - 1


def main(out):
    b = Buf()
    meshes, materials = [], []
    for name, (w, h, d, y0), color in (("Box", (0.8, 0.8, 0.8, 0.9), (0.85, 0.12, 0.1, 1)), ("Post", (0.2, 0.5, 0.2, 0.25), (0.1, 0.3, 0.9, 1))):
        pos, nor, idx = box(w, h, d, 0.0)
        pa = b.add("f", pos, 5126, "VEC3", 34962, minmax=True)
        na = b.add("f", nor, 5126, "VEC3", 34962)
        ia = b.add("H", idx, 5123, "SCALAR", 34963)
        materials.append({"name": name, "pbrMetallicRoughness": {"baseColorFactor": list(color), "metallicFactor": 0.0, "roughnessFactor": 0.4}})
        meshes.append({"name": name, "primitives": [{"attributes": {"POSITION": pa, "NORMAL": na}, "indices": ia, "material": len(materials) - 1}]})
    # nodes: 0 root (animated by Spin), 1 box (animated by Bob), 2 post
    nodes = [{"name": "Root", "children": [1, 2]}, {"name": "Box", "mesh": 0, "translation": [0, 0.9, 0]}, {"name": "Post", "mesh": 1, "translation": [0, 0.25, 0]}]
    anims = []
    # Spin: 2 s, a quaternion about y every 0.25 s
    ts = [i * 0.25 for i in range(9)]
    qs = [[0, math.sin(t * math.pi / 2), 0, math.cos(t * math.pi / 2)] for t in ts]  # angle = t * pi: a turn in 2 s
    ti = b.add("f", ts, 5126, "SCALAR", minmax=True)
    qi = b.add("f", qs, 5126, "VEC4")
    anims.append({"name": "Spin", "samplers": [{"input": ti, "output": qi, "interpolation": "LINEAR"}], "channels": [{"sampler": 0, "target": {"node": 0, "path": "rotation"}}]})
    # Bob: 1 s, the box up 0.5 and back
    ts2 = [i * 0.1 for i in range(11)]
    ys = [[0, 0.9 + 0.5 * math.sin(t * math.pi), 0] for t in ts2]
    ti2 = b.add("f", ts2, 5126, "SCALAR", minmax=True)
    yi = b.add("f", ys, 5126, "VEC3")
    anims.append({"name": "Bob", "samplers": [{"input": ti2, "output": yi, "interpolation": "LINEAR"}], "channels": [{"sampler": 0, "target": {"node": 1, "path": "translation"}}]})
    while len(b.data) % 4:
        b.data.append(0)
    gltf = {"asset": {"version": "2.0", "generator": "raea make_glb.py"}, "scene": 0, "scenes": [{"nodes": [0]}], "nodes": nodes, "meshes": meshes,
            "materials": materials, "animations": anims, "buffers": [{"byteLength": len(b.data)}], "bufferViews": b.views, "accessors": b.accessors}
    js = json.dumps(gltf, separators=(",", ":")).encode()
    while len(js) % 4:
        js += b" "
    body = struct.pack("<III", 0x46546C67, 2, 12 + 8 + len(js) + 8 + len(b.data))
    body += struct.pack("<II", len(js), 0x4E4F534A) + js + struct.pack("<II", len(b.data), 0x004E4942) + bytes(b.data)
    Path(out).write_bytes(body)
    print(f"{out}: {len(body)} bytes")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else str(Path(__file__).parent / "media" / "spinner.glb"))
