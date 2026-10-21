"""Triangle count, file size and bounds of every assets/*/*.glb (reads the GLB JSON chunk only)."""
import json, struct, sys, os, glob
root = sys.argv[1]
rows = []
for f in sorted(glob.glob(os.path.join(root, "*", "*.glb"))):
    b = open(f, "rb").read()
    n = struct.unpack_from("<I", b, 12)[0]
    g = json.loads(b[20:20 + n])
    tris = 0; lo = [1e18] * 3; hi = [-1e18] * 3
    for m in g["meshes"]:
        for p in m["primitives"]:
            acc = g["accessors"][p["indices"]] if "indices" in p else g["accessors"][p["attributes"]["POSITION"]]
            tris += acc["count"] // 3
    for node in g["nodes"]:
        if "mesh" in node:
            for p in g["meshes"][node["mesh"]]["primitives"]:
                a = g["accessors"][p["attributes"]["POSITION"]]
                lo = [min(x, y) for x, y in zip(lo, a["min"])]; hi = [max(x, y) for x, y in zip(hi, a["max"])]
    imgs = len(g.get("images", []))
    rows.append(dict(name=os.path.basename(os.path.dirname(f)), tris=tris, mb=round(len(b) / 1e6, 1),
                     size_m=[round(h - l, 3) for l, h in zip(lo, hi)], min_y=round(lo[1], 4), images=imgs))
json.dump(rows, sys.stdout, indent=0)
