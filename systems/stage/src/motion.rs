//! What moves between frames: where the shots cut, a camera that lurches or reverses (SG13), and parts that snap
//! (SG12), including a line drawn as a chain of parts.

use std::collections::{BTreeMap, HashMap};

use bevy::math::Vec3;
use raea_core_facts::{Finding, Level};

use super::{Item, Stage, View, called, num, origin, scale_of};

/// A cut: the eye jumps or the view turns this many times its neighbours' step in one frame (and past a floor)
const CUT_RATIO: f32 = 8.0;
const CUT_METRES: f32 = 0.25;
const CUT_DEGREES: f32 = 4.0;
/// A speed change this sharp in one frame, against the changes either side of it, is a lurch
const KINK: f32 = 3.0;
/// The camera counts as moving above this share of the shot's top speed, and above a speed a change in can be seen at
/// all: 0.1 degrees of turn a frame, 3 mm of travel
const MOVING: f32 = 0.1;
const SEEN_TURN: f32 = 0.1;
const SEEN_TRAVEL: f32 = 0.003;
/// A lurch's ratio: an error from 2x, a warning from 1.5x
const LURCH: f32 = 2.0;
const LURCH_WARN: f32 = 1.5;
/// A reversal: a move one way then the other within this long, each at least this far
const REVERSE_SECS: f64 = 1.5;
const REVERSE_METRES: f32 = 0.15;
const REVERSE_DEGREES: f32 = 3.0;
/// A part's size or place on screen jumping more than this in one frame, alone, is a snap
const SNAP_SIZE: f32 = 1.25;
const SNAP_MOVE: f32 = 0.1;
/// A part reaching further than this from its origin (water, a floor, a building) has no one size on screen to snap
const BIG_REACH: f32 = 6.0;
/// This many parts snapping in one frame is the layout changing (an edit), not a part drawn from a stale frame
const EDIT_PARTS: usize = 4;
/// A chain of more than this many parts named NAME-i is a line made of parts
const CHAIN: usize = 8;

/// cut[k]: frame k starts a shot.
pub(crate) fn cuts(views: &[Option<View>], frames: &[i64]) -> Vec<bool> {
    let n = views.len();
    let step = |k: usize| -> Option<(f32, f32)> {
        let (a, b) = (views[k - 1]?, views[k]?);
        Some(((b.eye - a.eye).length(), a.turn(&b)))
    };
    let steps: Vec<Option<(f32, f32)>> = (0..n).map(|k| if k == 0 { None } else { step(k) }).collect();
    (0..n)
        .map(|k| {
            if k == 0 || frames[k] != frames[k - 1] + 1 || views[k].is_none() != views[k - 1].is_none() {
                return true;
            }
            let Some((d, t)) = steps[k] else { return false };
            let around = |f: fn(&(f32, f32)) -> f32| [k.wrapping_sub(1), k + 1].iter().filter_map(|&j| steps.get(j).copied().flatten()).map(|s| f(&s)).fold(0.0, f32::max);
            (d > CUT_METRES && d > CUT_RATIO * around(|s| s.0)) || (t > CUT_DEGREES && t > CUT_RATIO * around(|s| s.1))
        })
        .collect()
}

/// The shots: (first, last) frame indices, each with a camera throughout.
fn shots(st: &Stage) -> Vec<(usize, usize)> {
    let n = st.views.len();
    let mut out = Vec::new();
    let mut a = 0;
    for k in 1..=n {
        if k == n || st.cut[k] {
            if st.views[a].is_some() {
                out.push((a, k - 1));
            }
            a = k;
        }
    }
    out
}

