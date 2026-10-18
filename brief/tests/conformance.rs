//! conformance/cases: each .brief says on its first line what reading it gives: `// expect: ok`, or the refusals it
//! must give as `LINE:COL words` joined by ` | ` (each refusal found at that place and starting with those words)

use raea_brief::{fmt, graph, load, parse, Loaded};
use std::path::Path;

#[test]
fn cases() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance/cases");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "brief")).collect();
    files.sort();
    let mut failed = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        let expect = text.lines().next().and_then(|l| l.strip_prefix("// expect:")).expect("a case starts with // expect:").trim().to_string();
        let mut docs: Vec<Loaded> = Vec::new();
        let got: Vec<String> = match load(f, &mut Vec::new(), &mut docs).and_then(|_| graph(&docs).map(|_| ())) {
            Ok(()) => vec![],
            Err(bad) => bad.into_iter().map(|r| {
                // a refusal from load carries its file: keep LINE:COL and the words
                let m = r.msg.rsplit_once(".brief:").map(|(_, x)| x.splitn(3, ':').nth(2).unwrap_or(x).trim().to_string()).unwrap_or(r.msg);
                format!("{}:{} {}", r.line, r.col, m)
            }).collect(),
        };
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        if expect == "ok" {
            if !got.is_empty() { failed.push(format!("{name}: expected ok, got {got:?}")); }
            // and fmt reads back
            let d = parse(&text).unwrap();
            if parse(&fmt(&d)).is_err() { failed.push(format!("{name}: fmt doesn't read back")); }
            continue;
        }
        for want in expect.split(" | ") {
            if !got.iter().any(|g| g.starts_with(want)) { failed.push(format!("{name}: no refusal `{want}` in {got:?}")); }
        }
        let n = expect.split(" | ").count();
        if got.len() != n { failed.push(format!("{name}: {} refusals, expected {n}: {got:?}", got.len())); }
    }
    assert!(failed.is_empty(), "{} of {} cases:\n{}", failed.len(), files.len(), failed.join("\n"));
}
