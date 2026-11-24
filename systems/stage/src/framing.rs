//! SG14, the frame is composed: frames where almost nothing of the scene is on screen, and the part a shot is about
//! cut by the frame's edge. Both are warnings while they are measured on more films: the page's own 2D (a headline over
//! an empty 3D) isn't known to them.

use std::collections::HashMap;

use bevy::math::Vec3;
use raea_core_facts::{Finding, Level};

use super::{Item, Stage, called, num, runs, scale_of};
use crate::view::{View, corners};

/// Under this share of the frame showing any part is an empty frame
const EMPTY: f32 = 0.08;
/// for this long, seconds; and not this near the piece's start or end (a fade)
const EMPTY_SECS: f64 = 1.0;
const FADE_SECS: f64 = 0.25;
/// The shot's subject cut: between these shares of its box outside the frame, for more than this many frames
const CUT_LO: f32 = 0.15;
const CUT_HI: f32 = 0.85;
const CUT_FRAMES: usize = 10;
/// A backdrop, not content: a slab wider than this in the world (a floor, a set's wall, a sky plane), its thinnest side
/// under SLAB of its widest; a frame of floor and fog is still an empty one
const BACKDROP_M: f32 = 12.0;
const SLAB: f32 = 0.05;
/// The grid the frame's coverage is counted on
const GX: usize = 48;
const GY: usize = 27;

/// A part's box in its own space: a shape's, a block of words' (estimated), a picture's; None when it isn't known.
pub fn own_box(it: &raea_core_scene::Item, boxes: &mut HashMap<String, Option<(Vec3, Vec3)>>) -> Option<(Vec3, Vec3)> {
    match it.kind {
        "mesh" => {
            let shape = it.shape.as_deref()?;
            *boxes.entry(shape.to_string()).or_insert_with(|| span(raea_core_shape::cached(shape).ok()?.positions.iter().copied()))
        }
        "text" => {
            let words = it.props.get("text").and_then(serde_json::Value::as_str).unwrap_or("");
            let size = num(it, "size", 0.5) as f32;
            let w = words.split('\n').map(|l| l.chars().count()).max().unwrap_or(0) as f32 * 0.55 * size;
            let h = words.split('\n').count() as f32 * 1.2 * size;
            Some((Vec3::new(-w / 2.0, -h / 2.0, 0.0), Vec3::new(w / 2.0, h / 2.0, 0.0)))
        }
        // a picture on a card: a unit quad scaled by its props (ImagePlane, Screen)
        "panel" => Some((Vec3::new(-0.5, -0.5, 0.0), Vec3::new(0.5, 0.5, 0.0))),
        _ => None,
    }
}

/// A slab wider than BACKDROP_M in the world, its thinnest side under SLAB of its widest: a floor, a set's wall, a sky
/// plane, the backdrop a frame's parts stand in front of, not one of them.
pub(crate) fn backdrop(it: &raea_core_scene::Item, lo: Vec3, hi: Vec3) -> bool {
    let ext = (hi - lo) * it.world.to_scale_rotation_translation().0.abs();
    ext.max_element() > BACKDROP_M && ext.min_element() < SLAB * ext.max_element()
}

/// A model's box in its own space, from its file's triangles (each file read once a render); None for one whose file
/// animates or can't be read. Coverage only, never sure: raea may pose its joints (a person walks off its rest box), and
/// a box is mostly air round a tree, so it never makes a frame judged, nor judges words over it (SG11); but a frame
/// its box fills isn't empty (Stay's pool room of wall models read empty with their origins out of view: SG14).
pub fn model_box(it: &raea_core_scene::Item, boxes: &mut HashMap<String, Option<(Vec3, Vec3)>>) -> Option<(Vec3, Vec3)> {
    let src = raea_core_scene::src_path(it.props.get("src").and_then(serde_json::Value::as_str)?);
    *boxes.entry(format!("model:{src}")).or_insert_with(|| {
        let (groups, moves) = raea_core_scene::gltf_tris(&src).ok()?;
        if moves {
            return None;
        }
        let mut points = Vec::new();
        for (_, tris) in &groups {
            points.extend(tris.iter().flatten().copied());
        }
        span(points)
    })
}

