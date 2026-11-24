use super::*;

/// A library with skillbase fixes.py's schema, in memory
fn library(rows: &[(i64, &str, &str, &str, &str, &str)]) -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE fixes(id INTEGER PRIMARY KEY, support INTEGER, problem TEXT, cause TEXT, fix TEXT, also TEXT, errors TEXT,
           idents TEXT, domains TEXT, how TEXT, sources TEXT);
         CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT);",
    )
    .unwrap();
    for (support, problem, cause, fix, errors, domains) in rows {
        db.execute(
            "INSERT INTO fixes(support, problem, cause, fix, also, errors, idents, domains, how, sources) VALUES (?1, ?2, ?3, ?4, '', ?5, '', ?6, 'table', 'a/b')",
            params![support, problem, cause, fix, errors, domains],
        )
        .unwrap();
    }
    db.execute_batch(
        "CREATE VIRTUAL TABLE fixes_fts USING fts5(problem, errors, idents, cause, fix, also, content='fixes', content_rowid='id',
           tokenize='porter unicode61 remove_diacritics 2');
         INSERT INTO fixes_fts(rowid, problem, errors, idents, cause, fix, also) SELECT id, problem, errors, idents, cause, fix, also FROM fixes;",
    )
    .unwrap();
    db
}

fn sample() -> Connection {
    let mut rows = vec![
        (2, "Shadows cut off or blocky", "the shadow camera doesn't cover the scene", "Fit the orthographic shadow camera and raise mapSize", "", "threejs 3d"),
        (1, "Shadow acne on lit surfaces", "bias too small", "Raise shadow.bias a little", "", "threejs"),
        (1, "Shadows missing in the game", "the light doesn't cast", "Enable shadows on the DirectionalLight", "", "bevy 3d"),
        (1, "Weights all zero after binding", "", "Count weighted vertices after binding", "Bone Heat Weighting: failed to find solution for one or more bones", "blender"),
    ];
    // filler, so "shadows" stays under the 4%-of-entries cut
    for i in 0..200 {
        rows.push((1, "Unrelated problem", "", "Something else entirely", "", if i % 2 == 0 { "ffmpeg" } else { "video" }));
    }
    library(&rows)
}

#[test]
fn parse_keeps_phrases_and_drops_stopwords() {
    let q = parse(r#"why is my "Bone Heat Weighting: failed" mesh black in three.js"#);
    assert_eq!(q.phrases, vec!["Bone Heat Weighting: failed"]);
    assert_eq!(q.words, vec!["mesh", "black", "three.js"]);
}

#[test]
fn library_ranks_the_problem_match_first() {
    let hits = search_library(&sample(), &parse("shadows look blocky, cut off"), None, 3).unwrap();
    assert_eq!(hits[0].problem, "Shadows cut off or blocky");
}

#[test]
fn a_domain_word_ranks_without_matching() {
    let hits = search_library(&sample(), &parse("bevy shadows missing"), None, 3).unwrap();
    assert_eq!(hits[0].domains, "bevy 3d");
    // "bevy" alone matches nothing extra: every hit still matches a plain word
    assert!(hits.iter().all(|h| h.problem.to_lowercase().contains("shadow")));
}

#[test]
fn a_quoted_error_matches_the_error_text() {
    let hits = search_library(&sample(), &parse(r#""failed to find solution""#), None, 3).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].problem, "Weights all zero after binding");
}

#[test]
fn domain_filters() {
    let hits = search_library(&sample(), &parse("shadows"), Some("bevy"), 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].domains, "bevy 3d");
}

#[test]
fn own_entries_need_half_the_words() {
    let rows = own_entries("# c\nText3D words sit left\tcentred origin\tlay out by centres\t\traea 3d\tfilm, 11-10\nSky cut off\tfar plane\traise far\t\traea 3d\tfilm, 11-10\n");
    assert_eq!(rows.len(), 2);
    let hits = search_own(&rows, &parse("my Text3D words are placed too far left"), None, 3).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].problem, "Text3D words sit left");
    // one word of five isn't enough
    assert!(search_own(&rows, &parse("sky colour wrong under fog at dusk"), None, 3).unwrap().is_empty());
}

#[test]
fn the_shipped_own_entries_parse() {
    let text = include_str!("../../../sdk/guides/fixes/raea.tsv");
    let lines = text.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()).count();
    let rows = own_entries(text);
    assert_eq!(rows.len(), lines, "every entry in raea.tsv has six tab-separated columns");
    assert!(rows.iter().all(|r| !r[0].is_empty() && !r[2].is_empty() && r[4].split_whitespace().any(|d| d == "raea") && !r[5].is_empty()));
}

#[test]
fn render_marks_raea_entries() {
    let rows = own_entries("Sky cut off\tfar plane\traise far\t\traea 3d\tfilm, 11-10\n");
    let out = render(&search_own(&rows, &parse("sky cut off"), None, 3).unwrap(), false);
    assert!(out.starts_with("raea  Sky cut off\n"));
    assert!(out.contains("found in raea work: film, 11-10"));
}

/// Against a real library: `RAEA_FIXES=PATH cargo test -p raea-sys-fixes --release -- --ignored --nocapture`. Prints each
/// query's hits and time, and fails if a known problem's fix isn't in its top three.
#[test]
#[ignore]
fn real_library() {
    let db = open_library(&library_path().expect("RAEA_FIXES names no file")).unwrap();
    let cases = [
        ("shadows look blocky or cut off", "shadow"),
        ("blender render fireflies", "clamp"),
        ("three.js colors washed out", "colorspace"),
        (r#""failed to find solution for one or more bones""#, "weight"),
        ("ffmpeg audio out of sync", "sync"),
        ("mesh renders black", "normal"),
    ];
    for (q, want) in cases {
        let t = std::time::Instant::now();
        let hits = search_library(&db, &parse(q), None, 3).unwrap();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        println!("--- {q} ({ms:.1} ms)\n{}", render(&hits, false));
        let top = hits.iter().map(|h| format!("{} {} {}", h.problem, h.cause, h.fix).to_lowercase().replace([' ', '_'], "")).collect::<Vec<_>>();
        assert!(top.iter().any(|t| t.contains(want)), "{q}: no top-3 hit mentions {want}");
    }
}
