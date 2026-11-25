//! How a reference is layered (docs/videoform.feature VF8e). A finished video is often more than one picture at a
//! time: the speaker cut out over B-roll, a photo or a testimonial card laid over the speaker, the product on a plain
//! backdrop, two pictures sharing the frame, a page or a screen shown. A shot's layers are read from small copies of its
//! frames (what holds still while the rest moves, what's cut while the rest isn't, where straight edges stay put) and
//! from what Vision saw in a few of them (the person mask, faces, lines of text, whether it's a screen). Captions are words on screen, kept in the
//! rows already, and aren't a layer here.

use raea_sys_videoform::rows::Layer;

/// A frame, small (about 90 x 160), RGB8 row by row
#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<u8>,
}

/// What Vision saw in one of the shot's frames (rectangles aren't asked for: a real thing's edges make one too), boxes [x, y, w, h] of the frame from the top left
#[derive(Clone, Debug, Default)]
pub struct Sighting {
    pub faces: Vec<[f64; 4]>,
    /// the person mask, row by row (0 to 255), and its size; empty without a person
    pub mask: Vec<u8>,
    pub mw: usize,
    pub mh: usize,
    /// lines of text laid over the picture
    pub text: Vec<[f64; 4]>,
    pub screen: bool,
}

/// A pixel holds still when it changes less than this between frames (of 1)
const STILL: f32 = 0.012;
/// and moves when it changes more than this
const MOVES: f32 = 0.04;
/// A pixel is flat when its 5 x 5 neighbourhood varies less than this
const FLAT: f32 = 0.008;
/// and is the backdrop's colour when within this of it (each channel, of 1)
const SAME: f32 = 0.05;
/// A backdrop covers at least this much of the frame
const BACKDROP: f64 = 0.15;
/// A picture laid over covers between these shares of the frame
const SMALLEST: f64 = 0.02;
const LARGEST: f64 = 0.8;
/// A held-still part has this much detail, on average (its 5 x 5 spread)
const DETAIL: f32 = 0.08;
/// A held-still part counts as laid over when the rest of the frame moves at least this much
const MOVING: f64 = 0.2;
/// and fills at least this much of its box
const SQUARE: f64 = 0.6;
/// A cut-out person is no bigger than this share of the frame, a face no taller than SMALL_FACE
const CUTOUT: f64 = 0.3;
const SMALL_FACE: f64 = 0.15;
/// An edge is a step of at least this between neighbouring pixels, in this share of the frames or more
const EDGE: f32 = 0.12;
const HELD: f64 = 0.7;
/// A side is a border when it's an edge along at least this much of it
const ALONG: f64 = 0.7;
/// A split's edge runs across at least this much of the frame
const ACROSS: f64 = 0.9;
/// A jump is a change of at least this, filling this much of its box
const JUMP: f32 = 0.1;
const CELL: usize = 5;
/// A cell jumps when this share of its pixels do
const DENSE: f32 = 0.6;
/// The patch fills this much of its box, which covers at least SWAPPED of the frame, and the cells outside it jump
/// less than CALM on average
const JUMP_FILL: f64 = 0.5;
const SWAPPED: f64 = 0.05;
const CALM: f32 = 0.2;
/// and the box changes less than this in the steps either side
const STEADY: f32 = 0.25;
/// A still thing on a backdrop this much of the frame is the backdrop's, not a card
const ON_BACKDROP: f64 = 0.4;
/// A box with words in it no taller than this is a caption's
const CAPTION: f64 = 0.12;
/// A cut-out person is a bust: the person no taller than this many faces
const BUST: f64 = 4.5;
/// A face this tall (of the frame) or taller is someone on camera, not in a picture laid over
const MAIN_FACE: f64 = 0.06;
/// Two stacked pictures' seam is within this of the middle
const SEAM: f64 = 0.1;
/// A box over a split frame is a picture laid over it only when it's no bigger than this share of the frame
const ON_SPLIT: f64 = 0.15;
/// A screen's text: at least this many lines, each no taller than this share of the frame, the bounds this much wider
const PAGE_LINES: usize = 4;
const SMALL_TEXT: f64 = 0.025;
const PAD: f64 = 0.02;
/// A form is the shot's when it's seen in at least half the frames Vision looked at
const HALF: f64 = 0.5;

