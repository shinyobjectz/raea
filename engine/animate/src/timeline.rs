//! Timelines (forked from animato-timeline 1.7.1's Timeline, Sequence, AnimationGroup and stagger, with
//! animato-tween's StaggerPattern). Animato's timeline owns its animations and steps them with update(dt). Here a
//! timeline is a schedule: labelled spans placed by the same rules (At::Start, End, Label, Offset, Absolute), sampled
//! by asking where a label is at time t. Composition is data, so it is decided once and seeking costs nothing.

/// Where an entry starts (Animato's At).
#[derive(Clone, Debug, PartialEq)]
pub enum At {
    /// at an absolute time
    Absolute(f64),
    /// at the timeline's start
    Start,
    /// when the last entry so far ends
    End,
    /// with the entry of this label (End when there's none)
    Label(String),
    /// this long after the last entry so far ends (negative overlaps)
    Offset(f64),
}

/// One placed entry.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub label: String,
    pub start: f64,
    pub duration: f64,
}

impl Span {
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }
}

/// Labelled spans in time.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Timeline {
    pub spans: Vec<Span>,
}

impl Timeline {
    pub fn new() -> Self {
        Timeline::default()
    }

    fn resolve(&self, at: &At) -> f64 {
        match at {
            At::Absolute(t) => t.max(0.0),
            At::Start => 0.0,
            At::End => self.duration(),
            At::Label(l) => self.start_of(l).unwrap_or_else(|| self.duration()),
            At::Offset(o) => (self.duration() + o).max(0.0),
        }
    }

    /// An entry `duration` long, placed by `at`.
    pub fn add(mut self, label: impl Into<String>, duration: f64, at: At) -> Self {
        let start = self.resolve(&at);
        self.spans.push(Span { label: label.into(), start, duration: duration.max(0.0) });
        self
    }

    /// Another timeline's entries, shifted to where `at` puts its start.
    pub fn add_timeline(mut self, other: &Timeline, at: At) -> Self {
        let start = self.resolve(&at);
        self.spans.extend(other.spans.iter().map(|s| Span { label: s.label.clone(), start: s.start + start, duration: s.duration }));
        self
    }

    /// The time the last entry ends.
    pub fn duration(&self) -> f64 {
        self.spans.iter().map(Span::end).fold(0.0, f64::max)
    }

    pub fn start_of(&self, label: &str) -> Option<f64> {
        self.spans.iter().find(|s| s.label == label).map(|s| s.start)
    }

    pub fn span(&self, label: &str) -> Option<&Span> {
        self.spans.iter().find(|s| s.label == label)
    }

    /// Time since `label` started at timeline time `t` (negative before it starts).
    pub fn local(&self, label: &str, t: f64) -> Option<f64> {
        self.start_of(label).map(|s| t - s)
    }

    /// `label`'s progress at `t`, 0 before it and 1 after it.
    pub fn progress(&self, label: &str, t: f64) -> Option<f64> {
        self.span(label).map(|s| if s.duration <= 0.0 { if t >= s.start { 1.0 } else { 0.0 } } else { ((t - s.start) / s.duration).clamp(0.0, 1.0) })
    }

    /// Every entry `by` later.
    pub fn shifted(mut self, by: f64) -> Self {
        for s in &mut self.spans {
            s.start += by;
        }
        self
    }
}

/// Entries one after another (Animato's Sequence; `gap` between each, negative to overlap).
pub fn sequence(items: &[(&str, f64)], gap: f64) -> Timeline {
    let mut t = Timeline::new();
    for (i, (l, d)) in items.iter().enumerate() {
        t = t.add(*l, *d, if i == 0 { At::Start } else { At::Offset(gap) });
    }
    t
}

/// Entries together (Animato's AnimationGroup::parallel).
pub fn parallel(items: &[(&str, f64)]) -> Timeline {
    items.iter().fold(Timeline::new(), |t, (l, d)| t.add(*l, *d, At::Start))
}

/// Entries `each` apart, in order (Animato's stagger).
pub fn stagger(items: &[(&str, f64)], each: f64) -> Timeline {
    stagger_by(items, &StaggerPattern::Linear { step: each })
}

/// Entries delayed by a pattern (Animato's AnimationGroup::stagger with a StaggerPattern).
pub fn stagger_by(items: &[(&str, f64)], pattern: &StaggerPattern) -> Timeline {
    let n = items.len();
    items.iter().enumerate().fold(Timeline::new(), |t, (i, (l, d))| t.add(*l, *d, At::Absolute(pattern.delay(i, n))))
}

/// Where a grid stagger spreads from (Animato's GridOrigin).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GridOrigin {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
    Top,
    Bottom,
    Left,
    Right,
}

