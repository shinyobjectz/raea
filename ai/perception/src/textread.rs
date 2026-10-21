//! Text read off a rendered video as rows (docs/the roadmap J1): every frame's lines from Apple Vision's accurate text
//! recognition, chained through frames into rows: a line continues a row when its text is the same (case and
//! punctuation aside) and its box overlaps the row's last one, with up to `GAP` frames missed between; each row keeps
//! its text, its first and last frame, and its median box. The rows read can then be set against the rows an edit
//! wrote (what J1 is graded on: our own renders, whose true rows are known).

use std::path::Path;

/// Frames a line may go unread inside a row before the row ends (a word Vision drops for a frame or two).
const GAP: usize = 3;

#[derive(Debug, Clone, serde::Serialize)]
pub struct TextRow {
    pub text: String,
    pub from: usize,
    /// the frame after the last one it was read in
    pub to: usize,
    /// median box, 0..1 of the frame from the top left: x, y, w, h
    pub r#box: [f64; 4],
    pub frames_read: usize,
    pub confidence: f32,
    /// the box in each frame it was read in (J3 reads motion from it)
    pub track: Vec<(usize, [f64; 4])>,
    /// lines of text joined into it (a caption that wraps); its box is that many lines high
    pub lines: usize,
    /// its box's left and top as keys (pixels of the frame), where it moves (J3)
    pub keys_x: Vec<crate::motion::Key>,
    pub keys_y: Vec<crate::motion::Key>,
    /// J5: its motion as a hypervector (hex), to match motions across edits (motion::similarity)
    pub motion: String,
}

