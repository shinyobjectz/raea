//! Editing breakdowns (docs/edit-score.feature): how a video was edited, written top to bottom in time in fixed
//! editor words. This module reads them, writes one from an edit's timeline, and checks one against a measured video.
//!
//!   edit "bedtime jewelry swap" 9:16 ~28s
//!   tracks V1 creator, V2 broll, T captions, A1 voice, A2 music
//!   beat hook 0:00-0:02.6 why relatable problem feel curiosity
//!     V1 talking-head close handheld eyes to camera
//!     cut jump @"sleep"
//!     T caption word-pop center "you can't sleep in these rings"
//!
//! Lines are pinned to spoken words where they can be (@"word", or @"word"@0:12.4 when the word repeats), else to a
//! shot change (@shot:3), an offset into the section (@+0.5) or a time (@0:03.2). "~" marks a guess, a trailing "?"
//! doubt. The checker turns the lines it can into measurements (a shot change near the word, text in that band of the
//! frame) and counts the rest as unchecked.

use serde::Serialize;
use serde_json::{json, Value};

pub const BEATS: &[&str] = &["hook", "setup", "problem", "agitate", "solution", "product", "demo", "proof", "objection", "offer", "urgency", "cta", "outro", "story", "gag", "other"];
pub const SHOTS: &[&str] = &["talking-head", "selfie", "interview", "hands-demo", "product", "broll", "screen", "text-card", "slideshow", "before-after", "reaction", "pov", "green-screen", "two-shot", "skit", "animation", "vfx"];
pub const TEXT_KINDS: &[&str] = &["caption", "title", "callout", "label", "list", "price", "cta", "sticker"];
pub const TEXT_ANIMS: &[&str] = &["word-pop", "karaoke", "line-fade", "typewriter", "highlight-sweep", "slide-up", "scale-pop", "static"];
pub const TEXT_POS: &[&str] = &["top", "upper-third", "center", "lower-third", "bottom", "follows-subject"];
pub const MUSIC: &[&str] = &["bed", "featured", "trending", "none"];
pub const SOUNDS: &[&str] = &["whoosh", "pop", "ding", "riser", "hit", "scratch", "click", "other"];
pub const EDITS: &[&str] = &["hard", "jump", "match", "j-cut", "l-cut", "smash"];
pub const TRANSITIONS: &[&str] = &["dissolve", "whip", "zoom", "slide", "flash", "glitch", "fade-black"];
pub const EFFECTS: &[&str] = &["punch-in", "zoom-out", "speed-ramp", "slow-mo", "freeze", "shake", "blur", "split-screen", "reframe", "color-pop"];
pub const MARKS: &[&str] = &["hook-line", "reveal", "product-first", "cta", "loop-point"];

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Anchor {
    /// a spoken word, and the time given with it when it repeats
    Word(String, Option<f64>),
    Shot(usize),
    /// seconds after the section starts
    After(f64),
    Time(f64),
    MusicBeat,
}

