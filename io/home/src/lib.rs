//! Where raea's own files are, for the crates that read them (the SDK's TypeScript, the tools that install the local
//! models, the fonts), and where its per-user caches and logs go on each platform.

pub mod sdk;
pub mod shared;

/// What a folder takes on disk, as `du -sh` says it (`links`: through a link to its files). Windows has no du (raea
/// setup-audio said "(?)" for every model there): its files' sizes are added up instead, in du's units.
pub fn disk_use(dir: &std::path::Path, links: bool) -> String {
    let o = std::process::Command::new("du").arg(if links { "-shL" } else { "-sh" }).arg(dir).output();
    o.ok().and_then(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().next().map(str::to_string)).unwrap_or_else(|| du_units(bytes_in(dir, links, 0)))
}

/// The bytes of the files under `p` (through links when `links`, 64 folders deep at most, so a link's loop ends).
fn bytes_in(p: &std::path::Path, links: bool, depth: u32) -> u64 {
    let md = if links { std::fs::metadata(p) } else { std::fs::symlink_metadata(p) };
    match md {
        Ok(m) if m.is_dir() && depth < 64 => std::fs::read_dir(p).map(|d| d.flatten().map(|e| bytes_in(&e.path(), links, depth + 1)).sum()).unwrap_or(0),
        Ok(m) if m.is_file() => m.len(),
        _ => 0,
    }
}

/// `n` bytes as `du -h` writes them: 1024s, one decimal under 10 ("1.9G", "512M", "0B").
fn du_units(n: u64) -> String {
    let mut v = n as f64;
    let mut unit = 0;
    while v >= 1024.0 && unit < 4 {
        v /= 1024.0;
        unit += 1;
    }
    let u = ["B", "K", "M", "G", "T"][unit];
    if unit == 0 { format!("{n}B") } else if v < 10.0 { format!("{v:.1}{u}") } else { format!("{v:.0}{u}") }
}

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
        let installed = std::env::current_exe().ok().and_then(|e| dunce::canonicalize(&e).ok()).map(plain).and_then(|e| e.parent()?.parent().map(|p| p.join("share/raea")));
        if let Some(d) = installed.filter(|d| d.join("sdk/raea/src").is_dir()) {
            return d;
        }
        if std::env::var_os("RAEA_NO_TREE").is_some() {
            eprintln!("raea: no share/raea beside {} and RAEA_NO_TREE is set: its own files can't be found", std::env::current_exe().map(|e| e.display().to_string()).unwrap_or_default());
            return std::path::PathBuf::from("/nonexistent/raea");
        }
        let tree = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        dunce::canonicalize(&tree).map(plain).unwrap_or(tree)
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

/// `p` canonical (links resolved, and on Windows plain C:\..., through dunce), or `p` as given when it can't be read
/// (it doesn't exist yet).
pub fn real(p: std::path::PathBuf) -> std::path::PathBuf {
    dunce::canonicalize(&p).unwrap_or(p)
}

