//! raea-runtime-lod-plan (docs/make.feature MK21), planned detail held to the frame: a part planned as a grey box or as something far away (`lod` on the part,
//! or on its object in the plan, `make` on brief.ts) that a shot then shows closer than its plan. A plan's level is how
//! large the part is meant to be seen, 0 (the hero, half the frame or more) to 4 (far, under 2% of it); a frame's level
//! is how large the part's box is on screen through that frame's camera. A part seen finer than planned was built for
//! a distance the camera no longer keeps: a warning one level finer, an error two or more. It reads the scene, not the
//! pixels: a part behind others is passed over only when every line from the camera to it crosses another mesh's box
//! (a hollow shape's box hides more than the shape does, so a part behind one may be missed; one in the open never is).

use std::collections::{BTreeMap, HashMap};

use bevy::math::Vec3;
use raea_core_facts::{Finding, Level};
use raea_core_scene::base_ident;
use serde_json::Value;

use raea_core_scene::Item;
use raea_sys_stage::{View, called, corners, crossing, model_box, own_box, see_through};

// the levels a job's plan gives its objects (`lod` on brief.ts's make objects, MK22)
pub mod plan;
// the gate a job is held at, and the table `raea scene lod` prints
mod report;
pub use report::{gate, held, ordered, table};

/// The coarsest level: far, a silhouette.
pub const COARSEST: u8 = 4;
/// SPANS[k]: a part across this share of the frame or more (the larger of its width's and its height's) is seen at
/// level k; under the last it is at level 4. Unity's and Unreal's levels switch on the same measure, size on screen.
pub const SPANS: [f32; 4] = [0.5, 0.2, 0.08, 0.02];
/// What each level is for.
pub const NAMES: [&str; 5] = ["hero", "featured", "dressing", "background", "far"];

/// The level a part across `span` of the frame is seen at.
pub fn level(span: f32) -> u8 {
    SPANS.iter().position(|s| span >= *s).unwrap_or(SPANS.len()) as u8
}

/// A level in words: "LOD 3 (background, under 8% of the frame)".
pub fn words(l: u8) -> String {
    let l = l.min(COARSEST) as usize;
    let size = if l == 0 { format!("{:.0}% of the frame or more", SPANS[0] * 100.0) } else { format!("under {:.0}% of the frame", SPANS[l - 1] * 100.0) };
    format!("LOD {l} ({}, {size})", NAMES[l])
}

/// A part's planned level: its own `lod`, else its name's in the plan.
fn planned(it: &Item, plan: &HashMap<String, u8>) -> Option<u8> {
    let own = it.props.get("lod").and_then(Value::as_f64).filter(|l| *l >= 0.0).map(|l| (l as u8).min(COARSEST));
    own.or_else(|| plan.get(base_ident(it)).copied())
}

/// How much of the frame a part's box spans through `v` (the larger of its width's and its height's share, 1 at
/// most), and the box's corners; None when its size isn't known or none of it is in the frame.
fn span(it: &Item, v: &View, boxes: &mut HashMap<String, Option<(Vec3, Vec3)>>) -> Option<(f32, [Vec3; 8])> {
    let (lo, hi) = match it.kind {
        "mesh" => own_box(it, boxes)?,
        "model" => model_box(it, boxes)?,
        _ => return None,
    };
    let pts = corners(&it.world, lo, hi);
    if let Some(b) = v.bounds(pts) {
        return (v.share(b) > 0.0).then(|| (((b[2] - b[0]) / v.w).max((b[3] - b[1]) / v.h).min(1.0), pts));
    }
    // across the camera's plane: the camera is in it or beside it, so it is measured at its nearest point
    let near = it.world.transform_point3(it.world.inverse().transform_point3(v.eye).clamp(lo, hi));
    if near.distance(v.eye) < 1e-4 {
        return Some((1.0, pts));
    }
    let [x, y, depth] = v.project(near)?;
    (x >= 0.0 && x <= v.w && y >= 0.0 && y <= v.h).then(|| (((pts[7] - pts[0]).length() * v.focal / depth / v.h).min(1.0), pts))
}

