use super::*;

pub fn copy_dir(from: &Path, to: &Path, depth: usize) -> R<()> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for e in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "node_modules" || name == "target" { continue; }
        let p = e.path();
        if p.is_dir() {
            if depth < 6 { copy_dir(&p, &to.join(&name), depth + 1)?; }
        } else if matches!(p.extension().and_then(|x| x.to_str()), Some("ts" | "tsx" | "js" | "jsx" | "json" | "css")) {
            std::fs::copy(&p, to.join(&name)).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    Ok(())
}

/// The copy of an edit's folder without its own tsconfig.json and jsconfig.json files: the work folder's is the one a
/// bundle resolves by. A project's own (raea writes one into every new project, for its editor) is the nearest to its
/// sources, so the bundler read it in place of the work folder's: its `paths` name no live runtime, a played
/// composition's `react` was the engine's React, and `raea play`, the app's stage and every still drawn by playing
/// threw "__at is not a function" before the first frame (0.1.2; a render, which needs no such path, went through).
pub fn drop_tsconfigs(dir: &Path, depth: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth < 6 {
                drop_tsconfigs(&p, depth + 1);
            }
        } else if matches!(e.file_name().to_str(), Some("tsconfig.json" | "jsconfig.json")) {
            let _ = std::fs::remove_file(&p);
        }
    }
}

pub const MEDIA_EXTS: [&str; 10] = ["mp4", "mov", "m4v", "mp3", "wav", "m4a", "png", "jpg", "webp", "gif"];

