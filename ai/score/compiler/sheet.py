"""The exposure sheet: the script timed against its takes, the total against its Format, and what the notation can't
read. It depends on the script and the transcripts only, never on a target, so it reads the same whether the edit is
written (the sheet M3 revised from in ai/score/bench, grown since: checks, open beats, what brings it to length).
"""
import re

from .front import UTIL, find_span

MUSTS = ["the hook reads within half a second", "nothing covers the face", "captions read on a phone",
         "everything inside the safe zone", "ends on a word, never mid-word", "no black frames", "the length is within the format"]
# what the compiler reads from the stylesheet: :root's background, music and music-gain, sfx-gain, platform and eyes
# (the face's eye line, as a percentage of the height); captions' place and keys. Anything else does nothing, so it's said
NOMINAL_BEAT = 2.5  # seconds an open beat is counted at before the footage is searched (its clip runs 1.5-4 s)
CSS_PROPS = {"background", "music", "music-gain", "sfx-gain", "platform", "eyes", "place", "keys"}


def unreadable(sc, notes_everywhere):
    """what the notation can't read: unknown roles, parts, props, utilities, stylesheet properties and rules"""
    bad = []  # sections are the writer's own: no list of roles to keep to
    for n in notes_everywhere:
        bad += n.unknown
    css = (sc.text.split("\n---\n", 1) + [""])[1]
    for sel, body in re.findall(r"([^{}]+)\{([^}]*)\}", css):
        if sel.strip() == "@must":
            continue
        for decl in [d.strip() for d in body.split(";") if d.strip()]:
            k = decl.split(":", 1)[0].strip()
            if ":" not in decl or k not in CSS_PROPS:
                bad.append(f"css {decl if ':' not in decl else k}")
    m = re.search(r"@must\s*\{([^}]*)\}", (sc.text.split("\n---\n", 1) + [""])[1])
    for s in [x.strip() for x in (m.group(1).split(";") if m else []) if x.strip()]:
        if not any(s.lower().startswith(x[:18]) for x in MUSTS):
            bad.append(f"must {s}")
    return bad


def notes_in(text):
    """every [[ ]] in the script, wherever it stands (a synopsis carries style too)"""
    from .front import parse_note, strip_fence
    body = strip_fence(text).split("\n---\n", 1)[0]
    return [parse_note(n) for n in re.findall(r"\[\[(.*?)\]\]", body)]


REHEARSAL = re.compile(r"\b(okay so|ok so|let me (try|do|start)|one more (time|take)|take (two|three|2|3)|from the top|start (over|again)|"
                       r"hold on|wait,? (no|sorry)|sorry,? (let|i)|and then you (would|could|pan)|pan (to|over) here|is it (rolling|recording)|"
                       r"are we (rolling|recording)|did (that|it) (work|record)|cut,? cut|and action)\b", re.I)
CONJ = re.compile(r"^(and|also|but|so|because|or|then|plus)\b", re.I)


def read_flags(text, take_hint, tk, first=False):
    """what looks wrong with one read of a line, from the transcripts alone: (span, [why]) or None when no take says it"""
    from .front import norm
    sp = find_span(tk, text, take_hint) or find_span(tk, text)
    if not sp:
        return None
    take, a, b = sp
    ws = tk[take]
    said = [w for w in ws if a - 1e-6 <= w[0] and w[1] <= b + 1e-6]
    toks = [t for w in said for t in norm(w[2])]
    why = []
    rep_ = next((toks[j] for j in range(len(toks) - 1) if toks[j] == toks[j + 1] and len(toks[j]) > 1 or
                 (j + 3 < len(toks) and toks[j:j + 2] == toks[j + 2:j + 4])), None)
    if rep_:
        why.append(f"a stutter (\"{rep_} {rep_}\")")
    if REHEARSAL.search(text) or REHEARSAL.search(" ".join(x[2] for x in ws if a - 4 <= x[0] < a)):
        why.append("rehearsal talk in or just before it")
    q = norm(text)[:5]
    if len(q) >= 4:
        flat = [t for w in ws for t in norm(w[2])]
        starts = [j for j in range(len(flat) - len(q) + 1) if flat[j:j + len(q)] == q]
        # which read this is: the token index where the quoted span starts
        at = sum(len(norm(w[2])) for w in ws if w[0] < a - 1e-6)
        k = next((n + 1 for n, j in enumerate(starts) if j == at), None)
        if len(starts) > 1 and k is not None and k < len(starts):
            why.append(f"read {k} of {len(starts)} in this take (a later read is usually the clean one)")
    if first and (CONJ.match(text.strip()) or "also" in [t for t in norm(text)[:3]]):
        why.append("the opening starts mid-thought")
    after = [w for w in ws if w[0] >= b - 1e-6 and w[0] > a][:4]
    if after and after[0][0] - b < 0.35 and not re.search(r"[.?!]\W*$", said[-1][2] if said else ""):
        why.append(f"it stops mid-sentence (the take goes on: \"{' '.join(w[2] for w in after)}\")")
    return sp, why


