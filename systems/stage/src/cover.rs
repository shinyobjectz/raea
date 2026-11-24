//! SG10, parts in front of the words: which part, if any, stands between a block of words and the camera. The block's
//! face is sampled on a 3x3 grid through the middle of its letters, and each point's line to the eye tested against
//! every other part's box, in the part's own space (a mesh's shape, a picture's card: framing.rs own_box). Words never
//! hide words: a block is mostly air between its letters (on the essay film, every "ASSETS behind VARIATION" read
//! clearly). A model has no box here (its file's box is mostly air round a tree, and raea poses a person off its rest
//! box), so it never hides words either.

use std::collections::HashMap;

use bevy::math::{Mat4, Vec3};
use raea_core_scene::Item;
use serde_json::Value;

use super::called;
use crate::framing::own_box;

/// More than this share of a block's points hidden, and the words are behind a part (the essay film's day labels, their
/// lower third under the next tile, still read)
const HIDES: f32 = 0.5;
/// A part this see-through or more never hides words (glass, a fade)
const SEE_THROUGH: f64 = 0.5;

/// A frame's parts that can stand in front of words: (its index, its world's inverse, its own box), the see-through
/// left out. Made once a frame, for every block of words in it.
pub(crate) type Part = (usize, Mat4, Vec3, Vec3);

pub(crate) fn parts(its: &[Item], boxes: &mut HashMap<String, Option<(Vec3, Vec3)>>) -> Vec<Part> {
    its.iter()
        .enumerate()
        .filter(|(_, it)| it.kind != "text" && !see_through(it))
        .filter_map(|(j, it)| own_box(it, boxes).map(|(lo, hi)| (j, it.world.inverse(), lo, hi)))
        .collect()
}

/// The part hiding at least half of a block of words (item `me`) from `eye`, by what it's called: the block spans `x`
/// across and `tall` up its own face (z = 0 in `world`), an `em` high. A line counts as stopped only where it enters a
/// part more than an em short of the words, so words on a sign's face, or cut into it, aren't behind the sign; and a
/// part round the eye (a room's box with the camera inside) is the set, not something in front.
pub(crate) fn behind(its: &[Item], parts: &[Part], me: usize, world: &Mat4, x: (f32, f32), tall: f32, em: f32, eye: Vec3) -> Option<String> {
    let points: Vec<Vec3> = [0.2f32, 0.5, 0.8]
        .iter()
        .flat_map(|fx| [-0.2f32, 0.0, 0.2].map(move |fy| (*fx, fy)))
        .map(|(fx, fy)| world.transform_point3(Vec3::new(x.0 + (x.1 - x.0) * fx, tall * fy, 0.0)))
        .collect();
    let mut hidden = 0;
    let mut by: HashMap<usize, usize> = HashMap::new();
    for p in &points {
        let dist = (*p - eye).length();
        if dist <= em {
            continue;
        }
        let stopped = parts.iter().find(|(j, inv, lo, hi)| {
            if *j == me {
                return false;
            }
            let (o, q) = (inv.transform_point3(eye), inv.transform_point3(*p));
            let inside = (0..3).all(|a| o[a] >= lo[a] && o[a] <= hi[a]);
            !inside && enters(o, q, *lo, *hi).is_some_and(|t| t * dist < dist - em)
        });
        if let Some((j, ..)) = stopped {
            hidden += 1;
            *by.entry(*j).or_default() += 1;
        }
    }
    if (hidden as f32) <= HIDES * points.len() as f32 {
        return None;
    }
    by.into_iter().max_by_key(|(j, n)| (*n, std::cmp::Reverse(*j))).map(|(j, _)| called(&its[j]))
}

/// Whether a part lets the words through: its material's opacity (else its own) under SEE_THROUGH.
fn see_through(it: &Item) -> bool {
    let material = match &it.material {
        Some(m) => opacity(&m.props),
        None => None,
    };
    material.or(opacity(&it.props)).is_some_and(|o| o < SEE_THROUGH)
}

fn opacity(props: &serde_json::Map<String, Value>) -> Option<f64> {
    props.get("opacity")?.as_f64()
}

/// Where the segment o to q first enters the box [lo, hi], as a share of its length, if it does before q.
fn enters(o: Vec3, q: Vec3, lo: Vec3, hi: Vec3) -> Option<f32> {
    let d = q - o;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for a in 0..3 {
        if d[a].abs() < 1e-9 {
            if o[a] < lo[a] || o[a] > hi[a] {
                return None;
            }
            continue;
        }
        let (mut ta, mut tb) = ((lo[a] - o[a]) / d[a], (hi[a] - o[a]) / d[a]);
        if ta > tb {
            std::mem::swap(&mut ta, &mut tb);
        }
        t0 = t0.max(ta);
        t1 = t1.min(tb);
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_segment_enters_a_box_where_it_meets_it() {
        let (lo, hi) = (Vec3::splat(-1.0), Vec3::splat(1.0));
        assert_eq!(enters(Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -5.0), lo, hi), Some(0.4));
        assert_eq!(enters(Vec3::new(3.0, 0.0, 5.0), Vec3::new(3.0, 0.0, -5.0), lo, hi), None);
        // ends before the box
        assert_eq!(enters(Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, 2.0), lo, hi), None);
    }
}