/// SG13: inside each shot, a speed that changes sharply in one frame (a lurch) or a move undone within 1.5 s.
pub(crate) fn camera(st: &Stage) -> Vec<Finding> {
    let mut out = Vec::new();
    for (a, b) in shots(st) {
        if b < a + 3 {
            continue;
        }
        let v: Vec<View> = (a..=b).map(|k| st.views[k].unwrap()).collect();
        // each frame's step: the eye's travel (m), the view's turn (degrees); index i is the step into frame a+i
        let lin: Vec<f32> = (0..v.len()).map(|i| if i == 0 { 0.0 } else { (v[i].eye - v[i - 1].eye).length() }).collect();
        let ang: Vec<f32> = (0..v.len()).map(|i| if i == 0 { 0.0 } else { v[i - 1].turn(&v[i]) }).collect();
        for (what, s, seen) in [("travel", &lin, SEEN_TRAVEL), ("turn", &ang, SEEN_TURN)] {
            let mut said = Vec::new();
            let top = s.iter().copied().fold(0.0, f32::max);
            if top <= 1e-5 {
                continue;
            }
            let mut i = 2;
            while i + 1 < s.len() {
                let (p, q) = (s[i - 1], s[i]);
                let hi = p.max(q);
                let jump = (q - p).abs();
                let around = (s[i - 1] - s[i - 2]).abs().max((s[i + 1] - s[i]).abs());
                if hi > MOVING * top && hi > seen && jump > 0.5 * hi && jump > KINK * around {
                    let r = hi / p.min(q).max(1e-6);
                    if r >= LURCH_WARN {
                        let k = a + i;
                        let level = if r >= LURCH { Level::Error } else { Level::Warn };
                        let says = if p.min(q) < 1e-4 { format!("the camera's {what} {} at {} with no ease", if q > p { "starts at full speed" } else { "stops dead" }, st.at(k)) } else { format!("the camera's {what} {} {r:.1}x in one frame at {}", if q > p { "jumps faster" } else { "brakes" }, st.at(k)) };
                        let mut f = st.finding("camera-lurch", level, k - 1, k, says).fix("move the camera on one smooth path through its keys (raea/camera's cameraPath, and path.check() for keys that still brake), easing only where it starts and stops");
                        f.measure = Some(raea_core_facts::finding::Measure { name: "speed change".into(), value: r.min(99.0) as f64, unit: "x".into(), limit: Some(LURCH as f64) });
                        out.push(f);
                        said.push(k);
                        i += 3;
                        continue;
                    }
                }
                i += 1;
            }
            out.extend(ramps(st, a, what, s, (MOVING * top).max(seen), &said));
        }
        out.extend(reversals(st, a, &v));
    }
    out
}

/// `raea fingerprint things`' lurch (camera-speed, read from the picture's motion and run only by hand), folded in on
/// the camera itself so every render runs it: inside a move, its eases (half a second each end) left out, a half
/// second whose mean speed is LURCH times the one before's, or that fraction of it. A speed that ramps up over a few
/// frames passes the one-frame kink above. A warning, as that check's was (P2); none near a kink already said.
fn ramps(st: &Stage, a: usize, what: &str, s: &[f32], moving: f32, said: &[usize]) -> Vec<Finding> {
    let half = ((st.fps * 0.5).round() as usize).max(2);
    let mean = |r: std::ops::Range<usize>| s[r].iter().sum::<f32>() / half as f32;
    let mut out = Vec::new();
    let mut i = 1;
    while i < s.len() {
        let start = i;
        while i < s.len() && s[i] > moving {
            i += 1;
        }
        let (mut t, end) = (start + half, i.saturating_sub(half));
        while t + 2 * half <= end {
            let (m0, m1) = (mean(t..t + half), mean(t + half..t + 2 * half));
            let (r, k) = (m0.max(m1) / m0.min(m1).max(1e-6), a + t + half);
            if r >= LURCH && !said.iter().any(|x| x.abs_diff(k) <= 2 * half) {
                let how = if m1 > m0 { "speeds up" } else { "slows" };
                let says = format!("the camera's {what} {how} {r:.1}x within half a second at {}, mid-move", st.at(k));
                let mut f = st.finding("camera-lurch", Level::Warn, k - half, k + half - 1, says).fix("ease the speed change over a second or more, or hold one speed through the move (raea/camera's cameraPath)");
                f.measure = Some(raea_core_facts::finding::Measure { name: "speed change over half a second".into(), value: r.min(99.0) as f64, unit: "x".into(), limit: Some(LURCH as f64) });
                out.push(f);
                t += 2 * half;
                continue;
            }
            t += half;
        }
        i += 1;
    }
    out
}

