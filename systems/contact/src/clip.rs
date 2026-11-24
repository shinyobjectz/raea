//! The clipping check as a render's frames come (docs/stage-guards.feature SG4a): each frame's parts tested as it
//! comes and let go, so a long render's check never holds its frames' parts. A part is followed from frame to frame by
//! its id (raea_core_scene::ids_of, as spans_of names it); what an overlap's sentence needs (its two parts at its deepest
//! frame, their solids) is kept as it's found.

use std::collections::HashMap;

use bevy::math::{Mat4, Vec3};
use serde_json::{Value, json};

use raea_core_scene::{Item, excused, ids_of, label};

use super::{Hit, clear_move, deepest, scale_of, world_box};
use crate::seen::shows;
use crate::solid::{Held, occluders};

type Pair = (usize, usize);
/// An overlap on one frame: the frame, the hit, and its two parts there.
type At = (usize, Hit, Item, Item);

/// The clipping check a frame at a time: push each frame's parts in order, then `check` (or `told`) once.
pub struct Clipping {
    w: u32,
    h: u32,
    fps: f64,
    nums: Vec<i64>,
    /// each part's index, numbered as parts first show
    ids: HashMap<String, usize>,
    held: Held,
    /// each pair's test on the frame before, reused while neither part moves (the camera may still have); only the
    /// last frame's pairs are kept, so a long render's cache holds one frame's
    last: HashMap<Pair, (Mat4, Mat4, Option<Hit>)>,
    /// the parts in an overlap still to be said, whose solids are kept for its sentence
    pinned: Vec<bool>,
    /// each pair's shown stretches (frames in a row) as they come: first, last, and the deepest hit (seam let go)
    found: HashMap<Pair, Vec<(usize, usize, At)>>,
    /// overlaps no frame shows (off frame, behind something): first and last frame, and the first frame's hit
    hidden: HashMap<Pair, (usize, usize, At)>,
}

impl Clipping {
    pub fn new(w: u32, h: u32, fps: f64) -> Clipping {
        Clipping { w, h, fps, nums: Vec::new(), ids: HashMap::new(), held: Held::default(), last: HashMap::new(), pinned: Vec::new(), found: HashMap::new(), hidden: HashMap::new() }
    }

    /// The next frame (its number, `frame`) and its parts: the parts' world boxes swept along x for the pairs that
    /// meet, each pair tested.
    pub fn push(&mut self, frame: i64, its: &[Item]) {
        let k = self.nums.len();
        self.nums.push(frame);
        let mut here: Vec<(usize, &Item)> = Vec::new();
        for (id, it) in ids_of(its, &["mesh", "model"]) {
            let next = self.ids.len();
            here.push((*self.ids.entry(id).or_insert(next), it));
        }
        here.sort_by_key(|h| h.0);
        self.held.at(k, &here);
        let solids = &self.held.solids;
        let item = |i: usize| here[here.binary_search_by_key(&i, |h| h.0).unwrap()].1;
        let mut boxes: Vec<(usize, Vec3, Vec3)> = Vec::new();
        for &(i, it) in &here {
            let Some(ss) = &solids[i] else { continue };
            if excused(it) {
                continue;
            }
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for s in ss.iter() {
                let (l, h) = world_box(s, &it.world);
                lo = lo.min(l);
                hi = hi.max(h);
            }
            boxes.push((i, lo, hi));
        }
        boxes.sort_by(|x, y| x.1.x.total_cmp(&y.1.x));
        let mut occ = None;
        let mut tested: HashMap<Pair, (Mat4, Mat4, Option<Hit>)> = HashMap::new();
        for (n, &(i0, lo0, hi0)) in boxes.iter().enumerate() {
            for &(j0, lo1, hi1) in &boxes[n + 1..] {
                if lo1.x > hi0.x {
                    break;
                }
                if lo0.cmpgt(hi1).any() || lo1.cmpgt(hi0).any() {
                    continue;
                }
                let (i, j) = (i0.min(j0), i0.max(j0));
                let (a, b) = (solids[i].as_ref().unwrap(), solids[j].as_ref().unwrap());
                let (pa, pb) = (item(i), item(j));
                let (wa, wb) = (pa.world, pb.world);
                let r = match self.last.get(&(i, j)) {
                    Some((la, lb, r)) if *la == wa && *lb == wb => r.clone(),
                    _ => deepest(a, &wa, b, &wb),
                };
                tested.insert((i, j), (wa, wb, r.clone()));
                let Some(mut hit) = r else { continue };
                self.pinned.resize(self.pinned.len().max(j + 1), false);
                (self.pinned[i], self.pinned[j]) = (true, true);
                let (sa, sb) = (&a[hit.pieces.0], &b[hit.pieces.1]);
                let skip = 3.0 * (sa.cell * scale_of(&wa)).max(sb.cell * scale_of(&wb));
                if shows(its, &std::mem::take(&mut hit.seam), occ.get_or_insert_with(|| occluders(&here, solids)), skip, self.w as f32, self.h as f32) {
                    let runs = self.found.entry((i, j)).or_default();
                    match runs.last_mut() {
                        Some(run) if run.1 + 1 == k => {
                            run.1 = k;
                            // the deepest, the last of equals (as max_by took it)
                            if hit.depth >= run.2.1.depth {
                                run.2 = (k, hit, pa.clone(), pb.clone());
                            }
                        }
                        _ => runs.push((k, k, (k, hit, pa.clone(), pb.clone()))),
                    }
                } else {
                    self.hidden.entry((i, j)).or_insert_with(|| (k, k, (k, hit, pa.clone(), pb.clone()))).1 = k;
                }
            }
        }
        self.last = tested;
        let pinned = &self.pinned;
        self.held.after(k, |i| pinned.get(i).copied().unwrap_or(false));
    }

