//! Previz layouts (docs/previz.feature): each shot's frame as labelled boxes in canvas fractions (0 to 1), scored by
//! fixed rules so a generator can propose many and keep the best. The scorer is deterministic and costs microseconds;
//! its pass or fail is also the good-or-bad mark for training later (KTO).
//!
//!   {"ratio": "9:16", "seconds": 3,
//!    "zones": [{"role": "subject", "x": 0.2, "y": 0.42, "w": 0.6, "h": 0.58, "face": {"x": 0.37, "y": 0.45, "w": 0.26, "h": 0.17}},
//!              {"role": "motion", "x": 0.15, "y": 0.16, "w": 0.7, "h": 0.26},
//!              {"role": "text", "x": 0.12, "y": 0.66, "w": 0.6, "h": 0.07, "words": 6}],
//!    "wants": [{"kind": "eye-line", "min": 0.5, "max": 0.62}, {"kind": "above", "role": "motion", "min_h": 0.2}]}

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn x1(&self) -> f64 {
        self.x + self.w
    }
    pub fn y1(&self) -> f64 {
        self.y + self.h
    }
    pub fn area(&self) -> f64 {
        self.w.max(0.0) * self.h.max(0.0)
    }
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub fn overlap(&self, o: &Rect) -> f64 {
        let w = (self.x1().min(o.x1()) - self.x.max(o.x)).max(0.0);
        let h = (self.y1().min(o.y1()) - self.y.max(o.y)).max(0.0);
        w * h
    }
    /// The gap between two boxes (0 when they touch or overlap).
    pub fn gap(&self, o: &Rect) -> f64 {
        let dx = (o.x - self.x1()).max(self.x - o.x1()).max(0.0);
        let dy = (o.y - self.y1()).max(self.y - o.y1()).max(0.0);
        (dx * dx + dy * dy).sqrt()
    }
    fn inside(&self, o: &Rect) -> bool {
        const E: f64 = 1e-6;
        self.x >= o.x - E && self.y >= o.y - E && self.x1() <= o.x1() + E && self.y1() <= o.y1() + E
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Subject,
    Product,
    Text,
    Motion,
    Logo,
    Button,
    Video,
}

