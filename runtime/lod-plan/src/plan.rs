//! The levels a job's plan gives its objects (docs/make.feature MK22): `lod` on the objects of `make` in the brief.ts
//! at or above a composition, inside its raea project. Read only when the brief's words hold a `lod` at all, since
//! reading it runs the module (`raea spec brief.ts --as brief`, a child process: this crate carries no TS engine);
//! read once a brief's version a process. A brief that doesn't load plans nothing: a render never fails on it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use serde_json::Value;

/// The last brief read: its path, when it last changed, and its levels.
static READ: Mutex<Option<(PathBuf, Option<SystemTime>, HashMap<String, u8>)>> = Mutex::new(None);

/// The composition a command was given: its first word naming a .tsx or .jsx file (`raea render EDIT.tsx ...`).
pub fn composition(words: impl IntoIterator<Item = String>) -> Option<PathBuf> {
    for w in words {
        if w.ends_with(".tsx") || w.ends_with(".jsx") {
            return Some(PathBuf::from(w));
        }
    }
    None
}

/// The plan's levels for the composition this process was asked to render, read from its own command line: a render's
/// end checks are made deep in the export, which isn't told where the composition is. None when it names none.
pub fn here() -> HashMap<String, u8> {
    let dir = composition(std::env::args().skip(1)).and_then(|c| std::path::absolute(c).ok()).and_then(|c| c.parent().map(Path::to_path_buf));
    dir.map(|d| lods(&d)).unwrap_or_default()
}

/// The brief.ts at or above `dir` in its project whose words plan a level.
fn brief(dir: &Path) -> Option<PathBuf> {
    let dir = dunce::canonicalize(dir).ok()?;
    let project = raea_core_facts::record::up(&dir, ".raea/project");
    for d in dir.ancestors() {
        let p = d.join("brief.ts");
        if p.is_file() {
            return std::fs::read_to_string(&p).is_ok_and(|t| t.contains("lod")).then_some(p);
        }
        if project.as_deref() == Some(d) {
            break;
        }
    }
    None
}

/// The levels in a brief as `raea spec --as brief` prints it: each of make's objects with a `lod`, by its id.
pub fn of_brief(v: &Value) -> HashMap<String, u8> {
    let b = v.get("brief").unwrap_or(v);
    let objects = b["make"]["objects"].as_array().into_iter().flatten();
    objects.filter_map(|o| Some((o["id"].as_str()?.to_string(), o["lod"].as_u64()?.min(crate::COARSEST as u64) as u8))).collect()
}

/// The plan's levels for a composition in `dir`: object id to level, empty when no brief above it plans any.
pub fn lods(dir: &Path) -> HashMap<String, u8> {
    let Some(brief) = brief(dir) else { return HashMap::new() };
    let changed = std::fs::metadata(&brief).and_then(|m| m.modified()).ok();
    if let Some((p, at, plan)) = READ.lock().unwrap().as_ref() {
        if *p == brief && *at == changed {
            return plan.clone();
        }
    }
    let read = || {
        let out = std::process::Command::new(std::env::current_exe().ok()?).arg("spec").arg(&brief).args(["--as", "brief"]).env("RAEA_RECORD", "off").output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        Some(of_brief(&serde_json::from_str(&text[text.find('{')?..]).ok()?))
    };
    let plan = read().unwrap_or_default();
    *READ.lock().unwrap() = Some((brief, changed, plan.clone()));
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_briefs_objects_with_a_level_are_the_plan() {
        let v = json!({"brief": {"make": {"kind": "scene", "objects": [{"id": "kiosk", "lod": 3}, {"id": "desk"}, {"id": "sign", "lod": 9}]}}});
        let plan = of_brief(&v);
        assert_eq!((plan.len(), plan["kiosk"], plan["sign"]), (2, 3, 4));
        assert!(of_brief(&json!({"title": "no job"})).is_empty());
        // as `raea spec brief.ts --as brief` prints it: the brief itself, every field given, a level left out null
        let printed = json!({"title": "Kiosk square", "make": {"kind": "scene", "objects": [{"id": "kiosk", "lod": 3, "hero": null}, {"id": "desk", "lod": null}]}});
        assert_eq!(of_brief(&printed), HashMap::from([("kiosk".to_string(), 3)]));
    }

    #[test]
    fn the_composition_is_the_commands_first_tsx() {
        let words = |w: &[&str]| composition(w.iter().map(|s| s.to_string()));
        assert_eq!(words(&["render", "shots/walk.tsx", "-o", "out/walk.mp4"]), Some(PathBuf::from("shots/walk.tsx")));
        assert_eq!(words(&["check", "out/walk.mp4"]), None);
    }

    #[test]
    fn a_folder_with_no_brief_planning_a_level_plans_nothing() {
        let d = std::env::temp_dir().join(format!("raea-lod-plan-{}", std::process::id()));
        std::fs::create_dir_all(d.join(".raea")).unwrap();
        std::fs::write(d.join(".raea/project"), "").unwrap();
        assert!(lods(&d).is_empty());
        std::fs::write(d.join("brief.ts"), "export default brief({ title: \"x\" })").unwrap();
        assert!(brief(&d).is_none(), "a brief whose words plan no level isn't run");
        let _ = std::fs::remove_dir_all(&d);
    }
}
