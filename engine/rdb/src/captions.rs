//! Word captions from a take's transcript, through the source map: every clip cut from the take shows the words spoken
//! inside its source window, each on the edit frames where it's said (held to the next word across short gaps, so the
//! captions don't flicker). One INSERT, applied as one move (undo takes it back). Agents wrote these by hand, word by
//! word, and ran out of output mid-snippet.

use std::path::Path;

use crate::eval::Tup;
use crate::ir::{Program, V};

fn num(t: &Tup, k: &str) -> Option<f64> {
    match t.vals.get(k) {
        Some(V::N(x)) => Some(*x),
        _ => None,
    }
}

fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The INSERT for the take's words (`per` words a caption at most; `highlight`: the colour of the word being said) over the edit's clips from it, and how many captions it makes.
pub fn caption_sql(prog: &Program, rows: &[Tup], dir: &Path, words_json: &Path, style: &str, per: usize, highlight: Option<&str>) -> Result<(String, usize), String> {
    let c = caption_rows(prog, rows, dir, words_json, style, per, highlight)?;
    let tuple = |v: &Vec<String>| format!("({})", v.join(", "));
    let n = c.captions.len();
    let mut sql = format!("INSERT INTO caption ({}) VALUES {}", c.names.join(", "), c.captions.iter().map(tuple).collect::<Vec<_>>().join(", "));
    if !c.runs.is_empty() {
        sql += &format!(";\nINSERT INTO caption ({}) VALUES {}", RUN_NAMES.join(", "), c.runs.iter().map(tuple).collect::<Vec<_>>().join(", "));
    }
    Ok((sql, n))
}

/// The columns of a highlighted caption's word runs.
pub const RUN_NAMES: [&str; 6] = ["id", "text", "parent", "color", "\"from\"", "\"to\""];

/// Captions as rows: column names (SQL-quoted where they need it), each caption's values and each word run's values,
/// all as SQL literals. caption_sql writes them as INSERTs.
pub struct CaptionRows {
    pub names: Vec<String>,
    pub captions: Vec<Vec<String>>,
    pub runs: Vec<Vec<String>>,
}

