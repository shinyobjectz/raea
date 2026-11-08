//! Where raea's own files are, for the crates that read them (the SDK's TypeScript, the tools that install the local
//! models, the fonts), and where its per-user caches and logs go on each platform.

pub mod sdk;

/// Where raea's own files are (the SDK's TypeScript, the lite runtime and its packages, the shell's sources, the
/// sandbox's modules, the tools that install the local models): $RAEA_HOME; else beside an installed binary,
/// `<prefix>/share/raea` for `<prefix>/bin/raea` (the release's layout, tools/release.sh); else the tree this
/// build came from. RAEA_NO_TREE=1 refuses the last: a release's check that it never reaches back into its build tree.
pub fn home() -> std::path::PathBuf {
    static HOME: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    HOME.get_or_init(|| {
        if let Some(h) = std::env::var_os("RAEA_HOME").filter(|v| !v.is_empty()) {
            return std::path::PathBuf::from(h);
        }
        let installed = std::env::current_exe().ok().and_then(|e| e.canonicalize().ok()).map(plain).and_then(|e| e.parent()?.parent().map(|p| p.join("share/raea")));
        if let Some(d) = installed.filter(|d| d.join("sdk/raea/src").is_dir()) {
            return d;
        }
        if std::env::var_os("RAEA_NO_TREE").is_some() {
            eprintln!("raea: no share/raea beside {} and RAEA_NO_TREE is set: its own files can't be found", std::env::current_exe().map(|e| e.display().to_string()).unwrap_or_default());
            return std::path::PathBuf::from("/nonexistent/raea");
        }
        let tree = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        tree.canonicalize().map(plain).unwrap_or(tree)
    })
    .clone()
}

/// A path without the verbatim `\\?\` prefix Windows' canonicalize() puts before a drive path: Rolldown's resolver
/// can't open an import that starts with it, so `raea play` couldn't bundle a game (the lite entry imports bevy-react's
/// bridge.ts by its path under home()), and raea-bundle's resolver names modules without it (engine/bundle/src/edit.rs). Any
/// other path, a `\\?\UNC\` one included, is kept as it is.
pub fn plain(p: std::path::PathBuf) -> std::path::PathBuf {
    match p.to_str().and_then(drive) {
        Some(r) => r.into(),
        None => p,
    }
}

/// a verbatim drive path's plain form (`\\?\C:\x` is `C:\x`); None for any other path
fn drive(s: &str) -> Option<&str> {
    s.strip_prefix(r"\\?\").filter(|r| r.as_bytes().get(1) == Some(&b':'))
}

/// raea's per-user cache folder: ~/Library/Caches/raea on macOS, $XDG_CACHE_HOME/raea (~/.cache/raea by default)
/// elsewhere on Unix, %LOCALAPPDATA%\raea\cache on Windows. None when that platform's home isn't set.
pub fn caches() -> Option<std::path::PathBuf> {
    user_dir("Library/Caches/raea", "XDG_CACHE_HOME", ".cache", "cache")
}

/// raea's per-user log folder: ~/Library/Logs/raea on macOS, $XDG_STATE_HOME/raea (~/.local/state/raea by default)
/// elsewhere on Unix, %LOCALAPPDATA%\raea\logs on Windows. None when that platform's home isn't set.
pub fn logs() -> Option<std::path::PathBuf> {
    user_dir("Library/Logs/raea", "XDG_STATE_HOME", ".local/state", "logs")
}

/// macOS's folder under HOME, the XDG base folder (its variable, else its default under HOME) with raea in it, or raea's
/// folder of that kind under %LOCALAPPDATA%. An XDG variable that isn't an absolute path is ignored, as the spec says.
fn user_dir(apple: &str, xdg: &str, xdg_default: &str, windows: &str) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA").filter(|d| !d.is_empty()).map(|d| PathBuf::from(d).join("raea").join(windows));
    }
    let home = PathBuf::from(std::env::var_os("HOME").filter(|h| !h.is_empty())?);
    if cfg!(target_vendor = "apple") {
        return Some(home.join(apple));
    }
    let base = std::env::var_os(xdg).map(PathBuf::from).filter(|d| d.is_absolute()).unwrap_or_else(|| home.join(xdg_default));
    Some(base.join("raea"))
}

/// A path an environment variable names, else `under_home` in the user's home folder: RAEA_GEN_CACHE's
/// ~/.cache/raea-gen, RAEA_ENV_FILE's ~/.raea/env. An empty variable counts as unset.
pub fn env_path(var: &str, under_home: &str) -> std::path::PathBuf {
    match std::env::var_os(var).filter(|v| !v.is_empty()) {
        Some(p) => p.into(),
        None => std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(under_home),
    }
}

/// A setting: the process environment first, then raea's settings file ($RAEA_ENV_FILE, else ~/.raea/env), where a
/// line is NAME=value (quotes around the value dropped) and a line starting with # is a comment. Empty is unset.
pub fn setting(name: &str) -> Option<String> {
    if let Some(v) = std::env::var(name).ok().filter(|v| !v.trim().is_empty()) {
        return Some(v.trim().to_string());
    }
    std::fs::read_to_string(env_path("RAEA_ENV_FILE", ".raea/env")).ok()?.lines().find_map(|l| {
        let (k, v) = l.trim().split_once('=')?;
        (k.trim() == name && !l.trim_start().starts_with('#')).then(|| v.trim().trim_matches('"').to_string())
    }).filter(|v| !v.is_empty())
}