/// Whether other parts stand between the camera and all of a part: its box's middle and eight points halfway out to
/// its corners, each behind a mesh's box the line passes right through (not one the camera or the point is inside: a
/// room round either hides nothing). Models never hide: a file's box is mostly air.
fn hidden(its: &[Item], me: usize, pts: &[Vec3; 8], eye: Vec3, boxes: &mut HashMap<String, Option<(Vec3, Vec3)>>) -> bool {
    let mid = pts.iter().copied().sum::<Vec3>() / 8.0;
    let targets: Vec<Vec3> = std::iter::once(mid).chain(pts.iter().map(|p| mid + (*p - mid) * 0.5)).collect();
    let mut others = Vec::new();
    for (j, it) in its.iter().enumerate() {
        if j == me || it.kind != "mesh" || it.at.is_some() || see_through(it) {
            continue;
        }
        if let Some((lo, hi)) = own_box(it, boxes) {
            others.push((it.world.inverse(), lo, hi));
        }
    }
    targets.iter().all(|p| others.iter().any(|(inv, lo, hi)| crossing(inv.transform_point3(eye), inv.transform_point3(*p), *lo, *hi).is_some_and(|(t0, t1)| t0 > 0.0 && t1 < 1.0)))
}

/// What the frames showed of one part.
#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    pub name: String,
    pub planned: Option<u8>,
    /// how many frames it was in
    pub frames: usize,
    /// where it was largest: the frame's number, and the share of the frame it spanned there
    pub finest: Option<(i64, f32)>,
    /// how many frames showed it finer than planned
    pub closer: usize,
}

impl Seen {
    /// The finest level it was seen at.
    pub fn level(&self) -> Option<u8> {
        self.finest.map(|(_, s)| level(s))
    }
}

#[derive(Default)]
struct Part {
    planned: Option<u8>,
    frames: usize,
    /// its largest: (frame, span)
    finest: Option<(i64, f32)>,
    /// the frames it was seen finer than planned: (frame, span)
    closer: Vec<(i64, f32)>,
}

/// Each frame as it comes: per part, how often it was in the frame, where it was largest, and the frames it was seen
/// finer than its plan. A render keeps only the parts with a plan; `every` keeps all (`raea scene lod`'s table).
pub struct Lods {
    plan: HashMap<String, u8>,
    every: bool,
    w: u32,
    h: u32,
    fps: f64,
    boxes: HashMap<String, Option<(Vec3, Vec3)>>,
    parts: BTreeMap<String, Part>,
}

impl Lods {
    /// For frames w x h at `fps`. `plan` is the levels the job's plan gives its objects, by name (`lod` on brief.ts's
    /// make objects); a part's own `lod` comes first.
    pub fn new(w: u32, h: u32, fps: f64, plan: HashMap<String, u8>, every: bool) -> Lods {
        Lods { plan, every, w, h, fps: fps.max(1.0), boxes: HashMap::new(), parts: BTreeMap::new() }
    }

    /// The next frame's parts, through its camera.
    pub fn push(&mut self, frame: i64, its: &[Item]) {
        if self.plan.is_empty() && !self.every && !its.iter().any(|it| it.props.contains_key("lod")) {
            return;
        }
        let Some(v) = View::of(its, self.w, self.h) else { return };
        for (j, it) in its.iter().enumerate() {
            if !matches!(it.kind, "mesh" | "model") || it.at.is_some() {
                continue;
            }
            let plan = planned(it, &self.plan);
            if plan.is_none() && !self.every {
                continue;
            }
            let Some((s, pts)) = span(it, &v, &mut self.boxes) else { continue };
            let closer = plan.is_some_and(|p| level(s) < p);
            if closer && hidden(its, j, &pts, v.eye, &mut self.boxes) {
                continue;
            }
            // parts of one name planned apart (two crates, one a hero) are told apart
            let mut name = called(it);
            if self.parts.get(&name).is_some_and(|p| p.planned != plan) {
                name = format!("{name} (lod {})", plan.map_or("none".into(), |p| p.to_string()));
            }
            let part = self.parts.entry(name).or_default();
            part.planned = plan;
            part.frames += 1;
            if part.finest.is_none_or(|(_, best)| s > best) {
                part.finest = Some((frame, s));
            }
            if closer {
                part.closer.push((frame, s));
            }
        }
    }

    /// What the frames showed of each part, by name.
    pub fn seen(&self) -> Vec<Seen> {
        self.parts.iter().map(|(name, p)| Seen { name: name.clone(), planned: p.planned, frames: p.frames, finest: p.finest, closer: p.closer.len() }).collect()
    }

