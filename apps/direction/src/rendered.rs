//! How `raea render` ends (docs/direction.feature DR3): what the render found, counted on the last line it prints
//! whatever its output is piped through, and its exit: 1 while an error stands that nothing lets through. An error is
//! let through by `--despite CHECK WHY` (a check's name, or `all`), or for a harness that renders many pieces and
//! compares pixels, not scenes, by RAEA_DESPITE="CHECK: why" (which a raea before this ignores, as it would refuse the
//! flag). What was let through, and why, is written into the render's manifest.

use std::path::PathBuf;
use std::time::SystemTime;

use raea_core_facts::record::{self, Despite};
use raea_core_facts::{Finding, Level, finding};
use serde_json::{Value, json};

/// Each `--despite CHECK WHY`, then RAEA_DESPITE's.
fn despite(args: &[String], env: Option<String>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = args.windows(3).filter(|w| w[0] == "--despite").map(|w| (w[1].clone(), w[2].clone())).collect();
    if let Some((check, why)) = env.as_deref().and_then(|e| e.split_once(':')) {
        out.push((check.trim().to_string(), why.trim().to_string()));
    }
    out
}

/// The last line and the exit, for `found` with `through` letting errors past.
fn ending(found: &[Finding], through: &[(String, String)]) -> (String, i32) {
    let excused = |f: &Finding| through.iter().any(|(c, _)| c == "all" || *c == f.check);
    let errors = |past: bool| found.iter().filter(|f| f.level == Level::Error && excused(f) == past).count();
    let mut line = format!("raea render: {}", finding::count(found));
    if errors(true) > 0 {
        let why: Vec<String> = through.iter().map(|(c, w)| format!("--despite {c} {w:?}")).collect();
        line.push_str(&format!("; {} let through by {}", errors(true), why.join(", ")));
    }
    (line, i32::from(errors(false) > 0))
}

/// `raea render`'s exit, once its findings' count is printed; 0 for any other command. Its findings (the clipping
/// check's, its exposure's) go into its manifest, with what was let through and why, and into its project's record.
pub fn rendered(args: &[String]) -> i32 {
    if args.first().map(String::as_str) != Some("render") {
        return 0;
    }
    let mut found = finding::taken();
    let through = despite(args, std::env::var("RAEA_DESPITE").ok());
    let excused: Vec<Despite> = through.iter().map(|(c, w)| Despite { check: c.clone(), why: w.clone() }).collect();
    let out = raea_runtime_bundle::flag(args, "-o").or_else(|| raea_runtime_bundle::flag(args, "--review")).map(PathBuf::from);
    // the manifest every render writes beside its output (DR2), or where --manifest put it
    let manifest = raea_runtime_bundle::flag(args, "--manifest").map(PathBuf::from).or_else(|| Some(raea_runtime_world::manifest_for(out.as_deref()?, std::path::Path::new(args.get(1)?))));
    if let Some(m) = manifest {
        if let Some(mut v) = std::fs::read(&m).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
            // a video's text, held to the check's text rules as it finishes, a check's round sooner
            if let Some(o) = out.as_deref().filter(|o| matches!(o.extension().and_then(|x| x.to_str()), Some("mp4" | "mov"))) {
                // and a sample of its frames rendered again, each held to the video's (SG9, pure.rs)
                for f in text_found(&v).into_iter().chain(crate::pure::check(args, o, &v)) {
                    println!("{}", f.line());
                    found.push(f);
                }
            }
            v["findings"] = json!(found);
            if !excused.is_empty() {
                v["despite"] = json!(excused);
            }
            let _ = std::fs::write(&m, v.to_string());
        }
    }
    // a render that is part of another (a review's tiles, a quick check's stills) leaves the entry to that one
    let file = args.get(1).filter(|a| !a.starts_with('-') && std::env::var("RAEA_RECORD").as_deref() != Ok("off")).map(PathBuf::from);
    if let Some(e) = file.and_then(|f| record::keep("raea render", &f, out.as_deref(), &found, excused, &raea_io_trace::utc(SystemTime::now()).1)) {
        println!("raea render: kept in the project's record as {} (raea findings reads it)", e.id);
    }
    let (line, code) = ending(&found, &through);
    println!("{line}");
    code
}

