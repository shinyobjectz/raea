//! Flash safety (docs/the roadmap G's catalogue, the one hard rule): WCAG 2.3.1's general flash threshold on an edit's
//! frames. A flash is a pair of opposing changes in relative luminance of 10% or more of the brighter, where the
//! darker state is under 0.80, over at least a quarter of the screen; more than three flashes in any one second fails.
//! (The red-flash test and the 341 x 256 "small area" refinement are not here: a quarter of the frame is the stricter
//! area for phone video.)

/// A frame's relative luminance per pixel (sRGB to linear, then BT.709 weights), from RGB8.
pub fn luminance(rgb: &[u8]) -> Vec<f32> {
    let lin = |v: u8| {
        let c = v as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    rgb.chunks_exact(3).map(|p| 0.2126 * lin(p[0]) + 0.7152 * lin(p[1]) + 0.0722 * lin(p[2])).collect()
}

/// Each frame's flash transitions: +1 where a quarter or more of the frame brightened by a flash-sized step since
/// the previous frame, -1 where it darkened, 0 otherwise.
pub fn transitions(frames: &[Vec<f32>]) -> Vec<i8> {
    let mut out = vec![0i8; frames.len()];
    for i in 1..frames.len() {
        let (a, b) = (&frames[i - 1], &frames[i]);
        let (mut up, mut down) = (0usize, 0usize);
        for (x, y) in a.iter().zip(b) {
            let (hi, lo) = (x.max(*y), x.min(*y));
            if hi - lo >= 0.1 * hi.max(1e-6) && hi - lo >= 0.1 && lo < 0.8 {
                if y > x { up += 1 } else { down += 1 }
            }
        }
        let quarter = a.len() / 4;
        out[i] = if up >= quarter && up >= down { 1 } else if down >= quarter { -1 } else { 0 };
    }
    out
}

/// The spans (first frame, last frame) where any one second holds more than three flashes (pairs of opposing
/// transitions), at `fps`.
pub fn unsafe_spans(frames: &[Vec<f32>], fps: f64) -> Vec<(usize, usize)> {
    let t = transitions(frames);
    // a flash ends at each transition that opposes the one before it
    let mut flashes = Vec::new();
    let mut last = 0i8;
    for (i, &d) in t.iter().enumerate() {
        if d != 0 {
            if last != 0 && d != last {
                // the pair is spent: the next flash needs two changes of its own
                flashes.push(i);
                last = 0;
            } else {
                last = d;
            }
        }
    }
    let window = fps.round().max(1.0) as usize;
    let mut bad: Vec<usize> = Vec::new();
    for (k, &f) in flashes.iter().enumerate() {
        // flashes within the second starting at this one
        let n = flashes[k..].iter().take_while(|&&g| g < f + window).count();
        if n > 3 {
            bad.extend(f..(f + window).min(frames.len()));
        }
    }
    bad.sort();
    bad.dedup();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for f in bad {
        match spans.last_mut() {
            Some(s) if f == s.1 + 1 => s.1 = f,
            _ => spans.push((f, f)),
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(v: f32) -> Vec<f32> {
        vec![v; 100]
    }

    #[test]
    fn steady_footage_is_safe() {
        let frames: Vec<Vec<f32>> = (0..90).map(|i| flat(0.3 + 0.01 * (i % 3) as f32)).collect();
        assert!(unsafe_spans(&frames, 30.0).is_empty());
    }

    #[test]
    fn a_strobe_at_ten_a_second_fails() {
        // bright and dark alternating every 3 frames at 30 fps: 5 flashes a second
        let frames: Vec<Vec<f32>> = (0..90).map(|i| flat(if (i / 3) % 2 == 0 { 0.05 } else { 0.9 })).collect();
        assert!(!unsafe_spans(&frames, 30.0).is_empty());
    }

    #[test]
    fn three_flashes_a_second_pass() {
        // a full flash every 10 frames at 30 fps: 3 a second, the limit
        let frames: Vec<Vec<f32>> = (0..90).map(|i| flat(if i % 10 < 5 { 0.05 } else { 0.9 })).collect();
        let t = transitions(&frames);
        assert!(t.iter().filter(|&&d| d != 0).count() > 0);
        assert!(unsafe_spans(&frames, 30.0).is_empty());
    }

    #[test]
    fn a_strobe_on_a_small_area_passes() {
        // the same strobe over a tenth of the frame
        let frames: Vec<Vec<f32>> = (0..90)
            .map(|i| (0..100).map(|p| if p < 10 && (i / 3) % 2 == 0 { 0.9 } else { 0.05 }).collect())
            .collect();
        assert!(unsafe_spans(&frames, 30.0).is_empty());
    }
}
