//! `raea blender run SCRIPT.ts [FILE.blend] [ARGS...]`: a headless Blender with raea's bridge (FILE.blend opened, ARGS
//! its sys.argv after --), the script on raea's engine with each batch posted to it (blender.post), Blender quit after.

use std::path::{Path, PathBuf};

use raea_runtime_calls::blender::{BRIDGE, post};
use serde_json::json;

use crate::{R, blender_bridge};

/// What an entry does once its work is a promise: the session closed (sdk/blender/src/blender.ts's
/// __raea_blender_end), so a failure in Blender the script never awaited fails the run, as an uncaught Python exception
/// would; then the run's exit.
pub(crate) const CLOSE: &str = ".then(() => (globalThis as any).__raea_blender_end?.()).then(() => call(\"done\", JSON.stringify({ code: 0 })), (e: any) => { call(\"print\", JSON.stringify({ text: \"error: \" + String(e) + (e && e.name === \"BlenderError\" ? (e.pythonTrace ? \"\\n\" + e.pythonTrace : \"\") : e && e.stack ? \"\\n\" + e.stack : \"\") })); call(\"done\", JSON.stringify({ code: 1 })); });";

/// The script's default export, called and awaited.
pub(crate) const DEFAULT: &str = "Promise.resolve().then(() => (typeof (M as any).default === \"function\" ? (M as any).default() : undefined))";

/// `--timeout SECS` taken out of ARGS (Blender's sys.argv), and its seconds.
pub(crate) fn timeout(rest: &mut Vec<String>) -> f64 {
    let mut secs = 3600.0;
    if let Some(i) = rest.iter().position(|a| a == "--timeout") {
        secs = rest.get(i + 1).and_then(|t| t.parse().ok()).unwrap_or(secs);
        rest.drain(i..(i + 2).min(rest.len()));
    }
    secs
}

/// An entry's bundle run against a headless Blender started with the bridge (`file` opened, `rest` its sys.argv):
/// the bundle's exit code. Blender is quit after, and killed if the run ends any other way.
pub(crate) fn session(entry: &str, file: Option<&Path>, rest: &[String], secs: f64) -> R<i32> {
    let (bin, _) = raea_app_tools::blender::find().ok_or("no Blender: raea setup blender finds or downloads one")?;
    let bridge = blender_bridge::start(&bin, file, rest)?;
    *BRIDGE.lock().map_err(|e| e.to_string())? = Some((bridge.port, bridge.token.clone()));
    let out = std::env::temp_dir().join(format!("raea-blender-{}.js", std::process::id()));
    let ran = raea_runtime_bundle::bundle_entry(entry, &out, &[]).and_then(|_| raea_runtime_export::run_bundle_to_terminal(&out, secs));
    let _ = std::fs::remove_file(&out);
    let _ = post(&json!({ "sid": "raea", "ops": [{ "o": "quit" }] }));
    *BRIDGE.lock().map_err(|e| e.to_string())? = None;
    bridge.wait();
    Ok(ran?.0)
}

/// A path as an entry's import specifier (Windows' verbatim prefix dropped, as raea-runtime-bundle does).
pub(crate) fn spec(p: &Path) -> String {
    format!("{:?}", dunce::simplified(p).display().to_string())
}

pub fn run(args: &[String]) -> R<String> {
    let ts = dunce::canonicalize(&PathBuf::from(&args[2])).map_err(|e| format!("{}: {e}", args[2]))?;
    let file = args.get(3).filter(|a| a.ends_with(".blend")).map(PathBuf::from);
    // ARGS for Blender's sys.argv, without raea's own --timeout SECS
    let mut rest = args[if file.is_some() { 4 } else { 3 }..].to_vec();
    let secs = timeout(&mut rest);
    let entry = format!("import * as M from {};\nconst call = (globalThis as any).__raea_call;\n{DEFAULT}{CLOSE}\n", spec(&ts));
    let code = session(&entry, file.as_deref(), &rest, secs)?;
    if code != 0 {
        return Err(format!("{} ended with exit {code}", args[2]));
    }
    Ok(String::new())
}
