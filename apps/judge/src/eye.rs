//! Shane's eye, the judge's calibration. `raea judge eye DIR form=5 light=7 ... [--pass yes|no]` records his scores for a
//! round the judge has already judged (DIR/judge.json), with the judge's raw scores beside them, in
//! ~/.raea/judge/eye.jsonl. `raea judge calibrate` fits, per category, a line from the judge's score to his
//! (least squares), and says how often the mapped judge's pass agrees with his. The judge gates only once it has 10
//! rounds or more, some passed by the eye and some not, and agrees on 8 in 10 or better; until then `gating` is false and its verdict is advice
//! (clef has rated almost everything acceptable: docs/craft-system.md). `raea judge eye DIR --by kimi|claude` records a
//! fresh agent session's reading instead (Shane, 11-06), each row saying whose eye it is.

use std::path::PathBuf;

use serde_json::{Map, Value, json};

use crate::rubric::{pass_for, scores};

fn home() -> PathBuf {
    raea_io_home::env_path("RAEA_JUDGE_DIR", ".raea/judge")
}

/// A kind's calibration file: calibration.json for the default rubric, calibration.KIND.json for a `raea make` kind's
/// (docs/make.feature MK6), each fitted on that kind's rounds alone.
fn file_of(kind: Option<&str>) -> PathBuf {
    home().join(kind.map_or_else(|| "calibration.json".to_string(), |k| format!("calibration.{k}.json")))
}

/// The calibration in force for a kind, if one was made.
pub fn calibration(kind: Option<&str>) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(file_of(kind)).ok()?).ok()
}

/// A row's or a verdict's kind: none for the default rubric.
fn kind_in(v: &Value) -> Option<&str> {
    v["kind"].as_str()
}

/// The judge's scores mapped through the calibration's lines, kept to 0-10.
pub fn apply(c: &Value, scores: &mut Map<String, Value>) {
    for (k, v) in scores.iter_mut() {
        if let (Some(x), Some(a), Some(b)) = (v.as_f64(), c["maps"][k]["a"].as_f64(), c["maps"][k]["b"].as_f64()) {
            *v = json!(((a * x + b).clamp(0.0, 10.0) * 100.0).round() / 100.0);
        }
    }
}

pub fn record(args: &[String]) -> Result<String, String> {
    let dir = args.get(2).map(PathBuf::from).ok_or("raea judge eye DIR CATEGORY=SCORE... [--pass yes|no]")?;
    let verdict: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("judge.json")).map_err(|_| format!("judge eye: no {}: judge the round first (raea judge {})", dir.join("judge.json").display(), dir.display()))?).map_err(|e| e.to_string())?;
    let kind = kind_in(&verdict);
    let cats = scores(kind);
    // an agent's reading as the eye (`--by kimi`): a fresh session's scores, and its pass by the same rule in code
    if let Some(by) = args.iter().position(|a| a == "--by").and_then(|i| args.get(i + 1)) {
        let v = crate::agent::ask(kind, by, &dir, &json!({}), None)?;
        let (eye, defects, _) = crate::agent::read(kind, &v);
        if eye.len() < cats.len() {
            return Err(format!("judge eye: {by} scored only {} of {} categories", eye.len(), cats.len()));
        }
        let eye_pass = pass_for(kind, &eye, &defects).0;
        let row = json!({"dir": verdict["dir"], "kind": kind, "by": by, "judge": verdict["raw"], "judge_pass": verdict["pass"], "defects": verdict["defects"], "eye": eye, "eye_pass": eye_pass, "eye_defects": defects});
        append(&row)?;
        let shown: Vec<String> = eye.iter().map(|(k, v)| format!("{k} {v}")).collect();
        return Ok(format!("APPLIED  {by}'s eye on {} recorded ({}): {}; raea judge calibrate fits clef to it", dir.display(), if eye_pass { "a pass" } else { "not yet" }, shown.join(", ")));
    }
    let mut eye = Map::new();
    let mut said_pass = None;
    let mut i = 3;
    while i < args.len() {
        if args[i] == "--pass" {
            said_pass = Some(matches!(args.get(i + 1).map(String::as_str), Some("yes" | "true" | "y")));
            i += 2;
            continue;
        }
        let (k, v) = args[i].split_once('=').ok_or_else(|| format!("judge eye: {:?} isn't CATEGORY=SCORE", args[i]))?;
        if !cats.iter().any(|s| s.0 == k) {
            return Err(format!("judge eye: no category {k}: {}", cats.iter().map(|s| s.0).collect::<Vec<_>>().join(", ")));
        }
        let x: f64 = v.parse().ok().filter(|x| (0.0..=10.0).contains(x)).ok_or_else(|| format!("judge eye: {k} is a score 0-10, got {v:?}"))?;
        eye.insert(k.into(), json!(x));
        i += 1;
    }
    if eye.is_empty() {
        return Err("judge eye: give your scores, e.g. composition=6 form=5 assembly=7 materials=4 light=6 detail=5 world=6".into());
    }
    // his pass: as he says it, else the same rule on his scores with the judge's defects
    let eye_pass = said_pass.unwrap_or_else(|| pass_for(kind, &eye, verdict["defects"].as_array().map(Vec::as_slice).unwrap_or(&[])).0);
    let row = json!({"dir": verdict["dir"], "kind": kind, "by": "shane", "judge": verdict["raw"], "judge_pass": verdict["pass"], "defects": verdict["defects"], "eye": eye, "eye_pass": eye_pass});
    append(&row)?;
    Ok(format!("APPLIED  your eye on {} recorded ({}): raea judge calibrate fits the judge to it", dir.display(), if eye_pass { "a pass" } else { "not yet" }))
}

