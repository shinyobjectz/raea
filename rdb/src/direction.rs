//! Screen direction at a cut (the roadmap G's catalogue): which way things move on screen either side of a cut. The
//! subject's way is its face's centre moving across the frame (B3's zones); without a face it is the whole picture's
//! shift frame to frame (the camera's pan, or everything moving), found by block matching the sketches. Kept, reversed,
//! or still on one side; a cut where the motion carries on at a like speed is a match on action.

use crate::previz::Pic;

/// The shift (dx, dy, in pixels) that best lines `b` up with `a`: b(x, y) ~ a(x - dx, y - dy), searched within
/// `r` pixels on luma, the mean absolute difference over the overlap.
pub fn shift(a: &Pic, b: &Pic, r: i64) -> (i64, i64) {
    let l = |p: &Pic, x: usize, y: usize| { let i = (y * p.w + x) * 3; (p.rgb[i] as i32 * 77 + p.rgb[i + 1] as i32 * 150 + p.rgb[i + 2] as i32 * 29) >> 8 };
    let (w, h) = (a.w as i64, a.h as i64);
    let mut best = ((0, 0), f64::MAX);
    for dy in -r..=r {
        for dx in -r..=r {
            let (mut sum, mut n) = (0i64, 0i64);
            for y in r.max(dy)..(h - r).min(h + dy) {
                for x in r.max(dx)..(w - r).min(w + dx) {
                    sum += (l(b, x as usize, y as usize) - l(a, (x - dx) as usize, (y - dy) as usize)).abs() as i64;
                    n += 1;
                }
            }
            if n == 0 { continue }
            // a small lean toward no shift, so a flat picture reads as still
            let cost = sum as f64 / n as f64 + 0.05 * (dx.abs() + dy.abs()) as f64;
            if cost < best.1 {
                best = ((dx, dy), cost);
            }
        }
    }
    best.0
}

/// The way of a span of frames, in canvas widths a second (+ is to the right): the face's centre if it is seen at both
/// ends, else the picture's shift from the first frame to the last.
pub fn way(faces: &[Option<f64>], pics: &[&Pic], fps: f64) -> (f64, &'static str) {
    let secs = (pics.len().max(2) - 1) as f64 / fps;
    if let (Some(Some(a)), Some(Some(b))) = (faces.first(), faces.last()) {
        return ((b - a) / secs, "subject");
    }
    // the span's ends against each other (frame to frame, a slow pan is under a sketch pixel and rounds away)
    let (Some(a), Some(b)) = (pics.first(), pics.last()) else { return (0.0, "picture") };
    (shift(a, b, 12).0 as f64 / a.w as f64 / secs, "picture")
}

/// Kept, reversed or still across a cut, from the two sides' ways (canvas widths a second; under 0.15 is still: a
/// talking head's sway is about 0.1, a subject crossing the frame in a few seconds is 0.2 or more).
pub fn across(out: f64, inn: f64) -> &'static str {
    let still = |v: f64| v.abs() < 0.15;
    match (still(out), still(inn)) {
        (true, true) => "still",
        (true, false) => "from still",
        (false, true) => "to still",
        _ if out.signum() == inn.signum() => "kept",
        _ => "reversed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pic(off: i64) -> Pic {
        let (w, h) = (64usize, 48usize);
        let mut rgb = vec![0u8; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                // a textured pattern moved `off` pixels to the right
                let v = ((((x as i64 - off) * 7) ^ (y as i64 * 13)).rem_euclid(256)) as u8;
                rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[v, v, v]);
            }
        }
        Pic { w, h, rgb }
    }

    #[test]
    fn a_pan_is_found() {
        assert_eq!(shift(&pic(0), &pic(3), 4), (3, 0));
        assert_eq!(shift(&pic(0), &pic(-2), 4), (-2, 0));
    }

    #[test]
    fn a_flat_picture_is_still() {
        let flat = Pic { w: 64, h: 48, rgb: vec![90; 64 * 48 * 3] };
        assert_eq!(shift(&flat, &flat, 4), (0, 0));
    }

    #[test]
    fn ways_across_a_cut() {
        assert_eq!(across(0.2, 0.3), "kept");
        assert_eq!(across(0.2, -0.3), "reversed");
        assert_eq!(across(0.01, -0.3), "from still");
        assert_eq!(across(0.0, 0.0), "still");
    }
}
