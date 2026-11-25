//! The plan as a script (docs/videoform.feature VF9c): NAME.script.ts, raea/brief's script, for the person to approve
//! before anything is edited when their project asks it (the brief's approve: ["script"]). A section is a scene, a
//! picture a beat: what's seen (the footage's file and seconds, or the brand kit's asset, and what's laid over it in
//! which place), how long, and the lines heard over it.

use crate::{Plan, Use};
use serde_json::{Value, json};

/// A footage file's name, of the files given
pub(crate) fn file(files: &[String], i: usize) -> String {
    files.get(i).map_or("?".into(), |f| f.rsplit('/').next().unwrap_or(f).to_string())
}

/// The slot number the plan's uses carry for its `k`th section
pub(crate) fn slot_of(p: &Plan, k: usize) -> usize {
    p.voice.iter().chain(&p.picture).find(|u| u.at >= p.slots[k].at - 1e-6).map_or(k, |u| u.slot)
}

/// Where a layer sits, as said
pub(crate) fn placed(b: [f64; 4]) -> String {
    format!("at x {:.2} y {:.2}, {:.2} x {:.2} of the frame", b[0], b[1], b[2], b[3])
}

fn seen(u: &Use, files: &[String]) -> String {
    let what = if u.asset.is_empty() { format!("{} {:.2}-{:.2} s", file(files, u.file), u.from, u.to) } else { u.asset.rsplit('/').next().unwrap_or(&u.asset).to_string() };
    let why = u.why.split(", showing ").next().unwrap_or(&u.why);
    let why = if why.is_empty() { String::new() } else { format!("{why}: ") };
    format!("{why}{what}")
}

/// The script's text: `name` its title, `about` its logline (the videoform, the length and the shape)
pub fn script_ts(p: &Plan, files: &[String], name: &str, about: &str) -> String {
    let mut scenes: Vec<Value> = Vec::new();
    for (k, s) in p.slots.iter().enumerate() {
        let slot = slot_of(p, k);
        let mut beats: Vec<Value> = Vec::new();
        let mut ends: Vec<f64> = Vec::new();
        for u in p.picture.iter().filter(|u| u.slot == slot) {
            let laid = !u.layer.is_empty();
            // a picture laid over goes in the beat of the one under it, which it shares the screen with
            if laid && let Some(b) = beats.last_mut() {
                let place = u.place.map_or(String::new(), |b| format!(" {}", placed(b)));
                let text = format!("{}; laid over as {}{place}: {}", b["onScreen"].as_str().unwrap_or_default(), u.layer, seen(u, files));
                b["onScreen"] = json!(text);
                continue;
            }
            let len = u.to - u.from;
            beats.push(json!({"onScreen": seen(u, files), "cut": if beats.is_empty() { "hard cut into the section" } else { "hard cut" }, "seconds": (len * 100.0).round() / 100.0, "lines": []}));
            ends.push(u.at + len);
        }
        // each line under the beat it starts in
        for v in p.voice.iter().filter(|v| v.slot == slot) {
            let i = ends.iter().position(|&e| v.at < e - 1e-6).unwrap_or(ends.len().saturating_sub(1));
            if let Some(b) = beats.get_mut(i) {
                b["lines"].as_array_mut().unwrap().push(json!({"who": "speaker", "says": p.pieces[v.piece].words.trim(), "how": format!("{} {:.2}-{:.2} s", file(files, v.file), v.from, v.to)}));
            }
        }
        for b in &mut beats {
            if b["lines"].as_array().is_some_and(|l| l.is_empty()) {
                b.as_object_mut().unwrap().remove("lines");
            }
        }
        if !beats.is_empty() {
            scenes.push(json!({"heading": s.name, "beats": beats}));
        }
    }
    let doc = json!({"title": name, "logline": about, "scenes": scenes});
    let lacks = if p.lacks.is_empty() { String::new() } else { format!("//\n// What the footage and the kit lack:\n{}\n", p.lacks.iter().map(|l| format!("//   {l}")).collect::<Vec<_>>().join("\n")) };
    format!(
        "// Planned by raea videoform follow (docs/videoform.feature VF9c): the cut, beat by beat, to approve before anything is\n// edited. Notes on it change this script; the edit is made from it once it's approved.\n{lacks}import {{ script }} from \"raea/brief\";\n\nexport default script({});\n",
        serde_json::to_string_pretty(&doc).unwrap()
    )
}

