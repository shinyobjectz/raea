//! `raea bake light EDIT.tsx [--composition ID] [--samples N] [--compare DIR]` (docs/bake.feature): raea's own bake of a
//! composition's static parts (BK1-BK5), and the bar it is held to (BK0). `--compare DIR` runs the test set (sets.json)
//! through Cycles (blender/look.py's still and blender/lightbake.py's bake, both at --samples, 1024 by default) and
//! through raea, then writes DIR/report.json and a table: each frame's RMSE and stops against the Cycles still, each
//! mesh's lightmap against Cycles', and each bake's time and peak memory.
//!
//! A set in sets.json: { "name", "dir" (under raea's home, or ~/...), "file", "composition", "cycles": [file,
//! composition] (the set drawn with lightbake.py's maps), "look": [look.py's flags, the composition's own values],
//! "texels" (lightbake.py's texels a metre), "bar": true (the set the bar is read on), "reference": false and "why" (a
//! set whose references can't be made yet) }.

pub mod compare;

use raea_runtime_bundle::{flag, R};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const USAGE: &str = "raea bake light EDIT.tsx [--composition ID] [--samples N] | raea bake light --compare DIR [--sets FILE] [--set NAME] [--samples N] [--fresh]   (bounce light baked into lightmaps on raea's ray tracer; --compare measures the test set against Cycles' bake and still, docs/bake.feature)";

/// One set of the test set, as sets.json lists it.
pub struct Set {
    pub name: String,
    pub dir: PathBuf,
    pub file: String,
    pub composition: String,
    pub cycles: Option<(String, String)>,
    pub look: Vec<String>,
    pub texels: u32,
    pub bar: bool,
    pub reference: Result<(), String>,
}

fn home_path(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), raea_io_home::user_home()) {
        (Some(rest), Some(h)) => h.join(rest),
        _ if Path::new(p).is_absolute() => PathBuf::from(p),
        _ => raea_io_home::home().join(p),
    }
}

pub fn sets(path: &Path) -> R<Vec<Set>> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let list = v["sets"].as_array().ok_or(format!("{}: no \"sets\" list", path.display()))?;
    let strs = |x: &Value| x.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect::<Vec<_>>()).unwrap_or_default();
    list.iter()
        .map(|s| {
            let text = |k: &str| match &s[k] {
                Value::String(v) => Ok(v.clone()),
                _ => Err(format!("{}: a set without \"{k}\"", path.display())),
            };
            let cycles = match strs(&s["cycles"]).as_slice() {
                [file, comp] => Some((file.clone(), comp.clone())),
                _ => None,
            };
            let reference = if s["reference"] == json!(false) { Err(s["why"].as_str().unwrap_or("not yet").to_string()) } else { Ok(()) };
            Ok(Set { name: text("name")?, dir: home_path(&text("dir")?), file: text("file")?, composition: text("composition")?, cycles, look: strs(&s["look"]), texels: s["texels"].as_u64().unwrap_or(128) as u32, bar: s["bar"] == json!(true), reference })
        })
        .collect()
}

/// What a step cost: its seconds, and its peak memory in MB where the OS reports it (a child's and its children's).
#[derive(Debug, Clone, Copy)]
pub struct Ran {
    pub seconds: f64,
    pub peak_mb: Option<u64>,
}

impl Ran {
    pub fn json(&self) -> Value {
        json!({ "seconds": (self.seconds * 10.0).round() / 10.0, "peak_mb": self.peak_mb })
    }
}

#[cfg(unix)]
fn wait(child: std::process::Child) -> R<(bool, Option<u64>)> {
    let (mut status, pid) = (0, child.id() as libc::pid_t);
    // SAFETY: rusage is plain data, zeroed is a valid value; wait4 reaps the child std spawned and never waited on
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::wait4(pid, &mut status, 0, &mut ru) } < 0 {
        return Err(format!("waiting for {pid}: {}", std::io::Error::last_os_error()));
    }
    // ru_maxrss is bytes on macOS, kilobytes elsewhere
    let kb = if cfg!(target_os = "macos") { ru.ru_maxrss as u64 / 1024 } else { ru.ru_maxrss as u64 };
    Ok((libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0, Some(kb / 1024)))
}

