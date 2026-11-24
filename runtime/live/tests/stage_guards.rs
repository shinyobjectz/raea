//! Stage guards (docs/stage-guards.feature SG1, SG2, SG4): a 3D part draws as its own frame has it, however that frame
//! is reached. The mobile-ads essay (11-21) showed each fault in an mp4, whose 3D is made live frame by frame
//! (raea-runtime-scene live.rs), and in a still, whose rows are made from the frames around it (rows):
//! - a part's shape and a RichText's words held their first frame's (its curves broke into dashes, its counters read
//!   "0.0 MB", its typed lines stopped half-typed);
//! - a RichText mounted as another went took that one's row, and drew its words in its own colours ("A RENDE? ENGINE");
//! - a glyph's colour held its first frame's.
//!
//! Tested here: raea-runtime-scene is at its 2,000-line limit. RAEA_LIVE3D_CHECK holds every frame the fast path makes
//! against the full path's.
use raea_core_program::eval::Tup;
use raea_core_program::ir::{Program, V};
use raea_runtime_react::scene::{LiveScene, Node3, Shot};
use serde_json::{Value, json};

fn check_on() {
    static ON: std::sync::Once = std::sync::Once::new();
    // SAFETY: every test calls this before it reads the environment, and the Once holds the others until it's set
    ON.call_once(|| unsafe { std::env::set_var("RAEA_LIVE3D_CHECK", "1") });
}

fn node(kind: &str, props: Value, kids: Vec<Node3>) -> Node3 {
    Node3 { kind: kind.into(), props: std::sync::Arc::new(props.as_object().unwrap().clone()), shape: None, at: None, kids: kids.into() }
}

/// A mesh with its shape and its own name ("" for none).
fn mesh(name: &str, shape: &str, color: &str) -> Node3 {
    let props = if name.is_empty() { json!({}) } else { json!({ "name": name }) };
    let mut m = node("Mesh", props, vec![node("MeshStandardMaterial", json!({ "color": color }), vec![])]);
    m.shape = Some(shape.into());
    m
}

/// A RichText of `words` in one colour, as raea/3d's RichText hands it on: its block and a glyph per letter.
fn words(name: &str, words: &str, color: &str) -> Node3 {
    let glyphs: Vec<Value> = words.char_indices().map(|(at, ch)| json!({ "at": at, "span": 0, "ch": ch.to_string(), "p": [0, 0, 0], "r": [0, 0, 0], "s": [1, 1, 1], "c": color, "o": 1 })).collect();
    let mut props = json!({ "text": words, "spans": [{ "t": words }], "size": 0.5, "glyphs": glyphs });
    if !name.is_empty() {
        props["name"] = json!(name);
    }
    node("RichText", props, vec![])
}

fn s(v: Option<&V>) -> String {
    match v {
        Some(V::S(s)) => s.clone(),
        _ => String::new(),
    }
}

/// The rows a frame spawns whole (those with the columns only a spawn reads), by kind.
fn spawned<'a>(set: &'a [Tup], kind: &str, col: &str) -> Vec<&'a Tup> {
    set.iter().filter(|t| t.kind == kind && t.vals.contains_key(col)).collect()
}

