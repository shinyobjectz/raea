//! The effects library: effects/catalog.json (each effect's name, status and parameters) and
//! effects/fx.js (the runtime that expands `data-fx="glow(14px, #7cf)"` into an SVG filter, the same
//! markup Chrome and WebKit render). The renderer runs fx.js when a composition uses `data-fx` without
//! including it, and lint checks every `data-fx` value against the catalog.

use serde_json::Value;

/// effects/fx.js
pub const FX_JS: &str = include_str!("../effects/fx.js");
/// effects/catalog.json
pub const CATALOG: &str = include_str!("../effects/catalog.json");

fn effects() -> &'static [Value] {
    static C: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    let c = C.get_or_init(|| serde_json::from_str(CATALOG).unwrap_or(Value::Null));
    c["effects"].as_array().map(Vec::as_slice).unwrap_or(&[])
}

/// The effect calls in a `data-fx` value, as fx.js reads them: `glow(14px, #7cf) grain` ->
/// `[("glow", ["14px", "#7cf"]), ("grain", [])]`. Commas inside nested parentheses (`rgb(…)`) stay put.
pub fn parse(spec: &str) -> Vec<(String, Vec<String>)> {
    let b = spec.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'-') {
            i += 1;
        }
        let name = spec[start..i].to_ascii_lowercase();
        let mut j = i;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        let mut args = Vec::new();
        if j < b.len() && b[j] == b'(' {
            let mut depth = 0;
            let mut end = None;
            for (k, c) in b.iter().enumerate().skip(j) {
                match c {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(k);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let close = end.unwrap_or(b.len());
            let inner = &spec[j + 1..close];
            if !inner.trim().is_empty() {
                let (mut depth, mut cur) = (0, String::new());
                for c in inner.chars() {
                    match c {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {}
                    }
                    if c == ',' && depth == 0 {
                        args.push(cur.trim().to_string());
                        cur.clear();
                    } else {
                        cur.push(c);
                    }
                }
                args.push(cur.trim().to_string());
            }
            i = close + 1;
        }
        out.push((name, args));
    }
    out
}

