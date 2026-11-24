//! A project's own checks in TypeScript (sdk/raea/src/check.ts, DR4): each file's declaration read first (its id,
//! line, what it needs and where it runs), then one run of every planned check over the worlds it ranges over, in the
//! engine's V8 with nothing drawn. raea/check's `runChecks` runs them; what they found comes back as Findings.

use std::path::{Path, PathBuf};

use raea_core_facts::{Facts, Finding, Level};
use serde_json::{json, Value};

use crate::systems::Frames;

type R<T> = Result<T, String>;

/// What the CLI lends a check: its evaluator of a .ts module's default export (as JSON), and its headless run of a
/// bundle (exit code and what it printed).
pub struct Host {
    pub eval: fn(&Path) -> R<String>,
    pub run: fn(&Path, f64) -> R<(i32, String)>,
}

/// A check file's declaration.
#[derive(Debug, Clone)]
pub struct Meta {
    pub file: PathBuf,
    pub id: String,
    pub text: String,
    pub render: bool,
    pub level: Level,
    pub frames: Frames,
    /// it runs on a story with no frames (`over: "story" | "both"`): what `raea check NAME.story.ts` runs
    pub story: bool,
}

/// The declaration of the check `file`'s default export is.
pub fn meta(file: &Path, host: &Host) -> R<Meta> {
    let name = file.display();
    let v: Value = serde_json::from_str(&(host.eval)(file)?).map_err(|e| format!("{name}: {e}"))?;
    let s = |k: &str| v[k].as_str().map(str::to_string);
    let id = s("id").ok_or_else(|| format!("{name}: its default export isn't a check({{ id, text }}, system) from \"raea/check\""))?;
    let render = match v["needs"].as_str() {
        None => false,
        Some("render") => true,
        Some(o) => return Err(format!("{name}: a check needs \"render\" or nothing, not \"{o}\"")),
    };
    let level = match s("level").as_deref() {
        None => Level::Error,
        Some(w @ ("error" | "warn" | "note")) => Level::of_word(w),
        Some(o) => return Err(format!("{name}: a check's level is error, warn or note, not \"{o}\"")),
    };
    let frames = Frames::of(&v["frames"]).map_err(|e| format!("{name}: {e}"))?;
    let story = match s("over").as_deref() {
        None | Some("frames") => false,
        Some("story" | "both") => true,
        Some(o) => return Err(format!("{name}: a check is over \"frames\", \"story\" or \"both\", not \"{o}\"")),
    };
    Ok(Meta { file: file.to_path_buf(), id, text: s("text").unwrap_or_default(), render, level, frames, story })
}

/// One planned run: the check (its index in `files`), its frames, and its rule's values, level (None: the check's
/// own) and `by`.
pub struct Planned {
    pub check: usize,
    pub frames: Vec<i64>,
    pub values: serde_json::Map<String, Value>,
    pub level: Option<Level>,
    pub by: String,
}

/// Every planned check run on its frames of `worlds`, in one bundle: what each found (its rule's level where the
/// finding names none, its `by` the rule's), and a finding for each check that threw.
pub fn run(files: &[PathBuf], plan: &[Planned], worlds: &[Facts], host: &Host) -> R<Vec<Finding>> {
    if plan.is_empty() {
        return Ok(Vec::new());
    }
    let work = std::env::temp_dir().join(format!("raea-check-{}", std::process::id()));
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let data = work.join("worlds.json");
    std::fs::write(&data, serde_json::to_string(worlds).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let imports: String = files.iter().enumerate().map(|(i, f)| format!("import c{i} from {:?};\n", dunce::canonicalize(&f).unwrap_or_else(|_| f.clone()).display().to_string())).collect();
    let checks = (0..files.len()).map(|i| format!("c{i}")).collect::<Vec<_>>().join(", ");
    let planned: Vec<Value> = plan.iter().map(|p| json!({"check": p.check, "frames": p.frames, "values": p.values, "level": p.level, "by": p.by})).collect();
    let entry = format!(
        "import {{ declared, runChecks }} from \"raea/check\";\n{imports}import worlds from {data:?};\nconst call = (globalThis as any).__raea_call;\n\
         try {{ const checks = [{checks}]; declared(checks); const r = runChecks(checks, {plan}, worlds as any); call(\"print\", JSON.stringify({{ text: JSON.stringify(r) }})); call(\"done\", JSON.stringify({{ code: 0 }})); }}\n\
         catch (e: any) {{ call(\"print\", JSON.stringify({{ text: \"error: \" + (e && e.message ? e.message : String(e)) }})); call(\"done\", JSON.stringify({{ code: 1 }})); }}\n",
        data = data.display().to_string(),
        plan = Value::Array(planned),
    );
    let js = work.join("checks.js");
    let ran = raea_runtime_bundle::bundle_entry(&entry, &js, &[]).and_then(|_| {
        // what the checks print is raea's to read: nothing reaches the terminal
        let echo = raea_runtime_react::native::ECHO.swap(false, std::sync::atomic::Ordering::Relaxed);
        let ran = (host.run)(&js, 120.0);
        raea_runtime_react::native::ECHO.store(echo, std::sync::atomic::Ordering::Relaxed);
        ran
    });
    let _ = std::fs::remove_dir_all(&work);
    let (code, printed) = ran?;
    let last = printed.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
    if code != 0 {
        return Err(format!("the project's checks: {}", last.strip_prefix("error: ").unwrap_or(last)));
    }
    let got: Value = serde_json::from_str(last).map_err(|e| format!("the project's checks printed {last:?}: {e}"))?;
    // a finding that doesn't read is said as it came, never dropped
    let unread = |f: &Value| Finding::new(f["check"].as_str().unwrap_or("check"), Level::Error, format!("a finding raea couldn't read: {f}"), "raea check");
    let mut out: Vec<Finding> = got["findings"].as_array().into_iter().flatten().map(|f| Finding::read(f).unwrap_or_else(|| unread(f))).collect();
    for t in got["threw"].as_array().into_iter().flatten() {
        let id = t["check"].as_str().unwrap_or("check");
        out.push(Finding::new(id, Level::Error, format!("the check threw {}", t["says"].as_str().unwrap_or("")), "raea check").fix("fix the check: it ran on none of the frames after this one"));
    }
    Ok(out)
}
