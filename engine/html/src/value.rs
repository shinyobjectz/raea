//! CSS values as GSAP tweens them, made interpolable through animato's `Interpolate` trait.
//!
//! GSAP's CSSPlugin reads the start from the computed style, parses numbers + units, colors and
//! "complex" strings (several numbers inside a template such as `inset(0 40% 0 0)` or
//! `drop-shadow(0 4px 12px rgba(...))`) and interpolates each number. Mismatched units are
//! converted by GSAP; here they become a `calc()` mix, which Stylo resolves to the same pixels.

use animato::Interpolate;

#[derive(Clone, Debug, PartialEq)]
pub enum CssValue {
    Num { v: f32, unit: String },
    Color([f32; 4]),
    /// `parts.len() == nums.len() + 1`; rendered as parts[0] nums[0] parts[1] ... `raw` is the source text of
    /// a parsed value, printed verbatim (GSAP writes a tween's start and end strings as given, colours as rgba: "12,002", not "12,2")
    Complex { nums: Vec<f32>, units: Vec<String>, parts: Vec<String>, raw: Option<String> },
    /// Mismatched units: (1-t)·a + t·b
    Mix { a: Box<CssValue>, b: Box<CssValue>, t: f32 },
    /// Not interpolable; `at_end` decides whether it switches at the first or the last instant.
    Discrete { from: String, to: String, t: f32, at_end: bool },
}

impl Interpolate for CssValue {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        use CssValue::*;
        match (self, other) {
            (Num { v: a, unit: ua }, Num { v: b, unit: ub }) => {
                if ua == ub || ua.is_empty() && *a == 0.0 || ub.is_empty() && *b == 0.0 {
                    let unit = if ub.is_empty() { ua.clone() } else { ub.clone() };
                    Num { v: a + (b - a) * t, unit }
                } else if ua.is_empty() || ub.is_empty() {
                    // unitless vs unit (e.g. line-height 1.2 -> 24px): GSAP adopts the end unit
                    let unit = if ub.is_empty() { ua.clone() } else { ub.clone() };
                    Num { v: a + (b - a) * t, unit }
                } else if t <= 0.0 {
                    self.clone()
                } else if t >= 1.0 && t <= 1.0 {
                    other.clone()
                } else {
                    Mix { a: Box::new(self.clone()), b: Box::new(other.clone()), t }
                }
            }
            (Color(a), Color(b)) => Color([
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
                a[3] + (b[3] - a[3]) * t,
            ]),
            (
                Complex { nums: na, units: ua, .. },
                Complex { nums: nb, units: ub, parts, .. },
            ) if na.len() == nb.len() => {
                // GSAP's _renderComplexString writes the start string at 0 and the end string at 1
                if t <= 0.0 {
                    return self.clone();
                }
                if t >= 1.0 {
                    return other.clone();
                }
                // GSAP rounds the r, g, b channels of colors inside complex strings
                let channels = color_channels(parts, nb.len());
                let nums = na
                    .iter()
                    .zip(nb)
                    .zip(&channels)
                    .map(|((a, b), ch)| {
                        let v = a + (b - a) * t;
                        if ch.is_some_and(|k| k < 3) { v.round() } else { v }
                    })
                    .collect();
                // Units follow the end value (GSAP keeps the end string as the template); a unitless
                // end number takes the start's unit, except color channels, which never have one.
                let units = ub
                    .iter()
                    .zip(ua)
                    .zip(&channels)
                    .map(|((b, a), ch)| if b.is_empty() && ch.is_none() { a.clone() } else { b.clone() })
                    .collect();
                Complex { nums, units, parts: parts.clone(), raw: None }
            }
            _ => {
                let at_end = matches!(other, Discrete { at_end: true, .. });
                Discrete { from: self.to_css(), to: other.to_css(), t, at_end }
            }
        }
    }
}