/// Rec. 601 luma's weights
const LUMA: [f32; 3] = [0.299, 0.587, 0.114];

fn grey(f: &Frame) -> Vec<f32> {
    f.rgb.chunks_exact(3).map(|p| p.iter().zip(LUMA).map(|(c, k)| *c as f32 * k).sum::<f32>() / 255.0).collect()
}

/// Each pixel's mean change between consecutive frames
fn change(greys: &[Vec<f32>]) -> Vec<f32> {
    let n = greys.first().map_or(0, Vec::len);
    let pairs = greys.len().saturating_sub(1).max(1) as f32;
    let mut out = vec![0f32; n];
    for w in greys.windows(2) {
        out.iter_mut().zip(w[0].iter().zip(&w[1])).for_each(|(o, (a, b))| *o += (a - b).abs() / pairs);
    }
    out
}

/// How much each pixel's 5 x 5 neighbourhood varies (its largest less its smallest, of the grey)
fn spread(g: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0f32; g.len()];
    for y in 0..h {
        for x in 0..w {
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for yy in y.saturating_sub(2)..(y + 3).min(h) {
                for xx in x.saturating_sub(2)..(x + 3).min(w) {
                    let v = g[yy * w + xx];
                    (lo, hi) = (lo.min(v), hi.max(v));
                }
            }
            out[y * w + x] = hi - lo;
        }
    }
    out
}

/// The plain colour the frame's flat, still pixels share most, and the share of the frame it covers
fn backdrop(f: &Frame, flat: &[f32], moved: &[f32]) -> Option<([f64; 4], f64)> {
    let px = |i: usize| [f.rgb[i * 3], f.rgb[i * 3 + 1], f.rgb[i * 3 + 2]].map(|c| c as f32 / 255.0);
    let candidates: Vec<usize> = (0..flat.len()).filter(|&i| flat[i] < FLAT && moved[i] < STILL).collect();
    // the commonest colour among them, in steps of 1/16
    let mut counts = std::collections::HashMap::new();
    for &i in &candidates {
        *counts.entry(px(i).map(|c| (c * 16.0) as u8)).or_insert(0usize) += 1;
    }
    let (bin, _) = counts.into_iter().max_by_key(|(_, n)| *n)?;
    let centre = bin.map(|b| (b as f32 + 0.5) / 16.0);
    let on: Vec<usize> = candidates.into_iter().filter(|&i| px(i).iter().zip(centre).all(|(a, b)| (a - b).abs() < SAME + 1.0 / 32.0)).collect();
    let share = on.len() as f64 / flat.len().max(1) as f64;
    (share >= BACKDROP).then(|| (bounds(&on, f.w, f.h), share))
}

/// The box of some pixels, as shares of the frame
fn bounds(px: &[usize], w: usize, h: usize) -> [f64; 4] {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for &i in px {
        let (x, y) = (i % w, i / w);
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
    }
    if x1 <= x0 {
        return [0.0; 4];
    }
    [x0 as f64 / w as f64, y0 as f64 / h as f64, (x1 - x0) as f64 / w as f64, (y1 - y0) as f64 / h as f64]
}

fn area(b: &[f64; 4]) -> f64 {
    b[2].max(0.0) * b[3].max(0.0)
}

fn overlap(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let w = (a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0]);
    let h = (a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1]);
    w.max(0.0) * h.max(0.0)
}

/// The parts of the frame that hold still while the rest moves: each one's pixels, when they're a box's worth
fn held(moved: &[f32], flat: &[f32], w: usize, h: usize) -> Vec<Vec<usize>> {
    let moving = moved.iter().filter(|&&m| m > MOVES).count() as f64 / moved.len().max(1) as f64;
    if moving < MOVING {
        return Vec::new();
    }
    let mut out = parts(&|i| moved[i] < STILL, w, h);
    out.retain(|p| {
        let b = bounds(p, w, h);
        let share = p.len() as f64 / moved.len() as f64;
        // a plain colour holds still whatever the camera does: a picture laid over has detail of its own
        let detail = p.iter().map(|&i| flat[i]).sum::<f32>() / p.len().max(1) as f32;
        (SMALLEST..=LARGEST).contains(&area(&b)) && share / area(&b).max(1e-9) >= SQUARE && detail >= DETAIL
    });
    out
}

