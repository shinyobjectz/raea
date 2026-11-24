//! The steps' own work, without a render or a paid call: sheets, prompts, flows, picks, components, the floor plan.

use std::path::PathBuf;

use image::{Rgba, RgbaImage};
use raea_core_brief::{make::Make, read};

use crate::sheet::{Tile, sheet};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("raea-make-steps-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).expect("a scratch folder");
    d
}

fn png(p: &PathBuf, w: u32, h: u32, c: [u8; 3]) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    RgbaImage::from_pixel(w, h, Rgba([c[0], c[1], c[2], 255])).save(p).unwrap();
}

fn kit() -> Make {
    read("make", r#"{"kind": "kit", "references": ["media/style.png"], "style": "1970s flight desk, cream enamel, maker Aurora Instruments.", "objects": [
        {"id": "rotary_phone", "label": "Rotary phone", "h": 0.12, "prompt": "a rotary desk phone, worn bakelite", "hero": true},
        {"id": "clock", "label": "Wall clock", "dims": [0.3, 0.05, 0.3]}]}"#)
    .unwrap()
}

// MK10, MK11, MK15: a sheet is its tiles in rows, labelled, any shape fitted without stretching, a missing one grey
#[test]
fn a_sheet_holds_its_tiles_in_rows_with_labels() {
    let d = dir("sheet");
    let (a, b, wide) = (d.join("a.png"), d.join("b.png"), d.join("wide.png"));
    png(&a, 200, 100, [200, 30, 30]);
    png(&b, 200, 100, [30, 200, 30]);
    png(&wide, 400, 100, [30, 30, 200]);
    let out = d.join("review/sheet.png");
    let rows = vec![
        vec![Tile { img: a.clone(), label: "red <a&b>".into() }, Tile { img: b, label: "green".into() }],
        vec![Tile { img: wide, label: "wide".into() }, Tile { img: d.join("gone.png"), label: "gone".into() }],
    ];
    sheet("Test sheet", &rows, 300, &out).unwrap();
    let img = image::open(&out).unwrap().to_rgba8();
    // two columns of 300 and three gaps of 6; the tile is 150 tall (from the first's 2:1), title 44, label 30, gap 6
    assert_eq!((img.width(), img.height()), (618, 44 + 2 * (150 + 30 + 6)));
    // the first tile is red in its middle; the wide one is letterboxed (fitted 300x75, centred), not stretched
    assert_eq!(img.get_pixel(6 + 150, 44 + 75).0[..3], [200, 30, 30]);
    let row2 = 44 + 150 + 30 + 6;
    assert_eq!(img.get_pixel(6 + 150, row2 + 75).0[..3], [30, 30, 200]);
    assert_ne!(img.get_pixel(6 + 150, row2 + 10).0[..3], [30, 30, 200]);
    // the missing one is grey
    assert_eq!(img.get_pixel(6 + 300 + 6 + 150, row2 + 75).0[..3], [70, 70, 70]);
    // a label was drawn under the first tile (light pixels on the dark ground)
    let lit = (0..300).flat_map(|x| (0..30).map(move |y| (x, y))).filter(|&(x, y)| img.get_pixel(6 + x, 44 + 150 + y).0[0] > 150).count();
    assert!(lit > 40, "no label drawn ({lit} lit pixels)");
    assert!(sheet("empty", &[vec![Tile { img: d.join("none.png"), label: "x".into() }]], 100, &d.join("e.png")).is_err());
}

// MK10: a reference prompt is the plan's style, the object's words and the scan template, never a brand
#[test]
fn a_reference_prompt_is_style_object_and_scan() {
    let m = kit();
    let p = crate::kit::ref_prompt(&m, &m.objects[0]);
    assert!(p.starts_with("1970s flight desk, cream enamel, maker Aurora Instruments. a rotary desk phone, worn bakelite. One object only"), "{p}");
    assert!(p.contains("no real brands"));
    // an object with no prompt is described by its label
    assert!(crate::kit::ref_prompt(&m, &m.objects[1]).contains(". Wall clock. "));
    assert_eq!(crate::kit::size_args(&m.objects[0]), ["--h", "0.12"]);
    assert_eq!(crate::kit::size_args(&m.objects[1]), ["--dims", "0.3,0.05,0.3"]);
}