/// The edit's source with its imports pointed at the bundle's copy of the SDK (its package name, a subpath such as /3d,
/// or a relative path into raea/motion/src), and media imported as modules turned into their paths (spec: "Media
/// imported as modules"): `import take from "./media/take01.mp4"` is `const take = "/abs/media/take01.mp4"`. Whole
/// statements, so an import over several lines is rewritten too. With `portable` a folder, imported media is copied
/// into its media/ and named `media/<file>`, so the bundle and that folder travel together (to the iPhone).
pub fn rewrite_into(src: &str, from_dir: &Path, motion_src: &Path, to_motion: &str, portable: Option<&Path>) -> R<String> {
    // `export ... from` too: a package re-exporting the umbrella (tone.tsx's sao and lyria) was left external
    let re = regex::Regex::new(r#"(?s)\b(?:(import)\s+([^;'"]*?)|(export)\s+((?:\*|\{|type\s*\{)[^;'"]*?))\s+from\s+(["'])([^"']+)["']"#).unwrap();
    let mut out = String::new();
    let mut last = 0;
    // only real statements: an import written inside a string, a template or a comment (a code chip's text) is text
    let text = text_spans(src);
    for c in re.captures_iter(src) {
        let whole = c.get(0).unwrap();
        if text.iter().any(|&(a, b)| a <= whole.start() && whole.start() < b) {
            continue;
        }
        let kw = c.get(1).or(c.get(3)).unwrap().as_str();
        let what = c.get(2).or(c.get(4)).unwrap().as_str().trim();
        let spec = c.get(6).unwrap().as_str();
        out.push_str(&src[last..whole.start()]);
        last = whole.end();
        let line = src[..whole.start()].matches('\n').count() + 1;
        let ext = Path::new(spec).extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
        let local = spec.starts_with('.') || spec.starts_with('/');
        if kw == "import" && MEDIA_EXTS.contains(&ext.as_str()) && local {
            // "/media/x" is the project's (its media/, then assets/: S5a), anything else beside the edit
            let path = if spec.starts_with("/media/") { raea_core_scene::media_path(from_dir, spec) } else { from_dir.join(spec) };
            if !path.exists() {
                return Err(format!("line {line}: {spec} doesn't exist (imported media resolves beside the edit)"));
            }
            let path = path.canonicalize().map_err(|e| e.to_string())?;
            match portable {
                Some(dir) => {
                    let name = path.file_name().unwrap().to_string_lossy().to_string();
                    std::fs::create_dir_all(dir.join("media")).map_err(|e| e.to_string())?;
                    std::fs::copy(&path, dir.join("media").join(&name)).map_err(|e| format!("{}: {e}", path.display()))?;
                    out.push_str(&format!("const {what} = {:?}", format!("media/{name}")));
                }
                None => out.push_str(&format!("const {what} = {:?}", path.display().to_string())),
            }
            continue;
        }
        // the SDK by its names (docs/sdk.feature S1, S1d): "raea" is its prelude, "raea/<module>" a module, "@raea/<package>"
        // a package, each where sdk/tsconfig.base.json's paths put it, and so at its copy (bundle_fresh): motion/ for
        // sdk/raea/src's files, <package>/ beside it for a package's
        let named = spec == "raea" || spec.starts_with("raea/") || spec.starts_with("@raea/");
        let sdk = motion_src.parent().and_then(|m| m.parent());
        let target = if let Some(f) = sdk.filter(|_| named).and_then(|sdk| raea_io_home::sdk::resolve(sdk, spec)) {
            sdk.and_then(|sdk| copied(&f, sdk, to_motion))
        } else if local {
            let abs = normalize(&from_dir.join(spec));
            abs.strip_prefix(motion_src).ok().map(|rest| if rest.as_os_str().is_empty() { to_motion.to_string() } else { format!("{to_motion}/{}", rest.display()) })
        } else {
            None
        };
        match target {
            Some(t) => out.push_str(&format!("{kw} {what} from \"{t}\"")),
            None => out.push_str(whole.as_str()),
        }
    }
    out.push_str(&src[last..]);
    Ok(out)
}

/// An SDK file `f` (under `sdk`) as its copy's import path from a file whose way to motion/ is `to_motion`:
/// sdk/raea/src/x.ts is `<to_motion>/x`, sdk/<package>/src/x.ts `<package>/x` beside it
fn copied(f: &Path, sdk: &Path, to_motion: &str) -> Option<String> {
    let rel = f.strip_prefix(sdk).ok()?.with_extension("");
    let mut parts = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned());
    let (pkg, _src) = (parts.next()?, parts.next()?);
    let rest = parts.collect::<Vec<_>>().join("/");
    Some(if pkg == "raea" { format!("{to_motion}/{rest}") } else { format!("{}{pkg}/{rest}", to_motion.strip_suffix("motion").unwrap_or(to_motion)) })
}

/// The packages the SDK's code moved into, that a bundle may import (docs/sdk.feature S1d), as sdk/tsconfig.base.json
/// lists them: each (`@raea/<name>`, its sources from raea's home, sdk/<name>/src) is copied into the work folder
/// beside motion/ as its package's folder name (app/, math/ ...), its own imports of the SDK pointed at that one copy,
/// so a controller's hooks read the Clock the engine drives.
pub fn sdks() -> Vec<(String, String)> {
    raea_io_home::sdk::packages(&raea_io_home::home().join("sdk")).into_iter().map(|(name, dir)| (name, format!("sdk/{dir}"))).collect()
}

