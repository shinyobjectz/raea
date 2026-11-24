//! SG11, page type stays off the parts: words drawn in the page over more than SHARE of a 3D part's box on screen, for
//! SECS or more, are an error (the essay's words over its curves, a headline over the card it names). A caption sliding
//! past a part is not one; a part that says allowOverlap, or a backdrop (a floor, a set's wall: framing's rule), is
//! not a part words can cover, and words in an element that says data-allow-overlap may cover any.

use std::collections::HashMap;

use raea_core_facts::{Finding, Level};
use serde_json::Value;

use super::{Item, Stage, called, runs};
use crate::framing::{backdrop, own_box};
use crate::view::{View, corners};

/// Words in the page on one frame: what they say, their box on screen ([x0, y0, x1, y1] in px), and whether they
/// may sit on a part (an element they're in says data-allow-overlap).
#[derive(Clone, Debug)]
pub struct Words {
    pub text: String,
    pub bx: [f32; 4],
    pub allow_overlap: bool,
    /// the world point a Label's words name (its `at`), which makes the part there the shot's subject (SG14)
    pub points: Option<[f32; 3]>,
}

/// What the page draws over one frame's 3D, as its manifest has it: its words, its videos' boxes and its pictures'
/// (an img, an svg, a background image or colour): SG14 counts all three as filling the frame.
#[derive(Clone, Debug, Default)]
pub struct Page {
    pub words: Vec<Words>,
    pub videos: Vec<[f32; 4]>,
    pub pictures: Vec<[f32; 4]>,
}

/// Over this share of a part's box on screen (the part inside the frame)
const SHARE: f32 = 0.10;
/// for this long or more, seconds
const SECS: f64 = 0.25;

/// What the words say in a finding, cut short.
fn short(t: &str) -> String {
    let mut s: String = t.chars().take(40).collect();
    if t.chars().count() > 40 {
        s.push('…');
    }
    s
}

/// Each words' frames over a part past SHARE as the frames come, and on each the parts it covers with their shares.
#[derive(Default)]
pub(crate) struct Over {
    boxes: HashMap<String, Option<(bevy::math::Vec3, bevy::math::Vec3)>>,
    over: HashMap<String, HashMap<usize, Vec<(String, f32)>>>,
}

impl Over {
    /// Frame k's parts through its camera `v`, and its page's words over them, on a w x h frame.
    pub(crate) fn push(&mut self, k: usize, its: &[Item], v: Option<View>, page: Option<&Page>, w: f32, h: f32) {
        let (Some(v), Some(words)) = (v, page.map(|p| &p.words).filter(|w| !w.is_empty())) else { return };
        let (boxes, over) = (&mut self.boxes, &mut self.over);
        for it in its {
            if it.props.get("allowOverlap").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            let Some((lo, hi)) = own_box(it, boxes).filter(|(lo, hi)| !backdrop(it, *lo, *hi)) else { continue };
            let Some(b) = v.bounds(corners(&it.world, lo, hi)) else { continue };
            let b = [b[0].max(0.0), b[1].max(0.0), b[2].min(w), b[3].min(h)];
            let area = (b[2] - b[0]) * (b[3] - b[1]);
            if !(area > 1.0) {
                continue;
            }
            for wd in words.iter().filter(|w| !w.allow_overlap) {
                let o = (wd.bx[2].min(b[2]) - wd.bx[0].max(b[0])).max(0.0) * (wd.bx[3].min(b[3]) - wd.bx[1].max(b[1])).max(0.0);
                if o / area > SHARE {
                    over.entry(wd.text.clone()).or_default().entry(k).or_default().push((called(it), o / area));
                }
            }
        }
    }

