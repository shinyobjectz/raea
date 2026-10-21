"""Writes sdk/motion/library/sounds/catalog.json: the open sample libraries `raea-motion sounds` searches and fetches from,
each instrument with the files it has, so a search works offline and a fetch knows its URLs without asking GitHub.
Only libraries whose licence allows use in a commercial piece: CC0, CC-BY (credit the author) and public domain. Tone's
own drum kits (tonejs.github.io/audio/drum-samples) are CC BY-NC-SA, so they're left out.
    python3 tools/sounds/catalog.py [--out sdk/motion/library/sounds/catalog.json]
Run it again to pick up files the libraries add; it reads GitHub's tree API (four requests)."""
import argparse, json, os, re, urllib.request

ap = argparse.ArgumentParser()
ap.add_argument("--out", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../sdk/motion/library/sounds/catalog.json"))
a = ap.parse_args()


def get(url):
    return json.load(urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "raea-sounds"})))


def tree(repo, branch):
    t = get(f"https://api.github.com/repos/{repo}/git/trees/{branch}?recursive=1")
    assert not t.get("truncated"), repo
    return [(e["path"], e.get("size", 0)) for e in t["tree"] if e["type"] == "blob"]


def slug(s):
    return re.sub(r"[^a-z0-9]+", "-", s.lower()).strip("-")


packs = []

# FluidR3 GM rendered one MP3 per note by midi-js-soundfonts: all 128 General MIDI instruments, A0 to C8 chromatic,
# named with flats (Db4)
names = get("https://gleitz.github.io/midi-js-soundfonts/FluidR3_GM/names.json")
packs.append({
    "id": "fluid",
    "name": "FluidR3 General MIDI (midi-js-soundfonts)",
    "licence": "CC-BY 3.0",
    "attribution": "FluidR3_GM soundfont by Frank Wen, rendered by Benjamin Gleitzman (github.com/gleitz/midi-js-soundfonts), CC-BY 3.0",
    "source": "https://github.com/gleitz/midi-js-soundfonts",
    "url": "https://gleitz.github.io/midi-js-soundfonts/FluidR3_GM/{instrument}-mp3/{file}",
    "pitched": True,
    "notes": "A0-C8",
    "flats": True,
    "ext": "mp3",
    "instruments": [{"id": n, "name": n.replace("_", " ")} for n in names],
})

# the Salamander Grand Piano (Yamaha C5) as Tone.js's examples load it: every minor third from A0
sal = sorted(p.split("/")[1][:-4] for p, _ in tree("Tonejs/audio", "master") if p.startswith("salamander/") and p.endswith(".mp3"))
packs.append({
    "id": "salamander",
    "name": "Salamander Grand Piano V2 (Tone.js's audio)",
    "licence": "CC-BY 3.0",
    "attribution": "Salamander Grand Piano by Alexander Holm, CC-BY 3.0 (via tonejs.github.io/audio/salamander)",
    "source": "https://github.com/Tonejs/audio/tree/master/salamander",
    "url": "https://tonejs.github.io/audio/salamander/{file}",
    "pitched": True,
    "ext": "mp3",
    "instruments": [{"id": "piano", "name": "grand piano (Yamaha C5)", "files": [f"{n}.mp3" for n in sal]}],
})

# nbrosowsky's tonejs-instruments: 20 orchestral and band instruments, sharps written `s` (Cs4)
tji = {}
for p, _ in tree("nbrosowsky/tonejs-instruments", "master"):
    m = re.match(r"samples/([^/]+)/([A-G]s?\d)\.mp3$", p)
    if m:
        tji.setdefault(m.group(1), []).append(m.group(2) + ".mp3")
packs.append({
    "id": "tonejs-instruments",
    "name": "tonejs-instruments (nbrosowsky)",
    "licence": "public domain and CC-BY sources (the repo's README; the cello is CC-BY 3.0 by flcellogrl on Freesound)",
    "attribution": "Samples from github.com/nbrosowsky/tonejs-instruments (Nicholaus Brosowsky); cello by flcellogrl (freesound.org), CC-BY 3.0",
    "source": "https://github.com/nbrosowsky/tonejs-instruments",
    "url": "https://nbrosowsky.github.io/tonejs-instruments/samples/{instrument}/{file}",
    "pitched": True,
    "ext": "mp3",
    "instruments": [{"id": k, "name": k.replace("-", " "), "files": sorted(v)} for k, v in sorted(tji.items())],
})

# the Versilian Community Sample Library: CC0 orchestral, world and percussion instruments, WAV (some are large: a
# fetch takes one velocity and round robin per note, or a drum's few hits)
vcsl = {}
for p, size in tree("sgossner/VCSL", "master"):
    if not p.lower().endswith(".wav"):
        continue
    parts = p.split("/")
    if len(parts) < 4:
        continue
    inst = "/".join(parts[:3])
    vcsl.setdefault(inst, []).append(["/".join(parts[3:]), size])
packs.append({
    "id": "vcsl",
    "name": "Versilian Community Sample Library",
    "licence": "CC0",
    "attribution": "Versilian Community Sample Library (Versilian Studios), CC0: no credit needed",
    "source": "https://github.com/sgossner/VCSL",
    "url": "https://raw.githubusercontent.com/sgossner/VCSL/master/{instrument_path}/{file}",
    "ext": "wav",
    "instruments": [
        {"id": slug(k.split("/")[-1]), "name": k.split("/")[-1], "family": " / ".join(k.split("/")[:2]), "path": k, "files": sorted(v)}
        for k, v in sorted(vcsl.items())
    ],
})

os.makedirs(os.path.dirname(a.out), exist_ok=True)
with open(a.out, "w") as f:
    json.dump({"note": "written by tools/sounds/catalog.py; raea-motion sounds reads it", "packs": packs}, f, separators=(",", ":"))
print(a.out, os.path.getsize(a.out), "bytes;", ", ".join(f"{p['id']} {len(p['instruments'])}" for p in packs))
