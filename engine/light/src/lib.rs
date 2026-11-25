//! `raea light check SCENES.json -o DIR` (docs/realism.feature LT1): the bar raealism is measured by. Each scene
//! is rendered on each light path (RAEA_LIGHT, engine/render/raytrace force.rs) and against its reference, the path
//! tracer held RAEA_LIGHT_SPP draws a frame: how far each path's stills are from it in stops (mean, bias, worst tile),
//! how much its moving shot flickers against the shot path-traced (each path drawing the 16 frames before a still
//! or the shot first, unmeasured, so what keeps a past has its own), and what a frame costs. The numbers go to
//! DIR/report.json and a table.
//!
//! A scene in SCENES.json: { "name", "dir" (its project, where its renders run; ~ is the home folder), "file" (in dir),
//! "composition", "stills": [frame, ...], "shot": [first, last] (optional; a few frames are enough, each is
//! path-traced), "spp": N (optional: its reference's samples a pixel, in place of --spp), "reference": false (optional: its
//! reference can't be made yet, and why, in "why"), "live": true (optional: `raea play FILE` plays this composition,
//! its file's first, so its live frame time is measured; false with "live_why" says why not), "size": [w, h] (the
//! live stage, 1920x1080 by default) }.

pub mod metric;
pub mod steady;

use raea_runtime_bundle::{flag, R};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// The light paths that trace, whose light is built over frames and can move with nothing else moving.
const TRACED: [&str; 2] = ["solari", "gi"];
/// How many times a traced path's still is held for `steady`.
const HELD: usize = 96;
pub const USAGE: &str = "raea light check SCENES.json -o DIR [--scene NAME] [--paths raster,traced-ao,solari,gi] [--spp N] [--fresh] | raea light steady EDIT.tsx --frame N [--composition ID] [--frames 96] [--run M] [--max 1.5] -o DIR | raea light steady DIR [--max 1.5]   (steady: frame N held, and whether its light holds still)";

/// The light paths a check compares: RAEA_LIGHT's, and traced-ao, raster with systems/rt's traced occlusion in place
/// of SSAO (on a camera with <AmbientOcclusion>; on any other it draws as raster).
const PATHS: [&str; 4] = ["raster", "traced-ao", "solari", "gi"];

/// The switches a path is drawn with (the reference is `pathtracer`). `gi` is raster with raealism's probes traced
/// (systems/rt gi.rs; RAEA_GI_RAYS and the rest pass through from the check's own environment).
fn switches(path: &str) -> Vec<(&'static str, String)> {
    match path {
        "traced-ao" => vec![("RAEA_LIGHT", "raster".into()), ("RAEA_TRACED_AO", "0.5".into())],
        "gi" => vec![("RAEA_LIGHT", "raster".into()), ("RAEA_GI", "1".into())],
        p => vec![("RAEA_LIGHT", p.to_string())],
    }
}

/// A scene of SCENES.json (the module's doc); raea bench denoise (engine/render/denoise/bench) reads its cases the same way.
pub struct Scene {
    pub name: String,
    pub dir: PathBuf,
    pub file: String,
    pub composition: String,
    pub stills: Vec<i64>,
    pub shot: Option<(i64, i64)>,
    pub reference: Result<(), String>,
    pub live: Result<(), String>,
    pub size: (i64, i64),
    pub spp: Option<u32>,
}

/// How long a live play runs, in seconds, its first second not counted.
const LIVE_SECONDS: &str = "6";

fn home(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), raea_io_home::user_home()) {
        (Some(rest), Some(h)) => PathBuf::from(h).join(rest),
        _ => PathBuf::from(p),
    }
}

impl Scene {
    /// Every frame of its shot, first to last
    pub fn shot_frames(&self) -> Option<Vec<i64>> {
        self.shot.map(|(a, b)| (a..=b).collect())
    }
}

