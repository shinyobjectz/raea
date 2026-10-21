//! The mix as a timeline (docs/the roadmap G3): editing-quality.feature's S1 and S2 (music ducked under the voice, loud
//! enough without clipping) measured second by second instead of once. From the same stems `hearing` reads: the
//! clips' sound (the voice) and the audio rows (the music), each measured before the master, and the master itself.

use std::path::Path;

use crate::eval::Tup;
use crate::ir::Program;

/// The house duck: music 23 dB under speech (editing-quality.feature S1). Under 18 dB the music crowds the voice.
const DUCK_DB: f64 = 23.0;
const CROWDED_DB: f64 = 18.0;
/// A frame has speech when the voice stem's momentary loudness is above this.
const SPEECH_LUFS: f64 = -40.0;

fn spans(frames: &[usize]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &f in frames {
        match out.last_mut() {
            Some(l) if f == l.1 + 1 => l.1 = f,
            _ => out.push((f, f)),
        }
    }
    out
}

/// The source's RMS level (dBFS) over [t0, t1) seconds, -120 where it has no sound there.
fn level(a: &raea_media::Audio, t0: f64, t1: f64) -> f64 {
    let r = a.rate.max(1) as f64;
    let (i0, i1) = ((t0.max(0.0) * r) as usize, (t1.max(0.0) * r) as usize);
    let (mut sum, mut n) = (0f64, 0usize);
    for c in &a.channels {
        for x in c.iter().take(i1.min(c.len())).skip(i0) {
            sum += (*x as f64) * (*x as f64);
            n += 1;
        }
    }
    if n == 0 { -120.0 } else { (10.0 * (sum / n as f64).max(1e-12).log10()).max(-120.0) }
}

/// Each sound's edges (the catalogue's mid-sound cut and dangling tail): at each lane's start and end inside the edit,
/// the source's level just inside and just outside the edge (20 ms each); a loud sound that carries on in the source
/// and is cut without a fade is cut mid-sound.
pub fn edges(prog: &Program, rows: &[Tup], dir: &Path) -> Result<Vec<serde_json::Value>, String> {
    let lanes = crate::lanes(prog, rows, dir)?;
    let secs = prog.frames as f64 / prog.fps;
    let (w, loud) = (0.02, -35.0);
    // which source a lane plays (the row's sound, else its src): a lane picked up by another of the same source at the
    // same source moment is continuous sound, not an edge
    let source_of = |key: &str| -> String {
        let id = key.split_once(':').map(|x| x.1).unwrap_or(key);
        rows.iter().find(|t| t.id == id).and_then(|t| ["sound", "src"].iter().find_map(|k| match t.vals.get(*k) { Some(crate::ir::V::S(x)) if !x.is_empty() => Some(x.clone()), _ => None })).unwrap_or_default()
    };
    let frame = 1.0 / prog.fps;
    let continued = |key: &str, l: &raea_media::audio::Lane| lanes.iter().any(|(k2, m)| k2 != key && source_of(k2) == source_of(key)
        && (m.at - (l.at + l.len)).abs() < frame && (m.from - (l.from + l.len)).abs() < frame);
    let continues_from = |key: &str, l: &raea_media::audio::Lane| lanes.iter().any(|(k2, m)| k2 != key && source_of(k2) == source_of(key)
        && (l.at - (m.at + m.len)).abs() < frame && (l.from - (m.from + m.len)).abs() < frame);
    let r1 = |x: f64| (x * 10.0).round() / 10.0;
    let mut out = Vec::new();
    for (key, l) in &lanes {
        let src_len = l.audio.len() as f64 / l.audio.rate.max(1) as f64;
        // the end: inside is the last 20 ms played, outside the next 20 ms of the source
        if l.at + l.len < secs - 0.05 && !continued(key, l) {
            let (inside, outside) = (level(&l.audio, l.from + l.len - w, l.from + l.len), level(&l.audio, l.from + l.len, l.from + l.len + w));
            let cut = inside > loud && outside > loud && l.from + l.len + w <= src_len && l.fade_out < 0.01;
            out.push(serde_json::json!({"lane": key, "edge": "end", "at": r1(l.at + l.len), "inside_db": r1(inside), "outside_db": r1(outside),
                "faded": l.fade_out >= 0.01, "cut_mid_sound": cut}));
        }
        if l.at > 0.05 && !continues_from(key, l) {
            let (inside, outside) = (level(&l.audio, l.from, l.from + w), level(&l.audio, l.from - w, l.from));
            let cut = inside > loud && outside > loud && l.from >= w && l.fade_in < 0.01;
            out.push(serde_json::json!({"lane": key, "edge": "start", "at": r1(l.at), "inside_db": r1(inside), "outside_db": r1(outside),
                "faded": l.fade_in >= 0.01, "cut_mid_sound": cut}));
        }
    }
    Ok(out)
}

