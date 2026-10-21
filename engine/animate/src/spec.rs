//! Declared animations: the wire format @raea/motion's `<Animate>` (and `<Mesh animate>`) sends once, when the element
//! mounts, and what Rust makes of it. An `Animation` is keyframe tracks on named properties in the element's local
//! frames; `compile` parses it once (curves, colours) and `Compiled::sample(frame)` evaluates every track at a
//! composition frame, a pure function of that frame. `css` and `patch_3d` turn the sample into the style keys or 3D
//! props the page and the scene draw.
//!
//! Properties: `opacity`, `x`, `y`, `z` (a page element's translate offset; a 3D node's position), `scale`, `scaleX`,
//! `scaleY`, `rotate` (degrees on the page), `rotateX`, `rotateY` (3D), `width`, `height` (with `unit`), `clipTop`,
//! `clipRight`, `clipBottom`, `clipLeft` (inset percentages: a wipe), `blur` (px), `text` (a number written as the
//! element's text, `digits` decimals), any colour property (`color`, `backgroundColor`, `borderColor`, `fill`,
//! `stroke`, 3D `color`/`emissive`: keys carry `color`), and any other name as a plain number on that style key or
//! 3D prop. Tracks on one property combine: x, y, z and rotations add, opacity and scales multiply (so an entrance
//! and an exit compose), anything else takes the latest track that has started.

#[cfg(feature = "bevy")]
use bevy::prelude::{ReflectComponent, ReflectDefault};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::color::{Color, Space};
use crate::easing::Easing;
use crate::track::{Repeat, Track};

/// One element's declared animation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Component, bevy::prelude::Reflect), reflect(Component, Default))]
#[serde(default)]
pub struct Animation {
    /// the composition frame the element's local frame 0 falls on (where its Sequence starts)
    pub origin: f64,
    /// a local frame to hold at whatever the composition's frame (inside a Freeze)
    pub freeze: Option<f64>,
    /// the frames per second its frames count; springs measure their settle time with it (0: the clock's)
    pub fps: f64,
    pub tracks: Vec<AnimTrack>,
    /// the local frame it's shown from, when it shows only for a window (mount="during"); before it, display none
    pub show_from: Option<f64>,
    /// the local frame it's hidden from again
    pub show_until: Option<f64>,
}

/// Keyframes on one property.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Reflect), reflect(Default))]
#[serde(default)]
pub struct AnimTrack {
    pub prop: String,
    pub keys: Vec<AnimKey>,
    /// a length's unit: "px" (when empty) or "%"
    pub unit: String,
    /// past the last key: "" or "hold", "loop", "pingpong" (for ever), or "loop:N" / "pingpong:N"
    pub repeat: String,
    /// decimals a `text` number is written with
    pub digits: u32,
}

/// One key: a value at a local frame, and the curve from it to the next key.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Reflect), reflect(Default))]
#[serde(default)]
pub struct AnimKey {
    pub at: f64,
    pub value: f64,
    /// a colour property's value (CSS text); empty for numbers
    pub color: String,
    /// the curve to the next key (easing::Easing::parse); empty is linear
    pub ease: String,
}

#[derive(Clone, Debug, PartialEq)]
enum Prop {
    Opacity,
    X,
    Y,
    Z,
    Scale,
    ScaleX,
    ScaleY,
    Rotate,
    RotateX,
    RotateY,
    Width,
    Height,
    Clip(usize),
    Blur,
    Text,
    Color(String),
    Number(String),
}

const COLOR_PROPS: &[&str] = &["color", "backgroundColor", "borderColor", "fill", "stroke", "outlineColor", "emissive", "textDecorationColor"];

