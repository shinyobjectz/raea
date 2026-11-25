//! `raea bench denoise [--backend raea|metalfx|dlss|off] [CASE...]` (docs/denoise.feature DN0): every case on every
//! denoiser against its converged truth, so each backend has numbers to meet and MetalFX's on the Mac are the first bar.
//!
//! The cases are raea light check's scenes (raea_light::scenes: name, dir, file, composition, stills, shot, live,
//! size, spp), from bench/denoise/cases.json by default (--cases FILE; a relative dir is the file's own folder's), each
//! with "events": [{ "at": FRAME, "what": "disocclusion" | "light" }] inside its shot for the lag. The truth is the path
//! tracer held RAEA_LIGHT_SPP draws a frame (raea_light::reference, kept between runs unless --fresh). Each backend
//! draws a case's stills and shot with 16 frames before each (raea_light::with_before), so its history is its own,
//! then is scored (metrics.rs) and, where the case says "live", played at its size for a frame's cost. The report is
//! DIR/report.json and a table (report.rs); DIR is --o, else runs/ beside the cases file.

pub mod metrics;
pub mod report;

use raea_light::{metric, Scene};
use raea_runtime_bundle::{flag, R};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const USAGE: &str = "raea bench denoise [--backend raea,metalfx,dlss,off] [CASE...] [--cases FILE] [-o DIR] [--spp N] [--fresh]";

/// The backends, in the order DN8's switch names them
pub const BACKENDS: [&str; 4] = ["raea", "metalfx", "dlss", "off"];

/// The switches a backend draws with, or why it can't run here yet. Every one traces with Solari (RAEA_LIGHT).
pub fn switches(backend: &str) -> Result<Vec<(&'static str, String)>, String> {
    let solari = |denoise: &str| Ok(vec![("RAEA_LIGHT", "solari".to_string()), ("RAEA_SOLARI_DENOISE", denoise.to_string())]);
    match backend {
        "off" => solari("0"),
        "metalfx" if cfg!(target_os = "macos") => solari("1"),
        "metalfx" => Err("MetalFX runs on the Mac only".into()),
        "raea" => Err("not built yet: raea's own denoiser is DN1-DN8".into()),
        "dlss" => Err("not wired yet (DN8): DLSS-RR needs the `dlss` build on an NVIDIA RTX machine".into()),
        b => Err(format!("{b} isn't a backend ({})", BACKENDS.join(", "))),
    }
}

/// A case: its scene, and the frames in its shot where its history is broken on purpose
pub struct Case {
    pub scene: Scene,
    pub events: Vec<(i64, String)>,
}

pub fn cases(path: &Path) -> R<Vec<Case>> {
    let base = path.parent().unwrap_or(Path::new("."));
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?).map_err(|e| format!("{}: {e}", path.display()))?;
    let scenes = raea_light::scenes(path)?;
    scenes
        .into_iter()
        .zip(v["scenes"].as_array().into_iter().flatten())
        .map(|(mut scene, s)| {
            if scene.dir.is_relative() {
                scene.dir = base.join(&scene.dir);
            }
            let events: Vec<(i64, String)> = s["events"].as_array().into_iter().flatten().filter_map(|e| Some((e["at"].as_i64()?, e["what"].as_str().unwrap_or("event").to_string()))).collect();
            if let Some(&(at, _)) = events.iter().find(|(at, _)| !scene.shot.is_some_and(|(a, b)| (a + 1..=b).contains(at))) {
                return Err(format!("{}: {}'s event at frame {at} isn't inside its shot, after its first frame", path.display(), scene.name));
            }
            Ok(Case { scene, events })
        })
        .collect()
}

fn round(x: f32, k: f32) -> Value {
    json!(((x * k).round() / k) as f64)
}

/// The stills' numbers' mean, rounded
fn over(v: &[f32], k: f32) -> Value {
    round(v.iter().sum::<f32>() / v.len().max(1) as f32, k)
}

