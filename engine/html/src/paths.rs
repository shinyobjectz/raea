//! GSAP's "RawPath" model, reimplemented natively (MotionPathPlugin, MorphSVGPlugin): SVG path data
//! as cubic Béziers, measured by sampling each curve `resolution` times, with progress mapped to a
//! point through those samples. Matching the plugins means matching their approximations, not
//! exact arc length, so this follows their algorithms step by step (gsap/src/utils/paths.js:
//! stringToRawPath, arcToSegment, measureSegment, getPositionOnPath, getRotationAtBezierT).
//! No GSAP code ships; the plugins are only used to produce test references.

/// One subpath: `[x, y, (cp1x, cp1y, cp2x, cp2y, x, y)*]`.
#[derive(Clone, Debug, Default)]
pub struct Segment {
    pub pts: Vec<f64>,
    pub closed: bool,
    resolution: usize,
    samples: Vec<f64>,
    lookup: Vec<usize>,
    pub total_length: f64,
    min_length: f64,
}

#[derive(Clone, Debug, Default)]
pub struct RawPath {
    pub segs: Vec<Segment>,
    pub total_length: f64,
}

const LARGE: f64 = 1e8;

/// JavaScript's Math.round (halves round toward +∞).
fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// GSAP's `_round`: 5 decimals.
pub fn round5(v: f64) -> f64 {
    let r = js_round(v * 1e5) / 1e5;
    if r.is_nan() { 0.0 } else { r }
}