pub fn scenes(path: &Path) -> R<Vec<Scene>> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let list = v["scenes"].as_array().ok_or(format!("{}: no \"scenes\" list", path.display()))?;
    list.iter()
        .map(|s| {
            let text = |k: &str| s[k].as_str().map(str::to_string).ok_or(format!("{}: a scene without \"{k}\"", path.display()));
            let frames = |k: &str| s[k].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect::<Vec<_>>());
            let shot = frames("shot").filter(|f| f.len() == 2).map(|f| (f[0], f[1]));
            let reference = if s["reference"] == json!(false) { Err(s["why"].as_str().unwrap_or("not yet").to_string()) } else { Ok(()) };
            let live = if s["live"] == json!(true) { Ok(()) } else { Err(s["live_why"].as_str().unwrap_or("not asked").to_string()) };
            let size = frames("size").filter(|f| f.len() == 2).map_or((1920, 1080), |f| (f[0], f[1]));
            let spp = s["spp"].as_u64().map(|n| n.max(1) as u32);
            Ok(Scene { name: text("name")?, dir: home(&text("dir")?), file: text("file")?, composition: text("composition")?, stills: frames("stills").unwrap_or_default(), shot, reference, live, size, spp })
        })
        .collect()
}

/// Frames `at` of a scene drawn on `path` into `out` (frame-00000.png onward, in `at`'s order), by this raea; the
/// seconds it took.
fn render(scene: &Scene, path: &str, spp: u32, at: &[i64], out: &Path) -> R<f64> {
    let mut env = switches(path);
    env.push(("RAEA_LIGHT_SPP", spp.to_string()));
    render_with(scene, path, &env, at, out)
}

/// Frames `at` of a scene drawn under the switches `env` into `out`, `label` naming the run in an error; the seconds it
/// took. `label` "pathtracer" is the truth.
pub fn render_with(scene: &Scene, label: &str, env: &[(&str, String)], at: &[i64], out: &Path) -> R<f64> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let at: Vec<String> = at.iter().map(i64::to_string).collect();
    let t = Instant::now();
    let mut cmd = std::process::Command::new(exe);
    cmd.current_dir(&scene.dir).envs(env.iter().map(|(k, v)| (*k, v)));
    // the truth is never steadied: RAEA_STEADY's jitter moved the path-traced shot's frames by part of a pixel each,
    // and that read as the paths' flicker (FLOOR3's reference 0.478 stops of its own against 0.101)
    if label == "pathtracer" {
        cmd.env_remove("RAEA_STEADY");
    }
    let run = cmd
        .args(["render", &scene.file, "--composition", &scene.composition, "--at", &at.join(","), "-o"])
        .arg(format!("{}/", out.display()))
        // a light check measures light, not placement: a set's clip (11-24: FLOOR3's fit-2 1 cm into grid-bay) is the
        // set's to fix and stays in its record, but doesn't stop the measure
        .args(["--despite", "clips", "raea light check measures light, not placement"])
        .output()
        .map_err(|e| e.to_string())?;
    if !run.status.success() {
        let err = String::from_utf8_lossy(&run.stderr);
        let last: Vec<&str> = err.lines().rev().take(4).collect();
        return Err(format!("{} on {label}: render failed: {}", scene.name, last.into_iter().rev().collect::<Vec<_>>().join(" | ")));
    }
    Ok(t.elapsed().as_secs_f64())
}

/// The scene played live on `path` at `size`, headless and unthrottled (`raea play --offscreen --no-vsync --3d`): what
/// a frame costs, from play's report (DIR/PATH-live-WxH.json): the frames presented a second as milliseconds a frame,
/// and each frame's work at the median and the 99th percentile; on `gi`, the probe pass's own GPU time (RAEA_GI_STATS).
fn live(scene: &Scene, path: &str, size: (i64, i64), out: &Path) -> R<Value> {
    live_with(scene, path, &switches(path), size, out)
}