impl Prop {
    fn of(name: &str, has_color: bool) -> Prop {
        match name {
            "opacity" => Prop::Opacity,
            "x" => Prop::X,
            "y" => Prop::Y,
            "z" => Prop::Z,
            "scale" => Prop::Scale,
            "scaleX" => Prop::ScaleX,
            "scaleY" => Prop::ScaleY,
            "rotate" | "rotateZ" => Prop::Rotate,
            "rotateX" => Prop::RotateX,
            "rotateY" => Prop::RotateY,
            "width" => Prop::Width,
            "height" => Prop::Height,
            "clipTop" => Prop::Clip(0),
            "clipRight" => Prop::Clip(1),
            "clipBottom" => Prop::Clip(2),
            "clipLeft" => Prop::Clip(3),
            "blur" => Prop::Blur,
            "text" => Prop::Text,
            n if has_color || COLOR_PROPS.contains(&n) => Prop::Color(n.to_string()),
            n => Prop::Number(n.to_string()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ColorKey {
    Parsed(Color),
    /// anything Rust can't read (a CSS variable): blended by the page's own color-mix()
    Raw(String),
}

#[derive(Clone, Debug, PartialEq)]
enum Values {
    Num(Track<f64>),
    Color(Track<ColorKey>),
}

#[derive(Clone, Debug, PartialEq)]
struct CTrack {
    prop: Prop,
    values: Values,
    unit: String,
    digits: u32,
    start: f64,
    /// the first track on the same property (override properties pick one of them each frame)
    group: usize,
}

/// An animation ready to sample.
#[derive(Clone, Debug, PartialEq)]
pub struct Compiled {
    pub origin: f64,
    pub freeze: Option<f64>,
    pub show_from: Option<f64>,
    pub show_until: Option<f64>,
    tracks: Vec<CTrack>,
    /// when its last key lands (local frames), repeats aside
    pub end: f64,
}

fn repeat_of(s: &str) -> Result<Repeat, String> {
    let (k, n) = match s.split_once(':') {
        Some((k, n)) => (k, Some(n.trim().parse::<u32>().map_err(|_| format!("repeat {s:?}: {n:?} isn't a count"))?)),
        None => (s, None),
    };
    Ok(match k.trim() {
        "" | "hold" | "once" => Repeat::Once,
        "loop" => Repeat::Loop(n),
        "pingpong" => Repeat::PingPong(n),
        other => return Err(format!("no repeat named {other:?} (hold, loop, pingpong)")),
    })
}

/// The animation parsed once: its curves and colours. An error names the track and what's wrong with it.
pub fn compile(a: &Animation) -> Result<Compiled, String> {
    let fps = if a.fps > 0.0 { a.fps } else { 30.0 };
    let mut tracks = Vec::with_capacity(a.tracks.len());
    let mut end: f64 = 0.0;
    for t in &a.tracks {
        if t.keys.is_empty() {
            continue;
        }
        let has_color = t.keys.iter().any(|k| !k.color.is_empty());
        let prop = Prop::of(&t.prop, has_color);
        let repeat = repeat_of(&t.repeat).map_err(|e| format!("{}: {e}", t.prop))?;
        let mut eases = Vec::with_capacity(t.keys.len());
        for k in &t.keys {
            if !k.at.is_finite() {
                return Err(format!("{}: a key's frame isn't a finite number", t.prop));
            }
            eases.push(Easing::parse(&k.ease, fps).map_err(|e| format!("{}: {e}", t.prop))?);
        }
        let values = if matches!(prop, Prop::Color(_)) {
            let mut tr = Track::new().repeat(repeat);
            for (k, e) in t.keys.iter().zip(eases) {
                let c = Color::parse(&k.color).map(ColorKey::Parsed).unwrap_or_else(|| ColorKey::Raw(k.color.clone()));
                tr = tr.key_eased(k.at, c, e);
            }
            Values::Color(tr)
        } else {
            let mut tr = Track::new().repeat(repeat);
            for (k, e) in t.keys.iter().zip(eases) {
                if !k.value.is_finite() {
                    return Err(format!("{}: a key's value isn't a finite number", t.prop));
                }
                tr = tr.key_eased(k.at, k.value, e);
            }
            Values::Num(tr)
        };
        let start = t.keys.iter().map(|k| k.at).fold(f64::INFINITY, f64::min);
        end = end.max(t.keys.iter().map(|k| k.at).fold(f64::NEG_INFINITY, f64::max));
        let group = tracks.iter().position(|o: &CTrack| o.prop == prop).unwrap_or(tracks.len());
        tracks.push(CTrack { prop, values, unit: if t.unit.is_empty() { "px".into() } else { t.unit.clone() }, digits: t.digits, start, group });
    }
    Ok(Compiled { origin: a.origin, freeze: a.freeze, show_from: a.show_from, show_until: a.show_until, tracks, end })
}

/// What one frame of an animation sets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sampled {
    pub opacity: Option<f64>,
    pub x: Option<(f64, String)>,
    pub y: Option<(f64, String)>,
    pub z: Option<f64>,
    pub scale_x: Option<f64>,
    pub scale_y: Option<f64>,
    pub rotate: Option<f64>,
    pub rotate_x: Option<f64>,
    pub rotate_y: Option<f64>,
    pub width: Option<(f64, String)>,
    pub height: Option<(f64, String)>,
    pub clip: [Option<f64>; 4],
    pub blur: Option<f64>,
    pub text: Option<String>,
    /// colour properties and their colour for the page (rgba() or color-mix()) and, when known, for 3D (#rrggbb)
    pub colors: Vec<(String, String, Option<String>)>,
    pub numbers: Vec<(String, f64)>,
    /// outside its show window: not laid out (display none)
    pub hidden: bool,
}

/// A number as CSS writes it: no "-0", no exponent for tiny values.
pub fn num(v: f64) -> String {
    let v = if v.abs() < 1e-9 { 0.0 } else { v };
    let r = (v * 1e6).round() / 1e6;
    format!("{r}")
}

impl Compiled {
    /// The element's local frame at composition frame `frame`.
    pub fn local(&self, frame: f64) -> f64 {
        self.freeze.unwrap_or(frame - self.origin)
    }

    /// Every track at composition frame `frame`.
    pub fn sample(&self, frame: f64) -> Sampled {
        let t = self.local(frame);
        let mut s = Sampled { hidden: self.show_from.is_some_and(|a| t < a) || self.show_until.is_some_and(|b| t >= b), ..Default::default() };
        // override properties: the latest track that has started (else the earliest) on that property
        let wins = |i: usize| {
            let g = self.tracks[i].group;
            let mut best = i;
            for (j, o) in self.tracks.iter().enumerate() {
                if o.group != g || j == best {
                    continue;
                }
                let cur = &self.tracks[best];
                let better = if o.start <= t { cur.start > t || o.start >= cur.start } else { cur.start > t && o.start < cur.start };
                if better {
                    best = j;
                }
            }
            best == i
        };
        for (i, tr) in self.tracks.iter().enumerate() {
            match &tr.values {
                Values::Num(track) => {
                    let Some(v) = track.value_at(t) else { continue };
                    let add = |o: &mut Option<f64>| *o = Some(o.unwrap_or(0.0) + v);
                    let mul = |o: &mut Option<f64>| *o = Some(o.unwrap_or(1.0) * v);
                    match &tr.prop {
                        Prop::Opacity => mul(&mut s.opacity),
                        Prop::X => s.x = Some((s.x.as_ref().map_or(0.0, |x| x.0) + v, tr.unit.clone())),
                        Prop::Y => s.y = Some((s.y.as_ref().map_or(0.0, |y| y.0) + v, tr.unit.clone())),
                        Prop::Z => add(&mut s.z),
                        Prop::Scale => {
                            mul(&mut s.scale_x);
                            mul(&mut s.scale_y);
                        }
                        Prop::ScaleX => mul(&mut s.scale_x),
                        Prop::ScaleY => mul(&mut s.scale_y),
                        Prop::Rotate => add(&mut s.rotate),
                        Prop::RotateX => add(&mut s.rotate_x),
                        Prop::RotateY => add(&mut s.rotate_y),
                        _ if !wins(i) => {}
                        Prop::Width => s.width = Some((v, tr.unit.clone())),
                        Prop::Height => s.height = Some((v, tr.unit.clone())),
                        Prop::Clip(k) => s.clip[*k] = Some(v),
                        Prop::Blur => s.blur = Some(v.max(0.0)),
                        Prop::Text => {
                            let d = tr.digits.min(6) as usize;
                            let txt = format!("{:.*}", d, v);
                            s.text = Some(if txt.trim_start_matches('-').chars().all(|c| c == '0' || c == '.') { txt.trim_start_matches('-').to_string() } else { txt });
                        }
                        Prop::Number(n) => s.numbers.push((n.clone(), v)),
                        Prop::Color(_) => {}
                    }
                }
                Values::Color(track) => {
                    if !wins(i) {
                        continue;
                    }
                    let Prop::Color(name) = &tr.prop else { continue };
                    let Some((k, p)) = track.locate(t) else { continue };
                    let keys = track.keys();
                    let a = &keys[k].value;
                    let (css, hex) = if p == 0.0 || k + 1 >= keys.len() {
                        match a {
                            ColorKey::Parsed(c) => (c.css(false), Some(c.css(true))),
                            ColorKey::Raw(r) => (r.clone(), None),
                        }
                    } else {
                        match (a, &keys[k + 1].value) {
                            (ColorKey::Parsed(x), ColorKey::Parsed(y)) => {
                                let c = x.mix(y, p, Space::Oklab);
                                (c.css(false), Some(c.css(true)))
                            }
                            (x, y) => {
                                let txt = |c: &ColorKey| match c {
                                    ColorKey::Parsed(c) => c.css(false),
                                    ColorKey::Raw(r) => r.clone(),
                                };
                                (format!("color-mix(in oklab, {} {}%, {})", txt(x), num((1.0 - p) * 100.0), txt(y)), None)
                            }
                        }
                    };
                    s.colors.push((name.clone(), css, hex));
                }
            }
        }
        s
    }
}

impl Sampled {
    /// The page's style keys (React's camelCase names, as the DOM mirror's style objects hold them). Transforms go to
    /// the individual `translate`, `scale` and `rotate` properties, so an element's own `transform` stays its own.
    pub fn css(&self) -> Map<String, Value> {
        let mut m = Map::new();
        let n = |v: f64| serde_json::Number::from_f64(if v.abs() < 1e-9 { 0.0 } else { (v * 1e6).round() / 1e6 }).map(Value::Number).unwrap_or(Value::from(0));
        if let Some(o) = self.opacity {
            m.insert("opacity".into(), n(o.clamp(0.0, 1.0)));
        }
        if self.x.is_some() || self.y.is_some() || self.z.is_some() {
            let (x, ux) = self.x.clone().unwrap_or((0.0, "px".into()));
            let (y, uy) = self.y.clone().unwrap_or((0.0, "px".into()));
            let mut t = format!("{}{ux} {}{uy}", num(x), num(y));
            if let Some(z) = self.z {
                t.push_str(&format!(" {}px", num(z)));
            }
            m.insert("translate".into(), Value::String(t));
        }
        if self.scale_x.is_some() || self.scale_y.is_some() {
            m.insert("scale".into(), Value::String(format!("{} {}", num(self.scale_x.unwrap_or(1.0)), num(self.scale_y.unwrap_or(1.0)))));
        }
        if let Some(r) = self.rotate {
            m.insert("rotate".into(), Value::String(format!("{}deg", num(r))));
        }
        if let Some((w, u)) = &self.width {
            m.insert("width".into(), Value::String(format!("{}{u}", num(w.max(0.0)))));
        }
        if let Some((h, u)) = &self.height {
            m.insert("height".into(), Value::String(format!("{}{u}", num(h.max(0.0)))));
        }
        if self.clip.iter().any(Option::is_some) {
            let c = self.clip.map(|v| num(v.unwrap_or(0.0).clamp(0.0, 100.0)));
            m.insert("clipPath".into(), Value::String(format!("inset({}% {}% {}% {}%)", c[0], c[1], c[2], c[3])));
        }
        if let Some(b) = self.blur {
            m.insert("filter".into(), Value::String(format!("blur({}px)", num(b))));
        }
        for (k, css, _) in &self.colors {
            m.insert(k.clone(), Value::String(css.clone()));
        }
        for (k, v) in &self.numbers {
            m.insert(k.clone(), n(*v));
        }
        m
    }

    /// A 3D node's props (its data-props) with what this frame sets: x, y, z its position, rotateX/Y and rotate its
    /// rotation (radians, as the 3D props take them), scale (a number when uniform), opacity, colours as #rrggbb and
    /// any other number under its own name. True when anything changed.
    pub fn patch_3d(&self, props: &mut Map<String, Value>) -> bool {
        let before = props.clone();
        let n = |v: f64| serde_json::Number::from_f64(v).map(Value::Number).unwrap_or(Value::from(0));
        let vec3 = |props: &Map<String, Value>, k: &str, d: f64| -> [f64; 3] {
            match props.get(k) {
                Some(Value::Array(a)) => std::array::from_fn(|i| a.get(i).and_then(Value::as_f64).unwrap_or(d)),
                Some(Value::Number(x)) => [x.as_f64().unwrap_or(d); 3],
                _ => [d; 3],
            }
        };
        if self.x.is_some() || self.y.is_some() || self.z.is_some() {
            let mut p = vec3(props, "position", 0.0);
            if let Some((x, _)) = &self.x {
                p[0] = *x;
            }
            if let Some((y, _)) = &self.y {
                p[1] = *y;
            }
            if let Some(z) = self.z {
                p[2] = z;
            }
            props.insert("position".into(), Value::Array(p.iter().map(|v| n(*v)).collect()));
        }
        if self.rotate_x.is_some() || self.rotate_y.is_some() || self.rotate.is_some() {
            let mut r = vec3(props, "rotation", 0.0);
            for (i, v) in [self.rotate_x, self.rotate_y, self.rotate].into_iter().enumerate() {
                if let Some(v) = v {
                    r[i] = v;
                }
            }
            props.insert("rotation".into(), Value::Array(r.iter().map(|v| n(*v)).collect()));
        }
        if self.scale_x.is_some() || self.scale_y.is_some() {
            let (sx, sy) = (self.scale_x.unwrap_or(1.0), self.scale_y.unwrap_or(1.0));
            props.insert("scale".into(), if sx == sy { n(sx.max(1e-6)) } else { Value::Array(vec![n(sx.max(1e-6)), n(sy.max(1e-6)), n(1.0)]) });
        }
        if let Some(o) = self.opacity {
            props.insert("opacity".into(), n(o.clamp(0.0, 1.0)));
        }
        for (k, css, hex) in &self.colors {
            props.insert(k.clone(), Value::String(hex.clone().unwrap_or_else(|| css.clone())));
        }
        for (k, v) in &self.numbers {
            props.insert(k.clone(), n(*v));
        }
        *props != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(at: f64, value: f64, ease: &str) -> AnimKey {
        AnimKey { at, value, color: String::new(), ease: ease.into() }
    }
    fn track(prop: &str, keys: Vec<AnimKey>) -> AnimTrack {
        AnimTrack { prop: prop.into(), keys, ..Default::default() }
    }

    #[test]
    fn a_rise_sampled_at_composition_frames() {
        let a = Animation { origin: 100.0, fps: 30.0, tracks: vec![track("y", vec![key(0.0, 30.0, "out-cubic"), key(6.0, 0.0, "")]), track("opacity", vec![key(0.0, 0.0, "out-cubic"), key(5.0, 1.0, "")])], ..Default::default() };
        let c = compile(&a).unwrap();
        let css = c.sample(100.0).css();
        assert_eq!(css["translate"], "0px 30px");
        assert_eq!(css["opacity"], 0.0);
        let css = c.sample(106.0).css();
        assert_eq!(css["translate"], "0px 0px");
        assert_eq!(css["opacity"], 1.0);
        // between keys, out-cubic: at frame 3 of 6, 30 * (1 - 0.875)
        assert_eq!(c.sample(103.0).css()["translate"], "0px 3.75px");
    }

    #[test]
    fn enter_and_exit_compose() {
        let a = Animation {
            fps: 30.0,
            tracks: vec![
                track("opacity", vec![key(0.0, 0.0, "out-cubic"), key(8.0, 1.0, "")]),
                track("opacity", vec![key(40.0, 1.0, "in-cubic"), key(48.0, 0.0, "")]),
                track("x", vec![key(0.0, -64.0, "out-cubic"), key(8.0, 0.0, "")]),
                track("x", vec![key(40.0, 0.0, "in-cubic"), key(48.0, -64.0, "")]),
            ],
            ..Default::default()
        };
        let c = compile(&a).unwrap();
        assert_eq!(c.sample(20.0).css()["opacity"], 1.0);
        assert_eq!(c.sample(48.0).css()["opacity"], 0.0);
        assert_eq!(c.sample(20.0).css()["translate"], "0px 0px");
        assert_eq!(c.sample(48.0).css()["translate"], "-64px 0px");
    }

    #[test]
    fn colours_blend_or_fall_back_to_color_mix() {
        let col = |at: f64, c: &str| AnimKey { at, value: 0.0, color: c.into(), ease: String::new() };
        let a = Animation { fps: 30.0, tracks: vec![AnimTrack { prop: "backgroundColor".into(), keys: vec![col(0.0, "#000000"), col(10.0, "#ffffff")], ..Default::default() }, AnimTrack { prop: "color".into(), keys: vec![col(0.0, "var(--a)"), col(10.0, "#ffffff")], ..Default::default() }], ..Default::default() };
        let c = compile(&a).unwrap();
        let s = c.sample(0.0).css();
        assert_eq!(s["backgroundColor"], "rgba(0, 0, 0, 1)");
        assert_eq!(s["color"], "var(--a)");
        let s = c.sample(5.0).css();
        assert!(s["color"].as_str().unwrap().starts_with("color-mix(in oklab, var(--a) 50%"));
        assert_eq!(c.sample(10.0).css()["backgroundColor"], "rgba(255, 255, 255, 1)");
    }

    #[test]
    fn text_counts_and_wipes_clip() {
        let a = Animation { fps: 30.0, tracks: vec![AnimTrack { prop: "text".into(), keys: vec![key(0.0, 0.0, "out-cubic"), key(24.0, 20.0, "")], ..Default::default() }, track("clipRight", vec![key(0.0, 100.0, "in-out-cubic"), key(8.0, 0.0, "")])], ..Default::default() };
        let c = compile(&a).unwrap();
        assert_eq!(c.sample(0.0).text.as_deref(), Some("0"));
        assert_eq!(c.sample(24.0).text.as_deref(), Some("20"));
        assert_eq!(c.sample(4.0).css()["clipPath"], "inset(0% 50% 0% 0%)");
    }

    #[test]
    fn freeze_holds_and_3d_patches() {
        let a = Animation { origin: 10.0, freeze: Some(3.0), fps: 30.0, tracks: vec![track("x", vec![key(0.0, 0.0, ""), key(6.0, 6.0, "")]), track("scale", vec![key(0.0, 0.5, ""), key(6.0, 1.0, "")])], ..Default::default() };
        let c = compile(&a).unwrap();
        assert_eq!(c.sample(500.0).x.unwrap().0, 3.0);
        let mut p: Map<String, Value> = serde_json::from_str(r#"{"position":[1,2,3],"scale":1}"#).unwrap();
        assert!(c.sample(0.0).patch_3d(&mut p));
        assert_eq!(p["position"], serde_json::json!([3.0, 2.0, 3.0]));
        assert_eq!(p["scale"], serde_json::json!(0.75));
    }

    #[test]
    fn a_show_window_hides_outside_it() {
        let a = Animation { origin: 100.0, fps: 30.0, show_from: Some(5.0), show_until: Some(20.0), tracks: vec![track("opacity", vec![key(5.0, 0.0, ""), key(10.0, 1.0, "")])], ..Default::default() };
        let c = compile(&a).unwrap();
        assert!(c.sample(104.0).hidden && !c.sample(105.0).hidden && !c.sample(119.0).hidden && c.sample(120.0).hidden);
    }

    #[test]
    fn bad_curves_are_named() {
        let a = Animation { fps: 30.0, tracks: vec![track("y", vec![key(0.0, 1.0, "outCubic"), key(1.0, 0.0, "")])], ..Default::default() };
        assert!(compile(&a).unwrap_err().starts_with("y: no easing named"));
    }

    #[test]
    fn spring_keys_overshoot_and_land() {
        let a = Animation { fps: 30.0, tracks: vec![track("scale", vec![key(0.0, 0.6, "spring(1,10,100)"), key(30.0, 1.0, "")])], ..Default::default() };
        let c = compile(&a).unwrap();
        let peak = (0..30).map(|f| c.sample(f as f64).scale_x.unwrap()).fold(0.0, f64::max);
        assert!(peak > 1.0, "peak {peak}");
        assert_eq!(c.sample(30.0).scale_x, Some(1.0));
    }
}