def takes_to_check(sc, tk):
    """quoted lines that look like a rehearsal, a retake or a bad read, from the transcripts alone (the judge, stage 2:
    blooper takes, openings mid-thought, a line cut off while the speaker keeps going, stutters left in)"""
    flags = []
    for i, ln in enumerate(sc.lines):
        r = read_flags(ln.text, ln.take, tk, first=i == 0)
        if r and r[1]:
            (take, a, _), why = r
            flags.append(f"- line {i + 1} ({take} {a:.1f} s): " + "; ".join(why))
    return flags


def pick_alternatives(sc, tk):
    """`first read || second read`: each line with alternatives gets the one said cleanly (no read flags) that brings the
    running total nearest the middle of what the Format allows; the line's text becomes it, and `picked` says why"""
    lines = sc.lines
    if not any(getattr(ln, "alts", None) for ln in lines) or getattr(sc, "picked", False):
        return
    sc.picked = True
    dur = lambda ln: (lambda sp: sp[2] - sp[1] if sp else 0.0)(find_span(tk, ln.text, ln.take) or find_span(tk, ln.text))
    mid = (sc.secs[0] + sc.secs[1]) / 2 if sc.secs else None
    for i, ln in enumerate(lines):
        if not getattr(ln, "alts", None):
            continue
        rest = sum(dur(x) for j, x in enumerate(lines) if j != i)
        cands = []
        for k, alt in enumerate(ln.alts):
            r = read_flags(alt, ln.take, tk, first=i == 0)
            if not r:
                cands.append((k, alt, None, ["no take says it"]))
                continue
            (_, a, b), why = r
            cands.append((k, alt, b - a, why))
        said = [c for c in cands if c[2] is not None]
        if not said:
            ln.picked = "none of its reads is said in a take: the first is kept"; continue
        clean = [c for c in said if not c[3]] or said
        best = min(clean, key=lambda c: abs(rest + c[2] - mid) if mid else c[0])
        ln.text = best[1]
        others = "; ".join(f"\"{c[1][:40]}\": " + (", ".join(c[3]) if c[3] else f"{c[2]:.1f} s puts the piece at {rest + c[2]:.1f} s") for c in cands if c is not best)
        ln.picked = (f"read {best[0] + 1} \"{best[1][:50]}\" ({best[2]:.1f} s, {'said cleanly' if not best[3] else ', '.join(best[3])}"
                     + (f", the piece at {rest + best[2]:.1f} s of {sc.secs[0]:.1f}-{sc.secs[1]:.1f}" if mid else "") + ")"
                     + (f" over {others}" if others else ""))


