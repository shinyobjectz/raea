//! `raea scene lint|explain|light|stage`: a composition's frames stepped without drawing and handed to raea-sys-contact
//! (what each part rests on, and parts that clip), raea-sys-light-meter (the light on a subject) and raea-sys-stage (3D
//! type that can't be read, parts that snap, a camera that lurches, empty frames: docs/stage-guards.feature).

use serde_json::json;

use raea_runtime_bundle::{bundle_composition, comp_opts};
use raea_runtime_export::{frame_of, items};

// how near the camera passes to each solid part, every frame (`--clearance M`, MK16)
pub mod clearance;
mod light;
mod stage;

pub const USAGE: &str = "raea scene lint|explain|light|stage EDIT.tsx [--composition ID] [--frame N] [--part NAME] [--from N] [--to N] [--clearance M] [--json]";

/// `raea scene lint|explain EDIT.tsx [--composition ID] [--frame N] [--json]`: the frame's parts measured
/// (frame 0 by default). `lint` says the faults (errors fail it: floating, sunk, clipping; warnings: in the air,
/// tilted); `explain` says what every part rests on. --json prints the findings (each part's support, gap and the y
/// that fixes it) instead. `light` and `stage` are light.rs's and stage.rs's.
pub fn cli(args: &[String]) -> Result<String, String> {
    let usage = USAGE;
    let (Some(mode), Some(edit)) = (args.get(1).map(String::as_str), args.get(2)) else { return Err(usage.into()) };
    match mode {
        "light" => return light::cli(args),
        "stage" => return stage::cli(args),
        "lint" | "explain" => {}
        _ => return Err(usage.into()),
    }
    let tsx = dunce::canonicalize(&std::path::PathBuf::from(edit)).map_err(|e| format!("{edit}: {e}"))?;
    let dir = tsx.parent().unwrap().to_path_buf();
    let frame = frame_of(args)?;
    let js = std::env::temp_dir().join(format!("raea-scene-{}.js", std::process::id()));
    bundle_composition(&tsx, &js, &comp_opts(args)?, None)?;
    // and the frame after it, to see what moves
    let got = raea_runtime_export::scene_frames(&js, &dir, frame + 2, false, &[frame, frame + 1]);
    let _ = std::fs::remove_file(&js);
    let (frames, ..) = got?.ok_or("the composition has no 3D")?;
    let k = frames.iter().position(|(f, _)| *f == frame).ok_or_else(|| format!("no frame {frame}"))?;
    // the frame, and the next (or the one before, at the end) to see what moves
    let other = if k + 1 < frames.len() { k + 1 } else { k.saturating_sub(1) };
    let (mut found, errors) = raea_sys_contact::lint(&items(&frames[k].1), &items(&frames[other].1));
    // and the Blender steps its models call for (a format raea can't load, triangles over budget for the distance)
    let (more, unloadable) = raea_sys_contact::assets(&items(&frames[k].1));
    found.extend(more);
    let (errors, json_out) = (errors + unloadable, args.iter().any(|a| a == "--json"));
    let parts = found.iter().filter(|v| v["check"] != "clips" && v["check"] != "tilted" && v["check"] != "skipped").count();
    if mode == "lint" {
        // its faults as Findings (DR3), worst first, every one shown, the count last; errors fail it, with nothing after
        let fs = raea_core_facts::finding::ranked(raea_sys_contact::lint_findings(&found, frame, "raea scene lint"));
        let text = if json_out { serde_json::to_string_pretty(&json!({"frame": frame, "findings": fs})).unwrap_or_default() } else { fs.iter().map(raea_core_facts::Finding::line).chain([format!("frame {frame}: {parts} parts; {}", raea_core_facts::finding::count(&fs))]).collect::<Vec<_>>().join("\n") };
        if errors > 0 {
            println!("{text}");
            std::process::exit(1);
        }
        return Ok(text);
    }
    // explain: every part's support
    let t: Vec<String> = found.iter().map(|v| format!("{:<8} {}", v["level"].as_str().unwrap_or(""), v["says"].as_str().unwrap_or(""))).collect();
    Ok(if json_out { serde_json::to_string_pretty(&json!({"frame": frame, "errors": errors, "found": found})).unwrap_or_default() } else { format!("{}\nframe {frame}: {parts} parts, {errors} errors", t.join("\n")) })
}