/// A dolly in then out, or a pan one way then back, within REVERSE_SECS.
fn reversals(st: &Stage, a: usize, v: &[View]) -> Vec<Finding> {
    let mut out = Vec::new();
    // signed steps: along the view (in is +), and the turn about the view's up (left is +)
    let dolly: Vec<f32> = (1..v.len()).map(|i| (v[i].eye - v[i - 1].eye).dot(v[i - 1].fwd)).collect();
    let pan: Vec<f32> = (1..v.len()).map(|i| v[i - 1].fwd.cross(v[i].fwd).dot(v[i - 1].up).asin().to_degrees()).collect();
    for (what, s, least, ways) in [("framing", &dolly, REVERSE_METRES, ("in", "out")), ("pan", &pan, REVERSE_DEGREES, ("left", "right"))] {
        // runs of one sign, each with its total and its frames (indices into s, step i is into frame a+i+1)
        let top = s.iter().map(|x| x.abs()).fold(0.0, f32::max);
        let mut moves: Vec<(usize, usize, f32)> = Vec::new();
        for (i, x) in s.iter().enumerate() {
            if x.abs() <= 0.05 * top {
                continue;
            }
            match moves.last_mut() {
                Some(m) if m.2.signum() == x.signum() && i <= m.1 + 3 => {
                    m.1 = i;
                    m.2 += x;
                }
                _ => moves.push((i, i, *x)),
            }
        }
        for w in moves.windows(2) {
            let (m0, m1) = (w[0], w[1]);
            // the whole there-and-back, not the pause between two long moves
            let span = (m1.1 - m0.0 + 1) as f64 / st.fps;
            if m0.2.signum() != m1.2.signum() && m0.2.abs() >= least && m1.2.abs() >= least && span <= REVERSE_SECS {
                let (k0, k1) = (a + m0.0 + 1, a + m1.1 + 1);
                let (one, other) = if m0.2 > 0.0 { ways } else { (ways.1, ways.0) };
                let unit = if what == "pan" { "°" } else { " m" };
                let says = format!("the camera's {what} goes {one} {:.2}{unit} then {other} {:.2}{unit} within {:.1} s ({} to {})", m0.2.abs(), m1.2.abs(), st.secs(k0, k1), st.at(k0), st.at(k1));
                // a dolly in and back out is a path overshooting its key; a pan there and back can be a look round (a
                // first-person camera), so it's a warning
                let level = if what == "pan" { Level::Warn } else { Level::Error };
                out.push(st.finding("camera-reverse", level, k0, k1, says).fix("let the move finish where it's going: drop the key that turns it back, or hold between the two moves"));
            }
        }
    }
    out
}

/// A part's reach from its origin, in metres, for its size on screen: a block of words' width, a shape's box.
fn reach(it: &raea_core_scene::Item, boxes: &mut HashMap<String, Option<f32>>) -> Option<f32> {
    let own = match it.kind {
        "text" => {
            let words = it.props.get("text").and_then(serde_json::Value::as_str).unwrap_or("");
            let wide = words.split('\n').map(|l| l.chars().count()).max().unwrap_or(0) as f32 * 0.55;
            Some(0.5 * wide.max(1.0) * num(it, "size", 0.5) as f32)
        }
        "mesh" => {
            let shape = it.shape.as_deref()?;
            *boxes.entry(shape.to_string()).or_insert_with(|| {
                let t = raea_core_shape::cached(shape).ok()?;
                let r = t.positions.iter().map(|p| Vec3::from(*p).length()).fold(0.0, f32::max);
                (r > 0.0).then_some(r)
            })
        }
        _ => None,
    }?;
    Some(own * scale_of(it))
}

