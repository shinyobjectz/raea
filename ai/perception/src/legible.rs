//! Legibility of a render's words, measured on its pixels: can each line be read (WCAG AA contrast against what's
//! actually behind it), and does it sit on the picture's focus (the subject the eye goes to without the words). The
//! engine paints each sampled frame twice, as rendered and with every glyph transparent (RAEA_MOTION_TEXT=hidden), and
//! render.json says where each line's box is, its size and weight. The pixels that differ between the two are the
//! glyphs; the second paint is what's behind them, and its saliency (and faces) is the focus.
//!
//! Contrast is the WCAG ratio between each glyph's core pixel (the strongest third of the difference, so
//! anti-aliased edges don't count) and the same pixel without the text, read at the worst tenth of the glyph: one
//! word on bright marble fails the line even if the rest sits on black. Shadows, outlines and plates are part of the
//! "without" picture only when they're not glyphs, so a plate behind a caption counts for it. AA: 4.5:1, or 3:1 for
//! large text (24 px, or 18.66 px bold, at a 1080 px short side).
//!
//! The focus is Vision's attention map of the picture without its words, plus its faces (a bust's face counts). A line
//! is on the focus when its glyphs' box covers 12% or more of the map's salient mass, or 8% of a face, in a picture that
//! has a subject. And the layout: a line whose box runs past the frame's edge, centred text drawn off its visible
//! centre (a 100%-wide box with side padding is wider than the frame), and text in a letterbox bar (rows black in every
//! sampled picture), which is outside the picture.

use crate::layout::Rect;
use serde_json::{Value, json};

/// The WCAG AA minimum for `size` px text of `weight` in a composition whose short side is `short` px.
pub fn needed(size: f64, weight: f64, short: f64) -> f64 {
    let s = size * 1080.0 / short.max(1.0);
    if s >= 24.0 || (s >= 18.66 && weight >= 700.0) { 3.0 } else { 4.5 }
}

/// Relative luminance (WCAG 2) of an sRGB colour.
pub fn luminance(c: [u8; 3]) -> f64 {
    let ch = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2])
}

pub fn ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// One line of text read off the pair of paints.
#[derive(Debug, Clone)]
pub struct Line {
    pub text: String,
    /// the glyphs' own box (where the two paints differ), 0..1 of the frame
    pub glyphs: Rect,
    /// contrast at the worst tenth of the glyph's core pixels, and the median
    pub worst: f64,
    pub median: f64,
    pub need: f64,
    /// share of the picture's salient mass, and of the most covered face, under the glyphs' box
    pub focus: f64,
    pub face: f64,
    /// share of the finished frame's salient mass the glyphs take (where the eye goes once the words are in)
    pub pull: f64,
    /// whether the picture without its words has a subject: its salient cells gathered in a quarter of the frame or
    /// less, and filled with something (`fill`): a title over black and scattered embers has none, and its words are
    /// meant to lead
    pub subject: bool,
    /// how far the line's layout box runs past the composition's edges, in composition px: [left, right, top, bottom]
    pub overflow: [f64; 4],
    /// for centred text, how far the glyphs' centre sits right (+) or left (-) of the visible box's centre, in
    /// composition px: a box wider than the frame (width 100% plus padding, content-box) centres off the frame
    pub off_centre: f64,
    /// the letterbox bar the glyphs are drawn in, if any: text in a cinema bar is outside the picture
    pub bar: Option<&'static str>,
}

/// The letterbox bars of a film, as rows black in every sampled picture without its words: (top, bottom), each 0..1 of
/// the height, 0 where there is none (under 2% of the height counts as none). `dark[y]` says whether row `y` was black
/// (no channel over 8) in every picture; `row_dark` reads one picture's rows.
pub fn bars(dark: &[bool]) -> (f64, f64) {
    let h = dark.len().max(1) as f64;
    let top = dark.iter().take_while(|d| **d).count() as f64 / h;
    let bottom = dark.iter().rev().take_while(|d| **d).count() as f64 / h;
    if top >= 0.999 {
        return (0.0, 0.0);
    }
    (if top >= 0.02 { top } else { 0.0 }, if bottom >= 0.02 { bottom } else { 0.0 })
}

pub fn row_dark(rgba: &[u8], w: usize, h: usize) -> Vec<bool> {
    (0..h).map(|y| rgba[y * w * 4..(y + 1) * w * 4].chunks_exact(4).all(|p| p[0] <= 8 && p[1] <= 8 && p[2] <= 8)).collect()
}

