"""The front end: a .brief's intent graph read into a Script (from_graph; brief/format.md, raea-brief parse) and placed
on the space's footage (resolve), into the intermediate form (ir.py). Score's Fountain reader left the compiler on
2026-10-17 (P5, brief/format.feature): it lives on in score/bench/fountain.py for re-running the recorded comparisons. Nothing here knows a target: no rrl, no SDK. Perception (framing a take on its face, its length) comes from
raea-perceive and ffprobe; an agent session's media verbs replace ffprobe and ffmpeg here when they land in the shared tree.

The notation is void/prompts/sops/score.md (a Fountain profile with [[part | utilities]] notes and a stylesheet after
`---`). Nothing creative is decided here (the founder, 2026-10-16: everything brand-specific, nothing hardcoded or handed
to the agent by default): which parts, where, how they move, whether there are captions, what sound, the background
and the punch-in are the script's or the brand's (SPACE/brand.json). What is kept is fact and craft that isn't taste:
where words are said and a clean edit point between them, where the face is, the platforms' safe zones (previz's
PLATFORM_BANDS), void's own framing and region margin, and what reads as a caption already in the footage.
"""
import hashlib, json, os, re, subprocess
from pathlib import Path

from .ir import Clip, Line, Note, Part, Script, Sound, Timeline

FPS = 30
# a utility the notation implements (motion as keys on the part, placement, timing, punch-in, foley); anything else in a
# note's utilities is unreadable and the sheet says so
UTIL = re.compile(r"^(on:'(?:[^']|'\w)+'|until:'(?:[^']|'\w)+'|to-end|place-(top|low|center|full|auto)|punch-in:\d+(\.\d+)?|"
                  r"enter-(fade|rise|slide|pop|bounce)|exit-fade|settle|dur-(sm|md|lg)|ease-(standard|emphasized|decelerate|accelerate)|"
                  r"stagger-after:\w+|sfx:'(?:[^']|'\w)+'|sfx-gain:-?\d+(\.\d+)?)$")
# the platforms' button bands at 1080 x 1920 (top, right, bottom, left px): rdb/src/previz.rs PLATFORM_BANDS
PLATFORM_BANDS = {"house": (220, 140, 420, 0), "tiktok": (108, 120, 320, 60), "reels": (210, 84, 310, 0), "shorts": (120, 96, 300, 0)}
PERCEIVE = Path.home() / "raea/perception/target/release/raea-perceive"


# ---- reading the script ----
def strip_fence(t):
    m = re.search(r"```[\w-]*\n(.*?)```", t, re.S)
    return m.group(1) if m else t


def toks(s):
    # a ' followed by a letter is inside the words (on:'don't know'), as the .brief parser reads it
    return re.findall(r"[\w-]+:'(?:[^']|'\w)*'|\S+", s)


def parse_note(n):
    """`name k:"v" ... | utilities` (a part: any the space has), `component name:"x" ...` (the same, spelled out), or
    utilities alone. Whether the space has the part is resolve's to say; the note only reads."""
    note = Note()
    first = toks(n)[0] if toks(n) else ""
    m = re.match(r"\s*([A-Za-z_][\w-]*)((?:\s+\w+:(?:\"[^\"]*\"|\[[^\]]*\]|-?\d+(?:\.\d+)?))*)\s*(\|(.*))?$", n)
    if m and not UTIL.match(first) and first not in ("|",):
        note.kind = m.group(1)
        note.props = {k: v.strip('"') for k, v in re.findall(r"(\w+):(\"[^\"]*\"|\[[^\]]*\]|-?\d+(?:\.\d+)?)", m.group(2))}
        if note.kind == "component":
            if "name" not in note.props:
                note.unknown.append("component without name:\"...\"")
            else:
                note.kind = note.props.pop("name")
        note.utils = toks(m.group(4) or "")
    else:
        note.utils = [x for x in toks(n) if x != "|"]
    note.unknown += [f"util {u}" for u in note.utils if not UTIL.match(u)]
    note.anchor = next((re.match(r"on:'(.*)'", u).group(1) for u in note.utils if u.startswith("on:'")), None)
    return note


def util(utils, name):
    """the value of `name:...` among a note's utilities ('...' unquoted), or None"""
    for u in utils:
        if u.startswith(name + ":"):
            return u[len(name) + 1:].strip("'")
    return None


def parse_css(css):
    style = {}
    for sel, body in re.findall(r"([^{}]+)\{([^}]*)\}", css):
        style[sel.strip()] = {k.strip(): v.strip() for k, v in (d.split(":", 1) for d in body.split(";") if ":" in d)}
    return style