/// The box round some points, None for none.
fn span<P: Into<Vec3>>(points: impl IntoIterator<Item = P>) -> Option<(Vec3, Vec3)> {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in points {
        let p = p.into();
        lo = lo.min(p);
        hi = hi.max(p);
    }
    (lo.x <= hi.x).then_some((lo, hi))
}

/// A backdrop slab standing up, its thin side facing within 45 degrees of level: a wall, not a floor, water or a
/// ceiling. A frame of floor and fog is empty; one of a room's walls or a canyon's is not (Stay's pool room and canyon
/// read empty when every slab was backdrop: SG14).
fn standing(it: &raea_core_scene::Item, lo: Vec3, hi: Vec3) -> bool {
    let (scale, turn, _) = it.world.to_scale_rotation_translation();
    let ext = (hi - lo) * scale.abs();
    let thin = if ext.x <= ext.y && ext.x <= ext.z { Vec3::X } else if ext.y <= ext.z { Vec3::Y } else { Vec3::Z };
    (turn * thin).y.abs() < std::f32::consts::FRAC_1_SQRT_2
}

/// Each part of known size that shows in a frame: its name as the checks call it, and its box on screen, [x0, y0, x1,
/// y1] in px (SG9 names the parts under a patch that differs between a frame rendered in sequence and alone).
pub fn on_screen(its: &[raea_core_scene::Item], w: u32, h: u32) -> Vec<(String, [f32; 4])> {
    let Some(v) = crate::view::View::of(its, w, h) else { return Vec::new() };
    let mut boxes = HashMap::new();
    let seen = |it: &raea_core_scene::Item, boxes: &mut HashMap<_, _>| {
        let (lo, hi) = own_box(it, boxes)?;
        v.bounds(corners(&it.world, lo, hi)).filter(|b| v.share(*b) > 0.0)
    };
    its.iter().filter_map(|it| Some((called(it), seen(it, &mut boxes)?))).collect()
}

/// The coverage grid's cells under a box on screen ([x0, y0, x1, y1] px, clipped to the w x h frame), set.
fn mark(grid: &mut [bool], b: [f32; 4], w: f32, h: f32) {
    let (x0, y0) = ((b[0].max(0.0) / w * GX as f32) as usize, (b[1].max(0.0) / h * GY as f32) as usize);
    let (x1, y1) = (((b[2].min(w) / w * GX as f32).ceil() as usize).min(GX), ((b[3].min(h) / h * GY as f32).ceil() as usize).min(GY));
    for y in y0..y1 {
        for x in x0..x1 {
            grid[y * GX + x] = true;
        }
    }
}

/// Each frame as it comes: the share covered, whether it can be judged (no part of unknown size in view, no footage or
/// 2D over the 3D), and its subject: (name, the share of its box outside the frame, whether it could fit, whether a
/// Label names it).
#[derive(Default)]
pub(crate) struct Framing {
    boxes: HashMap<String, Option<(Vec3, Vec3)>>,
    covered: Vec<f32>,
    judged: Vec<bool>,
    subject: Vec<Option<Subject>>,
}

/// A frame's subject: what it's called, the share of its box outside the frame, whether it could fit, and whether a
/// Label names it (the part nearest the Label's point; else the named part showing most, a guess)
type Subject = (String, f32, bool, bool);

impl Framing {
    /// A frame's parts through its camera `v`, and what its page draws over them, on a w x h frame.
    pub(crate) fn push(&mut self, _k: usize, its: &[Item], v: Option<View>, page: Option<&crate::Page>, w: f32, h: f32) {
        let (c, j, s) = v.map_or((1.0, false, None), |v| self.frame(its, v, page, w, h));
        self.covered.push(c);
        self.judged.push(j);
        self.subject.push(s);
    }