/// What's wrong with one `data-fx` value, each problem with how to fix it (empty when it's fine).
pub fn check(spec: &str) -> Vec<String> {
    let all = effects();
    let ids: Vec<&str> = all.iter().filter_map(|e| e["id"].as_str()).collect();
    let mut problems = Vec::new();
    let calls = parse(spec);
    if calls.is_empty() {
        problems.push(format!("no effect given; use one of: {}", ids.join(", ")));
    }
    for (name, args) in calls {
        let Some(effect) = all.iter().find(|e| e["id"].as_str() == Some(name.as_str())) else {
            let near = ids.iter().copied().min_by_key(|id| edit_distance(id, &name)).filter(|id| edit_distance(id, &name) <= 2);
            let hint = near.map(|n| format!(" (did you mean \"{n}\"?)")).unwrap_or_default();
            problems.push(format!("unknown effect \"{name}\"{hint}; effects: {}", ids.join(", ")));
            continue;
        };
        let params = effect["params"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        let form = effect["dsl"].as_str().unwrap_or(&name);
        if args.len() > params.len() {
            problems.push(format!("{name} takes at most {} argument(s), got {}: {form}", params.len(), args.len()));
        }
        // an argument "a..b" animates from a to b over the element's time (fx.js), "a..b..c@0s..1s..2s"
        // through keyframes at those times; check every value (and the times)
        let mut expanded: Vec<(String, &Value)> = Vec::new();
        for (a, p) in args.iter().zip(params) {
            let (values, times) = match a.split_once('@') {
                Some((v, t)) => (v, Some(t)),
                None => (a.as_str(), None),
            };
            let stops: Vec<String> = values.split("..").map(|v| v.trim().to_string()).collect();
            if let Some(times) = times {
                let pname = p["name"].as_str().unwrap_or("value");
                let ts: Vec<Option<f64>> = times.split("..").map(seconds).collect();
                if ts.iter().any(Option::is_none) {
                    problems.push(format!("{name} {pname} \"{a}\": the times after @ must be like 0.5s or 500ms"));
                } else if ts.len() != stops.len() {
                    problems.push(format!(
                        "{name} {pname} \"{a}\" has {} values but {} times; give one time per value, like 0px..12px@0s..1s",
                        stops.len(),
                        ts.len()
                    ));
                } else if ts.windows(2).any(|w| w[1] < w[0]) {
                    problems.push(format!("{name} {pname} \"{a}\": the times after @ must go up"));
                }
            }
            expanded.extend(stops.into_iter().map(|v| (v, p)));
        }
        for (arg, p) in &expanded {
            let arg = arg.as_str();
            if arg.is_empty() {
                continue;
            }
            let pname = p["name"].as_str().unwrap_or("value");
            let default = match &p["default"] {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                v => v.to_string(),
            };
            let (lo, hi) = (p["min"].as_f64(), p["max"].as_f64());
            let range = |v: f64, unit: &str| -> Option<String> {
                let out = lo.is_some_and(|lo| v < lo) || hi.is_some_and(|hi| v > hi);
                out.then(|| {
                    format!(
                        "{name} {pname} {v}{unit} is out of range ({}{unit} to {}{unit})",
                        lo.map(fmt_num).unwrap_or_default(),
                        hi.map(fmt_num).unwrap_or_default()
                    )
                })
            };
            let problem = match p["type"].as_str().unwrap_or("") {
                "length" => match length_px(arg) {
                    Some(v) => range(v, "px"),
                    None => Some(format!("{name} {pname} \"{arg}\" must be a length in px, like {default}")),
                },
                "time" => match seconds(arg) {
                    Some(v) => range(v, "s"),
                    None => Some(format!("{name} {pname} \"{arg}\" must be a time, like {default}")),
                },
                "times" => {
                    let ts: Vec<Option<f64>> = arg.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).map(seconds).collect();
                    if ts.iter().any(Option::is_none) {
                        Some(format!("{name} {pname} \"{arg}\" must be times separated by spaces, like 0.15s 0.55s 1.05s"))
                    } else {
                        ts.into_iter().flatten().find_map(|v| range(v, "s"))
                    }
                }
                "number" => match arg.parse::<f64>() {
                    Ok(v) if v.is_finite() => range(v, ""),
                    _ => Some(format!("{name} {pname} \"{arg}\" must be a number, like {default}")),
                },
                "color" => crate::value::parse_color(arg)
                    .is_none()
                    .then(|| format!("{name} {pname} \"{arg}\" is not a colour; use #rrggbb, rgb(…) or a colour name")),
                "hexcolor" => (!is_hex_or_rgb(arg))
                    .then(|| format!("{name} {pname} \"{arg}\" must be #rgb, #rrggbb or rgb(…), like {default}")),
                "enum" => {
                    let values: Vec<&str> = p["values"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                    (!values.contains(&arg.to_ascii_lowercase().as_str()))
                        .then(|| format!("{name} {pname} \"{arg}\" must be one of: {}", values.join(", ")))
                }
                _ => None,
            };
            problems.extend(problem);
        }
    }
    problems
}

/// Lint errors for every `data-fx` attribute in these HTML sources, and for literal values scripts set.
pub fn lint_sources<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();
    for text in texts {
        let scripted = scripted_fx(text).0;
        for (spec, how) in data_fx_values(text).into_iter().map(|s| (s, "")).chain(scripted.into_iter().map(|s| (s, " (set from a script)"))) {
            for p in check(&spec) {
                let e = format!("data-fx=\"{spec}\"{how}: {p}");
                if !errors.contains(&e) {
                    errors.push(e);
                }
            }
        }
    }
    errors
}

/// Lint warnings for these HTML sources: `data-fx` set from a script.
pub fn lint_warnings<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let scripted = texts.into_iter().any(|t| scripted_fx(t).1);
    scripted
        .then(|| "data-fx is set from a script: it renders, but write it in the markup (data-fx=\"…\" on the element) so lint can check it".to_string())
        .into_iter()
        .collect()
}

/// The `data-fx` values scripts set with a string literal (`el.setAttribute("data-fx", "glow(12px)")`,
/// `el.dataset.fx = "…"`), and whether any script sets `data-fx` at all.
fn scripted_fx(html: &str) -> (Vec<String>, bool) {
    let mut values = Vec::new();
    let mut any = false;
    for (needle, sep) in [("setAttribute(\"data-fx\"", ','), ("setAttribute('data-fx'", ','), ("dataset.fx", '='), ("dataset[\"fx\"]", '='), ("dataset['fx']", '=')] {
        let mut rest = html;
        while let Some(at) = rest.find(needle) {
            rest = &rest[at + needle.len()..];
            let t = rest.trim_start();
            let Some(t) = t.strip_prefix(sep) else { continue };
            if sep == '=' && t.starts_with('=') {
                continue; // a comparison, not an assignment
            }
            any = true;
            let t = t.trim_start();
            let Some(q) = t.chars().next().filter(|c| matches!(c, '"' | '\'' | '`')) else { continue };
            if let Some(end) = t[1..].find(q) {
                let v = &t[1..1 + end];
                if !v.contains("${") {
                    values.push(v.to_string());
                }
            }
        }
    }
    (values, any)
}