def sheet(sc, tk):
    pick_alternatives(sc, tk)
    out = ["| # | take | line | said from-to s | lasts s | ends at s |", "| --- | --- | --- | --- | --- | --- |"]
    t = 0.0
    beats = sorted(getattr(sc, "beats", []), key=lambda x: x.get("after", len(sc.lines)))
    def beat_rows(upto):
        nonlocal t
        while beats and beats[0].get("after", 0) <= upto:
            b = beats.pop(0); bd = b.get("bound")
            if bd and bd["verdict"] in ("covered", "weak") and bd["spans"]:
                sp = bd["spans"][0]; d = sp["to"] - sp["from"]; t += d
                out.append(f"| ~ | {sp['take']} | {b['kind']}: {b['phrase'][:44]} | {sp['from']:.1f}-{sp['to']:.1f} | {d:.1f} | {t:.1f} |")
            elif "bound" not in b and b["kind"] != "end-card":
                # not searched yet (no footage read for this sheet): counted at the middle of a beat's 1.5-4 s
                t += NOMINAL_BEAT
                out.append(f"| ~ | (footage not searched) | {b['kind']}: {b['phrase'][:44]} | about {NOMINAL_BEAT} s | {NOMINAL_BEAT} | {t:.1f} |")
    for i, ln in enumerate(sc.lines):
        beat_rows(i)
        sp = find_span(tk, ln.text, ln.take) or find_span(tk, ln.text)
        if not sp:
            near = nearest(tk, ln.text)
            hint = f"; the take says: \"{near[3]}\" ({near[0]} {near[1]:.1f}-{near[2]:.1f} s)" if near else ""
            out.append(f"| {i + 1} | {ln.take} | \"{ln.text[:50]}\" | NOT SAID in any take (quote it word for word{hint}) | 0 | {t:.1f} |"); continue
        d = sp[2] - sp[1]; t += d
        out.append(f"| {i + 1} | {sp[0]} | \"{ln.text[:50]}\" | {sp[1]:.1f}-{sp[2]:.1f} | {d:.1f} | {t:.1f} |")
    beat_rows(len(sc.lines))
    picks = [f"- line {i + 1}: {ln.picked}" for i, ln in enumerate(sc.lines) if getattr(ln, "picked", None)]
    if picks:
        out.append("Alternatives (the read each line uses, and why):"); out += picks
    check = takes_to_check(sc, tk)
    if check:
        out.append("Reads to check (from the transcripts):"); out += check
    bad = unreadable(sc, notes_in(sc.text))
    if bad:
        out.append("Not in the notation (the compiler can't read these): " + ", ".join(sorted(set(bad))[:20]))
    if sc.secs:
        lo, hi = sc.secs
        verdict = "within" if lo <= t <= hi else ("OVER by %.1f s" % (t - hi) if t > hi else "UNDER by %.1f s" % (lo - t))
        out.append(f"Total {t:.1f} s; the request allows {lo:.1f}-{hi:.1f} s: {verdict}.")
        out += to_length(sc, tk, t, lo, hi)
    if getattr(sc, "beats", None):
        out.append("Open beats (bound to footage by the moments table, or not):")
        for b in sc.beats:
            head = f"- ~ {b['kind']}: {b['phrase']}" + (f" [find: {' '.join(f if isinstance(f, str) else f.get('picture') or '?' for f in b['find'])}]" if b["find"] else "")
            bd = b.get("bound")
            if not bd:
                out.append(head + ": not searched (the takes aren't ingested)"); continue
            sp = bd["spans"][0] if bd["spans"] else None
            got = f"{sp['take']} {sp['from']:.1f}-{sp['to']:.1f} s (score {sp['score']:.3f})" if sp else "nothing"
            verdict = {"covered": "COVERED by", "weak": "WEAK, best is", "missing": "MISSING, best is", "part": "A PART"}[bd["verdict"]]
            out.append(f"{head}: {verdict} {got if bd['verdict'] != 'part' else ''}".rstrip() + f" ({bd['why']})")
    if getattr(sc, "checks", None):
        out.append("Checks:")
        out += [f"- {c['status'].upper()}  ! {c['rule']}" + (f": {c['why']}" if c.get("why") else "") for c in checks(sc, tk, t)]
    return "\n".join(out)


def nearest(tk, text):
    """the stretch of a take most like a line no take says word for word: (take, start_s, end_s, its words), from the
    window of the line's length (give or take a third) sharing the most words with it in order; None under half"""
    from .front import norm
    q = norm(text)
    if len(q) < 2:
        return None
    best, score = None, 0.0
    for name, ws in tk.items():
        toks = [(i, t) for i, w in enumerate(ws) for t in norm(w[2])]
        for n in {len(q), max(2, round(len(q) * 0.67)), round(len(q) * 1.33)}:
            for s in range(0, max(1, len(toks) - n + 1)):
                win = [t for _, t in toks[s:s + n]]
                # words in common, in order (a longest common subsequence), over the longer of the two
                a = [0] * (len(win) + 1)
                for x in q:
                    prev = 0
                    for j, y in enumerate(win):
                        cur = a[j + 1]
                        a[j + 1] = prev + 1 if x == y else max(a[j + 1], a[j])
                        prev = cur
                sc_ = a[-1] / max(len(q), len(win))
                if sc_ > score:
                    i0, i1 = toks[s][0], toks[min(s + n, len(toks)) - 1][0]
                    best, score = (name, ws[i0][0], ws[i1][1], " ".join(w[2] for w in ws[i0:i1 + 1])), sc_
    return best if score >= 0.5 else None


