//! A composition's TSX made into one script the engine runs: the edit's folder, raea's SDK packages and the media it
//! imports copied into a work folder and rewritten (rewrite.rs), the entry that steps its Clock (entry.rs), bundled by
//! raea-bundle's library (fresh.rs) with what it imports from outside its folder (outside.rs), kept by what it was made
//! from (cache.rs), with its styles.gen.ts written first (styles.rs). Also the options a render takes for it
//! (`CompOpts`) and the CSS a page gets beside it.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

mod cache;
mod entry;
mod fresh;
mod outside;
mod pid;
mod rewrite;
pub mod styles;
mod urls;
pub use cache::*;
pub use entry::*;
pub use fresh::*;
pub use outside::*;
pub use rewrite::*;
pub use urls::*;
pub use raea_core_scene::media_path;
#[cfg(test)]
mod tests;

pub type R<T> = Result<T, String>;

pub fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

/// The space's brand kit for a composition `w` px wide: the nearest brand/brand.css at or above the edit's folder (up
/// to four levels), its @import lines dropped (fonts load by name), then `:root { --brand-scale: 2.5 * w / 1080 }`.
pub fn brand_css(dir: &Path, w: u32) -> Option<String> {
    let mut d = Some(dir);
    for _ in 0..5 {
        let here = d?;
        let f = here.join("brand").join("brand.css");
        if let Ok(css) = std::fs::read_to_string(&f) {
            let kept: Vec<&str> = css.lines().filter(|l| !l.trim_start().starts_with("@import")).collect();
            // its fonts and images as files: /media/ from the edit, the rest from brand/ (urls.rs)
            let kept = css_file_urls(&kept.join("\n"), dir, &here.join("brand"), |_| ());
            return Some(format!("{kept}\n:root {{ --brand-scale: {}; }}\n", 2.5 * w as f64 / 1080.0));
        }
        d = here.parent();
    }
    None
}

/// The CSS a bundle's modules imported (`import "./x.css"`, CSS modules: raea-bundle, A2), carried in the bundle's
/// last `//# raea-css=` line as a JSON string
pub fn bundle_css(js: &str) -> Option<String> {
    let line = js.lines().rev().take(4).find_map(|l| l.strip_prefix("//# raea-css="))?;
    serde_json::from_str::<String>(line).ok().filter(|c| !c.is_empty())
}

/// A page's own CSS: the space's brand kit (brand_css), then the CSS the bundle `js` imported (bundle_css)
pub fn page_css(dir: &Path, w: u32, js: &Path) -> Option<String> {
    // its url()s as files from the edit's folder (urls.rs), where the CSS files it imports are found
    let bundled = std::fs::read_to_string(js).ok().and_then(|t| bundle_css(&t)).map(|c| css_file_urls(&c, dir, dir, |_| ()));
    match (brand_css(dir, w), bundled) {
        (Some(b), Some(c)) => Some(format!("{b}\n{c}")),
        (b, c) => b.or(c),
    }
}

/// A `\u` escape in a JSX attribute's plain string (`unit="\u00a0pts"`) is no escape there: the page shows the six
/// characters. Said once per bundle, with the fix (braces: `unit={"\u00a0pts"}`), for the edit's .tsx files.
pub fn jsx_escape_lint(dir: &Path) {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r#"(?:^|[\s<])([A-Za-z_][\w-]*)="([^"\n]*\\u[0-9A-Fa-f{][^"\n]*)""#).unwrap());
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    files.extend(std::fs::read_dir(dir.join("components")).into_iter().flatten().flatten().map(|e| e.path()));
    for f in files.iter().filter(|f| f.extension().is_some_and(|x| x == "tsx")) {
        let Ok(text) = std::fs::read_to_string(f) else { continue };
        for (n, line) in text.lines().enumerate() {
            for c in re.captures_iter(line) {
                eprintln!(
                    "raea: {}:{}: {}=\"{}\" is a JSX attribute's plain string, where \\u isn't an escape: it shows those characters. Put the string in braces: {}={{\"{}\"}}",
                    f.file_name().unwrap_or_default().to_string_lossy(), n + 1, &c[1], &c[2], &c[1], &c[2]
                );
            }
        }
    }
}

