//! PJ8: a project's files read as compositions and components, and what isn't connected linted.
use super::*;

fn project() -> (tempdir::Dir, PathBuf) {
    let t = tempdir::Dir::new();
    let p = t.0.join("film");
    let w = |rel: &str, text: &str| {
        let f = p.join(rel);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, text).unwrap();
    };
    w(".raea/project", "[project]\n\tid = pj-test\n");
    w("promo.tsx", "import { Composition, registerRoot } from \"raea\";\nimport { Card } from \"./card\";\nconst FPS = 30, edit = { w: 1080 };\nfunction Promo() { return <Card />; }\nregisterRoot(() => <Composition id=\"promo\" component={Promo} width={edit.w} height={1920} fps={FPS} durationInFrames={Math.round(2.5 * FPS) + total()} />);\n");
    w("promo-v2.tsx", "import { Composition, registerRoot } from \"raea\";\nfunction Promo() { return null; }\nregisterRoot(() => <Composition id=\"promo\" component={Promo} width={1080} height={1920} fps={30} durationInFrames={90} />);\n");
    w("teaser.tsx", "import { Composition, registerRoot } from \"raea\";\nfunction T() { return null; }\nregisterRoot(() => (\n  <Composition id=\"promo\" component={T} width={1080} height={1080} fps={30} durationInFrames={60} />\n));\n");
    w("card.tsx", "import Glyph from \"./lib/glyph\";\nimport { gone } from \"./nope\";\nimport { far } from \"../outside\";\nexport function Card() { return <Glyph />; }\n");
    w("lib/glyph.tsx", "export default function Glyph() { return <span />; }\n");
    w("stray.tsx", "export function Stray() { return null; }\n");
    w("shots.tsx", "import { registerRoot } from \"raea\";\nimport { shotComps, SHOTS } from \"./lib/glyph\";\nregisterRoot(() => <>{shotComps(SHOTS)}</>);\n");
    w("bare.tsx", "import { AbsoluteFill } from \"raea\";\nexport default function Bare() { return <AbsoluteFill />; }\n");
    w("bare.render.json", "{\"width\": 1080, \"height\": 1080, \"fps\": 30, \"frames\": [{}, {}, {}]}");
    w("anon.tsx", "export default () => <div />;\n");
    w("broken.tsx", "export default function (\n");
    std::fs::write(t.0.join("outside.tsx"), "export const far = 1;\n").unwrap();
    (t, p)
}

