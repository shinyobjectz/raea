//! Constants in the engine's shaders set from TypeScript (docs/nobuild.feature NB1a): <ShaderConstants> and the typed
//! props built on it (<SolariLighting constants>) put a ShaderConstants component in the scene, and when an Engine opens
//! each named `const NAME = value;` in that shader's text is rewritten before anything compiles, its type kept (a
//! float, a u32, an i32, a bool). The shader's own value is the default. They are compiled in once, at load: a value
//! changed later in the composition draws nothing new, and raea says so once.
use bevy::prelude::*;
use raea_core_program::ir::{Program, V};
use serde_json::Value;

/// One constant: its shader as Bevy embeds it (`<crate>/<path under its src>`), the const's name, its value (a bool as
/// 1 or 0), or for an array or a vector its values in order (`list`, `value` then unused).
#[derive(Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Default)]
pub struct ShaderConstant {
    pub shader: String,
    pub name: String,
    pub value: f64,
    pub list: Vec<f64>,
}

/// The constants a part sets in the engine's shaders, read from the first frame's rows when the Engine opens.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
pub struct ShaderConstants {
    pub values: Vec<ShaderConstant>,
}

const TYPE: &str = "raea_sys_engine_shaders::consts::ShaderConstants";

/// Every ShaderConstants value the program's first frame has, the last one for a shader and name. A part's components
/// are `component` rows (runtime/look `components`: its `type`, its `value` as JSON, and `from` when a later frame
/// brings it, which is left for `changed_later`); a column that holds components inline as JSON is read too.
pub(super) fn gather(prog: &Program) -> Vec<ShaderConstant> {
    let mut out: Vec<ShaderConstant> = Vec::new();
    for (_, rows) in &prog.spawns {
        for r in rows {
            if matches!(r.get("type"), Some(V::S(t)) if t == TYPE) {
                let first = r.get("from").is_none_or(|f| f.num() <= 0.0);
                if let (true, Some(V::S(v))) = (first, r.get("value")) {
                    if let Ok(j) = serde_json::from_str::<Value>(v) {
                        take(&j, &mut out);
                    }
                }
                continue;
            }
            for v in r.values() {
                let V::S(s) = v else { continue };
                if s.contains(TYPE) {
                    if let Ok(j) = serde_json::from_str::<Value>(s) {
                        walk(&j, &mut out);
                    }
                }
            }
        }
    }
    out
}

fn walk(j: &Value, out: &mut Vec<ShaderConstant>) {
    match j {
        Value::Object(o) if o.get("type").and_then(Value::as_str) == Some(TYPE) => take(o.get("value").unwrap_or(&Value::Null), out),
        Value::Object(o) => o.values().for_each(|v| walk(v, out)),
        Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
        _ => {}
    }
}

/// A ShaderConstants value's list, each replacing an earlier one of its shader and name.
fn take(value: &Value, out: &mut Vec<ShaderConstant>) {
    for c in value.get("values").and_then(Value::as_array).into_iter().flatten() {
        let (Some(shader), Some(name)) = (c.get("shader").and_then(Value::as_str), c.get("name").and_then(Value::as_str)) else { continue };
        let value = c.get("value").and_then(|v| v.as_f64().or_else(|| v.as_bool().map(f64::from))).unwrap_or(f64::NAN);
        let list = c.get("list").and_then(Value::as_array).map(|a| a.iter().map(|v| v.as_f64().unwrap_or(f64::NAN)).collect()).unwrap_or_default();
        out.retain(|k| !(k.shader == shader && k.name == name));
        out.push(ShaderConstant { shader: shader.into(), name: name.into(), value, list });
    }
}

