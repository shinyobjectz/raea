#!/usr/bin/env python3
"""Real-footage scenarios from the founder's drives (media/library/pairs.json): each pair's raw takes become a fixture
and Kelly's finished edit of them the reference a pair judge compares against. Writes evals/scenarios-drive/ and
evals/fixtures-drive/, both gitignored: they name clients and point at private drive paths. Usage:
    python3 -I evals/tools/drive_pairs.py [max_scenarios]
"""
import json, os, re, subprocess, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from voice_overlap import overlap

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LIB = os.path.expanduser("~/raea/media/library/pairs.json")
SCEN, FIX = os.path.join(ROOT, "scenarios-drive"), os.path.join(ROOT, "fixtures-drive")
MAX = int(sys.argv[1]) if len(sys.argv) > 1 else 50
# kept out of evals (the catalogue's README): minors, IDs, contracts, personal shoots, long-form
AVOID = re.compile(r"teen|kid|child|passport|licen|contract|invoice|lingerie|swim|model(ing)?\b|podcast|course|adoption|wedding|family", re.I)

def probe(p):
    try:
        out = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
                              "stream=width,height:stream_side_data=rotation:format=duration", "-of", "json", p],
                             capture_output=True, text=True, timeout=30).stdout
        d = json.loads(out)
        s = (d.get("streams") or [{}])[0]
        w, h = s.get("width", 0), s.get("height", 0)
        rot = next((abs(int(x.get("rotation", 0))) for x in s.get("side_data_list", []) if "rotation" in x), 0)
        if rot in (90, 270): w, h = h, w
        return float(d.get("format", {}).get("duration", 0) or 0), w, h
    except Exception:
        return 0.0, 0, 0

FORMATS = [(0.62, "9:16", 1080, 1920), (0.9, "4:5", 1080, 1350), (1.1, "1:1", 1080, 1080), (99, "16:9", 1920, 1080)]

def fmt(w, h):
    r = w / h if h else 0.5625
    return next((name, W, H) for lim, name, W, H in FORMATS if r < lim)

# The brief states goals only (rewritten 2026-10-16): the client by name, the platform and size from the reference's
# shape, the length, and what the viewer should do. No structure, no parts, no "captions", and no concept guessed from
# file names: a brief that names the solution grades the agent on following it, not on making something.
PLACES = {"9:16": "TikTok and Reels", "4:5": "the Instagram and Facebook feed", "1:1": "a square feed placement", "16:9": "YouTube"}

def brand(client):
    """The brand a client folder names ("BM Digital / Wynk" is Wynk, "310 Nutrition (Kerry UGC)" is 310 Nutrition), or
    None when the folder names no brand (Kelly's own portfolio and branding shoots, unsorted)."""
    b = re.sub(r"\(.*?\)", "", client).split("/")[-1].strip()
    return None if re.search(r"kelly|unsorted", b, re.I) or not b else b

def pair_brief(client, n_raws, secs, aspect, W, H):
    b = brand(client)
    clips = f"{n_raws} raw clip{'s' if n_raws != 1 else ''}"
    goal = f"that makes people who see it want to try {b}" if b else "that people watch to the end"
    return (f"media/ holds {clips}{f' we shot for {b}' if b else ''}. We need a {secs} second video for {PLACES[aspect]}, "
            f"{aspect} ({W}x{H}), {goal}. Use only this footage.")

def organic_brief(minutes, where, secs, aspect, W, H):
    return (f"media/ holds a {minutes} minute long-form recording. Make {where} from it, about {secs} seconds, {aspect} ({W}x{H}), "
            "that stands on its own and makes people want to follow for more.")

# The fixture holds only the raws (2026-10-16): setting the format is part of the job, not given; the eval's space gets
# `raea init`'s project (no edit.rrl since rrl's deletion, 2026-10-17).

def slug(s):
    return re.sub(r"[^a-z0-9]+", "-", s.lower()).strip("-")[:60]