/// SG12: a named part whose size or place on screen jumps in one frame, alone (no cut, no steady move either side),
/// and a line drawn as a growing chain of parts named NAME-i.
#[derive(Default)]
pub(crate) struct Snaps {
    boxes: HashMap<String, Option<f32>>,
    /// each named part's screen size (px) and centre, by frame, a run for each of what it is: new words or a new shape
    /// is the part spawned again (SG1), new content and no snap (a counter's words growing as its number does)
    by: BTreeMap<(String, u64), Vec<(usize, f32, [f32; 2])>>,
    /// each chain's first frame, and its fewest and most links
    chains: BTreeMap<String, (usize, usize, usize)>,
}

impl Snaps {
    /// Frame k's named parts, through its camera `v`, on a w x h frame.
    pub(crate) fn push(&mut self, k: usize, its: &[Item], v: Option<View>, w: f32, h: f32) {
        let (boxes, by, chains) = (&mut self.boxes, &mut self.by, &mut self.chains);
        let mut links: HashMap<String, usize> = HashMap::new();
        for it in its.iter() {
            let Some(name) = it.props.get("name").and_then(serde_json::Value::as_str) else { continue };
            if let Some((stem, i)) = name.rsplit_once('-') {
                if !stem.is_empty() && i.chars().all(|c| c.is_ascii_digit()) && !i.is_empty() {
                    *links.entry(stem.to_string()).or_default() += 1;
                }
            }
            let (Some(v), Some(r)) = (v, reach(it, boxes).filter(|r| *r <= BIG_REACH)) else { continue };
            let Some([x, y, depth]) = v.project(origin(it)) else { continue };
            if x < 0.0 || x > w || y < 0.0 || y > h {
                continue;
            }
            by.entry((called(it), raea_core_scene::spawn_sig(it))).or_default().push((k, r * v.focal / depth, [x, y]));
        }
        for (stem, n) in links {
            let c = chains.entry(stem).or_insert((k, n, n));
            c.1 = c.1.min(n);
            c.2 = c.2.max(n);
        }
    }

