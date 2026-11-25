//! A composition read as a reference is read (docs/videoform.feature VF10, VF11), from its worlds alone, with no
//! render: its cuts (each run of frames showing the same footage from the same start), each cut's picture pooled from
//! its footage's columns at the seconds it shows, its words on screen from the world's text, its words said from the
//! footage's own reading, and its scale, people and camera from that reading too. The footage's columns are kept by
//! content hash, so a shot is embedded once, when its footage is first read, and never again however often it's
//! checked, moved or recut: only footage new to raea is read.

use raea_core_facts::{Clip, Facts, Layer, Text};
use raea_sys_columns::{CLAP, CLAP_HOP, CLAP_WIN, Columns, FPS, SIG};
use raea_sys_videoform::rows::{Rows, Shot, VERSION, pool_frames, pool_windows};

/// A cut of the composition: its frames [from, to), the footage it shows and from which second, the footage laid
/// over it (an app's screen in a phone, a picture in a box) and from which second, and its words on screen
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cut {
    pub from: i64,
    pub to: i64,
    pub source: Option<(String, f64)>,
    pub over: Option<(String, f64)>,
    pub text: Vec<(String, [f64; 4])>,
}

/// The footage a frame shows, bottom first: under everything else, then laid over it
fn footage(w: &Facts) -> Vec<Clip> {
    let mut clips: Vec<(u32, Clip)> = w.find(&["Clip", "Layer"]).into_iter().filter_map(|e| Some((w.get::<Layer>(e.id)?.order, w.get::<Clip>(e.id)?))).collect();
    clips.sort_by_key(|c| c.0);
    clips.into_iter().map(|c| c.1).collect()
}

/// The composition's cuts, from its worlds (every frame, in order): a new cut wherever the footage under everything
/// changes, or the same footage starts again from another second
pub fn cuts(worlds: &[Facts]) -> Vec<Cut> {
    let mut out: Vec<Cut> = Vec::new();
    let mut key: Option<(String, i64)> = None;
    for w in worlds {
        let clips = footage(w);
        let at = |c: &Clip| (c.source.clone(), c.time.unwrap_or(c.in_));
        let k = clips.first().map(|c| (c.source.clone(), (c.in_ * 1000.0).round() as i64));
        if out.is_empty() || k != key {
            out.push(Cut { from: w.frame, to: w.frame + 1, source: clips.first().map(at), ..Default::default() });
            key = k;
        }
        let cut = out.last_mut().unwrap();
        // what's laid over it: the first frame it's there (it may come in after the cut)
        if cut.over.is_none() {
            cut.over = clips.last().filter(|_| clips.len() > 1).map(at);
        }
        cut.to = w.frame + 1;
        for e in w.find(&["Text"]) {
            let Some(t) = w.get::<Text>(e.id) else { continue };
            let b = t.box_.unwrap_or([0.0, 0.0, 0.0, 0.0]);
            if !t.words.trim().is_empty() && cut.text.iter().all(|x| x.0 != t.words) {
                cut.text.push((t.words.clone(), [b[0], b[1], b[2] - b[0], b[3] - b[1]]));
            }
        }
    }
    out
}

/// The words a footage's reading says between two of its seconds: each piece's words spread over its length
fn said(f: &Rows, t0: f64, t1: f64) -> String {
    let mut out = Vec::new();
    for p in f.shots.iter().filter(|p| p.end > t0 && p.start < t1) {
        let ws: Vec<&str> = p.words.split_whitespace().collect();
        let step = p.seconds().max(1e-6) / ws.len().max(1) as f64;
        out.extend(ws.iter().enumerate().filter(|(i, _)| (t0..t1).contains(&(p.start + (*i as f64 + 0.5) * step))).map(|x| *x.1));
    }
    out.join(" ")
}

/// The cuts as rows, a cut a shot, from each footage's reading and columns (`read`: by the footage's path)
pub fn rows(cuts: &[Cut], fps: f64, read: &mut dyn FnMut(&str) -> Option<(Rows, Option<Columns>)>) -> Rows {
    let mut out = Rows { version: VERSION, media: "composition".into(), ..Default::default() };
    for c in cuts {
        let (start, end) = (c.from as f64 / fps, c.to as f64 / fps);
        let got = c.source.as_ref().and_then(|(s, t)| read(s).map(|r| (r, *t)));
        let mut shot = Shot { start, end, transition: "cut".into(), text: c.text.clone(), heard: "quiet".into(), ..Default::default() };
        let (mut sig, mut clap) = (vec![0f32; SIG], vec![0f32; CLAP]);
        if let Some(((f, cols), t0)) = got {
            let t1 = t0 + (end - start);
            let mid = (t0 + t1) / 2.0;
            // the footage's own reading where this cut shows it: its scale, people, camera and sound
            let piece = f.shots.iter().min_by(|a, b| dist(a, mid).total_cmp(&dist(b, mid)));
            if let Some(p) = piece {
                shot = Shot { framing: p.framing.clone(), faces: p.faces, motion: p.motion.clone(), brightness: p.brightness, look: p.look, scene: p.scene.clone(), heard: p.heard.clone(), speech: p.speech, music: p.music, level_db: p.level_db, ..shot };
            }
            shot.words = said(&f, t0, t1);
            shot.words_per_minute = shot.words.split_whitespace().count() as f64 * 60.0 / (end - start).max(1e-6);
            if let Some(c) = cols {
                sig = pool_frames(&c.sig, SIG, FPS, t0, t1);
                clap = pool_windows(&c.clap, CLAP, CLAP_WIN, CLAP_HOP, t0, t1);
            }
        }
        out.shots.push(shot);
        out.sig.extend(sig);
        out.clap.extend(clap);
    }
    out
}

/// How far a piece is from a second: 0 inside it
fn dist(p: &Shot, t: f64) -> f64 {
    if (p.start..p.end).contains(&t) { 0.0 } else { (p.start - t).abs().min((p.end - t).abs()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(start: f64, end: f64, words: &str) -> Shot {
        Shot { start, end, words: words.into(), framing: "medium".into(), ..Default::default() }
    }

    #[test]
    fn a_cut_takes_the_words_said_in_its_seconds() {
        let f = Rows { shots: vec![piece(0.0, 4.0, "one two three four"), piece(4.0, 6.0, "five six")], ..Default::default() };
        assert_eq!(said(&f, 1.0, 3.0), "two three");
        assert_eq!(said(&f, 3.0, 5.0), "four five");
    }

    #[test]
    fn a_cut_without_footage_is_a_quiet_shot_with_no_picture() {
        let cuts = [Cut { from: 0, to: 30, text: vec![("Hi".into(), [0.1, 0.1, 0.2, 0.1])], ..Default::default() }];
        let r = rows(&cuts, 30.0, &mut |_| None);
        assert_eq!(r.shots.len(), 1);
        assert_eq!(r.shots[0].text[0].0, "Hi");
        assert!(r.sig.iter().all(|x| *x == 0.0));
    }
}