    /// SG11 over the frames given.
    pub(crate) fn check(self, st: &Stage) -> Vec<Finding> {
        // said in the order they start (a HashMap's order changed from run to run)
        let mut over: Vec<_> = self.over.into_iter().collect();
        over.sort_by_key(|(text, at)| (at.keys().min().copied(), text.clone()));
        let mut out = Vec::new();
        for (text, at) in over {
            for (a, b) in runs(st, st.len(), |k| at.contains_key(&k)) {
                if st.secs(a, b) < SECS && !st.alone(a) {
                    continue;
                }
                // the parts it covers over the run, the most covered first: one fault, however many parts are under it
                let mut most: Vec<(String, f32)> = Vec::new();
                for (name, share) in (a..=b).filter_map(|k| at.get(&k)).flatten() {
                    match most.iter_mut().find(|m| m.0 == *name) {
                        Some(m) => m.1 = m.1.max(*share),
                        None => most.push((name.clone(), *share)),
                    }
                }
                most.sort_by(|x, y| y.1.total_cmp(&x.1));
                let named: Vec<String> = most.iter().take(3).map(|(n, s)| format!("'{n}' ({:.0}%)", s * 100.0)).collect();
                let more = if most.len() > 3 { format!(" and {} more", most.len() - 3) } else { String::new() };
                let says = format!("'{}' in the page covers {}{more} from {} to {}", short(&text), named.join(", "), st.at(a), st.at(b));
                out.push(st.finding("type-over-part", Level::Error, a, b, says).fix("move the words off it, or beside it with a Label (raea/camera) that points at it; words meant to sit on it go in an element with data-allow-overlap"));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::{Mat4, Vec3};
    use raea_core_scene::Item;
    use serde_json::json;

    use super::{Page, Words};
    use crate::test_items::{cam, item};

    fn card(name: &str, extra: serde_json::Value) -> Item {
        let mut props = json!({"name": name});
        props.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        let mut it = item("mesh", props, Mat4::from_translation(Vec3::ZERO));
        it.shape = Some("box(2, 1, 0.1)".into());
        it
    }

    fn run(part: Item, words: [f32; 4], n: usize) -> Vec<raea_core_facts::Finding> {
        let all: Vec<Vec<Item>> = (0..n).map(|_| vec![cam([0.0, 0.0, 6.0], [0.0; 3]), part.clone()]).collect();
        let frames: Vec<i64> = (0..n as i64).collect();
        let page: Vec<Page> = (0..n).map(|_| Page { words: vec![Words { text: "Install now".into(), bx: words, allow_overlap: false, points: None }], videos: Vec::new(), pictures: Vec::new() }).collect();
        crate::check_with(&all, &frames, &page, 1920, 1080, 30.0, "test").into_iter().filter(|f| f.check == "type-over-part").collect()
    }

    #[test]
    fn a_headline_over_the_card_it_names_is_an_error() {
        // the card fills the middle of the frame; the headline sits across its centre for a second
        let f = run(card("card", json!({})), [760.0, 500.0, 1160.0, 580.0], 30);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].says.contains("'Install now' in the page covers 'card' ("), "{}", f[0].says);
    }

    #[test]
    fn words_over_several_parts_are_one_error_naming_them() {
        let mut left = card("left", json!({}));
        left.world = Mat4::from_translation(Vec3::new(-0.6, 0.0, 0.0));
        let mut right = card("right", json!({}));
        right.world = Mat4::from_translation(Vec3::new(0.6, 0.0, 0.0));
        let all: Vec<Vec<Item>> = (0..30).map(|_| vec![cam([0.0, 0.0, 6.0], [0.0; 3]), left.clone(), right.clone()]).collect();
        let frames: Vec<i64> = (0..30).collect();
        let page: Vec<Page> = (0..30).map(|_| Page { words: vec![Words { text: "Install now".into(), bx: [560.0, 500.0, 1360.0, 580.0], allow_overlap: false, points: None }], videos: Vec::new(), pictures: Vec::new() }).collect();
        let f: Vec<_> = crate::check_with(&all, &frames, &page, 1920, 1080, 30.0, "test").into_iter().filter(|f| f.check == "type-over-part").collect();
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].says.contains("'left'") && f[0].says.contains("'right'"), "{}", f[0].says);
    }

    #[test]
    fn words_that_say_allow_overlap_may_cover_a_part() {
        let all: Vec<Vec<Item>> = (0..30).map(|_| vec![cam([0.0, 0.0, 6.0], [0.0; 3]), card("card", json!({}))]).collect();
        let frames: Vec<i64> = (0..30).collect();
        let page: Vec<Page> = (0..30).map(|_| Page { words: vec![Words { text: "on the card".into(), bx: [760.0, 500.0, 1160.0, 580.0], allow_overlap: true, points: None }], ..Page::default() }).collect();
        assert!(!crate::check_with(&all, &frames, &page, 1920, 1080, 30.0, "test").iter().any(|f| f.check == "type-over-part"));
    }

    #[test]
    fn words_beside_it_brief_or_allowed_are_not() {
        assert!(run(card("card", json!({})), [40.0, 40.0, 400.0, 100.0], 30).is_empty());
        // over it for 4 frames (under a quarter second): a caption passing
        assert!(run(card("card", json!({})), [760.0, 500.0, 1160.0, 580.0], 4).is_empty());
        assert!(run(card("card", json!({"allowOverlap": true})), [760.0, 500.0, 1160.0, 580.0], 30).is_empty());
    }
}