    /// One finding a part seen finer than planned, worst first: where it was largest, and for how long.
    pub fn findings(&self, by: &str) -> Vec<Finding> {
        let mut out = Vec::new();
        for (name, p) in &self.parts {
            let (Some(plan), Some(&(first, _)), Some(&(last, _))) = (p.planned, p.closer.first(), p.closer.last()) else { continue };
            let Some(&(at, s)) = p.closer.iter().max_by(|a, b| a.1.total_cmp(&b.1)) else { continue };
            let seen = level(s);
            let lasts = if p.closer.len() == 1 { "on one frame".to_string() } else { format!("on {} frames ({:.1} s)", p.closer.len(), p.closer.len() as f64 / self.fps) };
            let says = format!("'{name}' is planned at {} and is seen at {}: across {:.0}% of the frame at frame {at}, closer than planned {lasts}", words(plan), words(seen), s * 100.0);
            let fix = format!("build it for LOD {seen} and plan it there (lod={{{seen}}} on the part, or `lod: {seen}` on its object in brief.ts), or keep the camera off it");
            let level = if plan - seen >= 2 { Level::Error } else { Level::Warn };
            out.push(Finding::new("lod-closer", level, says, by).frames(first, last + 1).about(name.clone(), None).measure("span", (s * 100.0) as f64, "% of the frame", Some((SPANS[plan as usize - 1] * 100.0) as f64)).fix(fix));
        }
        raea_core_facts::finding::ranked(out)
    }
}

