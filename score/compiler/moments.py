"""The moments table and the search a .brief's open beats are bound with (P3, brief/format.feature; Track F2).

A moment is one SigLIP frame of a take (8 a second), with the perception facts true then, read from the take's ingest
(raea-perceive ingest; front.ingested): the faces in shot and whether the speaker's is, speaking and eye contact
(performance, per second), the shot's framing and motion, text on screen, a screen, the light, and the words said.
An open beat (`~ KIND: PHRASE [find: ...]`) is searched as the phrase (or a $picture) against every moment's SigLIP
row, among the moments its fact finds hold for, and bound to the best span or named weak or missing, with how it was
matched: by facts, by a SigLIP score against FIND_THRESHOLD, or both.

    rows(space, rel)            a take's moments (cached per take)
    table(space, only)          the takes' moments as one table, SPACE/.score/moments.jsonl (python -m compiler.moments SPACE [--src A,B])
    bind(beat, space, files, tk)  the beat's best spans, best first, and the verdict
    needs(beats, tl, space)     what the footage can't give, as items to act on (NAME.needs.json beside the edit)
"""
import array, json, re, subprocess
from pathlib import Path

from . import front

FIND_THRESHOLD = 0.10       # SigLIP 2 text-image, rdb/src/assets.rs (present concepts 0.12-0.15, absent 0.03-0.06)
PICTURE_THRESHOLD = 0.70    # SigLIP 2 image-image: not calibrated yet (a guess, said in the sheet)
EMBED = Path.home() / "raea/meaning/target/release/raea-embed"
FPS = 8.0
SPAN = (1.5, 4.0)           # a beat's clip, seconds: no line times it, so it runs as long as the moment holds, in these bounds
DETECTED = {"face", "speech", "text", "screen", "speaking", "eye-contact", "still", "moving", "bright", "no-text"}
SIGLIP_ONLY = {"product", "logo", "action", "hands", "place"}   # no detector in ingest yet: the phrase carries them
SCREENISH = {"document", "screenshot", "website", "chart", "diagram", "text", "computer", "monitor", "display", "web site"}

_ROWS, _VEC = {}, {}


def embed_text(phrases):
    new = [p for p in phrases if p not in _VEC]
    if new:
        out = subprocess.run([str(EMBED), "text", *new], capture_output=True, text=True).stdout
        for l in out.splitlines():
            d = json.loads(l); _VEC[d["text"]] = array.array("f", d["v"])
    return [_VEC.get(p) for p in phrases]


def embed_image(path):
    key = ("image", str(path))
    if key not in _VEC:
        out = subprocess.run([str(EMBED), "image", str(path)], capture_output=True, text=True).stdout.splitlines()
        _VEC[key] = array.array("f", json.loads(out[0])["v"]) if out else None
    return _VEC[key]


def watermarks(facts):
    """text that stays on a take (a stock or app watermark, a burned-in logo): what it says and its box. The same words
    read three times or more at the same place (centres within a tenth of the frame), from one end of half the take or
    more to the other, are the take's, not a moment's; a faint mark is read in only some samples, so this asks for the
    spread, not the coverage"""
    secs = facts.get("seconds") or 0
    if not secs:
        return []
    key = lambda t: re.sub(r"[^a-z0-9]", "", t.lower())
    seen = {}
    for r in facts.get("text", []):
        k = key(r.get("text", ""))
        if k:
            seen.setdefault(k, []).append(r)
    out = []
    centre = lambda b: (b[0] + b[2] / 2, b[1] + b[3] / 2)
    for k, rs in seen.items():
        c0 = centre(rs[0]["box"])
        same = [r for r in rs if abs(centre(r["box"])[0] - c0[0]) <= 0.1 and abs(centre(r["box"])[1] - c0[1]) <= 0.1]
        spread = max(r["end"] for r in same) - min(r["start"] for r in same)
        if len(same) >= 3 and spread >= secs * 0.5:
            out.append({"text": rs[0]["text"], "box": rs[0]["box"], "share": round(min(1.0, spread / secs), 2)})
    return out