/// Whether the file at `p` (a project's words.json) is a transcript a render takes, saying why not when it isn't: live
/// play, the bake and export-game pass one only then, as render does
pub fn transcript_file(p: &Path) -> bool {
    let ok = std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).map(|v| words_shape(&v));
    match ok {
        Some(Ok(())) => true,
        Some(Err(e)) => {
            eprintln!("raea: {} isn't left in: {e} (pass --words FILE for a transcript elsewhere)", p.display());
            false
        }
        None => {
            eprintln!("raea: {} isn't left in: it isn't JSON", p.display());
            false
        }
    }
}

/// Whether a words file is a transcript the Clock reads: an array of words, each `{text|word, from, to}` (frames) or
/// `{text|word, start, end}` / `{start_ms, end_ms}` (seconds, milliseconds)
pub fn words_shape(v: &Value) -> R<()> {
    let expected = "expected an array of words, each {\"text\", \"start\", \"end\"} in seconds (or \"from\"/\"to\" in frames, or \"start_ms\"/\"end_ms\")";
    let Some(list) = v.as_array() else {
        return Err(format!("it isn't a transcript ({expected}; this one is a JSON {})", match v { Value::Object(_) => "object", Value::String(_) => "string", _ => "value" }));
    };
    for (i, w) in list.iter().enumerate() {
        let has = |k: &str| w.get(k).is_some_and(Value::is_number);
        let text = w.get("text").or_else(|| w.get("word")).is_some_and(Value::is_string);
        if !text || !((has("from") && has("to")) || (has("start") && has("end")) || (has("start_ms") && has("end_ms"))) {
            return Err(format!("word {i} isn't one ({expected}): {}", w.to_string().chars().take(120).collect::<String>()));
        }
    }
    Ok(())
}

/// How a composition is picked and, for a bare default export, sized (`--composition ID`, `--w --h --fps --frames`).
pub struct CompOpts {
    pub composition: Option<String>,
    pub w: u32,
    pub h: u32,
    pub fps: f64,
    pub frames: i64,
    /// the Clock's words, perception and input (`--words`, `--perception`, `--input`), as JSON for the entry
    pub words: Option<Value>,
    pub perception: Option<Value>,
    pub input: Option<Value>,
    /// live play's runtime instead of React (sdk/react/js/lite: each frame rendered and only what changed sent, no
    /// reconciler): what a game plays on; renders keep React
    pub lite: bool,
    /// with `lite`: each frame's marker says which components rendered (`--comps`; bake.rs)
    pub comps: bool,
}

/// A JSON file's value.
pub fn json_file(path: &str) -> R<Value> {
    let t = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    serde_json::from_str(&t).map_err(|e| format!("{path}: {e}"))
}

/// The DOM key a recorded key's name stands for (inputs.rrl names arrows `left`, `right`, ...).
pub fn dom_key(name: &str) -> String {
    match name {
        "left" | "right" | "up" | "down" => format!("Arrow{}{}", name[..1].to_uppercase(), &name[1..]),
        "space" => " ".into(),
        "enter" | "return" => "Enter".into(),
        "escape" | "esc" => "Escape".into(),
        _ => name.into(),
    }
}

