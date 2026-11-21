//! Specs written in TypeScript (docs/handoff/authoring.md, docs/projects.feature PJ0): brief.ts, script.ts, taste.ts
//! and NAME.parts.ts (an object's parts). A spec is a module whose default export is data; the engine evaluates it headless
//! (bundle_module with PRINT_DEFAULT, run_bundle) and Rust reads the JSON against the schema in core/brief, so tsc's errors come first and
//! these come second, naming the field.
use super::*;
use raea_core_brief as schema;
use serde::de::DeserializeOwned;
use std::sync::atomic::Ordering;

/// The default export of a spec module, as the JSON it evaluates to.
pub fn spec_json(ts: &Path) -> R<String> {
    let ts = dunce::canonicalize(&ts).map_err(|e| format!("{}: {e}", ts.display()))?;
    let out = std::env::temp_dir().join(format!("raea-spec-{}.js", std::process::id()));
    bundle_module(&ts, &out, PRINT_DEFAULT)?;
    // the JSON is ours to read, not the person's to see: the run prints nothing to the terminal
    let echo = raea_runtime_react::native::ECHO.swap(false, Ordering::Relaxed);
    let ran = run_bundle(&out, 60.0);
    raea_runtime_react::native::ECHO.store(echo, Ordering::Relaxed);
    let _ = std::fs::remove_file(&out);
    let (code, printed) = ran?;
    let last = printed.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").to_string();
    if code != 0 {
        return Err(format!("{}: {}", ts.display(), last.strip_prefix("error: ").unwrap_or(&last)));
    }
    Ok(last)
}

/// A spec read against its schema; the error names the file and the field.
pub fn read_spec<T: DeserializeOwned>(ts: &Path) -> R<T> {
    let name = ts.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    schema::read(&name, &spec_json(ts)?)
}

/// What `raea context` reads from a project's brief.ts.
pub struct Loaded {
    pub path: PathBuf,
    /// what the agent reads: the file as written, with what's missing from it after
    pub text: String,
    pub title: String,
    pub modes: Vec<String>,
    /// deliverables' own briefs and the project's script, each (path, text)
    pub more: Vec<(PathBuf, String)>,
    pub look: Option<String>,
    /// what the brief still needs (schema::brief::doctor), empty when it's shaped
    pub missing: Vec<String>,
}

/// A project's brief.ts: checked against the Brief schema, its modes, its deliverables' briefs and its script.
pub fn load_brief_ts(path: &Path) -> R<Loaded> {
    let brief: schema::Brief = read_spec(path)?;
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let missing = schema::brief::doctor(&brief);
    if !missing.is_empty() {
        text.push_str(&format!("\n\nWhat the brief still needs:\n{}", missing.iter().map(|m| format!("- {m}")).collect::<Vec<_>>().join("\n")));
    }
    let mut modes: Vec<String> = brief.mode.iter().map(|m| m.trim().to_ascii_lowercase()).filter(|m| !m.is_empty()).collect();
    let mut more = Vec::new();
    let mut files: Vec<String> = brief.deliver.iter().flat_map(|d| [d.brief.clone(), d.script.clone()]).flatten().collect();
    if dir.join("script.ts").is_file() && !files.iter().any(|f| f == "script.ts") {
        files.push("script.ts".into());
    }
    for f in files {
        let p = dir.join(&f);
        let checked = if f.contains("script") { read_spec::<schema::Script>(&p).map(|_| ()) } else { read_spec::<schema::Brief>(&p).map(|b| modes.extend(b.mode)) };
        let body = std::fs::read_to_string(&p).unwrap_or_else(|e| format!("(missing: {e})"));
        more.push((p, match checked {
            Ok(()) => body,
            Err(e) => format!("{body}\n\n(raea can't read it: {e})"),
        }));
    }
    modes.dedup();
    Ok(Loaded { path: path.to_path_buf(), text, title: brief.title.clone(), modes, more, look: brief.look, missing })
}

/// A taste file by name (taste, motion, games...): NAME.ts, checked as Taste.
pub fn taste_file(dir: &Path, name: &str) -> Option<(PathBuf, String)> {
    let ts = dir.join(format!("{name}.ts"));
    let body = std::fs::read_to_string(&ts).ok()?;
    Some(match read_spec::<schema::Taste>(&ts) {
        Ok(_) => (ts, body),
        Err(e) => (ts, format!("{body}\n\n(raea can't read it: {e})")),
    })
}

/// `--taste`'s .ts files, each checked as Taste.
pub fn tastes_ts(args: &[String]) -> R<Vec<schema::Taste>> {
    raea_app_direction::check::taste_files(args).iter().map(|f| read_spec(f)).collect()
}

/// `raea spec FILE.ts [--as KIND]`: the default export as JSON, checked as KIND (by default from the file's name:
/// brief, script, taste or parts in it; anything else is checked as Taste, the files in
/// ~/.raea/taste being named for their kind).
pub fn spec_cli(args: &[String]) -> R<String> {
    const USAGE: &str = "usage: raea spec FILE.ts [--as brief|script|taste|parts]";
    let file = args.iter().enumerate().find(|(i, a)| !a.starts_with("--") && (*i == 0 || args[i - 1] != "--as")).map(|(_, a)| PathBuf::from(a)).ok_or(USAGE)?;
    let kind = args.iter().position(|a| a == "--as").and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| {
        let base = file.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        ["brief", "script", "parts"].iter().find(|k| base.contains(*k)).map(|k| k.to_string()).unwrap_or_else(|| "taste".into())
    });
    let json = spec_json(&file)?;
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let value: serde_json::Value = match kind.as_str() {
        "brief" => serde_json::to_value(schema::read::<schema::Brief>(&name, &json)?),
        "script" => serde_json::to_value(schema::read::<schema::Script>(&name, &json)?),
        "taste" => serde_json::to_value(schema::read::<schema::Taste>(&name, &json)?),
        "parts" => serde_json::to_value(schema::read::<schema::Parts>(&name, &json)?),
        k => return Err(format!("raea spec can't check a {k:?}: brief, script, taste or parts\n{USAGE}")),
    }
    .map_err(|e| e.to_string())?;
    Ok(value.to_string())
}