/// A Sphere whose radius changes, named so it keeps its row: dropped and spawned again with the new shape, on the fast
/// path's frames as on the full path's. The same shape again changes nothing.
#[test]
fn a_shape_that_changes_is_spawned_again() {
    check_on();
    let mut ls = LiveScene::default();
    ls.open(Shot { scene: &[mesh("a", "sphere(0.5)", "#ff0000")], clear: None }, 64, 64, 30.0);
    let id = |set: &[Tup]| spawned(set, "mesh", "shape").first().map(|t| (t.id.clone(), s(t.vals.get("shape"))));
    let (set, gone) = ls.frame(Shot { scene: &[mesh("a", "sphere(0.5)", "#ff0000")], clear: None });
    assert!(gone.is_empty() && id(&set).is_none(), "the same shape: nothing spawned or dropped");
    let (set, gone) = ls.frame(Shot { scene: &[mesh("a", "sphere(1)", "#ff0000")], clear: None });
    let (row, shape) = id(&set).expect("the new radius spawns the sphere again");
    assert_eq!(shape, "sphere(1)");
    assert_eq!(gone, vec![("mesh".to_string(), row.clone())], "its old row dropped first, under the same id");
    // a colour change alone is a column, never a new spawn
    let (set, gone) = ls.frame(Shot { scene: &[mesh("a", "sphere(1)", "#0000ff")], clear: None });
    assert!(gone.is_empty() && id(&set).is_none());
    assert!(set.iter().any(|t| t.id == row && t.vals.contains_key("col_b")));
}

/// A named RichText whose words change (a counter): its block and every glyph dropped, glyphs first (SG4: the block's
/// entity takes theirs with it), and spawned again with the new words, one glyph a letter.
#[test]
fn new_words_are_laid_out_again() {
    check_on();
    let mut ls = LiveScene::default();
    ls.open(Shot { scene: &[words("count", "0.0 MB", "#ffffff")], clear: None }, 64, 64, 30.0);
    let (set, gone) = ls.frame(Shot { scene: &[words("count", "12.5 MB", "#ffffff")], clear: None });
    let block = spawned(&set, "text", "spans");
    assert_eq!(block.len(), 1, "the block spawned again");
    assert!(s(block[0].vals.get("spans")).contains("12.5 MB"));
    let chars: String = spawned(&set, "glyph", "ch").iter().map(|t| s(t.vals.get("ch"))).collect();
    assert_eq!(chars, "12.5 MB", "a glyph a letter of the new words");
    assert_eq!(gone.len(), 7, "the old block and its six glyphs: {gone:?}");
    assert_eq!(gone.last().map(|(k, _)| k.as_str()), Some("text"), "glyphs dropped before their block: {gone:?}");
}

/// kept-props3.tsx: "FOUR OPENINGS" unmounts as "FROM ITS STORE PAGE" mounts, neither named. The new block is a row of
/// its own with its own words and colour; the old one's row is hidden, never handed on.
#[test]
fn a_new_text_never_takes_an_old_ones_row() {
    check_on();
    let mut ls = LiveScene::default();
    ls.open(Shot { scene: &[words("", "FOUR OPENINGS", "#ffffff")], clear: None }, 64, 64, 30.0);
    let (set, _) = ls.frame(Shot { scene: &[words("", "FOUR OPENINGS", "#ffffff")], clear: None });
    let old: Vec<String> = set.iter().map(|t| t.id.clone()).collect();
    let (set, _) = ls.frame(Shot { scene: &[words("", "FROM ITS STORE PAGE", "#ff3b30")], clear: None });
    let block = spawned(&set, "text", "spans");
    assert_eq!(block.len(), 1);
    assert!(s(block[0].vals.get("spans")).contains("FROM ITS STORE PAGE"));
    assert!(!old.contains(&block[0].id), "a row of its own, not the old block's");
    let glyphs = spawned(&set, "glyph", "ch");
    assert_eq!(glyphs.iter().map(|t| s(t.vals.get("ch"))).collect::<String>(), "FROM ITS STORE PAGE");
    assert!(glyphs.iter().all(|t| s(t.vals.get("target")) == block[0].id && s(t.vals.get("color")) == "#ff3b30"));
    // the old block hidden, its glyphs with it
    assert!(set.iter().any(|t| t.kind == "text" && t.id != block[0].id && t.vals.get("visible") == Some(&V::N(0.0))));
}

