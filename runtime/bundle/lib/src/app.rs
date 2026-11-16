//! An app's script (docs/targets.feature T0b, T1c): its TSX with React, @raea/ui and @raea/rune from raea's home (the
//! checkout's, or a release's share/raea), mounted in DOM mode, bundled in process. Both targets run the same script:
//! the page host on a Blitz page, the native host (raea-rune-host) on Rune.

use std::path::Path;

/// The entry every app is mounted by: the DOM installed before the app's module runs, its default export mounted at
/// the root with `props` (JS; an app's none) (an app that mounts itself with createRoot needs no default export)
fn entry(app: &Path, props: &str) -> String {
    format!(
        "import \"@raea/ui/dom-globals\";\nimport {{ createElement }} from \"react\";\nimport {{ mount }} from \"@raea/ui/react\";\nimport * as M from {:?};\nconst App = (M as any).default;\nif (typeof App === \"function\") mount(createElement(App, {props}), 0, {{ dom: true }});\n",
        app.display().to_string()
    )
}

/// The work folder's tsconfig: JSX by React's automatic runtime, @raea/ui and @raea/rune from the SDK's source
fn tsconfig(home: &Path) -> String {
    let ui = home.join("sdk/ui/src");
    let rune = home.join("sdk/rune/src/index.ts");
    serde_json::json!({"compilerOptions": {"jsx": "react-jsx", "jsxImportSource": "react", "module": "esnext", "target": "es2022", "moduleResolution": "bundler",
        "paths": {"@raea/ui/*": [format!("{}/*", ui.display())], "@raea/rune": [rune.display().to_string()]}}})
    .to_string()
}

/// `@raea/ui/<module>` for each of sdk/ui/src's modules, by name: the work folder's tsconfig reaches only the entry,
/// and an app outside it (anywhere on disk) imports @raea/ui/wire, @raea/ui/platform... as well
fn ui(home: &Path) -> serde_json::Value {
    let src = home.join("sdk/ui/src");
    let mut m = serde_json::Map::new();
    for e in std::fs::read_dir(&src).into_iter().flatten().flatten() {
        let p = e.path();
        if let (Some(stem), Some("ts" | "tsx")) = (p.file_stem().and_then(|s| s.to_str()), p.extension().and_then(|x| x.to_str())) {
            m.insert(format!("@raea/ui/{stem}"), p.display().to_string().into());
        }
    }
    m.into()
}

/// What an app's default export is given, for `bundle`: nothing, as an app in a window of its own is (it reads the
/// window)
pub const WINDOW: &str = "null";

/// What an app screen's module (`<Screen app="kit.tsx">`, docs/blitz-bevy.feature H12) is given: `{width, height}`, the
/// screen's size (its document's, so one bundle serves a screen of any size)
pub const SCREEN: &str = "{ width: innerWidth, height: innerHeight }";

/// `app` bundled against `home`, its default export mounted with `props` (WINDOW or SCREEN): its script and the CSS its
/// modules import
pub fn bundle(app: &Path, home: &Path, props: &str) -> Result<(String, String), String> {
    // canonical, and on Windows without canonicalize()'s verbatim \\?\ prefix: Rolldown can't resolve an entry that
    // imports a path written with it (T1f: the Windows release check, 11-07)
    let app = raea_io_home::plain(app.canonicalize().map_err(|e| format!("{}: {e}", app.display()))?);
    // one path per file: a module reached by two spellings of its path is bundled twice (two wires, two call ids)
    let home = &raea_io_home::plain(home.canonicalize().map_err(|e| format!("{}: {e}", home.display()))?);
    // one work folder per call: tests bundle on several threads of one process
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let work = std::env::temp_dir().join(format!("raea-app-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let entry_file = work.join("entry.tsx");
    let wrote = std::fs::write(&entry_file, entry(&app, props)).and_then(|_| std::fs::write(work.join("tsconfig.json"), tsconfig(home)));
    let r = wrote.map_err(|e| e.to_string()).and_then(|_| {
        let opts = serde_json::json!({
            "script": {"js": home.join("sdk/react/js"), "motion": home.join("sdk/raea/src"), "shell": home.join("sdk/shell/src/index.ts"), "aliases": ui(home), "stubs": []},
            "defines": {},
        });
        crate::bundle(&entry_file, opts)
    });
    let _ = std::fs::remove_dir_all(&work);
    r.map_err(|e| format!("couldn't bundle {}: {}", app.display(), e.lines().filter(|l| !l.trim().is_empty()).take(8).collect::<Vec<_>>().join(" | ")))
}