    /// SG12 over the frames given.
    pub(crate) fn check(self, st: &Stage) -> Vec<Finding> {
        let (by, chains) = (self.by, self.chains);
        let mut out: Vec<(usize, Finding)> = Vec::new();
        for ((name, _), seen) in &by {
            // a name several items carry in one frame has no one size to follow
            let frames: std::collections::HashSet<usize> = seen.iter().map(|s| s.0).collect();
            if frames.len() < seen.len() {
                continue;
            }
            // a step's size change (log ratio) and travel (share of the width), for consecutive frames in one shot
            let step = |i: usize| -> Option<(f32, f32)> {
                let (a, b) = (seen.get(i.checked_sub(1)?)?, seen.get(i)?);
                (b.0 == a.0 + 1 && !st.cut[b.0]).then(|| ((b.1 / a.1.max(1e-3)).ln().abs(), ((b.2[0] - a.2[0]).powi(2) + (b.2[1] - a.2[1]).powi(2)).sqrt() / st.w))
            };
            for i in 1..seen.len() {
                // steady frames on both sides: a part coming in (a pop, a fade up) has none before it
                let (Some((ds, dm)), Some(before), Some(after)) = (step(i), step(i - 1), step(i + 1)) else { continue };
                let side = |f: fn((f32, f32)) -> f32| f(before).max(f(after));
                let k = seen[i].0;
                if ds > SNAP_SIZE.ln() && ds > 3.0 * side(|s| s.0) && seen[i].1.max(seen[i - 1].1) > 4.0 {
                    let says = format!("'{name}' snaps from {:.0} px to {:.0} px on screen at {} with no cut", seen[i - 1].1, seen[i].1, st.at(k));
                    out.push((k, st.finding("part-snap", Level::Error, k - 1, k, says).fix("animate the change over frames, or cut to it; a part that comes in a frame late or at another's place is drawn from a stale frame (raea render's frame purity)")));
                } else if dm > SNAP_MOVE && dm > 3.0 * side(|s| s.1) {
                    let says = format!("'{name}' jumps {:.0}% of the frame's width at {} with no cut", dm * 100.0, st.at(k));
                    out.push((k, st.finding("part-snap", Level::Warn, k - 1, k, says).fix("move it over frames, or cut; check it isn't placed from another part's frame")));
                }
            }
        }
        let mut at: HashMap<usize, usize> = HashMap::new();
        for (k, _) in &out {
            *at.entry(*k).or_default() += 1;
        }
        let mut out: Vec<Finding> = out.into_iter().filter(|(k, _)| at[k] < EDIT_PARTS).map(|(_, f)| f).collect();
        for (stem, (k, least, most)) in chains {
            if most > CHAIN && most > least {
                let says = format!("a line made of {most} parts ('{stem}-0' to '{stem}-{}'), mounted a part at a time as it grows", most - 1);
                out.push(st.finding("line-of-parts", Level::Warn, k, k, says).fix("draw it as one Line3d or Tube (raea/mesh) and animate its `p`"));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_items::{cam, item};
    use super::super::*;
    use bevy::math::Mat4;
    use serde_json::json;

    fn run(frames: Vec<Vec<Item>>) -> Vec<Finding> {
        let n: Vec<i64> = (0..frames.len() as i64).collect();
        check(&frames, &n, 1920, 1080, 30.0, "test")
    }

    fn eased(t: f32) -> f32 {
        t * t * (3.0 - 2.0 * t)
    }

    #[test]
    fn a_smooth_dolly_passes_and_a_kinked_one_lurches() {
        let smooth: Vec<Vec<Item>> = (0..90).map(|i| vec![cam([0.0, 0.0, 10.0 - 6.0 * eased(i as f32 / 89.0)], [0.0; 3])]).collect();
        assert_eq!(run(smooth).len(), 0);
        // two linear legs, the second three times as fast, joined with no ease: a 3x lurch
        let kinked: Vec<Vec<Item>> = (0..60).map(|i| { let z = if i < 30 { 10.0 - 0.02 * i as f32 } else { 9.4 - 0.06 * (i - 30) as f32 }; vec![cam([0.0, 0.0, z], [0.0, 0.0, -100.0])] }).collect();
        let f = run(kinked);
        assert!(f.iter().any(|f| f.check == "camera-lurch" && f.level == Level::Error), "{f:?}");
    }

    #[test]
    fn a_speed_that_ramps_up_over_a_few_frames_lurches_too() {
        // 2 cm a frame (eased in), then 2 to 8 cm a frame over 8 frames, then 8 (eased out): no frame kinks alone, but
        // its half seconds either side of the ramp differ 3x, as `raea fingerprint things` read it from the picture
        let speed = |i: usize| match i {
            0..15 => 0.02 * eased(i as f32 / 15.0),
            15..50 => 0.02,
            50..58 => 0.02 + 0.06 * (i - 50) as f32 / 8.0,
            58..105 => 0.08,
            _ => 0.08 * (1.0 - eased((i - 105) as f32 / 15.0)),
        };
        let mut z = 20.0;
        let ramp: Vec<Vec<Item>> = (0..120).map(|i| { z -= speed(i); vec![cam([0.0, 0.0, z], [0.0, 0.0, -100.0])] }).collect();
        let f: Vec<Finding> = run(ramp).into_iter().filter(|f| f.check == "camera-lurch").collect();
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].level == Level::Warn && f[0].says.contains("speeds up"), "{}", f[0].says);
    }

    #[test]
    fn a_cut_is_not_a_lurch_and_in_then_out_is_a_reversal() {
        let cut: Vec<Vec<Item>> = (0..60).map(|i| if i < 30 { vec![cam([0.0, 0.0, 10.0 - 0.01 * i as f32], [0.0; 3])] } else { vec![cam([5.0, 2.0, 3.0 - 0.01 * (i - 30) as f32], [5.0, 2.0, -10.0])] }).collect();
        assert_eq!(run(cut).iter().filter(|f| f.check.starts_with("camera")).count(), 0);
        // in 0.5 m over 15 frames, out 0.5 m over the next 15
        let back: Vec<Vec<Item>> = (0..40).map(|i| { let t = i.min(30) as f32; let z = if t <= 15.0 { 10.0 - 0.5 * eased(t / 15.0) } else { 9.5 + 0.5 * eased((t - 15.0) / 15.0) }; vec![cam([0.0, 0.0, z], [0.0, 0.0, -100.0])] }).collect();
        assert!(run(back).iter().any(|f| f.check == "camera-reverse"));
    }

    #[test]
    fn a_part_that_snaps_is_said_and_one_that_grows_isnt() {
        let dot = |s: f32| item("mesh", json!({"name": "dot"}), Mat4::from_scale(Vec3::splat(s)));
        let shaped = |s: f32| { let mut d = dot(s); d.shape = Some("sphere(0.1)".into()); d };
        let snap: Vec<Vec<Item>> = (0..20).map(|i| vec![cam([0.0, 0.0, 5.0], [0.0; 3]), shaped(if i < 10 { 1.0 } else { 1.6 })]).collect();
        assert!(run(snap).iter().any(|f| f.check == "part-snap"), "a 60% jump in one frame");
        let grow: Vec<Vec<Item>> = (0..20).map(|i| vec![cam([0.0, 0.0, 5.0], [0.0; 3]), shaped(1.0 + 0.05 * i as f32)]).collect();
        assert!(!run(grow).iter().any(|f| f.check == "part-snap"));
    }

    #[test]
    fn a_pop_in_two_long_moves_and_a_shared_name_are_not_faults() {
        let shaped = |name: &str, s: f32, x: f32| { let mut d = item("mesh", json!({"name": name}), Mat4::from_scale_rotation_translation(Vec3::splat(s), Default::default(), Vec3::new(x, 0.0, 0.0))); d.shape = Some("sphere(0.1)".into()); d };
        // absent, then in at 0.7 and up to full size: no steady frames before it
        let pop: Vec<Vec<Item>> = (0..20).map(|i| { let mut v = vec![cam([0.0, 0.0, 5.0], [0.0; 3])]; if i >= 10 { v.push(shaped("dot", if i == 10 { 0.7 } else { 1.0 }, 0.0)); } v }).collect();
        assert!(!run(pop).iter().any(|f| f.check == "part-snap"));
        // two items named alike, far apart: not one part jumping between them
        let twins: Vec<Vec<Item>> = (0..20).map(|_| vec![cam([0.0, 0.0, 5.0], [0.0; 3]), shaped("tile", 1.0, -1.5), shaped("tile", 1.6, 1.5)]).collect();
        assert!(!run(twins).iter().any(|f| f.check == "part-snap"));
        // its shape (or words) changed: spawned again as new content (SG1), not a snap
        let regrown: Vec<Vec<Item>> = (0..20).map(|i| { let mut d = shaped("count", 1.0, 0.0); d.shape = Some(if i < 10 { "sphere(0.1)" } else { "sphere(0.16)" }.into()); vec![cam([0.0, 0.0, 5.0], [0.0; 3]), d] }).collect();
        assert!(!run(regrown).iter().any(|f| f.check == "part-snap"));
        // out 2 m over 3 s, then in 2 m over 3 s: two moves, not a reversal
        let long: Vec<Vec<Item>> = (0..180).map(|i| { let t = i as f32 / 90.0; let z = if t <= 1.0 { 8.0 + 2.0 * eased(t) } else { 10.0 - 2.0 * eased(t - 1.0) }; vec![cam([0.0, 0.0, z], [0.0, 0.0, -100.0])] }).collect();
        assert!(!run(long).iter().any(|f| f.check == "camera-reverse"));
    }

    #[test]
    fn a_growing_chain_of_parts_is_a_line_made_of_parts() {
        let chain: Vec<Vec<Item>> = (0..20).map(|i| { let mut v = vec![cam([0.0, 0.0, 5.0], [0.0; 3])]; v.extend((0..=i).map(|j| item("mesh", json!({"name": format!("curve-{j}")}), Mat4::IDENTITY))); v }).collect();
        assert!(run(chain).iter().any(|f| f.check == "line-of-parts"));
    }
}