def format_secs(fmt):
    """`9:16, 30 s` -> (25.5, 34.5); `25 to 35 s` -> (25, 35): a length allows 15% either way"""
    m = re.search(r"(\d+)(?:\s*(?:to|-)\s*(\d+))?\s*s(?:ec|econds?)?\b", fmt or "", re.I)
    if not m:
        return None
    return (float(m.group(1)), float(m.group(2))) if m.group(2) else (float(m.group(1)) * 0.85, float(m.group(1)) * 1.15)


def shape_of(fmt):
    """the edit's size from a Format's ratio, the short side 1080 (9:16 is 1080 x 1920, 16:9 1920 x 1080); None without
    one (then 9:16)"""
    m = re.search(r"(\d+)\s*:\s*(\d+)", fmt or "")
    if not m:
        return None
    rw, rh = int(m.group(1)), int(m.group(2))
    return (round(1080 * rw / rh / 2) * 2, 1080) if rw >= rh else (1080, round(1080 * rh / rw / 2) * 2)




def from_graph(g, cut=None, variant=None):
    """a .brief's intent graph (raea-brief parse, brief/format.md) as the Script parse makes from Fountain: one
    deliverable (`cut`, a ratio; the first when none is named), the lines and open beats tagged for it or for none"""
    ds = g.get("deliverables") or [{}]
    d = next((x for x in ds if x.get("ratio") == cut), None) if cut else ds[0]
    if d is None:
        raise ValueError(f"no deliverable {cut} in Format: {g.get('format')}")
    names = g.get("variant_names") or []
    if variant and variant not in names:
        raise ValueError(f"no variant {variant} in the brief ({', '.join(names) or 'none'})")
    variant = variant or (names[0] if names else None)
    # in this deliverable (its ratio) and this variant (its name, or its section's)
    keep = lambda it: (not it.get("variants") or d.get("ratio") in it["variants"]) and (not it.get("variant") or variant in it["variant"])
    fmt = g.get("format") if len(ds) == 1 else d.get("format", "")
    sc = Script(title=g.get("title") or "", format=fmt or "", secs=format_secs(fmt), style=g.get("style") or {}, text=g.get("source", ""))
    if d.get("ratio"):
        sc.shape = shape_of(d["ratio"])
    sc.sections = [s["name"] for s in g.get("sections", []) if not s.get("variant") or variant in s["variant"]]
    sc.claims = g.get("claims") or {}
    for it in g.get("items", []):
        if it["type"] == "line" and keep(it):
            notes = [Note(kind=n["kind"], props=dict(n["props"]), utils=list(n["utils"]), anchor=n.get("anchor")) for n in it["notes"]]
            sc.lines.append(Line(role=it.get("section"), take=it.get("take"), text=it["text"], delivery=it.get("delivery"),
                                 punch=False if it.get("punch") is None else it["punch"], notes=notes, alts=it.get("alts"), claims=it.get("claims") or []))
        elif it["type"] == "screen":
            sc.screen_text.append(it["text"])
        elif it["type"] == "beat" and keep(it):
            sc.beats.append(dict(it, after=len(sc.lines)))  # where it stands: before the line of this index
    sc.checks = g.get("checks", [])
    return sc


# ---- the space: transcripts, media, framing ----
def takes(media):
    """take name -> [(start_s, end_s, word)] from each words.json"""
    out = {}
    for w in sorted(media.glob("*.words.json")) + sorted(media.glob("takes/*/words.json")):
        name = w.parent.name if w.name == "words.json" else w.name[: -len(".words.json")]
        d = json.loads(w.read_text()); d = d if isinstance(d, list) else d.get("words", [])
        ws = [(x["start_ms"] / 1000, x["end_ms"] / 1000, x["text"]) for x in d if x.get("text", "").strip()]
        if ws:
            out[name] = ws
    return out


def norm(s):
    return re.sub(r"[^a-z0-9$% ]+", " ", s.lower().replace("’", "'").replace("'", "")).split()


def find_span(tk, text, take=None):
    """(take, start_s, end_s) where these words are said in a row, or None"""
    q = norm(text)
    if not q:
        return None
    for name, ws in tk.items():
        if take and name.lower() != take.lower():
            continue
        flat = [norm(w[2]) for w in ws]
        tks = [(i, t) for i, ts in enumerate(flat) for t in ts]
        words = [t for _, t in tks]
        for i in range(len(words) - len(q) + 1):
            if words[i:i + len(q)] == q:
                return name, ws[tks[i][0]][0], ws[tks[i + len(q) - 1][0]][1]
    return None


def media_files(media):
    """take name -> path relative to the space (media/take01.mov, media/takes/<take>/video.mov): takes with picture"""
    out = {}
    for p in media.iterdir():
        if p.suffix.lower() in (".mp4", ".mov", ".m4v") and not p.name.endswith(".540.mp4"):
            out[p.stem] = f"media/{p.name}"
    if (media / "takes").exists():
        for d in (media / "takes").iterdir():
            v = next((x for x in d.iterdir() if x.suffix.lower() in (".mov", ".mp4")), None) if d.is_dir() else None
            if v:
                out[d.name] = f"media/takes/{d.name}/{v.name}"
    return out