    fn frame(&mut self, its: &[Item], v: View, page: Option<&crate::Page>, w: f32, h: f32) -> (f32, bool, Option<Subject>) {
        let boxes = &mut self.boxes;
        let mut grid = vec![false; GX * GY];
        let mut sure = true;
        let mut best: Option<(String, f32, f32, bool)> = None;
        // the points the page's Labels name, and the named part nearest one: (name, its distance, out, fits)
        let points: Vec<Vec3> = page.into_iter().flat_map(|p| p.words.iter().filter_map(|w| w.points.map(Vec3::from))).collect();
        let mut named: Option<(String, f32, f32, bool)> = None;
        for it in its {
            // drawn where the boxes can't say: footage, 2D, a field, a shader over the frame, a set of copies (<Instances>)
            if matches!(it.kind, "plate" | "sprite" | "shape2d" | "grass" | "splat" | "meshseq" | "frame_pass") || it.at.is_some() {
                sure = false;
                continue;
            }
            let Some((lo, hi)) = own_box(it, boxes) else {
                // a model or other part of unknown size: judged only when its origin is well out of view
                if matches!(it.kind, "model" | "entity" | "fire") {
                    if let Some([x, y, _]) = v.project(super::origin(it)) {
                        if x > -0.5 * w && x < 1.5 * w && y > -0.5 * h && y < 1.5 * h {
                            sure = false;
                        }
                    }
                }
                // its file's box fills what it shows, a floor's or terrain's left out as a slab mesh's is; across the
                // camera's plane (a room model round it: Stay's pool) it may fill the view
                let model = (it.kind == "model").then(|| model_box(it, boxes)).flatten();
                if let Some((lo, hi)) = model.filter(|(lo, hi)| !backdrop(it, *lo, *hi) || standing(it, *lo, *hi)) {
                    match v.bounds(corners(&it.world, lo, hi)) {
                        Some(b) if v.share(b) > 0.0 => mark(&mut grid, b, w, h),
                        Some(_) => {}
                        None => sure &= corners(&it.world, lo, hi).into_iter().all(|p| v.project(p).is_none()),
                    }
                }
                continue;
            };
            // a floor, water or a ceiling round the camera is backdrop before it is a part behind it; a wall (a slab
            // standing up: a room's, a canyon's) is what the frame shows, so it fills the frame, but is never its subject
            let wall = backdrop(it, lo, hi) && standing(it, lo, hi);
            if backdrop(it, lo, hi) && !wall {
                continue;
            }
            let Some(b) = v.bounds(corners(&it.world, lo, hi)) else {
                // across the camera's plane it may fill the view; wholly behind it, it's out of view
                sure &= corners(&it.world, lo, hi).into_iter().all(|p| v.project(p).is_none());
                continue;
            };
            let inside = v.share(b);
            let whole = ((b[2] - b[0]) * (b[3] - b[1]) / (w * h)).max(1e-9);
            if inside > 0.0 {
                mark(&mut grid, b, w, h);
            }
            // a wall fills the frame but isn't its subject
            if wall {
                continue;
            }
            // the subject: the named part showing most, when it could fit in the frame
            if it.props.get("name").is_some() && inside > 0.0 {
                let fits = b[2] - b[0] < 0.9 * w && b[3] - b[1] < 0.9 * h && scale_of(it) > 0.0;
                if best.as_ref().is_none_or(|s| inside > s.1) {
                    best = Some((called(it), inside, 1.0 - inside / whole, fits));
                }
                // a Label's point is on or near its part (its top, as Label asks): within a quarter of the part's size
                // and 0.3 m
                if !points.is_empty() {
                    let (lo, hi) = corners(&it.world, lo, hi).into_iter().fold((Vec3::MAX, Vec3::MIN), |(a, b), p| (a.min(p), b.max(p)));
                    let d = points.iter().map(|p| (lo - *p).max(*p - hi).max(Vec3::ZERO).length()).fold(f32::MAX, f32::min);
                    if d <= 0.25 * (hi - lo).length() + 0.3 && named.as_ref().is_none_or(|s| d < s.1) {
                        named = Some((called(it), d, 1.0 - inside / whole, fits));
                    }
                }
            }
        }
        // the page's words and videos fill the frame too: a headline over an empty 3D isn't an empty frame
        for b in page.into_iter().flat_map(|p| p.words.iter().map(|w| w.bx).chain(p.videos.iter().copied()).chain(p.pictures.iter().copied())) {
            mark(&mut grid, b, w, h);
        }
        let subject = named.map(|(name, _, out, fits)| (name, out, fits, true)).or(best.map(|(name, _, out, fits)| (name, out, fits, false)));
        (grid.iter().filter(|c| **c).count() as f32 / (GX * GY) as f32, sure, subject)
    }

