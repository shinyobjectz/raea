//! `raea solid info FILE.ts [--json] [--set k=v,...] [--expect E.json] [--walk]`: the file's solid measured
//! (raea-core-solidinfo). FILE.ts exports `default`: a Solid, an object of named Solids (the parts), or a function
//! of the params returning either; and optionally `params`, each a number, a boolean, or `[low, high, default]`. The
//! file is run by `raea run` (a wrapper that prints each part's shape expression), then measured natively. --set
//! builds it at other params and says what moved from the defaults; --walk moves each ranged param alone to each end
//! and says which break it (more pieces, empty) and which reach no geometry. FILE.expect.json beside it is read when
//! --expect isn't given. Errors exit 1, as scene lint. A part's `role` comes from the file's `roles` export ({body:
//! "contain"}), its `at` from the line that names it. `raea solid view` takes its pictures (view.rs); `raea solid export`
//! writes it as a .glb for a Blender step (export.rs). Its `needs` export ({textured: true}) asks for what solids can't
//! give yet: real UVs and baked maps name bake.py (docs/blender.feature BL14).

use std::path::{Path, PathBuf};

use raea_core_facts::{Finding, Level, finding};
use raea_runtime_bundle::flag;
use serde_json::{Map, Value, json};

mod export;
mod view;

pub const USAGE: &str = "raea solid info FILE.ts [--json] [--set k=v,...] [--expect E.json] [--walk]   (volume, box, pieces, genus per part; joints; predictions and mutants)\n       raea solid view FILE.ts -o DIR [--set k=v,...] [--json]   (the solid's parts in colour and clay from fixed cameras)\n       raea solid export FILE.ts -o OUT.glb [--set k=v,...] [--part NAME] [--colour #RRGGBB]   (the solid as one .glb for a Blender step, read back and checked)";

thread_local! {
    /// the last run file's `roles` and `needs` exports
    static META: std::cell::RefCell<Value> = const { std::cell::RefCell::new(Value::Null) };
}