impl CssValue {
    pub fn to_css(&self) -> String {
        match self {
            CssValue::Num { v, unit } => format!("{}{}", fmt_num(*v), unit),
            CssValue::Color(c) => {
                let r = c[0].round().clamp(0.0, 255.0);
                let g = c[1].round().clamp(0.0, 255.0);
                let b = c[2].round().clamp(0.0, 255.0);
                let a = c[3].clamp(0.0, 1.0);
                if a >= 1.0 {
                    format!("rgb({r}, {g}, {b})")
                } else {
                    format!("rgba({r}, {g}, {b}, {})", fmt_num(a))
                }
            }
            CssValue::Complex { nums, units, parts, raw } => {
                if let Some(raw) = raw {
                    return raw.clone();
                }
                let mut s = String::new();
                for (i, part) in parts.iter().enumerate() {
                    s.push_str(part);
                    if let Some(n) = nums.get(i) {
                        s.push_str(&fmt_num(*n));
                        s.push_str(units.get(i).map(String::as_str).unwrap_or(""));
                    }
                }
                s
            }
            CssValue::Mix { a, b, t } => {
                format!("calc({} * {} + {} * {})", a.to_css(), fmt_num(1.0 - t), b.to_css(), fmt_num(*t))
            }
            CssValue::Discrete { from, to, t, at_end } => {
                let switched = if *at_end { *t >= 1.0 } else { *t > 0.0 };
                if switched { to.clone() } else { from.clone() }
            }
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        match self {
            CssValue::Num { v, .. } => Some(*v),
            CssValue::Mix { a, b, t } => Some(a.as_f32()? * (1.0 - t) + b.as_f32()? * t),
            CssValue::Complex { nums, .. } if nums.len() == 1 => nums.first().copied(),
            CssValue::Discrete { from, to, t, at_end } => {
                let s = if (*at_end && *t >= 1.0) || (!*at_end && *t > 0.0) { to } else { from };
                s.trim().parse().ok()
            }
            _ => None,
        }
    }

    pub fn unit(&self) -> &str {
        match self {
            CssValue::Num { unit, .. } => unit,
            _ => "",
        }
    }

    pub fn num(v: f32, unit: &str) -> CssValue {
        CssValue::Num { v, unit: unit.to_string() }
    }

    pub fn discrete(s: &str) -> CssValue {
        CssValue::Discrete { from: s.to_string(), to: s.to_string(), t: 1.0, at_end: false }
    }

    /// The value as JSON for a plain-object target.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            CssValue::Num { v, unit } if unit.is_empty() => serde_json::json!(*v as f64),
            other => match other.as_f32() {
                Some(n) if other.unit().is_empty() && !matches!(other, CssValue::Discrete { .. }) => serde_json::json!(n as f64),
                _ => serde_json::Value::String(other.to_css()),
            },
        }
    }
}