_LEN = {}


def media_len(p):
    if p not in _LEN:
        _LEN[p] = float(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(p)], capture_output=True, text=True).stdout.strip() or 1e9)
    return _LEN[p]


def parts_in(space):
    """the parts a script can name in this space: its own components (components/<name>.tsx, through void's component
    tool) and the space's kit (parts/parts.tsx's registry, themed from the brand); the global kit only when the space
    has none, and the answer says so"""
    have = {p.stem: "component" for p in (space / "components").glob("*.tsx") if not p.stem.endswith("_tw")} if (space / "components").exists() else {}
    kit = space / "parts" / "parts.tsx"
    source = "space"
    if not kit.exists():
        kit, source = Path.home() / "raea/void/parts/parts.tsx", "global"
    m = re.search(r"export const registry\s*=\s*\{([^}]*)\}", kit.read_text()) if kit.exists() else None
    for name in re.findall(r"(\w+)\s*:", m.group(1)) if m else []:
        have.setdefault(name, "kit")
    return have, source


def brand_of(space):
    """the brand's tokens: the host app's brand kit in the space (brand/brand.css's :root custom properties, shared with UGC
    Pro: --brand-background, --brand-accent, --brand-text, --brand-font...), else brand.json's palette"""
    css = space / "brand" / "brand.css"
    roles = space / "brand" / "brand.json"  # derived roles (palette.primary/accent/surface/text_on_*, scale, logos)
    out = {}
    if roles.exists():
        try:
            out = json.loads(roles.read_text())
        except json.JSONDecodeError:
            out = {}
    if css.exists():
        m = re.search(r":root\s*\{([^}]*)\}", css.read_text())
        out["vars"] = dict(re.findall(r"(--[\w-]+)\s*:\s*([^;]+);", m.group(1))) if m else {}
        return out
    if out:
        return out
    p = space / "brand.json"
    try:
        return json.loads(p.read_text()) if p.exists() else {}
    except json.JSONDecodeError:
        return {}


def brand_color(brand, role):
    """a palette role's colour from brand.json, whether the palette is {role: hex} or [{role, hex}]"""
    if "vars" in brand:
        v = brand["vars"].get(f"--brand-{role}")
        return v.strip() if v else None
    pal = brand.get("palette") or brand.get("colors") or {}
    if isinstance(pal, dict):
        v = pal.get(role)
        return v.get("hex") if isinstance(v, dict) else v
    return next((c.get("hex") or c.get("value") for c in pal if isinstance(c, dict) and c.get("role") == role), None)


def frame_box(f, sw, sh, W, H, eyes=0.38):
    """the pan at zoom 1 that centres a face (x, y, w, h, 0..1 of a sw x sh picture) with its eyes about `eyes` down the
    edit, and where the face then lands in the edit; `map` carries the crop (left, top, width, height in source px)"""
    cw0, ch0 = (sh * W / H, sh) if sw / sh > W / H else (sw, sw * H / W)
    pan = lambda c, s_, cw: max(-1.0, min(1.0, (c - s_ / 2) / ((s_ - cw) / 2))) if s_ - cw > 1 else 0.0
    cx = (f["x"] + f["w"] / 2) * sw; cy = (f["y"] + f["h"] * 0.4) * sh + ch0 * (0.5 - eyes)
    px, py = pan(cx, sw, cw0), pan(cy, sh, ch0)
    left = sw / 2 + px * (sw - cw0) / 2 - cw0 / 2; top = sh / 2 + py * (sh - ch0) / 2 - ch0 / 2
    return {"pan_x": round(px, 3), "pan_y": round(py, 3), "crop": (sh, top, ch0), "map": (sw, sh, left, top, cw0, ch0),
            "--face-top": (f["y"] * sh - top) / ch0 * H, "--face-bottom": ((f["y"] + f["h"]) * sh - top) / ch0 * H,
            "--face-left": (f["x"] * sw - left) / cw0 * W, "--face-right": ((f["x"] + f["w"]) * sw - left) / cw0 * W}


def in_edit(box, mp, W, H):
    """a box (x, y, w, h, 0..1 of the source) in edit px through a clip's crop: [x, y, w, h]"""
    sw, sh, left, top, cw0, ch0 = mp
    x0, y0 = (box[0] * sw - left) / cw0 * W, (box[1] * sh - top) / ch0 * H
    return [round(x0), round(y0), round(box[2] * sw / cw0 * W), round(box[3] * sh / ch0 * H)]


INGEST = Path(os.environ.get("RAEA_INGEST") or Path.home() / ".cache/raea-ingest")
_ING = {}