/// The same, under the switches `env`, `path` naming its report (DIR/PATH-live-WxH.json).
pub fn live_with(scene: &Scene, path: &str, env: &[(&str, String)], size: (i64, i64), out: &Path) -> R<Value> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let size = format!("{}x{}", size.0, size.1);
    let json_out = out.join(format!("{path}-live-{size}.json"));
    let gi_out = out.join(format!("{path}-gi-{size}.json"));
    let _ = std::fs::remove_file(&gi_out);
    let run = std::process::Command::new(exe)
        .current_dir(&scene.dir)
        .envs(env.iter().map(|(k, v)| (*k, v)))
        .env("RAEA_GI_STATS", &gi_out)
        .env("RAEA_AUDIO_MUTE", "1")
        .args(["play", &scene.file, "--offscreen", "--no-vsync", "--3d", "--seconds", LIVE_SECONDS, "--size", &size, "--json"])
        .arg(&json_out)
        .output()
        .map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(&json_out).map_err(|_| {
        let err = String::from_utf8_lossy(&run.stderr);
        format!("{} on {path}: play wrote no report: {}", scene.name, err.lines().rev().take(3).collect::<Vec<_>>().join(" | "))
    })?;
    let r: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", json_out.display()))?;
    let fps = r["presented_fps"].as_f64().unwrap_or(0.0);
    Ok(json!({
        "size": size,
        "ms_a_frame": if fps > 0.0 { json!(((1000.0 / fps) * 10.0).round() / 10.0) } else { Value::Null },
        "work_ms_p50": r["work_ms"]["p50"], "work_ms_p99": r["work_ms"]["p99"], "presented": r["presented"], "error": r["error"],
        "gi": std::fs::read_to_string(&gi_out).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()),
    }))
}

/// The frames written to `dir` at these places in its render
pub fn frames(dir: &Path, at: impl IntoIterator<Item = usize>) -> R<Vec<metric::Luma>> {
    frames_with(dir, at, metric::load)
}

/// The same, each read by `load` (raea bench denoise reads them as colour)
pub fn frames_with<T>(dir: &Path, at: impl IntoIterator<Item = usize>, load: fn(&Path) -> R<T>) -> R<Vec<T>> {
    at.into_iter().map(|k| load(&dir.join(format!("frame-{k:05}.png")))).collect()
}

/// A table's cell: the number at `k`, to three places, or "-"
pub fn cell(v: &Value, k: &str) -> String {
    v[k].as_f64().map_or("-".to_string(), |x| format!("{x:.3}"))
}

/// A comma-separated `name` flag's values, each one of `all` (all of them when it isn't given); `what` says what they are
pub fn list_flag(args: &[String], name: &str, all: &[&str], what: &str) -> R<Vec<String>> {
    let v: Vec<String> = flag(args, name).map_or_else(|| all.iter().copied().map(String::from).collect(), |p| p.split(',').map(String::from).collect());
    match v.iter().find(|p| all.iter().all(|a| a != p)) {
        Some(p) => Err(format!("{name}: {p} isn't {what} ({})", all.join(", "))),
        None => Ok(v),
    }
}

/// --spp N, the truth's samples a pixel (256)
pub fn spp(args: &[String]) -> R<u32> {
    Ok(flag(args, "--spp").map(|v| v.parse().map_err(|_| format!("--spp {v}: a count of samples"))).transpose()?.unwrap_or(256))
}

/// frames drawn before each measured frame that doesn't follow the one before it, and not measured, so whatever
/// keeps a past (raea's temporal filter, the GI's caches) has its own, as it does in play: from a cold start a
/// filter's first frames average one, two, three frames
pub const BEFORE: i64 = 16;

/// `at` with BEFORE frames drawn before each of its frames that doesn't follow the one before, and where each of
/// `at`'s frames lands in that render
pub fn with_before(at: &[i64]) -> (Vec<i64>, Vec<usize>) {
    let (mut all, mut place) = (Vec::new(), Vec::new());
    for &f in at {
        if all.last() != Some(&(f - 1)) {
            all.extend((f - BEFORE).max(0)..f);
        }
        place.push(all.len());
        all.push(f);
    }
    (all, place)
}

fn round(x: f32, k: f32) -> f64 {
    ((x * k).round() / k) as f64
}

