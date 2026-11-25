//! What a render's text breaks, from its manifest alone: clipped, in the platform's UI zones, too brief to read, over
//! the face, off its cue, and the brand kit it was styled from. raea check's text rules, split out of raea-app-check so
//! `raea render --manifest` holds its own text to them as it finishes, a check's round sooner.
use serde_json::Value;

pub type R<T> = Result<T, String>;

use raea_core_facts::{Finding, Level};

/// One thing a rule found: its rule, its rank (P0 breaks the piece to P3 polish), when (seconds) and what to do.
#[derive(Clone, Debug)]
pub struct Issue {
    pub rule: &'static str,
    pub p: u8,
    pub at: Option<f64>,
    pub says: String,
}

impl Issue {
    /// As a Finding `by` a command (docs/direction.feature DR3: P0 and P1 errors, P2 a warn, P3 a note), about the
    /// frame its second falls on at `fps`
    pub fn finding(&self, by: &str, fps: f64) -> Finding {
        let f = Finding::new(self.rule, Level::of_rank(self.p), &self.says, by);
        match self.at {
            Some(t) => {
                let n = (t * fps).round() as i64;
                f.frames(n, n + 1)
            }
            None => f,
        }
    }
}

/// A text as it stays on screen: from the frame it comes on to the frame it leaves. Typing (a text growing into the
/// next) and counting (only its digits changing) are one run, judged by its last text.
#[derive(Clone)]
pub struct Run {
    pub text: String,
    pub from: f64,
    pub to: f64,
    pub box_: [f64; 4],
    /// where its letters are on every frame it shows its whole text (the manifest's `ink`, else its box: a centred line in
    /// a full-width box is nowhere near x = 0, #89): a text the camera carries (a sign on a panning street) moves
    /// through the frame's edges and zones, which only counts against it when no frame shows it clear
    pub boxes: Vec<[f64; 4]>,
    pub color: String,
    pub size: f64,
}

impl Run {
    /// The frame midway through those that show its whole text, and its box on that frame: `box_` (the last frame's)
    /// moved back by how far its letters travel from there to where they end. A text that moves was read at its last
    /// place on its middle frame (#236).
    pub fn middle(&self, fps: f64) -> (f64, [f64; 4]) {
        let n = self.boxes.len();
        let (Some(last), true) = (self.boxes.last(), fps > 0.0 && n > 1) else { return ((self.from + self.to) / 2.0, self.box_) };
        let (at, [x, y, w, h]) = (self.boxes[n / 2], self.box_);
        (self.to - (n - n / 2) as f64 / fps, [x + at[0] - last[0], y + at[1] - last[1], w, h])
    }
}