def ingested(space, rel):
    """the take's folder in the ingest store (raea-perceive ingest: its facts, faces tracked at 4 fps, performance and
    SigLIP columns; perception/src/ingest.rs keys it by the first 16 hex of the file's SHA-256), ingesting it once when
    it isn't there; None when it can't be (RAEA_SCORE_INGEST=0 keeps the stills, as before 2026-10-17)"""
    p = space / rel
    if p not in _ING:
        d = None
        if os.environ.get("RAEA_SCORE_INGEST", "1") != "0":
            h = hashlib.sha256()
            with open(p, "rb") as fh:
                for chunk in iter(lambda: fh.read(1 << 20), b""):
                    h.update(chunk)
            d = INGEST / h.hexdigest()[:16]
            if not ((d / "faces.json").exists() and (d / "facts.json").exists()):
                subprocess.run([str(PERCEIVE), "ingest", str(p)], capture_output=True, text=True)
            if (d / "faces.json").exists() and (d / "facts.json").exists():
                d = {"dir": d, "faces": json.loads((d / "faces.json").read_text()), "facts": json.loads((d / "facts.json").read_text())}
            else:
                d = None
        _ING[p] = d
    return _ING[p]


def speaker_track(faces, a, b, every=False):
    """the face a clip is about over [a, b] seconds of its take: of the confirmed tracks, the one visible there longest,
    the bigger face on a tie (the speaker of a talking head; nothing in perception yet ties a face to the voice).
    [(t, box)] of its visible samples, or []; with `every`, each of its samples there, box None where it isn't seen"""
    best, key = None, (0, 0)
    for tr in faces.get("tracks", []):
        vis = [(s_["t"], s_["box"]) for s_ in tr["samples"] if a - 1e-6 <= s_["t"] <= b + 1e-6 and s_.get("visible") and s_.get("box")]
        k = (len(vis), sorted(bx[2] * bx[3] for _, bx in vis)[len(vis) // 2] if vis else 0)
        if k > key:
            best, key = tr, k
    if best is None:
        return []
    inside = [s_ for s_ in best["samples"] if a - 1e-6 <= s_["t"] <= b + 1e-6]
    return [(s_["t"], s_["box"] if s_.get("visible") else None) for s_ in inside] if every else [(s_["t"], s_["box"]) for s_ in inside if s_.get("visible") and s_.get("box")]


def frame_clip(ing, a, b, src_in, src_out, W, H, eyes=0.38):
    """framing for one clip from the take's ingest (P1b, brief/format.feature: perception follows the clip, not the
    take): the speaker's median box over the words the clip keeps sets the pan; the face at every 4 fps sample of the
    clip in edit px (--track), the faces in shot at each (--faces), and caption-like text burned into the source there
    (--source-text, as source_text reads it), all through this clip's crop"""
    facts, faces = ing["facts"], ing["faces"]
    sw, sh = float(facts["width"]), float(facts["height"])
    vis = speaker_track(faces, a, b) or speaker_track(faces, src_in, src_out)
    if vis:
        med = lambda i: sorted(bx[i] for _, bx in vis)[len(vis) // 2]
        fr = frame_box({"x": med(0), "y": med(1), "w": med(2), "h": med(3)}, sw, sh, W, H, eyes)
    else:
        cw0, ch0 = (sh * W / H, sh) if sw / sh > W / H else (sw, sw * H / W)
        fr = {"crop": (sh, (sh - ch0) / 2, ch0), "map": (sw, sh, (sw - cw0) / 2, (sh - ch0) / 2, cw0, ch0)}
    track = [(t, in_edit(bx, fr["map"], W, H) if bx else None) for t, bx in speaker_track(faces, src_in, src_out, every=True)]
    fr["--track"] = track
    fr["--faces"] = [(f_["t"], sum(1 for tr in faces.get("tracks", []) for x in tr["samples"] if x["t"] == f_["t"] and x.get("visible")))
                     for f_ in faces.get("frames", []) if src_in - 1e-6 <= f_["t"] <= src_out + 1e-6]
    boxes = []
    for r in facts.get("text", []):
        if r["end"] < src_in or r["start"] > src_out or len(r.get("text", "").split()) < 3 or r["box"][2] < 0.35:
            continue
        _, y, _, hh = in_edit(r["box"], fr["map"], W, H)
        if y + hh > 0 and y < H:
            boxes.append([max(0, y), min(H, y + hh)])
    fr["--source-text"] = sorted(map(list, {tuple(x) for x in boxes}))
    return fr


def frame_take(space, rel, at, W, H, scratch, eyes=0.38):
    """void's framing (brain.rs `framing`): a frame of the take read by see-layout, the pan at zoom 1 that centres the
    face with its eyes about 38% down, and where the face then lands in the edit"""
    still = scratch / f".frame-{abs(hash((str(space), rel, round(at, 1))))}.jpg"
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", f"{at:.2f}", "-i", str(space / rel), "-frames:v", "1", "-q:v", "3", str(still)], check=False)
    if not still.exists():
        return {}
    dims = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "stream=width,height", "-of", "csv=p=0", str(still)], capture_output=True, text=True).stdout.strip()
    sw, sh = (float(x) for x in dims.split(","))
    out = subprocess.run([str(PERCEIVE), "see-layout", str(still)], capture_output=True, text=True).stdout
    still.unlink()
    cw0, ch0 = (sh * W / H, sh) if sw / sh > W / H else (sw, sw * H / W)
    centred = {"crop": (sh, (sh - ch0) / 2, ch0)}  # how the source maps into the edit when no face moves the crop
    try:
        zones = json.loads(out.splitlines()[0])["layout"]["zones"]
    except (IndexError, KeyError, json.JSONDecodeError):
        return centred
    subj = next((z for z in zones if z.get("role") == "subject" and isinstance(z.get("face"), dict)), None)
    if not subj:
        return centred
    fr = frame_box(subj["face"], sw, sh, W, H, eyes)
    del fr["map"]
    return fr


def faces_at(space, rel, at, scratch):
    """how many faces see-layout finds in the take at `at` seconds (None when the frame can't be read)"""
    still = scratch / f".who-{abs(hash((str(space), rel, round(at, 2))))}.jpg"
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", f"{max(0.0, at):.2f}", "-i", str(space / rel), "-frames:v", "1", "-q:v", "4", str(still)], check=False)
    if not still.exists():
        return None
    out = subprocess.run([str(PERCEIVE), "see-layout", str(still)], capture_output=True, text=True).stdout
    still.unlink()
    try:
        zones = json.loads(out.splitlines()[0])["layout"]["zones"]
    except (IndexError, KeyError, json.JSONDecodeError):
        return None
    return sum(1 for z in zones if isinstance(z.get("face"), dict))


def who_is_in(space, rel, a, b, scratch):
    """faces at a clip's start, middle and end: [(seconds in the take, faces or None)]"""
    ts = [a + 0.15, (a + b) / 2, max(a + 0.15, b - 0.15)]
    return [(t, faces_at(space, rel, t, scratch)) for t in ts]


def who_notes(tl):
    """a blooper the transcript can't hear (an agent session, 2026-10-16: someone walking into frame), against the rest of the
    edit: a clip with more faces than most of the edit's clips has someone else in shot; one whose face count rises
    inside it has someone walking in; one with no face where the edit has one has lost its speaker"""
    # a beat's clip was chosen for what it shows (people:2 asks for two): only the lines' clips are read for bloopers
    counts = [max((x for _, x in c.vars.get("--faces", []) if x is not None), default=None) if c.line >= 0 else None for c in tl.clips]
    seen = [x for x in counts if x is not None]
    if not seen:
        return
    usual = max(set(seen), key=seen.count)
    for c, most in zip(tl.clips, counts):
        fs = [(t, x) for t, x in c.vars.get("--faces", []) if x is not None] if c.line >= 0 else []
        if not fs:
            continue
        where = f"{f'line {c.line + 1}' if c.line >= 0 else 'a beat'} ({c.take} {c.src_in:.1f}-{c.src_out:.1f} s)"
        if fs[-1][1] > fs[0][1]:
            tl.notes.append(f"{where}: someone comes into frame (at {fs[-1][0]:.1f} s in the take): check the take, or use another read")
        elif most > usual:
            tl.notes.append(f"{where}: {most} faces in shot where the rest of the edit has {usual}: someone else is in the take; check it, or use another read")
        elif usual >= 1 and min(x for _, x in fs) == 0:
            tl.notes.append(f"{where}: the speaker's face leaves the frame: check the take")


def source_text(space, rel, a, b, crop, H, scratch):
    """captions already burned into the footage over [a, b]: raea-perceive read-text on a 2 fps proxy of that span,
    caption-like lines only (three or more words across a third of the width or more, not a label like "Cabelas"),
    as [top, bottom] in edit pixels through the clip's crop"""
    if not crop:
        return []
    proxy = scratch / f".text-{abs(hash((str(space), rel, round(a, 2), round(b, 2))))}.mp4"
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", f"{a:.2f}", "-t", f"{max(0.5, b - a):.2f}", "-i", str(space / rel), "-an",
                    "-vf", "fps=2,scale=540:-2", "-c:v", "libx264", "-crf", "23", str(proxy)], check=False)
    if not proxy.exists():
        return []
    out = subprocess.run([str(PERCEIVE), "read-text", str(proxy)], capture_output=True, text=True).stdout
    proxy.unlink()
    try:
        rows = json.loads(out.splitlines()[0])["rows"]
    except (IndexError, KeyError, json.JSONDecodeError):
        return []
    sh, top, ch0 = crop
    boxes = []
    for r in rows:
        x, y, w_, h_ = r["box"]
        if len(r.get("text", "").split()) >= 3 and w_ >= 0.35:
            y0, y1 = (y * sh - top) / ch0 * H, ((y + h_) * sh - top) / ch0 * H
            if y1 > 0 and y0 < H:
                boxes.append([round(max(0, y0)), round(min(H, y1))])
    return boxes