/// The lines of one frame. `with` and `without` are RGBA, `w` x `h`; `frame` is render.json's entry for it, `comp` the
/// composition's size; `attention` is the saliency map of `without` (rows from the top) and `seen` the finished
/// frame's, `faces` the faces in `without` (0..1); `bars` the film's letterbox (`bars`).
#[allow(clippy::too_many_arguments)]
pub fn lines(with: &[u8], without: &[u8], w: usize, h: usize, frame: &Value, comp: (f64, f64), attention: Option<&(Vec<f32>, usize, usize)>, seen: Option<&(Vec<f32>, usize, usize)>, faces: &[Rect], bars: (f64, f64)) -> Vec<Line> {
    let subject = !faces.is_empty() || attention.is_some_and(|a| spread(a) <= 0.25 && fill(without, w, h, a) >= 0.3);
    let (sx, sy) = (w as f64 / comp.0, h as f64 / comp.1);
    let mut out = Vec::new();
    for t in frame["text"].as_array().into_iter().flatten() {
        // only lines fully in: a fade's half-drawn frames aren't what the viewer reads
        if t["opacity"].as_f64().unwrap_or(1.0) < 0.98 {
            continue;
        }
        let Some(b) = t["box"].as_array() else { continue };
        let g = |i: usize| b.get(i).and_then(Value::as_f64).unwrap_or(0.0);
        let x0 = ((g(0) * sx).floor().max(0.0) as usize).min(w);
        let y0 = ((g(1) * sy).floor().max(0.0) as usize).min(h);
        let x1 = (((g(0) + g(2)) * sx).ceil().max(0.0) as usize).min(w);
        let y1 = (((g(1) + g(3)) * sy).ceil().max(0.0) as usize).min(h);
        // the difference between the paints, per pixel: the glyphs and their anti-aliasing
        let mut px: Vec<(usize, usize, u8)> = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                let o = (y * w + x) * 4;
                let d = (0..3).map(|c| with[o + c].abs_diff(without[o + c])).max().unwrap_or(0);
                if d > 24 {
                    px.push((x, y, d));
                }
            }
        }
        if px.len() < 12 {
            continue;
        }
        // the core: the strongest third of the difference
        let mut ds: Vec<u8> = px.iter().map(|p| p.2).collect();
        ds.sort_unstable();
        let cut = ds[ds.len() * 2 / 3];
        let mut rs: Vec<f64> = px.iter().filter(|p| p.2 >= cut).map(|&(x, y, _)| {
            let o = (y * w + x) * 4;
            ratio([with[o], with[o + 1], with[o + 2]], [without[o], without[o + 1], without[o + 2]])
        }).collect();
        rs.sort_by(|a, b| a.total_cmp(b));
        let worst = rs[rs.len() / 10];
        let median = rs[rs.len() / 2];
        let (gx0, gx1) = px.iter().fold((usize::MAX, 0), |a, p| (a.0.min(p.0), a.1.max(p.0)));
        let (gy0, gy1) = px.iter().fold((usize::MAX, 0), |a, p| (a.0.min(p.1), a.1.max(p.1)));
        let glyphs = Rect { x: gx0 as f64 / w as f64, y: gy0 as f64 / h as f64, w: (gx1 + 1 - gx0) as f64 / w as f64, h: (gy1 + 1 - gy0) as f64 / h as f64 };
        let focus = attention.map(|a| covered(a, &glyphs)).unwrap_or(0.0);
        let face = faces.iter().map(|f| overlap(f, &glyphs) / (f.w * f.h).max(1e-9)).fold(0.0, f64::max);
        let pull = seen.map(|a| covered(a, &glyphs)).unwrap_or(0.0);
        let overflow = [(-g(0)).max(0.0), (g(0) + g(2) - comp.0).max(0.0), (-g(1)).max(0.0), (g(1) + g(3) - comp.1).max(0.0)];
        let off_centre = if t["align"].as_str().is_some_and(|a| a == "center" || a == "-webkit-center") {
            let visible = (g(0).max(0.0) + (g(0) + g(2)).min(comp.0)) / 2.0;
            (glyphs.x + glyphs.w / 2.0) * comp.0 - visible
        } else {
            0.0
        };
        let mid = glyphs.y + glyphs.h / 2.0;
        let bar = if bars.0 > 0.0 && mid < bars.0 { Some("top") } else if bars.1 > 0.0 && mid > 1.0 - bars.1 { Some("bottom") } else { None };
        out.push(Line {
            text: t["text"].as_str().unwrap_or_default().to_string(),
            glyphs,
            worst,
            median,
            need: needed(t["size"].as_f64().unwrap_or(16.0), t["weight"].as_f64().unwrap_or(400.0), comp.0.min(comp.1)),
            focus,
            face,
            pull,
            subject,
            overflow,
            off_centre,
            bar,
        });
    }
    out
}