/// The voice stem's level (dBFS) every 10 ms, and its median while talking: what "the voice is silent here" means.
pub fn voice_levels(prog: &Program, rows: &[Tup], dir: &Path) -> Result<(Vec<f64>, f64), String> {
    use raea_media::audio::mix;
    let lanes = crate::lanes(prog, rows, dir)?;
    let secs = prog.frames as f64 / prog.fps;
    // the voice: the clip lanes, and any audio row playing a source a clip names as its sound (a take's voice laid as
    // its own row)
    let sounds: std::collections::HashSet<String> = rows.iter().filter(|t| t.kind == "clip").filter_map(|t| match t.vals.get("sound") { Some(crate::ir::V::S(x)) if !x.is_empty() => Some(x.clone()), _ => None }).collect();
    let is_voice = |k: &str| k.starts_with("clip:") || (k.starts_with("audio:") && rows.iter().find(|t| t.kind == "audio" && Some(t.id.as_str()) == k.split_once(':').map(|x| x.1))
        .and_then(|t| match t.vals.get("src") { Some(crate::ir::V::S(x)) => Some(sounds.contains(x)), _ => None }).unwrap_or(false));
    let voice = mix(&lanes.iter().filter(|(k, _)| is_voice(k)).map(|(_, l)| l.clone()).collect::<Vec<_>>(), secs, 1);
    let lv: Vec<f64> = (0..(secs * 100.0) as usize).map(|i| level(&voice, i as f64 / 100.0, i as f64 / 100.0 + 0.01)).collect();
    let mut talk: Vec<f64> = lv.iter().copied().filter(|l| *l > -45.0).collect();
    talk.sort_by(f64::total_cmp);
    Ok((lv, talk.get(talk.len() / 2).copied().unwrap_or(-20.0)))
}

pub fn timeline(prog: &Program, rows: &[Tup], dir: &Path) -> Result<serde_json::Value, String> {
    timeline_at(prog, rows, dir, &[])
}