/// Tokens as GSAP's `_svgPathExp` splits them: command letters and numbers (strings, because
/// arc flags may be glued to the next number, "a8 8 0 018 8").
fn tokenize(d: &str) -> Vec<String> {
    let b = d.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if b"achlmqstvzACHLMQSTVZ".contains(&c) {
            out.push((c as char).to_string());
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i;
        if j < b.len() && b[j] == b'-' {
            j += 1;
        }
        let digits_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j < b.len() && b[j] == b'.' {
            j += 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
        }
        let has_digit = b[digits_start..j].iter().any(u8::is_ascii_digit);
        if has_digit && j < b.len() && (b[j] == b'e' || b[j] == b'E') {
            let mut k = j + 1;
            if k < b.len() && (b[k] == b'+' || b[k] == b'-') {
                k += 1;
            }
            if k < b.len() && b[k].is_ascii_digit() {
                while k < b.len() && b[k].is_ascii_digit() {
                    k += 1;
                }
                j = k;
            }
        }
        if has_digit {
            let s = &d[start..j];
            // tiny scientific values become 0, like GSAP's pre-pass
            let s = match s.parse::<f64>() {
                Ok(n) if s.contains(['e', 'E']) && n.abs() < 1e-4 => "0".to_string(),
                _ => s.to_string(),
            };
            out.push(s);
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

fn segment_is_closed(s: &mut Segment) {
    let n = s.pts.len();
    s.closed = n >= 2 && (s.pts[0] - s.pts[n - 2]).abs() < 0.001 && (s.pts[1] - s.pts[n - 1]).abs() < 0.001;
}

/// SVG arc -> cubic Béziers (without the start point); None when start == end.
#[allow(clippy::too_many_arguments)]
fn arc_to_segment(last_x: f64, last_y: f64, rx: f64, ry: f64, angle: f64, large: f64, sweep: f64, x: f64, y: f64) -> Option<Vec<f64>> {
    if last_x == x && last_y == y {
        return None;
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    let pi = std::f64::consts::PI;
    let two_pi = pi * 2.0;
    let angle_rad = (angle % 360.0) * pi / 180.0;
    let (cos_a, sin_a) = (angle_rad.cos(), angle_rad.sin());
    let dx2 = (last_x - x) / 2.0;
    let dy2 = (last_y - y) / 2.0;
    let x1 = cos_a * dx2 + sin_a * dy2;
    let y1 = -sin_a * dx2 + cos_a * dy2;
    let (x1_sq, y1_sq) = (x1 * x1, y1 * y1);
    let radii_check = x1_sq / (rx * rx) + y1_sq / (ry * ry);
    if radii_check > 1.0 {
        rx *= radii_check.sqrt();
        ry *= radii_check.sqrt();
    }
    let (rx_sq, ry_sq) = (rx * rx, ry * ry);
    let mut sq = ((rx_sq * ry_sq) - (rx_sq * y1_sq) - (ry_sq * x1_sq)) / ((rx_sq * y1_sq) + (ry_sq * x1_sq));
    if sq < 0.0 {
        sq = 0.0;
    }
    let coef = (if large == sweep { -1.0 } else { 1.0 }) * sq.sqrt();
    let cx1 = coef * ((rx * y1) / ry);
    let cy1 = coef * -((ry * x1) / rx);
    let sx2 = (last_x + x) / 2.0;
    let sy2 = (last_y + y) / 2.0;
    let cx = sx2 + (cos_a * cx1 - sin_a * cy1);
    let cy = sy2 + (sin_a * cx1 + cos_a * cy1);
    let ux = (x1 - cx1) / rx;
    let uy = (y1 - cy1) / ry;
    let vx = (-x1 - cx1) / rx;
    let vy = (-y1 - cy1) / ry;
    let temp = ux * ux + uy * uy;
    let mut angle_start = (if uy < 0.0 { -1.0 } else { 1.0 }) * (ux / temp.sqrt()).acos();
    let mut angle_extent = (if ux * vy - uy * vx < 0.0 { -1.0 } else { 1.0 }) * ((ux * vx + uy * vy) / (temp * (vx * vx + vy * vy)).sqrt()).acos();
    if angle_extent.is_nan() {
        angle_extent = pi;
    }
    if sweep == 0.0 && angle_extent > 0.0 {
        angle_extent -= two_pi;
    } else if sweep != 0.0 && angle_extent < 0.0 {
        angle_extent += two_pi;
    }
    angle_start %= two_pi;
    angle_extent %= two_pi;
    let segments = (angle_extent.abs() / (two_pi / 4.0)).ceil() as usize;
    let increment = angle_extent / segments as f64;
    let control = 4.0 / 3.0 * (increment / 2.0).sin() / (1.0 + (increment / 2.0).cos());
    let (ma, mb, mc, md) = (cos_a * rx, sin_a * rx, sin_a * -ry, cos_a * ry);
    let mut raw = Vec::with_capacity(segments * 6);
    for i in 0..segments {
        let a0 = angle_start + i as f64 * increment;
        let (x1, y1) = (a0.cos(), a0.sin());
        let a1 = a0 + increment;
        let (ux, uy) = (a1.cos(), a1.sin());
        raw.extend_from_slice(&[x1 - control * y1, y1 + control * x1, ux + control * uy, uy - control * ux, ux, uy]);
    }
    let mut i = 0;
    while i < raw.len() {
        let (px, py) = (raw[i], raw[i + 1]);
        raw[i] = px * ma + py * mc + cx;
        raw[i + 1] = px * mb + py * md + cy;
        i += 2;
    }
    let n = raw.len();
    if n >= 2 {
        raw[n - 2] = x;
        raw[n - 1] = y;
    }
    Some(raw)
}

/// SVG path data -> RawPath with absolute coordinates (GSAP's stringToRawPath).
pub fn parse(d: &str) -> RawPath {
    let a = tokenize(d);
    let num = |i: isize| -> f64 { if i < 0 { f64::NAN } else { a.get(i as usize).and_then(|s| s.parse::<f64>().ok()).unwrap_or(f64::NAN) } };
    let is_cmd = |i: isize| i >= 0 && a.get(i as usize).is_some_and(|s| s.parse::<f64>().is_err());
    let mut path: Vec<Segment> = Vec::new();
    if d.trim().is_empty() || !is_cmd(0) || is_cmd(1) {
        return RawPath::default();
    }
    let (mut rel_x, mut rel_y) = (0.0f64, 0.0f64);
    let two_thirds = 2.0 / 3.0;
    let n = a.len() as isize;
    let mut command = String::new();
    let mut is_relative = false;
    let (mut start_x, mut start_y) = (0.0, 0.0);
    // the segment being built (GSAP keeps a reference even after dropping a too-short one)
    let mut seg: Option<Segment> = None;
    let mut in_path = false;
    let flush = |seg: &mut Option<Segment>, in_path: &mut bool, path: &mut Vec<Segment>| {
        if let Some(s) = seg.as_mut() {
            segment_is_closed(s);
            if *in_path {
                path.push(s.clone());
            }
        }
        *in_path = false;
    };
    let line = |s: &mut Segment, sx: f64, sy: f64, ex: f64, ey: f64| {
        let dx = (ex - sx) / 3.0;
        let dy = (ey - sy) / 3.0;
        s.pts.extend_from_slice(&[sx + dx, sy + dy, ex - dx, ey - dy, ex, ey]);
    };
    let mut i: isize = 0;
    while i < n {
        let prev_command = command.clone();
        if is_cmd(i) {
            let tok = &a[i as usize];
            command = tok.to_uppercase();
            is_relative = command != *tok;
        } else {
            i -= 1;
        }
        let mut x = num(i + 1);
        let mut y = num(i + 2);
        if is_relative {
            x += rel_x;
            y += rel_y;
        }
        if i == 0 {
            start_x = x;
            start_y = y;
        }
        match command.as_str() {
            "M" => {
                if seg.is_some() {
                    // a segment that never drew anything is dropped
                    if seg.as_ref().unwrap().pts.len() < 8 {
                        in_path = false;
                    }
                    flush(&mut seg, &mut in_path, &mut path);
                }
                rel_x = x;
                start_x = x;
                rel_y = y;
                start_y = y;
                seg = Some(Segment { pts: vec![x, y], ..Default::default() });
                in_path = true;
                i += 2;
                command = "L".into();
            }
            "C" => {
                let s = seg.get_or_insert_with(|| Segment { pts: vec![0.0, 0.0], ..Default::default() });
                if !is_relative {
                    rel_x = 0.0;
                    rel_y = 0.0;
                }
                let (c2x, c2y) = (rel_x + num(i + 3), rel_y + num(i + 4));
                rel_x += num(i + 5);
                rel_y += num(i + 6);
                s.pts.extend_from_slice(&[x, y, c2x, c2y, rel_x, rel_y]);
                i += 6;
            }
            "S" => {
                let s = seg.get_or_insert_with(Segment::default);
                let (mut dif_x, mut dif_y) = (rel_x, rel_y);
                if (prev_command == "C" || prev_command == "S") && s.pts.len() >= 4 {
                    let l = s.pts.len();
                    dif_x += rel_x - s.pts[l - 4];
                    dif_y += rel_y - s.pts[l - 3];
                }
                if !is_relative {
                    rel_x = 0.0;
                    rel_y = 0.0;
                }
                rel_x += num(i + 3);
                rel_y += num(i + 4);
                s.pts.extend_from_slice(&[dif_x, dif_y, x, y, rel_x, rel_y]);
                i += 4;
            }
            "Q" => {
                let s = seg.get_or_insert_with(Segment::default);
                let dif_x = rel_x + (x - rel_x) * two_thirds;
                let dif_y = rel_y + (y - rel_y) * two_thirds;
                if !is_relative {
                    rel_x = 0.0;
                    rel_y = 0.0;
                }
                rel_x += num(i + 3);
                rel_y += num(i + 4);
                s.pts.extend_from_slice(&[dif_x, dif_y, rel_x + (x - rel_x) * two_thirds, rel_y + (y - rel_y) * two_thirds, rel_x, rel_y]);
                i += 4;
            }
            "T" => {
                let s = seg.get_or_insert_with(Segment::default);
                let l = s.pts.len();
                let (dif_x, dif_y) = if l >= 4 { (rel_x - s.pts[l - 4], rel_y - s.pts[l - 3]) } else { (0.0, 0.0) };
                s.pts.extend_from_slice(&[
                    rel_x + dif_x,
                    rel_y + dif_y,
                    x + ((rel_x + dif_x * 1.5) - x) * two_thirds,
                    y + ((rel_y + dif_y * 1.5) - y) * two_thirds,
                    x,
                    y,
                ]);
                rel_x = x;
                rel_y = y;
                i += 2;
            }
            "H" => {
                let s = seg.get_or_insert_with(Segment::default);
                line(s, rel_x, rel_y, x, rel_y);
                rel_x = x;
                i += 1;
            }
            "V" => {
                let s = seg.get_or_insert_with(Segment::default);
                let ny = x + if is_relative { rel_y - rel_x } else { 0.0 };
                line(s, rel_x, rel_y, rel_x, ny);
                rel_y = ny;
                i += 1;
            }
            "L" | "Z" => {
                let s = seg.get_or_insert_with(Segment::default);
                if command == "Z" {
                    x = start_x;
                    y = start_y;
                    s.closed = true;
                }
                if command == "L" || (rel_x - x).abs() > 0.5 || (rel_y - y).abs() > 0.5 {
                    line(s, rel_x, rel_y, x, y);
                    if command == "L" {
                        i += 2;
                    }
                }
                rel_x = x;
                rel_y = y;
            }
            "A" => {
                let s = seg.get_or_insert_with(Segment::default);
                let tok = |k: isize| a.get((i + k) as usize).cloned().unwrap_or_default();
                let (mut flag1, mut flag2, mut dif_x, mut dif_y) = (tok(4), tok(5), tok(6), tok(7));
                let mut j = 7;
                if flag1.len() > 1 {
                    if flag1.len() < 3 {
                        dif_y = dif_x;
                        dif_x = flag2.clone();
                        j -= 1;
                    } else {
                        dif_y = flag2.clone();
                        dif_x = flag1[2..].to_string();
                        j -= 2;
                    }
                    flag2 = flag1[1..2].to_string();
                    flag1 = flag1[0..1].to_string();
                }
                let f = |v: &str| v.parse::<f64>().unwrap_or(f64::NAN);
                let ex = if is_relative { rel_x } else { 0.0 } + f(&dif_x);
                let ey = if is_relative { rel_y } else { 0.0 } + f(&dif_y);
                if let Some(beziers) = arc_to_segment(rel_x, rel_y, num(i + 1), num(i + 2), num(i + 3), f(&flag1), f(&flag2), ex, ey) {
                    s.pts.extend_from_slice(&beziers);
                }
                i += j;
                let l = s.pts.len();
                if l >= 2 {
                    rel_x = s.pts[l - 2];
                    rel_y = s.pts[l - 1];
                }
            }
            _ => {}
        }
        i += 1;
    }
    if let Some(mut s) = seg {
        if s.pts.len() >= 6 {
            segment_is_closed(&mut s);
            if in_path {
                path.push(s);
            }
        }
    }
    RawPath { segs: path, total_length: 0.0 }
}

/// GSAP's measureSegment for a whole segment: per-curve samples, the lookup table and length.
fn measure_segment(seg: &mut Segment) -> f64 {
    let resolution = if seg.resolution == 0 { 12 } else { seg.resolution };
    let inc = 1.0 / resolution as f64;
    seg.samples.clear();
    seg.lookup.clear();
    if seg.pts.len() < 2 {
        seg.total_length = 0.0;
        return 0.0;
    }
    let mut end = seg.pts.len();
    let (mut x1, mut y1) = (seg.pts[0], seg.pts[1]);
    let mut min = LARGE;
    let mut length = 0.0;
    let mut j = 2;
    while j < end {
        if j + 5 >= seg.pts.len() {
            break;
        }
        let x4 = seg.pts[j + 4] - x1;
        let x3 = seg.pts[j + 2] - x1;
        let x2 = seg.pts[j] - x1;
        let y4 = seg.pts[j + 5] - y1;
        let y3 = seg.pts[j + 3] - y1;
        let y2 = seg.pts[j + 1] - y1;
        if x4.abs() < 0.01 && y4.abs() < 0.01 && x2.abs() + y2.abs() < 0.01 {
            // a curve that is basically a point
            if seg.pts.len() > 8 {
                seg.pts.drain(j..j + 6);
                end -= 6;
                x1 += x4;
                y1 += y4;
                continue;
            }
        } else {
            let (mut xd1, mut yd1) = (0.0f64, 0.0f64);
            for k in 1..=resolution {
                let t = inc * k as f64;
                let inv = 1.0 - t;
                let nx = (t * t * x4 + 3.0 * inv * (t * x3 + inv * x2)) * t;
                let ny = (t * t * y4 + 3.0 * inv * (t * y3 + inv * y2)) * t;
                let xd = xd1 - nx;
                let yd = yd1 - ny;
                xd1 = nx;
                yd1 = ny;
                let l = (yd * yd + xd * xd).sqrt();
                if l < min {
                    min = l;
                }
                length += l;
                seg.samples.push(length);
            }
        }
        x1 += x4;
        y1 += y4;
        j += 6;
    }
    if !seg.samples.is_empty() && min != 0.0 {
        let total = *seg.samples.last().unwrap();
        seg.total_length = total;
        seg.min_length = min;
        if total / min < 9999.0 {
            let mut length_index = 0usize;
            let mut i = 0.0;
            while i < total {
                let v = if seg.samples.get(length_index).is_some_and(|&v| v < i) {
                    length_index += 1;
                    length_index
                } else {
                    length_index
                };
                seg.lookup.push(v);
                i += min;
            }
        }
    } else {
        seg.total_length = 0.0;
        seg.samples = vec![0.0];
    }
    length
}

/// GSAP's cacheRawPathMeasurements.
pub fn measure(path: &mut RawPath, resolution: usize) {
    let mut total = 0.0;
    for seg in &mut path.segs {
        seg.resolution = if resolution == 0 { 12 } else { resolution };
        total += measure_segment(seg);
    }
    path.total_length = total;
}

fn sample_index(samples: &[f64], length: f64, progress: f64) -> usize {
    let l = samples.len();
    if l == 0 {
        return 0;
    }
    let mut i = (progress * l as f64) as isize;
    let at = |i: isize| if i >= 0 && (i as usize) < l { Some(samples[i as usize]) } else { None };
    if at(i).is_some_and(|v| v > length) {
        loop {
            i -= 1;
            if i == 0 || !at(i).is_some_and(|v| v > length) {
                break;
            }
        }
        if i < 0 {
            i = 0;
        }
    } else {
        loop {
            i += 1;
            if !(at(i).is_some_and(|v| v < length) && (i as usize) < l) {
                break;
            }
        }
    }
    if (i as usize) < l { i as usize } else { l - 1 }
}

fn rotation_at_bezier_t(s: &[f64], i: usize, t: f64) -> f64 {
    let g = |k: usize| s.get(i + k).copied().unwrap_or(0.0);
    let (mut a, mut b, c) = (g(0), g(2), g(4));
    a += (b - a) * t;
    b += (c - b) * t;
    a += (b - a) * t;
    let x = b + ((c + (g(6) - c) * t) - b) * t - a;
    let (mut a, mut b, c) = (g(1), g(3), g(5));
    a += (b - a) * t;
    b += (c - b) * t;
    a += (b - a) * t;
    round5((b + ((c + (g(7) - c) * t) - b) * t - a).atan2(x).to_degrees())
}

fn wrap_progress(p: f64) -> f64 {
    let r = js_round(((p + LARGE) % 1.0) * 1e5) / 1e5;
    if r != 0.0 { r } else if p < 0.0 { 0.0 } else { 1.0 }
}

/// GSAP's getPositionOnPath: (x, y, angle in degrees).
pub fn position(path: &RawPath, progress: f64) -> (f64, f64, f64) {
    let Some(first) = path.segs.first() else { return (0.0, 0.0, 0.0) };
    let mut progress = if !(0.0..=1.0).contains(&progress) { wrap_progress(progress) } else { progress };
    let mut seg = first;
    if path.segs.len() > 1 {
        let length = path.total_length * progress;
        let (mut max, mut i) = (0.0, 0usize);
        loop {
            max += path.segs[i].total_length;
            i += 1;
            if !(max < length) || i >= path.segs.len() {
                break;
            }
            seg = &path.segs[i];
        }
        let min = max - seg.total_length;
        progress = ((length - min) / (max - min)).max(0.0);
        if progress.is_nan() {
            progress = 0.0;
        }
    }
    let samples = &seg.samples;
    let resolution = seg.resolution.max(1);
    let length = seg.total_length * progress;
    let mut i = if !seg.lookup.is_empty() {
        let k = if progress < 1.0 { (length / seg.min_length) as usize } else { seg.lookup.len() - 1 };
        seg.lookup.get(k).copied().unwrap_or(0)
    } else {
        sample_index(samples, length, progress)
    };
    let sample = |k: usize| samples.get(k).copied().unwrap_or(f64::NAN);
    let mut min = if i > 0 { sample(i - 1) } else { 0.0 };
    let mut max = sample(i);
    if max < length {
        min = max;
        i += 1;
        max = sample(i);
    }
    let mut t = (1.0 / resolution as f64) * (((length - min) / (max - min)) + (i % resolution) as f64);
    if t.is_nan() {
        t = 0.0;
    }
    let inv = 1.0 - t;
    let i = (i / resolution) * 6;
    let p = &seg.pts;
    let g = |k: usize| p.get(i + k).copied().unwrap_or(f64::NAN);
    let a = g(0);
    let x = round5((t * t * (g(6) - a) + 3.0 * inv * (t * (g(4) - a) + inv * (g(2) - a))) * t + a);
    let a = g(1);
    let y = round5((t * t * (g(7) - a) + 3.0 * inv * (t * (g(5) - a) + inv * (g(3) - a))) * t + a);
    let angle = if seg.total_length != 0.0 {
        rotation_at_bezier_t(p, i, if t >= 1.0 { 1.0 - 1e-9 } else if t != 0.0 { t } else { 1e-9 })
    } else {
        0.0
    };
    (x, y, angle)
}

/// Blink's SVGPathElement length traversal (PathTraversalState::CurveLength): each cubic is split in
/// half until its control polygon is within 1e-5 of its chord (at most 20 deep), in f32 like Blink.
/// Calls `visit(start, end, length)` for every flat piece in path order.
fn blink_traverse(path: &RawPath, mut visit: impl FnMut([f32; 2], [f32; 2], f32) -> bool) {
    const TOLERANCE: f32 = 0.00001;
    const DEPTH: usize = 20;
    type Cubic = [[f32; 2]; 4];
    let dist = |a: [f32; 2], b: [f32; 2]| ((b[0] - a[0]) * (b[0] - a[0]) + (b[1] - a[1]) * (b[1] - a[1])).sqrt();
    let mid = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    for seg in &path.segs {
        let p = &seg.pts;
        let mut i = 0;
        while i + 7 < p.len() {
            let c: Cubic = [
                [p[i] as f32, p[i + 1] as f32],
                [p[i + 2] as f32, p[i + 3] as f32],
                [p[i + 4] as f32, p[i + 5] as f32],
                [p[i + 6] as f32, p[i + 7] as f32],
            ];
            let mut stack: Vec<Cubic> = vec![c];
            while let Some(top) = stack.last().copied() {
                let approx = dist(top[0], top[1]) + dist(top[1], top[2]) + dist(top[2], top[3]);
                if approx - dist(top[0], top[3]) > TOLERANCE && stack.len() < DEPTH {
                    // de Casteljau at 1/2; the left half is traversed first
                    let (ab, bc, cd) = (mid(top[0], top[1]), mid(top[1], top[2]), mid(top[2], top[3]));
                    let (abc, bcd) = (mid(ab, bc), mid(bc, cd));
                    let m = mid(abc, bcd);
                    let n = stack.len();
                    stack[n - 1] = [m, bcd, cd, top[3]];
                    stack.push([top[0], ab, abc, m]);
                } else {
                    stack.pop();
                    if !visit(top[0], top[3], approx) {
                        return;
                    }
                }
            }
            i += 6;
        }
    }
}

/// `getTotalLength()` of an SVG `<path>` as Blink computes it.
pub fn blink_length(path: &RawPath) -> f64 {
    let mut total = 0f32;
    blink_traverse(path, |_, _, l| {
        total += l;
        true
    });
    total as f64
}

/// `getPointAtLength(len)` for path data `d` (parsed into `path`): at length 0 Blink stops on the very
/// first move-to, even when that subpath is empty (`M120,180 m-80,0 ...`).
pub fn svg_point_at_length(d: &str, path: &RawPath, len: f64) -> (f64, f64) {
    if len <= 0.0 {
        let t = tokenize(d);
        if t.first().is_some_and(|c| c.eq_ignore_ascii_case("m")) {
            if let (Some(x), Some(y)) = (t.get(1).and_then(|v| v.parse().ok()), t.get(2).and_then(|v| v.parse().ok())) {
                return (x, y);
            }
        }
    }
    blink_point_at_length(path, len)
}

/// `getPointAtLength(len)` of an SVG `<path>` as Blink computes it: the flat piece that crosses `len`,
/// stepped back along its chord by the overshoot.
pub fn blink_point_at_length(path: &RawPath, len: f64) -> (f64, f64) {
    let want = len.max(0.0) as f32;
    let mut total = 0f32;
    let mut last = path.segs.first().and_then(|s| s.pts.get(..2)).map_or([0.0, 0.0], |p| [p[0] as f32, p[1] as f32]);
    let mut hit = None;
    blink_traverse(path, |a, b, l| {
        total += l;
        last = b;
        if total >= want {
            let slope = (b[1] - a[1]).atan2(b[0] - a[0]);
            let back = want - total;
            hit = Some([b[0] + back * slope.cos(), b[1] + back * slope.sin()]);
            return false;
        }
        true
    });
    let p = hit.unwrap_or(last);
    (p[0] as f64, p[1] as f64)
}

/// GSAP's transformRawPath.
pub fn transform(path: &mut RawPath, a: f64, b: f64, c: f64, d: f64, tx: f64, ty: f64) {
    for seg in &mut path.segs {
        let mut i = 0;
        while i + 1 < seg.pts.len() {
            let (x, y) = (seg.pts[i], seg.pts[i + 1]);
            seg.pts[i] = x * a + y * c + tx;
            seg.pts[i + 1] = x * b + y * d + ty;
            i += 2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_curves_lines_and_arcs() {
        let p = parse("M40,300 C150,40 300,40 360,180 S560,320 600,60");
        assert_eq!(p.segs.len(), 1);
        assert_eq!(p.segs[0].pts.len(), 2 + 12);
        assert_eq!(&p.segs[0].pts[8..14], &[360.0 + 60.0, 320.0, 560.0, 320.0, 600.0, 60.0]);
        let circle = parse("M120,180 m-80,0 a80,80 0 1,0 160,0 a80,80 0 1,0 -160,0z");
        let s = circle.segs.last().unwrap();
        assert!(s.closed);
        assert!((s.pts[0] - 40.0).abs() < 1e-9 && (s.pts[1] - 180.0).abs() < 1e-9);
    }

    fn goldens() -> serde_json::Value {
        serde_json::from_str(include_str!("../tests/gsap-plugin-goldens.json")).unwrap()
    }

    /// MotionPathPlugin's stringToRawPath + cacheRawPathMeasurements(12) + getPositionOnPath(p, true),
    /// sampled from GSAP 3.14.2's real plugin in Chrome (bench/gen-plugin-goldens.cjs; test environment
    /// only, no GSAP code ships).
    #[test]
    fn motion_path_matches_gsap_plugin() {
        let g = goldens();
        let progress: Vec<f64> = g["progress"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let mut bad = Vec::new();
        for case in g["motion"].as_array().unwrap() {
            let d = case["d"].as_str().unwrap();
            let mut p = parse(d);
            measure(&mut p, 12);
            let want_len = case["totalLength"].as_f64().unwrap();
            if (p.total_length - want_len).abs() > 1e-6 * want_len.max(1.0) {
                bad.push(format!("{d}: length {} vs {want_len}", p.total_length));
            }
            for (prog, want) in progress.iter().zip(case["points"].as_array().unwrap()) {
                let w: Vec<f64> = want.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
                let (x, y, a) = position(&p, *prog);
                // GSAP rounds x/y to 5 decimals and the angle (degrees) to 4 in getPositionOnPath
                if (x - w[0]).abs() > 1e-4 || (y - w[1]).abs() > 1e-4 || (a - w[2]).abs() > 1e-3 {
                    bad.push(format!("{d} @{prog}: ours ({x}, {y}, {a}) gsap ({}, {}, {})", w[0], w[1], w[2]));
                }
            }
        }
        assert!(bad.is_empty(), "{} mismatches:\n{}", bad.len(), bad.join("\n"));
    }

    /// SVGPathElement getTotalLength / getPointAtLength against Chrome itself (same generator).
    #[test]
    fn svg_path_geometry_matches_chrome() {
        let g = goldens();
        let mut bad = Vec::new();
        for case in g["geometry"].as_array().unwrap() {
            let d = case["d"].as_str().unwrap();
            let p = parse(d);
            let want = case["total"].as_f64().unwrap();
            let got = blink_length(&p);
            if (got - want).abs() > 0.01 {
                bad.push(format!("{d}: length {got} vs chrome {want}"));
            }
            for pt in case["points"].as_array().unwrap() {
                let w: Vec<f64> = pt.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
                let (x, y) = svg_point_at_length(d, &p, w[0]);
                if (x - w[1]).abs() > 0.05 || (y - w[2]).abs() > 0.05 {
                    bad.push(format!("{d} @{}: ours ({x}, {y}) chrome ({}, {})", w[0], w[1], w[2]));
                }
            }
        }
        assert!(bad.is_empty(), "{} mismatches:\n{}", bad.len(), bad.join("\n"));
    }

    /// MorphSVGPlugin `type: "linear"` path data at progress 0..1, from the real plugin (as above).
    #[test]
    fn morph_matches_gsap_plugin() {
        let g = goldens();
        let ts: Vec<f64> = g["ts"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let nums = |s: &str| -> Vec<f64> {
            s.split(|c: char| c.is_ascii_alphabetic() || c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()).map(|t| t.parse().unwrap()).collect()
        };
        let cmds = |s: &str| -> String { s.chars().filter(|c| c.is_ascii_alphabetic()).collect() };
        let mut bad = Vec::new();
        for m in g["morph"].as_array().unwrap() {
            let shape_index = m["shapeIndex"].as_i64();
            let fill = m["fill"].as_str().unwrap();
            let precision = m["precision"].as_u64().unwrap() as u32;
            let morph = Morph::new(m["start"].as_str().unwrap(), m["end"].as_str().unwrap(), shape_index, m["map"].as_str().unwrap(), fill != "none", precision)
                .expect("morph");
            let tol = 1.01 * 10f64.powi(-(precision as i32));
            for (t, want) in ts.iter().zip(m["ds"].as_array().unwrap()) {
                let want = want.as_str().unwrap();
                let got = morph.at(*t);
                let (gn, wn) = (nums(&got), nums(want));
                let close = gn.len() == wn.len() && gn.iter().zip(&wn).all(|(a, b)| (a - b).abs() <= tol);
                if cmds(&got) != cmds(want) || !close {
                    bad.push(format!("{} -> {} @{t}:\n  ours {got}\n  gsap {want}", m["start"], m["end"]));
                }
            }
        }
        assert!(bad.is_empty(), "{} mismatches:\n{}", bad.len(), bad.join("\n"));
    }

    #[test]
    fn positions_along_a_line() {
        let mut p = parse("M0,0 L100,0");
        measure(&mut p, 12);
        let (x, y, a) = position(&p, 0.25);
        assert!((x - 25.0).abs() < 1e-6 && y == 0.0 && a == 0.0, "{x} {y} {a}");
        assert_eq!(position(&p, 1.0).0, 100.0);
    }
}

// ---- MorphSVGPlugin (linear type) ----------------------------------------------------------------

/// A segment as MorphSVG manipulates it: coordinates plus the cached size/center.
#[derive(Clone, Debug, Default)]
struct MSeg {
    p: Vec<f64>,
    closed: bool,
    reversed: bool,
    size: f64,
    cx: f64,
    cy: f64,
}

fn round5_js(v: f64) -> f64 {
    round5(v)
}

fn get_size(s: &mut MSeg) -> f64 {
    let l = s.p.len();
    let (mut xmax, mut ymax) = (s.p[0], s.p[1]);
    let (mut xmin, mut ymin) = (xmax, ymax);
    let mut i = 6;
    while i + 1 < l {
        let (x, y) = (s.p[i], s.p[i + 1]);
        if x > xmax {
            xmax = x;
        } else if x < xmin {
            xmin = x;
        }
        if y > ymax {
            ymax = y;
        } else if y < ymin {
            ymin = y;
        }
        i += 6;
    }
    s.cx = (xmax + xmin) / 2.0;
    s.cy = (ymax + ymin) / 2.0;
    s.size = (xmax - xmin) * (ymax - ymin);
    s.size
}

fn size_of(s: &mut MSeg) -> f64 {
    if s.size != 0.0 { s.size } else { get_size(s) }
}

/// (centerX, centerY) of the whole path from 3 samples per curve (MorphSVG's _getTotalSize).
fn total_center(path: &[MSeg]) -> (f64, f64) {
    let (mut xmax, mut ymax) = (path[0].p[0], path[0].p[1]);
    let (mut xmin, mut ymin) = (xmax, ymax);
    let inc = 1.0 / 3.0;
    for seg in path.iter().rev() {
        let s = &seg.p;
        let l = s.len();
        let mut i = 6;
        while i + 7 < l {
            let (x1, y1) = (s[i], s[i + 1]);
            let (x2, y2) = (s[i + 2] - x1, s[i + 3] - y1);
            let (x3, y3) = (s[i + 4] - x1, s[i + 5] - y1);
            let (x4, y4) = (s[i + 6] - x1, s[i + 7] - y1);
            for k in (0..3).rev() {
                let t = inc * k as f64;
                let inv = 1.0 - t;
                let x = (t * t * x4 + 3.0 * inv * (t * x3 + inv * x2)) * t + x1;
                let y = (t * t * y4 + 3.0 * inv * (t * y3 + inv * y2)) * t + y1;
                if x > xmax {
                    xmax = x;
                } else if x < xmin {
                    xmin = x;
                }
                if y > ymax {
                    ymax = y;
                } else if y < ymin {
                    ymin = y;
                }
            }
            i += 6;
        }
    }
    ((xmax + xmin) / 2.0, (ymax + ymin) / 2.0)
}

/// Stable insertion sort with a JS-style comparator (negative = a first); never panics on an
/// inconsistent comparator.
fn js_sort<T>(v: &mut [T], mut cmp: impl FnMut(&mut T, &mut T) -> f64) {
    for i in 1..v.len() {
        let mut j = i;
        while j > 0 {
            let (left, right) = v.split_at_mut(j);
            if cmp(&mut left[j - 1], &mut right[0]) > 0.0 {
                v.swap(j - 1, j);
                j -= 1;
            } else {
                break;
            }
        }
    }
}

fn sort_by_size(a: &mut MSeg, b: &mut MSeg) -> f64 {
    let (sa, sb) = (size_of(a), size_of(b));
    if (sb - sa).abs() < (sa + sb) / 20.0 {
        let d = b.cx - a.cx;
        if d != 0.0 { d } else { b.cy - a.cy }
    } else {
        sb - sa
    }
}

fn reverse_segment(s: &mut MSeg) {
    s.p.reverse();
    let mut i = 0;
    while i + 1 < s.p.len() {
        s.p.swap(i, i + 1);
        i += 2;
    }
    s.reversed = !s.reversed;
}

fn offset_segment(s: &mut MSeg, shape_index: i64) {
    let a = s.p.clone();
    let l = s.p.len();
    let wrap = l - 2;
    let mut i = 0;
    while i + 1 < l {
        let index = (i + shape_index as usize) % wrap;
        s.p[i] = a[index];
        s.p[i + 1] = a[index + 1];
        i += 2;
    }
}

fn average_xy(s: &[f64]) -> (f64, f64) {
    let (mut x, mut y) = (0.0, 0.0);
    let mut i = 0;
    while i + 1 < s.len() {
        x += s[i];
        y += s[i + 1];
        i += 2;
    }
    let n = (s.len() / 2) as f64;
    (x / n, y / n)
}

fn total_movement(sb: &[f64], eb: &[f64], shape_index: usize, ox: f64, oy: f64) -> f64 {
    let l = sb.len();
    let wrap = l - 2;
    let shape_index = shape_index * 6;
    let mut d = 0.0;
    let mut i = 0;
    while i < l {
        let index = (i + shape_index) % wrap;
        let y = sb[index] - (eb.get(i).copied().unwrap_or(0.0) - ox);
        let x = sb[index + 1] - (eb.get(i + 1).copied().unwrap_or(0.0) - oy);
        d += (x * x + y * y).sqrt();
        i += 6;
    }
    d
}

fn closest_shape_index(sb: &MSeg, eb: &MSeg, check_reverse: bool) -> i64 {
    let l = sb.p.len();
    let (scx, scy) = average_xy(&sb.p);
    let (ecx, ecy) = average_xy(&eb.p);
    let (ox, oy) = (ecx - scx, ecy - scy);
    let mut min = total_movement(&sb.p, &eb.p, 0, ox, oy);
    let mut min_index: i64 = 0;
    let mut i = 6;
    while i < l {
        let d = total_movement(&sb.p, &eb.p, i / 6, ox, oy);
        if d < min {
            min = d;
            min_index = i as i64;
        }
        i += 6;
    }
    if check_reverse {
        let mut copy = sb.clone();
        reverse_segment(&mut copy);
        let mut i = 6;
        while i < l {
            let d = total_movement(&copy.p, &eb.p, i / 6, ox, oy);
            if d < min {
                min = d;
                min_index = -(i as i64);
            }
            i += 6;
        }
    }
    min_index / 6
}

fn closest_anchor(path: &[MSeg], x: f64, y: f64) -> (f64, f64) {
    let mut best = 1e20;
    let (mut cx, mut cy) = (0.0, 0.0);
    for seg in path.iter().rev() {
        let mut i = 0;
        while i + 1 < seg.p.len() {
            let (dx, dy) = (seg.p[i] - x, seg.p[i + 1] - y);
            let d = (dx * dx + dy * dy).sqrt();
            if d < best {
                best = d;
                cx = seg.p[i];
                cy = seg.p[i + 1];
            }
            i += 6;
        }
    }
    (cx, cy)
}

fn closest_segment(bezier: &mut MSeg, pool: &mut Vec<MSeg>, start: usize, sort_ratio: f64, ox: f64, oy: f64) -> MSeg {
    let min_size = size_of(bezier).min(size_of(&mut pool[start])) * sort_ratio;
    let mut min = 1e20;
    let (cx, cy) = (bezier.cx + ox, bezier.cy + oy);
    let mut index = 0;
    for i in start..pool.len() {
        let size = size_of(&mut pool[i]);
        if size < min_size {
            break;
        }
        let (dx, dy) = (pool[i].cx - cx, pool[i].cy - cy);
        let d = (dx * dx + dy * dy).sqrt();
        if d < min {
            index = i;
            min = d;
        }
    }
    pool.remove(index)
}

fn add_anchors_to_bezier(s: &mut Vec<f64>, i: usize, quantity: usize) {
    let (ax, ay) = (s[i], s[i + 1]);
    let (mut cp1x, mut cp1y, mut cp2x, mut cp2y) = (s[i + 2], s[i + 3], s[i + 4], s[i + 5]);
    let (mut bx, mut by) = (s[i + 6], s[i + 7]);
    let mut q = quantity as i64;
    while q > 0 {
        q -= 1;
        let t = 1.0 - 1.0 / (q as f64 + 2.0);
        let x1a = ax + (cp1x - ax) * t;
        let mut x2 = cp1x + (cp2x - cp1x) * t;
        let y1a = ay + (cp1y - ay) * t;
        let mut y2 = cp1y + (cp2y - cp1y) * t;
        let x1 = x1a + (x2 - x1a) * t;
        let y1 = y1a + (y2 - y1a) * t;
        let x2a = cp2x + (bx - cp2x) * t;
        let y2a = cp2y + (by - cp2y) * t;
        x2 += (x2a - x2) * t;
        y2 += (y2a - y2) * t;
        cp1x = round5_js(x1a);
        cp1y = round5_js(y1a);
        cp2x = round5_js(x1);
        cp2y = round5_js(y1);
        bx = round5_js(x1 + (x2 - x1) * t);
        by = round5_js(y1 + (y2 - y1) * t);
        let insert = [cp1x, cp1y, cp2x, cp2y, bx, by, round5_js(x2), round5_js(y2), round5_js(x2a), round5_js(y2a)];
        s.splice(i + 2..i + 6, insert);
    }
}

fn subdivide_segment_qty(s: &mut Vec<f64>, quantity: usize) {
    let mut distances = Vec::new();
    let l = s.len().saturating_sub(2);
    let mut i = 0;
    while i < l {
        distances.push((s[i] - s[i + 6]).powi(2) + (s[i + 1] - s[i + 7]).powi(2));
        i += 6;
    }
    if distances.is_empty() {
        return;
    }
    let mut add = vec![0usize; distances.len()];
    for _ in 0..quantity {
        // _getLargestIndex: scans from the end, strict >, so the last maximum... first from the end
        let mut max = -1e20;
        let mut li = 0;
        for k in (0..distances.len()).rev() {
            if distances[k] > max {
                max = distances[k];
                li = k;
            }
        }
        add[li] += 1;
        let n = add[li] as f64;
        distances[li] *= n / (n + 1.0);
    }
    for k in (0..distances.len()).rev() {
        if add[k] > 0 {
            add_anchors_to_bezier(s, k * 6, add[k]);
        }
    }
}

/// The equalized start/end of a linear MorphSVG tween, interpolated per frame.
#[derive(Clone, Debug)]
pub struct Morph {
    start: Vec<MSeg>,
    end: Vec<MSeg>,
    rnd: f64,
    end_d: String,
}

impl Morph {
    /// MorphSVG's init for `type: "linear"`: equalize segment counts, point counts and start
    /// points (shapeIndex "auto" or a number, map "size" | "position" | "complexity").
    pub fn new(start_d: &str, end_d: &str, shape_index: Option<i64>, map: &str, fill_safe: bool, precision: u32) -> Option<Morph> {
        let to_m = |p: RawPath| -> Vec<MSeg> { p.segs.into_iter().map(|s| MSeg { p: s.pts, closed: s.closed, ..Default::default() }).collect() };
        let mut start = to_m(parse(start_d));
        let mut end = to_m(parse(end_d));
        if start.is_empty() || end.is_empty() {
            return None;
        }
        let dif = end.len() as i64 - start.len() as i64;
        let start_longer = dif <= 0;
        let by_complexity = map == "complexity";
        let sort_ratio = if map == "position" { 0.0 } else { map.parse::<f64>().unwrap_or(0.8) };
        {
            let (longer, shorter) = if start_longer { (&mut start, &mut end) } else { (&mut end, &mut start) };
            if longer.len() > 1 {
                if by_complexity {
                    js_sort(longer, |a, b| b.p.len() as f64 - a.p.len() as f64);
                    js_sort(shorter, |a, b| b.p.len() as f64 - a.p.len() as f64);
                } else {
                    js_sort(longer, sort_by_size);
                    js_sort(shorter, sort_by_size);
                }
                let (lcx, lcy) = total_center(longer);
                let (scx, scy) = total_center(shorter);
                let (ox, oy) = (lcx - scx, lcy - scy);
                if !by_complexity {
                    for i in 0..shorter.len() {
                        let seg = closest_segment(&mut shorter[i], longer, i, sort_ratio, ox, oy);
                        longer.insert(i, seg);
                    }
                }
            }
            if dif != 0 {
                let need = dif.unsigned_abs() as usize;
                if longer[0].p.len() > shorter[0].p.len() {
                    let q = (longer[0].p.len() - shorter[0].p.len()) / 6;
                    subdivide_segment_qty(&mut shorter[0].p, q);
                }
                let mut i = shorter.len();
                for _ in 0..need {
                    if i >= longer.len() {
                        break;
                    }
                    size_of(&mut longer[i]);
                    let (x, y) = closest_anchor(shorter, longer[i].cx, longer[i].cy);
                    shorter.push(MSeg { p: vec![x, y, x, y, x, y, x, y], ..Default::default() });
                    i += 1;
                }
            }
        }
        let mut reverse = shape_index.is_some_and(|s| s < 0);
        for i in 0..start.len().min(end.len()) {
            let d = end[i].p.len() as i64 - start[i].p.len() as i64;
            if d < 0 {
                subdivide_segment_qty(&mut end[i].p, ((-d) / 6) as usize);
            } else if d > 0 {
                subdivide_segment_qty(&mut start[i].p, (d / 6) as usize);
            }
            if reverse && fill_safe && !start[i].reversed {
                reverse_segment(&mut start[i]);
            }
            // a numeric shapeIndex applies to the first segment; 0 leaves it as is
            let shape_index = if i == 0 { shape_index } else { None };
            if shape_index == Some(0) {
                continue;
            }
            let sb = &start[i];
            let eb = &end[i];
            let n = sb.p.len();
            let closed = sb.closed || ((sb.p[0] - sb.p[n - 2]).abs() < 0.5 && (sb.p[1] - sb.p[n - 1]).abs() < 0.5);
            match shape_index {
                None if closed => {
                    let mut idx = closest_shape_index(sb, eb, i == 0 || !fill_safe);
                    if idx < 0 {
                        reverse = true;
                        reverse_segment(&mut start[i]);
                        idx = -idx;
                    }
                    offset_segment(&mut start[i], idx * 6);
                }
                Some(idx) if closed => {
                    if i > 0 && idx < 0 {
                        reverse_segment(&mut start[i]);
                    }
                    offset_segment(&mut start[i], idx.abs() * 6);
                }
                _ => {
                    let (sb, eb) = (&start[i].p, &end[i].p);
                    let (sl, el) = (sb.len(), eb.len());
                    let auto = shape_index.is_none();
                    let straight = (eb[0] - sb[0]).abs() + (eb[1] - sb[1]).abs() + (eb[el - 2] - sb[sl - 2]).abs() + (eb[el - 1] - sb[sl - 1]).abs();
                    let crossed = (eb[0] - sb[sl - 2]).abs() + (eb[1] - sb[sl - 1]).abs() + (eb[el - 2] - sb[0]).abs() + (eb[el - 1] - sb[1]).abs();
                    if !reverse && ((auto && straight > crossed) || shape_index.is_some_and(|s| s % 2 != 0)) {
                        reverse_segment(&mut start[i]);
                        reverse = true;
                    }
                }
            }
            if start[i].closed != end[i].closed {
                start[i].closed = false;
                end[i].closed = false;
            }
        }
        Some(Morph { start, end, rnd: 10f64.powi(precision as i32), end_d: end_d.to_string() })
    }

    /// The `d` attribute at eased progress `ratio` (MorphSVG's render; the exact end shape at 1).
    pub fn at(&self, ratio: f64) -> String {
        if ratio >= 1.0 {
            return self.end_d.clone();
        }
        let rnd = self.rnd;
        let f = |v: f64| {
            let v = ((v * rnd).trunc()) / rnd;
            if v == 0.0 { "0".to_string() } else { format!("{v}") }
        };
        let mut s = String::new();
        for (a, b) in self.start.iter().zip(&self.end) {
            let v = |i: usize| a.p[i] + (b.p.get(i).copied().unwrap_or(a.p[i]) - a.p[i]) * ratio;
            s.push('M');
            s.push_str(&f(v(0)));
            s.push(' ');
            s.push_str(&f(v(1)));
            s.push_str(" C");
            for i in 2..a.p.len() {
                s.push_str(&f(v(i)));
                s.push(' ');
            }
            if a.closed {
                s.push('z');
            }
        }
        s
    }
}
