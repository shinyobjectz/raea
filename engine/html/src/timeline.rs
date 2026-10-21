//! The React engine's data core (docs/react-engine.feature): an edit is rows of component instances,
//! and this module turns those rows into one snapshot of resolved props per frame. React (sdk/) renders
//! the snapshot; it never sees curves, springs or bindings.
//!
//! Time is integer frames at a rational fps. Every animated prop compiles to a track: a column of key
//! frames and a column of values, evaluated by binary search plus an ease. A frame's snapshot is a pure
//! function of the edit and the frame, so seek order never matters.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Map, Value, json};

// === The edit ===

#[derive(Clone, Debug, PartialEq)]
pub enum Ease {
    Linear,
    Step,
    In,
    Out,
    InOut,
    Bezier(f64, f64, f64, f64),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Prop {
    Const(Value),
    /// Keys sorted by frame (relative to the instance's start)
    Curve { keys: Vec<(i64, Value)>, ease: Ease },
    /// A damped spring from `from` to `to`, released at frame `at` (relative)
    Spring { from: f64, to: f64, at: i64, stiffness: f64, damping: f64, mass: f64 },
    /// A data column, looked up as of the frame (latest sample at or before it)
    Bind(String),
}

#[derive(Clone, Debug)]
pub struct Instance {
    pub id: String,
    pub kind: String,
    pub parent: Option<String>,
    pub order: i64,
    /// Span in frames, relative to the parent's start (absolute for roots); `to` is exclusive
    pub from: i64,
    pub to: i64,
    pub props: BTreeMap<String, Prop>,
}

#[derive(Clone, Debug)]
pub struct Edit {
    pub fps: (u32, u32),
    pub width: u32,
    pub height: u32,
    pub frames: i64,
    pub instances: Vec<Instance>,
    /// Data columns for bindings: name -> samples (absolute frame, value), sorted by frame
    pub data: HashMap<String, Vec<(i64, Value)>>,
    pub expects: Vec<Value>,
}

fn parse_fps(v: &Value) -> Result<(u32, u32), String> {
    let s = match v {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Null => "30".into(),
        _ => return Err("fps must be a number or \"N/D\"".into()),
    };
    let (n, d) = s.split_once('/').unwrap_or((&s, "1"));
    let n: u32 = n.trim().parse().map_err(|_| format!("bad fps {s:?}"))?;
    let d: u32 = d.trim().parse().map_err(|_| format!("bad fps {s:?}"))?;
    if n == 0 || d == 0 {
        return Err(format!("bad fps {s:?}"));
    }
    Ok((n, d))
}

fn parse_ease(v: Option<&Value>) -> Result<Ease, String> {
    let Some(v) = v else { return Ok(Ease::Linear) };
    if let Some(a) = v.as_array() {
        let n: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
        if n.len() == 4 {
            return Ok(Ease::Bezier(n[0], n[1], n[2], n[3]));
        }
        return Err("ease as an array needs 4 numbers (a cubic-bezier)".into());
    }
    match v.as_str().unwrap_or("") {
        "linear" => Ok(Ease::Linear),
        "step" => Ok(Ease::Step),
        "in" => Ok(Ease::In),
        "out" => Ok(Ease::Out),
        "inOut" | "in-out" => Ok(Ease::InOut),
        other => Err(format!("unknown ease {other:?} (linear, step, in, out, inOut, or [x1,y1,x2,y2])")),
    }
}

pub(crate) fn parse_prop(v: &Value) -> Result<Prop, String> {
    let Some(o) = v.as_object() else { return Ok(Prop::Const(v.clone())) };
    if let Some(keys) = o.get("keys") {
        let mut out = Vec::new();
        for k in keys.as_array().ok_or("keys must be an array of [frame, value]")? {
            let pair = k.as_array().filter(|p| p.len() == 2).ok_or("each key is [frame, value]")?;
            let f = pair[0].as_i64().ok_or("a key's frame must be an integer")?;
            out.push((f, pair[1].clone()));
        }
        if out.is_empty() {
            return Err("a curve needs at least one key".into());
        }
        out.sort_by_key(|(f, _)| *f);
        return Ok(Prop::Curve { keys: out, ease: parse_ease(o.get("ease"))? });
    }
    if let Some(s) = o.get("spring") {
        let num = |k: &str, d: f64| s.get(k).and_then(Value::as_f64).unwrap_or(d);
        return Ok(Prop::Spring {
            from: s.get("from").and_then(Value::as_f64).ok_or("spring.from must be a number")?,
            to: s.get("to").and_then(Value::as_f64).ok_or("spring.to must be a number")?,
            at: s.get("at").and_then(Value::as_i64).unwrap_or(0),
            stiffness: num("stiffness", 100.0),
            damping: num("damping", 10.0),
            mass: num("mass", 1.0),
        });
    }
    if let Some(b) = o.get("bind") {
        return Ok(Prop::Bind(b.as_str().ok_or("bind must be a column name")?.to_string()));
    }
    Ok(Prop::Const(v.clone()))
}

impl Edit {
    pub fn from_json(text: &str) -> Result<Edit, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("edit.json: {e}"))?;
        Edit::from_value(&v)
    }

    pub fn from_value(v: &Value) -> Result<Edit, String> {
        let fps = parse_fps(&v["fps"])?;
        let dim = |k: &str, d: u64| v.get(k).and_then(Value::as_u64).unwrap_or(d) as u32;
        let (width, height) = (dim("width", 1080), dim("height", 1920));
        let frames = v["frames"].as_i64().or_else(|| v["duration"].as_f64().map(|s| (s * fps.0 as f64 / fps.1 as f64).round() as i64));
        let frames = frames.ok_or("an edit needs frames (or duration in seconds)")?;
        let mut instances = Vec::new();
        for (i, row) in v["instances"].as_array().map(|a| a.as_slice()).unwrap_or(&[]).iter().enumerate() {
            let id = row["id"].as_str().ok_or(format!("instance {i}: id is required"))?.to_string();
            let kind = row["kind"].as_str().ok_or(format!("{id}: kind is required"))?.to_string();
            let from = row["from"].as_i64().unwrap_or(0);
            let to = row["to"].as_i64().unwrap_or(frames);
            if to <= from {
                return Err(format!("{id}: to ({to}) must be after from ({from})"));
            }
            let mut props = BTreeMap::new();
            if let Some(p) = row["props"].as_object() {
                for (k, pv) in p {
                    props.insert(k.clone(), parse_prop(pv).map_err(|e| format!("{id}.{k}: {e}"))?);
                }
            }
            instances.push(Instance {
                id,
                kind,
                parent: row["parent"].as_str().map(str::to_string),
                order: row["order"].as_i64().unwrap_or(i as i64),
                from,
                to,
                props,
            });
        }
        let mut data = HashMap::new();
        if let Some(d) = v["data"].as_object() {
            for (name, samples) in d {
                let mut col = Vec::new();
                for s in samples.as_array().ok_or(format!("data.{name} must be an array of [frame, value]"))? {
                    let pair = s.as_array().filter(|p| p.len() == 2).ok_or(format!("data.{name}: each sample is [frame, value]"))?;
                    col.push((pair[0].as_i64().ok_or(format!("data.{name}: frame must be an integer"))?, pair[1].clone()));
                }
                col.sort_by_key(|(f, _)| *f);
                data.insert(name.clone(), col);
            }
        }
        let edit = Edit { fps, width, height, frames, instances, data, expects: v["expects"].as_array().cloned().unwrap_or_default() };
        edit.validate()?;
        Ok(edit)
    }

    fn validate(&self) -> Result<(), String> {
        let ids: HashMap<&str, &Instance> = self.instances.iter().map(|i| (i.id.as_str(), i)).collect();
        if ids.len() != self.instances.len() {
            return Err("instance ids must be unique".into());
        }
        for inst in &self.instances {
            let mut seen = vec![inst.id.as_str()];
            let mut cur = inst.parent.as_deref();
            while let Some(p) = cur {
                let parent = ids.get(p).ok_or(format!("{}: unknown parent {p:?}", inst.id))?;
                if seen.contains(&p) {
                    return Err(format!("{}: parent cycle through {p:?}", inst.id));
                }
                seen.push(p);
                cur = parent.parent.as_deref();
            }
            for (name, prop) in &inst.props {
                if let Prop::Bind(col) = prop {
                    if !self.data.contains_key(col) {
                        return Err(format!("{}.{name}: no data column {col:?}", inst.id));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn fps_f64(&self) -> f64 {
        self.fps.0 as f64 / self.fps.1 as f64
    }
}

// === Evaluation ===

fn bezier(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> f64 {
    // solve x(t) = x by Newton, then bisection; return y(t)
    let bx = |t: f64| 3.0 * (1.0 - t) * (1.0 - t) * t * x1 + 3.0 * (1.0 - t) * t * t * x2 + t * t * t;
    let by = |t: f64| 3.0 * (1.0 - t) * (1.0 - t) * t * y1 + 3.0 * (1.0 - t) * t * t * y2 + t * t * t;
    let (mut lo, mut hi, mut t) = (0.0f64, 1.0f64, x);
    for _ in 0..60 {
        let v = bx(t) - x;
        if v.abs() < 1e-9 {
            break;
        }
        if v > 0.0 { hi = t } else { lo = t }
        t = (lo + hi) / 2.0;
    }
    by(t)
}

fn ease(e: &Ease, p: f64) -> f64 {
    match e {
        Ease::Linear => p,
        Ease::Step => 0.0,
        Ease::In => p * p * p,
        Ease::Out => 1.0 - (1.0 - p).powi(3),
        Ease::InOut => if p < 0.5 { 4.0 * p * p * p } else { 1.0 - (-2.0 * p + 2.0).powi(3) / 2.0 },
        Ease::Bezier(a, b, c, d) => bezier(*a, *b, *c, *d, p),
    }
}

fn parse_hex(s: &str) -> Option<[f64; 4]> {
    let h = s.strip_prefix('#')?;
    let c = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok().map(|v| v as f64);
    match h.len() {
        6 => Some([c(0)?, c(2)?, c(4)?, 255.0]),
        8 => Some([c(0)?, c(2)?, c(4)?, c(6)?]),
        _ => None,
    }
}

fn hex(c: [f64; 4]) -> String {
    let b = |v: f64| v.round().clamp(0.0, 255.0) as u8;
    if b(c[3]) == 255 {
        format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]), b(c[3]))
    }
}

/// Round to 1/1000 so the Rust and JS evaluators print identical numbers.
pub(crate) fn num(v: f64) -> Value {
    json!((v * 1000.0).round() / 1000.0)
}

/// Interpolate between two key values: numbers, number arrays and #hex colours blend; anything else holds `a`.
fn lerp(a: &Value, b: &Value, p: f64) -> Value {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => num(x.as_f64().unwrap() + (y.as_f64().unwrap() - x.as_f64().unwrap()) * p),
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() && x.iter().chain(y).all(Value::is_number) => {
            Value::Array(x.iter().zip(y).map(|(u, v)| lerp(u, v, p)).collect())
        }
        (Value::String(x), Value::String(y)) => match (parse_hex(x), parse_hex(y)) {
            (Some(c), Some(d)) => Value::String(hex([0, 1, 2, 3].map(|i| c[i] + (d[i] - c[i]) * p))),
            _ => a.clone(),
        },
        _ => a.clone(),
    }
}

pub(crate) fn eval_curve(keys: &[(i64, Value)], e: &Ease, f: i64) -> Value {
    // the first key at or after f
    let i = keys.partition_point(|(k, _)| *k < f);
    if i == 0 {
        return keys[0].1.clone();
    }
    if i == keys.len() {
        return keys[keys.len() - 1].1.clone();
    }
    let ((f0, v0), (f1, v1)) = (&keys[i - 1], &keys[i]);
    if *f1 == f {
        return v1.clone();
    }
    let p = (f - f0) as f64 / (f1 - f0) as f64;
    lerp(v0, v1, ease(e, p))
}

/// A damped harmonic oscillator released at rest from `from` toward `to`, in closed form.
pub(crate) fn eval_spring(from: f64, to: f64, at: i64, k: f64, c: f64, m: f64, f: i64, fps: f64) -> Value {
    if f <= at {
        return num(from);
    }
    let t = (f - at) as f64 / fps;
    let (x0, w0) = (from - to, (k / m).sqrt());
    let zeta = c / (2.0 * (k * m).sqrt());
    let x = if zeta < 1.0 {
        let wd = w0 * (1.0 - zeta * zeta).sqrt();
        (-zeta * w0 * t).exp() * (x0 * (wd * t).cos() + (zeta * w0 * x0 / wd) * (wd * t).sin())
    } else if (zeta - 1.0).abs() < 1e-9 {
        (-w0 * t).exp() * (x0 + w0 * x0 * t)
    } else {
        let s = w0 * (zeta * zeta - 1.0).sqrt();
        let (r1, r2) = (-zeta * w0 + s, -zeta * w0 - s);
        let b = (r1 * x0) / (r1 - r2);
        let a = x0 - b;
        a * (r1 * t).exp() + b * (r2 * t).exp()
    };
    num(to + x)
}

/// As of frame `f`: the latest sample at or before it (or the first, before any sample).
pub(crate) fn as_of(col: &[(i64, Value)], f: i64) -> Value {
    let i = col.partition_point(|(k, _)| *k <= f);
    if i == 0 { col.first().map(|s| s.1.clone()).unwrap_or(Value::Null) } else { col[i - 1].1.clone() }
}

/// The edit compiled for evaluation: instances in tree order with absolute spans.
pub struct Timeline {
    edit: Edit,
    /// (index into edit.instances, absolute from, absolute to, depth), parents before children, siblings by order
    order: Vec<(usize, i64, i64, usize)>,
}

impl Timeline {
    pub fn new(edit: Edit) -> Timeline {
        let mut children: HashMap<Option<&str>, Vec<usize>> = HashMap::new();
        for (i, inst) in edit.instances.iter().enumerate() {
            children.entry(inst.parent.as_deref()).or_default().push(i);
        }
        for list in children.values_mut() {
            list.sort_by_key(|&i| (edit.instances[i].order, i));
        }
        let mut order = Vec::new();
        // depth-first from the roots; a child's span is clipped to its parent's
        let mut stack: Vec<(usize, i64, i64, usize)> = children
            .get(&None)
            .map(|r| r.iter().rev().map(|&i| (i, edit.instances[i].from, edit.instances[i].to, 0)).collect())
            .unwrap_or_default();
        while let Some((i, from, to, depth)) = stack.pop() {
            order.push((i, from, to, depth));
            if let Some(kids) = children.get(&Some(edit.instances[i].id.as_str())) {
                for &k in kids.iter().rev() {
                    let c = &edit.instances[k];
                    stack.push((k, (from + c.from).max(from), (from + c.to).min(to), depth + 1));
                }
            }
        }
        Timeline { edit, order }
    }

    pub fn edit(&self) -> &Edit {
        &self.edit
    }

    /// (index into edit.instances, absolute from, absolute to, depth): parents before children, siblings by order.
    pub fn order(&self) -> &[(usize, i64, i64, usize)] {
        &self.order
    }

    /// One prop at `rel` frames into its instance (`f` is absolute, for bindings).
    pub fn eval(&self, p: &Prop, rel: i64, f: i64) -> Value {
        match p {
            Prop::Const(v) => v.clone(),
            Prop::Curve { keys, ease } => eval_curve(keys, ease, rel),
            Prop::Spring { from, to, at, stiffness, damping, mass } => eval_spring(*from, *to, *at, *stiffness, *damping, *mass, rel, self.edit.fps_f64()),
            Prop::Bind(col) => as_of(&self.edit.data[col], f),
        }
    }

    /// The snapshot at frame `f`: visible instances in tree order with resolved props.
    pub fn frame(&self, f: i64) -> Value {
        let fps = self.edit.fps_f64();
        let mut nodes = Vec::new();
        for &(i, from, to, depth) in &self.order {
            if f < from || f >= to {
                continue;
            }
            let inst = &self.edit.instances[i];
            let rel = f - from;
            let mut props = Map::new();
            for (name, p) in &inst.props {
                let v = match p {
                    Prop::Const(v) => v.clone(),
                    Prop::Curve { keys, ease } => eval_curve(keys, ease, rel),
                    Prop::Spring { from, to, at, stiffness, damping, mass } => eval_spring(*from, *to, *at, *stiffness, *damping, *mass, rel, fps),
                    Prop::Bind(col) => as_of(&self.edit.data[col], f),
                };
                props.insert(name.clone(), v);
            }
            let span = (to - from).max(1);
            nodes.push(json!({
                "id": inst.id,
                "kind": inst.kind,
                "parent": inst.parent,
                "depth": depth,
                "frame": rel,
                "progress": num(rel as f64 / (span - 1).max(1) as f64),
                "props": props,
            }));
        }
        json!({ "frame": f, "nodes": nodes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(rows: &str) -> Timeline {
        Timeline::new(Edit::from_json(&format!(r#"{{"fps":30,"frames":90,"instances":{rows},
            "data":{{"face.box":[[0,[10,10,50,50]],[30,[20,10,50,50]]]}}}}"#)).unwrap())
    }

    #[test]
    fn spans_nest_and_clip() {
        let t = edit(r#"[{"id":"a","kind":"Box","from":10,"to":40},{"id":"b","kind":"Text","parent":"a","from":5,"to":100}]"#);
        assert_eq!(t.frame(9)["nodes"].as_array().unwrap().len(), 0);
        let n = t.frame(15)["nodes"].clone();
        assert_eq!(n.as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(n[1]["frame"], 0, "a child's frame is relative to its own start");
        assert_eq!(t.frame(40)["nodes"].as_array().unwrap().len(), 0, "a child never outlives its parent");
    }

    #[test]
    fn curves_ease_and_blend_values() {
        let t = edit(r##"[{"id":"a","kind":"Box","props":{
            "x":{"keys":[[0,0],[10,100]]},
            "o":{"keys":[[0,0],[10,1]],"ease":"inOut"},
            "c":{"keys":[[0,"#000000"],[10,"#ffffff"]]},
            "box":{"keys":[[0,[0,0]],[10,[10,20]]]},
            "label":{"keys":[[0,"one"],[5,"two"]]},
            "b":{"keys":[[0,0],[10,1]],"ease":[0.25,0.1,0.25,1]}}}]"##);
        let p = |f: i64| t.frame(f)["nodes"][0]["props"].clone();
        assert_eq!(p(5)["x"], 50.0);
        assert_eq!(p(20)["x"], 100.0, "after the last key the value holds");
        assert_eq!(p(5)["o"], 0.5);
        assert_eq!(p(5)["c"], "#808080");
        assert_eq!(p(5)["box"], json!([5.0, 10.0]));
        assert_eq!(p(4)["label"], "one");
        assert_eq!(p(5)["label"], "two");
        let b = p(5)["b"].as_f64().unwrap();
        assert!(b > 0.75 && b < 0.85, "CSS ease at 0.5 is about 0.80, got {b}");
    }

    #[test]
    fn springs_settle_and_bindings_look_back() {
        let t = edit(r#"[{"id":"a","kind":"Box","props":{"s":{"spring":{"from":0,"to":1,"at":0,"stiffness":100,"damping":10}},
            "face":{"bind":"face.box"}}}]"#);
        let p = |f: i64| t.frame(f)["nodes"][0]["props"].clone();
        assert_eq!(p(0)["s"], 0.0);
        let peak = (1..30).map(|f| p(f)["s"].as_f64().unwrap()).fold(0.0, f64::max);
        assert!(peak > 1.0, "an underdamped spring overshoots");
        assert!((p(89)["s"].as_f64().unwrap() - 1.0).abs() < 0.01, "and settles");
        assert_eq!(p(29)["face"], json!([10, 10, 50, 50]));
        assert_eq!(p(30)["face"], json!([20, 10, 50, 50]));
    }

    #[test]
    fn frames_are_pure() {
        let t = edit(r#"[{"id":"a","kind":"Box","props":{"x":{"keys":[[0,0],[60,600]],"ease":"out"}}}]"#);
        let forward: Vec<Value> = (0..90).map(|f| t.frame(f)).collect();
        let backward: Vec<Value> = (0..90).rev().map(|f| t.frame(f)).collect();
        assert_eq!(forward, backward.into_iter().rev().collect::<Vec<_>>());
    }

    #[test]
    fn bad_edits_are_refused_with_reasons() {
        let bad = |rows: &str| Edit::from_json(&format!(r#"{{"fps":30,"frames":30,"instances":{rows}}}"#)).unwrap_err();
        assert!(bad(r#"[{"id":"a","kind":"Box","parent":"zz"}]"#).contains("unknown parent"));
        assert!(bad(r#"[{"id":"a","kind":"Box","parent":"b"},{"id":"b","kind":"Box","parent":"a"}]"#).contains("cycle"));
        assert!(bad(r#"[{"id":"a","kind":"Box","from":10,"to":5}]"#).contains("must be after"));
        assert!(bad(r#"[{"id":"a","kind":"Box","props":{"x":{"bind":"nope"}}}]"#).contains("no data column"));
        assert!(bad(r#"[{"id":"a","kind":"Box","props":{"x":{"keys":[[0,1]],"ease":"wobble"}}}]"#).contains("unknown ease"));
        assert!(bad(r#"[{"id":"a","kind":"Box"},{"id":"a","kind":"Box"}]"#).contains("unique"));
    }
}