    /// The overlaps the frames show: each with a sentence (for the agent) and its numbers (for the manifest).
    pub fn check(self) -> Vec<Value> {
        let Clipping { fps, nums, held, found, hidden, .. } = self;
        // unseen on every frame: still said, so a clip turned away from reads as hidden, not fixed (DR3, DR9's cheats)
        let mut unseen: Vec<_> = hidden.into_iter().filter(|(p, _)| !found.contains_key(p)).collect();
        unseen.sort_by_key(|(p, (k, _, _))| (*k, *p));
        let mut spans: Vec<_> = found.into_iter().collect();
        spans.sort_by_key(|(p, g)| (g[0].0, *p));
        let clock = |f: i64| {
            let s = f as f64 / fps;
            format!("{}:{:04.1}", (s / 60.0) as i64, s % 60.0)
        };
        let mut out = Vec::new();
        // one overlap said, over frames f0 to f1 (`seen`: its seam on screen, else said as standing in the scene unseen)
        let mut say = |i: usize, j: usize, (_, hit, pa, pb): &At, f0: i64, f1: i64, seen: bool| {
            let (sa, sb) = (held.solids[i].as_ref().unwrap(), held.solids[j].as_ref().unwrap());
            let (la, lb) = (label(pa, &sa[hit.pieces.0].name), label(pb, &sb[hit.pieces.1].name));
            let (small, big) = if hit.small_first { (la, lb) } else { (lb, la) };
            // out the shortest way: the smallest move along a world axis (not down) that leaves no piece in the other's
            let (axis, step) = clear_move(&sa[..], &pa.world, &sb[..], &pb.world, hit.small_first);
            let axis_name = ["x", "y", "z"][axis];
            let when = if f0 == f1 { clock(f0) } else { format!("{}–{}", clock(f0), clock(f1)) };
            let shows = if seen { "and the seam shows" } else { "where no frame shows it (off frame or behind something), so it stands in the scene unseen" };
            let says = format!(
                "{when} {small} passes {:.2} m into {big} ({:.0}% of it inside), {shows}: move {small} {step:+.2} along world {axis_name} (or move {big}), or give it allowOverlap if it's meant to",
                hit.depth, hit.share * 100.0
            );
            out.push(json!({"from": f0, "to": f1, "part": small, "into": big, "depth": (hit.depth * 1000.0).round() / 1000.0, "share": (hit.share * 1000.0).round() / 1000.0, "move": {"axis": axis_name, "by": (step * 1000.0).round() / 1000.0}, "seen": seen, "says": says}));
        };
        for ((i, j), runs) in spans {
            for (first, last, at) in runs {
                say(i, j, &at, nums[first], nums[last], true);
            }
        }
        for ((i, j), (k0, k1, at)) in unseen {
            say(i, j, &at, nums[k0], nums[k1], false);
        }
        out
    }

    /// `check`'s overlaps as Findings (DR3), each said on stderr (what an agent driving raea reads) as it's returned
    /// for the manifest.
    pub fn told(self) -> Vec<raea_core_facts::Finding> {
        let t = std::time::Instant::now();
        let found = crate::clip_findings(&self.check(), "raea render");
        for f in &found {
            eprintln!("raea: {}", f.line());
        }
        raea_core_facts::finding::said(&found);
        if raea_io_trace::stat("RAEA_TIMES") {
            eprintln!("raea times: the clipping check {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
        }
        found
    }
}
