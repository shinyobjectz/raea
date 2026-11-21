//! A reference read in one pass (docs/videoform.feature VF3): its readers (readers.rs) run side by side on the files
//! the pass holds, perception, words and beats each in its own process while the encoders embed here, and what they
//! found is put together shot by shot into the rows a videoform keeps (raea-sys-videoform rows.rs). Each reader still
//! decodes the held file itself; they run at once, so a reference takes about as long as its slowest reader.

use crate::readers::{self, Word};
use raea_sys_columns::{CLAP, CLAP_HOP, CLAP_WIN, Columns, Encoders, FPS, SIG, YAM_CLASSES, YAM_HOP};
use raea_sys_videoform::rows::{self, Rows, Shot};
use serde_json::{Value, json};
use std::path::Path;

/// Cuts nearer than this to the one before are the same cut (a dissolve's frames, a flash)
const SAME_CUT: f64 = 0.25;
/// What's heard, by YAMNet's AudioSet classes: speech (Speech, Child speech, Conversation, Narration) and music
/// (Music), each a shot's mean score; at HEARD or more it's there. Under QUIET dB it's quiet.
const SPEECH: [usize; 4] = [0, 1, 2, 3];
const MUSIC: [usize; 1] = [132];
const HEARD: f64 = 0.2;
const QUIET: f64 = -50.0;