/// A RichText's colour that changes (kept-props2.tsx): sent to its glyphs as col_r/g/b, as a mesh's is.
#[test]
fn a_glyph_colour_follows_the_frame() {
    check_on();
    let mut ls = LiveScene::default();
    ls.open(Shot { scene: &[words("tag", "AB", "#ffffff")], clear: None }, 64, 64, 30.0);
    let (set, gone) = ls.frame(Shot { scene: &[words("tag", "AB", "#ff0000")], clear: None });
    assert!(gone.is_empty(), "a colour spawns nothing again");
    let glyphs: Vec<&Tup> = set.iter().filter(|t| t.kind == "glyph").collect();
    assert_eq!(glyphs.len(), 2);
    assert!(glyphs.iter().all(|t| t.vals.get("col_r") == Some(&V::N(1.0)) && t.vals.get("col_g") == Some(&V::N(0.0))), "{glyphs:?}");
}

/// The rows of a still, or of RAEA_3D=program, are made from several frames (a still's neighbours): a part whose shape
/// or words change between them is a row per run, each shown in its own frames, never the first frame's held over.
#[test]
fn a_still_draws_its_own_frames_shape_and_words() {
    let frames = [vec![mesh("a", "sphere(0.5)", "#ff0000"), words("count", "0.0 MB", "#ffffff")], vec![mesh("a", "sphere(1)", "#ff0000"), words("count", "12.5 MB", "#ffffff")]];
    let all: Vec<_> = frames.iter().map(|f| raea_runtime_scene::items_of(f)).collect();
    let mut p = Program::default();
    raea_runtime_scene::rows(&all, &mut p);
    let of = |kind: &str, col: &str| -> Vec<String> { p.spawns.iter().filter(|(k, _)| k == kind).flat_map(|(_, rows)| rows.iter().map(|r| s(r.get(col)))).collect() };
    let mut shapes = of("mesh", "shape");
    shapes.sort();
    assert_eq!(shapes, vec!["sphere(0.5)", "sphere(1)"]);
    let spans = of("text", "spans");
    assert_eq!(spans.len(), 2);
    assert!(spans[0].contains("0.0 MB") && spans[1].contains("12.5 MB"));
}

/// Footage behind the 3D (SG2): a Plate unmounting as another mounts, neither named. The new one is a row of its own
/// with its own clip; the old one's row is hidden, never handed on with the old clip. A fire whose simulation changes
/// is spawned again, as a mesh whose shape changes is.
#[test]
fn a_new_plate_never_takes_an_old_ones_row() {
    check_on();
    let plate = |src: &str| node("Plate", json!({ "src": src, "time": 0.0 }), vec![]);
    let mut ls = LiveScene::default();
    ls.open(Shot { scene: &[plate("/clips/street.mp4")], clear: None }, 64, 64, 30.0);
    let (set, _) = ls.frame(Shot { scene: &[plate("/clips/street.mp4")], clear: None });
    let old: Vec<String> = set.iter().map(|t| t.id.clone()).collect();
    let (set, _) = ls.frame(Shot { scene: &[plate("/clips/harbour.mp4")], clear: None });
    let new = spawned(&set, "plate", "src");
    assert_eq!(new.len(), 1, "{set:?}");
    assert!(s(new[0].vals.get("src")).ends_with("harbour.mp4"));
    assert!(!old.contains(&new[0].id), "a row of its own, not the old plate's");
    assert!(set.iter().any(|t| t.kind == "plate" && t.id != new[0].id && t.vals.get("visible") == Some(&V::N(0.0))), "{set:?}");

    let fire = |sim: &str| node("Fire", json!({ "sim": { "kind": sim }, "time": 0.0 }), vec![]);
    let mut ls = LiveScene::default();
    ls.open(Shot { scene: &[fire("smoke")], clear: None }, 64, 64, 30.0);
    ls.frame(Shot { scene: &[fire("smoke")], clear: None });
    let (set, gone) = ls.frame(Shot { scene: &[fire("cloud")], clear: None });
    assert_eq!(gone.len(), 1, "the old fire dropped");
    assert!(s(spawned(&set, "fire", "sim").first().and_then(|t| t.vals.get("sim"))).contains("cloud"), "{set:?}");
}