/// The mix timeline, with `cut_frames` (the authored cuts) checked for cuts mid-speech.
pub fn timeline_at(prog: &Program, rows: &[Tup], dir: &Path, cut_frames: &[usize]) -> Result<serde_json::Value, String> {
    use raea_media::audio::{lufs, mix, momentary};
    let lanes = crate::lanes(prog, rows, dir)?;
    if lanes.is_empty() {
        return Ok(serde_json::json!({"sound": false}));
    }
    let (fps, n) = (prog.fps, prog.frames as usize);
    let secs = n as f64 / fps;
    let stem = |kind: &str| mix(&lanes.iter().filter(|(k, _)| k.starts_with(kind)).map(|(_, l)| l.clone()).collect::<Vec<_>>(), secs, 1);
    let (voice, music) = (stem("clip:"), stem("audio:"));
    let master = crate::mastered(prog, rows, lanes.clone()).unwrap_or_default();
    let (mv, mm, ml) = (momentary(&voice, fps, n), momentary(&music, fps, n), momentary(&master, fps, n));
    let r1 = |x: f64| (x * 10.0).round() / 10.0;
    // each second: short-term loudness (the mean power of the last 3 s of momentary readings) and the sample peak
    let per_frame_power: Vec<f64> = ml.iter().map(|l| 10f64.powf((l + 0.691) / 10.0)).collect();
    let rate = master.rate.max(1) as usize;
    let mut seconds = Vec::new();
    for s in 0..secs.ceil() as usize {
        let end = ((s + 1) as f64 * fps).round() as usize;
        let start = end.saturating_sub((3.0 * fps) as usize);
        let w = &per_frame_power[start.min(n)..end.min(n)];
        let short = if w.is_empty() { -120.0 } else { (-0.691 + 10.0 * (w.iter().sum::<f64>() / w.len() as f64).max(1e-15).log10()).max(-120.0) };
        let peak = master.channels.iter().flat_map(|c| c.iter().skip(s * rate).take(rate)).fold(0f32, |m, x| m.max(x.abs()));
        seconds.push(serde_json::json!({"t": s, "short_term_lufs": r1(short), "peak_db": r1(20.0 * (peak as f64).max(1e-12).log10())}));
    }
    // while someone speaks: how far under the voice the music sits
    let speech: Vec<usize> = (0..n).filter(|&f| mv[f] > SPEECH_LUFS).collect();
    let with_music: Vec<usize> = speech.iter().copied().filter(|&f| mm[f] > -70.0).collect();
    let mut gaps: Vec<f64> = with_music.iter().map(|&f| mv[f] - mm[f]).collect();
    gaps.sort_by(f64::total_cmp);
    let crowded: Vec<usize> = with_music.iter().copied().filter(|&f| mv[f] - mm[f] < CROWDED_DB).collect();
    // cuts mid-speech: the voice talking on both sides of the cut, no pause in the 0.1 s around it (the cut frames are
    // where the visible clips change, as previz finds them)
    // a cut mid-speech: the voice stem's level in the 60 ms around the cut within 10 dB of its own median while
    // speaking (a pause between words or sentences sits well under it)
    let mut talk: Vec<f64> = (0..secs.floor() as usize * 10).map(|i| level(&voice, i as f64 / 10.0, i as f64 / 10.0 + 0.1)).filter(|l| *l > -45.0).collect();
    talk.sort_by(f64::total_cmp);
    let talk_median = talk.get(talk.len() / 2).copied().unwrap_or(-20.0);
    let mid_speech = |f: usize| { let t = f as f64 / fps; level(&voice, t - 0.03, t + 0.03) > talk_median - 10.0 };
    let sound_edges = edges(prog, rows, dir)?;
    let clipping: Vec<usize> = seconds.iter().filter(|s| s["peak_db"].as_f64().unwrap_or(-120.0) > -1.0).filter_map(|s| s["t"].as_u64().map(|t| t as usize)).collect();
    let baked = baked_music(prog, rows, dir, &lanes, &mv);
    Ok(serde_json::json!({
        "sound": true,
        "baked_music": baked,
        "integrated_lufs": r1(lufs(&master)),
        // true peak (EBU R128 / BS.1770): the master upsampled 4x (raea-media's band-limited resampler), then peaked
        "true_peak_dbtp": r1(raea_media::audio::peak_db(&raea_media::soft_resample(&master, master.rate * 4))), "voice_lufs": r1(lufs(&voice)), "music_lufs": if mm.iter().any(|v| *v > -70.0) { Some(r1(lufs(&music))) } else { None },
        "speech_frames": speech.len(),
        "music_under_voice_db": if gaps.is_empty() { None } else { Some(serde_json::json!({"median": r1(gaps[gaps.len() / 2]), "lowest": r1(gaps[0]), "house": DUCK_DB})) },
        "crowded_speech": {"frames": crowded.len(), "spans": spans(&crowded)},
        "seconds_over_minus_1_dbtp": clipping,
        // a cut that cuts the sound (an edge where the sound doesn't carry on) while the voice is mid-speech: a picture
        // cut over continuous sound (b-roll over the voice, a jump cut on contiguous audio) isn't one
        "mid_speech_at": cut_frames.iter().copied().filter(|&f| mid_speech(f) && sound_edges.iter().any(|e| (e["at"].as_f64().unwrap_or(-1.0) - f as f64 / fps).abs() <= 1.0 / fps + 0.05)).collect::<Vec<usize>>(),
        "picture_cuts_over_speech": cut_frames.iter().copied().filter(|&f| mid_speech(f)).count(),
        "sound_edges": sound_edges,
        "seconds": seconds,
    }))
}