/// How much of the picture's salient area is something rather than background: the share of pixels under the
/// attention map's salient cells lit to a quarter of the picture's own highlights (its 99.9th percentile luminance) or
/// more. A bust in a dark room fills its salient area; a title over black and scattered embers, where Vision's
/// attention still finds a "focus", leaves it nearly empty. Relative, so a low-key grade isn't "empty".
pub fn fill(rgba: &[u8], w: usize, h: usize, a: &(Vec<f32>, usize, usize)) -> f64 {
    let lum: Vec<f64> = rgba.chunks_exact(4).map(|p| luminance([p[0], p[1], p[2]])).collect();
    let mut sorted = lum.clone();
    sorted.sort_by(|x, y| x.total_cmp(y));
    let hi = sorted[(sorted.len() * 999 / 1000).min(sorted.len().saturating_sub(1))];
    let (m, mw, mh) = (&a.0, a.1, a.2);
    let peak = m.iter().cloned().fold(0.0f32, f32::max);
    if peak <= 0.0 || hi <= 0.0 {
        return 0.0;
    }
    let (mut n, mut on) = (0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            if m[(y * mh / h) * mw + x * mw / w] < peak * 0.5 {
                continue;
            }
            n += 1;
            if lum[y * w + x] >= hi * 0.25 {
                on += 1;
            }
        }
    }
    on as f64 / n.max(1) as f64
}

/// The share of the map's cells that are salient (at half its peak or more).
pub fn spread(a: &(Vec<f32>, usize, usize)) -> f64 {
    let peak = a.0.iter().cloned().fold(0.0f32, f32::max);
    if peak <= 0.0 {
        return 1.0;
    }
    a.0.iter().filter(|v| **v >= peak * 0.5).count() as f64 / a.0.len().max(1) as f64
}

/// The share of the map's salient mass (cells at half its peak or more) that `r` covers.
pub fn covered(a: &(Vec<f32>, usize, usize), r: &Rect) -> f64 {
    let (m, mw, mh) = (&a.0, a.1, a.2);
    let peak = m.iter().cloned().fold(0.0f32, f32::max);
    if peak <= 0.0 {
        return 0.0;
    }
    let (mut all, mut under) = (0.0, 0.0);
    for y in 0..mh {
        for x in 0..mw {
            let v = m[y * mw + x];
            if v < peak * 0.5 {
                continue;
            }
            let cell = Rect { x: x as f64 / mw as f64, y: y as f64 / mh as f64, w: 1.0 / mw as f64, h: 1.0 / mh as f64 };
            all += v as f64;
            under += v as f64 * overlap(&cell, r) / (cell.w * cell.h);
        }
    }
    if all > 0.0 { under / all } else { 0.0 }
}

fn overlap(a: &Rect, b: &Rect) -> f64 {
    let ox = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
    let oy = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
    if ox > 0.0 && oy > 0.0 { ox * oy } else { 0.0 }
}

impl Line {
    /// What's wrong with this line, if anything.
    pub fn faults(&self) -> Vec<String> {
        let mut f = Vec::new();
        if let Some(b) = self.bar {
            f.push(format!("drawn in the {b} letterbox bar, outside the picture: set it inside the frame (on a plate if the picture is busy)"));
        }
        let sides = ["left", "right", "top", "bottom"];
        let over: Vec<String> = self.overflow.iter().zip(sides).filter(|(o, _)| **o > 2.0).map(|(o, s)| format!("{o:.0} px past the {s}")).collect();
        if !over.is_empty() {
            f.push(format!("its box runs {} edge of the frame (box-sizing: border-box, or a narrower box)", over.join(" and ")));
        }
        if self.off_centre.abs() > 24.0 {
            f.push(format!("centred text drawn {:.0} px {} of its visible box's centre", self.off_centre.abs(), if self.off_centre > 0.0 { "right" } else { "left" }));
        }
        if self.worst < self.need {
            f.push(format!("contrast {:.1}:1 at its worst tenth (median {:.1}:1), under AA's {}:1", self.worst, self.median, self.need));
        }
        if self.face >= 0.08 {
            f.push(format!("covers {:.0}% of a face", (self.face * 100.0).min(100.0)));
        } else if self.subject && self.focus >= 0.12 {
            f.push(format!("sits on the picture's focus ({:.0}% of where the eye goes without the words)", self.focus * 100.0));
        } else if self.subject && self.pull >= 0.6 {
            f.push(format!("takes the eye from the subject ({:.0}% of where it goes once the words are in)", self.pull * 100.0));
        }
        f
    }