// MK11: one reference to both generators, the hero at 150k faces; the single-shot gen's file read from its JSON
#[test]
fn both_generators_get_each_reference() {
    let m = kit();
    let f = crate::kit::gen_flow(&m, &m.objects[0], std::path::Path::new("/r/rotary_phone.png"), crate::kit::TRIPO, crate::kit::MESHY);
    assert_eq!(f["out"], serde_json::json!(["tripo", "meshy"]));
    assert_eq!(f["steps"]["tripo"]["model"], "tripo:H3.1");
    assert_eq!(f["steps"]["tripo"]["in"]["x_face_limit"], 150000);
    assert_eq!(f["steps"]["meshy"]["model"], "fal:meshy/v7.1/image-to-3d");
    assert_eq!(f["steps"]["meshy"]["in"]["image"], "@ref");
    let small = crate::kit::gen_flow(&m, &m.objects[1], std::path::Path::new("/r/clock.png"), crate::kit::TRIPO, crate::kit::MESHY);
    assert_eq!(small["steps"]["tripo"]["in"]["x_face_limit"], 60000);
    assert_eq!(crate::kit::gen_file(r#"{"out": {"out": "/g/out.png"}, "cents": 4}"#), Some(PathBuf::from("/g/out.png")));
    assert_eq!(crate::kit::gen_file(r#"{"out": {"out": ["/g/a.png", "/g/b.png"]}}"#), Some(PathBuf::from("/g/a.png")));
    assert_eq!(crate::kit::gen_file("not json"), None);
}

// MK11: a pick names a model that exists and says why; it's kept by object
#[test]
fn a_pick_needs_its_model_and_a_reason() {
    let d = dir("pick");
    let m = kit();
    assert!(crate::kit::choose(&d, &m, "rotary_phone", "tripo", "sharper").unwrap_err().contains("raea make gen --only rotary_phone"));
    std::fs::create_dir_all(crate::kit::gen_dir(&d, "rotary_phone")).unwrap();
    std::fs::write(crate::kit::gen_dir(&d, "rotary_phone").join("meshy.glb"), "x").unwrap();
    assert!(crate::kit::choose(&d, &m, "rotary_phone", "meshy", "  ").unwrap_err().contains("say why"));
    assert!(crate::kit::choose(&d, &m, "lamp", "meshy", "x").unwrap_err().contains("no object `lamp`"));
    crate::kit::choose(&d, &m, "rotary_phone", "meshy", "Tripo fused the handset to the cradle").unwrap();
    assert_eq!(crate::kit::picks(&d)["rotary_phone"]["generator"], "meshy");
    // prep names what isn't picked yet rather than skipping it
    let said = crate::kit::prep(&d, &m, &["clock".to_string()]).unwrap();
    assert!(said.contains("clock: no pick yet"), "{said}");
}

// MK13: each object a component importing its own model and LOD beside it; the lineup imports them all with a 1 m bar
#[test]
fn the_kit_lands_as_components_and_a_lineup() {
    let m = kit();
    assert_eq!(crate::kitout::component_name("rotary_phone"), "RotaryPhone");
    let c = crate::kitout::component(&m.objects[0], "sharper numerals");
    assert!(c.starts_with("// raea component add: Rotary phone, 0.12 m"), "{c}");
    assert!(c.contains("import full from \"./rotary_phone.glb\";") && c.contains("import far from \"./rotary_phone_lod1.glb\";"));
    assert!(c.contains("export default function RotaryPhone("));
    let l = crate::kitout::lineup(&m, "desk");
    assert!(l.contains("import RotaryPhone from \"./RotaryPhone\";") && l.contains("import Clock from \"./Clock\";"));
    assert!(l.contains("</Text3D>") && l.contains("registerRoot("));
    // the bar is the longest round length the set covers
    assert_eq!(crate::kitout::scale_bar(0.52), (0.5, "50 cm"));
    assert_eq!(crate::kitout::scale_bar(1.7), (1.0, "1 m"));
    assert_eq!(crate::kitout::scale_bar(0.02), (0.05, "5 cm"));
    // the viewer frames a model by its height
    let v = crate::kitout::viewer(std::path::Path::new("/k/tripo.fit.glb"), 0.12);
    assert!(v.contains("import model from \"./tripo.fit.glb\";") && v.contains("<Model src={model} />"));
    // where it lands: the workspace's shared/components, else the project's components
    let d = dir("land");
    let job = d.join("projects/p/kit");
    std::fs::create_dir_all(&job).unwrap();
    std::fs::create_dir_all(d.join("projects/p/.raea")).unwrap();
    std::fs::write(d.join("projects/p/.raea/project"), "{}").unwrap();
    let real = dunce::canonicalize(&d).unwrap();
    assert_eq!(crate::kitout::kit_dir(&job, "desk", None), real.join("projects/p/components/desk"));
    std::fs::create_dir_all(d.join(".raea")).unwrap();
    std::fs::write(d.join(".raea/workspace"), "{}").unwrap();
    std::fs::create_dir_all(d.join("shared")).unwrap();
    assert_eq!(crate::kitout::kit_dir(&job, "desk", None), real.join("shared/components/desk"));
    assert_eq!(crate::kitout::kit_dir(&job, "desk", Some("/x")), PathBuf::from("/x"));
}

// MK15: a shot's frames are its first, middle and last (a still: its first); the lens chart's angles
#[test]
fn a_shot_frames_its_start_middle_and_end() {
    let m: Make = read("make", r#"{"kind": "shot", "shots": [{"name": "push", "seconds": 4, "composition": "push.tsx"}, {"name": "hold", "still": true}, {"name": "fast", "seconds": 2, "fps": 24}]}"#).unwrap();
    assert_eq!(crate::shots::frames_of(&m.shots[0]), vec![0, 59, 119]);
    assert_eq!(crate::shots::frames_of(&m.shots[1]), vec![0]);
    assert_eq!(crate::shots::frames_of(&m.shots[2]), vec![0, 23, 47]);
    assert!((raea_core_review::fov_of_lens(50.0) - 26.99).abs() < 0.01);
    // a shot with no composition can't be framed, and says what's needed
    let d = dir("framing");
    assert!(crate::shots::framing(&d, &m, &["hold".to_string()], &d).unwrap_err().contains("no composition"));
    assert!(raea_core_brief::make::doctor(&m).iter().any(|w| w.contains("shot `hold` has no composition")));
}

// MK18: the floor plan draws every placed footprint (invented ones dashed); the blockout boxes them from the entrance
#[test]
fn the_plan_draws_a_floor_plan_and_a_blockout() {
    let m: Make = read("make", r#"{"kind": "scene", "references": ["media/ref.png"], "objects": [
        {"id": "desk", "label": "Desk", "dims": [1.6, 0.8, 0.75], "at": [2, 0, 3], "rot": 90, "source": "reference", "hero": true},
        {"id": "plant", "label": "Plant", "h": 1.2, "at": [0.5, 0, 4], "source": "invented"},
        {"id": "pen", "label": "Pen", "max": 0.14}]}"#)
    .unwrap();
    let (placed, b) = crate::floor::extent(&m);
    assert_eq!(placed.len(), 2);
    assert!(b[0] < 0.0 && b[3] > 4.5, "{b:?}");
    let svg = crate::floor::floor_svg(&m).unwrap();
    assert_eq!(svg.matches("stroke-dasharray").count(), 1);
    assert!(svg.contains(">Desk<") && svg.contains(">Plant<") && !svg.contains(">Pen<"));
    // the entrance runs along the front edge (the plan's nearest z), where the blockout's camera stands
    assert!(svg.contains("entrance: the camera looks in from here"));
    let img = crate::sheet::svg_png(&svg).unwrap();
    assert!(img.width() > 400 && img.height() > 400);
    let t = crate::floor::blockout_tsx(&m);
    assert_eq!(t.matches("<Cuboid").count(), 3, "the floor and two boxes");
    assert!(t.contains("half_size={[0.800, 0.375, 0.400]}"), "{t}");
    let none: Make = read("make", r#"{"kind": "scene", "objects": [{"id": "pen", "label": "Pen"}]}"#).unwrap();
    assert!(crate::floor::floor_svg(&none).unwrap_err().contains("no object has an `at`"));
}

// the CLI's positional words skip flags and their values
#[test]
fn positional_words_skip_flags() {
    let a: Vec<String> = ["make", "pick", "phone", "--only", "a,b", "tripo", "--dry", "why not", "proj"].iter().map(|s| s.to_string()).collect();
    assert_eq!(crate::positional(&a, 2), ["phone", "tripo", "why not", "proj"]);
}

#[test]
fn a_quote_reads_as_one_line() {
    let out = "raea: planning\n{\"steps\":[{\"op\":\"media.load\",\"runs_on\":\"load\",\"cents\":0},{\"op\":\"image\",\"runs_on\":\"made:nano_banana_2\",\"cents\":6}],\"cents\":6,\"cap_cents\":100}";
    assert_eq!(crate::kit::quote(out), "made:nano_banana_2 (about 6¢)");
    assert_eq!(crate::kit::quote("not json  at all"), "not json at all");
}

#[test]
fn a_boxy_model_is_fitted_by_its_height_and_turned_to_lie_front_to_back() {
    use raea_core_brief::make::Object;
    let stapler: Object = serde_json::from_value(serde_json::json!({"id": "stapler", "label": "Stapler", "dims": [0.05, 0.17, 0.07]})).unwrap();
    // long across (x) where the plan's is front to back (z): a quarter turn
    assert_eq!(crate::kit::fit_args(&stapler, Some([1.9, 0.72, 0.58])), ["--dims", "0.05,0.17,0.07", "--h", "0.07", "--turn", "90"]);
    // already front to back, or about square: no turn
    assert_eq!(crate::kit::fit_args(&stapler, Some([0.27, 0.12, 0.84])), ["--dims", "0.05,0.17,0.07", "--h", "0.07"]);
    assert_eq!(crate::kit::fit_args(&stapler, None), ["--dims", "0.05,0.17,0.07", "--h", "0.07"]);
    let clock: Object = serde_json::from_value(serde_json::json!({"id": "clock", "label": "Clock", "h": 0.14})).unwrap();
    assert_eq!(crate::kit::fit_args(&clock, Some([1.0, 1.0, 0.5])), ["--h", "0.14"]);
    assert!((crate::kitout::frame_size(&stapler) - 0.17).abs() < 1e-9);
}