/// The script sources under a copied folder, as paths relative to it (a file in a subfolder too)
pub fn sources(root: &Path) -> R<Vec<PathBuf>> {
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))?.flatten() {
            // what copy_dir leaves out is no source of the edit's (a project's .git and node_modules were walked whole:
            // 2 s before a render's first frame in a project of ten thousand files)
            let name = e.file_name();
            if name.to_string_lossy().starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            let p = e.path();
            // the entry's own kind, without another stat; a link is followed
            let dir = match e.file_type() {
                Ok(t) if !t.is_symlink() => t.is_dir(),
                _ => p.is_dir(),
            };
            if dir {
                stack.push(p);
            } else if matches!(p.extension().and_then(|x| x.to_str()), Some("ts" | "tsx" | "js" | "jsx" | "mjs")) {
                out.push(p.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    Ok(out)
}

/// A file of motion's own copy with its imports of the packages its code moved into (`sdks`) pointed at their copies
/// beside it, `up` being the way from its folder to the copy's root (motion/x.ts imports "@raea/app/clock":
/// "../app/clock"; motion/3d/x.ts, "../../app/clock"). A module listed alone ("@raea/ui/canvas") is pointed too.
pub fn point_sdks(src: &str, up: &str, sdks: &[(String, String)]) -> String {
    let re = regex::Regex::new(r#"(\bfrom\s+)(["'])(@raea/[a-z]+)((?:/[^"']*)?)["']"#).unwrap();
    re.replace_all(src, |c: &regex::Captures| match sdks.iter().find(|(name, _)| *name == &c[3] || name.strip_prefix(&c[3]) == Some(&c[4])) {
        Some((_, dir)) => format!("{}\"{up}{}{}\"", &c[1], dir.rsplit('/').nth(1).unwrap_or(dir), &c[4]),
        None => c[0].to_string(),
    })
    .into_owned()
}

pub fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            o => out.push(o),
        }
    }
    out
}

/// The byte ranges of `src` that are text, not code: string literals, a template's text (not its `${}`), comments and
/// regular expression literals. A lexer's pass, enough to tell an import statement from one written in a string.
pub(crate) fn text_spans(src: &str) -> Vec<(usize, usize)> {
    let b = src.as_bytes();
    let (mut spans, mut i, mut braces) = (Vec::new(), 0, Vec::<usize>::new());
    // whether a `/` here starts a regular expression: after an operator or opening punctuation, or at the start
    let regex_ok = |i: usize| b[..i].iter().rev().find(|c| !c.is_ascii_whitespace()).is_none_or(|c| b"(,=:[!&|?{};+-*%<>~^".contains(c));
    // a template's text from `at` up to its end or a `${`; returns where the code resumes
    let template = |mut i: usize, spans: &mut Vec<(usize, usize)>| -> (usize, bool) {
        let at = i;
        while i < b.len() && b[i] != b'`' && !(b[i] == b'$' && b.get(i + 1) == Some(&b'{')) {
            i += if b[i] == b'\\' { 2 } else { 1 };
        }
        spans.push((at, i.min(b.len())));
        if i < b.len() && b[i] == b'$' { (i + 2, true) } else { (i + 1, false) }
    };
    while i < b.len() {
        match b[i] {
            b'\'' | b'"' => {
                let (q, at) = (b[i], i);
                i += 1;
                while i < b.len() && b[i] != q && b[i] != b'\n' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
                spans.push((at, i.min(b.len())));
            }
            b'`' => {
                let (next, open) = template(i + 1, &mut spans);
                if open { braces.push(0) }
                i = next;
            }
            b'{' if !braces.is_empty() => { *braces.last_mut().unwrap() += 1; i += 1; }
            b'}' if braces.last() == Some(&0) => {
                braces.pop();
                let (next, open) = template(i + 1, &mut spans);
                if open { braces.push(0) }
                i = next;
            }
            b'}' if !braces.is_empty() => { *braces.last_mut().unwrap() -= 1; i += 1; }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let at = i;
                while i < b.len() && b[i] != b'\n' { i += 1 }
                spans.push((at, i));
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let at = i;
                i = src[i + 2..].find("*/").map_or(b.len(), |e| i + 2 + e + 2);
                spans.push((at, i));
            }
            b'/' if regex_ok(i) => {
                let (at, mut class) = (i, false);
                i += 1;
                while i < b.len() && b[i] != b'\n' && (class || b[i] != b'/') {
                    if b[i] == b'[' { class = true } else if b[i] == b']' { class = false }
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
                spans.push((at, i.min(b.len())));
            }
            _ => i += 1,
        }
    }
    spans
}