/// Every part of the frames against its planned detail (`all[k]` is frame `frames[k]`'s parts): each part's row, and a
/// finding a part seen finer than planned.
pub fn lods(all: &[Vec<Item>], frames: &[i64], w: u32, h: u32, fps: f64, by: &str, plan: HashMap<String, u8>) -> (Vec<Seen>, Vec<Finding>) {
    let mut l = Lods::new(w, h, fps, plan, true);
    for (k, its) in all.iter().enumerate() {
        l.push(frames[k], its);
    }
    (l.seen(), l.findings(by))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Mat4;
    use serde_json::json;
    use std::sync::Arc;

    fn item(kind: &'static str, props: Value, world: Mat4) -> Item {
        Item { kind, props: Arc::new(props.as_object().cloned().unwrap_or_default()), shape: None, at: None, material: None, world }
    }

    /// A camera at `eye` looking at `at`, 50 degrees.
    fn cam(eye: [f32; 3], at: [f32; 3]) -> Item {
        item("camera", json!({"position": eye, "lookAt": at, "fov": 50}), Mat4::IDENTITY)
    }

    /// a 1 m cube at `at`, with these props
    fn cube(props: Value, at: [f32; 3]) -> Item {
        let mut it = item("mesh", props, Mat4::from_translation(Vec3::from(at)));
        it.shape = Some(Arc::from("box(1, 1, 1)"));
        it
    }

    fn run(frames: Vec<Vec<Item>>, plan: &[(&str, u8)]) -> (Vec<Seen>, Vec<Finding>) {
        let nums: Vec<i64> = (0..frames.len() as i64).collect();
        lods(&frames, &nums, 1920, 1080, 30.0, "test", plan.iter().map(|(n, l)| (n.to_string(), *l)).collect())
    }

    #[test]
    fn a_level_is_how_much_of_the_frame_a_part_spans() {
        assert_eq!([0.9, 0.5, 0.3, 0.1, 0.05, 0.01].map(level), [0, 0, 1, 2, 3, 4]);
        assert_eq!(words(3), "LOD 3 (background, under 8% of the frame)");
        assert_eq!(words(0), "LOD 0 (hero, 50% of the frame or more)");
    }

    #[test]
    fn a_far_part_the_camera_comes_up_to_is_said_with_where_and_how_close() {
        // planned far (4); the camera dollies from 60 m to 2 m
        let frames: Vec<Vec<Item>> = [60.0, 30.0, 8.0, 2.0].iter().map(|z| vec![cam([0.0, 0.0, *z], [0.0, 0.0, 0.0]), cube(json!({"name": "crate", "lod": 4}), [0.0, 0.0, 0.0])]).collect();
        let (seen, found) = run(frames, &[]);
        assert_eq!((seen[0].planned, seen[0].level(), seen[0].closer), (Some(4), Some(0), 3), "{seen:?}");
        assert_eq!(seen[0].finest.map(|f| f.0), Some(3));
        let f = &found[0];
        assert_eq!((f.check.as_str(), f.level, f.frames), ("lod-closer", Level::Error, Some([1, 4])), "{f:?}");
        assert!(f.says.starts_with("'crate' is planned at LOD 4 (far, under 2% of the frame) and is seen at LOD 0 (hero") && f.says.contains("at frame 3, closer than planned on 3 frames (0.1 s)"), "{}", f.says);
        assert!(f.fix.as_deref().unwrap().contains("lod={0}"), "{:?}", f.fix);
    }

    #[test]
    fn one_level_finer_warns_and_a_part_at_its_plan_says_nothing() {
        // a 1 m cube 5 m off a 50 degree lens is about 23% of the frame's height: level 1
        let scene = |lod: u8| vec![vec![cam([0.0, 0.0, 5.5], [0.0, 0.0, 0.0]), cube(json!({"name": "crate", "lod": lod}), [0.0, 0.0, 0.0])]];
        assert!(run(scene(1), &[]).1.is_empty() && run(scene(0), &[]).1.is_empty());
        assert_eq!(run(scene(2), &[]).1[0].level, Level::Warn);
        assert_eq!(run(scene(3), &[]).1[0].level, Level::Error);
    }

    #[test]
    fn the_plan_names_a_part_and_its_own_lod_wins() {
        let scene = |props: Value| vec![vec![cam([0.0, 0.0, 3.0], [0.0, 0.0, 0.0]), cube(props, [0.0, 0.0, 0.0])]];
        let (seen, found) = run(scene(json!({"name": "kiosk"})), &[("kiosk", 3)]);
        assert_eq!((seen[0].planned, found.len()), (Some(3), 1));
        assert!(run(scene(json!({"name": "kiosk", "lod": 0})), &[("kiosk", 3)]).1.is_empty(), "the part's own level is the newer word");
        // a part with no plan is never a finding; the table still has its row, and a render doesn't keep it
        let (seen, found) = run(scene(json!({"name": "other"})), &[("kiosk", 3)]);
        assert_eq!((seen[0].planned, seen[0].level(), found.len()), (None, Some(1), 0));
        let mut l = Lods::new(1920, 1080, 30.0, HashMap::from([("kiosk".to_string(), 3)]), false);
        l.push(0, &scene(json!({"name": "other"}))[0]);
        assert!(l.seen().is_empty());
    }

    #[test]
    fn a_part_out_of_frame_behind_the_camera_or_behind_a_wall_is_not_seen() {
        let far = |at: [f32; 3]| cube(json!({"name": "crate", "lod": 4}), at);
        // beside the frame, and behind the camera
        assert!(run(vec![vec![cam([0.0, 0.0, 3.0], [0.0, 0.0, 0.0]), far([40.0, 0.0, 0.0])]], &[]).1.is_empty());
        assert!(run(vec![vec![cam([0.0, 0.0, 3.0], [0.0, 0.0, 0.0]), far([0.0, 0.0, 9.0])]], &[]).1.is_empty());
        // a wall wider than it between the two hides it; a post narrower than it doesn't
        let mut wall = cube(json!({"name": "wall"}), [0.0, 0.0, 1.5]);
        wall.world = Mat4::from_translation(Vec3::new(0.0, 0.0, 1.5)) * Mat4::from_scale(Vec3::new(6.0, 6.0, 0.2));
        assert!(run(vec![vec![cam([0.0, 0.0, 3.0], [0.0, 0.0, 0.0]), far([0.0, 0.0, 0.0]), wall.clone()]], &[]).1.is_empty());
        wall.world = Mat4::from_translation(Vec3::new(0.0, 0.0, 1.5)) * Mat4::from_scale(Vec3::new(0.1, 6.0, 0.1));
        assert_eq!(run(vec![vec![cam([0.0, 0.0, 3.0], [0.0, 0.0, 0.0]), far([0.0, 0.0, 0.0]), wall]], &[]).1.len(), 1);
        // a room round both hides nothing
        let mut room = cube(json!({"name": "room"}), [0.0, 0.0, 0.0]);
        room.world = Mat4::from_scale(Vec3::splat(20.0));
        assert_eq!(run(vec![vec![cam([0.0, 0.0, 3.0], [0.0, 0.0, 0.0]), far([0.0, 0.0, 0.0]), room]], &[]).1.len(), 1);
    }

    #[test]
    fn a_camera_inside_or_beside_a_part_sees_it_at_its_nearest() {
        // a 30 m hall the camera stands in, planned as background
        let mut hall = cube(json!({"name": "hall", "lod": 3}), [0.0, 0.0, 0.0]);
        hall.world = Mat4::from_scale(Vec3::new(30.0, 4.0, 30.0));
        let (seen, found) = run(vec![vec![cam([0.0, 0.0, 3.0], [0.0, 0.0, 0.0]), hall]], &[]);
        assert_eq!((seen[0].level(), found[0].level), (Some(0), Level::Error));
    }
}