#[cfg(not(unix))]
fn wait(mut child: std::process::Child) -> R<(bool, Option<u64>)> {
    Ok((child.wait().map_err(|e| e.to_string())?.success(), None))
}

/// This raea with `args` in `dir`, its output into `log`: what it cost, or the log's last lines.
pub fn run(dir: &Path, args: &[String], log: &Path) -> R<Ran> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let out = std::fs::File::create(log).map_err(|e| format!("{}: {e}", log.display()))?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    let t = std::time::Instant::now();
    let child = std::process::Command::new(exe).current_dir(dir).args(args).stdout(out).stderr(err).spawn().map_err(|e| e.to_string())?;
    let (ok, peak_mb) = wait(child)?;
    let ran = Ran { seconds: t.elapsed().as_secs_f64(), peak_mb };
    if ok {
        return Ok(ran);
    }
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let last: Vec<&str> = text.lines().filter(|l| !l.starts_with("Fra:") && l.chars().any(|c| !c.is_whitespace())).rev().take(4).collect();
    Err(format!("raea {} failed ({}): {}", args.first().map_or("", String::as_str), log.display(), last.into_iter().rev().collect::<Vec<_>>().join(" | ")))
}

/// A set's identity for its kept references: every .tsx in its folder and its parts/, the flags look.py and lightbake.py
/// get, and the samples.
pub fn key(s: &Set, samples: u32) -> String {
    let mut h = Sha256::new();
    for d in [s.dir.clone(), s.dir.join("parts")] {
        let mut fs: Vec<PathBuf> = std::fs::read_dir(&d).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "tsx")).collect();
        fs.sort();
        for f in fs {
            h.update(f.file_name().unwrap_or_default().as_encoded_bytes());
            h.update(std::fs::read(&f).unwrap_or_default());
        }
    }
    h.update(format!("{:?} {} {samples}", s.look, s.texels));
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Where the Blender library scripts are (sdk/guides/craft/bevy/blender), with `name` there, or why not.
pub fn library(name: &str) -> R<PathBuf> {
    let p = raea_io_home::home().join("sdk/guides/craft/bevy/blender").join(name);
    if p.is_file() { Ok(p) } else { Err(format!("no {} (the Blender library script the Cycles reference is made with)", p.display())) }
}

pub fn cli(args: &[String]) -> R<String> {
    if args.get(1).map(String::as_str) != Some("light") {
        return Err(format!("usage: {USAGE}"));
    }
    let samples: u32 = flag(args, "--samples").map(|v| v.parse().map_err(|_| format!("--samples {v}: a count of samples a texel"))).transpose()?.unwrap_or(1024);
    let Some(out) = flag(args, "--compare").map(PathBuf::from) else {
        // rask: BK5 `raea bake light EDIT.tsx` bakes a composition's <LightBake> ahead of a render and says what it baked
        return Err("raea's own bake isn't built yet (docs/bake.feature BK1-BK5); `raea bake light --compare DIR` measures the test set against Cycles".into());
    };
    let list = sets(&flag(args, "--sets").map_or_else(|| raea_io_home::home().join("apps/lightbake/sets.json"), PathBuf::from))?;
    let only = flag(args, "--set");
    let fresh = args.iter().any(|a| a == "--fresh");
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut reports = Vec::new();
    for s in list.iter().filter(|s| only.is_none() || only.as_deref() == Some(s.name.as_str())) {
        eprintln!("raea bake light: {} ...", s.name);
        reports.push(compare::set(s, samples, &out.join(&s.name), fresh)?);
    }
    if reports.is_empty() {
        return Err(format!("no set named {}", only.unwrap_or_default()));
    }
    let report = json!({ "samples": samples, "sets": reports });
    let path = out.join("report.json");
    std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap_or_default()).map_err(|e| e.to_string())?;
    Ok(format!("{}raea bake light: wrote {}", compare::table(&reports), path.display()))
}