/// A shot's mean score on `classes` (the highest of them each patch) from YAMNet's [patches][YAM_CLASSES]
fn heard_score(scores: &[f32], classes: &[usize], start: f64, end: f64) -> f64 {
    let patches = scores.chunks(YAM_CLASSES);
    // a patch is 0.96 s from its start, every YAM_HOP s; it's the shot's when its middle is
    let inside: Vec<f32> = patches.enumerate().filter(|(i, _)| (start..end).contains(&(*i as f64 * YAM_HOP + 0.48))).map(|(_, p)| classes.iter().map(|&c| p.get(c).copied().unwrap_or(0.0)).fold(0.0, f32::max)).collect();
    if inside.is_empty() { 0.0 } else { r2((inside.iter().sum::<f32>() / inside.len() as f32) as f64) }
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Shot bounds from cut times, where perception couldn't run: [start, end] each, from 0 to the end
pub fn shots_from_cuts(cuts: &[f64], dur: f64) -> Vec<[f64; 2]> {
    let mut at = vec![0.0];
    for &c in cuts {
        if c - at.last().unwrap() >= SAME_CUT && c < dur - SAME_CUT / 2.0 {
            at.push(c);
        }
    }
    at.iter().enumerate().map(|(i, &s)| [s, at.get(i + 1).copied().unwrap_or(dur)]).collect()
}

/// Shot lengths summed up: how many, how often they cut, the median and the spread
fn pace(shots: &[Shot], dur: f64) -> Value {
    let mut len: Vec<f64> = shots.iter().map(Shot::seconds).collect();
    len.sort_by(f64::total_cmp);
    let q = |p: f64| len.get(((len.len() as f64 - 1.0) * p).round() as usize).copied().unwrap_or(0.0);
    json!({
        "shots": shots.len(),
        "cuts_per_minute": r2(shots.len().saturating_sub(1) as f64 / dur.max(1e-3) * 60.0),
        "shot_seconds": {"median": r2(q(0.5)), "p10": r2(q(0.1)), "p90": r2(q(0.9)), "shortest": r2(q(0.0)), "longest": r2(q(1.0))},
    })
}

/// The voice's silence at a shot's start and end: from the edge to its first and last word (None: no words in it)
fn pauses(words: &[&Word], start: f64, end: f64) -> (Option<f64>, Option<f64>) {
    let first = words.iter().map(|w| w.start).fold(f64::INFINITY, f64::min);
    let last = words.iter().map(|w| w.end).fold(f64::NEG_INFINITY, f64::max);
    if words.is_empty() {
        return (None, None);
    }
    (Some(r2((first - start).max(0.0))), Some(r2((end - last).max(0.0))))
}

/// A shot's row from what every reader found
#[allow(clippy::too_many_arguments)]
fn shot(p: &Value, bounds: [f64; 2], text: &[Value], words: &[Word], beats: &[f64], cols: &Columns, classes: &[f32], first: bool) -> (Shot, Vec<f32>, Vec<f32>) {
    let [start, end] = bounds;
    let sig = rows::pool_frames(&cols.sig, SIG, FPS, start, end);
    let clap = rows::pool_windows(&cols.clap, CLAP, CLAP_WIN, CLAP_HOP, start, end);
    let level_db = r2(rows::level(&cols.level, 0.1, start, end));
    let (speech, music) = (heard_score(classes, &SPEECH, start, end), heard_score(classes, &MUSIC, start, end));
    let heard = match (level_db < QUIET, speech >= HEARD, music >= HEARD) {
        (true, ..) => "quiet",
        (_, true, true) => "speech and music",
        (_, true, false) => "speech",
        (_, false, true) => "music",
        _ => "other",
    };
    let said: Vec<&Word> = words.iter().filter(|w| (start..end).contains(&((w.start + w.end) / 2.0))).collect();
    let (pause_before, pause_after) = pauses(&said, start, end);
    let gap = beats.windows(2).map(|w| w[1] - w[0]).fold(f64::INFINITY, f64::min).max(0.3);
    let off_beat = beats.iter().map(|b| (b - start).abs()).fold(f64::INFINITY, f64::min);
    let on_screen = text.iter().filter(|t| t["start"].as_f64().unwrap_or(-1.0) < end && t["end"].as_f64().unwrap_or(-1.0) >= start);
    let s = |k: &str| p[k].as_str().unwrap_or("").to_string();
    let row = Shot {
        start: r2(start),
        end: r2(end),
        transition: if first { String::new() } else { s("transition") },
        framing: s("framing"),
        faces: p["faces"].as_u64().unwrap_or(0) as usize,
        motion: s("motion"),
        brightness: p["brightness"].as_f64().unwrap_or(0.0),
        look: p["look"].as_f64(),
        scene: p["scene"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default(),
        text: on_screen.filter_map(|t| Some((t["text"].as_str()?.to_string(), serde_json::from_value(t["box"].clone()).ok()?))).collect(),
        words: said.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "),
        words_per_minute: r2(said.len() as f64 / (end - start).max(1e-3) * 60.0),
        pause_before,
        pause_after,
        heard: heard.into(),
        speech,
        music,
        level_db,
        off_beat: (!first && off_beat <= gap).then_some(r2(off_beat)),
    };
    (row, sig, clap)
}

/// One reference read: its readers run at once on the held files (`work` is the pass's scratch folder, dropped with
/// it), its rows kept in the videoform's folder `keep`, and what's said of it returned
pub fn read(picture: &Path, sound: Option<&Path>, hash: &str, enc: &mut Encoders, work: &Path, keep: &Path) -> Result<Value, String> {
    let p = picture.to_string_lossy().into_owned();
    let probe = readers::raea(&["media", "probe", &p])?;
    let dur = probe["duration"].as_f64().filter(|d| *d > 0.0).ok_or_else(|| format!("{p}: no length raea can read"))?;
    let v = &probe["video"];
    if v.is_null() {
        return Err(format!("{p}: no picture raea can read"));
    }
    let heard_file = sound.unwrap_or(picture);
    let has_sound = sound.is_some() || !probe["audio"].is_null();
    // sound that doesn't last as long as the picture was decoded wrong (HE-AAC read at twice its rate lasted half as
    // long), and every reading of it with it: refused, not kept
    let heard = match sound {
        Some(s) => readers::raea(&["media", "probe", &s.to_string_lossy()])?["audio"]["duration"].as_f64(),
        None => probe["audio"]["duration"].as_f64(),
    };
    /// a second's difference is past any priming or padding
    const APART: f64 = 1.0;
    if let Some(h) = heard.filter(|h| (h - dur).abs() > APART) {
        return Err(format!("{p}: its sound lasts {h:.1} s and its picture {dur:.1} s, so raea can't read its sound right"));
    }
    std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
    let (facts, words, beats, loud, cols) = std::thread::scope(|s| {
        // the shots cut to cut, frame by frame, then perception on each
        let facts = s.spawn(|| readers::shots(picture, dur).map(|b| (readers::perception(picture, &work.join("facts"), &b), b)));
        // the words, then the beats (a few seconds each), from the sound
        let said = s.spawn(|| match has_sound {
            true => (readers::words(heard_file, work), readers::beats(heard_file)),
            false => (Ok(Vec::new()), Ok(Vec::new())),
        });
        let loud = s.spawn(|| if has_sound { readers::raea(&["media", "loudness", &heard_file.to_string_lossy()]).ok() } else { None });
        let cols = readers::columns(picture, sound, hash, dur, enc);
        let (words, beats) = said.join().unwrap_or_else(|_| (Err("stopped".into()), Err("stopped".into())));
        (facts.join(), words, beats, loud.join(), cols)
    });
    let cols = cols?;
    // a reader that couldn't run is said, and the rest go on without it
    let mut missing = Vec::new();
    let (facts, cut) = facts.unwrap_or_else(|_| Err("stopped".into()))?;
    let facts = facts.map_err(|e| missing.push(format!("perception: {e}"))).ok();
    let words = words.map_err(|e| missing.push(format!("words: {e}"))).unwrap_or_default();
    let beats = beats.unwrap_or_default();
    let loud = loud.ok().flatten();
    // the shots cut to cut, each with what perception saw in it (nothing, where it couldn't run)
    let per: Vec<Value> = match facts.as_ref().and_then(|f| f["shots"].as_array()).filter(|sh| sh.len() == cut.len()) {
        Some(sh) => sh.clone(),
        None => vec![Value::Null; cut.len()],
    };
    let bounds = cut;
    let text = facts.as_ref().and_then(|f| f["text"].as_array().cloned()).unwrap_or_default();
    let classes = if has_sound { enc.sound_classes(&raea_sys_columns::media::sound(heard_file, 16_000).map_err(|e| e.to_string())?).map_err(|e| format!("{e:#}"))? } else { Vec::new() };
    let mut kept = Rows { version: rows::VERSION, media: hash.into(), pins: enc.pins(), ..Default::default() };
    for (i, (b, pf)) in bounds.iter().zip(&per).enumerate() {
        let (row, sig, clap) = shot(pf, *b, &text, &words, &beats, &cols, &classes, i == 0);
        kept.shots.push(row);
        kept.sig.extend(sig);
        kept.clap.extend(clap);
    }
    kept.save(keep)?;
    // a still from each shot's middle, 160 px wide, for the contact sheets the person confirms rules on (VF8); the
    // list always has a comma, so one shot is a folder of one
    let mids: String = kept.shots.iter().map(|s| format!("{:.3},", (s.start + s.end) / 2.0)).collect();
    let stills = rows::stills(keep, hash).to_string_lossy().into_owned();
    if readers::raea(&["media", "still", &p, "--at", &mids, "--width", "160", "-o", &stills]).is_err() {
        missing.push("stills: raea media still couldn't cut them".into());
    }
    let shots = &kept.shots;
    let spoken: f64 = shots.iter().filter(|s| !s.words.is_empty()).map(Shot::seconds).sum();
    let share = |h: &str| r2(shots.iter().filter(|s| s.heard == h).map(Shot::seconds).sum::<f64>() / dur);
    let on_beat = raea_sys_videoform::ranges::on_beat(&shots);
    let mut framing = std::collections::BTreeMap::new();
    for s in shots {
        *framing.entry(s.framing.clone()).or_insert(0) += 1;
    }
    Ok(json!({
        "seconds": r2(dur),
        "picture": {"width": v["width"], "height": v["height"], "fps": v["fps"], "codec": v["codec"]},
        "pace": pace(shots, dur),
        "shots": shots.iter().map(|s| [s.start, s.end]).collect::<Vec<_>>(),
        "loudness": loud.map(|l| json!({"lufs": l["lufs"], "true_peak_db": l["true_peak_db"]})),
        "words": {"count": words.len(), "per_minute_spoken": r2(words.len() as f64 / spoken.max(1e-3) * 60.0)},
        "heard": {"speech": share("speech"), "music": share("music"), "speech and music": share("speech and music"), "quiet": share("quiet"), "other": share("other")},
        "beats": {"count": beats.len(), "cuts_on_beat": on_beat},
        "on_screen_text": text.len(),
        "framing": framing,
        "rows": {"version": rows::VERSION, "shots": shots.len()},
        "not_read": missing,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_cuts_are_one_and_shots_cover_the_video() {
        assert_eq!(shots_from_cuts(&[1.25, 1.292, 4.583, 8.917, 8.958, 9.99], 10.0), vec![[0.0, 1.25], [1.25, 4.583], [4.583, 8.917], [8.917, 10.0]]);
        assert_eq!(shots_from_cuts(&[], 30.0), vec![[0.0, 30.0]]);
    }

    #[test]
    fn a_shot_hears_the_patches_whose_middles_it_holds() {
        // three patches: speech, then music, then both
        let mut scores = vec![0f32; 3 * YAM_CLASSES];
        scores[0] = 0.9;
        scores[YAM_CLASSES + 132] = 0.8;
        scores[2 * YAM_CLASSES + 2] = 0.6;
        scores[2 * YAM_CLASSES + 132] = 0.4;
        assert_eq!(heard_score(&scores, &SPEECH, 0.0, 0.5), 0.9);
        assert_eq!(heard_score(&scores, &MUSIC, 0.5, 1.5), 0.6);
        assert_eq!(heard_score(&scores, &SPEECH, 5.0, 6.0), 0.0);
    }

    #[test]
    fn a_shots_pauses_are_its_edges_to_its_words() {
        let a = Word { text: "a".into(), start: 1.4, end: 1.6 };
        let b = Word { text: "b".into(), start: 2.0, end: 2.5 };
        assert_eq!(pauses(&[&a, &b], 1.0, 3.0), (Some(0.4), Some(0.5)));
        assert_eq!(pauses(&[], 1.0, 3.0), (None, None));
    }
}