/// Diegetic continuity at each authored cut (docs/the roadmap G's catalogue): does the outgoing shot's own sound carry or
/// stop as its picture does, and is the incoming shot's own sound there, in step with its picture? A shot's own sound
/// is its clip row's `sound` (else `src`, when the clip sounds); "in step" means a lane of that source plays the same
/// source moment the picture shows (within 1.5 frames). Per cut: the outgoing sound 0.2 s after the cut (carries: an
/// L-cut, or a voice under a shot with none of its own), the incoming sound 0.2 s before it (leads: a J-cut), and the
/// incoming sound 0.1 s in: in step, out of step (its source plays, another moment: the mouth says something else),
/// silent (a shot with sound of its own shown without it), or none of its own.
pub fn diegetic(prog: &Program, rows: &[Tup], dir: &Path, states: &[Vec<Tup>], cut_frames: &[usize]) -> Result<Vec<serde_json::Value>, String> {
    let lanes = crate::lanes(prog, rows, dir)?;
    let fps = prog.fps;
    let n = |t: &Tup, k: &str, d: f64| match t.vals.get(k) { Some(crate::ir::V::N(x)) => *x, _ => d };
    let s = |t: &Tup, k: &str| match t.vals.get(k) { Some(crate::ir::V::S(x)) if !x.is_empty() => Some(x.clone()), _ => None };
    let lane_source = |key: &str| -> Option<String> {
        let id = key.split_once(':').map(|x| x.1).unwrap_or(key);
        rows.iter().find(|t| t.id == id).and_then(|t| s(t, "sound").or_else(|| s(t, "src")))
    };
    // the top clip at frame f
    let top = |f: usize| -> Option<Tup> {
        states.get(f)?.iter().filter(|t| t.kind == "clip" && crate::previz::showing(t, f as i64))
            .max_by(|a, b| n(a, "z", 0.0).total_cmp(&n(b, "z", 0.0)).then(a.id.cmp(&b.id))).cloned()
    };
    // a clip's own sound: its `sound`, else its `src` when it has a lane (a clip that sounds)
    // (`sound = ''` mutes a clip that has sound of its own: its own is then the sound another clip of the same picture
    // source names, through the source map, else the picture's source)
    let own = |t: &Tup| {
        if matches!(t.vals.get("sound"), Some(crate::ir::V::S(x)) if x.is_empty()) {
            let src = s(t, "src");
            return rows.iter().filter(|r| r.kind == "clip" && s(r, "src") == src).find_map(|r| s(r, "sound")).or(src);
        }
        s(t, "sound").or_else(|| lanes.iter().any(|(k, _)| k == &format!("clip:{}", t.id)).then(|| s(t, "src")).flatten())
    };
    // the source moment the picture of clip t shows at edit time x (extrapolated past its span)
    let pic_at = |t: &Tup, x: f64| n(t, "src_in", 0.0) + (x - n(t, "from", 0.0) / fps);
    // what a source's lanes play at edit time x: Some(source moments) (empty: nothing playing)
    let playing = |src: &str, x: f64| -> Vec<f64> {
        lanes.iter().filter(|(k, l)| lane_source(k).as_deref() == Some(src) && x >= l.at && x < l.at + l.len).map(|(_, l)| l.from + (x - l.at)).collect()
    };
    let tol = 1.5 / fps;
    let mut out = Vec::new();
    for &c in cut_frames {
        let (Some(o), Some(i)) = (top(c.saturating_sub(1)), top(c)) else { continue };
        let t = c as f64 / fps;
        let carries = own(&o).map(|src| playing(&src, t + 0.2).iter().any(|m| (m - pic_at(&o, t + 0.2)).abs() < tol));
        let leads = own(&i).map(|src| playing(&src, t - 0.2).iter().any(|m| (m - pic_at(&i, t - 0.2)).abs() < tol));
        let incoming = match own(&i) {
            None => "none of its own",
            Some(src) => {
                let p = playing(&src, t + 0.1);
                if p.iter().any(|m| (m - pic_at(&i, t + 0.1)).abs() < tol) { "in step" } else if p.is_empty() { "silent" } else { "out of step" }
            }
        };
        let kind = match (carries == Some(true), leads == Some(true)) {
            (true, true) => "continuous",
            (true, false) => "L-cut",
            (false, true) => "J-cut",
            _ => "straight",
        };
        out.push(serde_json::json!({"frame": c, "out": o.id, "in": i.id, "kind": kind, "outgoing_carries": carries, "incoming_leads": leads,
            "incoming_sound": incoming, "mismatch": matches!(incoming, "out of step" | "silent")}));
    }
    Ok(out)
}

