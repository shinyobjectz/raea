//! `raea scene stage EDIT.tsx [--composition ID] [--from N] [--to N] [--json]`: every frame's scene stepped without
//! drawing and held to the stage checks a render runs at its end (raea-sys-stage, docs/stage-guards.feature SG10-SG14):
//! 3D type that can't be read, parts that snap, a camera that lurches or reverses, frames with nothing composed in
//! them. Errors fail it, as they fail a render, so it's the cheap look before one. It lays out no page, so what the
//! page holds waits for a render: its words over the parts (type-over-part), a Label's part as the shot's subject, and
//! a frame its words or pictures fill, which reads empty here and is said as a warning, not failed (SG11).
//! `--clearance M` holds the camera M metres off every solid part on every frame (0.45 for a carried camera, MK16).

use serde_json::json;

use raea_core_facts::{Level, finding};
use raea_runtime_bundle::{bundle_composition, comp_opts, flag};
use raea_runtime_export::{all_items, scene_frames_to};

pub(crate) fn cli(args: &[String]) -> Result<String, String> {
    let usage = "raea scene stage EDIT.tsx [--composition ID] [--from N] [--to N] [--clearance M] [--json]";
    let Some(edit) = args.get(2) else { return Err(usage.into()) };
    let tsx = dunce::canonicalize(&std::path::PathBuf::from(edit)).map_err(|e| format!("{edit}: {e}"))?;
    let dir = tsx.parent().unwrap().to_path_buf();
    let frame = |k: &str| flag(args, k).map(|v| v.parse::<i64>().map_err(|_| format!("{k} takes a frame number"))).transpose();
    let (from, to) = (frame("--from")?.unwrap_or(0), frame("--to")?);
    let js = std::env::temp_dir().join(format!("raea-stage-{}.js", std::process::id()));
    bundle_composition(&tsx, &js, &comp_opts(args)?, None)?;
    let got = scene_frames_to(&js, &dir, 1);
    let _ = std::fs::remove_file(&js);
    let (frames, w, h, fps) = got?.ok_or("the composition has no 3D")?;
    let frames: Vec<_> = frames.into_iter().filter(|(f, _)| *f >= from && to.is_none_or(|t| *f <= t)).collect();
    let (Some(first), Some(last)) = (frames.first().map(|f| f.0), frames.last().map(|f| f.0)) else { return Err(format!("no frames from {from}")) };
    let nums: Vec<i64> = frames.iter().map(|(f, _)| *f).collect();
    let items = all_items(&frames);
    let mut fs = raea_sys_stage::check(&items, &nums, w, h, fps, "raea scene stage");
    // the camera held off every solid part, when asked: the number after the flag, else a carried camera's 0.45 m
    if let Some(v) = flag(args, "--clearance") {
        let min = v.parse::<f32>().ok().filter(|m| *m >= 0.0).ok_or_else(|| format!("--clearance takes metres (0.45 for a carried camera), got {v:?}"))?;
        let mut c = crate::clearance::clearance(&items, &nums, w, h, fps, min, "raea scene stage");
        c.append(&mut fs);
        fs = c;
    }
    for f in fs.iter_mut().filter(|f| f.check == "frame-empty" && f.level == Level::Error) {
        f.level = Level::Warn;
        f.says.push_str(" (no page is laid out here: if its words or pictures fill these frames, a render passes them)");
    }
    let text = if args.iter().any(|a| a == "--json") {
        serde_json::to_string_pretty(&json!({"frames": [first, last], "findings": fs})).unwrap_or_default()
    } else {
        let note = "the page's checks (its words over parts, a Label's subject) run in raea render";
        fs.iter().map(raea_core_facts::Finding::line).chain([format!("frames {first} to {last}: {} ({note})", finding::count(&fs))]).collect::<Vec<_>>().join("\n")
    };
    if fs.iter().any(|f| f.level == Level::Error) {
        println!("{text}");
        std::process::exit(1);
    }
    Ok(text)
}
