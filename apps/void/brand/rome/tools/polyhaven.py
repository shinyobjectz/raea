"""The film's skies, surfaces and props from Poly Haven (all CC0): HDRIs to media/sky/<id>.hdr, texture sets to
media/tex/<id>/{diff,nor_gl,rough,ao}.jpg, models to media/props/<id>/ (glTF and its textures). Credits go to
media/ph/SOURCES.txt. Skips what's already there.

  python3 tools/polyhaven.py
"""
import json, urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent
M = HERE / "media"
UA = {"User-Agent": "raea-rome/1.0"}

SKIES = ["qwantani_dawn_puresky", "qwantani_morning_puresky", "qwantani_mid_morning_puresky", "kloofendal_overcast_puresky",
         "wasteland_clouds_puresky", "qwantani_dusk_2_puresky", "qwantani_sunset_puresky", "qwantani_night_puresky"]
TEXTURES = ["large_sandstone_blocks_01", "monastery_stone_floor", "marble_01", "marble_tiles", "white_plaster_rough_01",
            "dry_ground_rocks", "rocks_ground_05", "bark_brown_02", "wood_planks_dirt", "worn_plaster_wall", "stone_tiles_02"]
PROPS = ["brass_candleholders", "wooden_candlestick", "antique_ceramic_vase_01", "ceramic_vase_02", "stone_fire_pit",
         "ornate_medieval_dagger", "rock_moss_set_01", "dead_tree_trunk"]


def get(url):
    return urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=120).read()


def save(url, dest: Path):
    if dest.exists():
        return
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(get(url))
    print(f"  {dest.relative_to(HERE)} ({dest.stat().st_size // 1024} KB)")


credits = []
for kind, ids in (("hdri", SKIES), ("texture", TEXTURES), ("model", PROPS)):
    for id in ids:
        info = json.loads(get(f"https://api.polyhaven.com/info/{id}"))
        files = json.loads(get(f"https://api.polyhaven.com/files/{id}"))
        authors = ", ".join(info.get("authors", {}))
        credits.append(f"{info['name']} ({id}) by {authors}, Poly Haven, CC0: https://polyhaven.com/a/{id}")
        print(id)
        if kind == "hdri":
            save(files["hdri"]["2k"]["hdr"]["url"], M / "sky" / f"{id}.hdr")
        elif kind == "texture":
            for k, name in (("Diffuse", "diff"), ("nor_gl", "nor_gl"), ("Rough", "rough"), ("AO", "ao")):
                if k in files:
                    save(files[k]["2k"]["jpg"]["url"], M / "tex" / id / f"{name}.jpg")
        else:
            g = files["gltf"]["1k"]["gltf"]
            save(g["url"], M / "props" / id / f"{id}.gltf")
            for rel, f in g.get("include", {}).items():
                save(f["url"], M / "props" / id / rel)
            if id == "ornate_medieval_dagger":  # the dagger drawn: the same file without its scabbard (node 1)
                gl = json.loads((M / "props" / id / f"{id}.gltf").read_text())
                gl["scenes"][0]["nodes"] = [0]
                (M / "props" / id / f"{id}_bare.gltf").write_text(json.dumps(gl))
(M / "ph").mkdir(parents=True, exist_ok=True)
(M / "ph" / "SOURCES.txt").write_text("\n".join(credits) + "\n")
