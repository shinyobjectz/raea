//! `raea scene light`: the light on a shot's subject, metered (raea-sys-light-meter; docs/scene.feature, "A light plan").

use serde_json::{Value, json};

use raea_runtime_bundle::{bundle_composition, comp_opts, flag};
use raea_runtime_export::{Frame, all_items, scene_frames_to};

/// One composition's frames, stepped.
fn frames_of(args: &[String], tsx: &std::path::Path, comp: &str) -> Result<(Vec<(i64, Frame)>, f32), String> {
    let dir = tsx.parent().unwrap().to_path_buf();
    let mut opts = comp_opts(args)?;
    opts.composition = Some(comp.to_string()).filter(|c| !c.is_empty());
    let js = std::env::temp_dir().join(format!("raea-light-{}-{comp}.js", std::process::id()));
    bundle_composition(tsx, &js, &opts, None)?;
    let got = scene_frames_to(&js, &dir, 1);
    let _ = std::fs::remove_file(&js);
    let (frames, w, h, _) = got?.ok_or("the composition has no 3D")?;
    Ok((frames, w as f32 / h.max(1) as f32))
}

/// `raea scene light`: the light on a shot's subject, metered (raea-sys-light-meter; docs/scene.feature, "A light plan").
pub(crate) fn cli(args: &[String]) -> Result<String, String> {
    let usage = "raea scene light EDIT.tsx [--composition ID[,ID...]] [--frame N|last] [--part NAME] [--json]";
    let Some(edit) = args.get(2) else { return Err(usage.into()) };
    let tsx = dunce::canonicalize(&std::path::PathBuf::from(edit)).map_err(|e| format!("{edit}: {e}"))?;
    let flag = |k: &str| flag(args, k);
    let comps: Vec<String> = flag("--composition").map(|c| c.split(',').map(str::to_string).collect()).unwrap_or_else(|| vec![String::new()]);
    let part = flag("--part");
    let json_out = args.iter().any(|a| a == "--json");
    let pick = |frames: &[(i64, Frame)], which: &str| -> Result<usize, String> {
        match which {
            "last" => Ok(frames.len() - 1),
            f => {
                let f: i64 = f.parse().map_err(|_| "--frame takes a frame number or last")?;
                frames.iter().position(|(n, _)| *n == f).ok_or_else(|| format!("no frame {f}"))
            }
        }
    };
    let mut shots: Vec<(String, Vec<(i64, Value)>)> = Vec::new();
    for c in &comps {
        let (frames, aspect) = frames_of(args, &tsx, c)?;
        let all = all_items(&frames);
        // one composition: the frame asked for; a sequence: each shot's first and last, for its cuts
        let at: Vec<usize> = if comps.len() == 1 { vec![pick(&frames, flag("--frame").as_deref().unwrap_or("0"))?] } else { vec![0, frames.len() - 1] };
        let mut m = Vec::new();
        for k in at {
            m.push((frames[k].0, raea_sys_light_meter::meter(&all[k], part.as_deref(), aspect).map_err(|e| format!("{c} frame {}: {e}", frames[k].0))?));
        }
        shots.push((c.clone(), m));
    }
    let cuts = raea_sys_light_meter::cuts(&shots);
    if json_out {
        let shots: Vec<Value> = shots.iter().map(|(c, m)| json!({"composition": c, "frames": m.iter().map(|(f, v)| json!({"frame": f, "meter": v})).collect::<Vec<_>>()})).collect();
        return Ok(serde_json::to_string_pretty(&json!({"shots": shots, "cuts": cuts, "findings": raea_sys_light_meter::cut_findings(&cuts)})).unwrap_or_default());
    }
    // each shot's meter, each cut, and the cuts' jumps and drifts as Findings (DR3), counted last
    Ok(raea_sys_light_meter::report(&shots, &cuts))
}