def edit_shape(space):
    """the size when the brief's Format gives no ratio: 9:16 at 1080 (rrl's edit.rrl, which used to say, is gone)"""
    return 1080, 1920


# ---- placing the script on the footage ----
DUR = {"sm": 0.2, "md": 0.3, "lg": 0.5}  # Material 3's motion tokens short4, medium2, long2 (seconds)


def safe_zone(sc, w, h):
    """the edit's safe rows (top, bottom) from the platforms the script names (its Format, or `platform:` in :root):
    the strictest of their button bands, scaled to the edit's height; the house band when none is named"""
    named = (sc.format + " " + sc.style.get(":root", {}).get("platform", "")).lower()
    ps = [p for p in ("tiktok", "reels", "shorts") if p in named or (p == "reels" and "instagram" in named) or (p == "shorts" and "youtube" in named)] or ["house"]
    k = h / 1920
    return round(max(PLATFORM_BANDS[p][0] for p in ps) * k), round(h - max(PLATFORM_BANDS[p][2] for p in ps) * k), ps


def bands_on(fr, w, h, safe):
    """the clear bands on a clip: above and below the face when one is seen (void's region margin, 6% of the width),
    else the halves of the safe area; center and full as void's regions have them"""
    top, bottom = safe; m = round(w * 0.06)
    if fr.get("--face-top") is not None:
        b = {"top": (top, fr["--face-top"] - m), "low": (fr["--face-bottom"] + m, bottom)}
    else:
        b = {"top": (top, h / 2), "low": (h / 2, bottom)}
    b["center"] = (h * 0.33, h * 0.67); b["full"] = (top, bottom)
    b["auto"] = max((b["top"], b["low"]), key=lambda r: r[1] - r[0])
    return b


