//! The native target: raea-rune-host (docs/rune.feature), handed the verb, the app's script and its window. raea bundles
//! the app (runtime/bundle/lib app.rs, the same script the web target runs), as the page host is handed its bundle:
//! no bun (docs/targets.feature T1c).

use std::path::{Path, PathBuf};

use crate::verbs::{Ask, Verb, find};

const HOST: &str = "raea-rune-host";

/// raea-rune-host beside raea, on PATH, or built in the checkout (engine/rune, into the workspace's target/)
fn bin() -> Result<PathBuf, String> {
    let home = raea_io_home::home();
    let built: Vec<PathBuf> = ["release", "debug"].map(|p| home.join("target").join(p).join(HOST)).to_vec();
    find(HOST, &built)
}

/// The host's own arguments, for an ask
pub fn args(a: &Ask) -> Vec<String> {
    let size = format!("{}x{}", a.size.0, a.size.1);
    let app = a.app.display().to_string();
    let mut v: Vec<String> = match a.verb {
        Verb::Run => vec!["run".into(), app, "--size".into(), size, "--title".into(), a.title.clone()],
        Verb::Bundle => {
            let mut v = vec!["bundle".into(), app, "--size".into(), size, "--title".into(), a.title.clone()];
            if let Some(o) = &a.out {
                v.extend(["-o".into(), o.display().to_string()]);
            }
            v
        }
        Verb::Check => {
            let mut v = vec!["--headless".into(), app, "--png".into(), a.png.display().to_string(), "--size".into(), size, "--settle".into(), a.settle.to_string()];
            if let Some(s) = &a.script {
                v.extend(["--script".into(), s.display().to_string()]);
            }
            v
        }
    };
    v.extend(a.rest.iter().cloned());
    v
}

pub fn run(a: &Ask) -> Result<String, String> {
    host(&args(a))?;
    match a.verb {
        Verb::Run => return Ok(String::new()),
        // the portability report (T4a): anything the host didn't draw or answer fails the check
        Verb::Check => return crate::portable::verdict(&crate::portable::report(&a.png)?, &a.png),
        Verb::Bundle => {}
    }
    // the host makes the .app (signed ad hoc); raea signs it as the developer asks, as it does a web app (T1d)
    let out = a.out.clone().unwrap_or_else(|| PathBuf::from(format!("{}.app", crate::verbs::stem(&a.app))));
    Ok(crate::sign::sign(&out, &a.sign)?.join("\n"))
}

/// raea-rune-host run with `args`, its output passed through; an app's source among them (a .tsx, .ts or .jsx file) is
/// bundled first and the host is handed its script
pub fn host(args: &[String]) -> Result<String, String> {
    let host = bin()?;
    let work = std::env::temp_dir().join(format!("raea-rune-app-{}", std::process::id()));
    let mut handed = Vec::with_capacity(args.len());
    let mut extra = Vec::new();
    for a in args {
        let p = Path::new(a);
        if p.extension().is_some_and(|x| x == "tsx" || x == "ts" || x == "jsx") && p.is_file() {
            std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
            let (js, css) = raea_runtime_bundle::inproc::app::bundle(p, &raea_io_home::home(), raea_runtime_bundle::inproc::app::WINDOW)?;
            let script = work.join(format!("{}.js", p.file_stem().unwrap_or_default().to_string_lossy()));
            std::fs::write(&script, js).map_err(|e| format!("{}: {e}", script.display()))?;
            handed.push(script.display().to_string());
            // the app's folder (relative media and fonts) and the CSS it imported (its @font-face rules), T3d
            let sheet = script.with_extension("css");
            std::fs::write(&sheet, css).map_err(|e| format!("{}: {e}", sheet.display()))?;
            let dir = p.canonicalize().ok().map(raea_io_home::plain).and_then(|c| c.parent().map(Path::to_path_buf)).unwrap_or_default();
            extra.extend(["--base".to_string(), dir.display().to_string(), "--css".to_string(), sheet.display().to_string()]);
        } else {
            handed.push(a.clone());
        }
    }
    let mut cmd = std::process::Command::new(&host);
    // the host films through `raea media encode`: this raea, so the film is signed (or not) as this build signs (T1e)
    if std::env::var_os("RAEA_BIN").is_none() && let Ok(me) = std::env::current_exe() {
        cmd.env("RAEA_BIN", me);
    }
    let status = cmd.args(&handed).args(&extra).status().map_err(|e| format!("{}: {e}", host.display()));
    let _ = std::fs::remove_dir_all(&work);
    let status = status?;
    status.success().then(String::new).ok_or(format!("{HOST} exited with {status}"))
}
