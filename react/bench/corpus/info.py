#!/usr/bin/env python3
"""What run.sh needs to know about an edit before it runs it, printed as shell assignments:
KIND (game when the edit's folder uses useInput, 3d when it uses Canvas3D, else timeline) and CONF (the size and length
flags from `export const config = {...}`, the form score's SDK backend writes, passed as score passes them;
empty when the edit registers a <Composition> or leaves the CLI's defaults), plus --words and --perception for the
files beside it."""
import json
import os
import re
import sys

edit = sys.argv[1]
folder = os.path.dirname(edit)
src = open(edit).read()
texts = [src]
for root, dirs, files in os.walk(folder):
    dirs[:] = [d for d in dirs if d not in ("node_modules", "media", ".score", "assets", "brand")]
    for f in files:
        if f.endswith((".ts", ".tsx")) and not f.endswith(".gen.ts"):
            try:
                texts.append(open(os.path.join(root, f)).read())
            except OSError:
                pass
every = "\n".join(texts)
kind = "game" if re.search(r"\buseInput\b", every) else "3d" if re.search(r"\bCanvas3D\b", every) else "timeline"
conf = ""
m = re.search(r"export const config = (\{.*?\});", src)
if m and "registerRoot" not in src:
    try:
        c = json.loads(m.group(1))
        conf = f"--w {c['width']} --h {c['height']} --fps {c['fps']} --frames {c['durationInFrames']}"
    except (ValueError, KeyError):
        pass  # a config built from variables ({ width, height, fps, durationInFrames: total }): the engine reads it
# the words and perception beside the edit, as score passes them to a render (a baked play reads them by itself)
for name, flag in (("words.json", "--words"), ("perception.json", "--perception")):
    if os.path.exists(os.path.join(folder, name)):
        conf = f"{conf} {flag} {os.path.join(folder, name)}".strip()
print(f"KIND={kind}")
print(f"CONF='{conf}'")
