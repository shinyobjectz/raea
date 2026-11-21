//! The crate's own checks: which fingerprint judges a piece, where two differ, and a profile's critique.

use raea_sys_fingerprint::evidence::{Ledger, Row, width};
use raea_sys_fingerprint::folder::Folder;
use raea_sys_fingerprint::taxonomy::base;
use raea_sys_fingerprint::words::Words;

/// A blur label in a project, its window `at` seconds in
fn row(bad: bool, project: &str, at: f64) -> Row {
    Row { media: "m".into(), a: at, b: at + 2.0, axis: "blur".into(), bad, weight: 1.0, who: "t".into(), source: "teach".into(), project: project.into(), kind: None, region: None, words: None, at: 0, pin: None, band: None }
}

/// Features with the label's sign in the picture's first values and a little made-up spread in the rest
fn signal(x: f32, seed: usize) -> Vec<f32> {
    (0..width()).map(|i| if i < 16 { x } else { (((i * 31 + seed * 17) % 97) as f32 / 97.0 - 0.5) * 0.1 }).collect()
}

/// FP14c: the project's fingerprints first (the one that applies to the piece's kind, else the project's default),
/// then the workspace's, then the person's own default
#[test]
fn which_one_judges_a_piece() {
    use crate::which::{default_file, which};
    let tmp = std::env::temp_dir().join(format!("raea-fp-which-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let (ws, home) = (tmp.join("ws"), tmp.join("home"));
    let proj = ws.join("projects/brand");
    std::fs::create_dir_all(ws.join(".raea")).unwrap();
    std::fs::write(ws.join(".raea/workspace"), "[workspace]\n\tname = W\n").unwrap();
    std::fs::create_dir_all(proj.join(".raea")).unwrap();
    std::fs::write(proj.join(".raea/project"), "[project]\n\tname = brand\n\tkind = ad\n\ttags = brand-x, fast\n").unwrap();
    // SAFETY: no other test in this crate reads RAEA_FINGERPRINTS
    unsafe { std::env::set_var("RAEA_FINGERPRINTS", &home) };
    let make = |dir: &std::path::Path, name: &str, kinds: &[&str]| {
        let mut f = base(name);
        f.about = Some(format!("{name}'s"));
        if !kinds.is_empty() {
            f.applies = Some(raea_sys_fingerprint::taxonomy::Applies { kinds: kinds.iter().map(|k| k.to_string()).collect(), tags: vec![] });
        }
        Folder { dir: dir.join(name) }.create(&f).unwrap();
    };
    let cut = proj.join("out/cut.mp4");
    // a videoform judges only what it applies to: alone, and first by name, it's never the account's fallback (VF2)
    let mut form = base("a-form");
    form.videoform = Some(raea_core_brief::Videoform { context: "street interviews".into(), ..Default::default() });
    form.applies = Some(raea_sys_fingerprint::taxonomy::Applies { kinds: vec![], tags: vec!["a-form".into()] });
    Folder { dir: home.join("a-form") }.create(&form).unwrap();
    assert!(which(&cut).is_none(), "a videoform the piece isn't tagged for doesn't judge it");
    make(&home, "me", &[]);
    let c = which(&cut).unwrap();
    assert_eq!((c.name.as_str(), c.level.as_str()), ("me", "account"), "nothing nearer: the person's own");

    make(&ws.join(".raea/fingerprints"), "studio", &[]);
    std::fs::write(ws.join(".raea/fingerprints/default"), "studio").unwrap();
    assert_eq!(which(&cut).unwrap().name, "studio", "the workspace's default");

    make(&proj.join(".raea/fingerprints"), "films", &["film"]);
    make(&proj.join(".raea/fingerprints"), "ads", &["ad"]);
    let c = which(&cut).unwrap();
    assert_eq!((c.name.as_str(), c.level.as_str()), ("ads", "project"));
    assert_eq!(c.why, "the project's, and it applies to ad pieces");

    std::fs::write(proj.join(".raea/project"), "[project]\n\tname = brand\n").unwrap();
    assert_eq!(which(&cut).unwrap().name, "studio", "no kind matches and no project default: the workspace's");
    std::fs::write(proj.join(".raea/fingerprints/default"), "me").unwrap();
    let c = which(&cut).unwrap();
    assert_eq!((c.name.as_str(), c.level.as_str()), ("me", "project"), "a project can pin the person's own");
    assert_eq!(default_file(&cut, "project").unwrap(), proj.join(".raea/fingerprints/default"));
    assert_eq!(default_file(&home.join("x.mp4"), "project"), None);
    let _ = std::fs::remove_dir_all(&tmp);
}

/// FP8b: two people who call opposite things blurry are far apart on blur, and the moments say where
#[test]
fn diff_finds_where_two_tastes_differ() {
    let tmp = std::env::temp_dir().join(format!("raea-fp-diff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let make = |name: &str, flip: bool| {
        let dir = Folder { dir: tmp.join(name) };
        dir.create(&base(name)).unwrap();
        let mut l = Ledger::default();
        for i in 0..60 {
            let bad = i % 2 == 0;
            l.push(row(bad, &format!("p{}", i % 4), i as f64 * 2.0), &signal(if bad != flip { 1.0 } else { -1.0 }, i));
        }
        dir.write_ledger(&l).unwrap();
        raea_sys_fingerprint::retrain(&dir, &base(name), &Words::default(), &l, None).unwrap();
        dir
    };
    let (a, b, c) = (make("a", false), make("b", true), make("c", false));
    let ab = crate::diff::diff(&a, &b, 2).unwrap();
    let blur = ab.iter().find(|d| d.axis == "blur").unwrap();
    assert!(blur.apart > 1.5, "opposed tastes: {}", blur.apart);
    assert_eq!(blur.moments.len(), 2);
    let ac = crate::diff::diff(&a, &c, 2).unwrap();
    assert!(ac.iter().find(|d| d.axis == "blur").unwrap().apart < 0.2, "the same taste agrees");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// FP6e: a profile's ranges come from its references, the leave-one-out check says whether it tells a taste apart, and
/// a critique says what's out, which way and where
#[test]
fn a_profile_critiques_a_piece() {
    use crate::profile::{Measures, critique, profile};
    let m = |b: f32, l: f32| Measures::from([("brightness".to_string(), b), ("loudness".to_string(), l)]);
    let refs = vec![("a".to_string(), m(0.40, -18.0)), ("b".to_string(), m(0.45, -16.0)), ("c".to_string(), m(0.50, -17.0)), ("d".to_string(), m(0.42, -16.5))];
    let other = vec![m(0.10, -30.0), m(0.90, -8.0)];
    let p = profile("bright", refs, &other);
    assert_eq!(p.tells, Some(true), "own {:?}, other {:?}", p.inside_own, p.inside_other);
    let (lo, hi) = p.ranges["brightness"];
    assert!(lo < 0.40 && hi > 0.50);
    let windows = vec![((0.0, 4.0), m(0.45, -17.0)), ((4.0, 8.0), m(0.12, -17.0))];
    let out = critique(&p, &m(0.30, -17.0), &windows);
    assert_eq!(out.len(), 1);
    assert_eq!((out[0].measure.as_str(), out[0].move_.as_str(), out[0].at), ("brightness", "raise", Some((4.0, 8.0))));
}

/// A script has no picture: score judges it on text axes only, never calls its windows blurry, and a video's picture
/// axes still score
#[test]
fn an_axis_judges_only_what_the_file_has() {
    use raea_sys_columns::{CLAP, Columns, SIG, TEXT, window::Modality};
    let fp = base("t");
    let w = Words { axes: vec![("blur".into(), Modality::Picture, vec![0.3; SIG])], sound_vocab: vec![vec![0.1; CLAP]; 2], picture_vocab: vec![vec![0.2; SIG], vec![-0.2; SIG]], text_vocab: vec![], made_from: vec![] };
    let script = Columns { dur: 3.0, text: vec![0.1; TEXT * 2], ..Default::default() };
    let s = crate::score::score(&fp, Some(&w), None, &script, "script.md");
    assert!(!s.windows.is_empty() && s.windows.iter().all(|x| x.axes.is_empty() && x.seen.is_none()), "{:?}", s.windows.iter().map(|x| x.axes.iter().map(|a| a.axis.clone()).collect::<Vec<_>>()).collect::<Vec<_>>());
    let video = Columns { dur: 3.0, sig: vec![0.1; SIG * 24], ..Default::default() };
    let v = crate::score::score(&fp, Some(&w), None, &video, "cut.mp4");
    assert!(v.windows.iter().all(|x| x.axes.iter().any(|a| a.axis == "blur")));
}