pub fn fmt_num(v: f32) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let r = (v as f64 * 10000.0).round() / 10000.0;
    if r == r.trunc() {
        format!("{}", r as i64)
    } else {
        let s = format!("{r:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// `+=`, `-=`, `*=` and `/=` prefixes
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RelOp {
    Add,
    Sub,
    Mul,
    Div,
    /// GSAP's directional rotation (`"270_cw"`, `"_ccw"`, `"_short"`): the operand is the absolute end
    Cw,
    Ccw,
    Short,
}

#[derive(Clone, Debug)]
pub enum Raw {
    Abs(CssValue),
    Rel(RelOp, CssValue),
}

impl Raw {
    /// Resolve against the value the tween starts from.
    pub fn resolve(&self, start: &CssValue) -> CssValue {
        match self {
            Raw::Abs(v) => v.clone(),
            Raw::Rel(op, delta) => {
                let (Some(d), Some(s)) = (delta.as_f32(), start.as_f32()) else {
                    return delta.clone();
                };
                let v = match op {
                    RelOp::Add => s + d,
                    RelOp::Sub => s - d,
                    RelOp::Mul => s * d,
                    RelOp::Div => {
                        if d == 0.0 {
                            s
                        } else {
                            s / d
                        }
                    }
                    // CSSPlugin's _addRotationalPropTween
                    RelOp::Cw | RelOp::Ccw | RelOp::Short => {
                        let (cap, big) = (360.0f64, 1e8f64);
                        let mut change = d as f64 - s as f64;
                        let trunc = |v: f64| v.trunc();
                        match op {
                            RelOp::Short => {
                                change %= cap;
                                if change != change % (cap / 2.0) {
                                    change += if change < 0.0 { cap } else { -cap };
                                }
                            }
                            RelOp::Cw if change < 0.0 => change = ((change + cap * big) % cap) - trunc(change / cap) * cap,
                            RelOp::Ccw if change > 0.0 => change = ((change - cap * big) % cap) - trunc(change / cap) * cap,
                            _ => {}
                        }
                        (s as f64 + change) as f32
                    }
                };
                let unit = if delta.unit().is_empty() { start.unit() } else { delta.unit() };
                CssValue::num(v, unit)
            }
        }
    }
}

/// Default unit GSAP appends to a bare number for a property.
pub fn default_unit(prop: &str) -> &'static str {
    match prop {
        "x" | "y" | "z" | "transformPerspective" => "px",
        "rotation" | "rotationX" | "rotationY" | "skewX" | "skewY" => "deg",
        "opacity" | "scale" | "scaleX" | "scaleY" | "xPercent" | "yPercent" | "zIndex" | "fontWeight"
        | "lineHeight" | "flexGrow" | "flexShrink" | "order" | "autoAlpha" | "strokeOpacity" | "fillOpacity"
        | "strokeMiterlimit" => "",
        p if p.starts_with("--") => "",
        p if p.starts_with("attr:") => "",
        // DOM properties GSAP sets on the element (numeric innerText count-ups)
        "innerText" | "textContent" | "innerHTML" | "value" | "scrollTop" | "scrollLeft" => "",
        _ => "px",
    }
}

pub fn is_color_prop(prop: &str) -> bool {
    let p = prop.to_ascii_lowercase();
    p.ends_with("color") || p == "fill" || p == "stroke" || p == "background" || p == "attr:fill" || p == "attr:stroke" || p == "stopcolor"
}

/// Parse an authored or computed value for `prop`. Numbers without unit get the property's
/// default unit (so `x: 100` means 100px, `rotation: 90` means 90deg).
pub fn parse_raw(prop: &str, value: &serde_json::Value) -> Raw {
    match value {
        serde_json::Value::Number(n) => {
            Raw::Abs(CssValue::num(n.as_f64().unwrap_or(0.0) as f32, default_unit(prop)))
        }
        serde_json::Value::Bool(b) => Raw::Abs(CssValue::num(if *b { 1.0 } else { 0.0 }, "")),
        serde_json::Value::String(s) => {
            let s = s.trim();
            for (prefix, op) in [("+=", RelOp::Add), ("-=", RelOp::Sub), ("*=", RelOp::Mul), ("/=", RelOp::Div)] {
                if let Some(rest) = s.strip_prefix(prefix) {
                    return Raw::Rel(op, parse_value(prop, rest));
                }
            }
            if matches!(prop, "rotation" | "rotationX" | "rotationY" | "skewX" | "skewY") {
                for (suffix, op) in [("_short", RelOp::Short), ("_ccw", RelOp::Ccw), ("_cw", RelOp::Cw)] {
                    if let Some(rest) = s.strip_suffix(suffix) {
                        return Raw::Rel(op, parse_value(prop, rest));
                    }
                }
            }
            Raw::Abs(parse_value(prop, s))
        }
        _ => Raw::Abs(CssValue::discrete("")),
    }
}

pub fn parse_value(prop: &str, s: &str) -> CssValue {
    let s = s.trim();
    if let Some((v, unit)) = split_number(s) {
        let unit = unit.to_ascii_lowercase();
        // angles are stored in degrees
        if matches!(prop, "rotation" | "rotationX" | "rotationY" | "skewX" | "skewY") {
            let deg = match unit.as_str() {
                "rad" => v.to_degrees(),
                "turn" => v * 360.0,
                "grad" => v * 0.9,
                _ => v,
            };
            return CssValue::num(deg, "deg");
        }
        let unit = if unit.is_empty() { default_unit(prop).to_string() } else { unit };
        return CssValue::Num { v, unit };
    }
    if is_color_prop(prop) || looks_like_color(s) {
        if let Some(c) = parse_color(s) {
            return CssValue::Color(c);
        }
    }
    parse_complex(s).unwrap_or_else(|| {
        let at_end = matches!((prop, s), ("display", "none") | ("visibility", "hidden") | ("pointerEvents", "none"));
        CssValue::Discrete { from: s.to_string(), to: s.to_string(), t: 1.0, at_end }
    })
}

fn looks_like_color(s: &str) -> bool {
    s.starts_with('#') || s.starts_with("rgb") || s.starts_with("hsl") || s.starts_with("oklch") || s.starts_with("color(")
        || (s.chars().all(|c| c.is_ascii_alphabetic()) && csscolorparser::parse(s).is_ok() && s != "none" && s != "auto")
}

pub fn parse_color(s: &str) -> Option<[f32; 4]> {
    if s.eq_ignore_ascii_case("transparent") {
        return Some([0.0, 0.0, 0.0, 0.0]);
    }
    let c = csscolorparser::parse(s).ok()?;
    Some([c.r * 255.0, c.g * 255.0, c.b * 255.0, c.a])
}

/// A single number with an optional unit: "12", "-3.5px", "50%", "1e-3", ".5em"
pub fn split_number(s: &str) -> Option<(f32, String)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    let num_start = i;
    let mut seen_digit = false;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
        seen_digit |= bytes[i].is_ascii_digit();
        i += 1;
    }
    if !seen_digit || num_start == i && !seen_digit {
        return None;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        if j < bytes.len() && bytes[j].is_ascii_digit() {
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    let v: f32 = s[..i].parse().ok()?;
    let unit = &s[i..];
    if unit.chars().all(|c| c.is_ascii_alphabetic() || c == '%') {
        Some((v, unit.to_string()))
    } else {
        None
    }
}

/// Template + numbers. Colors are normalized to rgba() first so their channels tween too.
pub fn parse_complex(s: &str) -> Option<CssValue> {
    let normalized = normalize_colors(s);
    let mut nums = Vec::new();
    let mut units = Vec::new();
    let mut parts = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = normalized.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let starts_number = c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit()))
            || ((c == '-' || c == '+')
                && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit() || *n == '.')
                && !chars.get(i.wrapping_sub(1)).is_some_and(|p| p.is_ascii_alphanumeric()));
        // identifiers like "rotate3d" or "h1" keep their digits
        let in_ident = i > 0 && (chars[i - 1].is_ascii_alphabetic() || chars[i - 1] == '_' || chars[i - 1] == '#') && c.is_ascii_digit();
        if starts_number && !in_ident {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                j += 1;
            }
            if j < chars.len() && (chars[j] == 'e' || chars[j] == 'E') && chars.get(j + 1).is_some_and(|n| n.is_ascii_digit() || *n == '-') {
                j += 2;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
            }
            let num: String = chars[i..j].iter().collect();
            let Ok(v) = num.parse::<f32>() else {
                cur.push(c);
                i += 1;
                continue;
            };
            let mut k = j;
            while k < chars.len() && (chars[k].is_ascii_alphabetic() || chars[k] == '%') {
                k += 1;
            }
            parts.push(std::mem::take(&mut cur));
            nums.push(v);
            units.push(chars[j..k].iter().collect());
            i = k;
            continue;
        }
        cur.push(c);
        i += 1;
    }
    parts.push(cur);
    if nums.is_empty() {
        return None;
    }
    Some(CssValue::Complex { nums, units, parts, raw: Some(normalized.to_string()) })
}