def frame_of(word_frames, phrase, lo, hi, edge="start"):
    """the edit frame where a phrase is said inside [lo, hi), from the kept words; None if it isn't"""
    q = norm(phrase)
    toks_ = [(f, t) for (f0, f1, txt) in word_frames if lo <= f0 < hi for f, t in [((f0, f1), x) for x in norm(txt)]]
    words = [t for _, t in toks_]
    for i in range(len(words) - len(q) + 1):
        if words[i:i + len(q)] == q:
            return toks_[i][0][0] if edge == "start" else toks_[i + len(q) - 1][0][1]
    return None


def seed_of(prompt):
    return int(hashlib.md5(prompt.encode()).hexdigest()[:6], 16) % 100000


def resolve(sc, space, tk, scratch):
    """the Script on this space's takes: clips at clean edit points, framing, the script's parts timed to their words and
    placed where it asks clear of the face, captions only if it asks, sound only as it asks. Lines no take with picture
    says are kept in `dropped`; anything the script asked for that can't be done goes to `notes`."""
    w, h = sc.shape or edit_shape(space); files = media_files(space / "media")
    have, kit_source = parts_in(space); brand = brand_of(space); root = sc.style.get(":root", {})
    top, bottom, platforms = safe_zone(sc, w, h)
    tl = Timeline(fps=FPS, width=w, height=h, frames=0, vars={"--safe-top": top, "--safe-bottom": bottom})
    tl.background = root.get("background") or brand_color(brand, "background")
    tl.kit = kit_source
    if kit_source == "global":
        tl.notes.append("the space has no parts/ kit of its own: the global kit was used (seed the brand's kit into the space)")
    tkv = {k: v for k, v in tk.items() if k in files}  # a script names lines, not files: words found in takes with picture
    framed, t, raw, word_frames, cap_notes = {}, 0, [], [], []
    # the script's lines and its open beats (a .brief's ~ lines) in the order they stand; a beat is bound to the best
    # span of footage the moments table has for it (moments.py), or left open and said in the sheet
    order, bi = [], 0
    beats = sorted(getattr(sc, "beats", []), key=lambda x: x.get("after", len(sc.lines)))
    for li, ln in enumerate(sc.lines):
        while bi < len(beats) and beats[bi].get("after", 0) <= li:
            order.append((-1, beats[bi])); bi += 1
        order.append((li, ln))
    order += [(-1, x) for x in beats[bi:]]
    for li, ln in order:
        if li < 0:
            from . import moments
            bd = moments.bind(ln, space, files, tk, used=[(c.take, c.src_in, c.src_out) for c in tl.clips])
            ln["bound"] = bd
            if not bd or bd["verdict"] not in ("covered", "weak") or not bd["spans"]:
                continue
            best = bd["spans"][0]
            sp = (best["take"], best["from"], best["to"])
            ln = Line(role=ln.get("section"), take=best["take"], text="", notes=[])
        else:
            sp = find_span(tkv, ln.text, ln.take) or find_span(tkv, ln.text)
        if not sp:
            tl.dropped.append(li); continue
        take, a, b = sp
        # a clean edit point: half the gap to the neighbouring words, at most 60 ms before and 250 ms after (speech, not taste)
        ws = tk.get(take, []); prev = max((x[1] for x in ws if x[1] <= a + 1e-6), default=None); nxt = min((x[0] for x in ws if x[0] >= b - 1e-6), default=None)
        pre = min(0.06, (a - prev) / 2) if prev is not None else 0.06
        post = min(0.25, (nxt - b) / 2) if nxt is not None else 0.3
        src_in = max(0.0, a - pre); src_out = min(b + post, media_len(space / files[take]) - 0.05)
        f0 = t; f1 = t + max(1, round((src_out - src_in) * FPS)); t = f1
        tl.takes[take] = files[take]
        eyes = root.get("eyes", "").strip().rstrip("%"); eyes = float(eyes) / 100 if eyes else 0.38
        ing = ingested(space, files[take])
        if ing:
            fr = frame_clip(ing, a, b, src_in, src_out, w, h, eyes)
        else:
            if take not in framed:
                framed[take] = frame_take(space, files[take], a + (b - a) / 2, w, h, scratch, eyes=eyes)
            fr = framed[take]
        utils = [u for n in ln.notes for u in n.utils]
        punch = ln.punch if isinstance(ln.punch, float) else (float(util(utils, "punch-in")) if util(utils, "punch-in") else None)
        if ln.punch == "unset":
            tl.notes.append(f"line {li + 1}: PUNCH IN without how far (PUNCH IN 1.2X: or punch-in:1.2): left as it is")
        cv = {k: v for k, v in fr.items() if k.startswith("--")}
        if not ing:
            cv["--source-text"] = source_text(space, files[take], src_in, src_out, fr.get("crop"), h, scratch)
            cv["--faces"] = who_is_in(space, files[take], src_in, src_out, scratch)
        if not cv.get("--source-text"):
            cv.pop("--source-text", None)
        tl.clips.append(Clip(name=f"c{len(tl.clips) + 1}", take=take, file=files[take], src_in=round(src_in, 3), src_out=round(src_out, 3),
                             start=f0, end=f1, line=li, zoom=punch, pan_x=fr.get("pan_x"), pan_y=fr.get("pan_y"), vars=cv))
        for wd in ws:
            if src_in <= wd[0] < src_out:
                tl.words.append({"start_ms": (f0 / FPS + wd[0] - src_in) * 1000, "end_ms": (f0 / FPS + wd[1] - src_in) * 1000, "text": wd[2]})
                word_frames.append((f0 + round((wd[0] - src_in) * FPS), f0 + round((wd[1] - src_in) * FPS), wd[2]))
        bands = bands_on(fr, w, h, (top, bottom))
        for n in ln.notes:
            if not n.kind:
                if util(n.utils, "sfx"):  # foley on the line itself
                    raw.append(Part(kind="", name="", props={}, start=f0, end=f1, sfx=util(n.utils, "sfx"), motion=n.utils))
                continue
            if n.kind == "captions":
                cap_notes.append(n); continue
            if n.kind not in have:
                tl.refused.append({"part": f"{n.kind} on line {li + 1}", "error": f"this space has no part {n.kind}: it has {', '.join(sorted(have)) or 'none'} (write components/{n.kind}.tsx for your own)"})
                continue
            band = next((u.split("-", 1)[1] for u in n.utils if u.startswith("place-")), "auto")
            y0, _ = bands[band]
            part = Part(kind=n.kind if have[n.kind] == "kit" else "component", name="", props=dict(n.props), start=f0, end=f1,
                        band=band, place=f"band({band})", y=round(y0), component=n.kind if have[n.kind] == "component" else None,
                        sfx=util(n.utils, "sfx"), motion=n.utils)
            part.anchor, part.until, part.to_end, part.line = n.anchor, util(n.utils, "until"), "to-end" in n.utils, (f0, f1)
            raw.append(part)
    tl.frames = t
    who_notes(tl)
    from .moments import watermarks
    for take in dict.fromkeys(c.take for c in tl.clips):
        ing = ingested(space, files[take]) if take in files else None
        for mark in watermarks(ing["facts"]) if ing else []:  # not `w`: that's the edit's width below
            tl.notes.append(f"{take} carries a watermark or burned-in mark (\"{mark['text']}\", on {mark['share']:.0%} of it): it shows wherever the take does")
    # timing on the words: on:'...' starts a part where it's said in its line, until:'...' ends it after those words
    for p in raw:
        if getattr(p, "anchor", None):
            f = frame_of(word_frames, p.anchor, p.line[0], p.line[1])
            if f is None:
                tl.notes.append(f"{p.kind or 'sfx'}: on:'{p.anchor}' isn't said in its line; it starts with the line")
            else:
                p.start = f
        if getattr(p, "until", None):
            f = frame_of(word_frames, p.until, p.start, t, edge="end")
            if f is None:
                tl.notes.append(f"{p.kind}: until:'{p.until}' isn't said after it starts; it ends with its line")
            else:
                p.end = f
        if getattr(p, "to_end", False):
            p.end = t
        p.end = min(max(p.end, p.start + 1), t)
    named = {}
    for i, p in enumerate(raw):
        p.name = p.name or (f"{p.component}{i + 1}" if p.kind == "component" else f"{p.kind}{i + 1}" if p.kind else f"sfx{i + 1}")
        named.setdefault(p.kind if p.kind != "component" else p.component, p)
    for p in raw:
        after = util(p.motion or [], "stagger-after")
        if after and after in named:
            gap = DUR.get(next((u[4:] for u in p.motion if u.startswith("dur-")), "sm"))
            p.start = max(p.start, named[after].start + round(gap * FPS))
    # captions only when the script asks (a captions note, or a captions rule in the stylesheet), placed where it says
    cap_style = sc.style.get("captions")
    if cap_notes or cap_style is not None:
        if "captions" not in have:
            tl.refused.append({"part": "captions", "error": f"this space has no captions part: it has {', '.join(sorted(have)) or 'none'}"})
        else:
            utils = [u for n in cap_notes for u in n.utils] + ([f"place-{cap_style['place']}"] if cap_style and cap_style.get("place") else [])
            want = next((u.split("-", 1)[1] for u in utils if u.startswith("place-")), "auto")
            keys = sorted({k.strip(" *\"[]").lower() for n in cap_notes for k in re.split(r"[,\[\]\"]", n.props.get("keys", "")) if k.strip(" *\"[]")}
                          | {k.strip().lower() for k in (cap_style or {}).get("keys", "").strip('"').split(",") if k.strip()})
            other = {"top": "low", "low": "top"}
            runs = []
            for c in tl.clips:
                bands = bands_on({k: v for k, v in c.vars.items()}, w, h, (top, bottom))
                band = want if want != "auto" else ("top" if bands["auto"] == bands["top"] else "low")
                boxes = c.vars.get("--source-text", [])
                clash = lambda b_: any(b0 < bands[b_][1] and b1 > bands[b_][0] for b0, b1 in boxes)
                if boxes and clash(band):
                    alt = other.get(band)
                    band2 = alt if alt and not clash(alt) else None
                    tl.notes.append(f"{c.take} has captions burned in at {c.start / FPS:.1f}-{c.end / FPS:.1f} s: "
                                    + (f"ours move to the {band2} band over it" if band2 else "ours stay out over it"))
                    band = band2
                y = round(bands[band][0]) if band else None
                if runs and runs[-1][2] == y and runs[-1][1] == c.start:
                    runs[-1][1] = c.end
                else:
                    runs.append([c.start, c.end, y, band])
            # a part in the captions' band holds it for its span: captions step aside there
            held = sorted((p.start, p.end, p.band) for p in raw if p.kind and p.band)
            out = []
            for a_, b_, y, band in [r if len(r) == 4 else r + [None] for r in runs]:
                if y is None:
                    continue
                cur = a_
                for h0, h1, pb in held:
                    if pb != band or h1 <= cur or h0 >= b_:
                        continue
                    if h0 > cur:
                        out.append([cur, h0, y, band])
                    cur = max(cur, h1)
                if cur < b_:
                    out.append([cur, b_, y, band])
            for i, (a_, b_, y, band) in enumerate([r for r in out if r[1] - r[0] >= 6]):
                raw.append(Part(kind="captions", name=f"captions{i}", props={"keys": ",".join(keys)} if keys else {}, start=a_, end=b_, band=band, place=f"band({band})", y=y))
    tl.parts = [p for p in raw if p.kind]
    # sound only as the script asks: a bed from :root's music prompt at its music-gain; foley from sfx:'prompt' at sfx-gain
    if root.get("music"):
        g = root.get("music-gain")
        if g is None:
            tl.notes.append("music asked for without music-gain in :root: not added (say how loud under the voice)")
        else:
            prompt = root["music"].strip('"')
            tl.sounds.append(Sound("bed", prompt, "lyria", seed_of(prompt), 0, round(t / FPS) + 2, float(g)))
    for p in raw:
        if p.sfx:
            g = util(p.motion or [], "sfx-gain") or root.get("sfx-gain")
            if g is None:
                tl.notes.append(f"foley '{p.sfx}' without sfx-gain (on the note or in :root): not added")
                continue
            tl.sounds.append(Sound("fx", p.sfx, "sao", seed_of(p.sfx), max(0, p.start), 1, float(g)))
    tl.vars["--platforms"] = ",".join(platforms)
    return tl