/// The brand kit the cut was styled from (brand/ at or above the manifest's or the video's folder, as render finds
/// brand.css): a kit `raea brand` read from a thin fetch that nobody corrected by hand is a placeholder, P1; a brand.css
/// with no brand.json beside it wasn't read from the brand at all, P2
pub fn brand_kit(near: &[&str], issues: &mut Vec<Issue>, passed: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    for start in near.iter().filter(|p| !p.is_empty()) {
        let abs = dunce::canonicalize(&std::path::Path::new(start)).unwrap_or_else(|_| std::path::PathBuf::from(start));
        let mut d = abs.parent();
        for _ in 0..5 {
            let Some(here) = d else { break };
            let dir = here.join("brand");
            if dir.join("brand.css").is_file() || dir.join("brand.json").is_file() {
                if !seen.insert(dir.clone()) {
                    return;
                }
                let kit: Option<Value> = std::fs::read_to_string(dir.join("brand.json")).ok().and_then(|t| serde_json::from_str(&t).ok());
                let Some(kit) = kit else {
                    issues.push(Issue { rule: "brand", p: 2, at: None, says: format!("{} wasn't read from the brand (no brand.json beside it): if the brief names a brand, run `raea brand DOMAIN` and style from its kit", dir.join("brand.css").display()) });
                    return;
                };
                let thin: Vec<&str> = kit["source"]["thin"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
                let by_hand = kit["source"]["by_hand"].as_object().is_some_and(|o| !o.is_empty());
                if !thin.is_empty() && !by_hand {
                    issues.push(Issue { rule: "brand", p: 1, at: None, says: format!("styled from a placeholder kit: raea brand's fetch of {} was thin ({}) and nothing in brand.json's source.by_hand says it was checked; read the live site in a browser and fix the kit (raea guide brand, \"When the fetch was thin\")", kit["domain"].as_str().unwrap_or("the site"), thin.join("; ")) });
                } else {
                    passed.push(format!("brand: the cut is styled from {}'s kit{}", kit["domain"].as_str().unwrap_or("the brand"), if by_hand { ", checked by hand" } else { "" }));
                }
                return;
            }
            d = here.parent();
        }
    }
}

pub fn text_runs(manifest: &Value) -> Vec<Run> {
    let fps = manifest["fps"].as_f64().unwrap_or(30.0);
    let frames = manifest["frames"].as_array().cloned().unwrap_or_default();
    let mut open: Vec<Run> = Vec::new();
    let mut done = Vec::new();
    let key = |s: &str| s.chars().filter(|c| !c.is_ascii_digit()).collect::<String>().trim().to_lowercase();
    for f in &frames {
        let t = f["frame"].as_f64().unwrap_or(0.0) / fps;
        let shown: Vec<&Value> = f["text"].as_array().map(|a| a.iter().filter(|x| x["opacity"].as_f64().unwrap_or(1.0) > 0.05 && !x["text"].as_str().unwrap_or("").trim().is_empty()).collect()).unwrap_or_default();
        let mut next = Vec::new();
        for x in shown {
            let text = x["text"].as_str().unwrap_or("").trim().to_string();
            let quad = |k: &str| x[k].as_array().map(|b| [0, 1, 2, 3].map(|i| b.get(i).and_then(Value::as_f64).unwrap_or(0.0)));
            let (b, ink) = (quad("box").unwrap_or([0.0; 4]), quad("ink"));
            let k = key(&text);
            let found = open.iter().position(|r| {
                let rk = key(&r.text);
                rk == k || (!rk.is_empty() && k.starts_with(&rk)) || (!k.is_empty() && rk.starts_with(&k) && k.len() * 2 > rk.len())
            });
            let mut r = match found {
                Some(i) => open.remove(i),
                None => Run { text: text.clone(), from: t, to: t, box_: b, boxes: Vec::new(), color: String::new(), size: 0.0 },
            };
            if text.len() > r.text.len() {
                r.boxes.clear();
            }
            if text.len() >= r.text.len() {
                r.text = text;
                r.box_ = b;
                r.boxes.push(ink.unwrap_or(b));
            }
            r.to = t + 1.0 / fps;
            r.color = x["color"].as_str().unwrap_or("rgb(255, 255, 255)").to_string();
            r.size = x["size"].as_f64().unwrap_or(0.0);
            next.push(r);
        }
        done.append(&mut open);
        open = next;
    }
    done.append(&mut open);
    done.sort_by(|a, b| a.from.total_cmp(&b.from));
    done
}

/// The share of the face's area a text may cover before it's a fault (@raea/video's proofBoxes keeps the same 8%)
pub const FACE_COVER: f64 = 0.08;

/// The face over time from a perception.json (`raea cut`: each clip's first frame, the face at its middle in the
/// edit's px): [from, to) in seconds and the face's box, for the stretches that have one
pub fn faces_of(perception: &Value, fps: f64) -> Vec<(f64, f64, [f64; 4])> {
    let mut keys: Vec<(f64, Option<[f64; 4]>)> = perception.as_object().into_iter().flatten().filter_map(|(k, v)| {
        let face = v["face"].as_object().map(|f| ["x", "y", "w", "h"].map(|n| f.get(n).and_then(Value::as_f64).unwrap_or(0.0)));
        Some((k.parse::<f64>().ok()? / fps, face))
    }).collect();
    keys.sort_by(|a, b| a.0.total_cmp(&b.0));
    (0..keys.len()).filter_map(|i| Some((keys[i].0, keys.get(i + 1).map_or(f64::INFINITY, |k| k.0), keys[i].1?))).collect()
}

/// Text over the face: a run that covers more than FACE_COVER of the face's area while that face is up, on every frame
/// that shows it whole (a text the camera carries counts only when no frame shows it clear)
pub fn face_cover(runs: &[Run], faces: &[(f64, f64, [f64; 4])], issues: &mut Vec<Issue>, passed: &mut Vec<String>) {
    let share = |b: &[f64; 4], f: &[f64; 4]| {
        let ox = (b[0] + b[2]).min(f[0] + f[2]) - b[0].max(f[0]);
        let oy = (b[1] + b[3]).min(f[1] + f[3]) - b[1].max(f[1]);
        if ox > 0.0 && oy > 0.0 { ox * oy / (f[2] * f[3]).max(1.0) } else { 0.0 }
    };
    let mut over = 0;
    for r in runs {
        let boxes: &[[f64; 4]] = if r.boxes.is_empty() { std::slice::from_ref(&r.box_) } else { &r.boxes };
        for &(from, _, f) in faces.iter().filter(|(from, to, _)| r.from < *to && r.to > *from) {
            let least = boxes.iter().map(|b| share(b, &f)).fold(f64::INFINITY, f64::min);
            if least > FACE_COVER {
                over += 1;
                issues.push(Issue { rule: "face", p: 1, at: Some(r.from.max(from)), says: format!("{} covers {:.0}% of the face (at {:.0},{:.0} {:.0}x{:.0}): put it in a <Region> (it keeps off the face) or make it smaller", quote(&r.text), (least * 100.0).min(100.0), f[0], f[1], f[2], f[3]) });
                break;
            }
        }
    }
    if over == 0 && !faces.is_empty() {
        passed.push(format!("face: no text covers more than {:.0}% of the face", FACE_COVER * 100.0));
    }
}

/// the zones where text can't go, as [top, right, bottom, left] at 1080 wide (sdk/guides/craft/floor.md)
pub fn zones(w: f64, h: f64, shape: Option<&str>, platform: Option<&str>) -> [f64; 4] {
    let r = w / h;
    let shape = shape.unwrap_or(if r < 0.7 { "9:16" } else if r < 0.9 { "4:5" } else if r < 1.2 { "1:1" } else { "16:9" });
    let z = match (shape, platform) {
        ("9:16", Some("tiktok")) => [108.0, 120.0, 320.0, 60.0],
        ("9:16", Some("reels")) => [210.0, 84.0, 310.0, 60.0],
        ("9:16", Some("shorts")) => [120.0, 96.0, 300.0, 60.0],
        ("9:16", _) => [220.0, 140.0, 420.0, 60.0],
        ("4:5", _) => [110.0, 60.0, 170.0, 60.0],
        ("1:1", _) => [70.0, 60.0, 120.0, 60.0],
        _ => return [120.0 * h / 1080.0, 60.0 * w / 1920.0, 120.0 * h / 1080.0, 60.0 * w / 1920.0],
    };
    let s = w / 1080.0;
    z.map(|v| v * s)
}

/// A caption the voice reads out: every word of the text is said while it's on screen (from a little before it comes
/// on). The viewer hears it as it shows, so it needs no reading time of its own (karaoke groups are up for as long as
/// they take to say).
pub fn spoken(r: &Run, words: &[Word]) -> bool {
    let tokens: Vec<String> = r.text.split_whitespace().map(norm).filter(|t| !t.is_empty()).collect();
    !tokens.is_empty() && tokens.iter().all(|t| words.iter().any(|w| norm(&w.text) == *t && w.start >= r.from - 0.3 && w.start <= r.to + 0.1))
}

pub fn text_rules(runs: &[Run], heard: Option<&[Word]>, w: f64, h: f64, shape: Option<&str>, platform: Option<&str>, issues: &mut Vec<Issue>, passed: &mut Vec<String>) {
    let [zt, zr, zb, zl] = zones(w, h, shape, platform);
    let (mut clipped, mut unsafe_, mut brief) = (0, 0, 0);
    for r in runs {
        let [x, y, bw, bh] = r.box_;
        let q = quote(&r.text);
        let off = |&[x, y, bw, bh]: &[f64; 4]| x < -1.0 || y < -1.0 || x + bw > w + 1.0 || y + bh > h + 1.0;
        let zoned = |&[x, y, bw, bh]: &[f64; 4]| y < zt || x + bw > w - zr || y + bh > h - zb || x < zl;
        if r.boxes.iter().all(off) {
            clipped += 1;
            issues.push(Issue { rule: "clipped", p: 0, at: Some(r.from), says: format!("{q} runs off the frame (box {x:.0},{y:.0} {bw:.0}x{bh:.0} in {w:.0}x{h:.0}); fit it or break the line") });
        } else if r.boxes.iter().all(zoned) {
            unsafe_ += 1;
            let [ix, iy, iw, ih] = *r.boxes.last().unwrap_or(&r.box_);
            let edge = if iy < zt { format!("the top (keep {zt:.0} px clear)") } else if iy + ih > h - zb { format!("the bottom (keep {zb:.0} px clear)") } else if ix < zl { format!("the left (keep {zl:.0} px clear)") } else { format!("the right (keep {zr:.0} px clear)") };
            issues.push(Issue { rule: "safe-area", p: 1, at: Some(r.from), says: format!("{q} sits in the app's UI zone at {edge}: its {} {ix:.0},{iy:.0} {iw:.0}x{ih:.0} in {w:.0}x{h:.0}", if r.boxes.last() == Some(&r.box_) { "box" } else { "letters" }) });
        }
        let words = r.text.split_whitespace().count() as f64;
        let need = (0.3 * words).max(1.0);
        let held = r.to - r.from;
        if held + 0.05 < need && r.size >= 24.0 && !heard.is_some_and(|ws| spoken(r, ws)) {
            brief += 1;
            issues.push(Issue { rule: "reading", p: 2, at: Some(r.from), says: format!("{q} is on for {held:.1} s; {words:.0} words need {need:.1} s to read") });
        }
    }
    if clipped == 0 {
        passed.push(format!("clipped: all {} texts fit the frame", runs.len()));
    }
    if unsafe_ == 0 {
        passed.push("safe areas: no text in the UI zones".into());
    }
    if brief == 0 {
        passed.push("reading: every text holds long enough".into());
    }
}

pub fn quote(s: &str) -> String {
    let s: String = s.chars().take(40).collect();
    format!("\u{201c}{s}\u{201d}")
}

/// A transcript's word: its text, start and end in seconds (its end is its start when the transcript doesn't say).
pub use raea_core_facts::Word;

/// A transcript's words (raea_core_facts::transcript_words: {text, start} in seconds, or t, start_ms, or {text, from}
/// in frames at `fps`), as the app's words.json and `raea render --words` have them.
pub fn transcript(path: &str, fps: f64) -> R<Vec<Word>> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| format!("{path}: {e}"))?;
    v.as_array().ok_or_else(|| format!("{path}: expected an array of words ({{text, start}} in seconds or {{text, from}} in frames)"))?;
    Ok(raea_core_facts::transcript_words(&v, fps))
}