/// For each number of a complex template, its channel index when it sits inside `rgba(...)`.
fn color_channels(parts: &[String], n: usize) -> Vec<Option<usize>> {
    let mut channel: Option<usize> = None;
    (0..n)
        .map(|i| {
            let part = parts[i].as_str();
            if let Some(open) = part.rfind("rgba(") {
                channel = (!part[open..].contains(')')).then_some(0);
            } else if part.contains(')') {
                channel = None;
            } else if let Some(k) = channel {
                channel = Some(k + 1);
            }
            channel
        })
        .collect()
}

/// GSAP's number-with-unit tokens (`_numWithUnitExp`, `[-+=.]*\d+[.e-]*\d*[a-z%]*`): (start, end).
fn number_tokens(s: &str) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        let mut j = i;
        while j < b.len() && matches!(b[j], b'-' | b'+' | b'=' | b'.') {
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_digit() {
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            while j < b.len() && matches!(b[j], b'.' | b'e' | b'-') {
                j += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            while j < b.len() && (b[j].is_ascii_lowercase() || b[j] == b'%') {
                j += 1;
            }
            out.push((start, j));
            i = j;
        } else {
            i = start + 1;
        }
    }
    out
}

/// Splits a normalized string at its `rgba(...)` colors: (colors, the text around them).
fn split_colors(s: &str) -> (Vec<String>, Vec<String>) {
    let mut colors = Vec::new();
    let mut segments = Vec::new();
    let mut rest = s;
    while let Some(open) = rest.find("rgba(") {
        let Some(close) = rest[open..].find(')') else { break };
        segments.push(rest[..open].to_string());
        colors.push(rest[open..open + close + 1].to_string());
        rest = &rest[open + close + 1..];
    }
    segments.push(rest.to_string());
    (colors, segments)
}