/// Frames `at` path-traced at `spp` into `dir`, kept from an earlier check when it made the same frames of the same
/// composition at the same samples (DIR/made.json) and --fresh isn't given; the frames, and how they were had.
pub fn reference(scene: &Scene, spp: u32, at: &[i64], dir: &Path, fresh: bool) -> R<(Vec<metric::Luma>, Value)> {
    let made = json!({ "file": scene.file, "composition": scene.composition, "frames": at, "spp": spp });
    let kept = std::fs::read_to_string(dir.join("made.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let how = if !fresh && kept.as_ref() == Some(&made) {
        json!({ "spp": spp, "kept": true })
    } else {
        let s = render(scene, "pathtracer", spp, at, dir)?;
        std::fs::write(dir.join("made.json"), made.to_string()).map_err(|e| e.to_string())?;
        json!({ "spp": spp, "seconds_a_frame": round(s as f32 / at.len().max(1) as f32, 10.0) })
    };
    Ok((frames(dir, 0..at.len())?, how))
}

/// One scene: its reference stills and shot (kept as `reference` says), then each path's stills and shot against them,
/// and each path live.
fn check(scene: &Scene, paths: &[String], spp: u32, out: &Path, fresh: bool) -> R<Value> {
    let root = out.join(&scene.name);
    let spp = scene.spp.unwrap_or(spp);
    let shot = scene.shot_frames();
    let mut report = json!({ "name": scene.name, "stills": scene.stills, "shot": shot, "paths": {} });
    let (reference, ref_shot) = match &scene.reference {
        Err(why) => {
            report["reference"] = json!({ "made": false, "why": why });
            (None, None)
        }
        Ok(()) => {
            let (stills, how) = reference(scene, spp, &scene.stills, &root.join("reference"), fresh)?;
            report["reference"] = json!({ "made": true, "stills": how });
            let ref_shot = match &shot {
                Some(at) => {
                    let (frames, how) = reference(scene, spp, at, &root.join("reference-shot"), fresh)?;
                    report["reference"]["shot"] = how;
                    Some(frames)
                }
                None => None,
            };
            (Some(stills), ref_shot)
        }
    };
    for path in paths {
        let dir = root.join(path);
        let (all, place) = with_before(&scene.stills);
        let secs = render(scene, path, 1, &all, &dir)?;
        let mut r = json!({ "seconds_a_still": round(secs as f32 / all.len().max(1) as f32, 100.0) });
        if let Some(refs) = &reference {
            let got = frames(&dir, place)?;
            let d: Vec<metric::Distance> = got.iter().zip(refs).map(|(a, b)| metric::distance(a, b)).collect::<R<_>>()?;
            let n = d.len().max(1) as f32;
            r["mean_stops"] = json!(round(d.iter().map(|x| x.mean).sum::<f32>() / n, 1000.0));
            r["bias_stops"] = json!(round(d.iter().map(|x| x.bias).sum::<f32>() / n, 1000.0));
            r["worst_tile_stops"] = json!(round(d.iter().map(|x| x.worst).fold(0.0, f32::max), 1000.0));
            r["per_still"] = json!(d.iter().map(|x| json!({ "mean": round(x.mean, 1000.0), "bias": round(x.bias, 1000.0), "worst": round(x.worst, 1000.0), "worst_tile": [x.worst_at.0, x.worst_at.1] })).collect::<Vec<_>>());
        }
        if let Some(at) = &shot {
            let dir = root.join(format!("{path}-shot"));
            let (all, place) = with_before(at);
            let secs = render(scene, path, 1, &all, &dir)?;
            let got = frames(&dir, place)?;
            // in stops, against the shot path-traced; and the frames' own second difference, in 8-bit steps, which
            // counts the shot's motion as well
            if let Some(refs) = &ref_shot {
                r["flicker_stops"] = json!(metric::error_flicker(&got, refs)?.map(|f| round(f, 1000.0)));
            }
            r["flicker_raw"] = json!(metric::flicker(&got).map(|f| round(f, 100.0)));
            r["seconds_a_shot_frame"] = json!(round(secs as f32 / all.len() as f32, 100.0));
        }
        // a traced path's light held: the first still drawn again and again, and what still changes (steady.rs)
        if TRACED.contains(&path.as_str()) {
            let held = root.join(format!("{path}-held"));
            steady::hold(&scene.dir, &scene.file, Some(&scene.composition), &scene.stills[0].to_string(), HELD, &switches(path), &held)?;
            r["steady"] = steady::report(&held, None)?;
        }
        r["live"] = match &scene.live {
            // at the scene's size only: the live 3D draws at its composition's size whatever the stage's
            // (runtime/live scene_3d), so a smaller stage measured the same frame (11-17), and no switch draws it smaller
            Ok(()) => json!({ "full": live(scene, path, scene.size, &root)? }),
            Err(why) => json!({ "measured": false, "why": why }),
        };
        report["paths"][path] = r;
    }
    Ok(report)
}

/// The table a person reads: a line a scene and path.
fn table(reports: &[Value]) -> String {
    let mut s = format!("{:<18} {:<9} {:>7} {:>7} {:>7} {:>8} {:>7} {:>8} {:>8} {:>8}\n", "scene", "path", "mean", "bias", "worst", "flicker", "raw", "s/still", "live ms", "gi gpu");
    for r in reports {
        let name = r["name"].as_str().unwrap_or("");
        let Some(paths) = r["paths"].as_object() else { continue };
        for (p, v) in paths {
            let f = |k: &str| cell(v, k);
            let ms = |k: &str| v["live"][k]["ms_a_frame"].as_f64().map_or("-".to_string(), |x| format!("{x:.1}"));
            // the cache's passes and the screen's (its probes, their filter, the screen), each a median
            let g = &v["live"]["full"]["gi"];
            let gi = g["gpu_ms_p50"].as_f64().map_or("-".to_string(), |x| format!("{:.2}", x + g["screen_ms_p50"].as_f64().unwrap_or(0.0)));
            s += &format!("{name:<18} {p:<9} {:>7} {:>7} {:>7} {:>8} {:>7} {:>8} {:>8} {gi:>8}\n", f("mean_stops"), f("bias_stops"), f("worst_tile_stops"), f("flicker_stops"), f("flicker_raw"), f("seconds_a_still"), ms("full"));
        }
        if r["reference"]["made"] == json!(false) {
            s += &format!("{name:<18} (no reference: {})\n", r["reference"]["why"].as_str().unwrap_or(""));
        }
    }
    s
}

pub fn light(args: &[String]) -> R<String> {
    if args.first().map(String::as_str) == Some("steady") {
        return steady::steady(&args[1..]);
    }
    if args.first().map(String::as_str) != Some("check") || args.len() < 2 {
        return Err(format!("usage: {USAGE}"));
    }
    let list = scenes(Path::new(&args[1]))?;
    let out = PathBuf::from(flag(args, "-o").ok_or(format!("usage: {USAGE}"))?);
    let only = flag(args, "--scene");
    let paths = list_flag(args, "--paths", &PATHS, "a light path to check")?;
    let spp = spp(args)?;
    let fresh = args.iter().any(|a| a == "--fresh");
    let mut reports = Vec::new();
    for s in list.iter().filter(|s| match &only { Some(o) => *o == s.name, None => true }) {
        eprintln!("raea light: {} ...", s.name);
        reports.push(check(s, &paths, spp, &out, fresh)?);
    }
    if reports.is_empty() {
        return Err(format!("no scene named {}", only.unwrap_or_default()));
    }
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let report = json!({ "scenes": reports });
    std::fs::write(out.join("report.json"), serde_json::to_string_pretty(&report).unwrap_or_default()).map_err(|e| e.to_string())?;
    let said = format!("{}{}", table(&reports), format_args!("raea light: wrote {}", out.join("report.json").display()));
    // a traced path whose held frame's light moved fails the check: its distance to the reference is of a frame that
    // won't be the same the next time
    let moved: Vec<String> = reports.iter().flat_map(|r| r["paths"].as_object().into_iter().flatten().filter(|(_, p)| p["steady"]["steady"] == json!(false)).map(|(path, p)| format!("{} on {path}: {}", r["name"].as_str().unwrap_or(""), steady::line(&p["steady"])))).collect();
    if !moved.is_empty() {
        return Err(format!("{said}\nthe light doesn't hold still ({}). {}", moved.join("; "), steady::WHY));
    }
    Ok(said)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_that_doesnt_follow_the_one_before_gets_16_drawn_before_it() {
        let (all, place) = with_before(&[20, 21, 22, 100]);
        assert_eq!(all.len(), 16 + 3 + 16 + 1);
        assert_eq!(place, vec![16, 17, 18, 35]);
        assert_eq!(place.iter().map(|&k| all[k]).collect::<Vec<_>>(), vec![20, 21, 22, 100]);
        // none before the composition's first frame
        assert_eq!(with_before(&[3]).0, vec![0, 1, 2, 3]);
    }
}