/// One eye row onto ~/.raea/judge/eye.jsonl; a newer reading of the same round by the same eye replaces the older.
fn append(row: &Value) -> Result<(), String> {
    std::fs::create_dir_all(home()).map_err(|e| e.to_string())?;
    let path = home().join("eye.jsonl");
    let mut rows: Vec<Value> = std::fs::read_to_string(&path).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).filter(|r: &Value| !(r["dir"] == row["dir"] && whose(r) == whose(&row))).collect();
    rows.push(row.clone());
    std::fs::write(&path, rows.iter().map(Value::to_string).collect::<Vec<_>>().join("\n") + "\n").map_err(|e| e.to_string())
}

/// Least squares y = a x + b; flat x (one value) maps by the mean offset.
fn fit(xy: &[(f64, f64)]) -> (f64, f64) {
    let n = xy.len() as f64;
    let (mx, my) = (xy.iter().map(|p| p.0).sum::<f64>() / n, xy.iter().map(|p| p.1).sum::<f64>() / n);
    let sxx: f64 = xy.iter().map(|p| (p.0 - mx).powi(2)).sum();
    if sxx < 1e-9 {
        return (1.0, my - mx);
    }
    let a = xy.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>() / sxx;
    (a, my - a * mx)
}

/// The fit over a kind's rows of {judge, eye, defects, eye_pass} (none: the default rubric's): per-category maps and
/// errors, and the pass agreement.
pub fn fit_rows_for(kind: Option<&str>, rows: &[Value]) -> Value {
    let mut maps = Map::new();
    for &(k, _, _) in scores(kind) {
        let xy: Vec<(f64, f64)> = rows.iter().filter_map(|r| Some((r["judge"][k].as_f64()?, r["eye"][k].as_f64()?))).collect();
        if xy.len() < 2 {
            continue;
        }
        let (a, b) = fit(&xy);
        let mae = |f: &dyn Fn(f64) -> f64| xy.iter().map(|(x, y)| (f(*x) - y).abs()).sum::<f64>() / xy.len() as f64;
        let r2 = |x: f64| (x * 1000.0).round() / 1000.0;
        maps.insert(k.into(), json!({"a": r2(a), "b": r2(b), "n": xy.len(), "maeRaw": r2(mae(&|x| x)), "maeMapped": r2(mae(&|x| (a * x + b).clamp(0.0, 10.0)))}));
    }
    let cal = json!({"maps": maps});
    let mut agree = 0;
    for r in rows {
        let mut s = r["judge"].as_object().cloned().unwrap_or_default();
        apply(&cal, &mut s);
        let p = pass_for(kind, &s, r["defects"].as_array().map(Vec::as_slice).unwrap_or(&[])).0;
        if Value::Bool(p) == r["eye_pass"] {
            agree += 1;
        }
    }
    let n = rows.len();
    let agreement = if n == 0 { 0.0 } else { agree as f64 / n as f64 };
    // agreement means something only when the eye passed some rounds and failed others: all fails agree trivially
    let passes = rows.iter().filter(|r| r["eye_pass"] == true).count();
    let both = passes > 0 && passes < n;
    json!({"maps": maps, "rounds": n, "eyePasses": passes, "agreement": (agreement * 1000.0).round() / 1000.0, "gate": n >= 10 && agreement >= 0.8 && both})
}

pub fn calibrate(args: &[String]) -> Result<String, String> {
    let kind = crate::kind_of(args)?;
    let kind = kind.as_deref();
    let rows: Vec<Value> = std::fs::read_to_string(home().join("eye.jsonl")).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).filter(|r: &Value| kind_in(r) == kind).collect();
    if rows.is_empty() {
        let k = kind.map(|k| format!(" of a {k}")).unwrap_or_default();
        return Err(format!("judge calibrate: no rounds{k} with your eye yet (raea judge eye DIR form=5 ...)"));
    }
    let mut cal = fit_rows_for(kind, &rows);
    cal["kind"] = json!(kind);
    let mut eyes: Vec<String> = rows.iter().map(whose).map(str::to_string).collect();
    eyes.sort();
    eyes.dedup();
    cal["eyes"] = json!(eyes);
    std::fs::write(file_of(kind), serde_json::to_string_pretty(&cal).unwrap_or_default()).map_err(|e| e.to_string())?;
    if args.iter().any(|a| a == "--json") {
        return Ok(serde_json::to_string_pretty(&cal).unwrap_or_default());
    }
    let mut t: Vec<String> = cal["maps"].as_object().into_iter().flatten().map(|(k, m)| format!("{k:<12} eye = {} x judge {:+}  (off by {} raw, {} mapped, {} rounds)", m["a"], m["b"], m["maeRaw"], m["maeMapped"], m["n"])).collect();
    t.push(format!("agreement on the pass: {} over {} rounds (eyes: {}); {}", cal["agreement"], cal["rounds"], eyes.join(", "), if cal["gate"] == true { "the judge gates now".to_string() } else if cal["rounds"].as_u64() >= Some(10) && cal["agreement"].as_f64() >= Some(0.8) { format!("advice only: the eye passed {} of {} rounds, and agreement means something only with passes and fails both (judge a scene that is good)", cal["eyePasses"], cal["rounds"]) } else { "advice only until 10 rounds agree 8 in 10, with passes and fails both".to_string() }));
    Ok(t.join("\n"))
}

/// Whose eye a row records: Shane's when it doesn't say.
fn whose(r: &Value) -> &str {
    r["by"].as_str().unwrap_or("shane")
}