/// Cuts on beats or speech (docs/the roadmap G's catalogue): the music stem's beats (crate::beats, in edit time) and the
/// voice's pauses (150 ms or more 20 dB under its talking median) and the edges of the edit's speech rows, against
/// each authored cut. A cut is on the beat within 50 ms of one (chance: 0.1 s times the beats a second); on a phrase when
/// it falls in a pause or within 0.1 s of one, or of a speech row's edge (a fast talker's sentences can abut with only
/// an 11 dB dip, shallower than a word gap mid-sentence, so level alone can't find every phrase end).
pub fn cut_timing(prog: &Program, rows: &[Tup], dir: &Path, cut_frames: &[usize]) -> Result<serde_json::Value, String> {
    use raea_media::audio::mix;
    let lanes = crate::lanes(prog, rows, dir)?;
    let secs = prog.frames as f64 / prog.fps;
    // the music as it was written, its gain held flat: a duck at every speech edge would otherwise read as onsets at the
    // cuts and pull the beats onto them
    let music: Vec<_> = lanes.iter().filter(|(k, _)| k.starts_with("audio:")).map(|(_, l)| raea_media::audio::Lane { gain_db: vec![(0.0, 0.0)], ..l.clone() }).collect();
    let (mut beats, bpm, pulse) = if music.is_empty() { (Vec::new(), None, None) } else { crate::beats::beats_with_strength(&mix(&music, secs, 1)) };
    // Beat This!'s beats from the music sources' ingest (raea-perceive beats --store) win where every music row has
    // them: each beat in source time placed through its row (edit time = at + beat - from, inside the row's span)
    let mut from_model = !music.is_empty();
    let mut placed = Vec::new();
    for (k, l) in lanes.iter().filter(|(k, _)| k.starts_with("audio:")) {
        let id = k.split_once(':').map(|x| x.1).unwrap_or(k);
        let src = rows.iter().find(|t| t.kind == "audio" && t.id == id).and_then(|t| match t.vals.get("src") { Some(crate::ir::V::S(x)) => Some(x.clone()), _ => None });
        let j: Option<serde_json::Value> = src.as_deref().and_then(|s| crate::previz::ingest_dir(s.strip_prefix("media:").unwrap_or(s)))
            .and_then(|d| std::fs::read_to_string(d.join("beats.json")).ok()).and_then(|t| serde_json::from_str(&t).ok());
        match j.as_ref().and_then(|j| j["beats"].as_array()) {
            Some(bs) => placed.extend(bs.iter().filter_map(|b| b.as_f64()).map(|b| l.at + b - l.from).filter(|t| *t >= l.at && *t < l.at + l.len)),
            None => from_model = false,
        }
    }
    if from_model {
        placed.sort_by(f64::total_cmp);
        beats = placed;
    }
    let (voice, talk) = voice_levels(prog, rows, dir)?;
    // pauses: runs of 10 ms steps under the talking median - 20 dB, 150 ms or longer, as (start s, end s)
    let mut pauses = Vec::new();
    let mut start: Option<usize> = None;
    for (i, l) in voice.iter().chain(std::iter::once(&f64::MAX)).enumerate() {
        match (*l < talk - 20.0, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                if i - s >= 15 {
                    pauses.push((s as f64 / 100.0, i as f64 / 100.0));
                }
                start = None;
            }
            _ => {}
        }
    }
    // the edit's speech rows (perception rows from the transcript, in edit frames): their edges are phrase ends even
    // where the voice runs on without a pause
    let phrase_edges: Vec<f64> = rows.iter().filter(|t| t.kind == "speech").flat_map(|t| ["from", "to"].map(|k| t.vals.get(k).map(crate::ir::V::num)))
        .flatten().map(|f| f / prog.fps).collect();
    let r3 = |x: f64| (x * 1000.0).round() / 1000.0;
    let at_cuts: Vec<serde_json::Value> = cut_frames.iter().map(|&f| {
        let t = f as f64 / prog.fps;
        let near = beats.iter().map(|b| b - t).min_by(|a, b| a.abs().total_cmp(&b.abs()));
        let pause = pauses.iter().any(|(a, b)| t >= a - 0.1 && t <= b + 0.1);
        let edge = phrase_edges.iter().any(|e| (e - t).abs() <= 0.1);
        serde_json::json!({"frame": f, "to_beat_s": near.map(r3), "on_beat": near.is_some_and(|d| d.abs() <= 0.05), "on_pause": pause,
            "on_speech_edge": edge, "on_phrase": pause || edge})
    }).collect();
    let on_beat = at_cuts.iter().filter(|c| c["on_beat"] == true).count();
    let on_phrase = at_cuts.iter().filter(|c| c["on_phrase"] == true).count();
    let on_pause = at_cuts.iter().filter(|c| c["on_pause"] == true).count();
    Ok(serde_json::json!({"beats_from": if from_model { "beat_this" } else { "model-free" }, "bpm": bpm.map(|b| (b * 10.0).round() / 10.0), "pulse_strength": pulse.map(|p| (p * 10.0).round() / 10.0), "beats": beats.len(), "pauses": pauses.len(),
        "cuts": cut_frames.len(), "on_beat": on_beat, "on_beat_by_chance": (!beats.is_empty()).then(|| r3((0.1 * beats.len() as f64 / secs).min(1.0) * cut_frames.len() as f64)),
        "on_phrase": on_phrase, "on_pause": on_pause, "speech_rows": rows.iter().filter(|t| t.kind == "speech").count(), "at_cuts": at_cuts}))
}

