//! The web target: the app on @raea/ui's React adapter in DOM mode, bundled by raea's own bundler (Rolldown, in
//! process) with Tailwind's CSS for the classes it uses, on a Blitz page in Bevy: raea-page-host, as the feed is.

use std::path::{Path, PathBuf};

use crate::verbs::{Ask, Verb, find};

const HOST: &str = "raea-page-host";

/// The page host beside raea, on PATH, or built in the checkout (its window build: `--features window`)
pub fn bin() -> Result<PathBuf, String> {
    let home = raea_io_home::home();
    find(HOST, &[home.join("target/release").join(HOST), home.join("target/debug").join(HOST)])
}

/// The app's page, built: its script and its stylesheet, in `dir`
pub struct Built {
    pub js: PathBuf,
    pub css: PathBuf,
}

/// `app` bundled into `dir` (made if missing): app.js and app.css (Tailwind's, for the classes the app's folder uses, after
/// any CSS the app imports)
pub fn build(app: &Path, dir: &Path) -> Result<Built, String> {
    let app = raea_io_home::plain(app.canonicalize().map_err(|e| format!("{}: {e}", app.display()))?);
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (js, imported) = raea_runtime_bundle::inproc::app::bundle(&app, &raea_io_home::home(), raea_runtime_bundle::inproc::app::WINDOW)?;
    let folder = app.parent().unwrap_or(Path::new("."));
    let tw = raea_runtime_bundle::styles::tailwind(folder)?;
    let built = Built { js: dir.join("app.js"), css: dir.join("app.css") };
    std::fs::write(&built.js, js).map_err(|e| format!("{}: {e}", built.js.display()))?;
    std::fs::write(&built.css, format!("{imported}\n{tw}")).map_err(|e| format!("{}: {e}", built.css.display()))?;
    Ok(built)
}

pub fn run(a: &Ask) -> Result<String, String> {
    match a.verb {
        Verb::Run => {
            let dir = std::env::temp_dir().join(format!("raea-app-run-{}", std::process::id()));
            let b = build(&a.app, &dir)?;
            let st = std::process::Command::new(bin()?)
                .arg(&b.js)
                .args(["--css".as_ref(), b.css.as_os_str(), "--window".as_ref(), "--title".as_ref(), a.title.as_ref(), "--size".as_ref(), format!("{}x{}", a.size.0, a.size.1).as_ref()])
                .status()
                .map_err(|e| format!("{HOST}: {e}"))?;
            let _ = std::fs::remove_dir_all(&dir);
            st.success().then(String::new).ok_or(format!("{HOST} exited with {st}"))
        }
        Verb::Check => {
            let said = crate::webcheck::check(a)?;
            Ok([said, native_note(a)].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n"))
        }
        Verb::Bundle => crate::macapp::web(a),
    }
}

/// The Rune props the web ignores (T4b) and what the native target wouldn't draw (T4a): the app through
/// raea-rune-host headless too, with the same steps, into `--png`'s native/. Said, never failed on: a web app is
/// checked against web. A host that can't run says why.
fn native_note(a: &Ask) -> String {
    let dir = a.png.join("native");
    let native = Ask { png: dir.clone(), ..a.clone() };
    let read = |_| Ok::<_, String>((crate::portable::report(&dir)?, crate::portable::web_ignored(&dir)?));
    match crate::native::host(&crate::native::args(&native)).and_then(read) {
        Ok((lines, web)) => [crate::portable::web_note(&web), crate::portable::note(&lines)].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n"),
        Err(e) => format!("native: the Rune host couldn't run it headless ({e}), so what native wouldn't draw isn't known"),
    }
}
