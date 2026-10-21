//! raea's own TypeScript tools on JavaScriptCore (js::run_node) give what they gave under bun: each run both ways on
//! the same input, the output compared byte for byte. Skipped where bun isn't installed.
#![cfg(feature = "modules")]
use std::path::{Path, PathBuf};
use std::time::Duration;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn bun(script: &Path, args: &[&str]) -> Option<(i32, String)> {
    let o = std::process::Command::new("bun").arg(script).args(args).output().ok()?;
    Some((o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into_owned()))
}

fn ours(script: &Path, args: &[&str]) -> (i32, String, String) {
    let r = raea_native::js::run_node(script, &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(), String::new(), Duration::from_secs(120));
    (r.exit, r.stdout, r.stderr)
}

/// @raea/data/src/props.ts: components' props through the TypeScript 5 compiler API
#[test]
fn props_as_under_bun() {
    let space = std::env::temp_dir().join(format!("raea-tools-props-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&space);
    let w = |p: &str, s: &str| {
        let f = space.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    };
    w("components/Card.tsx", "export default function Card(p: { title: string; tone?: \"warm\" | \"cool\"; count: number; on?: boolean; tags: string[]; meta?: { a: number } }) { return null; }\n");
    w("components/Lower.tsx", "type P = { name: string; align: \"left\" | \"right\" | \"center\" };\nconst Lower = ({ name, align }: P) => null;\nexport default Lower;\n");
    w("components/Bare.tsx", "export default function Bare() { return null; }\n");
    w("components/Card_tw.tsx", "export default function X(p: { z: string }) { return null; }\n");
    // @raea/data where its packages are installed (RAEA_DATA, as brief and void find it; a worktree has no node_modules)
    let data = std::env::var_os("RAEA_DATA").map(PathBuf::from).unwrap_or_else(|| repo().join("sdk/data"));
    if !data.join("node_modules/typescript5").is_dir() {
        eprintln!("props_as_under_bun: skipped, no {} (bun install in data/, or RAEA_DATA)", data.join("node_modules").display());
        return;
    }
    let script = data.join("src/props.ts");
    let Some((bun_exit, bun_out)) = bun(&script, &[space.to_str().unwrap()]) else { return };
    let _ = std::fs::remove_dir_all(space.join(".score"));
    let (exit, out, err) = ours(&script, &[space.to_str().unwrap()]);
    let cached = std::fs::read_to_string(space.join(".score/props.json")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&space);
    assert_eq!(bun_exit, 0);
    assert_eq!(exit, 0, "stderr: {err}");
    assert!(bun_out.contains("\"warm\""), "bun's output: {bun_out}");
    assert_eq!(out, bun_out, "stderr: {err}");
    assert_eq!(cached, bun_out, "the cache it writes");
}

/// @raea/data/src/check.ts on a piece it can't check (no render): the same message and exit code as under bun (2), so
/// its whole import graph (facts.ts, @raea/motion's ECS) loads and runs the same way
#[test]
fn check_as_under_bun() {
    let data = std::env::var_os("RAEA_DATA").map(PathBuf::from).unwrap_or_else(|| repo().join("sdk/data"));
    if !data.join("node_modules").is_dir() {
        eprintln!("check_as_under_bun: skipped, no {}", data.join("node_modules").display());
        return;
    }
    let space = std::env::temp_dir().join(format!("raea-tools-check-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&space);
    std::fs::create_dir_all(&space).unwrap();
    std::fs::write(space.join("edit.tsx"), "export default function Edit() { return null; }\n").unwrap();
    let script = data.join("src/check.ts");
    let Some((bun_exit, bun_out)) = bun(&script, &["edit", space.to_str().unwrap()]) else { return };
    let (exit, out, err) = ours(&script, &["edit", space.to_str().unwrap()]);
    let _ = std::fs::remove_dir_all(&space);
    assert_eq!((exit, out.as_str()), (bun_exit, bun_out.as_str()), "stderr: {err}");
}

/// @raea/data/src/project.ts (void's `raea` project commands): the same command sequence in two fresh projects, one
/// under bun and one here; every command's output the same (the project folder's path aside)
#[test]
fn project_as_under_bun() {
    let data = std::env::var_os("RAEA_DATA").map(PathBuf::from).unwrap_or_else(|| repo().join("sdk/data"));
    if !data.join("node_modules").is_dir() {
        eprintln!("project_as_under_bun: skipped, no {}", data.join("node_modules").display());
        return;
    }
    let script = data.join("src/project.ts");
    let make = |tag: &str| {
        let root = std::env::temp_dir().join(format!("raea-tools-project-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("objects/ad")).unwrap();
        std::fs::write(root.join("objects/ad/space.json"), r#"{"name":"ad","slug":"ad","kind":"video"}"#).unwrap();
        std::fs::write(root.join("objects/ad/edit.tsx"), "export default function Edit() { return null; }\n").unwrap();
        root
    };
    let (a, b) = (make("bun"), make("ours"));
    let steps: &[&[&str]] = &[
        &["purpose", "ads for a coffee brand"],
        &["objects"],
        &["object", "open", "ad"],
        &["task", "new", "make a 15 s ad", "--object", "ad", "--accept", "! the hook reads within half a second"],
        &["task", "step", "t1", "rough cut"],
        &["task", "step", "t1", "rough cut", "--done"],
        &["ask", "t1", "which tone?", "--option", "warm: cosy", "--option", "bold: loud"],
        &["answer", "q1", "warm"],
        &["note", "keep it under 15 s", "--rule", "! the length is within the format"],
        &["notes"],
        &["tasks"],
        &["save", "--object", "ad", "--message", "first"],
        &["versions", "--object", "ad"],
        &["task", "state", "t1", "ready"],
        &["task", "show", "t1"],
        &["status"],
        &["status", "--json"],
    ];
    let norm = |s: &str, root: &Path| s.replace(&root.display().to_string(), "ROOT");
    // times ("at") differ run to run: compare with digits of epoch milliseconds masked
    let mask = |s: String| {
        let re_like = |t: &str| -> String {
            let mut out = String::new();
            let mut digits = String::new();
            for ch in t.chars() {
                if ch.is_ascii_digit() {
                    digits.push(ch);
                } else {
                    out.push_str(if digits.len() >= 12 { "T" } else { &digits });
                    digits.clear();
                    out.push(ch);
                }
            }
            out.push_str(if digits.len() >= 12 { "T" } else { &digits });
            out
        };
        re_like(&s)
    };
    for step in steps {
        let mut args_a = vec![step[0].to_string(), a.display().to_string()];
        args_a.extend(step[1..].iter().map(|s| s.to_string()));
        let mut args_b = vec![step[0].to_string(), b.display().to_string()];
        args_b.extend(step[1..].iter().map(|s| s.to_string()));
        let Some((bun_exit, bun_out)) = bun(&script, &args_a.iter().map(String::as_str).collect::<Vec<_>>()) else { return };
        let (exit, out, err) = ours(&script, &args_b.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!((exit, mask(norm(&out, &b))), (bun_exit, mask(norm(&bun_out, &a))), "{step:?}; stderr: {err}");
    }
    for d in [&a, &b] {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// apps/void/parts/brand.ts (void's brand tool): the same brand kit (styleguide.json, brand.css, brand.json, BRAND.md) and
/// the same lines from the same input as under bun
#[test]
fn brand_as_under_bun() {
    let script = repo().join("apps/void/parts/brand.ts");
    let base = std::env::temp_dir().join(format!("raea-tools-brand-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let input = base.join("in.json");
    std::fs::write(&input, r##"{"brand":{"name":"Bean & Co","domain":"beanco.example","slogan":"Slow coffee, fast mornings","colors":[{"hex":"#3b2314"},{"hex":"#f4e9dc"},{"hex":"#d9822b"},"#1a1a1a"]},
 "styleguide":{"mode":"light","colors":{"accent":"#d9822b","background":"#f4e9dc","text":"#1a1a1a"},"typography":{"headings":{"h1":{"fontFamily":"Gotham","fontSize":"48px","fontWeight":700}},"p":{"fontFamily":"Inter","fontSize":"16px","fontWeight":400}},"components":{"button":{"primary":{"backgroundColor":"#d9822b","color":"#ffffff","borderRadius":"8px"}}}},
 "logos":[{"url":"https://beanco.example/logo.png","mode":"dark"}]}"##).unwrap();
    let (a, b) = (base.join("bun"), base.join("ours"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    let Some((bun_exit, bun_out)) = bun(&script, &[a.to_str().unwrap(), input.to_str().unwrap()]) else { return };
    let (exit, out, err) = ours(&script, &[b.to_str().unwrap(), input.to_str().unwrap()]);
    assert_eq!((exit, out.as_str()), (bun_exit, bun_out.as_str()), "stderr: {err}");
    for f in ["BRAND.md", "brand/brand.json", "brand/brand.css", "brand/styleguide.json"] {
        let (x, y) = (std::fs::read_to_string(a.join(f)).unwrap_or_default(), std::fs::read_to_string(b.join(f)).unwrap_or_default());
        assert!(!x.is_empty() && x == y, "{f} differs from bun's");
    }
    let _ = std::fs::remove_dir_all(&base);
}