    pub fn json(&self) -> Value {
        let r = |v: f64| (v * 1000.0).round() / 1000.0;
        json!({"text": self.text, "glyphs": [r(self.glyphs.x), r(self.glyphs.y), r(self.glyphs.w), r(self.glyphs.h)],
            "contrast": r(self.worst), "median": r(self.median), "need": self.need, "focus": r(self.focus), "face": r(self.face), "pull": r(self.pull), "subject": self.subject, "overflow": self.overflow, "off_centre": r(self.off_centre), "bar": self.bar, "faults": self.faults()})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wcag_ratios() {
        assert!((ratio([255, 255, 255], [0, 0, 0]) - 21.0).abs() < 1e-6);
        // the rome film's subtitle colour on black passes; its red on black doesn't
        assert!(ratio([0xa3, 0x97, 0x82], [0, 0, 0]) > 4.5);
        assert!(ratio([0xa8, 0x32, 0x3a], [0, 0, 0]) < 4.5);
        assert_eq!(needed(38.0, 400.0, 1080.0), 3.0);
        assert_eq!(needed(20.0, 700.0, 1080.0), 3.0);
        assert_eq!(needed(20.0, 400.0, 1080.0), 4.5);
        // a 9:16 composition's 16 px is 24 px at a 1080 short side
        assert_eq!(needed(16.0, 400.0, 720.0), 3.0);
    }

    #[test]
    fn grey_on_grey_fails_and_white_on_black_passes() {
        let (w, h) = (40, 20);
        let paint = |bg: u8, fg: Option<u8>| -> Vec<u8> {
            let mut v = vec![0u8; w * h * 4];
            for y in 0..h {
                for x in 0..w {
                    let o = (y * w + x) * 4;
                    let c = if (8..32).contains(&x) && (6..14).contains(&y) && x % 2 == 0 { fg.unwrap_or(bg) } else { bg };
                    v[o..o + 4].copy_from_slice(&[c, c, c, 255]);
                }
            }
            v
        };
        let frame = json!({"text": [{"text": "hi", "box": [0, 0, 40, 20], "opacity": 1.0, "size": 16, "weight": 400}]});
        let bad = lines(&paint(150, Some(200)), &paint(150, None), w, h, &frame, (40.0, 20.0), None, None, &[], (0.0, 0.0));
        assert_eq!(bad.len(), 1);
        assert!(!bad[0].faults().is_empty());
        let good = lines(&paint(0, Some(255)), &paint(0, None), w, h, &frame, (40.0, 20.0), None, None, &[], (0.0, 0.0));
        assert!(good[0].faults().is_empty());
    }

    #[test]
    fn a_frame_wide_box_with_side_padding_centres_off_the_frame() {
        // width 100% plus 160 px of padding a side, content-box: the box is 320 px wider than the frame, from its left
        // edge, so centred words land 160 px right of the frame's centre
        let (w, h) = (960, 40);
        let blank = vec![0u8; w * h * 4];
        let mut with = blank.clone();
        for y in 15..25 {
            for x in (560 - 100)..(560 + 100) {
                if x % 3 == 0 {
                    let o = (y * w + x) * 4;
                    with[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
        let frame = json!({"text": [{"text": "DICTATOR", "box": [0, 0, 1120, 40], "opacity": 1.0, "size": 40, "weight": 400, "align": "center"}]});
        let l = &lines(&with, &blank, w, h, &frame, (960.0, 40.0), None, None, &[], (0.0, 0.0))[0];
        assert!((l.off_centre - 80.0).abs() < 2.0, "{}", l.off_centre);
        assert_eq!(l.overflow[1], 160.0);
        let f = l.faults();
        assert!(f.iter().any(|x| x.contains("right edge")) && f.iter().any(|x| x.contains("right of its visible box")), "{f:?}");
    }

    #[test]
    fn letterbox_bars_are_rows_black_in_every_picture() {
        let dark: Vec<bool> = (0..100).map(|y| !(12..88).contains(&y)).collect();
        assert_eq!(bars(&dark), (0.12, 0.12));
        // an all-black film has no bars, and a sliver isn't one
        assert_eq!(bars(&[true; 50]), (0.0, 0.0));
        let sliver: Vec<bool> = (0..100).map(|y| y == 0).collect();
        assert_eq!(bars(&sliver), (0.0, 0.0));
        // words whose middle sits in a bar are in it
        let (w, h) = (40, 100);
        let blank = vec![0u8; w * h * 4];
        let mut with = blank.clone();
        for y in 92..97 {
            for x in (5..35).step_by(2) {
                let o = (y * w + x) * 4;
                with[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let frame = json!({"text": [{"text": "caption", "box": [0, 90, 40, 10], "opacity": 1.0, "size": 30, "weight": 400}]});
        let l = &lines(&with, &blank, w, h, &frame, (40.0, 100.0), None, None, &[], (0.12, 0.12))[0];
        assert_eq!(l.bar, Some("bottom"));
    }
}