    /// SG14 over the frames given.
    pub(crate) fn check(self, st: &Stage) -> Vec<Finding> {
        let (n, covered, judged, subject) = (st.len(), self.covered, self.judged, self.subject);
        let mut out = Vec::new();
        let total = st.secs(0, n.saturating_sub(1).max(0));
        for (a, b) in runs(st, n, |k| judged[k] && covered[k] < EMPTY) {
            let (t0, t1) = ((st.frames[a] - st.frames[0]) as f64 / st.fps, (st.frames[b] - st.frames[0] + 1) as f64 / st.fps);
            if st.secs(a, b) >= EMPTY_SECS && t0 > FADE_SECS && t1 < total - FADE_SECS {
                let least = covered[a..=b].iter().copied().fold(1.0, f32::min);
                let says = format!("almost nothing is on screen from {} to {} ({:.0}% of the frame shows any part)", st.at(a), st.at(b), least * 100.0);
                out.push(st.finding("frame-empty", Level::Error, a, b, says).fix("aim the camera at what the shot is about, or bring it into the frame"));
            }
        }
        for (a, b) in runs(st, n, |k| subject[k].as_ref().is_some_and(|(_, out, fits, _)| *fits && *out > CUT_LO && *out < CUT_HI)) {
            let names: Vec<&str> = subject[a..=b].iter().filter_map(|s| s.as_ref().map(|s| s.0.as_str())).collect();
            if b + 1 - a > CUT_FRAMES && names.windows(2).all(|w| w[0] == w[1]) {
                // a part a Label names is what the shot is about, so its cut is an error; the part showing most is a
                // guess at the subject, so its cut is a warning
                let labelled = subject[a..=b].iter().all(|s| s.as_ref().is_some_and(|s| s.3));
                let (level, what) = if labelled { (Level::Error, "the part a Label names") } else { (Level::Warn, "the part the shot shows most") };
                let says = format!("'{}', {what}, is cut by the frame's edge from {} to {}", names[0], st.at(a), st.at(b));
                out.push(st.finding("subject-cut", level, a, b, says).fix("frame it whole (move the camera back or aim it), or move in until the cut is plainly meant"));
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

    fn cube(name: &str, at: [f32; 3]) -> Item {
        let mut it = item("mesh", json!({"name": name}), Mat4::from_translation(Vec3::from(at)));
        it.shape = Some("box(1, 1, 1)".into());
        it
    }

    #[test]
    fn a_camera_aimed_past_everything_is_an_empty_frame() {
        let fs: Vec<Vec<Item>> = (0..120).map(|i| vec![cam([0.0, 0.0, 6.0], if (30..90).contains(&i) { [0.0, 40.0, 0.0] } else { [0.0; 3] }), cube("dash", [0.0; 3])]).collect();
        let f = run(fs);
        assert!(f.iter().any(|f| f.check == "frame-empty"), "{f:?}");
        let framed: Vec<Vec<Item>> = (0..120).map(|_| vec![cam([0.0, 0.0, 4.0], [0.0; 3]), cube("dash", [0.0; 3])]).collect();
        assert!(!run(framed).iter().any(|f| f.check == "frame-empty"));
    }

    #[test]
    fn copies_or_a_shader_over_the_frame_leave_it_unjudged() {
        // the empty stretch of the test above, with a forest of copies or a frame pass in it: either may fill the frame
        let aimed = |extra: Item| (0..120).map(|i| vec![cam([0.0, 0.0, 6.0], if (30..90).contains(&i) { [0.0, 40.0, 0.0] } else { [0.0; 3] }), cube("dash", [0.0; 3]), extra.clone()]).collect::<Vec<_>>();
        let mut forest = cube("forest", [0.0, 30.0, 0.0]);
        forest.at = Some("[0,0,0, 0,0,0, 1,1,1]".into());
        assert!(!run(aimed(forest)).iter().any(|f| f.check == "frame-empty"));
        assert!(!run(aimed(item("frame_pass", json!({}), Mat4::IDENTITY))).iter().any(|f| f.check == "frame-empty"));
        let f = run(aimed(item("fog", json!({}), Mat4::IDENTITY)));
        assert!(f.iter().any(|f| f.check == "frame-empty" && f.level == Level::Error), "{f:?}");
    }

    #[test]
    fn words_or_a_video_in_the_page_fill_the_frame() {
        let fs: Vec<Vec<Item>> = (0..120).map(|i| vec![cam([0.0, 0.0, 6.0], if (30..90).contains(&i) { [0.0, 40.0, 0.0] } else { [0.0; 3] }), cube("dash", [0.0; 3])]).collect();
        let n: Vec<i64> = (0..120).collect();
        let headline = crate::Page { words: vec![crate::Words { text: "The rule".into(), bx: [360.0, 380.0, 1560.0, 700.0], allow_overlap: false, points: None }], videos: Vec::new(), pictures: Vec::new() };
        let f = crate::check_with(&fs, &n, &vec![headline; 120], 1920, 1080, 30.0, "test");
        assert!(!f.iter().any(|f| f.check == "frame-empty"), "{f:?}");
        let clip = crate::Page { words: Vec::new(), videos: Vec::new(), pictures: vec![[656.0, 0.0, 1264.0, 1080.0]] };
        assert!(!crate::check_with(&fs, &n, &vec![clip; 120], 1920, 1080, 30.0, "test").iter().any(|f| f.check == "frame-empty"));
    }

    #[test]
    fn a_floor_filling_the_frame_is_no_content() {
        let mut floor = item("mesh", json!({"name": "floor"}), Mat4::from_translation(Vec3::new(0.0, -1.0, 0.0)));
        floor.shape = Some("box(100, 0.1, 100)".into());
        // a 4 m crate to the left, filling a sixth of the frame; for two seconds the camera looks along the floor, away
        let mut crate_ = cube("crate", [-5.0, 0.0, 0.0]);
        crate_.shape = Some("box(4, 4, 4)".into());
        let fs: Vec<Vec<Item>> = (0..120).map(|i| vec![cam([0.0, 1.0, 6.0], if (30..90).contains(&i) { [40.0, -1.0, 6.0] } else { [-5.0, 0.0, 0.0] }), floor.clone(), crate_.clone()]).collect();
        assert!(run(fs).iter().any(|f| f.check == "frame-empty"));
    }

    #[test]
    fn a_rooms_wall_filling_the_frame_is_what_it_shows() {
        // the same floor and crate, and a 30 m wall standing 8 m off along the look away: the room's inside, not empty
        let mut floor = item("mesh", json!({"name": "floor"}), Mat4::from_translation(Vec3::new(0.0, -1.0, 0.0)));
        floor.shape = Some("box(100, 0.1, 100)".into());
        let mut wall = item("mesh", json!({"name": "wall"}), Mat4::from_translation(Vec3::new(8.0, 3.0, 6.0)));
        wall.shape = Some("box(0.2, 10, 30)".into());
        let mut crate_ = cube("crate", [-5.0, 0.0, 0.0]);
        crate_.shape = Some("box(4, 4, 4)".into());
        let fs: Vec<Vec<Item>> = (0..120).map(|i| vec![cam([0.0, 1.0, 6.0], if (30..90).contains(&i) { [40.0, 1.0, 6.0] } else { [-5.0, 0.0, 0.0] }), floor.clone(), wall.clone(), crate_.clone()]).collect();
        let f = run(fs);
        assert!(!f.iter().any(|f| f.check == "frame-empty"), "{f:?}");
        // a wall is never the shot's subject: looking at it alone cuts nothing
        assert!(!f.iter().any(|f| f.check == "subject-cut" && f.says.contains("wall")), "{f:?}");
    }

    #[test]
    fn a_model_filling_the_frame_is_sized_from_its_file() {
        // a cabinet scaled 20x (25 m wide, 18 m tall), the camera 6 m off its face near the top: its origin (its base)
        // is far below the frame, so before its file was read the frame was judged empty
        let cabinet = |src: &str| item("model", json!({"name": "cabinet", "src": src}), Mat4::from_scale(Vec3::splat(20.0)));
        // the middle two seconds of four; before and after, the camera turns round to a 6 m crate behind it
        let mut crate_ = cube("crate", [0.0, 16.0, 25.0]);
        crate_.shape = Some("box(6, 6, 6)".into());
        let look = |i: usize| if (30..90).contains(&i) { [0.0, 16.0, 0.0] } else { [0.0, 16.0, 25.0] };
        let shot = |src: &str| (0..120).map(|i| vec![cam([0.0, 16.0, 15.0], look(i)), cabinet(src), crate_.clone()]).collect::<Vec<_>>();
        let f = run(shot("../../sdk/react/bench/games/room3d/media/cabinet.glb"));
        assert!(!f.iter().any(|f| f.check == "frame-empty"), "{f:?}");
        assert!(run(shot("../../sdk/react/bench/games/room3d/media/no-such.glb")).iter().any(|f| f.check == "frame-empty"));
        // the camera inside it (a room model round the camera), looking away from its origin: not an empty frame
        let inside = |src: &str| (0..120).map(|i| vec![cam([0.0, 9.0, 4.0], if (30..90).contains(&i) { [0.0, 9.0, 9.0] } else { [0.0, 16.0, 25.0] }), cabinet(src), crate_.clone()]).collect::<Vec<_>>();
        let f = run(inside("../../sdk/react/bench/games/room3d/media/cabinet.glb"));
        assert!(!f.iter().any(|f| f.check == "frame-empty"), "{f:?}");
    }

    #[test]
    fn parts_on_screen_are_named_with_their_boxes() {
        let its = vec![cam([0.0, 0.0, 6.0], [0.0; 3]), cube("dash", [0.0; 3]), cube("gone", [0.0, 0.0, 20.0])];
        let on = super::on_screen(&its, 1920, 1080);
        assert_eq!(on.len(), 1, "{on:?}");
        let (name, b) = &on[0];
        assert!(name.contains("dash"), "{name}");
        assert!(b[0] < 960.0 && b[2] > 960.0 && b[1] < 540.0 && b[3] > 540.0, "{b:?}");
    }

    #[test]
    fn the_shots_subject_half_out_of_frame_is_cut() {
        // the cube's centre near the frame's right edge
        let fs: Vec<Vec<Item>> = (0..30).map(|_| vec![cam([0.0, 0.0, 6.0], [0.0; 3]), cube("logo", [4.9, 0.0, 0.0])]).collect();
        assert!(run(fs).iter().any(|f| f.check == "subject-cut"));
    }

    #[test]
    fn a_part_a_label_names_cut_by_the_edge_is_an_error() {
        // the badge, small and near the middle, shows most; the logo half out at the right is the one the Label names
        let fs: Vec<Vec<Item>> = (0..30).map(|_| vec![cam([0.0, 0.0, 6.0], [0.0; 3]), cube("badge", [0.0, 0.0, 1.0]), cube("logo", [4.9, 0.0, 0.0])]).collect();
        let label = |at: [f32; 3]| crate::Page { words: vec![crate::Words { text: "Our logo".into(), bx: [1500.0, 200.0, 1800.0, 260.0], allow_overlap: false, points: Some(at) }], ..crate::Page::default() };
        let n: Vec<i64> = (0..30).collect();
        let named = check_with(&fs, &n, &vec![label([4.9, 0.5, 0.0]); 30], 1920, 1080, 30.0, "test");
        let cut = named.iter().find(|f| f.check == "subject-cut").expect("subject-cut");
        assert_eq!(cut.level, Level::Error);
        assert!(cut.says.contains("'logo'") && cut.says.contains("a Label names"), "{}", cut.says);
        // a Label on the badge, shown whole: nothing is cut
        let whole = check_with(&fs, &n, &vec![label([0.0, 0.5, 1.0]); 30], 1920, 1080, 30.0, "test");
        assert!(!whole.iter().any(|f| f.check == "subject-cut"), "{whole:?}");
        // a Label pointing at nothing near a part leaves the guess: the logo cut is a warning
        let far = check_with(&fs, &n, &vec![label([0.0, 3.0, -9.0]); 30], 1920, 1080, 30.0, "test");
        assert!(far.iter().all(|f| f.check != "subject-cut" || f.level == Level::Warn), "{far:?}");
    }
}