def rows(space, rel, tk_words=None):
    """a take's moments: [{t, faces, speaker, speaking, eye, framing, motion, text, screen, luma, said}] at 8 fps, and
    its SigLIP rows; None when the take isn't ingested"""
    p = space / rel
    if p in _ROWS:
        return _ROWS[p]
    ing = front.ingested(space, rel)
    if not ing:
        _ROWS[p] = None; return None
    d = ing["dir"]
    meta = json.loads((d / "siglip.json").read_text()) if (d / "siglip.json").exists() else None
    col = array.array("f")
    if meta:
        col.frombytes((d / "siglip.f32").read_bytes())
    perf = json.loads((d / "performance.json").read_text()) if (d / "performance.json").exists() else {"seconds": []}
    light = json.loads((d / "light.json").read_text()) if (d / "light.json").exists() else {"frames": []}
    facts, faces = ing["facts"], ing["faces"]
    secs = {int(s_["t"]): s_ for s_ in perf.get("seconds", [])}
    wm = {re.sub(r"[^a-z0-9]", "", w["text"].lower()) for w in watermarks(facts)}
    content = [r for r in facts.get("text", []) if re.sub(r"[^a-z0-9]", "", r.get("text", "").lower()) not in wm]
    n = meta["n"] if meta else 0
    fs = faces.get("frames", [])
    track = front.speaker_track(faces, 0, facts.get("seconds", 1e9))
    vis = {round(t, 2) for t, _ in track}
    out = []
    for i in range(n):
        t = i / (meta["fps"] if meta else FPS)
        fr = min(fs, key=lambda x: abs(x["t"] - t)) if fs else {"faces": []}
        shot = next((s_ for s_ in facts.get("shots", []) if s_["start"] <= t < s_["end"]), {})
        zones = (shot.get("layout") or {}).get("zones", [])
        sec = secs.get(int(t), {})
        lu = min(light.get("frames", []), key=lambda x: abs(x["t"] - t))["luma"] if light.get("frames") else None
        out.append({
            "t": round(t, 3), "faces": len(fr.get("faces", [])),
            "speaker": any(abs(v - t) <= 0.13 for v in vis),
            "speaking": (sec.get("speaking") or 0) >= 0.5, "eye": sec.get("eye_contact") == "lens",
            "framing": shot.get("framing"), "motion": shot.get("motion"),
            "text": any(r["start"] - 0.5 <= t <= r["end"] + 0.5 for r in content), "watermark": bool(wm),
            "screen": any(z.get("role") == "video" for z in zones) or (shot.get("faces", 1) == 0 and bool(SCREENISH & set(shot.get("scene") or []))),
            "luma": lu,
            "said": any(a <= t <= b for a, b, _ in (tk_words or [])),
        })
    _ROWS[p] = (out, col, meta["dim"] if meta else 768)
    return _ROWS[p]


def holds(f, m):
    """whether a find that perception detects holds at a moment (format.md's find table)"""
    if f == "face": return m["faces"] >= 1
    if f == "speech": return m["said"]
    if f == "speaking": return m["speaking"] or m["said"] and m["speaker"]
    if f == "eye-contact": return m["eye"]
    if f == "text": return m["text"]
    if f == "no-text": return not m["text"]
    if f == "screen": return m["screen"]
    if f in ("still", "moving"): return m["motion"] == f
    if f == "bright": return (m["luma"] or 0) >= 0.45
    if f.startswith("people:"):
        lo, _, hi = f.split(":")[1].partition("-")
        return int(lo) <= m["faces"] <= int(hi or lo)
    if f.startswith("framing:"): return m["framing"] == f.split(":")[1]
    return True