/// The file run with these params: (the params it used, its spec, each part's expression).
fn made(file: &Path, set: &Map<String, Value>) -> Result<(Value, Value, Vec<(String, String)>), String> {
    let dir = std::env::temp_dir().join(format!("raea-solid-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let w = dir.join("info.ts");
    let src = format!(
        r#"import {{ print }} from "raea/native";
import * as m from {file:?};
const spec: Record<string, unknown> = (m as any).params ?? {{}};
const set: Record<string, unknown> = {set};
const p: Record<string, unknown> = {{}};
for (const [k, v] of Object.entries(spec)) p[k] = Array.isArray(v) ? v[2] : (v && typeof v === "object" && "default" in (v as any)) ? (v as any).default : v;
for (const [k, v] of Object.entries(set)) {{ if (!(k in spec)) throw new Error(`--set ${{k}}: the file's params are ${{Object.keys(spec).join(", ") || "none"}}`); p[k] = v; }}
const made = typeof (m as any).default === "function" ? (m as any).default(p) : (m as any).default;
if (!made) throw new Error("the file exports no default solid");
const parts = typeof made.expr === "string" ? {{ solid: made }} : made;
const exprs: Record<string, string> = {{}};
for (const [k, v] of Object.entries(parts)) {{ if (typeof (v as any)?.expr !== "string") throw new Error(`part ${{k}} is not a Solid`); exprs[k] = (v as any).expr; }}
print("RAEA_SOLID_INFO " + JSON.stringify({{ params: p, spec, parts: exprs, roles: (m as any).roles ?? {{}}, needs: (m as any).needs ?? {{}} }}));
"#,
        file = file.display().to_string(),
        set = Value::Object(set.clone()),
    );
    std::fs::write(&w, src).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let run = std::process::Command::new(exe).arg("run").arg(&w).current_dir(&dir).output().map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    let out = String::from_utf8_lossy(&run.stdout).to_string();
    let Some(line) = out.lines().find_map(|l| l.strip_prefix("RAEA_SOLID_INFO ")) else {
        let err = format!("{}{out}", String::from_utf8_lossy(&run.stderr));
        return Err(format!("solid info: {} didn't run: {}", file.display(), err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no output")));
    };
    let v: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
    let parts: Vec<(String, String)> = serde_json::from_value::<std::collections::BTreeMap<String, String>>(v["parts"].clone()).unwrap_or_default().into_iter().collect();
    META.with(|r| *r.borrow_mut() = json!({"roles": v["roles"], "needs": v["needs"]}));
    Ok((v["params"].clone(), v["spec"].clone(), parts))
}

/// Textures asked of a solid: a solid's UVs are box projections, so real UVs and baked maps come from Blender (the
/// solid exported, then bake.py), and the gap is one to build into solids.
fn textured(out: &mut Value, file: &Path) {
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("solid");
    let f = raea_core_meshcheck::blender::fix("bake.py", &format!("media/{stem}.glb media/{stem}-baked.glb --into-colour"));
    let run = format!("raea solid export {} -o media/{stem}.glb --colour #RRGGBB && {}", file.display(), f["run"].as_str().unwrap_or(""));
    let says = format!("its file asks for textures (needs.textured), and a solid's UVs are box projections: export it and bake real UVs and maps; fix: {run}");
    if let Some(a) = out["found"].as_array_mut() {
        a.push(json!({"check": "textures", "level": "warning", "says": says, "fix": {"blender": "bake.py", "run": run}}));
    }
    out["warnings"] = json!(out["warnings"].as_u64().unwrap_or(0) + 1);
}

fn parse_set(s: &str) -> Result<Map<String, Value>, String> {
    let mut m = Map::new();
    for kv in s.split(',').filter(|x| !x.trim().is_empty()) {
        let (k, v) = kv.split_once('=').ok_or_else(|| format!("--set takes k=v,..., got {kv:?}"))?;
        let v = v.trim();
        let val = match v {
            "true" => json!(true),
            "false" => json!(false),
            n => json!(n.parse::<f64>().map_err(|_| format!("--set {k}: a number, true or false, got {n:?}"))?),
        };
        m.insert(k.trim().to_string(), val);
    }
    Ok(m)
}

/// `raea solid ...`, `raea judge ...`, `raea make ...` (docs/make.feature) and `raea ask ...` (docs/ask.feature AK3, which prints as it goes and exits with
/// its own code), the verbs that build and judge a piece (engine/cli hands them over), or None for another verb.
pub fn verb(args: &[String]) -> Option<Result<String, String>> {
    match args.first().map(String::as_str) {
        Some("solid") => Some(cli(args)),
        Some("judge") => Some(raea_app_judge::cli(args)),
        Some("ask") => std::process::exit(raea_app_ask::cli(&args[1..])),
        Some("make") => Some(raea_app_make::cli(args)),
        _ => None,
    }
}

pub fn cli(args: &[String]) -> Result<String, String> {
    let (Some(verb @ ("info" | "view" | "export")), Some(file)) = (args.get(1).map(String::as_str), args.get(2)) else { return Err(USAGE.into()) };
    let file = dunce::canonicalize(&PathBuf::from(file)).map_err(|e| format!("{file}: {e}"))?;
    let json_out = args.iter().any(|a| a == "--json");
    let set = flag(args, "--set").map(|s| parse_set(&s)).transpose()?.unwrap_or_default();
    let expect_path = flag(args, "--expect").map(PathBuf::from).or_else(|| {
        let p = file.with_extension("expect.json");
        p.is_file().then_some(p)
    });
    let expect: Option<Value> = expect_path.as_ref().map(|p| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display())).and_then(|t| serde_json::from_str(&t).map_err(|e| format!("{}: {e}", p.display())))).transpose()?;
    let (params, spec, parts) = made(&file, &set)?;
    let mut out = raea_core_solidinfo::info(&parts, expect.as_ref())?;
    out["file"] = json!(file.display().to_string());
    let src = std::fs::read_to_string(&file).unwrap_or_default();
    let meta = META.with(|r| r.borrow().clone());
    for p in out["parts"].as_array_mut().into_iter().flatten() {
        let name = p["part"].as_str().unwrap_or("").to_string();
        if let Some(role) = meta["roles"][&name].as_str() {
            p["role"] = json!(role);
        }
        if let Some(line) = crate::view::at(&src, &name) {
            p["at"] = json!({"file": file.display().to_string(), "line": line});
        }
    }
    if meta["needs"]["textured"] == true {
        textured(&mut out, &file);
    }
    if verb == "export" {
        return export::export(&file, &parts, &out, args);
    }
    if verb == "view" {
        let dir = crate::view::out_dir(args).ok_or("raea solid view FILE.ts -o DIR: give the folder for the sheets")?;
        return crate::view::view(&file, &set, &out, &dir, json_out);
    }
    out["params"] = params;
    if let Some(p) = &expect_path {
        out["expect"]["file"] = json!(p.display().to_string());
    }
    if !set.is_empty() {
        out["set"] = Value::Object(set.clone());
        let (_, _, base) = made(&file, &Map::new())?;
        let was = raea_core_solidinfo::info(&base, None)?;
        let from = set.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(",");
        out["delta"] = raea_core_solidinfo::delta(&from, &was["solid"], &out["solid"]);
    }
    if args.iter().any(|a| a == "--walk") {
        let base = if set.is_empty() { out["solid"].clone() } else { raea_core_solidinfo::info(&made(&file, &Map::new())?.2, None)?["solid"].clone() };
        let mut walk = Vec::new();
        for (k, v) in spec.as_object().into_iter().flatten() {
            let ends: Vec<Value> = match v {
                Value::Array(a) if a.len() >= 2 => vec![a[0].clone(), a[1].clone()],
                Value::Bool(b) => vec![json!(!b)],
                _ => continue,
            };
            for end in ends {
                let mut one = Map::new();
                one.insert(k.clone(), end.clone());
                let row = match made(&file, &one).and_then(|(_, _, p)| raea_core_solidinfo::info(&p, None)) {
                    Ok(r) => {
                        let (verdict, says) = raea_core_solidinfo::walk_verdict(&base, &r["solid"]);
                        json!({"param": k, "value": end, "verdict": verdict, "says": says, "volume": r["solid"]["volume"], "size": r["solid"]["box"]["size"]})
                    }
                    Err(e) => json!({"param": k, "value": end, "verdict": "broken", "says": e}),
                };
                if row["verdict"] == "broken" {
                    let says = format!("{k} = {end}: {}", row["says"].as_str().unwrap_or("broken"));
                    out["found"].as_array_mut().unwrap().push(json!({"check": "walk", "level": "warning", "says": says}));
                    out["warnings"] = json!(out["warnings"].as_u64().unwrap_or(0) + 1);
                }
                walk.push(row);
            }
        }
        out["walk"] = json!(walk);
    }
    // what it found as raea's one Finding (docs/direction.feature DR3), about the part, or the solid's file
    let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let found = finding::ranked(out["found"].as_array().into_iter().flatten().filter_map(|v| Finding::of_value(v, "raea solid info")).map(|f| if f.about.is_empty() { f.about(&name, None) } else { f }).collect());
    let errors = found.iter().any(|f| f.level == Level::Error);
    if let Some(o) = out.as_object_mut() {
        for k in ["found", "errors", "warnings"] {
            o.remove(k);
        }
    }
    out["count"] = json!(finding::count(&found));
    out["findings"] = json!(found);
    let text = if json_out {
        serde_json::to_string_pretty(&out).unwrap_or_default()
    } else {
        let s = &out["solid"];
        let mut t: Vec<String> = found.iter().map(Finding::line).collect();
        for p in out["parts"].as_array().into_iter().flatten() {
            t.push(format!("part     {}: {} m³, size {}, {} pieces, genus {}", p["part"].as_str().unwrap_or(""), p["volume"], p["box"]["size"], p["pieces"], p["genus"]));
        }
        for w in out["walk"].as_array().into_iter().flatten() {
            t.push(format!("walk     {} = {}: {}{}", w["param"].as_str().unwrap_or(""), w["value"], w["verdict"].as_str().unwrap_or(""), w["says"].as_str().map(|s| format!(" ({s})")).unwrap_or_default()));
        }
        t.push(format!("solid: {} m³, size {} m, {} pieces, genus {}, {} triangles", s["volume"], s["box"]["size"], s["pieces"], s["genus"], s["tris"]));
        t.push(format!("raea solid info: {}", finding::count(&found)));
        t.join("\n")
    };
    if errors {
        println!("{text}");
        std::process::exit(1);
    }
    Ok(text)
}