/// A local file as a page's `src` (an <img>, a page's snapshot): its path on Unix; on Windows a file URL, since the
/// page reads `C:\...` as a URL whose scheme is `c` and loads nothing (PT20). A verbatim `\\?\C:\` path loses its
/// prefix first.
pub fn src(p: &std::path::Path) -> String {
    src_on(&p.display().to_string(), cfg!(windows))
}

/// `src` of the path `s` as Windows writes it (`windows`) or as Unix does, whichever system asks (a test of Windows'
/// form runs on the Mac too)
pub fn src_on(s: &str, windows: bool) -> String {
    if !windows {
        return s.to_string();
    }
    let s = drive(s).unwrap_or(s);
    if s.as_bytes().get(1) == Some(&b':') { format!("file:///{}", s.replace('\\', "/")) } else { s.to_string() }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_src_is_a_file_url_on_windows() {
        assert_eq!(super::src_on("/Users/a/assets/m.png", false), "/Users/a/assets/m.png");
        assert_eq!(super::src_on(r"C:\Users\a\assets/m.png", true), "file:///C:/Users/a/assets/m.png");
        assert_eq!(super::src_on(r"\\?\C:\a\m.png", true), "file:///C:/a/m.png");
        assert_eq!(super::src_on("assets/m.png", true), "assets/m.png");
    }

    #[test]
    fn home_is_no_verbatim_path() {
        use std::path::PathBuf;
        assert_eq!(super::plain(PathBuf::from(r"\\?\C:\github\raea")), PathBuf::from(r"C:\github\raea"));
        assert_eq!(super::plain(PathBuf::from(r"\\?\UNC\server\share")), PathBuf::from(r"\\?\UNC\server\share"));
        assert_eq!(super::plain(PathBuf::from("/Users/a/raea")), PathBuf::from("/Users/a/raea"));
        // the tree a test build came from, canonicalized: on Windows that added \\?\ before
        if std::env::var_os("RAEA_HOME").is_none() {
            let home = super::home().display().to_string();
            assert!(!home.starts_with(r"\\?\"), "{home}");
        }
    }

    #[test]
    fn caches_and_logs_follow_the_platform() {
        // one test, so nothing else reads these variables while it sets them
        unsafe {
            std::env::set_var("HOME", "/h");
            std::env::set_var("XDG_CACHE_HOME", "relative");
            std::env::set_var("XDG_STATE_HOME", "/state");
        }
        let (c, l) = (super::caches().unwrap(), super::logs().unwrap());
        if cfg!(target_vendor = "apple") {
            assert_eq!((c.to_str().unwrap(), l.to_str().unwrap()), ("/h/Library/Caches/raea", "/h/Library/Logs/raea"));
        } else if cfg!(unix) {
            // a relative XDG_CACHE_HOME is ignored, as the spec says
            assert_eq!((c.to_str().unwrap(), l.to_str().unwrap()), ("/h/.cache/raea", "/state/raea"));
        }
    }
}

/// RAEA_OFFLINE (in the environment, or set in raea's settings file, $RAEA_ENV_FILE else ~/.raea/env): raea sends
/// nothing over the network. Every outbound call asks this first, so set, a fetch, a model download, a sample library, a
/// composition's own fetch() or WebSocket, a generation step or a signing request is refused instead, saying what wanted
/// out; what's already on disk (raea's caches, installed models) still works. "0" or empty is off.
pub fn offline(what: &str) -> Result<(), String> {
    let on = |v: &str| !v.trim().is_empty() && v.trim() != "0";
    let set = match std::env::var("RAEA_OFFLINE") {
        Ok(v) => on(&v),
        Err(_) => {
            let file = std::env::var_os("RAEA_ENV_FILE").map(std::path::PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".raea/env")));
            file.and_then(|f| std::fs::read_to_string(f).ok()).is_some_and(|s| {
                s.lines().filter(|l| !l.trim_start().starts_with('#')).filter_map(|l| l.split_once('=')).any(|(k, v)| k.trim() == "RAEA_OFFLINE" && on(v.trim().trim_matches('"')))
            })
        }
    };
    if set { Err(format!("{what}: not sent, because RAEA_OFFLINE is set (raea makes no network requests)")) } else { Ok(()) }
}

#[cfg(test)]
mod offline_tests {
    // the only test that touches RAEA_OFFLINE and RAEA_ENV_FILE in this crate
    #[test]
    fn offline_is_read_from_the_environment_then_the_settings_file() {
        let file = std::env::temp_dir().join(format!("raea-offline-{}.env", std::process::id()));
        // SAFETY: no other test in this crate reads these
        unsafe {
            std::env::remove_var("RAEA_OFFLINE");
            std::env::set_var("RAEA_ENV_FILE", &file);
        }
        assert!(super::offline("x").is_ok(), "no file, no variable: online");
        std::fs::write(&file, "# RAEA_OFFLINE=1\nRAEA_OFFLINE=1\n").unwrap();
        let e = super::offline("the sao model").unwrap_err();
        assert!(e.starts_with("the sao model: not sent") && e.contains("RAEA_OFFLINE"), "{e}");
        unsafe { std::env::set_var("RAEA_OFFLINE", "0") };
        assert!(super::offline("x").is_ok(), "the environment's 0 wins over the file");
        unsafe { std::env::set_var("RAEA_OFFLINE", "1") };
        assert!(super::offline("x").is_err());
        unsafe {
            std::env::remove_var("RAEA_OFFLINE");
            std::env::remove_var("RAEA_ENV_FILE");
        }
        let _ = std::fs::remove_file(&file);
    }
}
