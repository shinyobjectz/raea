//! Motion paths (forked from animato-path 1.7.1's bezier module, in f64): quadratic and cubic Bezier curves and a
//! Catmull-Rom spline through points, each evaluated by arc length (t = 0.5 is halfway along the curve, not halfway
//! through its parameter), with the tangent and its angle for something that turns as it travels. A point moving
//! along a path is `path.position(track.value_at(frame))`.

type P = [f64; 2];

const ARC_TABLE_SIZE: usize = 32;

fn clamp01(t: f64) -> f64 {
    if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) }
}
fn lerp(a: P, b: P, t: f64) -> P {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}
fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}
fn scale(v: P, s: f64) -> P {
    [v[0] * s, v[1] * s]
}
fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}
fn dist(a: P, b: P) -> f64 {
    let d = sub(b, a);
    (d[0] * d[0] + d[1] * d[1]).sqrt()
}
fn normalize(v: P) -> P {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l <= f64::EPSILON { [0.0, 0.0] } else { [v[0] / l, v[1] / l] }
}

/// Something a point can travel along.
pub trait PathEvaluate {
    /// The point `t` of the way along (by arc length).
    fn position(&self, t: f64) -> P;
    /// The unit direction of travel there.
    fn tangent(&self, t: f64) -> P;
    /// The direction as an angle in degrees (0 along +x, clockwise with y down, as CSS rotate()).
    fn rotation_deg(&self, t: f64) -> f64 {
        let v = self.tangent(t);
        v[1].atan2(v[0]).to_degrees()
    }
    fn arc_length(&self) -> f64;
}

#[derive(Clone, Debug, PartialEq)]
struct ArcTable {
    cumulative: [f64; ARC_TABLE_SIZE],
    length: f64,
}

impl ArcTable {
    fn build(mut sample: impl FnMut(f64) -> P) -> Self {
        let mut cumulative = [0.0; ARC_TABLE_SIZE];
        let mut prev = sample(0.0);
        let mut length = 0.0;
        for (i, v) in cumulative.iter_mut().enumerate().skip(1) {
            let cur = sample(i as f64 / (ARC_TABLE_SIZE - 1) as f64);
            length += dist(prev, cur);
            *v = length;
            prev = cur;
        }
        ArcTable { cumulative, length }
    }

    /// The curve parameter at arc-length fraction `t`.
    fn raw_t(&self, t: f64) -> f64 {
        let t = clamp01(t);
        if self.length <= f64::EPSILON {
            return t;
        }
        let target = self.length * t;
        if target <= 0.0 {
            return 0.0;
        }
        if target >= self.length {
            return 1.0;
        }
        let upper = self.cumulative.partition_point(|l| *l < target).max(1);
        let lower = upper - 1;
        let span = (self.cumulative[upper] - self.cumulative[lower]).max(f64::EPSILON);
        let local = ((target - self.cumulative[lower]) / span).clamp(0.0, 1.0);
        (lower as f64 + local) / (ARC_TABLE_SIZE - 1) as f64
    }
}

/// A quadratic Bezier curve.
#[derive(Clone, Debug, PartialEq)]
pub struct QuadBezier {
    pub start: P,
    pub control: P,
    pub end: P,
    table: ArcTable,
}

impl QuadBezier {
    pub fn new(start: P, control: P, end: P) -> Self {
        let table = ArcTable::build(|t| Self::sample(start, control, end, t));
        QuadBezier { start, control, end, table }
    }
    fn sample(s: P, c: P, e: P, t: f64) -> P {
        let t = clamp01(t);
        lerp(lerp(s, c, t), lerp(c, e, t), t)
    }
    fn derivative(&self, t: f64) -> P {
        add(scale(sub(self.control, self.start), 2.0 * (1.0 - t)), scale(sub(self.end, self.control), 2.0 * t))
    }
}

impl PathEvaluate for QuadBezier {
    fn position(&self, t: f64) -> P {
        Self::sample(self.start, self.control, self.end, self.table.raw_t(t))
    }
    fn tangent(&self, t: f64) -> P {
        let v = normalize(self.derivative(self.table.raw_t(t)));
        if v == [0.0, 0.0] { normalize(sub(self.position((t + 0.001).min(1.0)), self.position(t))) } else { v }
    }
    fn arc_length(&self) -> f64 {
        self.table.length
    }
}

/// A cubic Bezier curve.
#[derive(Clone, Debug, PartialEq)]
pub struct CubicBezierCurve {
    pub start: P,
    pub control1: P,
    pub control2: P,
    pub end: P,
    table: ArcTable,
}

impl CubicBezierCurve {
    pub fn new(start: P, control1: P, control2: P, end: P) -> Self {
        let table = ArcTable::build(|t| Self::sample(start, control1, control2, end, t));
        CubicBezierCurve { start, control1, control2, end, table }
    }
    fn sample(s: P, c1: P, c2: P, e: P, t: f64) -> P {
        let t = clamp01(t);
        let (a, b, c) = (lerp(s, c1, t), lerp(c1, c2, t), lerp(c2, e, t));
        lerp(lerp(a, b, t), lerp(b, c, t), t)
    }
    fn derivative(&self, t: f64) -> P {
        let mt = 1.0 - t;
        add(add(scale(sub(self.control1, self.start), 3.0 * mt * mt), scale(sub(self.control2, self.control1), 6.0 * mt * t)), scale(sub(self.end, self.control2), 3.0 * t * t))
    }
}