/// What a video render's text breaks, from its manifest (raea-app-check-text, the rules `raea check` holds it to): text
/// off the frame is an error, as the check has it; text in a UI zone a warn, by the default zones, since the platform's
/// are the check's to decide (`--platform`); reading time waits for the check, which has the transcript to tell a
/// caption the voice reads out from one that has to be read.
fn text_found(manifest: &Value) -> Vec<Finding> {
    let runs = raea_app_check_text::text_runs(manifest);
    let (w, h, fps) = (manifest["width"].as_f64().unwrap_or(1080.0), manifest["height"].as_f64().unwrap_or(1920.0), manifest["fps"].as_f64().unwrap_or(30.0));
    let (mut issues, mut passed) = (Vec::new(), Vec::new());
    raea_app_check_text::text_rules(&runs, None, w, h, None, None, &mut issues, &mut passed);
    let mut found = Vec::new();
    for mut i in issues.into_iter().filter(|i| i.rule != "reading") {
        if i.rule == "safe-area" {
            i.p = 2;
            i.says.push_str(" (by the default zones: `raea check --platform P` holds it to the platform's)");
        }
        found.push(i.finding("raea render", fps));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|a| a.to_string()).collect()
    }

    /// A video's text off the frame fails its render, as the check fails it; text in a UI zone is a warn by the
    /// default zones; a word on screen too briefly is left to the check, which has the transcript
    #[test]
    fn a_video_render_holds_its_text_to_the_check_s_rules() {
        let text = |t: &str, b: [f64; 4]| json!({"text": t, "box": b, "opacity": 1.0, "size": 60.0, "color": "rgb(255, 255, 255)"});
        let frames: Vec<Value> = (0..48).map(|f| json!({"frame": f, "videos": [], "text": [
            text("OFF THE EDGE", [900.0, 800.0, 400.0, 80.0]),
            text("Per month", [700.0, 1233.0, 318.0, 44.0]),
            if f < 3 { text("Two words", [200.0, 900.0, 300.0, 80.0]) } else { json!({"text": "", "box": [0, 0, 0, 0]}) },
        ]})).collect();
        let found = text_found(&json!({"width": 1080, "height": 1920, "fps": 24, "frames": frames}));
        let got: Vec<(&str, Level)> = found.iter().map(|f| (f.check.as_str(), f.level)).collect();
        assert_eq!(got, vec![("clipped", Level::Error), ("safe-area", Level::Warn)], "{found:?}");
        assert!(found[1].says.contains("--platform"));
        assert_eq!(found[0].frames, Some([0, 1]));
    }

    #[test]
    fn an_error_fails_the_render_unless_it_is_let_through_and_says_by_what() {
        let found = vec![Finding::new("clips", Level::Error, "rider in seat", "raea render"), Finding::new("tilted", Level::Warn, "vase leans", "raea render")];
        assert_eq!(ending(&found, &[]), ("raea render: 1 error, 1 warn, 0 notes".into(), 1));
        let through = despite(&s(&["render", "a.tsx", "--despite", "clips", "the rider is posed through it"]), None);
        assert_eq!(ending(&found, &through), ("raea render: 1 error, 1 warn, 0 notes; 1 let through by --despite clips \"the rider is posed through it\"".into(), 0));
        // another check's despite lets nothing through
        assert_eq!(ending(&found, &despite(&s(&["render", "--despite", "floats", "x"]), None)).1, 1);
        // a harness's
        assert_eq!(ending(&found, &despite(&s(&["render"]), Some("all: P26 compares pixels".into()))).1, 0);
        assert_eq!(ending(&[], &[]), ("raea render: 0 errors, 0 warns, 0 notes".into(), 0));
    }
}