impl GridOrigin {
    pub fn parse(s: &str) -> Option<GridOrigin> {
        Some(match s {
            "top-left" | "start" => GridOrigin::TopLeft,
            "top-right" => GridOrigin::TopRight,
            "bottom-left" => GridOrigin::BottomLeft,
            "bottom-right" | "end" => GridOrigin::BottomRight,
            "center" => GridOrigin::Center,
            "top" => GridOrigin::Top,
            "bottom" => GridOrigin::Bottom,
            "left" => GridOrigin::Left,
            "right" => GridOrigin::Right,
            _ => return None,
        })
    }
}

/// How each item's delay is chosen (Animato's StaggerPattern; Linear added for the plain case).
#[derive(Clone, Debug, PartialEq)]
pub enum StaggerPattern {
    Linear { step: f64 },
    /// by grid distance (Manhattan) from an origin
    Grid { cols: usize, rows: usize, origin: GridOrigin, step: f64 },
    /// seeded, the same for the same seed and index
    Random { seed: u32, min: f64, max: f64 },
    CenterOut { step: f64 },
    EdgesIn { step: f64 },
}

impl StaggerPattern {
    /// Item `index`'s delay out of `total`.
    pub fn delay(&self, index: usize, total: usize) -> f64 {
        let fin = |v: f64| if v.is_finite() { v.max(0.0) } else { 0.0 };
        match self {
            StaggerPattern::Linear { step } => index as f64 * fin(*step),
            StaggerPattern::Grid { cols, rows, origin, step } => grid_distance(*cols, *rows, *origin, index) as f64 * fin(*step),
            StaggerPattern::Random { seed, min, max } => {
                let lo = fin(*min);
                let hi = if max.is_finite() { max.max(lo) } else { lo };
                let mut x = seed ^ (index as u32).wrapping_mul(0x9E37_79B9);
                x ^= x >> 16;
                x = x.wrapping_mul(0x7FEB_352D);
                x ^= x >> 15;
                x = x.wrapping_mul(0x846C_A68B);
                x ^= x >> 16;
                lo + (hi - lo) * (x as f64 / u32::MAX as f64)
            }
            StaggerPattern::CenterOut { step } => {
                if total <= 1 {
                    return 0.0;
                }
                let i = index.min(total - 1);
                i.abs_diff((total - 1) / 2).min(i.abs_diff(total / 2)) as f64 * fin(*step)
            }
            StaggerPattern::EdgesIn { step } => {
                if total == 0 {
                    return 0.0;
                }
                let i = index.min(total - 1);
                i.min(total - 1 - i) as f64 * fin(*step)
            }
        }
    }
}

fn grid_distance(cols: usize, rows: usize, origin: GridOrigin, index: usize) -> usize {
    let (cols, rows) = (cols.max(1), rows.max(1));
    let index = index.min(cols * rows - 1);
    let (x, y) = (index % cols, index / cols);
    match origin {
        GridOrigin::TopLeft => x + y,
        GridOrigin::TopRight => (cols - 1 - x) + y,
        GridOrigin::BottomLeft => x + (rows - 1 - y),
        GridOrigin::BottomRight => (cols - 1 - x) + (rows - 1 - y),
        GridOrigin::Center => x.abs_diff((cols - 1) / 2).min(x.abs_diff(cols / 2)) + y.abs_diff((rows - 1) / 2).min(y.abs_diff(rows / 2)),
        GridOrigin::Top => y,
        GridOrigin::Bottom => rows - 1 - y,
        GridOrigin::Left => x,
        GridOrigin::Right => cols - 1 - x,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_parallel_and_labels() {
        let t = sequence(&[("card", 7.0), ("title", 8.0), ("cta", 6.0)], -2.0);
        assert_eq!(t.start_of("title"), Some(5.0));
        assert_eq!(t.start_of("cta"), Some(11.0));
        assert_eq!(t.duration(), 17.0);
        let p = parallel(&[("a", 4.0), ("b", 9.0)]);
        assert_eq!((p.start_of("b"), p.duration()), (Some(0.0), 9.0));
        let both = Timeline::new().add_timeline(&t, At::Start).add("chip", 6.0, At::Label("title".into()));
        assert_eq!(both.start_of("chip"), Some(5.0));
        assert_eq!(both.progress("chip", 8.0), Some(0.5));
    }

    #[test]
    fn grid_from_the_centre() {
        let p = StaggerPattern::Grid { cols: 3, rows: 3, origin: GridOrigin::Center, step: 2.0 };
        let d: Vec<f64> = (0..9).map(|i| p.delay(i, 9)).collect();
        assert_eq!(d, vec![4.0, 2.0, 4.0, 2.0, 0.0, 2.0, 4.0, 2.0, 4.0]);
        let s = stagger(&[("a", 6.0), ("b", 6.0), ("c", 6.0)], 3.0);
        assert_eq!(s.start_of("c"), Some(6.0));
    }

    #[test]
    fn random_is_seeded() {
        let p = StaggerPattern::Random { seed: 7, min: 0.0, max: 10.0 };
        assert_eq!(p.delay(3, 10), p.delay(3, 10));
        assert!((0.0..=10.0).contains(&p.delay(5, 10)));
    }
}