/// GSAP's `_formatColors` order matching: when the start puts its colors in other slots than the
/// end (computed `outline: rgb(0, 0, 0) none 0px` against `2px solid #1ABCFE`), the start's colors
/// move to the end's slots so the numbers pair up as GSAP pairs them.
pub fn align_colors(start: &CssValue, end: &CssValue) -> Option<CssValue> {
    let (CssValue::Complex { .. }, CssValue::Complex { .. }) = (start, end) else { return None };
    let (s, e) = (start.to_css(), end.to_css());
    let (s_colors, s_segments) = split_colors(&s);
    let (_, e_segments) = split_colors(&e);
    if s_colors.is_empty() {
        return None;
    }
    let order = |segments: &[String]| {
        let mut i: i64 = -1;
        segments.iter().map(|seg| { i += number_tokens(seg).len() as i64 + 1; i }).collect::<Vec<_>>()
    };
    let end_order = order(&e_segments);
    if order(&s_segments) == end_order {
        return None;
    }
    let mut numbers: std::collections::VecDeque<String> = s_segments
        .iter()
        .flat_map(|seg| number_tokens(seg).into_iter().map(|(a, b)| seg[a..b].to_string()).collect::<Vec<_>>())
        .collect();
    let mut colors: std::collections::VecDeque<String> = s_colors.into();
    let shell_src = s_segments.join("1");
    let tokens = number_tokens(&shell_src);
    let mut out = String::new();
    let mut last = 0;
    for (i, &(a, b)) in tokens.iter().enumerate() {
        out.push_str(&shell_src[last..a]);
        if end_order.contains(&(i as i64)) {
            out.push_str(&colors.pop_front().unwrap_or_else(|| "rgba(0, 0, 0, 0)".to_string()));
        } else if let Some(n) = numbers.pop_front().or_else(|| colors.pop_front()) {
            out.push_str(&n);
        }
        last = b;
    }
    out.push_str(&shell_src[last..]);
    parse_complex(&out)
}