/// The parts of a w x h map where `on` holds, apart (4-connected): each one's pixels
fn parts(on: &dyn Fn(usize) -> bool, w: usize, h: usize) -> Vec<Vec<usize>> {
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    for start in 0..w * h {
        if seen[start] || !on(start) {
            continue;
        }
        let (mut part, mut stack) = (Vec::new(), vec![start]);
        seen[start] = true;
        while let Some(i) = stack.pop() {
            part.push(i);
            let (x, y) = (i % w, i / w);
            let near = [(x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w)];
            for j in near.into_iter().flatten() {
                if !seen[j] && on(j) {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        out.push(part);
    }
    out
}

/// The share of a line's pixels where the grey steps by EDGE across it: along a row at `line` (its pixels `span`), or
/// down a column (`down`)
fn stepped(g: &[f32], w: usize, line: usize, span: std::ops::Range<usize>, down: bool) -> f64 {
    stepped_but(g, w, line, span, down, &|_| false)
}

/// The share of a line's span that steps, leaving out the places `hid` covers (a caption over a split's seam)
fn stepped_but(g: &[f32], w: usize, line: usize, span: std::ops::Range<usize>, down: bool, hid: &dyn Fn(usize) -> bool) -> f64 {
    let last = g.len().saturating_sub(1);
    let hit = |k: usize| {
        let (a, b) = if down { (k * w + line - 1, k * w + line + 1) } else { ((line - 1) * w + k, (line + 1) * w + k) };
        (g[a.min(last)] - g[b.min(last)]).abs() > EDGE
    };
    let open: Vec<usize> = span.filter(|&k| !hid(k)).collect();
    open.iter().filter(|&&k| hit(k)).count() as f64 / open.len().max(1) as f64
}

/// Whether something holds in HELD of the frames
fn through(greys: &[Vec<f32>], hit: &dyn Fn(&[f32]) -> bool) -> bool {
    greys.iter().filter(|g| hit(g)).count() as f64 >= HELD * greys.len() as f64
}

/// Whether a box's edges are straight steps that stay put through the frames (a picture's border, not a thing's)
fn bordered(greys: &[Vec<f32>], w: usize, h: usize, b: &[f64; 4]) -> bool {
    let (x0, y0) = ((b[0] * w as f64) as usize, (b[1] * h as f64) as usize);
    let (x1, y1) = ((((b[0] + b[2]) * w as f64) as usize).min(w - 1), (((b[1] + b[3]) * h as f64) as usize).min(h - 1));
    // each side away from the frame's own edge: its line, its span, and whether it runs down
    let sides = [(y0 > 1, y0, x0..x1, false), (y1 + 1 < h, y1, x0..x1, false), (x0 > 1, x0, y0..y1, true), (x1 + 1 < w, x1, y0..y1, true)];
    let inside: Vec<_> = sides.into_iter().filter(|s| s.0).collect();
    !inside.is_empty() && inside.iter().all(|(_, line, span, down)| through(greys, &|g| stepped(g, w, *line, span.clone(), *down) >= ALONG))
}

/// A straight edge across the whole frame through every frame, between a third and two thirds of the way, with no
/// such edge a few pixels either side (a border, not texture): [0, y, 1, 0] across or [x, 0, 0, 1] down. A caption
/// laid over the seam (`text`) hides that much of it, and isn't counted against it
fn split(greys: &[Vec<f32>], w: usize, h: usize, text: &[[f64; 4]]) -> Option<[f64; 4]> {
    let line = |g: &[f32], i: usize, down: bool| {
        let span = if down { 0..h } else { 0..w };
        let (n, m) = if down { (h, w) } else { (w, h) };
        let hid = |k: usize| {
            let (along, across) = (k as f64 / n as f64, i as f64 / m as f64);
            text.iter().any(|t| if down { (t[1]..t[1] + t[3]).contains(&along) && (t[0] - 0.02..t[0] + t[2] + 0.02).contains(&across) } else { (t[0]..t[0] + t[2]).contains(&along) && (t[1] - 0.02..t[1] + t[3] + 0.02).contains(&across) })
        };
        stepped_but(g, w, i, span.clone(), down, &hid) >= ACROSS && stepped_but(g, w, i - 4, span.clone(), down, &hid) < 0.5 && stepped_but(g, w, i + 4, span, down, &hid) < 0.5
    };
    for down in [false, true] {
        let n = if down { w } else { h };
        if let Some(i) = (n / 3..2 * n / 3).find(|&i| through(greys, &|g| line(g, i, down))) {
            let at = i as f64 / n as f64;
            return Some(if down { [at, 0.0, 0.0, 1.0] } else { [0.0, at, 1.0, 0.0] });
        }
    }
    None
}

/// Whether most sightings have a face wholly in the frame's bottom half under one in its top half: two pictures stacked
/// (a vertical frame shows one person's face high, never one under another's)
fn stacked(seen: &[Sighting], looked: f64) -> bool {
    let two = |s: &&Sighting| s.faces.iter().any(|f| f[1] + f[3] / 2.0 < 0.5 - SEAM) && s.faces.iter().any(|f| f[1] > 0.5);
    let n = seen.iter().filter(two).count() as f64;
    n > 0.0 && n >= HALF * looked
}

/// The seam of two stacked pictures: the row near the middle that steps most, through the frames
fn seam(greys: &[Vec<f32>], w: usize, h: usize) -> [f64; 4] {
    let lo = ((0.5 - SEAM) * h as f64) as usize;
    let rows = lo.max(1)..((0.5 + SEAM) * h as f64) as usize;
    let score = |i: usize| greys.iter().map(|g| stepped(g, w, i, 0..w, false)).sum::<f64>();
    let i = rows.max_by(|a, b| score(*a).total_cmp(&score(*b))).unwrap_or(h / 2);
    [0.0, i as f64 / h as f64, 1.0, 0.0]
}

/// Where a sighting shows a screen or page: PAGE_LINES or more lines of small text (an interface's, not a caption's),
/// their bounds a little wider
fn page(s: &Sighting) -> Option<[f64; 4]> {
    let small: Vec<&[f64; 4]> = s.text.iter().filter(|t| t[3] < SMALL_TEXT).collect();
    if small.len() < PAGE_LINES {
        return None;
    }
    let (x0, y0) = small.iter().fold((1f64, 1f64), |(x, y), t| (x.min(t[0]), y.min(t[1])));
    let (x1, y1) = small.iter().fold((0f64, 0f64), |(x, y), t| (x.max(t[0] + t[2]), y.max(t[1] + t[3])));
    let (x0, y0, x1, y1) = ((x0 - PAD).max(0.0), (y0 - PAD).max(0.0), (x1 + PAD).min(1.0), (y1 + PAD).min(1.0));
    let at = [x0, y0, x1 - x0, y1 - y0];
    (area(&at) >= SMALLEST).then_some(at)
}

/// A person in a sighting cut out over another picture, its box: a bust (no taller than BUST faces) with a small face
/// that stops short of the frame's bottom, floating over what's behind it
fn cutout(s: &Sighting) -> Option<[f64; 4]> {
    if s.mask.len() != s.mw * s.mh || s.mw == 0 {
        return None;
    }
    for person in parts(&|i| s.mask[i] > 127, s.mw, s.mh) {
        let b = bounds(&person, s.mw, s.mh);
        // the tallest face that's this person's
        let face = s.faces.iter().filter(|f| overlap(f, &b) > 0.5 * area(f)).map(|f| f[3]).fold(0.0, f64::max);
        if face > 0.0 && face <= SMALL_FACE && area(&b) <= CUTOUT && b[3] <= BUST * face && b[1] + b[3] < 0.95 {
            return Some(b);
        }
    }
    None
}

/// How much each CELL-pixel cell changes between two frames: the share of its pixels that change by JUMP; and the
/// cells' width and height
fn cells(a: &[f32], b: &[f32], w: usize, h: usize) -> (Vec<f32>, usize, usize) {
    let (cw, ch) = (w / CELL, h / CELL);
    let density = |cx: usize, cy: usize| {
        let px = (0..CELL).flat_map(|dy| (0..CELL).map(move |dx| (cy * CELL + dy) * w + cx * CELL + dx));
        px.filter(|&i| (a[i] - b[i]).abs() > JUMP).count() as f32 / (CELL * CELL) as f32
    };
    ((0..cw * ch).map(|i| density(i % cw, i / cw)).collect(), cw, ch)
}

/// The mean change of the cells inside a box (shares of the frame)
fn inside(c: &(Vec<f32>, usize, usize), at: &[f64; 4]) -> f32 {
    let mine: Vec<usize> = pixels(at, c.1, c.2).collect();
    mine.iter().map(|&i| c.0[i]).sum::<f32>() / mine.len().max(1) as f32
}

/// Where the picture jumps between two frames while the rest barely changes: a photo swapped, a card put on or taken
/// off. A cell jumps when most of its pixels change; the biggest patch of such cells is the box, when it fills most of
/// it and few cells outside jump. A cut of the whole frame isn't one.
fn jump(c: &(Vec<f32>, usize, usize)) -> Option<[f64; 4]> {
    let (cells, cw, ch) = (&c.0, c.1, c.2);
    // a cell beside one that jumps counts with it: a card swapped for another changes its words, not its white
    let near = |i: usize| {
        let (x, y) = (i % cw, i / cw);
        (y.saturating_sub(1)..(y + 2).min(ch)).flat_map(|yy| (x.saturating_sub(1)..(x + 2).min(cw)).map(move |xx| yy * cw + xx)).any(|k| cells[k] >= DENSE)
    };
    let patch = parts(&near, cw, ch).into_iter().max_by_key(Vec::len)?;
    let at = bounds(&patch, cw, ch);
    let n = pixels(&at, cw, ch).count();
    let outside = (cells.iter().sum::<f32>() - inside(c, &at) * n as f32) / (cells.len() - n).max(1) as f32;
    let fill = patch.len() as f64 / n.max(1) as f64;
    (area(&at) >= SWAPPED && area(&at) <= LARGEST && fill >= JUMP_FILL && outside < CALM).then_some(at)
}

/// A shot's layers, from its frames (in order, a few a second), the frames just before and after it (where a jump
/// confined to a box shows a layer's own cut), and what Vision saw in some of them
pub fn layers(frames: &[Frame], around: [Option<&Frame>; 2], seen: &[Sighting]) -> Vec<Layer> {
    let Some(mid) = frames.get(frames.len() / 2) else { return Vec::new() };
    let (w, h) = (mid.w, mid.h);
    let alike = |f: &&Frame| f.w == w && f.h == h;
    let greys: Vec<Vec<f32>> = frames.iter().filter(alike).map(grey).collect();
    let moved = change(&greys);
    let mut out: Vec<Layer> = Vec::new();
    let looked = seen.len().max(1) as f64;
    let flat = spread(&grey(mid), w, h);
    if let Some((at, share)) = backdrop(mid, &flat, &moved) {
        out.push(Layer { form: "backdrop".into(), at, seen: share.min(1.0), kind: None });
    }
    // laid over: a part that holds still while the rest moves, its border staying put, or a part whose own cut leaves
    // the rest as it was (Vision's rectangles aren't used: a sauna's light panel or a mirror is one too)
    let text: Vec<[f64; 4]> = seen.iter().flat_map(|s| s.text.iter().copied()).collect();
    let faces: Vec<[f64; 4]> = seen.iter().flat_map(|s| s.faces.iter().copied()).collect();
    let halves = split(&greys, w, h, &text).or_else(|| stacked(seen, looked).then(|| seam(&greys, w, h)));
    if let Some(at) = halves {
        out.push(Layer { form: "split".into(), at, seen: 1.0, kind: None });
    }
    let mut boxes: Vec<[f64; 4]> = held(&moved, &flat, w, h).iter().map(|p| bounds(p, w, h)).collect();
    // jumps inside the shot, and at its ends against the frames either side
    let [before, after] = around.map(|f| f.filter(alike).map(grey));
    let run: Vec<&Vec<f32>> = before.iter().chain(&greys).chain(after.iter()).collect();
    // a layer's own cut is at once: the box holds still in the steps either side of its jump, where a thing moving
    // through the picture goes on changing
    let steps: Vec<(Vec<f32>, usize, usize)> = run.windows(2).map(|p| cells(p[0], p[1], w, h)).collect();
    let steady = |k: Option<usize>, at: &[f64; 4]| k.and_then(|k| steps.get(k)).is_none_or(|c| inside(c, at) < STEADY);
    let found: Vec<(usize, [f64; 4])> = (0..steps.len()).filter_map(|k| jump(&steps[k]).filter(|at| steady(k.checked_sub(1), at) && steady(Some(k + 1), at)).map(|at| (k, at))).collect();
    // at the shot's own cut, one is enough; inside it, a layer is swapped more than once (a hand flung once isn't one)
    let edge = |k: usize| (k == 0 && before.is_some()) || (k + 1 == steps.len() && after.is_some());
    let again = |a: &[f64; 4]| found.iter().filter(|(_, b)| overlap(a, b) > 0.6 * area(a).max(area(b))).count() >= 2;
    let jumped: Vec<[f64; 4]> = found.iter().filter(|(k, at)| edge(*k) || again(at)).map(|x| x.1).collect();
    boxes.extend(jumped.iter().copied());
    for b in boxes {
        // a caption's own box isn't a picture laid over
        let words = text.iter().map(|t| overlap(t, &b)).sum::<f64>() / area(&b).max(1e-9);
        // its own cuts: the jumps that are it
        let swaps = jumped.iter().filter(|j| overlap(j, &b) > 0.6 * area(&b).max(area(j))).count();
        let cut_alone = swaps > 0;
        // in a split frame each half moves and cuts on its own: only a small picture laid over one is a layer
        let half = halves.is_some() && area(&b) > ON_SPLIT;
        // a box around someone's face on camera is that person moving, not a picture of them laid over
        let on_face = faces.iter().any(|f| f[3] >= MAIN_FACE && overlap(f, &b) >= 0.5 * area(f));
        if half || on_face || words > 0.7 || (words > 0.2 && b[3] < CAPTION) || !(cut_alone || bordered(&greys, w, h, &b)) || out.iter().any(|l| l.form != "backdrop" && overlap(&l.at, &b) > 0.6 * area(&b)) {
            continue;
        }
        // a moving picture changes between most frames; a card only where it's swapped
        let changing = steps.iter().filter(|c| inside(c, &b) >= STEADY).count();
        let moves = changing.saturating_sub(swaps) as f64 / steps.len().max(1) as f64;
        let form = if moves > MOVING { "inset" } else { "card" };
        if form == "card" && out.iter().any(|l| l.form == "backdrop" && l.seen >= ON_BACKDROP) {
            continue;
        }
        out.push(Layer { form: form.into(), at: r3(b), seen: 1.0, kind: None });
    }
    let cut: Vec<[f64; 4]> = seen.iter().filter_map(cutout).collect();
    if cut.len() as f64 >= HALF * looked && !cut.is_empty() {
        out.push(Layer { form: "cutout".into(), at: r3(cut[cut.len() / 2]), seen: cut.len() as f64 / looked, kind: None });
    }
    // a screen or page shown: where its small lines of text are (an app's or a site's), else the frame Vision took for one
    let pages: Vec<[f64; 4]> = seen.iter().filter_map(page).collect();
    let screens = seen.iter().filter(|s| s.screen).count() as f64;
    if pages.len() as f64 >= HALF * looked && !pages.is_empty() {
        out.push(Layer { form: "screen".into(), at: pages[pages.len() / 2], seen: pages.len() as f64 / looked, kind: None });
    } else if screens >= HALF * looked && screens > 0.0 {
        out.push(Layer { form: "screen".into(), at: [0.0, 0.0, 1.0, 1.0], seen: screens / looked, kind: None });
    }
    out.iter_mut().for_each(|l| (l.at, l.seen) = (r3(l.at), (l.seen * 100.0).round() / 100.0));
    out
}

fn pixels(b: &[f64; 4], w: usize, h: usize) -> impl Iterator<Item = usize> {
    let (x0, y0) = ((b[0] * w as f64) as usize, (b[1] * h as f64) as usize);
    let (x1, y1) = ((((b[0] + b[2]) * w as f64) as usize).min(w), (((b[1] + b[3]) * h as f64) as usize).min(h));
    (y0..y1).flat_map(move |y| (x0..x1).map(move |x| y * w + x))
}

fn r3(b: [f64; 4]) -> [f64; 4] {
    b.map(|v| (v * 1000.0).round() / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a 40 x 60 frame: noise that changes with `t`, and where `card` says, a still grey box with a dark border
    fn frame(t: usize, card: Option<[usize; 4]>, plain: bool) -> Frame {
        let (w, h) = (40, 60);
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let n = ((x * 7 + y * 13 + t * 31) % 97) as u8 * 2 + 20;
                let v = match card {
                    Some([cx, cy, cw, ch]) if (cx..cx + cw).contains(&x) && (cy..cy + ch).contains(&y) => {
                        if x == cx || y == cy || x + 1 == cx + cw || y + 1 == cy + ch { 10 } else { 200 }
                    }
                    _ if plain => 128,
                    _ => n,
                };
                rgb.extend([v, v, v]);
            }
        }
        Frame { w, h, rgb }
    }

    #[test]
    fn a_still_box_over_moving_picture_is_a_card_and_a_plain_colour_a_backdrop() {
        let frames: Vec<Frame> = (0..6).map(|t| frame(t, Some([8, 10, 20, 16]), false)).collect();
        let l = layers(&frames, [None, None], &[Sighting::default()]);
        assert_eq!(l.iter().map(|l| l.form.as_str()).collect::<Vec<_>>(), ["card"], "{l:?}");
        let plain: Vec<Frame> = (0..6).map(|t| frame(t, Some([8, 10, 20, 16]), true)).collect();
        assert!(layers(&plain, [None, None], &[Sighting::default()]).iter().any(|l| l.form == "backdrop"));
        let moving: Vec<Frame> = (0..6).map(|t| frame(t, None, false)).collect();
        assert!(layers(&moving, [None, None], &[Sighting::default()]).is_empty(), "moving picture alone is one layer");
        // a still picture whose middle alone changes at the cut after it: the middle is laid over
        let still: Vec<Frame> = (0..4).map(|_| frame(0, Some([8, 10, 20, 16]), false)).collect();
        let mut after = frame(0, Some([8, 10, 20, 16]), false);
        (12..24).for_each(|y| (10..26).for_each(|x| after.rgb[(y * 40 + x) * 3..(y * 40 + x) * 3 + 3].copy_from_slice(&[30, 30, 30])));
        assert!(layers(&still, [None, Some(&after)], &[Sighting::default()]).iter().any(|l| l.form == "card"));
    }

    #[test]
    fn a_small_bust_cut_off_mid_frame_is_cut_out() {
        let (mw, mh) = (20, 40);
        let mut mask = vec![0u8; mw * mh];
        // a bust: head rows 20-24 narrow, shoulders 25-30 wide, cut straight at row 30
        for y in 20..31 {
            for x in 2..(if y < 25 { 6 } else { 9 }) {
                mask[y * mw + x] = 255;
            }
        }
        let s = Sighting { faces: vec![[0.15, 0.5, 0.15, 0.1]], mask, mw, mh, ..Default::default() };
        assert!(cutout(&s).is_some());
        let mut whole = s.clone();
        whole.faces = vec![[0.15, 0.5, 0.3, 0.3]];
        assert!(cutout(&whole).is_none(), "a big face is the shot's own person");
    }
}