/// A folder removed when the test ends.
mod tempdir {
    pub struct Dir(pub std::path::PathBuf);
    impl Dir {
        pub fn new() -> Dir {
            static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let d = std::env::temp_dir().join(format!("raea-registry-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
            std::fs::create_dir_all(&d).unwrap();
            Dir(dunce::canonicalize(&d).unwrap())
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn compositions_and_components_are_read_from_the_files() {
    let (_t, p) = project();
    let r = scan(&p);
    assert!(r.project);
    let comps: Vec<(&str, &str, How)> = r.compositions.iter().map(|c| (c.id.as_str(), c.file.as_str(), c.how)).collect();
    assert!(comps.contains(&("promo", "promo.tsx", How::Registered)));
    assert!(comps.contains(&("promo", "promo-v2.tsx", How::Registered)));
    assert!(comps.contains(&("bare", "bare.tsx", How::Default)));
    assert!(comps.contains(&("anon", "anon.tsx", How::Default)));
    let promo = r.compositions.iter().find(|c| c.file == "promo.tsx").unwrap();
    assert_eq!((promo.line, promo.component.as_deref(), promo.width.clone(), promo.fps.clone()), (5, Some("Promo"), Some(Prop::Num(1080.0)), Some(Prop::Num(30.0))), "constants read as their values");
    assert_eq!(promo.frames, Some(Prop::Computed("Math.round(2.5 * FPS) + total()".into())), "a call only the render knows");
    let teaser = r.compositions.iter().find(|c| c.file == "teaser.tsx").unwrap();
    assert_eq!(teaser.line, 4, "the <Composition>'s own line, inside a multi-line registerRoot");
    let bare = r.compositions.iter().find(|c| c.file == "bare.tsx").unwrap();
    assert_eq!((bare.width.clone(), bare.frames.clone()), (Some(Prop::Num(1080.0)), Some(Prop::Num(3.0))), "sized from its render's manifest");
    // card is imported by promo, glyph through card (a default export, but imported): both components, used by promo
    let comps: Vec<(&str, Vec<&str>)> = r.components.iter().map(|c| (c.file.as_str(), c.used_by.iter().map(String::as_str).collect())).collect();
    assert_eq!(comps, [("card.tsx", vec!["promo.tsx"]), ("lib/glyph.tsx", vec!["promo.tsx"])]);
}

#[test]
fn what_isnt_connected_is_linted_with_its_place_and_fix() {
    let (_t, p) = project();
    let r = scan(&p);
    let has = |rule: &str, file: &str, line: usize| r.lint.iter().any(|l| l.rule == rule && l.file == file && l.line == line);
    assert!(has("orphan", "stray.tsx", 1));
    assert!(!r.lint.iter().any(|l| l.rule == "orphan" && ["card.tsx", "lib/glyph.tsx", "promo.tsx"].contains(&l.file.as_str())));
    assert!(has("unresolved", "card.tsx", 2));
    assert!(has("outside", "card.tsx", 3));
    assert!(has("broken", "broken.tsx", 1));
    // promo's versions share an id; teaser takes it too
    assert!(has("duplicate-id", "teaser.tsx", 4));
    assert!(r.lint.iter().filter(|l| l.rule == "duplicate-id").all(|l| l.says.contains("teaser") || l.file == "teaser.tsx"));
    assert!(has("computed", "promo.tsx", 5));
    // a default export with a render is registered by --fix; a nameless one says how
    let bare = r.lint.iter().find(|l| l.rule == "unregistered" && l.file == "bare.tsx").unwrap();
    assert!(bare.action.is_some());
    let anon = r.lint.iter().find(|l| l.rule == "unregistered" && l.file == "anon.tsx").unwrap();
    assert!(anon.action.is_none() && anon.fix.contains("name the default export"));
    // a registerRoot whose compositions a helper makes is no orphan: it says the scan can't list them
    assert!(!r.lint.iter().any(|l| l.rule == "orphan" && l.file == "shots.tsx"));
    assert!(has("computed-root", "shots.tsx", 1));
    // as Findings (DR3): about the file at its line, a fix --fix applies saying so, the count last
    let stray = r.lint.iter().find(|l| l.rule == "orphan" && l.file == "stray.tsx").unwrap().finding();
    assert_eq!((stray.about[0].key.as_deref(), stray.fix.as_deref().map(|f| f.starts_with("register it"))), (Some("stray.tsx:1"), Some(true)));
    let bare = bare.finding();
    assert_eq!((bare.about[0].key.as_deref(), bare.fix.as_deref().map(|f| f.starts_with("raea doctor --fix registers it"))), (Some("bare.tsx:1"), Some(true)), "a fix --fix applies says so");
    assert!(r.text().lines().last().unwrap().starts_with("raea project scan: 0 errors, "));
}

#[test]
fn a_fix_registers_a_default_export() {
    let (_t, p) = project();
    let r = scan(&p);
    for l in r.lint.iter().filter(|l| l.action.is_some()) {
        lint::apply(l.action.as_ref().unwrap()).unwrap();
    }
    let bare = std::fs::read_to_string(p.join("bare.tsx")).unwrap();
    assert!(bare.starts_with("import { Composition, registerRoot } from \"raea\";\nimport { AbsoluteFill }"), "{bare}");
    assert!(bare.ends_with("registerRoot(() => <Composition id=\"bare\" component={Bare} width={1080} height={1080} fps={30} durationInFrames={3} />);\n"), "{bare}");
    let again = scan(&p);
    assert_eq!(again.compositions.iter().find(|c| c.file == "bare.tsx").unwrap().how, How::Registered);
    assert!(!again.lint.iter().any(|l| l.rule == "unregistered" && l.file == "bare.tsx"));
}

#[test]
fn a_folder_outside_any_project_says_so() {
    let t = tempdir::Dir::new();
    std::fs::write(t.0.join("a.tsx"), "export default function A() { return null; }\n").unwrap();
    let r = scan(&t.0);
    assert!(!r.project);
    assert!(r.lint.iter().any(|l| l.rule == "not-a-project"));
}

#[test]
fn versions_share_a_stem() {
    assert_eq!(stem("promo-v12.tsx"), "promo");
    assert_eq!(stem("dir/my-video.tsx"), "my-video");
    assert_eq!(stem("x-v.tsx"), "x-v");
}

/// A real folder's registry, printed: `RAEA_SCAN_DIR=~/Studio/x cargo test -p raea-app-registry real_folder -- --ignored --nocapture`.
#[test]
#[ignore]
fn real_folder() {
    let Some(dir) = std::env::var_os("RAEA_SCAN_DIR") else { return };
    let t = std::time::Instant::now();
    let r = scan(Path::new(&dir));
    println!("{}\n({} ms)", r.text(), t.elapsed().as_millis());
}

#[test]
fn a_project_in_the_layout_reaches_shared_and_says_whats_out_of_place() {
    // docs/layout.feature LY2, LY3: @shared/ resolves and isn't "outside"; a relative climb into shared/ is told its
    // @shared/ name; a composition outside compositions/ is misplaced; a project from before the layout is said once
    let t = tempdir::Dir::new();
    let w = |rel: &str, text: &str| {
        let f = t.0.join(rel);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, text).unwrap();
    };
    w(".raea/workspace", "[workspace]\n\tname = W\n");
    w("shared/components/desk.tsx", "export function Desk() { return <div />; }\n");
    w("projects/a/.raea/project", "[project]\n\tid = pj-a\n");
    w("projects/a/compositions/main.tsx", "import { Desk } from \"@shared/components/desk\";\nimport { Leg } from \"../components/leg\";\nimport { Desk as D2 } from \"../../../shared/components/desk\";\nexport default () => <><Desk /><Leg /><D2 /></>;\n");
    w("projects/a/components/leg.tsx", "export function Leg() { return <i />; }\n");
    w("projects/a/loose.tsx", "export default () => <b />;\n");
    let r = scan(&t.0.join("projects/a"));
    let says = |rule: &str| r.lint.iter().filter(|l| l.rule == rule).map(|l| format!("{} {}: {}", l.file, l.says, l.fix)).collect::<Vec<_>>();
    assert!(says("unresolved").is_empty(), "{:?}", says("unresolved"));
    let outside = says("outside");
    assert_eq!(outside.len(), 1, "{outside:?}");
    assert!(outside[0].contains("\"@shared/components/desk\""), "{outside:?}");
    assert_eq!(says("misplaced"), ["loose.tsx a composition outside compositions/: move it into compositions/ (`raea project tidy` moves it and its imports)"]);
    assert!(!r.lint.iter().any(|l| l.rule == "orphan" && l.file == "components/leg.tsx"));

    w("projects/old/.raea/project", "[project]\n\tid = pj-old\n");
    w("projects/old/promo.tsx", "import { Card } from \"./card\";\nexport default () => <Card />;\n");
    w("projects/old/card.tsx", "export function Card() { return <i />; }\n");
    let r = scan(&t.0.join("projects/old"));
    let old: Vec<_> = r.lint.iter().filter(|l| l.rule == "old-layout").collect();
    assert!(old.len() == 1 && old[0].says.contains("2 files loose") && old[0].fix.contains("raea project tidy"), "{:?}", r.lint);
    assert!(!r.lint.iter().any(|l| l.rule == "misplaced"));
}