impl PathEvaluate for CubicBezierCurve {
    fn position(&self, t: f64) -> P {
        Self::sample(self.start, self.control1, self.control2, self.end, self.table.raw_t(t))
    }
    fn tangent(&self, t: f64) -> P {
        let v = normalize(self.derivative(self.table.raw_t(t)));
        if v == [0.0, 0.0] { normalize(sub(self.position((t + 0.001).min(1.0)), self.position(t))) } else { v }
    }
    fn arc_length(&self) -> f64 {
        self.table.length
    }
}

/// A smooth curve through every point (centripetal ends clamped to the first and last points).
#[derive(Clone, Debug, PartialEq)]
pub struct CatmullRomSpline {
    pub points: Vec<P>,
    cumulative: Vec<f64>,
    length: f64,
}

impl CatmullRomSpline {
    pub fn new(points: impl Into<Vec<P>>) -> Self {
        let points = points.into();
        let (cumulative, length) = if points.len() < 2 {
            (vec![0.0], 0.0)
        } else {
            let n = ((points.len() - 1) * 16 + 1).max(2);
            let mut cum = vec![0.0; n];
            let mut prev = Self::sample_raw(&points, 0.0);
            let mut len = 0.0;
            for (i, v) in cum.iter_mut().enumerate().skip(1) {
                let cur = Self::sample_raw(&points, i as f64 / (n - 1) as f64);
                len += dist(prev, cur);
                *v = len;
                prev = cur;
            }
            (cum, len)
        };
        CatmullRomSpline { points, cumulative, length }
    }

    fn raw_t(&self, t: f64) -> f64 {
        let t = clamp01(t);
        if self.length <= f64::EPSILON || self.cumulative.len() < 2 {
            return t;
        }
        let target = self.length * t;
        if target <= 0.0 {
            return 0.0;
        }
        if target >= self.length {
            return 1.0;
        }
        let upper = self.cumulative.partition_point(|l| *l < target).max(1);
        let lower = upper - 1;
        let span = (self.cumulative[upper] - self.cumulative[lower]).max(f64::EPSILON);
        let local = ((target - self.cumulative[lower]) / span).clamp(0.0, 1.0);
        (lower as f64 + local) / (self.cumulative.len() - 1) as f64
    }

    fn sample_raw(pts: &[P], raw_t: f64) -> P {
        match pts.len() {
            0 => [0.0, 0.0],
            1 => pts[0],
            2 => lerp(pts[0], pts[1], clamp01(raw_t)),
            len => {
                let segs = len - 1;
                let scaled = clamp01(raw_t) * segs as f64;
                let i = (scaled as usize).min(segs - 1);
                let t = scaled - i as f64;
                let (t2, t3) = (t * t, t * t * t);
                let p0 = if i == 0 { pts[0] } else { pts[i - 1] };
                let (p1, p2) = (pts[i], pts[i + 1]);
                let p3 = if i + 2 < len { pts[i + 2] } else { pts[len - 1] };
                let c = |k: usize| 0.5 * (2.0 * p1[k] + (-p0[k] + p2[k]) * t + (2.0 * p0[k] - 5.0 * p1[k] + 4.0 * p2[k] - p3[k]) * t2 + (-p0[k] + 3.0 * p1[k] - 3.0 * p2[k] + p3[k]) * t3);
                [c(0), c(1)]
            }
        }
    }
}

impl PathEvaluate for CatmullRomSpline {
    fn position(&self, t: f64) -> P {
        Self::sample_raw(&self.points, self.raw_t(t))
    }
    fn tangent(&self, t: f64) -> P {
        if self.points.len() < 2 {
            return [0.0, 0.0];
        }
        let r = self.raw_t(t);
        normalize(sub(Self::sample_raw(&self.points, (r + 0.001).min(1.0)), Self::sample_raw(&self.points, (r - 0.001).max(0.0))))
    }
    fn arc_length(&self) -> f64 {
        self.length
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_hit_their_ends_and_measure_lines() {
        let q = QuadBezier::new([0.0, 0.0], [50.0, 100.0], [100.0, 0.0]);
        assert_eq!((q.position(0.0), q.position(1.0)), ([0.0, 0.0], [100.0, 0.0]));
        let c = CubicBezierCurve::new([0.0, 0.0], [33.0, 0.0], [66.0, 0.0], [100.0, 0.0]);
        assert!((c.arc_length() - 100.0).abs() < 0.01);
        assert!(c.rotation_deg(0.5).abs() < 1e-9);
        let s = CatmullRomSpline::new(vec![[0.0, 0.0], [50.0, 100.0], [100.0, 0.0]]);
        assert_eq!((s.position(0.0), s.position(1.0)), ([0.0, 0.0], [100.0, 0.0]));
    }
}