/// Replace every color token in `s` by `rgba(r, g, b, a)`.
pub fn normalize_colors(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while !rest.is_empty() {
        let lower = rest.to_ascii_lowercase();
        let func = ["rgba(", "rgb(", "hsla(", "hsl(", "oklch(", "oklab(", "color("].iter().find(|f| lower.starts_with(**f));
        if let Some(f) = func {
            if let Some(end) = rest.find(')') {
                let token = &rest[..=end];
                if let Some(c) = parse_color(token) {
                    out.push_str(&rgba_string(c));
                    rest = &rest[end + 1..];
                    continue;
                }
            }
            out.push_str(&rest[..f.len()]);
            rest = &rest[f.len()..];
            continue;
        }
        if rest.starts_with('#') {
            let len = rest[1..].chars().take_while(|c| c.is_ascii_hexdigit()).count();
            if matches!(len, 3 | 4 | 6 | 8) {
                if let Some(c) = parse_color(&rest[..=len]) {
                    out.push_str(&rgba_string(c));
                    rest = &rest[len + 1..];
                    continue;
                }
            }
        }
        let ch = rest.chars().next().unwrap();
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

fn rgba_string(c: [f32; 4]) -> String {
    format!("rgba({}, {}, {}, {})", fmt_num(c[0]), fmt_num(c[1]), fmt_num(c[2]), fmt_num(c[3]))
}

/// When one side is `none` (filter, clip-path, box-shadow...) and the other a function list,
/// build the neutral version of the function list so the numbers can tween from/to it.
pub fn neutral_like(template: &CssValue) -> Option<CssValue> {
    let CssValue::Complex { nums, units, parts, .. } = template else { return None };
    let mut neutral = Vec::with_capacity(nums.len());
    let mut func = String::new();
    for (i, part) in parts.iter().enumerate().take(nums.len()) {
        if let Some(p) = part.rfind('(') {
            func = part[..p].rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '-').next().unwrap_or("").to_ascii_lowercase();
        }
        let identity = match func.as_str() {
            "brightness" | "contrast" | "saturate" | "opacity" | "scale" | "scalex" | "scaley" => 1.0,
            "rgba" | "rgb" => {
                // keep the color, fade alpha to 0
                if i + 1 < nums.len() && parts.get(i + 1).is_some_and(|p| p.contains(')')) { 0.0 } else { nums[i] }
            }
            _ => 0.0,
        };
        neutral.push(identity);
    }
    Some(CssValue::Complex { nums: neutral, units: units.clone(), parts: parts.clone(), raw: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complex_colors_follow_gsap_order_and_units() {
        // Chrome's computed outline puts the color first; GSAP moves it to the end's slot
        let start = parse_complex("rgb(0, 0, 0) none 0px").unwrap();
        let end = parse_complex("2px solid #1ABCFE").unwrap();
        let start = align_colors(&start, &end).unwrap();
        assert_eq!(start.to_css(), "0px none rgba(0, 0, 0, 1)");
        assert_eq!(start.lerp(&end, 1.0).to_css(), "2px solid rgba(26, 188, 254, 1)");
        assert_eq!(start.lerp(&end, 0.5).to_css(), "1px solid rgba(13, 94, 127, 1)");
    }

    #[test]
    fn numbers_units_and_relative() {
        assert_eq!(parse_value("x", "40"), CssValue::num(40.0, "px"));
        assert_eq!(parse_value("rotation", "0.5turn"), CssValue::num(180.0, "deg"));
        let start = CssValue::num(10.0, "px");
        let Raw::Rel(op, d) = parse_raw("x", &serde_json::json!("+=40")) else { panic!() };
        assert_eq!(op, RelOp::Add);
        assert_eq!(Raw::Rel(op, d).resolve(&start), CssValue::num(50.0, "px"));
    }

    #[test]
    fn complex_and_colors() {
        let a = parse_value("clipPath", "inset(0 100% 0 0)");
        let b = parse_value("clipPath", "inset(0 0% 0 0)");
        assert_eq!(a.lerp(&b, 0.5).to_css(), "inset(0 50% 0 0)");
        let c = parse_value("backgroundColor", "#ff0000").lerp(&parse_value("backgroundColor", "rgb(0, 0, 255)"), 0.5);
        assert_eq!(c.to_css(), "rgb(128, 0, 128)");
        let f = parse_value("filter", "blur(10px) brightness(1.5)");
        let n = neutral_like(&f).unwrap();
        assert_eq!(n.to_css(), "blur(0px) brightness(1)");
    }

    #[test]
    fn mismatched_units_mix() {
        let a = CssValue::num(100.0, "px");
        let b = CssValue::num(50.0, "%");
        assert_eq!(a.lerp(&b, 0.25).to_css(), "calc(100px * 0.75 + 50% * 0.25)");
        assert_eq!(a.lerp(&b, 1.0).to_css(), "50%");
    }
}