pub use raea_core_facts::word_key as norm;

pub const SMALL: [&str; 24] = ["the", "and", "you", "your", "for", "with", "that", "this", "its", "it's", "are", "was", "our", "out", "from", "into", "then", "than", "but", "not", "all", "can", "has", "have"];

/// Cues land on their words: a text whose words the voice says nearby comes on with the first of them, not after it.
/// The text is found by its first word of weight, and then measured from the first word it shows: a caption "of the
/// spend" comes on with "of", when the voice says "of the" just before "spend".
pub fn cues(runs: &[Run], words: &[Word], issues: &mut Vec<Issue>, passed: &mut Vec<String>) {
    let (mut checked, mut off) = (0, 0);
    for r in runs {
        let shown: Vec<String> = r.text.split_whitespace().map(norm).filter(|t| !t.is_empty()).collect();
        let Some((k, i)) = shown.iter().enumerate().filter(|(_, t)| t.len() >= 3 && !SMALL.contains(&t.as_str())).find_map(|(k, t)| {
            // a one-word card matches only a word said right by it: the same word said 2 s earlier is another moment
            let near = if shown.len() == 1 { 0.75 } else { 2.0 };
            words.iter().enumerate().filter(|(_, w)| norm(&w.text) == *t && (w.start - r.from).abs() <= near).min_by(|(_, a), (_, b)| (a.start - r.from).abs().total_cmp(&(b.start - r.from).abs())).map(|(i, _)| (k, i))
        }) else {
            continue;
        };
        // back over the words the text shows before it, while the voice says them in the same order just before
        let back = (1..=k.min(i)).take_while(|&n| norm(&words[i - n].text) == shown[k - n]).last().unwrap_or(0);
        // a caption says what the voice says: at least half its words, in order, around the anchor. A card that only
        // shares one word with another line ("Wirestock x OpenAI Sora" against "At Wirestock, I worked on...") isn't
        // a caption of it, and holding it to that word's time was a false P1
        let fwd = (1..).take_while(|&n| k + n < shown.len() && i + n < words.len() && norm(&words[i + n].text) == shown[k + n]).last().unwrap_or(0);
        if (back + 1 + fwd) * 2 < shown.len() {
            continue;
        }
        let w = &words[i - back];
        let tok = norm(&w.text);
        checked += 1;
        let drift = r.from - w.start;
        if !(-0.25..=0.15).contains(&drift) {
            off += 1;
            let when = if drift > 0.0 { format!("{drift:.2} s after") } else { format!("{:.2} s before", -drift) };
            issues.push(Issue { rule: "cue", p: 1, at: Some(r.from), says: format!("{} comes on {when} the voice says \u{201c}{tok}\u{201d} ({:.2} s); time it from the transcript's word", quote(&r.text), w.start) });
        }
    }
    if checked > 0 && off == 0 {
        passed.push(format!("cues: {checked} texts land on their words"));
    }
}