def bind(beat, space, files, tk, used=()):
    """the beat's best spans across the space's takes with picture: {verdict, by, spans: [{take, file, from, to,
    score, facts}], why}. `used`: (take, a, b) spans the edit already keeps, not bound again"""
    finds = beat.get("find") or []
    facts_ = [f for f in finds if isinstance(f, str) and (f in DETECTED or ":" in f)]
    pics = [f["picture"] for f in finds if isinstance(f, dict) and f.get("picture")]
    if beat["kind"] == "end-card":
        return {"verdict": "part", "by": "kind", "spans": [], "why": "an end card is a part, not footage: write [[end-card ...]] on the last line"}
    if pics:
        q, thr, by = embed_image(space / pics[0]), PICTURE_THRESHOLD, f"the picture {pics[0]} (threshold {PICTURE_THRESHOLD}, not calibrated)"
    else:
        q, thr, by = embed_text([beat["phrase"]])[0], FIND_THRESHOLD, f"SigLIP on the phrase (threshold {FIND_THRESHOLD})"
    cands, searched = [], 0
    for take, rel in files.items():
        r = rows(space, rel, tk.get(take))
        if not r or q is None:
            continue
        searched += 1
        ms, col, dim = r
        scores = [sum(a * b for a, b in zip(q, col[i * dim:(i + 1) * dim])) for i in range(len(ms))]
        ok = [all(holds(f, m) for f in facts_) for m in ms]
        quiet = beat["kind"] in ("b-roll", "product", "screen") and "speech" not in finds and "speaking" not in finds
        # a span grows from each peak while its facts hold and its score stays near the peak
        order = sorted(range(len(ms)), key=lambda i: -scores[i])
        taken = []
        for i in order[:200]:
            if not ok[i] or any(a <= i <= b for a, b in taken):
                continue
            a = b = i
            while a > 0 and ok[a - 1] and scores[a - 1] >= scores[i] - 0.03 and (i - a + 1) / FPS < SPAN[1] / 2:
                a -= 1
            while b + 1 < len(ms) and ok[b + 1] and scores[b + 1] >= scores[i] - 0.03 and (b - a + 1) / FPS < SPAN[1]:
                b += 1
            while (b - a + 1) / FPS < SPAN[0] and (b + 1 < len(ms) or a > 0):
                if b + 1 < len(ms): b += 1
                if (b - a + 1) / FPS < SPAN[0] and a > 0: a -= 1
            t0, t1 = ms[a]["t"], ms[b]["t"] + 1 / FPS
            if any(tt == take and t0 < u1 and t1 > u0 for tt, u0, u1 in used):
                continue
            talk = sum(ms[k]["said"] for k in range(a, b + 1)) / (b - a + 1)
            taken.append((a, b))
            cands.append({"take": take, "file": rel, "from": round(t0, 2), "to": round(t1, 2), "score": round(scores[i], 3),
                          "facts": facts_, "talk": round(talk, 2), "rank": scores[i] - (0.05 * talk if quiet else 0)})
            if len(taken) >= 3:
                break
    if not searched:
        return None  # no take is ingested (or the phrase couldn't be embedded): nothing was searched
    cands.sort(key=lambda c: -c["rank"])
    for c in cands:
        c.pop("rank")
    best = cands[0] if cands else None
    only = [f for f in finds if isinstance(f, str) and f in SIGLIP_ONLY]
    if best and best["score"] >= thr:
        verdict = "covered"
    elif best and best["score"] >= thr * 0.6:
        verdict = "weak"
    else:
        verdict = "missing"
    why = by + (f"; {', '.join(only)} not detected in ingest, carried by the phrase" if only else "") + (f"; facts held: {' '.join(facts_)}" if facts_ else "")
    if verdict == "missing":
        why += "; " + ("record it (a screen recording of what the beat shows)" if beat["kind"] == "screen" else
                       "a reshoot: it has to be the creator" if beat["kind"] == "to-camera" else
                       "find stock (Pixabay) or shoot it")
    return {"verdict": verdict, "by": by, "spans": cands[:3], "why": why}