/// a verbatim drive path's plain form (`\\?\C:\x` is `C:\x`); None for any other path
fn drive(s: &str) -> Option<&str> {
    s.strip_prefix(r"\\?\").filter(|r| r.as_bytes().get(1) == Some(&b':'))
}

/// The person's home folder: $HOME, else %USERPROFILE%, the order Git for Windows reads them in. Windows gives an app
/// opened from the Start menu or Explorer no HOME, so a read of HOME alone found nothing there and put ~/.raea, the
/// caches and the inbox under whatever folder raea ran in. None when neither is set (an empty one counts as unset).
/// Every read of the home folder goes through here (tools/windows-paths.sh).
pub fn user_home() -> Option<std::path::PathBuf> {
    ["HOME", "USERPROFILE"].into_iter().find_map(|k| std::env::var_os(k).filter(|v| !v.is_empty())).map(std::path::PathBuf::from)
}

/// Where raea's models are kept (Beat This!, Demucs, TransNet, Silero, the Core ML ones): ~/.local/share/raea-models on
/// the Mac and Linux, %LOCALAPPDATA%\raea\models on Windows, which has no HOME (#43). Relative when neither is set.
pub fn models() -> std::path::PathBuf {
    use std::path::PathBuf;
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    match var("LOCALAPPDATA").filter(|_| cfg!(windows)) {
        Some(d) => d.join("raea").join("models"),
        None => user_home().unwrap_or_default().join(".local/share/raea-models"),
    }
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

/// A new temp file named `raea-NAME-PID-N`, open to read and write, gone when raea is: unlinked once open on unix,
/// delete-on-close on Windows, so a render that dies leaves nothing behind (an encoder's samples, a render's parts).
pub fn scratch_file(name: &str) -> Result<std::fs::File, String> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!("raea-{name}-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let mut o = std::fs::OpenOptions::new();
    o.read(true).write(true).create_new(true);
    #[cfg(windows)]
    std::os::windows::fs::OpenOptionsExt::custom_flags(&mut o, 0x0400_0000); // FILE_FLAG_DELETE_ON_CLOSE
    let f = o.open(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    #[cfg(unix)]
    let _ = std::fs::remove_file(&p);
    Ok(f)
}

/// macOS's folder under HOME, the XDG base folder (its variable, else its default under HOME) with raea in it, or raea's
/// folder of that kind under %LOCALAPPDATA%. An XDG variable that isn't an absolute path is ignored, as the spec says.
fn user_dir(apple: &str, xdg: &str, xdg_default: &str, windows: &str) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA").filter(|d| !d.is_empty()).map(|d| PathBuf::from(d).join("raea").join(windows));
    }
    let home = user_home()?;
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
        None => user_home().unwrap_or_default().join(under_home),
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

/// The import path from a file in folder `from` to the file `to` (both relative to one folder): ./x, ../y/z.
pub fn import_path(from: &std::path::Path, to: &std::path::Path) -> String {
    let (a, b): (Vec<_>, Vec<_>) = (from.components().collect(), to.components().collect());
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut parts = vec![".."; a.len() - common].into_iter().map(String::from).collect::<Vec<_>>();
    parts.extend(b[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    let p = parts.join("/");
    if p.starts_with("..") { p } else { format!("./{p}") }
}

/// A local file as a page names it in a URL's path: its path on Unix; on Windows `/C:/x/y` (the path of the file URL
/// file:///C:/x/y, `src`'s, which a page's file:/// base joins it to). Written `C:\x`, a URL reads `c` as its scheme
/// and CSS reads the backslashes as escapes (#40, #41, #42).
pub fn url_path(p: &std::path::Path) -> String {
    let s = src(p);
    s.strip_prefix("file://").map_or_else(|| s.clone(), str::to_string)
}

/// A path a page named (`url_path`'s `/C:/x`, or any other) as the file it is
pub fn from_url_path(s: &str) -> std::path::PathBuf {
    from_url_path_on(s, cfg!(windows)).into()
}

/// `from_url_path` as Windows reads it (`windows`) or as Unix does
pub fn from_url_path_on(s: &str, windows: bool) -> &str {
    match s.strip_prefix('/') {
        Some(r) if windows && r.as_bytes().get(1) == Some(&b':') => r,
        _ => s,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_folder_is_sized_without_du() {
        assert_eq!(super::du_units(0), "0B");
        assert_eq!(super::du_units(1023), "1023B");
        assert_eq!(super::du_units(1536), "1.5K");
        assert_eq!(super::du_units(512 << 20), "512M");
        assert_eq!(super::du_units((19 << 30) / 10), "1.9G");
        let d = std::env::temp_dir().join(format!("raea-du-{}", std::process::id()));
        std::fs::create_dir_all(d.join("a/b")).unwrap();
        std::fs::write(d.join("x"), vec![0u8; 1000]).unwrap();
        std::fs::write(d.join("a/b/y"), vec![0u8; 2500]).unwrap();
        assert_eq!(super::bytes_in(&d, false, 0), 3500);
        assert_eq!(super::bytes_in(&d, true, 0), 3500);
        assert_eq!(super::bytes_in(&d.join("none"), false, 0), 0);
        assert_ne!(super::disk_use(&d, false), "?");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_src_is_a_file_url_on_windows() {
        assert_eq!(super::src_on("/Users/a/assets/m.png", false), "/Users/a/assets/m.png");
        assert_eq!(super::src_on(r"C:\Users\a\assets/m.png", true), "file:///C:/Users/a/assets/m.png");
        assert_eq!(super::src_on(r"\\?\C:\a\m.png", true), "file:///C:/a/m.png");
        assert_eq!(super::src_on("assets/m.png", true), "assets/m.png");
    }

    #[test]
    fn a_page_names_a_windows_file_by_its_url_path() {
        use super::from_url_path_on;
        // url_path is src's file URL without its scheme
        let url_path_on = |s: &str, w: bool| super::src_on(s, w).trim_start_matches("file://").to_string();
        assert_eq!(url_path_on("/Users/a/page", false), "/Users/a/page");
        assert_eq!(url_path_on(r"D:\work\page", true), "/D:/work/page");
        assert_eq!(url_path_on(r"\\?\D:\06.AI\banana-hf5", true), "/D:/06.AI/banana-hf5");
        assert_eq!(url_path_on(r"C:\Users\a\AppData\Local\raea\cache\f.woff2", true), "/C:/Users/a/AppData/Local/raea/cache/f.woff2");
        assert_eq!(from_url_path_on("/D:/work/page/assets/hero.png", true), "D:/work/page/assets/hero.png");
        assert_eq!(from_url_path_on("/D:/x", false), "/D:/x");
        assert_eq!(from_url_path_on("assets/hero.png", true), "assets/hero.png");
        // joined to a page's file:/// base as Blitz joins it, a drive's path is a file URL, not scheme `d` (#41)
        for p in [r"D:\work\page", r"\\?\D:\work\page"] {
            let u = url::Url::parse("file:///").unwrap().join(&format!("{}/assets/hero.png", url_path_on(p, true))).unwrap();
            assert_eq!((u.scheme(), u.path()), ("file", "/D:/work/page/assets/hero.png"));
        }
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
    fn caches_logs_and_the_home_folder_follow_the_platform() {
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
        // the home folder: HOME, else USERPROFILE (an app opened from Windows' Start menu has no HOME); empty is unset
        let was = std::env::var_os("USERPROFILE");
        unsafe { std::env::set_var("USERPROFILE", r"C:\Users\Ada") };
        assert_eq!(super::user_home().unwrap().to_str(), Some("/h"));
        unsafe { std::env::set_var("HOME", "") };
        assert_eq!(super::user_home().unwrap().to_str(), Some(r"C:\Users\Ada"));
        assert_eq!(super::env_path("RAEA_NO_SUCH_VAR", ".raea/env"), std::path::Path::new(r"C:\Users\Ada").join(".raea/env"));
        unsafe { std::env::remove_var("USERPROFILE") };
        assert_eq!(super::user_home(), None);
        unsafe {
            std::env::set_var("HOME", "/h");
            if let Some(w) = was {
                std::env::set_var("USERPROFILE", w);
            }
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
            let file = std::env::var_os("RAEA_ENV_FILE").map(std::path::PathBuf::from).or_else(|| user_home().map(|h| h.join(".raea/env")));
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

/// Whether the environment variable `name` is set to exactly "1" (raea's switches: RAEA_SOURCE_STAMPS=1,
/// RAEA_PLAYER_PROBE=1).
pub fn env_on(name: &str) -> bool {
    std::env::var(name).as_deref() == Ok("1")
}
