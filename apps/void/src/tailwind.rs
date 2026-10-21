//! A raea project's CSS in process (docs/vite-in-raea.md A5): the brand kit's variables (SPACE/brand/brand.css, its
//! @imports dropped: Blitz loads no remote CSS), then Tailwind 4.3.3 for every class any .ts or .tsx in the project
//! uses, with the brand theme (apps/cli/template/theme.css: class names onto --brand-* variables). Tailwind's compiler runs on
//! raea's JavaScriptCore (raea-native's js-exec) instead of a bun process: Rust finds the candidates and reads Tailwind's
//! stylesheets, the compiler gets them as text. What tw/project.ts made, byte for byte.
use anyhow::{anyhow, Context};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// tailwindcss's package (tw/node_modules: `bun install` in apps/void/tw)
fn package() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tw/node_modules/tailwindcss")
}

/// The compile, as tw/project.ts ran it: `@import "tailwindcss"` and the theme, Tailwind's own stylesheets from the
/// package, the candidates built
const COMPILE: &str = r#"import { compile } from "./dist/lib.mjs";
const { candidates, theme, files } = JSON.parse(process.stdin.text);
const c = await compile(`@import "tailwindcss";\n${theme}`, {
  base: "/",
  loadStylesheet: async (id) => {
    const name = id === "tailwindcss" ? "index.css" : id.replace(/^tailwindcss\//, "");
    const file = name.endsWith(".css") ? name : name + ".css";
    if (!(file in files)) throw new Error("tailwind: no stylesheet " + id);
    return { path: file, base: "/", content: files[file] };
  },
});
process.stdout.write(c.build(candidates));
"#;

/// The project's CSS: its brand kit's variables, then its Tailwind.
pub fn css(space: &Path, theme: &Path) -> anyhow::Result<String> {
    let pkg = package();
    let mut files = BTreeMap::new();
    for e in std::fs::read_dir(&pkg).with_context(|| format!("{} (bun install in apps/void/tw)", pkg.display()))?.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "css") {
            files.insert(p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read_to_string(&p)?);
        }
    }
    let theme = std::fs::read_to_string(theme).with_context(|| theme.display().to_string())?;
    let input = json!({ "candidates": candidates(space), "theme": theme, "files": files }).to_string();
    let entry = raea_native::js::Entry::Code { text: COMPILE.into(), ts: false, dir: pkg.clone() };
    let run = raea_native::js::run(&pkg, &pkg, entry, Vec::new(), Default::default(), input, Vec::new(), std::time::Duration::from_secs(60));
    if run.exit != 0 {
        return Err(anyhow!("tailwind: {}", run.stderr.chars().take(400).collect::<String>()));
    }
    let brand_file = space.join("brand/brand.css");
    let brand = match std::fs::read_to_string(&brand_file) {
        Ok(b) => b.split('\n').filter(|l| !l.trim_start().starts_with("@import")).collect::<Vec<_>>().join("\n"),
        Err(_) => String::new(),
    };
    Ok(format!("{brand}\n{}", run.stdout))
}

/// Every whitespace-separated word inside a quoted string ('…', "…" or `…`, backslash escapes skipped) of every .ts and
/// .tsx under `space` (not dot folders, node_modules or media), in the order project.ts collected them (a Set's: first
/// sighting), files in its walk order
fn candidates(space: &Path) -> Vec<String> {
    fn walk(d: &Path, out: &mut Vec<PathBuf>) {
        // readdirSync's order: names sorted
        let mut names: Vec<_> = std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.file_name()).collect();
        names.sort();
        for name in names {
            let n = name.to_string_lossy();
            if n.starts_with('.') || n == "node_modules" || n == "media" {
                continue;
            }
            let p = d.join(&name);
            if p.is_dir() {
                walk(&p, out);
            } else if n.ends_with(".tsx") || n.ends_with(".ts") {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(space, &mut files);
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for f in files {
        let Ok(src) = std::fs::read_to_string(&f) else { continue };
        for s in quoted(&src) {
            for c in s.split(|c: char| c.is_whitespace()) {
                if !c.is_empty() && seen.insert(c.to_string()) {
                    out.push(c.to_string());
                }
            }
        }
    }
    out
}

/// The strings project.ts's /(["'`])((?:(?!\1)[^\\]|\\.)*)\1/g matched: from a quote to the next unescaped same
/// quote; a quote with no closing one is passed over
fn quoted(src: &str) -> Vec<&str> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let q = b[i];
        if q == b'"' || q == b'\'' || q == b'`' {
            let mut j = i + 1;
            let mut closed = None;
            while j < b.len() {
                if b[j] == b'\\' {
                    // `\\.`: an escape takes the next character, whatever it is (a newline or a quote included); a
                    // trailing backslash ends the match attempt
                    if j + 1 >= b.len() {
                        break;
                    }
                    j += 1 + utf8_len(b[j + 1]);
                    continue;
                }
                if b[j] == q {
                    closed = Some(j);
                    break;
                }
                j += utf8_len(b[j]);
            }
            if let Some(end) = closed {
                out.push(&src[i + 1..end]);
                i = end + 1;
                continue;
            }
        }
        i += utf8_len(b[i]).max(1);
    }
    out
}

fn utf8_len(lead: u8) -> usize {
    match lead {
        0xf0..=0xff => 4,
        0xe0..=0xef => 3,
        0xc0..=0xdf => 2,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// the same CSS tw/project.ts makes with bun, for real projects (skipped where bun isn't installed)
    #[test]
    fn same_css_as_bun() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        if !package().is_dir() {
            eprintln!("same_css_as_bun: skipped, no {} (bun install in apps/void/tw)", package().display());
            return;
        }
        let theme = here.join("../cli/template/theme.css");
        for space in ["../cli/template", "../../sdk/react/bench/timelines/titles", "../../sdk/react/bench/games/pong"] {
            let space = here.join(space);
            let Ok(bun) = std::process::Command::new("bun").arg(here.join("tw/project.ts")).arg(&space).arg(&theme).output() else { return };
            assert!(bun.status.success(), "bun: {}", String::from_utf8_lossy(&bun.stderr));
            let ours = css(&space, &theme).unwrap();
            assert!(ours == String::from_utf8_lossy(&bun.stdout), "{}: differs from bun's ({} vs {} bytes)", space.display(), ours.len(), bun.stdout.len());
        }
    }

    #[test]
    fn finds_quoted_words_as_project_ts() {
        assert_eq!(quoted(r#"a "b c" 'd' `e ${f}` "g\"h" 'unclosed"#), vec!["b c", "d", "e ${f}", "g\\\"h"]);
    }
}