def table(space, only=None):
    """the space's moments as one table, SPACE/.score/moments.jsonl: a row per SigLIP frame of every take with picture
    (take, file, from, to, the facts, the words said then, and where its SigLIP row is: the ingest column and index);
    `only`: the takes whose files are these (real paths), as a checker that needs just what a render shows asks"""
    import os
    space = Path(space).resolve()
    tk = front.takes(space / "media"); files = front.media_files(space / "media")
    if only is not None:
        want = {os.path.realpath(p) for p in only}
        files = {k: v for k, v in files.items() if os.path.realpath(space / v) in want}
    out = space / ".score" / "moments.jsonl"; out.parent.mkdir(exist_ok=True)
    n = 0
    with open(out, "w") as f:
        for take, rel in sorted(files.items()):
            r = rows(space, rel, tk.get(take))
            if not r:
                continue
            ms, _, _ = r
            col = str(front.ingested(space, rel)["dir"] / "siglip.f32")
            for i, m in enumerate(ms):
                words = [w for a, b, w in tk.get(take, []) if a < m["t"] + 1 / FPS and b > m["t"]]
                f.write(json.dumps({"take": take, "file": rel, "from": m["t"], "to": round(m["t"] + 1 / FPS, 3), **{k: v for k, v in m.items() if k != "t"},
                                    "words": " ".join(words), "siglip": [col, i]}) + "\n")
                n += 1
    return out, n


if __name__ == "__main__":
    import sys
    only = sys.argv[sys.argv.index("--src") + 1].split(",") if "--src" in sys.argv else None
    p, n = table(sys.argv[1], only)
    print(f"{n} moments in {p}")


def needs(beats, tl, space):
    """the open beats the footage doesn't cover, as items someone (the agent or a person) can act on: a reshoot when it
    has to be the creator, a screen recording, a stock search (Pixabay: the generation policy makes no AI picture), a
    part to write for an end card, and the weak ones to improve; each with the beat, its finds, where it stands, and the
    look of the edit's footage to match (framing, light) from the moments of its clips"""
    looks = []
    for c in tl.clips:
        r = rows(Path(space), c.file)
        if r:
            ms = [m for m in r[0] if c.src_in <= m["t"] <= c.src_out]
            if ms:
                looks.append({"framing": max({m["framing"] for m in ms}, key=lambda f: sum(m["framing"] == f for m in ms)),
                              "luma": round(sum(m["luma"] or 0 for m in ms) / len(ms), 2)})
    look = looks[0] if looks else {}
    out = []
    for b in beats:
        bd = b.get("bound")
        if bd and bd["verdict"] == "covered":
            continue
        finds = [f if isinstance(f, str) else f.get("picture") for f in b.get("find", [])]
        item = {"beat": f"~ {b['kind']}: {b['phrase']}", "finds": finds, "at": b.get("at"), "section": b.get("section"), "look": look}
        if not bd:
            item.update(kind="search", why="the takes aren't ingested: ingest them and compile again")
        elif bd["verdict"] == "part":
            item.update(kind="part", why=bd["why"], do="write [[end-card ...]] on the last line (the space's components/end-card.tsx)")
        elif b["kind"] == "to-camera":
            item.update(kind="reshoot", why=bd["why"], do=f"the creator, to camera: {b['phrase']}", best=bd["spans"][:1])
        elif b["kind"] == "screen":
            item.update(kind="record-screen", why=bd["why"], do=f"record the screen: {b['phrase']}", best=bd["spans"][:1])
        else:
            item.update(kind="stock", why=bd["why"], do=f"find stock footage (Pixabay): {b['phrase']}", query=b["phrase"], best=bd["spans"][:1])
        if bd and bd["verdict"] == "weak":
            item["kind"] = "improve:" + item["kind"]
        out.append(item)
    return out