/// `--input FILE`: a JSON input timeline ({frame: {keys, pointer}}), or rrl's recorded input (inputs.rrl: `key NAME
/// {down: {F: 0|1, ...} step}`) as {"steps": {DOMKEY: {F: 0|1}}}, which the entry expands frame by frame.
pub fn input_file(path: &str) -> R<Value> {
    if !path.ends_with(".rrl") {
        return json_file(path);
    }
    let t = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut steps = Map::new();
    for line in t.lines().map(str::trim).filter(|l| l.starts_with("key ")) {
        let name = line[4..].split_whitespace().next().unwrap_or("");
        let (Some(a), Some(b)) = (line.find("down:").and_then(|i| line[i..].find('{').map(|j| i + j)), line.rfind('}')) else { continue };
        let body = &line[a + 1..];
        let body = &body[..body.find('}').unwrap_or(b - a - 1)];
        let mut m = Map::new();
        for kv in body.split(',') {
            if let Some((k, v)) = kv.split_once(':') {
                if let (Ok(k), Ok(v)) = (k.trim().parse::<i64>(), v.trim().parse::<f64>()) {
                    m.insert(k.to_string(), json!(v));
                }
            }
        }
        steps.insert(dom_key(name), Value::Object(m));
    }
    if steps.is_empty() {
        return Err(format!("{path}: no `key NAME {{down: {{...}}}}` lines"));
    }
    Ok(json!({ "steps": steps }))
}

pub fn comp_opts(args: &[String]) -> R<CompOpts> {
    let n = |k: &str| -> R<Option<f64>> { flag(args, k).map(|v| v.parse::<f64>().map_err(|_| format!("{k} takes a number"))).transpose() };
    Ok(CompOpts {
        composition: flag(args, "--composition"),
        w: n("--w")?.unwrap_or(1080.0) as u32,
        h: n("--h")?.unwrap_or(1920.0) as u32,
        fps: n("--fps")?.unwrap_or(30.0),
        frames: n("--frames")?.unwrap_or(60.0) as i64,
        words: flag(args, "--words").map(|p| json_file(&p).and_then(|v| words_shape(&v).map(|_| v).map_err(|e| format!("--words {p}: {e}")))).transpose()?,
        perception: flag(args, "--perception").map(|p| json_file(&p)).transpose()?,
        input: flag(args, "--input").map(|p| input_file(&p)).transpose()?,
        lite: args.iter().any(|a| a == "--lite"),
        comps: args.iter().any(|a| a == "--comps"),
    })
}

/// A composition bundled for the engine (raea-bundle's library: Rolldown, on the Mac): the edit's folder and the SDK beside the engine's
/// node_modules (one React), behind the entry that steps its Clock. With `portable` a folder, imported media is copied
/// into its media/ and the bundle names it relatively, so the two travel together (to the iPhone).
pub fn bundle_composition(tsx: &Path, out_js: &Path, o: &CompOpts, portable: Option<&Path>) -> R<()> {
    let dir = tsx.parent().unwrap().to_path_buf();
    // `import { css } from "./styles.gen"`: written (or brought up to date) before the bundler looks for it
    styles::ensure(&dir)?;
    jsx_escape_lint(&dir);
    // a bundle made from these same inputs before is copied instead of bundled again (lightspeed F5): the copying and
    // bundling on every open, render and play. Not a portable one (it copies media beside it)
    let cached = if portable.is_none() { bundle_cache(tsx, o) } else { None };
    if let Some(c) = &cached {
        if std::fs::copy(c, out_js).is_ok() {
            let _ = std::fs::File::options().append(true).open(c).and_then(|f| f.set_modified(std::time::SystemTime::now()));
            return Ok(());
        }
    }
    bundle_fresh(tsx, out_js, o, portable)?;
    if let Some(c) = cached {
        // written whole, then moved in
        let tmp = c.with_extension(format!("{}.tmp", std::process::id()));
        if c.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) && std::fs::copy(out_js, &tmp).is_ok() && std::fs::rename(&tmp, &c).is_ok() {
            if let Some(d) = c.parent() {
                prune_cache(d, "js", 64);
            }
        } else {
            let _ = std::fs::remove_file(&tmp);
        }
    }
    Ok(())
}