def to_length(sc, tk, t, lo, hi):
    """what would bring the piece inside the length, in whole lines: the lines to drop when it's over (the fewest whose
    seconds land it inside), how much to add when it's under; never by changing a line's words"""
    if lo <= t <= hi:
        return []
    durs = []
    for i, ln in enumerate(sc.lines):
        sp = find_span(tk, ln.text, ln.take) or find_span(tk, ln.text)
        if sp:
            durs.append((sp[2] - sp[1], i + 1))
    if t > hi:
        need_lo, need_hi = t - hi, t - lo
        for k in (1, 2, 3):
            # the fewest lines (k) whose seconds together fall inside what has to go, the hook (line 1) kept
            from itertools import combinations
            fits = [c for c in combinations([d for d in durs if d[1] != 1], k) if need_lo <= sum(x[0] for x in c) <= need_hi]
            if fits:
                c = min(fits, key=lambda c: abs(sum(x[0] for x in c) - (need_lo + need_hi) / 2))
                return [f"To fit: drop line{'s' if k > 1 else ''} {', '.join(str(x[1]) for x in sorted(c, key=lambda x: x[1]))} "
                        f"({' + '.join(f'{x[0]:.1f}' for x in sorted(c, key=lambda x: x[1]))} s), or others adding up to {need_lo:.1f}-{need_hi:.1f} s. "
                        "Keep each line whole: a shorter line is a shorter stretch the take says in a row, never the same line with words taken out."]
        return [f"To fit: drop {need_lo:.1f}-{need_hi:.1f} s of whole lines (no one to three of them add up to that: drop more, or quote shorter stretches the take says in a row)."]
    return [f"To fit: add {lo - t:.1f}-{hi - t:.1f} s of whole lines, quoted word for word from the takes (an open beat counts about {NOMINAL_BEAT} s)."]


def checks(sc, tk, total=None):
    """a .brief's ! rules, each answered where the compiler can (what the kept lines say, the length) and otherwise left
    for the render (raea check, over render.json and cuts.json): [{rule, form, args, status: pass|fail|render, why}]"""
    from .front import norm
    if total is None:
        total = sum(sp[2] - sp[1] for ln in sc.lines for sp in [find_span(tk, ln.text, ln.take) or find_span(tk, ln.text)] if sp)
    out = []
    for c in sc.checks:
        r = dict(c, status="render", why="checked on the render")
        if c["form"] == "never-says":
            qs = [" ".join(norm(a)) for a in c["args"]]
            hit = [i + 1 for i, ln in enumerate(sc.lines) if any(q and f" {q} " in f" {' '.join(norm(ln.text))} " for q in qs)]
            r.update(status="fail" if hit else "pass", why=f"said in line {', '.join(map(str, hit))}" if hit else "")
        elif c["form"] == "must" and c["args"][0] == "the length is within the format" and sc.secs:
            lo, hi = sc.secs
            r.update(status="pass" if lo <= total <= hi else "fail", why=f"{total:.1f} s against {lo:.1f}-{hi:.1f} s")
        elif c["form"] == "claims":
            claims = getattr(sc, "claims", {}) or {}
            used = [(i + 1, r) for i, ln in enumerate(sc.lines) for r in (getattr(ln, "claims", None) or [])]
            bad = [f"line {i} claim:{r}" for i, r in used if not (claims.get(r) or {}).get("source")]
            r.update(status="fail" if bad else "pass", why=("no source for " + ", ".join(bad)) if bad else (f"{len(used)} claim(s), each with a source" if used else "no claims made"))
        elif c["form"] == "hook-by":
            first = sc.lines[0].role if sc.lines else None
            end = sum(sp[2] - sp[1] for ln in sc.lines if ln.role == first for sp in [find_span(tk, ln.text, ln.take) or find_span(tk, ln.text)] if sp)
            r.update(status="pass" if end <= float(c["args"][0]) else "fail", why=f"the first section ({first}) ends at about {end:.1f} s; the render confirms")
        elif c["form"] == "must" and c["args"][0] == "ends on a word, never mid-word":
            r["why"] = "cuts are at clean edit points by construction; cutcheck confirms on the render"
        out.append(r)
    return out