/// The plan, as the agent reads it before it cuts
pub fn lines(p: &Plan, files: &[String]) -> Vec<String> {
    let n = p.pieces.len();
    let kinded = p.pieces.iter().filter(|x| x.kind.is_some()).count();
    let mut out = vec![format!("The footage: {n} pieces in {} files, {kinded} of them a shot kind of the videoform's", files.len())];
    if p.lacks.is_empty() {
        out.push("It has every shot kind most references have.".into());
    } else {
        out.push("What it lacks (a shot list):".into());
        out.extend(p.lacks.iter().map(|l| format!("  {l}")));
    }
    let f = |u: &Use| file(files, u.file);
    if p.voice.is_empty() {
        out.push("No line in the footage carries the voice: write it, each section's for the length its picture runs.".into());
    }
    let length = p.slots.iter().map(|s| s.seconds).sum::<f64>();
    out.push(format!("The cut, {length:.0} s (the references' {:.0} s), section by section:", p.seconds));
    for (k, s) in p.slots.iter().enumerate() {
        out.push(format!("  {} at {:.1} s, {:.1} s (the references' {:.1}):", s.name, s.at, s.seconds, s.wanted));
        let slot = slot_of(p, k);
        for u in p.voice.iter().filter(|u| u.slot == slot) {
            out.push(format!("    said  {} {:.2}-{:.2}: \"{}\"", f(u), u.from, u.to, p.pieces[u.piece].words.trim()));
        }
        for u in p.picture.iter().filter(|u| u.slot == slot) {
            let laid = match (u.layer.as_str(), u.place) {
                ("", _) => String::new(),
                (l, Some(b)) => format!(", laid over as {l} {}", placed(b)),
                (l, None) => format!(", laid over as {l}"),
            };
            out.push(format!("    shown {:5.1} s  {} {:.2}-{:.2}  {}{}{laid}", u.at, f(u), u.from, u.to, u.why, if u.over { ", over the voice" } else { "" }));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Piece, Slot};

    #[test]
    fn a_plan_is_a_scene_a_section_and_a_beat_a_picture() {
        let piece = |words: &str| Piece { words: words.into(), ..Default::default() };
        let p = Plan {
            pieces: vec![piece("this oil saved my hair"), piece("")],
            slots: vec![Slot { name: "the hook".into(), at: 0.0, seconds: 3.0, wanted: 3.0 }],
            voice: vec![Use { piece: 0, file: 0, from: 1.0, to: 4.0, at: 0.0, ..Default::default() }],
            picture: vec![
                Use { piece: 0, file: 0, from: 1.0, to: 2.5, at: 0.0, why: "the speaker".into(), ..Default::default() },
                Use { piece: 1, file: 1, from: 0.0, to: 1.5, at: 1.5, why: "under the layer".into(), ..Default::default() },
                Use { piece: 1, file: 1, from: 0.0, to: 1.5, at: 1.5, layer: "card".into(), place: Some([0.1, 0.2, 0.5, 0.3]), asset: "/fp/kit/cody.png".into(), why: "a card, showing T's testimonial".into(), ..Default::default() },
            ],
            lacks: vec!["a close-up of the oil".into()],
            ..Default::default()
        };
        let ts = script_ts(&p, &["/raw/talk.mov".into(), "/raw/broll.mov".into()], "Thicc, 45 s", "kelly-ugc's form");
        assert!(ts.contains("//   a close-up of the oil") && ts.contains("import { script } from \"raea/brief\""));
        let body: Value = serde_json::from_str(ts.split("export default script(").nth(1).unwrap().trim_end().trim_end_matches(");")).unwrap();
        let beats = &body["scenes"][0]["beats"];
        assert_eq!(beats.as_array().unwrap().len(), 2, "the card shares its beat with the picture under it");
        assert_eq!(beats[0]["onScreen"], "the speaker: talk.mov 1.00-2.50 s");
        assert_eq!(beats[0]["lines"][0]["says"], "this oil saved my hair");
        assert_eq!(beats[1]["onScreen"], "under the layer: broll.mov 0.00-1.50 s; laid over as card at x 0.10 y 0.20, 0.50 x 0.30 of the frame: a card: cody.png");
    }
}