pairs = json.load(open(LIB))["pairs"]
pairs = [p for p in pairs if p.get("confidence") in ("high", "medium") and not AVOID.search(p["project"] + p.get("client", ""))]
pairs.sort(key=lambda p: (p.get("confidence") != "high", p.get("raw_gb") or 99))
per_client = {}
os.makedirs(SCEN, exist_ok=True); os.makedirs(FIX, exist_ok=True)
by_client, n = {}, 0
for p in pairs:
    if n >= MAX: break
    if not (1 <= len(p["raws"]) <= 20) or (p.get("raw_gb") or 0) > 3: continue
    if not os.path.isdir(p["project"]): continue  # drive unplugged
    # the catalogue's raw/final labels are by name and folder: a "final" named raw, or a "raw" named export, is skipped
    raws = [r for r in p["raws"] if not re.search(r"export|final|\bv\d|_v\d|edit", os.path.basename(r), re.I)]
    if not (1 <= len(raws) <= 20): continue
    if per_client.get(p["client"], 0) >= 5: continue  # spread across clients
    finals = []
    for f in [f for f in p["finals"] if not re.search(r"raw|unedit|footage", os.path.basename(f), re.I)][:8]:
        d, w, h = probe(f)
        if 8 <= d <= 90 and w and h: finals.append((d, f, w, h))
    if not finals: continue
    # prefer a format the suite has few of, so aspect ratios spread
    finals.sort(key=lambda x: (fmt(x[2], x[3])[0] == "9:16", x[0]))
    # the reference must be her edit of these raws: it says what they say (an animated cut, a stock-footage version or
    # another script beat nothing in a pair; 2026-10-16 four "wins" were against such references)
    finals = [x for x in finals if (overlap(x[1], raws) or 0) >= 0.5]
    if not finals: continue
    dur, ref, rw, rh = finals[0]
    aspect, W, H = fmt(rw, rh)
    per_client[p["client"]] = per_client.get(p["client"], 0) + 1
    name = slug(f"{p['client']} {os.path.basename(p['project'])}")
    fx = os.path.join(FIX, name, "media")
    os.makedirs(fx, exist_ok=True)
    for i, r in enumerate(raws):
        ext = os.path.splitext(r)[1].lower() or ".mov"
        link = os.path.join(fx, f"take{i + 1:02d}{ext}")
        if not os.path.lexists(link): os.symlink(r, link)
    secs = int(round(dur / 5) * 5) or int(dur)
    brief = pair_brief(p["client"], len(raws), secs, aspect, W, H)
    by_client.setdefault(p["client"], []).append((name, brief, ref, secs, W, H))
    n += 1
for client, items in by_client.items():
    with open(os.path.join(SCEN, slug(client) + ".feature"), "w") as f:
        f.write(f"@medium:ugc\nFeature: {client}: Kelly's raw takes, cut by the agent, compared with her real edit\n")
        f.write("  Local only: built by evals/tools/drive_pairs.py from the founder's drives; never committed.\n")
        for name, brief, ref, secs, W, H in items:
            lo, hi = int(secs * 30 * 0.75), int(secs * 30 * 1.3)
            f.write(f"\n  @level:hard @design\n  Scenario: {name}\n    Given the space \"../fixtures-drive/{name}\"\n")
            f.write(f"    When the person says \"{brief}\"\n")
            f.write(f"    Then expect edit.width = {W} and edit.height = {H}\n    And expect edit.frames >= {lo} and edit.frames <= {hi}\n")
            f.write(f"    And it's compared with \"{ref}\"\n    And it costs under $2.00\n")
# organic clips from long-form footage (podcast and course episodes): no reference, judged on the anchored scale
A = json.load(open(os.path.expanduser("~/raea/media/library/library.json")))["assets"]
longs = [a for a in A if re.search(r"podcast|course", a["path"], re.I) and not AVOID.search(a["path"].replace("course", "").replace("Course", "").replace("podcast", "").replace("Podcast", ""))
         and a["path"].lower().endswith((".mp4", ".mov"))]
picked, seen_dirs = [], set()
for a in sorted(longs, key=lambda a: a["path"]):
    if len(picked) >= 12: break
    d = os.path.dirname(a["path"])
    if d in seen_dirs or not os.path.exists(a["path"]): continue
    dur, w, h = probe(a["path"])
    if not (300 <= dur <= 3600) or not w: continue
    seen_dirs.add(d); picked.append((a, dur, w, h))
asks = [("9:16", 1080, 1920, 35, "a vertical clip for TikTok and Reels"),
        ("1:1", 1080, 1080, 30, "a square clip for the feed"),
        ("16:9", 1920, 1080, 50, "a landscape highlight for YouTube"),
        ("4:5", 1080, 1350, 30, "a 4:5 clip for the Instagram feed")]
organic = []
for i, (a, dur, w, h) in enumerate(picked):
    aspect, W, H, secs, words = asks[i % len(asks)]
    name = slug(f"organic {os.path.basename(os.path.dirname(a['path']))} {i}")
    fx = os.path.join(FIX, name, "media"); os.makedirs(fx, exist_ok=True)
    link = os.path.join(fx, "source" + os.path.splitext(a["path"])[1].lower())
    if not os.path.lexists(link): os.symlink(a["path"], link)
    brief = organic_brief(round(dur / 60), words, secs, aspect, W, H)
    organic.append((name, brief, secs, W, H))
if organic:
    with open(os.path.join(SCEN, "organic-clips.feature"), "w") as f:
        f.write("@medium:ugc\nFeature: Organic clips cut from long-form recordings (podcast and course episodes)\n")
        f.write("  Local only: built by evals/tools/drive_pairs.py from the founder's drives; never committed.\n")
        for name, brief, secs, W, H in organic:
            lo, hi = int(secs * 30 * 0.7), int(secs * 30 * 1.4)
            f.write(f"\n  @level:hard @design @concept:organic\n  Scenario: {name}\n    Given the space \"../fixtures-drive/{name}\"\n")
            f.write(f"    When the person says \"{brief}\"\n")
            f.write(f"    Then expect edit.width = {W} and edit.height = {H}\n    And expect edit.frames >= {lo} and edit.frames <= {hi}\n")
            f.write("    And the judge sees \"a clip that stands on its own as one complete idea, holds attention, and is framed for the format\"\n    And it costs under $2.50\n")
print(f"{n} pair scenarios for {len(by_client)} clients and {len(organic)} organic clips in {SCEN}")