/// `text` with `const NAME ... = EXPR;` given `value` in the const's type, or `list` for an array or a vector (as many
/// values as it holds, each in its element type): the new text and the expression it had.
pub(super) fn rewrite(text: &str, name: &str, value: f64, list: &[f64]) -> Result<(String, String), String> {
    let mut found = None;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("const ").map(str::trim_start).and_then(|r| r.strip_prefix(name)) {
            if rest.starts_with([' ', '\t', ':', '=']) {
                if found.is_some() {
                    return Err(format!("{name} is declared more than once"));
                }
                found = Some((at + line.len() - rest.len(), rest));
            }
        }
        at += line.len();
    }
    let Some((start, rest)) = found else { return Err(format!("no `const {name}`")) };
    let eq = rest.find('=').ok_or_else(|| format!("{name} has no value"))?;
    let end = rest.find(';').filter(|e| *e > eq).ok_or_else(|| format!("{name}'s value doesn't end on its line"))?;
    let ty = rest[..eq].trim().trim_start_matches(':').trim();
    let old = rest[eq + 1..end].trim();
    let e0 = start + eq + 1;
    let put = |new: String| (format!("{} {new}{}", &text[..e0], &text[start + end..]), old.to_string());
    if !list.is_empty() {
        return several(name, old, list).map(put);
    }
    if !value.is_finite() {
        return Err(format!("{name} needs a number or a boolean"));
    }
    let int = |suffix: &str, min: f64| {
        if value.fract() != 0.0 || value < min {
            Err(format!("{name} is a whole number{}, not {value}", if min >= 0.0 { " from 0" } else { "" }))
        } else {
            Ok(format!("{value:.0}{suffix}"))
        }
    };
    let new = match ty {
        "bool" => (if value != 0.0 { "true" } else { "false" }).to_string(),
        "u32" => int("u", 0.0)?,
        "i32" => int("i", f64::MIN)?,
        "f32" | "f16" => float(value),
        "" if old == "true" || old == "false" => (if value != 0.0 { "true" } else { "false" }).to_string(),
        "" if old.strip_suffix('u').is_some_and(|n| n.parse::<u64>().is_ok()) => int("u", 0.0)?,
        "" if old.strip_suffix('i').is_some_and(|n| n.parse::<i64>().is_ok()) => int("i", f64::MIN)?,
        "" if old.parse::<i64>().is_ok() => int("", f64::MIN)?,
        // a float literal, or an expression of floats (PI, 1.0 / 16.0); not an array, a vector or a matrix
        "" if !old.contains('<') && !["array", "vec", "mat"].iter().any(|t| old.starts_with(t)) => float(value),
        _ => return Err(format!("{name} is {}, not a number{}", if ty.is_empty() { old } else { ty }, if old.contains('(') { " (give its values as a list)" } else { "" })),
    };
    Ok(put(new))
}

/// An array's or a vector's new expression, its constructor as the shader writes it (`array<f32, 6>(...)`,
/// `vec3<f32>(...)`, `vec3(...)`), each value in its element type.
fn several(name: &str, old: &str, list: &[f64]) -> Result<String, String> {
    let (Some(open), true) = (old.find('('), old.ends_with(')')) else { return Err(format!("{name} is {old}, one number (not a list)")) };
    let ctor = old[..open].trim();
    if !(ctor.starts_with("array") || ctor.starts_with("vec")) {
        return Err(format!("{name} is {old}, not an array or a vector"));
    }
    // its values: the commas at the top level inside the constructor's brackets
    let args = &old[open + 1..old.len() - 1];
    let (mut depth, mut n, mut floats) = (0, 1, args.contains('.'));
    for ch in args.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => n += 1,
            _ => {}
        }
    }
    // array<T, N> or vecN<T>: T
    let elem = ctor.split_once('<').map_or("", |(_, g)| {
        let inner = g.strip_suffix('>').unwrap_or(g);
        inner.rsplit_once(',').map_or(inner, |(e, _)| e).trim()
    });
    if ["vec", "array", "mat"].iter().any(|t| elem.starts_with(t)) {
        return Err(format!("{name} is an array of {elem}, not of numbers"));
    }
    if args.trim().is_empty() || n != list.len() {
        return Err(format!("{name} holds {n} values, not {}", list.len()));
    }
    floats |= matches!(elem, "f32" | "f16");
    let each = |v: f64| -> Result<String, String> {
        let whole = |suffix: &str, min: f64| if v.fract() != 0.0 || v < min || !v.is_finite() { Err(format!("{name}'s values are whole numbers{}, not {v}", if min >= 0.0 { " from 0" } else { "" })) } else { Ok(format!("{v:.0}{suffix}")) };
        match elem {
            "u32" => whole("u", 0.0),
            "i32" => whole("i", f64::MIN),
            "bool" => Ok((if v != 0.0 { "true" } else { "false" }).into()),
            _ if floats => if v.is_finite() { Ok(float(v)) } else { Err(format!("{name}'s values are numbers")) },
            _ => whole("", f64::MIN),
        }
    };
    Ok(format!("{ctor}({})", list.iter().map(|v| each(*v)).collect::<Result<Vec<_>, _>>()?.join(", ")))
}

/// A float literal WGSL reads as one: 24 as 24.0.
fn float(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 { format!("{v:.1}") } else { format!("{v}") }
}

/// The constants the Engine opened with, to say once that a later change isn't drawn.
#[derive(Resource)]
pub(super) struct Loaded(pub Vec<ShaderConstant>);

pub(super) fn changed_later(q: Query<&ShaderConstants, Changed<ShaderConstants>>, loaded: Res<Loaded>, mut said: Local<bool>) {
    if *said {
        return;
    }
    if let Some(c) = q.iter().flat_map(|c| &c.values).find(|c| !loaded.0.contains(c)) {
        *said = true;
        let v = if c.list.is_empty() { c.value.to_string() } else { format!("{:?}", c.list) };
        eprintln!("raea: the engine's shader constants are compiled in when the scene loads; {} {} = {v} came later and isn't drawn (set it from the first frame)", c.shader, c.name);
    }
}