impl Role {
    fn name(self) -> &'static str {
        match self {
            Role::Subject => "subject",
            Role::Product => "product",
            Role::Text => "text",
            Role::Motion => "motion",
            Role::Logo => "logo",
            Role::Button => "button",
            Role::Video => "video",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Zone {
    pub role: Role,
    #[serde(flatten)]
    pub rect: Rect,
    /// subject only: the face inside the zone; the eye line is EYE of the way down it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<Rect>,
    /// text only: how many words it holds, for reading time
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Split {
    #[default]
    None,
    Stacked,
    SideBySide,
    /// the dividing line's angle from horizontal, in degrees
    Diagonal { angle: f64 },
}

/// What the shot asked for, from the creator's words ("headroom for animations above my head").
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Want {
    /// the eye line between these fractions of the height from the top
    EyeLine { min: f64, max: f64 },
    /// a zone of `role` above the head, at least `min_h` of the canvas high
    Above { role: Role, min_h: f64 },
    /// at least one zone of `role`
    Has { role: Role },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Layout {
    #[serde(default = "nine_sixteen")]
    pub ratio: String,
    #[serde(default)]
    pub split: Split,
    pub zones: Vec<Zone>,
    #[serde(default)]
    pub wants: Vec<Want>,
    /// how long the shot is on screen
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f64>,
    /// a Reels placement keeps faces and words inside the centred 4:5 area too (default: on for 9:16)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reels: Option<bool>,
}

fn nine_sixteen() -> String {
    "9:16".into()
}

/// The eyes sit a little under halfway down a face box.
pub const EYE: f64 = 0.45;
/// Reading speed for on-screen text, about the subtitle norm of 17 characters a second.
pub const WORDS_PER_SECOND: f64 = 3.0;
/// Overlap that counts: this share of the smaller box.
const OVERLAP: f64 = 0.02;
/// Lines the eye line and the product like to sit on: thirds and the golden ratio.
const POWER_LINES: [f64; 4] = [1.0 / 3.0, 2.0 / 3.0, 0.382, 0.618];

/// The bands TikTok, Reels and Shorts cover with buttons (top, right, bottom, left as canvas fractions): the house
/// safe zone of 220, 140, 420, 0 px at 1080x1920 (advice platforms/safe-top-bottom-default, platforms/safe-right-default).
/// Other ratios have no known overlay bands here; they keep only the frame edges.
pub fn safe_bands(ratio: &str) -> [f64; 4] {
    match ratio {
        "9:16" => [220.0 / 1920.0, 140.0 / 1080.0, 420.0 / 1920.0, 0.0],
        _ => [0.0; 4],
    }
}

/// The centred 4:5 area of a 9:16 frame that Meta's feed keeps (advice platforms/reels-4x5-area): 1350 of 1920 px.
pub fn reels_area() -> Rect {
    let h = 1350.0 / 1920.0;
    Rect { x: 0.0, y: (1.0 - h) / 2.0, w: 1.0, h }
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub pass: bool,
    pub fails: Vec<String>,
    /// each soft score 0 to 1; only the ones that apply to this layout
    pub soft: Vec<(String, f64)>,
    /// the soft scores' mean, for ranking passing layouts
    pub total: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eye_line: Option<f64>,
}

impl Report {
    pub fn to_json(&self) -> Value {
        let soft: serde_json::Map<String, Value> = self.soft.iter().map(|(k, v)| (k.clone(), json!(r3(*v)))).collect();
        json!({"pass": self.pass, "fails": self.fails, "soft": soft, "total": r3(self.total), "eye_line": self.eye_line.map(r3)})
    }
}

fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// A box the rules look at: a zone, or the face inside a subject zone.
struct Item {
    name: String,
    kind: Kind,
    rect: Rect,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Face,
    Role(Role),
}

fn items(l: &Layout) -> Vec<Item> {
    let mut out = Vec::new();
    let mut count = std::collections::BTreeMap::<Role, usize>::new();
    for z in &l.zones {
        let n = count.entry(z.role).or_default();
        *n += 1;
        let name = z.label.clone().unwrap_or_else(|| if *n == 1 { z.role.name().to_string() } else { format!("{} {}", z.role.name(), n) });
        if let Some(f) = z.face {
            out.push(Item { name: format!("{name}'s face"), kind: Kind::Face, rect: f });
        }
        out.push(Item { name, kind: Kind::Role(z.role), rect: z.rect });
    }
    out
}

/// Pairs that must not overlap: words on words, words or marks on the face or the product, marks on each other.
fn clash(a: Kind, b: Kind) -> bool {
    use Role::*;
    let words_or_mark = |k: Kind| matches!(k, Kind::Role(Text | Logo | Button));
    let pair = |x: Kind, y: Kind| -> bool {
        match (x, y) {
            (Kind::Face, k) => words_or_mark(k) || k == Kind::Role(Motion) || k == Kind::Role(Product),
            (Kind::Role(Product), k) => words_or_mark(k),
            (x, y) => words_or_mark(x) && words_or_mark(y),
        }
    };
    pair(a, b) || pair(b, a)
}

/// Score one layout: hard rules first (any failure fails it), then soft scores to rank the ones that pass.
pub fn score(l: &Layout) -> Report {
    let mut fails = Vec::new();
    let canvas = Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
    let all = items(l);

    for it in &all {
        if it.rect.w <= 0.0 || it.rect.h <= 0.0 {
            fails.push(format!("{} has no size", it.name));
        }
    }
    for z in &l.zones {
        if let Some(f) = z.face {
            if z.role != Role::Subject {
                fails.push(format!("only a subject has a face ({} has one)", z.role.name()));
            } else if !f.inside(&z.rect) {
                fails.push("the face is outside its subject box".into());
            }
        }
    }

    // cut off by the frame, the platform's buttons, or the 4:5 feed crop
    let [top, right, bottom, left] = safe_bands(&l.ratio);
    let safe = Rect { x: left, y: top, w: 1.0 - left - right, h: 1.0 - top - bottom };
    let reels = l.reels.unwrap_or(l.ratio == "9:16");
    let crop = reels_area();
    for it in &all {
        let keep_whole = matches!(it.kind, Kind::Face | Kind::Role(Role::Product | Role::Text | Role::Logo | Role::Button));
        if !keep_whole {
            continue;
        }
        if !it.rect.inside(&canvas) {
            fails.push(format!("{} is cut off by the frame", it.name));
        } else if !it.rect.inside(&safe) {
            fails.push(format!("{} reaches the platform's button bands", it.name));
        } else if reels && !it.rect.inside(&crop) {
            fails.push(format!("{} is outside the 4:5 feed crop", it.name));
        }
    }

    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            if clash(a.kind, b.kind) && a.rect.overlap(&b.rect) > OVERLAP * a.rect.area().min(b.rect.area()) {
                fails.push(format!("{} overlaps {}", a.name, b.name));
            }
        }
    }

    let face = l.zones.iter().find_map(|z| (z.role == Role::Subject).then_some(z.face).flatten());
    let eye_line = face.map(|f| f.y + EYE * f.h);
    for w in &l.wants {
        match w {
            Want::EyeLine { min, max } => match eye_line {
                None => fails.push("the shot wants an eye line but has no face".into()),
                Some(e) if e < *min || e > *max => fails.push(format!("the eye line is at {:.2}, wanted {min:.2} to {max:.2}", e)),
                _ => {}
            },
            Want::Above { role, min_h } => {
                let Some(f) = face else {
                    fails.push(format!("the shot wants {} above the head but has no face", role.name()));
                    continue;
                };
                let room = l.zones.iter().any(|z| {
                    z.role == *role && z.rect.y1() <= f.y + 0.01 && z.rect.h >= *min_h && z.rect.x < f.x1() && z.rect.x1() > f.x
                });
                if !room {
                    fails.push(format!("no {} zone at least {min_h:.2} high above the head", role.name()));
                }
            }
            Want::Has { role } => {
                if !l.zones.iter().any(|z| z.role == *role) {
                    fails.push(format!("the shot wants a {} zone", role.name()));
                }
            }
        }
    }

    let mut soft = Vec::new();

    // power lines: the eye line, the product's centre
    let mut points = Vec::new();
    if let Some(e) = eye_line {
        points.push(e);
    }
    for z in l.zones.iter().filter(|z| z.role == Role::Product) {
        points.push(z.rect.center().1);
    }
    if !points.is_empty() {
        let s: f64 = points.iter().map(|p| 1.0 - (POWER_LINES.iter().map(|l| (p - l).abs()).fold(f64::MAX, f64::min) / 0.08).min(1.0)).sum::<f64>();
        soft.push(("power_lines".to_string(), s / points.len() as f64));
    }

    // balance: weighted centre of what draws the eye, near the frame's centre
    let weight = |k: Kind| match k {
        Kind::Face => 3.0,
        Kind::Role(Role::Product) => 2.0,
        Kind::Role(Role::Text | Role::Button) => 1.5,
        Kind::Role(Role::Logo | Role::Motion) => 1.0,
        Kind::Role(Role::Subject | Role::Video) => 0.5,
    };
    let (mut sx, mut sy, mut sw) = (0.0, 0.0, 0.0);
    for it in &all {
        let w = weight(it.kind) * it.rect.area().max(1e-4).sqrt();
        let (cx, cy) = it.rect.center();
        sx += cx * w;
        sy += cy * w;
        sw += w;
    }
    if sw > 0.0 {
        let d = ((sx / sw - 0.5).powi(2) + (sy / sw - 0.5).powi(2)).sqrt();
        soft.push(("balance".to_string(), 1.0 - (d / 0.3).min(1.0)));
    }

    // breathing room between the face and words or marks
    if let Some(f) = face {
        let gaps: Vec<f64> = all.iter().filter(|it| matches!(it.kind, Kind::Role(Role::Text | Role::Logo | Role::Button))).map(|it| f.gap(&it.rect)).collect();
        if let Some(g) = gaps.into_iter().reduce(f64::min) {
            soft.push(("breathing".to_string(), (g / 0.03).min(1.0)));
        }
    }

    // reading time
    let words: u32 = l.zones.iter().filter(|z| z.role == Role::Text).filter_map(|z| z.words).sum();
    if let (Some(s), true) = (l.seconds, words > 0) {
        soft.push(("reading".to_string(), (s * WORDS_PER_SECOND / words as f64).min(1.0)));
    }

    let total = if soft.is_empty() { 1.0 } else { soft.iter().map(|s| s.1).sum::<f64>() / soft.len() as f64 };
    Report { pass: fails.is_empty(), fails, soft, total, eye_line }
}

/// Rank layouts: passing ones first, then by total. Returns indices into `ls`.
pub fn rank(ls: &[Layout]) -> Vec<(usize, Report)> {
    let mut r: Vec<(usize, Report)> = ls.iter().map(score).enumerate().collect();
    r.sort_by(|a, b| b.1.pass.cmp(&a.1.pass).then(b.1.total.partial_cmp(&a.1.total).unwrap_or(std::cmp::Ordering::Equal)));
    r
}

/// Canvas size in px for a ratio, 1080 wide.
pub fn canvas_px(ratio: &str) -> (u32, u32) {
    match ratio {
        "4:5" => (1080, 1350),
        "1:1" => (1080, 1080),
        "16:9" => (1920, 1080),
        _ => (1080, 1920),
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The layout as a one-second composition the renderer can snapshot: a labelled box per zone, the eye line, the split
/// line, and the safe-zone bands shaded. The boxes keep their roles as classes, so Post can start from the same HTML.
pub fn to_html(l: &Layout) -> String {
    let (w, h) = canvas_px(&l.ratio);
    let (wf, hf) = (w as f64, h as f64);
    let px = |r: &Rect| format!("left:{:.0}px;top:{:.0}px;width:{:.0}px;height:{:.0}px", r.x * wf, r.y * hf, r.w * wf, r.h * hf);
    let mut body = String::new();
    let [top, right, bottom, left] = safe_bands(&l.ratio);
    for (name, r) in [("top", Rect { x: 0.0, y: 0.0, w: 1.0, h: top }), ("right", Rect { x: 1.0 - right, y: 0.0, w: right, h: 1.0 }),
                      ("bottom", Rect { x: 0.0, y: 1.0 - bottom, w: 1.0, h: bottom }), ("left", Rect { x: 0.0, y: 0.0, w: left, h: 1.0 })] {
        if r.area() > 0.0 {
            body.push_str(&format!("<div class=\"band band-{name}\" style=\"{}\"></div>\n", px(&r)));
        }
    }
    match &l.split {
        Split::None => {}
        Split::Stacked => body.push_str(&format!("<div class=\"split\" style=\"left:0;top:{:.0}px;width:{w}px;height:6px\"></div>\n", hf / 2.0 - 3.0)),
        Split::SideBySide => body.push_str(&format!("<div class=\"split\" style=\"left:{:.0}px;top:0;width:6px;height:{h}px\"></div>\n", wf / 2.0 - 3.0)),
        Split::Diagonal { angle } => body.push_str(&format!(
            "<div class=\"split\" style=\"left:{:.0}px;top:{:.0}px;width:{:.0}px;height:6px;transform:rotate({:.1}deg)\"></div>\n",
            -wf, hf / 2.0 - 3.0, wf * 3.0, -angle)),
    }
    let mut count = std::collections::BTreeMap::<Role, usize>::new();
    for z in &l.zones {
        let n = count.entry(z.role).or_default();
        *n += 1;
        let label = z.label.clone().unwrap_or_else(|| match z.words {
            Some(k) => format!("{} · {k} words", z.role.name()),
            None => z.role.name().to_string(),
        });
        body.push_str(&format!("<div class=\"zone {}\" style=\"{}\"><span>{}</span></div>\n", z.role.name(), px(&z.rect), esc(&label)));
        if let Some(f) = z.face {
            body.push_str(&format!("<div class=\"face\" style=\"{}\"></div>\n", px(&f)));
            let e = (f.y + EYE * f.h) * hf;
            body.push_str(&format!("<div class=\"eye-line\" style=\"left:0;top:{:.0}px;width:{w}px;height:4px\"><span>eye line</span></div>\n", e - 2.0));
        }
    }
    format!(
        r#"<!doctype html>
<html><head><style>
html,body{{margin:0;width:{w}px;height:{h}px;background:#14161a;font-family:Inter,Helvetica,sans-serif;color:#fff}}
#root{{position:relative;width:{w}px;height:{h}px;overflow:hidden}}
#root>div{{position:absolute;box-sizing:border-box}}
.band{{background:rgba(255,80,80,.10)}}
.split{{background:#fff;opacity:.5}}
.zone{{border:4px solid;border-radius:18px}}
.zone span{{position:absolute;left:14px;top:10px;font-size:32px;font-weight:700}}
.subject{{border-color:#7aa2ff;background:rgba(122,162,255,.12)}}
.product{{border-color:#ffb84d;background:rgba(255,184,77,.14)}}
.text{{border-color:#f5f5f5;border-style:dashed}}
.motion{{border-color:#c792ff;border-style:dotted;background:rgba(199,146,255,.10)}}
.logo{{border-color:#5fe0b0}}
.button{{border-color:#ff6fa8;background:rgba(255,111,168,.18)}}
.video{{border-color:#9aa4b2;background:rgba(154,164,178,.10)}}
.face{{border:3px solid #7aa2ff;border-radius:50%}}
.eye-line{{background:#7aa2ff}}
.eye-line span{{position:absolute;right:20px;top:-40px;font-size:26px;color:#7aa2ff}}
</style></head><body>
<div id="root" data-composition-id="main" data-start="0" data-duration="1" data-width="{w}" data-height="{h}">
{body}</div>
<script>window.__timelines = {{ main: {{ duration: function () {{ return 1; }}, seek: function () {{}}, pause: function () {{}}, time: function () {{ return 0; }} }} }};</script>
</body></html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn talking_head() -> Layout {
        serde_json::from_value(json!({
            "ratio": "9:16", "seconds": 3,
            "zones": [
                {"role": "subject", "x": 0.2, "y": 0.42, "w": 0.6, "h": 0.58, "face": {"x": 0.37, "y": 0.45, "w": 0.26, "h": 0.17}},
                {"role": "motion", "x": 0.15, "y": 0.16, "w": 0.7, "h": 0.26},
                {"role": "text", "x": 0.12, "y": 0.66, "w": 0.6, "h": 0.07, "words": 6}
            ],
            "wants": [{"kind": "eye-line", "min": 0.5, "max": 0.62}, {"kind": "above", "role": "motion", "min_h": 0.2}]
        }))
        .unwrap()
    }

    fn with(mut l: Layout, i: usize, f: impl FnOnce(&mut Zone)) -> Layout {
        f(&mut l.zones[i]);
        l
    }

    #[test]
    fn a_good_talking_head_passes() {
        let r = score(&talking_head());
        assert!(r.pass, "{:?}", r.fails);
        assert!((r.eye_line.unwrap() - 0.5265).abs() < 1e-9);
        assert!(r.total > 0.5, "{:?}", r.soft);
    }

    #[test]
    fn text_in_the_button_bands_fails() {
        // bottom band: 420 / 1920 = 0.219 of the height
        let l = with(talking_head(), 2, |z| z.rect.y = 0.80);
        assert!(score(&l).fails.iter().any(|f| f.contains("button bands")), "{:?}", score(&l).fails);
        // right band: 140 / 1080 = 0.130 of the width
        let l = with(talking_head(), 2, |z| z.rect.x = 0.35);
        assert!(score(&l).fails.iter().any(|f| f.contains("button bands")));
    }

    #[test]
    fn the_reels_crop_applies_to_9x16_unless_turned_off() {
        // y 0.12 is under the top band (0.115) but above the 4:5 crop (0.148)
        let l = with(talking_head(), 2, |z| z.rect.y = 0.12);
        assert!(score(&l).fails.iter().any(|f| f.contains("4:5")));
        let mut off = l.clone();
        off.reels = Some(false);
        assert!(!score(&off).fails.iter().any(|f| f.contains("4:5")), "{:?}", score(&off).fails);
    }

    #[test]
    fn a_face_cut_off_by_the_frame_fails() {
        let l = with(talking_head(), 0, |z| {
            z.rect = Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
            z.face = Some(Rect { x: 0.4, y: -0.05, w: 0.2, h: 0.15 });
        });
        assert!(score(&l).fails.iter().any(|f| f.contains("face is outside") || f.contains("cut off")));
    }

    #[test]
    fn text_over_the_face_fails_but_text_over_the_body_is_fine() {
        let over_face = with(talking_head(), 2, |z| z.rect = Rect { x: 0.3, y: 0.5, w: 0.4, h: 0.06 });
        assert!(score(&over_face).fails.iter().any(|f| f.contains("face overlaps text")), "{:?}", score(&over_face).fails);
        let over_body = with(talking_head(), 2, |z| z.rect = Rect { x: 0.22, y: 0.70, w: 0.5, h: 0.06 });
        assert!(score(&over_body).pass, "{:?}", score(&over_body).fails);
    }

    #[test]
    fn marks_and_words_must_not_overlap_each_other() {
        let mut l = talking_head();
        l.zones.push(Zone { role: Role::Logo, rect: Rect { x: 0.15, y: 0.66, w: 0.15, h: 0.06 }, face: None, words: None, label: None });
        assert!(score(&l).fails.iter().any(|f| f == "text overlaps logo"), "{:?}", score(&l).fails);
    }

    #[test]
    fn the_shots_wants_are_checked() {
        let low_eyes = with(talking_head(), 0, |z| z.face = Some(Rect { x: 0.37, y: 0.60, w: 0.26, h: 0.17 }));
        assert!(score(&low_eyes).fails.iter().any(|f| f.starts_with("the eye line is at")));
        let short_room = with(talking_head(), 1, |z| z.rect.h = 0.1);
        assert!(score(&short_room).fails.iter().any(|f| f.starts_with("no motion zone")));
        let mut no_product = talking_head();
        no_product.wants.push(Want::Has { role: Role::Product });
        assert!(score(&no_product).fails.iter().any(|f| f == "the shot wants a product zone"));
    }

    #[test]
    fn soft_scores_reward_power_lines_and_reading_time() {
        // eye line exactly on the golden-ratio line versus halfway between lines
        let on = with(talking_head(), 0, |z| z.face = Some(Rect { x: 0.37, y: 0.618 - EYE * 0.17, w: 0.26, h: 0.17 }));
        let s = |l: &Layout, k: &str| score(l).soft.iter().find(|s| s.0 == k).unwrap().1;
        assert!((s(&on, "power_lines") - 1.0).abs() < 1e-9);
        let off = with(talking_head(), 0, |z| z.face = Some(Rect { x: 0.37, y: 0.5 - EYE * 0.17, w: 0.26, h: 0.17 }));
        assert!(s(&off, "power_lines") < 0.2);
        // 30 words in 3 s can't be read; 6 can
        let wordy = with(talking_head(), 2, |z| z.words = Some(30));
        assert!(s(&wordy, "reading") < 0.31);
        assert_eq!(s(&talking_head(), "reading"), 1.0);
    }

    #[test]
    fn rank_puts_passing_layouts_first() {
        let bad = with(talking_head(), 2, |z| z.rect.y = 0.85);
        let r = rank(&[bad, talking_head()]);
        assert_eq!(r[0].0, 1);
        assert!(r[0].1.pass && !r[1].1.pass);
    }

    #[test]
    fn splits_and_html() {
        let mut l = talking_head();
        l.split = Split::Diagonal { angle: 20.0 };
        let back: Layout = serde_json::from_str(&serde_json::to_string(&l).unwrap()).unwrap();
        assert_eq!(back.split, Split::Diagonal { angle: 20.0 });
        let html = to_html(&l);
        assert!(html.contains("data-width=\"1080\" data-height=\"1920\""));
        assert!(html.contains("class=\"zone subject\"") && html.contains("eye line") && html.contains("rotate(-20.0deg)"));
        assert!(html.contains("text · 6 words"));
    }
}