/// One case: its truth, then each backend against it
fn run(case: &Case, backends: &[String], spp: u32, out: &Path, fresh: bool) -> R<Value> {
    let s = &case.scene;
    let root = out.join(&s.name);
    let spp = s.spp.unwrap_or(spp);
    let shot = s.shot_frames();
    let mut report = json!({ "name": s.name, "stills": s.stills, "shot": shot, "events": case.events.iter().map(|(at, what)| json!({"at": at, "what": what})).collect::<Vec<_>>(), "backends": {} });
    let truth = match &s.reference {
        Err(why) => {
            report["truth"] = json!({ "made": false, "why": why });
            None
        }
        Ok(()) => {
            let (_, how) = raea_light::reference(s, spp, &s.stills, &root.join("truth"), fresh)?;
            let stills = raea_light::frames_with(&root.join("truth"), 0..s.stills.len(), metrics::load)?;
            report["truth"] = json!({ "made": true, "stills": how });
            let shot = match &shot {
                Some(at) => {
                    let (frames, how) = raea_light::reference(s, spp, at, &root.join("truth-shot"), fresh)?;
                    report["truth"]["shot"] = how;
                    Some(frames)
                }
                None => None,
            };
            Some((stills, shot))
        }
    };
    for b in backends {
        let env = match switches(b) {
            Ok(env) => env,
            Err(why) => {
                report["backends"][b] = json!({ "ran": false, "why": why });
                continue;
            }
        };
        let dir = root.join(b);
        let (all, place) = raea_light::with_before(&s.stills);
        let secs = raea_light::render_with(s, b, &env, &all, &dir)?;
        let mut r = json!({ "ran": true, "seconds_a_still": round(secs as f32 / all.len().max(1) as f32, 100.0) });
        if let Some((refs, ref_shot)) = &truth {
            let got = raea_light::frames_with(&dir, place, metrics::load)?;
            let per = |f: fn(&metrics::Rgb, &metrics::Rgb) -> Result<f32, String>| got.iter().zip(refs).map(|(a, t)| f(a, t)).collect::<R<Vec<f32>>>();
            let (flip, rmse, bias, blotch) = (per(metrics::flip)?, per(metrics::rmse)?, per(metrics::bias)?, per(metrics::blotch)?);
            r["flip"] = over(&flip, 10000.0);
            r["rmse"] = over(&rmse, 10000.0);
            r["bias_stops"] = over(&bias, 1000.0);
            r["blotch_stops"] = over(&blotch, 1000.0);
            r["per_still"] = json!((0..got.len()).map(|k| json!({ "flip": round(flip[k], 10000.0), "rmse": round(rmse[k], 10000.0), "bias": round(bias[k], 1000.0), "blotch": round(blotch[k], 1000.0) })).collect::<Vec<_>>());
            if let (Some(at), Some(ref_shot)) = (&shot, ref_shot) {
                let sdir = root.join(format!("{b}-shot"));
                let (all, place) = raea_light::with_before(at);
                raea_light::render_with(s, b, &env, &all, &sdir)?;
                let frames = raea_light::frames(&sdir, place)?;
                r["flicker_stops"] = json!(metric::error_flicker(&frames, ref_shot)?.map(|f| round(f, 1000.0)));
                r["spike_stops"] = json!(metrics::spike(&frames, ref_shot).map(|f| round(f, 1000.0)));
                let errors: Vec<f32> = frames.iter().zip(ref_shot).map(|(a, t)| metric::distance(a, t).map(|d| d.mean)).collect::<R<_>>()?;
                r["shot_error_stops"] = json!(errors.iter().map(|&e| round(e, 1000.0)).collect::<Vec<_>>());
                let first = at[0];
                r["lag"] = json!(case.events.iter().map(|(ev, what)| json!({ "at": ev, "what": what, "frames": metrics::lag(&errors, (ev - first) as usize) })).collect::<Vec<_>>());
            }
        }
        r["live"] = match &s.live {
            Ok(()) => raea_light::live_with(s, b, &env, s.size, &root)?,
            Err(why) => json!({ "measured": false, "why": why }),
        };
        report["backends"][b] = r;
    }
    Ok(report)
}

/// The positional CASE names: what isn't a flag or a flag's value
fn names(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-o" | "--cases" | "--backend" | "--spp" => {
                it.next();
            }
            a if a.starts_with('-') => {}
            a => out.push(a.to_string()),
        }
    }
    out
}

/// `raea bench denoise ...` (args after `bench`)
pub fn cli(args: &[String]) -> R<String> {
    if args.first().map(String::as_str) != Some("denoise") {
        return Err(format!("usage: {USAGE}"));
    }
    let args = &args[1..];
    let file = PathBuf::from(flag(args, "--cases").unwrap_or_else(|| "bench/denoise/cases.json".into()));
    let list = cases(&file)?;
    let out = flag(args, "-o").map_or_else(|| file.parent().unwrap_or(Path::new(".")).join("runs"), PathBuf::from);
    let backends = raea_light::list_flag(args, "--backend", &BACKENDS, "a backend")?;
    let spp = raea_light::spp(args)?;
    let only = names(args);
    if let Some(n) = only.iter().find(|n| !list.iter().any(|c| &c.scene.name == *n)) {
        return Err(format!("no case named {n} in {} ({})", file.display(), list.iter().map(|c| c.scene.name.as_str()).collect::<Vec<_>>().join(", ")));
    }
    let fresh = args.iter().any(|a| a == "--fresh");
    let mut reports = Vec::new();
    for c in list.iter().filter(|c| only.is_empty() || only.contains(&c.scene.name)) {
        eprintln!("raea bench denoise: {} ...", c.scene.name);
        reports.push(run(c, &backends, spp, &out, fresh)?);
    }
    report::write(&reports, &out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_skip_flags_and_their_values() {
        let a: Vec<String> = ["doorway", "--backend", "off", "-o", "x", "mirror", "--fresh"].map(String::from).to_vec();
        assert_eq!(names(&a), vec!["doorway", "mirror"]);
    }

    #[test]
    fn backends_that_cant_run_say_why() {
        assert!(switches("raea").unwrap_err().contains("DN1"));
        assert!(switches("dlss").unwrap_err().contains("NVIDIA"));
        assert_eq!(switches("off").unwrap()[1], ("RAEA_SOLARI_DENOISE", "0".to_string()));
        assert!(switches("nope").is_err());
    }

    #[test]
    fn a_case_file_reads_its_events_and_its_folder() {
        let d = std::env::temp_dir().join(format!("raea-dn0-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("cases.json");
        std::fs::write(&f, r#"{"scenes":[{"name":"a","dir":"cases","file":"a.tsx","composition":"a","stills":[3],"shot":[0,9],"events":[{"at":5,"what":"light"}]}]}"#).unwrap();
        let c = cases(&f).unwrap();
        assert_eq!(c[0].events, vec![(5, "light".to_string())]);
        assert_eq!(c[0].scene.dir, d.join("cases"));
        std::fs::write(&f, r#"{"scenes":[{"name":"a","dir":"cases","file":"a.tsx","composition":"a","stills":[3],"shot":[0,9],"events":[{"at":12}]}]}"#).unwrap();
        assert!(cases(&f).is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