/// The values of `data-fx="…"` attributes in an HTML source (not `data-fx-filter`).
fn data_fx_values(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("data-fx") {
        let after = &rest[at + "data-fx".len()..];
        rest = after;
        let t = after.trim_start();
        let Some(t) = t.strip_prefix('=') else { continue };
        let t = t.trim_start();
        let Some(q) = t.chars().next().filter(|c| *c == '"' || *c == '\'') else { continue };
        if let Some(end) = t[1..].find(q) {
            out.push(t[1..1 + end].to_string());
        }
    }
    out
}

/// A time in seconds: "0.2s", "200ms" or a bare number of seconds (as fx.js reads it).
fn seconds(s: &str) -> Option<f64> {
    let t = s.trim();
    let (n, k) = match t.strip_suffix("ms") {
        Some(n) => (n, 0.001),
        None => (t.strip_suffix('s').unwrap_or(t), 1.0),
    };
    n.trim().parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| v * k)
}

fn length_px(s: &str) -> Option<f64> {
    let n = s.trim().strip_suffix("px").unwrap_or(s.trim());
    n.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

fn is_hex_or_rgb(s: &str) -> bool {
    let s = s.trim();
    if let Some(h) = s.strip_prefix('#') {
        return (h.len() == 3 || h.len() == 6) && h.chars().all(|c| c.is_ascii_hexdigit());
    }
    let l = s.to_ascii_lowercase();
    (l.starts_with("rgb(") || l.starts_with("rgba(")) && l.ends_with(')')
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + usize::from(ca != cb)).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_like_fx_js() {
        assert_eq!(
            parse("glow(14px, rgb(0, 128, 255)) grain motion-blur(24px, vertical)"),
            vec![
                ("glow".into(), vec!["14px".into(), "rgb(0, 128, 255)".into()]),
                ("grain".into(), vec![]),
                ("motion-blur".into(), vec!["24px".into(), "vertical".into()]),
            ]
        );
    }

    #[test]
    fn catalog_and_runtime_list_the_same_effects() {
        let ids: Vec<&str> = effects().iter().filter_map(|e| e["id"].as_str()).collect();
        assert!(!ids.is_empty());
        for id in &ids {
            // every catalog effect has a preset in fx.js
            assert!(FX_JS.contains(&format!("\n    {id}: function")) || FX_JS.contains(&format!("\n    \"{id}\": function")), "fx.js has no preset for {id}");
        }
    }

    #[test]
    fn lint_explains_mistakes() {
        assert!(check("glow(14px, #7cf)").is_empty());
        assert!(check("glow(14, red, 0.5) duotone(#000, rgb(255,200,0))").is_empty());
        assert!(check("glw(12px)")[0].contains("did you mean \"glow\""));
        assert!(check("glow(big)")[0].contains("must be a length in px"));
        assert!(check("grain(3)")[0].contains("out of range"));
        assert!(check("motion-blur(20px, diagonal)")[0].contains("horizontal, vertical"));
        assert!(check("duotone(navy, white)")[0].contains("#rgb"));
        assert!(check("blur(0px..12px) grain(0.1..0.4)").is_empty());
        assert!(check("blur(0px..huge)")[0].contains("must be a length in px"));
        // keyframes and times
        assert!(check("blur(0px..18px@0.33s..1.13s, linear) scale(1..1.5..1.5..1@0.4s..0.9s..2s..2.6s)").is_empty());
        assert!(check("flash(#fff, 200ms) invert(1, 0.08s, 1s, 0.4s) zoom(1.9, 2.4s, 0.5s)").is_empty());
        assert!(check("scale(1..1.5@0s)")[0].contains("2 values but 1 times"));
        assert!(check("scale(1..2@1s..0s)")[0].contains("must go up"));
        assert!(check("flash(#fff, fast)")[0].contains("must be a time"));
        assert!(check("flash(#fff, 0.12s, 1, triangle, 0.15s 0.55s 1.05s)").is_empty());
        assert!(check("flash(#fff, 0.12s, 1, triangle, 0.15s soon)")[0].contains("times separated by spaces"));
        assert_eq!(lint_sources([r#"<div data-fx="goo(8px)" data-fx-filter="blur(2px)"></div><p data-fx='glo'>"#]).len(), 1);
    }

    #[test]
    fn lint_reads_scripted_fx() {
        let html = r#"<script>a.setAttribute("data-fx", "motion-blurr(20px)"); b.dataset.fx = 'glow(12px)'; if (c.dataset.fx == "x") {}</script>"#;
        let errors = lint_sources([html]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("(set from a script)") && errors[0].contains("motion-blur"));
        assert_eq!(lint_warnings([html]).len(), 1);
        assert!(lint_warnings([r#"<div data-fx="glow(12px)"></div><script>if (el.dataset.fx === "a") {}</script>"#]).is_empty());
    }
}