pub fn norm(t: &str) -> String {
    t.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect::<String>().to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn iou(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let (x0, y0) = (a[0].max(b[0]), a[1].max(b[1]));
    let (x1, y1) = ((a[0] + a[2]).min(b[0] + b[2]), (a[1] + a[3]).min(b[1] + b[3]));
    let i = (x1 - x0).max(0.0) * (y1 - y0).max(0.0);
    let u = a[2] * a[3] + b[2] * b[3] - i;
    if u > 0.0 { i / u } else { 0.0 }
}

/// Lines per frame (frame index, text, confidence, box) into rows.
pub fn track(per_frame: &[Vec<(String, f32, [f64; 4])>]) -> Vec<TextRow> {
    struct Open {
        key: String,
        text: String,
        from: usize,
        last: usize,
        boxes: Vec<[f64; 4]>,
        frames: Vec<usize>,
        conf: Vec<f32>,
    }
    let mut open: Vec<Open> = Vec::new();
    let mut done: Vec<Open> = Vec::new();
    for (f, lines) in per_frame.iter().enumerate() {
        for (text, conf, b) in lines {
            let key = norm(text);
            if key.is_empty() {
                continue;
            }
            match open.iter_mut().find(|o| o.key == key && f - o.last <= GAP + 1 && o.last < f && iou(o.boxes.last().unwrap(), b) > 0.3) {
                Some(o) => {
                    o.last = f;
                    o.boxes.push(*b);
                    o.frames.push(f);
                    o.conf.push(*conf);
                }
                None => open.push(Open { key, text: text.clone(), from: f, last: f, boxes: vec![*b], frames: vec![f], conf: vec![*conf] }),
            }
        }
        let (keep, close): (Vec<Open>, Vec<Open>) = open.into_iter().partition(|o| f - o.last <= GAP);
        open = keep;
        done.extend(close);
    }
    done.extend(open);
    let med = |mut v: Vec<f64>| -> f64 {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let mut rows: Vec<TextRow> = done
        .into_iter()
        .map(|o| TextRow {
            text: o.text,
            from: o.from,
            to: o.last + 1,
            r#box: [0, 1, 2, 3].map(|k| med(o.boxes.iter().map(|b| b[k]).collect())),
            frames_read: o.boxes.len(),
            confidence: o.conf.iter().sum::<f32>() / o.conf.len() as f32,
            track: o.frames.iter().copied().zip(o.boxes.iter().copied()).collect(),
            lines: 1,
            keys_x: Vec::new(),
            keys_y: Vec::new(),
            motion: String::new(),
        })
        .collect();
    rows.sort_by_key(|r| (r.from, (r.r#box[1] * 1000.0) as i64));
    join_lines(rows)
}

/// Lines that come and go together (first and last frame within one of each other), centred over one another and
/// stacked (the next line's top within a line's height of the last one's bottom: the UGC captions' leading leaves
/// 0.019 of the frame between 0.028-high lines) are one text that wraps: joined, top to
/// bottom, into one row with the union of their boxes.
pub fn join_lines(rows: Vec<TextRow>) -> Vec<TextRow> {
    let mut out: Vec<TextRow> = Vec::new();
    for r in rows {
        let cx = r.r#box[0] + r.r#box[2] / 2.0;
        let fits = |o: &TextRow| {
            let (ocx, bottom) = (o.r#box[0] + o.r#box[2] / 2.0, o.r#box[1] + o.r#box[3]);
            o.from.abs_diff(r.from) <= 1 && o.to.abs_diff(r.to) <= 1 && (ocx - cx).abs() < 0.05
                && r.r#box[1] >= bottom - 0.5 * r.r#box[3] && r.r#box[1] - bottom < r.r#box[3]
        };
        match out.iter_mut().rev().find(|o| fits(o)) {
            Some(o) => {
                let (x0, y0) = (o.r#box[0].min(r.r#box[0]), o.r#box[1].min(r.r#box[1]));
                let (x1, y1) = ((o.r#box[0] + o.r#box[2]).max(r.r#box[0] + r.r#box[2]), (o.r#box[1] + o.r#box[3]).max(r.r#box[1] + r.r#box[3]));
                o.text = format!("{} {}", o.text, r.text);
                o.lines += r.lines;
                o.r#box = [x0, y0, x1 - x0, y1 - y0];
                o.frames_read = o.frames_read.min(r.frames_read);
                let theirs: std::collections::HashMap<usize, [f64; 4]> = r.track.iter().copied().collect();
                o.track = o.track.iter().filter_map(|(f, a)| { let b = theirs.get(f)?; let (x0, y0) = (a[0].min(b[0]), a[1].min(b[1]));
                    Some((*f, [x0, y0, (a[0] + a[2]).max(b[0] + b[2]) - x0, (a[1] + a[3]).max(b[1] + b[3]) - y0])) }).collect();
                o.confidence = o.confidence.min(r.confidence);
            }
            None => out.push(r),
        }
    }
    out
}

/// Every frame of a video read, then tracked: (fps, frames, rows).
#[cfg(target_vendor = "apple")]
pub fn read(path: &Path, backend: &str) -> Result<(f64, usize, Vec<TextRow>), String> {
    let mut d = raea_media::backend(backend)?.open_video(path)?;
    let info = d.info().clone();
    let n = (info.duration * info.fps).round() as usize;
    let mut per_frame = Vec::with_capacity(n);
    for f in 0..n {
        let fr = d.frame_at((f as f64 + 0.5) / info.fps)?;
        per_frame.push(crate::vision::text_lines(&fr.rgba, fr.width as usize, fr.height as usize)?);
    }
    let mut rows = track(&per_frame);
    // J3: each row's motion as keys, in pixels; a move of 1% of the frame or more
    let (w, h) = (info.width as f64, info.height as f64);
    for r in rows.iter_mut() {
        r.keys_x = crate::motion::keys(&r.track.iter().map(|(f, b)| (*f, b[0] * w)).collect::<Vec<_>>(), 0.01 * w);
        r.keys_y = crate::motion::keys(&r.track.iter().map(|(f, b)| (*f, b[1] * h)).collect::<Vec<_>>(), 0.01 * h);
        r.motion = crate::motion::motion_hv(&r.keys_x, &r.keys_y, w, h).iter().map(|w| format!("{w:016x}")).collect();
    }
    Ok((info.fps, n, rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_chain_into_rows_across_short_gaps() {
        let b = [0.1, 0.7, 0.8, 0.05];
        let moved = [0.5, 0.1, 0.3, 0.05];
        let mut frames: Vec<Vec<(String, f32, [f64; 4])>> = vec![vec![]; 20];
        for f in 0..8 {
            if f != 4 {
                frames[f].push(("All right".into(), 0.9, b));
            }
        }
        for f in 8..14 {
            frames[f].push(("so we're not".into(), 0.9, b));
        }
        // the same words somewhere else: another row
        frames[10].push(("SO WE'RE NOT!".into(), 0.8, moved));
        let rows = track(&frames);
        assert_eq!(rows.len(), 3, "{rows:?}");
        assert_eq!((rows[0].from, rows[0].to, rows[0].frames_read), (0, 8, 7)); // the one-frame drop bridged
        assert_eq!((rows[1].from, rows[1].to), (8, 14));
        assert_eq!(norm(&rows[2].text), "so were not");
    }

    #[test]
    fn a_wrapped_caption_is_one_row() {
        let row = |t: &str, y: f64| TextRow { text: t.into(), from: 23, to: 76, r#box: [0.27, y, 0.46, 0.04], frames_read: 53, confidence: 0.9, track: vec![], lines: 1, keys_x: vec![], keys_y: vec![], motion: String::new() };
        let rows = join_lines(vec![row("so we're not", 0.70), row("automating", 0.74), row("elsewhere", 0.2)]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].text, "so we're not automating");
        assert_eq!(rows[0].lines, 2);
    }
}

/// J4: the overlays read, as Gherkin (the canon form): what text appears, where, when, how it moves, for the agent and
/// previz to reason about when rows can't be rebuilt. Only rows J2's rule keeps (confidence 0.5+, read in 80% of their
/// span, 6+ frames); places in thirds of the frame, moves from J3's keys.
pub fn overview(name: &str, rows: &[TextRow], fps: f64, w: f64, h: f64) -> String {
    let keep = |r: &&TextRow| r.confidence >= 0.5 && r.frames_read as f64 / (r.to - r.from).max(1) as f64 >= 0.8 && r.to - r.from >= 6;
    let s = |f: usize| format!("{:.2} s", f as f64 / fps);
    let third = |v: f64, a: &str, b: &str, c: &str| if v < 1.0 / 3.0 { a.to_string() } else if v < 2.0 / 3.0 { b.to_string() } else { c.to_string() };
    let mut out = format!("Feature: The text over {name}, as read off its frames\n");
    // the same words ending one frame and continuing the next are one text that jumped (the tracker splits a row
    // whose box moves too far between frames)
    let kept: Vec<&TextRow> = rows.iter().filter(keep).collect();
    let mut chains: Vec<Vec<&TextRow>> = Vec::new();
    for r in kept {
        match chains.iter_mut().find(|c| { let l = c.last().unwrap(); norm(&l.text) == norm(&r.text) && r.from.abs_diff(l.to) <= 1 }) {
            Some(c) => c.push(r),
            None => chains.push(vec![r]),
        }
    }
    for chain in chains {
        let (first, last) = (chain[0], *chain.last().unwrap());
        let [x, y, bw, bh] = first.r#box;
        let (cx, cy) = (x + bw / 2.0, y + bh / 2.0);
        let across = if (cx - 0.5).abs() < 0.03 { "centred".to_string() } else { third(cx, "on the left", "in the middle", "on the right") };
        let down = third(cy, "the top", "the middle", "the bottom");
        let lines = if first.lines > 1 { format!(", in {} lines", first.lines) } else { String::new() };
        out += &format!("\n  Scenario: \"{}\" (frames {}-{})\n", first.text.replace('"', "'"), first.from, last.to);
        out += &format!("    Given text {:.0} px high{lines}, {across} in {down} third of the frame ({:.0}% down)\n", bh * h / first.lines.max(1) as f64, cy * 100.0);
        out += &format!("    When it appears at {}\n", s(first.from));
        let mut moves: Vec<String> = Vec::new();
        for (i, r) in chain.iter().enumerate() {
            if i > 0 {
                // the jump between two pieces: the last box of one against the first of the next
                let (a, b) = (chain[i - 1].track.last().map(|t| t.1).unwrap_or(chain[i - 1].r#box), r.track.first().map(|t| t.1).unwrap_or(r.r#box));
                let (dx, dy) = ((b[0] - a[0]) * w, (b[1] - a[1]) * h);
                let way = if dy.abs() >= dx.abs() { if dy < 0.0 { "up" } else { "down" } } else if dx < 0.0 { "left" } else { "right" };
                moves.push(format!("it jumps {:.0} px {way} at {}", dx.hypot(dy), s(r.from)));
            }
            for (k, axis) in r.keys_y.iter().map(|k| (k, "y")).chain(r.keys_x.iter().map(|k| (k, "x"))) {
                let d = k.end - k.start;
                let way = match (axis, d < 0.0) { ("y", true) => "rises", ("y", false) => "drops", (_, true) => "moves left", _ => "moves right" };
                let ease = match k.ease { "out" => "easing out", "in" => "easing in", "inout" => "easing in and out", _ => "at a steady speed" };
                moves.push(format!("it {way} {:.0} px from {} to {}, {ease}", d.abs(), s(k.from), s(k.to)));
            }
        }
        if moves.is_empty() {
            out += "    Then it holds still\n";
        } else {
            for (i, m) in moves.iter().enumerate() {
                out += &format!("    {} {m}\n", if i == 0 { "Then" } else { "And" });
            }
        }
        out += &format!("    And it's gone at {}\n", s(last.to));
    }
    out
}