#[derive(Clone, Debug, Serialize)]
pub struct Line {
    pub lane: String,
    /// the words after the lane, quotes and anchors removed
    pub words: Vec<String>,
    pub quoted: Option<String>,
    pub anchor: Option<Anchor>,
    pub span: Option<(f64, f64)>,
    /// "every ~1.2s": a rhythm instead of one cut
    pub every: Option<f64>,
    pub unsure: bool,
    pub raw: String,
    /// why the line isn't in the notation, when it isn't
    pub problem: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Beat {
    pub kind: String,
    pub start: f64,
    pub end: f64,
    pub why: Option<String>,
    pub feel: Option<String>,
    pub lines: Vec<Line>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Breakdown {
    pub title: Option<String>,
    pub aspect: Option<String>,
    pub length: Option<f64>,
    pub tracks: Vec<String>,
    pub beats: Vec<Beat>,
    /// lines outside any section or not understood at all
    pub stray: Vec<String>,
}

/// "0:03.2", "3.2", "~0:24", "1:02:03" to seconds.
pub fn time(s: &str) -> Option<f64> {
    let s = s.trim().trim_start_matches('~').trim_end_matches('s');
    if s.is_empty() {
        return None;
    }
    let mut total = 0.0;
    for part in s.split(':') {
        total = total * 60.0 + part.parse::<f64>().ok()?;
    }
    total.is_finite().then_some(total)
}

fn span(s: &str) -> Option<(f64, f64)> {
    let s = s.replace('–', "-");
    let (a, b) = s.split_once('-')?;
    Some((time(a)?, time(b)?))
}

/// Splits a line into tokens, keeping "quoted text" (and @"quoted words") whole.
fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in s.chars() {
        match ch {
            '"' | '“' | '”' => {
                quoted = !quoted;
                cur.push('"');
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn anchor(tok: &str) -> Option<Anchor> {
    let t = tok.strip_prefix('@')?;
    if let Some(rest) = t.strip_prefix('"') {
        let (word, after) = rest.split_once('"').unwrap_or((rest, ""));
        let at = after.strip_prefix('@').and_then(time);
        return Some(Anchor::Word(word.trim().to_string(), at));
    }
    if let Some(n) = t.strip_prefix("shot:") {
        return n.parse().ok().map(Anchor::Shot);
    }
    if let Some(n) = t.strip_prefix('+') {
        return time(n).map(Anchor::After);
    }
    time(t).map(Anchor::Time)
}

fn check_vocab(lane: &str, words: &[String], l: &Line) -> Option<String> {
    let first = words.first().map(|s| s.as_str()).unwrap_or("");
    let need = |list: &[&str], what: &str| if list.contains(&first) { None } else { Some(format!("{lane}: {what} \"{first}\" isn't one of the notation's words")) };
    match lane {
        "V1" | "V2" => need(SHOTS, "shot"),
        "T" => need(TEXT_KINDS, "text kind").or_else(|| {
            let a = words.get(1).map(|s| s.as_str()).unwrap_or("");
            (!TEXT_ANIMS.contains(&a)).then(|| format!("T: animation \"{a}\" isn't one of the notation's words"))
        }),
        "A1" => (!["says", "vo", "silent"].contains(&first)).then(|| format!("A1: \"{first}\" isn't says, vo or silent")),
        "A2" => need(MUSIC, "music"),
        "A3" => need(SOUNDS, "sound"),
        "cut" => need(EDITS, "edit").or_else(|| (l.anchor.is_none() && l.every.is_none()).then(|| "cut: no anchor (@\"word\", @time) or every ~Ns".to_string())),
        "trans" => need(TRANSITIONS, "transition").or_else(|| l.anchor.is_none().then(|| "trans: no anchor".to_string())),
        "fx" => need(EFFECTS, "effect"),
        "mark" => need(MARKS, "mark").or_else(|| l.anchor.is_none().then(|| "mark: no anchor".to_string())),
        "note" => None,
        _ => Some(format!("\"{lane}\" isn't a lane")),
    }
}

pub fn parse(text: &str) -> Breakdown {
    let mut b = Breakdown::default();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let toks = tokens(line);
        let head = toks[0].as_str();
        match head {
            "edit" | "score" | "ad" => {
                b.title = toks.get(1).map(|t| t.trim_matches('"').to_string());
                b.aspect = toks.get(2).cloned();
                b.length = toks.get(3).and_then(|t| time(t));
            }
            "tracks" => b.tracks = line[6..].split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            "beat" => {
                let kind = toks.get(1).cloned().unwrap_or_default();
                let (start, end) = toks.get(2).and_then(|t| span(t)).unwrap_or((f64::NAN, f64::NAN));
                let text_after = |key: &str, stop: &str| {
                    let i = line.find(&format!(" {key} "))?;
                    let rest = &line[i + key.len() + 2..];
                    Some(rest.split(&format!(" {stop} ")).next().unwrap_or(rest).trim().to_string())
                };
                b.beats.push(Beat { kind, start, end, why: text_after("why", "feel"), feel: text_after("feel", "why"), lines: vec![] });
            }
            _ => {
                let unsure = line.ends_with('?');
                let mut words = Vec::new();
                let (mut quoted, mut anc, mut sp, mut every) = (None, None, None, None);
                let mut it = toks[1..].iter().peekable();
                while let Some(t) = it.next() {
                    let t = t.trim_end_matches('?');
                    if t.starts_with('@') {
                        anc = anc.or_else(|| anchor(t));
                    } else if t.starts_with('"') {
                        quoted = Some(t.trim_matches('"').to_string());
                    } else if t == "every" {
                        every = it.next().and_then(|n| time(n));
                    } else if t == "on" && it.peek().map(|n| n.starts_with("music-beat")).unwrap_or(false) {
                        it.next();
                        anc = anc.or(Some(Anchor::MusicBeat));
                    } else if sp.is_none() && t.contains('-') && span(t).is_some() && head == "V2" {
                        sp = span(t);
                    } else if !t.is_empty() {
                        words.push(t.to_string());
                    }
                }
                let mut l = Line { lane: head.to_string(), words, quoted, anchor: anc, span: sp, every, unsure, raw: line.to_string(), problem: None };
                l.problem = check_vocab(head, &l.words, &l);
                match b.beats.last_mut() {
                    Some(beat) => beat.lines.push(l),
                    None => b.stray.push(line.to_string()),
                }
            }
        }
    }
    b
}

impl Breakdown {
    pub fn lines(&self) -> impl Iterator<Item = (&Beat, &Line)> {
        self.beats.iter().flat_map(|b| b.lines.iter().map(move |l| (b, l)))
    }

    /// Lines in the notation, out of all lines (the "parses" rate).
    pub fn parse_rate(&self) -> (usize, usize) {
        let all = self.lines().count() + self.stray.len();
        let ok = self.lines().filter(|(_, l)| l.problem.is_none()).count();
        (ok, all)
    }
}

/// A spoken word with its time in the video or edit (seconds).
#[derive(Clone, Debug, serde::Deserialize, Serialize)]
pub struct Word {
    pub w: String,
    pub s: f64,
    pub e: f64,
}

fn norm(w: &str) -> String {
    w.to_lowercase().chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect()
}

/// When an anchor happens, if it can be found: a word is matched to the transcript (the occurrence nearest the time
/// given with it, else the first inside the section, else the first at all); a cut on a word lands at its start.
pub fn resolve(a: &Anchor, beat: &Beat, words: &[Word], shots: &[f64]) -> Option<f64> {
    resolve_after(a, beat, words, shots, f64::NEG_INFINITY)
}

/// As `resolve`, for lines read in order: a word without a time is the first occurrence from `after` on (the previous
/// cut), so "so" in the third cut isn't matched to the "so" before the first.
pub fn resolve_after(a: &Anchor, beat: &Beat, words: &[Word], shots: &[f64], after: f64) -> Option<f64> {
    match a {
        Anchor::Time(t) => Some(*t),
        Anchor::After(d) => beat.start.is_finite().then(|| beat.start + d),
        Anchor::Shot(n) => shots.get(n.checked_sub(1)?).copied(),
        Anchor::MusicBeat => None,
        Anchor::Word(w, at) => {
            let target: Vec<String> = w.split_whitespace().map(norm).filter(|x| !x.is_empty()).collect();
            let first = target.first()?;
            let hits: Vec<f64> = words.iter().enumerate().filter(|(i, x)| {
                &norm(&x.w) == first && target.iter().enumerate().all(|(k, t)| words.get(i + k).map(|y| &norm(&y.w) == t).unwrap_or(false))
            }).map(|(_, x)| x.s).collect();
            if let Some(at) = at {
                return hits.iter().copied().min_by(|a, b| (a - at).abs().partial_cmp(&(b - at).abs()).unwrap());
            }
            let from = after - 0.3;
            hits.iter().copied().find(|t| *t >= from && beat.start.is_finite() && *t >= beat.start - 0.5 && *t <= beat.end + 0.5)
                .or_else(|| hits.iter().copied().find(|t| *t >= from))
                .or_else(|| hits.iter().copied().find(|t| beat.start.is_finite() && *t >= beat.start - 0.5 && *t <= beat.end + 0.5))
                .or_else(|| hits.first().copied())
        }
    }
}

/// Text height bands of the frame, by position word (centre of the text, 0 = top).
fn band(pos: &str) -> Option<(f64, f64)> {
    Some(match pos {
        "top" => (0.0, 0.25),
        "upper-third" => (0.2, 0.42),
        "center" => (0.38, 0.64),
        "lower-third" => (0.6, 0.84),
        "bottom" => (0.8, 1.0),
        _ => return None,
    })
}

/// Checks a breakdown against a measured video or edit: each line passes, fails or is unchecked.
/// `tol` is how far a cut may sit from its anchor (0.15 s for an exact edit, 0.5 s for a perceived breakdown).
pub fn check(b: &Breakdown, card: &crate::measure::Card, words: &[Word], tol: f64) -> Value {
    // the sensitive detector's shot changes when the card has them (jump cuts included), else the fixed-threshold cuts
    let shots: Vec<f64> = if card.shot_changes.is_empty() { card.inner_cuts() } else {
        card.shot_changes.iter().copied().filter(|c| *c > 0.2 && *c < card.duration - 0.2).collect()
    };
    let mut lines = Vec::new();
    let (mut pass, mut fail, mut unchecked) = (0, 0, 0);
    let mut last_cut = f64::NEG_INFINITY;
    let mut claimed: Vec<f64> = Vec::new();
    for (beat, l) in b.lines() {
        let verdict: (Option<bool>, String) = if l.problem.is_some() {
            (Some(false), l.problem.clone().unwrap())
        } else {
            match l.lane.as_str() {
                "cut" | "trans" if l.every.is_some() => {
                    let n = shots.iter().filter(|t| **t >= beat.start - tol && **t <= beat.end + tol).count();
                    let want = (beat.end - beat.start) / l.every.unwrap();
                    let ok = beat.end.is_finite() && want > 0.0 && (n as f64 - want).abs() <= (want * 0.3).max(1.0);
                    (Some(ok), format!("{n} shot changes in the section, about {want:.0} expected"))
                }
                "cut" | "trans" => match l.anchor.as_ref().and_then(|a| resolve_after(a, beat, words, &shots, last_cut)) {
                    Some(t) => {
                        last_cut = t;
                        claimed.push(t);
                        let near = shots.iter().copied().min_by(|a, b| (a - t).abs().partial_cmp(&(b - t).abs()).unwrap());
                        match near {
                            Some(s) if (s - t).abs() <= tol => (Some(true), format!("shot change at {s:.2} s, {:.0} ms from {t:.2} s", (s - t).abs() * 1000.0)),
                            Some(s) => (Some(false), format!("nearest shot change {s:.2} s is {:.0} ms from {t:.2} s", (s - t).abs() * 1000.0)),
                            None => (Some(false), "no shot changes at all".into()),
                        }
                    }
                    None => (None, "anchor not found in the words".into()),
                },
                "T" => {
                    let pos = l.words.iter().find_map(|w| band(w));
                    // the seconds the line is about: from its anchor to the next text line's (at most 3 s), else its section
                    let (lo_t, hi_t) = match l.anchor.as_ref().and_then(|a| resolve(a, beat, words, &shots)) {
                        Some(t) => {
                            let next = beat.lines.iter().filter(|o| o.lane == "T").filter_map(|o| o.anchor.as_ref().and_then(|a| resolve(a, beat, words, &shots)))
                                .filter(|n| *n > t + 1e-6).fold(f64::INFINITY, f64::min);
                            (t, next.min(t + 3.0))
                        }
                        None => (beat.start - 0.5, beat.end + 0.5),
                    };
                    let secs: Vec<&crate::measure::Second> = card.seconds.iter().filter(|s| {
                        let t = s.t as f64;
                        lo_t.is_finite() && t >= lo_t - 0.01 && t < hi_t - 0.01
                    }).collect();
                    if card.seconds.is_empty() {
                        (None, "no text measured".into())
                    } else if !secs.iter().any(|s| s.text) {
                        (Some(false), "no on-screen text in the section".into())
                    } else if let Some((lo, hi)) = pos {
                        // each second's text sits where its lines' median is (the same measure the timeline converter uses)
                        let ys: Vec<f64> = secs.iter().filter_map(|s| crate::measure::median(&s.text_y)).collect();
                        let inside = ys.iter().filter(|y| **y >= lo && **y <= hi).count();
                        (Some(inside * 2 >= ys.len().max(1)), format!("{inside} of {} seconds with text in the {:.2}-{:.2} band", ys.len(), lo, hi))
                    } else {
                        (Some(true), "text on screen".into())
                    }
                }
                "V1" if ["talking-head", "selfie", "interview"].contains(&l.words[0].as_str()) && card.faces_known => {
                    let secs: Vec<&crate::measure::Second> = card.seconds.iter().filter(|s| beat.start.is_finite() && (s.t as f64 + 0.5) >= beat.start && (s.t as f64 + 0.5) <= beat.end).collect();
                    let faces = secs.iter().filter(|s| s.face > 0.0).count();
                    if secs.is_empty() { (None, "no seconds measured in the section".into()) } else {
                        (Some(faces * 10 >= secs.len() * 3), format!("a face in {faces} of {} seconds", secs.len()))
                    }
                }
                _ => (None, "not checkable yet".into()),
            }
        };
        match verdict.0 {
            Some(true) => pass += 1,
            Some(false) => fail += 1,
            None => unchecked += 1,
        }
        lines.push(json!({"beat": beat.kind, "line": l.raw, "ok": verdict.0, "why": verdict.1}));
    }
    // The cuts as a set: how many of the breakdown's cut lines match a measured shot change, and how many shot
    // changes a cut line accounts for (precision and recall, within tol).
    let hit = |t: f64, set: &[f64]| set.iter().any(|s| (s - t).abs() <= tol);
    let precision = if claimed.is_empty() { None } else { Some(claimed.iter().filter(|t| hit(**t, &shots)).count() as f64 / claimed.len() as f64) };
    let recall = if shots.is_empty() { None } else { Some(shots.iter().filter(|s| hit(**s, &claimed)).count() as f64 / shots.len() as f64) };
    let (ok, all) = b.parse_rate();
    json!({"lines": lines, "pass": pass, "fail": fail, "unchecked": unchecked, "parsed": ok, "total_lines": all,
           "cuts": {"claimed": claimed.len(), "measured": shots.len(), "precision": precision, "recall": recall, "tolerance": tol}})
}

/// Merges measured shot changes into a perceived breakdown (measure says where every cut is, the model says what kind
/// and why): each shot change becomes a cut line at its exact time. Where the breakdown had a cut within `tol`, its
/// edit type and spoken word are kept; a shot change it missed is added as an unsure hard cut, and a cut of its that no
/// shot change confirms stays, marked unsure (the detector misses some jump cuts). Everything else is unchanged.
pub fn merge(b: &Breakdown, card: &crate::measure::Card, words: &[Word], tol: f64) -> String {
    let shots: Vec<f64> = if card.shot_changes.is_empty() { card.inner_cuts() } else {
        card.shot_changes.iter().copied().filter(|c| *c > 0.2 && *c < card.duration - 0.2).collect()
    };
    let mut used = vec![false; shots.len()];
    let mut out = String::new();
    if let Some(t) = &b.title {
        out.push_str(&format!("edit \"{t}\" {} ~{}s\n", b.aspect.as_deref().unwrap_or("9:16"), b.length.map(|l| format!("{l:.0}")).unwrap_or_default()));
    }
    if !b.tracks.is_empty() {
        out.push_str(&format!("tracks {}\n", b.tracks.join(", ")));
    }
    let n = b.beats.len();
    let mut last_cut = f64::NEG_INFINITY;
    for (bi, beat) in b.beats.iter().enumerate() {
        let (lo, hi) = (if bi == 0 { f64::NEG_INFINITY } else { beat.start }, if bi + 1 == n { f64::INFINITY } else { beat.end });
        out.push_str(&format!("beat {} {}-{}{}{}\n", beat.kind, clock(beat.start.max(0.0)), clock(beat.end.max(0.0)),
            beat.why.as_ref().map(|w| format!(" why {w}")).unwrap_or_default(), beat.feel.as_ref().map(|f| format!(" feel {f}")).unwrap_or_default()));
        for l in &beat.lines {
            let is_cut = (l.lane == "cut" || l.lane == "trans") && l.every.is_none();
            if !is_cut {
                out.push_str(&format!("  {}\n", l.raw));
                continue;
            }
            let t = l.anchor.as_ref().and_then(|a| resolve_after(a, beat, words, &shots, last_cut));
            let kind = l.words.first().cloned().unwrap_or_else(|| "hard".into());
            let word = match &l.anchor { Some(Anchor::Word(w, _)) => Some(w.clone()), _ => None };
            let near = t.and_then(|t| shots.iter().enumerate().filter(|(i, s)| !used[*i] && (**s - t).abs() <= tol)
                .min_by(|a, b| (a.1 - t).abs().partial_cmp(&(b.1 - t).abs()).unwrap()).map(|(i, s)| (i, *s)));
            match near {
                Some((i, s)) => {
                    used[i] = true;
                    last_cut = s;
                    let anchor = word.map(|w| format!("@\"{w}\"@{}", clock(s))).unwrap_or_else(|| format!("@{}", clock(s)));
                    out.push_str(&format!("  {} {kind} {anchor}\n", l.lane));
                }
                None => {
                    if let Some(t) = t { last_cut = t; }
                    out.push_str(&format!("  {} ?\n", l.raw.trim_end_matches('?').trim_end()));
                }
            }
        }
        // the shot changes in this section the breakdown didn't name
        for (i, s) in shots.iter().enumerate() {
            if !used[i] && *s >= lo && *s < hi {
                used[i] = true;
                let w = words.iter().find(|w| w.s >= s - 0.12 && w.s <= s + 0.12).map(|w| w.w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'').to_string());
                match w {
                    Some(w) if !w.is_empty() => out.push_str(&format!("  cut hard @\"{w}\"@{} ?\n", clock(*s))),
                    _ => out.push_str(&format!("  cut hard @{} ?\n", clock(*s))),
                }
            }
        }
    }
    out
}

/// A breakdown written from an edit's timeline, exactly: visible cuts at their times (pinned to the word heard there
/// when the edit has speech), the main picture per stretch (a take's video is a talking head, anything else b-roll),
/// on-screen text by band, and the music bed.
pub fn from_timeline(title: &str, card: &crate::measure::Card, top: &[(f64, f64, bool)], words: &[Word], music_bed: bool) -> String {
    let aspect = match (card.w, card.h) {
        (Some(w), Some(h)) if h > w => "9:16".to_string(),
        (Some(w), Some(h)) if (w - h).abs() < 1.0 => "1:1".to_string(),
        _ => "16:9".to_string(),
    };
    let d = card.duration;
    let mut out = format!("edit \"{title}\" {aspect} ~{d:.1}s\ntracks V1 creator{}{}{}\n", if top.iter().any(|t| !t.2) { ", V2 broll" } else { "" },
        if card.seconds.iter().any(|s| s.text) { ", T text" } else { "" }, if music_bed { ", A2 music" } else { "" });
    out.push_str(&format!("beat other 0:00-{} why the whole edit feel -\n", clock(d)));
    let word_at = |t: f64| words.iter().find(|w| w.s >= t - 0.12 && w.s <= t + 0.12);
    let mut last_kind = None;
    for (s, _e, take) in top {
        let kind = if *take { "talking-head medium static" } else { "broll medium static" };
        if *s > 0.2 {
            let edit = if *take && last_kind == Some(true) { "jump" } else { "hard" };
            match word_at(*s) {
                Some(w) => out.push_str(&format!("  cut {edit} @\"{}\"@{}\n", w.w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\''), clock(*s))),
                None => out.push_str(&format!("  cut {edit} @{}\n", clock(*s))),
            }
        }
        if last_kind != Some(*take) {
            out.push_str(&format!("  V1 {kind}\n"));
        }
        last_kind = Some(*take);
    }
    // text: one line per run of seconds whose text sits in the same band
    let mut run: Option<(u32, &str)> = None;
    let bands = ["top", "upper-third", "center", "lower-third", "bottom"];
    let pos_of = |y: f64| if y < 0.22 { "top" } else if y < 0.42 { "upper-third" } else if y < 0.62 { "center" } else if y < 0.82 { "lower-third" } else { "bottom" };
    for s in &card.seconds {
        let p = if s.text { crate::measure::median(&s.text_y).map(pos_of) } else { None };
        match (run, p) {
            (Some((_, rp)), Some(np)) if rp == np => {}
            (Some((st, rp)), _) => {
                out.push_str(&format!("  T caption static {rp} @{}\n", clock(st as f64)));
                run = p.map(|np| (s.t, np));
            }
            (None, Some(np)) => run = Some((s.t, np)),
            (None, None) => {}
        }
    }
    if let Some((st, rp)) = run {
        out.push_str(&format!("  T caption static {rp} @{}\n", clock(st as f64)));
    }
    let _ = bands;
    if music_bed {
        out.push_str("  A2 bed under\n");
    }
    out
}

fn clock(t: f64) -> String {
    format!("{}:{:04.1}", (t / 60.0).floor() as u32, t % 60.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::{Card, Second};

    const SAMPLE: &str = r#"edit "jewelry swap" 9:16 ~28s
tracks V1 creator, T captions, A1 voice, A2 music
beat hook 0:00-0:02.6 why relatable problem feel curiosity
  V1 talking-head close handheld eyes to camera
  A1 says "you can't sleep in these rings"
  cut jump @"sleep"
  T caption word-pop center "you can't sleep in these rings"
  A2 bed lo-fi synth under
beat demo 0:02.6-0:10 why shows the swap feel satisfaction
  V1 hands-demo close static rings off one by one
  cut hard every ~1.2s on music-beat
  A3 pop @+0.2 ?
  V1 interpretive-dance wide static
"#;

    #[test]
    fn reads_the_notation() {
        let b = parse(SAMPLE);
        assert_eq!(b.title.as_deref(), Some("jewelry swap"));
        assert_eq!(b.length, Some(28.0));
        assert_eq!(b.beats.len(), 2);
        assert_eq!(b.beats[0].why.as_deref(), Some("relatable problem"));
        assert_eq!(b.beats[0].feel.as_deref(), Some("curiosity"));
        let cut = &b.beats[0].lines[2];
        assert_eq!(cut.anchor, Some(Anchor::Word("sleep".into(), None)));
        let rhythm = &b.beats[1].lines[1];
        assert_eq!(rhythm.every, Some(1.2));
        assert!(b.beats[1].lines[2].unsure);
        // "interpretive-dance" isn't a shot word
        assert_eq!(b.parse_rate(), (8, 9));
    }

    #[test]
    fn times_and_anchors() {
        assert_eq!(time("0:02.6"), Some(2.6));
        assert_eq!(time("~1:02"), Some(62.0));
        assert_eq!(anchor("@\"word here\"@0:12.4"), Some(Anchor::Word("word here".into(), Some(12.4))));
        assert_eq!(anchor("@shot:3"), Some(Anchor::Shot(3)));
        assert_eq!(anchor("@+0.5"), Some(Anchor::After(0.5)));
    }

    fn card(cuts: Vec<f64>) -> Card {
        Card { duration: 10.0, w: Some(1080.0), h: Some(1920.0), cuts, faces_known: true,
               seconds: (0..10).map(|t| Second { t, text: true, words: 4, text_y: vec![0.5], text_h: vec![0.04], face: 0.1 }).collect(), ..Default::default() }
    }

    fn words() -> Vec<Word> {
        ["you", "can't", "sleep", "in", "these", "rings"].iter().enumerate()
            .map(|(i, w)| Word { w: w.to_string(), s: 0.3 + i as f64 * 0.35, e: 0.6 + i as f64 * 0.35 }).collect()
    }

    #[test]
    fn a_cut_on_a_word_passes_only_near_that_word() {
        let b = parse(SAMPLE);
        // "sleep" starts at 1.0 s
        let ok = check(&b, &card(vec![1.02, 4.0, 5.2, 6.4, 7.6, 8.8]), &words(), 0.15);
        let cut = ok["lines"].as_array().unwrap().iter().find(|l| l["line"] == "cut jump @\"sleep\"").unwrap();
        assert_eq!(cut["ok"], true, "{cut}");
        // moved 300 ms: fails
        let moved = check(&b, &card(vec![1.32, 4.0, 5.2, 6.4, 7.6, 8.8]), &words(), 0.15);
        let cut = moved["lines"].as_array().unwrap().iter().find(|l| l["line"] == "cut jump @\"sleep\"").unwrap();
        assert_eq!(cut["ok"], false, "{cut}");
    }

    #[test]
    fn captions_in_the_wrong_band_fail() {
        let b = parse(SAMPLE);
        let mut top = card(vec![1.0]);
        for s in &mut top.seconds {
            s.text_y = vec![0.1];
        }
        let r = check(&b, &top, &words(), 0.15);
        let t = r["lines"].as_array().unwrap().iter().find(|l| l["line"].as_str().unwrap().starts_with("T caption")).unwrap();
        assert_eq!(t["ok"], false, "{t}");
    }

    #[test]
    fn merging_puts_every_shot_change_in() {
        let b = parse(SAMPLE);
        let mut c = card(vec![]);
        // "sleep" at 1.0 (the breakdown's jump cut, measured at 1.05) and two shot changes it never named
        c.shot_changes = vec![1.05, 4.0, 5.2];
        let m = merge(&b, &c, &words(), 0.5);
        assert!(m.contains("cut jump @\"sleep\"@0:01.1"), "{m}");
        assert!(m.contains("cut hard @0:04.0 ?"), "{m}");
        assert!(m.contains("cut hard @0:05.2 ?"), "{m}");
        let again = parse(&m);
        // every cut line is in the notation (the sample's deliberately bad V1 line passes through untouched)
        assert!(again.lines().filter(|(_, l)| l.lane == "cut").all(|(_, l)| l.problem.is_none()), "{m}");
        let r = check(&again, &c, &words(), 0.5);
        assert_eq!(r["cuts"]["recall"], 1.0);
    }

    #[test]
    fn a_timeline_breakdown_passes_its_own_check() {
        let c = card(vec![1.0, 4.0]);
        let top = vec![(0.0, 1.0, true), (1.0, 4.0, true), (4.0, 10.0, false)];
        let text = from_timeline("t", &c, &top, &words(), true);
        let b = parse(&text);
        assert_eq!(b.parse_rate().0, b.parse_rate().1, "{text}");
        let r = check(&b, &c, &words(), 0.15);
        assert_eq!(r["fail"], 0, "{text}\n{r}");
        assert_eq!(r["cuts"]["recall"], 1.0);
    }
}