/// Music baked into the clips' own sound (docs/the roadmap H3): where a clip lane's source has ingest stems (stems.json,
/// HTDemucs's levels 20 times a second), its music stem (drums + bass + other) against its vocals at the source moment
/// each frame plays, with the lane's gain; over the frames someone speaks (the voice stem's momentary loudness), how far
/// under the vocals the baked music sits, and the frames where it crowds them (under CROWDED_DB). None where no clip's
/// source has stems with music in them.
fn baked_music(prog: &Program, rows: &[Tup], _dir: &Path, lanes: &[(String, raea_media::audio::Lane)], mv: &[f64]) -> serde_json::Value {
    let fps = prog.fps;
    let n = mv.len();
    let (mut voc, mut mus) = (vec![0.0f64; n], vec![0.0f64; n]);
    let mut any = false;
    for (k, l) in lanes.iter().filter(|(k, _)| k.starts_with("clip:")) {
        let id = k.split_once(':').map(|x| x.1).unwrap_or(k);
        let Some(t) = rows.iter().find(|t| t.kind == "clip" && t.id == id) else { continue };
        let src = ["sound", "src"].iter().find_map(|c| match t.vals.get(*c) { Some(crate::ir::V::S(x)) if !x.is_empty() => Some(x.clone()), _ => None });
        let Some(j) = src.as_deref().and_then(|s| crate::previz::ingest_dir(s.strip_prefix("media:").unwrap_or(s)))
            .and_then(|d| std::fs::read_to_string(d.join("stems.json")).ok()).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { continue };
        let lv = |name: &str| -> Vec<f64> { j["levels_db"][name].as_array().map(|a| a.iter().filter_map(|v| v.as_f64()).collect()).unwrap_or_default() };
        let (v, d, b, o) = (lv("vocals"), lv("drums"), lv("bass"), lv("other"));
        let rate = j["levels_per_second"].as_f64().unwrap_or(20.0);
        // a source has music in it when its music stem's loud end (90th percentile) is over -45 dB: a clean voice's
        // stems leak about -54 there, the UGC render with its bed reads -29, the bed alone -22
        let mut m: Vec<f64> = (0..v.len()).filter_map(|i| Some(10.0 * (10f64.powf(d.get(i)? / 10.0) + 10f64.powf(b.get(i)? / 10.0) + 10f64.powf(o.get(i)? / 10.0)).log10())).collect();
        m.sort_by(f64::total_cmp);
        if m.get(m.len() * 9 / 10).is_none_or(|p| *p <= -45.0) {
            continue;
        }
        any = true;
        let gain_at = |x: f64| -> f64 {
            let g = &l.gain_db;
            match g.iter().position(|(t, _)| *t > x) { Some(0) => g[0].1, Some(i) => g[i - 1].1, None => g.last().map(|k| k.1).unwrap_or(0.0) }
        };
        for (f, (vf, mf)) in voc.iter_mut().zip(mus.iter_mut()).enumerate() {
            let x = f as f64 / fps;
            if x < l.at || x >= l.at + l.len {
                continue;
            }
            let i = ((l.from + x - l.at) * rate) as usize;
            let (Some(a), Some(p1), Some(p2), Some(p3)) = (v.get(i), d.get(i), b.get(i), o.get(i)) else { continue };
            let g = 10f64.powf(gain_at(x) / 10.0);
            *vf += 10f64.powf(a / 10.0) * g;
            *mf += (10f64.powf(p1 / 10.0) + 10f64.powf(p2 / 10.0) + 10f64.powf(p3 / 10.0)) * g;
        }
    }
    if !any {
        return serde_json::Value::Null;
    }
    let db = |p: f64| 10.0 * (p + 1e-15).log10();
    let speaking: Vec<usize> = (0..n).filter(|&f| mv[f] > SPEECH_LUFS && db(mus[f]) > -70.0).collect();
    let mut gaps: Vec<f64> = speaking.iter().map(|&f| db(voc[f]) - db(mus[f])).collect();
    gaps.sort_by(f64::total_cmp);
    let r1 = |x: f64| (x * 10.0).round() / 10.0;
    serde_json::json!({"speech_frames_with_music": speaking.len(), "under_voice_db_median": gaps.get(gaps.len() / 2).map(|g| r1(*g)),
        "crowded_frames": gaps.iter().filter(|g| **g < CROWDED_DB).count()})
}
