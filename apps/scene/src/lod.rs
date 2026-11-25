//! `raea scene lod EDIT.tsx [--composition ID] [--from N] [--to N] [--json]` (docs/make.feature MK23): every part's
//! planned detail beside how large the frames show it. A part's plan is its own `lod` (0 the hero to 4 far), else its
//! name's on the job's plan (`lod` on brief.ts's make objects); what a frame shows is how much of it the part's box
//! spans (raea-runtime-lod-plan). The table lists the planned parts first, the ones seen closer than planned on top,
//! then the parts nobody planned by how large they get, so a part forgotten since the plan was written is found by
//! reading down it. A part seen two or more levels finer than planned fails it, as it fails a render.

use raea_core_facts::{Finding, Level, finding};
use raea_runtime_lod_plan::{ordered, table};
use serde_json::json;

pub(crate) fn cli(args: &[String]) -> Result<String, String> {
    let crate::stage::Stepped { items, nums, size: (w, h, fps), dir } = crate::stage::stepped(args, "raea scene lod EDIT.tsx [--composition ID] [--from N] [--to N] [--json]")?;
    let (first, last) = (nums[0], nums[nums.len() - 1]);
    let plan = raea_runtime_lod_plan::plan::lods(&dir);
    let mut unseen: Vec<String> = plan.keys().cloned().collect();
    let (seen, fs) = raea_runtime_lod_plan::lods(&items, &nums, w, h, fps, "raea scene lod", plan);
    let seen = ordered(seen);
    // a level the plan gives a name no part in these frames goes by holds nothing: said, so the part gets its name
    unseen.retain(|n| !seen.iter().any(|s| s.planned.is_some() && (s.name == *n || s.name.starts_with(&format!("{n} (lod ")))));
    unseen.sort();
    let text = if args.iter().any(|a| a == "--json") {
        let parts: Vec<_> = seen.iter().map(|s| json!({"part": s.name, "planned": s.planned, "seen": s.level(), "largest": s.finest.map(|f| f.1), "at": s.finest.map(|f| f.0), "frames": s.frames, "closer": s.closer})).collect();
        serde_json::to_string_pretty(&json!({"frames": [first, last], "parts": parts, "unseen": unseen, "findings": fs})).unwrap_or_default()
    } else {
        let levels = "levels, by how much of the frame a part spans: 0 hero 50%+, 1 featured 20%+, 2 dressing 8%+, 3 background 2%+, 4 far";
        let none = seen.iter().filter(|s| s.planned.is_none()).count();
        let missing = (!unseen.is_empty()).then(|| format!("planned in brief.ts, but no part of that name is in these frames (name the part as its object's id, or its plan holds nothing): {}", unseen.join(", ")));
        let sum = format!("frames {first} to {last}: {} parts, {none} with no plan; {}", seen.len(), finding::count(&fs));
        table(&seen).into_iter().chain(fs.iter().map(Finding::line)).chain(missing).chain([levels.to_string(), sum]).collect::<Vec<_>>().join("\n")
    };
    if fs.iter().any(|f| f.level == Level::Error) {
        println!("{text}");
        std::process::exit(1);
    }
    Ok(text)
}