pub fn caption_rows(prog: &Program, rows: &[Tup], dir: &Path, words_json: &Path, style: &str, per: usize, highlight: Option<&str>) -> Result<CaptionRows, String> {
    // the take's video, audio or folder stand for its words.json (an agent wrote CAPTIONS FROM '.../video.mov')
    let words_json = &if words_json.is_dir() {
        words_json.join("words.json")
    } else if words_json.extension().is_some_and(|x| x != "json") {
        words_json.with_file_name("words.json")
    } else {
        words_json.to_path_buf()
    };
    let take = words_json.parent().ok_or("words.json has no folder")?.canonicalize().map_err(|e| format!("{}: {e}", words_json.display()))?;
    let words: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(words_json).map_err(|e| format!("{}: {e}", words_json.display()))?)
        .map_err(|e| format!("{}: not a take's words.json (a list of {{text, start_ms, end_ms}}): {e}", words_json.display()))?;
    let words: Vec<(String, f64, f64)> = words.iter().filter_map(|w| Some((w["text"].as_str()?.to_string(), w["start_ms"].as_f64()? / 1000.0, w["end_ms"].as_f64()? / 1000.0))).collect();
    let fps = prog.fps.max(1.0);
    let (wf, hf) = (prog.width as f64, prog.height as f64);
    // the style: sized for the frame, then the caller's overrides
    let mut cols: Vec<(String, String)> = vec![
        ("font".into(), q("Avenir Next")),
        ("weight".into(), "800".into()),
        ("size".into(), format!("{}", (84.0 * wf / 1080.0).round())),
        ("color".into(), q("#ffffff")),
        ("x".into(), format!("{}", (60.0 * wf / 1080.0).round())),
        ("y".into(), format!("{}", (0.70 * hf).round())),
        ("w".into(), format!("{}", (960.0 * wf / 1080.0).round())),
        ("align".into(), q("center")),
        ("stroke".into(), q("2px #000")),
    ];
    // k=v pairs split at the commas that start a new key (a value like rgba(0,0,0,0.5) keeps its own)
    let key = regex::Regex::new(r",\s*([A-Za-z_]+)\s*=").unwrap();
    let mut pairs = Vec::new();
    let mut at = 0;
    for m in key.find_iter(style) {
        pairs.push(&style[at..m.start()]);
        at = m.start() + 1;
    }
    pairs.push(&style[at..]);
    for kv in pairs.into_iter().map(str::trim).filter(|s| !s.is_empty()) {
        let (k, v) = kv.split_once('=').ok_or(format!("STYLE '{style}': key=value pairs, e.g. 'font=Inter,size=72,color=#ffffff'"))?;
        let (k, v) = (k.trim(), v.trim());
        let v = if v.parse::<f64>().is_ok() { v.to_string() } else { q(v) };
        match cols.iter_mut().find(|(c, _)| c == k) {
            Some(c) => c.1 = v,
            None => cols.push((k.to_string(), v)),
        }
    }
    // a words file kept elsewhere (assets/words-hook.json, an agent's trimmed or corrected copy) times the words
    // through the edit's take when its clips all come from one
    let take = {
        let dirs: std::collections::BTreeSet<std::path::PathBuf> = rows.iter().filter(|t| t.kind == "clip")
            .filter_map(|t| match t.vals.get("src") { Some(V::S(s)) => crate::engine::resolve_src(rows, s, dir).ok(), _ => None })
            .filter_map(|p| p.parent().and_then(|d| d.canonicalize().ok())).collect();
        if !dirs.contains(&take) && dirs.len() == 1 { dirs.into_iter().next().unwrap() } else { take }
    };
    let words_stem: Option<String> = words_json.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_suffix(".words.json")).map(String::from);
    let taken: std::collections::HashSet<String> = rows.iter().map(|t| t.id.clone()).collect();
    let mut next_id = 1usize;
    let mut values = Vec::new();
    let mut runs = Vec::new();
    let highlight = highlight.map(q);
    let highlight = highlight.as_deref();
    let mut clips: Vec<&Tup> = rows.iter().filter(|t| t.kind == "clip").collect();
    clips.sort_by(|a, b| num(a, "from").unwrap_or(0.0).total_cmp(&num(b, "from").unwrap_or(0.0)));
    for c in clips {
        let Some(V::S(src)) = c.vals.get("src") else { continue };
        let Ok(path) = crate::engine::resolve_src(rows, src, dir) else { continue };
        if path.parent().and_then(|p| p.canonicalize().ok()).as_deref() != Some(take.as_path()) {
            continue;
        }
        // a words file named for one source (take.words.json, as raea/void writes them) captions only clips cut from
        // that source, not the b-roll that shares its folder
        if let Some(stem) = words_stem.as_deref() {
            if path.file_stem().and_then(|s| s.to_str()).map(|s| s.split('.').next().unwrap_or(s)) != Some(stem) {
                continue;
            }
        }
        let (from, to, src_in) = (num(c, "from").unwrap_or(0.0), num(c, "to").unwrap_or(prog.frames as f64), num(c, "src_in").unwrap_or(0.0));
        let end_s = src_in + (to - from) / fps;
        // up to `per` words a caption, a sentence or clause ending closes one early; each keeps its words' times
        let mut inside: Vec<Vec<&(String, f64, f64)>> = Vec::new();
        for w in words.iter().filter(|w| w.1 >= src_in - 0.05 && w.1 < end_s) {
            match inside.last_mut() {
                Some(l) if l.len() < per && !l.last().unwrap().0.ends_with(['.', ',', '?', '!', ';', ':']) => l.push(w),
                _ => inside.push(vec![w]),
            }
        }
        let frame = |t: f64| (from + (t - src_in) * fps).round();
        let mut last = from;
        for (i, ws) in inside.iter().enumerate() {
            let (start, end) = (ws[0].1, ws.last().unwrap().2);
            let f0 = frame(start).max(last).clamp(from, to - 1.0);
            // held to the next caption when it follows within 0.3 s, so a caption never blinks off between words
            let until = match inside.get(i + 1) { Some(n) if n[0].1 - end < 0.3 => n[0].1, _ => end };
            let f1 = frame(until).clamp(f0 + 1.0, to);
            last = f1;
            let mut id = format!("cap{next_id}");
            while taken.contains(&id) {
                next_id += 1;
                id = format!("cap{next_id}");
            }
            next_id += 1;
            let text = ws.iter().map(|w| w.0.as_str()).collect::<Vec<_>>().join(" ");
            // highlighted: the caption holds its words as runs, each lit from when it's said to the next word
            let mut vals = vec![q(&id), q(if highlight.is_some() { "" } else { &text })];
            vals.extend(cols.iter().map(|(_, v)| v.clone()));
            vals.push(format!("{f0}"));
            vals.push(format!("{f1}"));
            values.push(vals);
            if let Some(lit) = highlight {
                let base = cols.iter().find(|(c, _)| c == "color").map(|c| c.1.clone()).unwrap_or(q("#ffffff"));
                let starts: Vec<f64> = ws.iter().map(|w| frame(w.1).clamp(f0, f1)).chain([f1]).collect();
                for (k, w) in ws.iter().enumerate() {
                    let word = if k + 1 < ws.len() { format!("{} ", w.0) } else { w.0.clone() };
                    let (a, b) = (if k == 0 { f0 } else { starts[k] }, starts[k + 1].max(starts[k]));
                    for (n, (p0, p1, c)) in [(f0, a, &base), (a, b, &lit.to_string()), (b, f1, &base)].into_iter().enumerate() {
                        if p1 > p0 {
                            runs.push(vec![q(&format!("{id}_{k}_{n}")), q(&word), q(&id), c.to_string(), format!("{p0}"), format!("{p1}")]);
                        }
                    }
                }
            }
        }
    }
    if values.is_empty() {
        let clips: Vec<String> = rows.iter().filter(|t| t.kind == "clip").map(|t| format!("{} ({})", t.id, match t.vals.get("src") { Some(V::S(s)) => s.as_str(), _ => "no src" })).collect();
        return Err(if clips.is_empty() {
            "the edit has no clips yet: insert them first (CAPTIONS times the words through the clips cut from the take)".into()
        } else {
            format!("none of the edit's clips ({}) is cut from {}: their media isn't in that folder", clips.join(", "), take.display())
        });
    }
    let names: Vec<String> = ["id".to_string(), "text".to_string()].into_iter().chain(cols.iter().map(|(c, _)| c.clone())).chain(["\"from\"".to_string(), "\"to\"".to_string()]).collect();
    Ok(CaptionRows { names, captions: values, runs })
}
