# Per-character advances (px at size 100) for every face a part can be set in, so components size text without a
# browser: the fonts bundled with the engine (engine/fonts) and the installed system faces the SOPs offer.
# metrics.json is {family: {weight: {char: advance}}}; kit.ts's measure() picks the nearest weight.
#   uv run --with fonttools --with brotli python void/parts/metrics.py
import json, os
from fontTools.ttLib import TTFont, TTCollection

here = os.path.dirname(os.path.abspath(__file__))
fonts = os.path.join(here, "..", "..", "engine", "fonts")
chars = [chr(c) for c in range(32, 383)] + ["¾", "’", "‘", "“", "”", "—", "–", "…", "€", "™"]
# display names the engine knows (engine/src/fonts.rs display_name)
NAMES = {"ibm-plex-mono": "IBM Plex Mono", "eb-garamond": "EB Garamond", "noto-sans-jp": "Noto Sans JP", "jetbrains-mono": "JetBrains Mono"}

def advances(f):
    upm, cmap, hmtx = f["head"].unitsPerEm, f.getBestCmap(), f["hmtx"]
    return {ch: round(hmtx[cmap[ord(ch)]][0] * 100 / upm, 2) for ch in chars if ord(ch) in cmap}

out = {}
for e in json.load(open(os.path.join(fonts, "index.json"))):
    if e["style"] != "normal": continue
    name = NAMES.get(e["family"]) or " ".join(w.capitalize() for w in e["family"].split("-"))
    out.setdefault(name, {})[e["weight"]] = advances(TTFont(os.path.join(fonts, e["file"])))
for name, path in [("Avenir Next Condensed", "/System/Library/Fonts/Avenir Next Condensed.ttc"),
                   ("Avenir Next", "/System/Library/Fonts/Avenir Next.ttc"),
                   ("Futura", "/System/Library/Fonts/Supplemental/Futura.ttc")]:
    if not os.path.exists(path): continue
    for f in TTCollection(path).fonts:
        full = f["name"].getDebugName(4)
        if "Italic" in full or not full.startswith(name) or "Condensed" in full[len(name):]: continue
        out.setdefault(name, {})[str(f["OS/2"].usWeightClass)] = advances(f)
json.dump(out, open(os.path.join(here, "metrics.json"), "w"), ensure_ascii=False, separators=(",", ":"))
print({k: sorted(v, key=int) for k, v in out.items()})
