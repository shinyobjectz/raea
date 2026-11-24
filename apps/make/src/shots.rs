//! Blocking a shot list (docs/make.feature MK15): `raea make framing` renders each shot's first, middle and last
//! frame and puts them on one sheet, a row a shot, each stamped with its name and time, so the person, `raea ask`
//! (make.framing.*) and the critic (`raea judge --kind shot`) see every shot's start and end at once; `raea make
//! lenschart SHOT` renders a shot's first frame at 18, 24, 35, 50, 85 and 135 mm (RAEA_LENS, core/review) to choose its
//! lens before it's moved.

use std::path::{Path, PathBuf};

use raea_core_brief::make::{Make, Shot};

use crate::sheet::{Tile, sheet};

/// The focal lengths a lens chart shows.
pub const LENSES: [u32; 6] = [18, 24, 35, 50, 85, 135];

/// raea itself, run on `args` with `env`: its output, or what it said when it failed.
pub fn raea(args: &[String], env: &[(&str, String)]) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut c = std::process::Command::new(exe);
    c.args(args);
    for (k, v) in env {
        c.env(k, v);
    }
    // stderr passes through as it comes (a generation's progress, a render's frames), so whoever watches sees it
    // working, and is kept for the error
    let mut child = c.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let err = child.stderr.take().map(|e| {
        std::thread::spawn(move || {
            use std::io::BufRead;
            let mut kept = Vec::new();
            for line in std::io::BufReader::new(e).lines().map_while(Result::ok) {
                eprintln!("  {line}");
                kept.push(line);
            }
            kept
        })
    });
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let err = err.and_then(|t| t.join().ok()).unwrap_or_default();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() {
        Ok(text)
    } else {
        Err(format!("raea {}: {}", args.first().map(String::as_str).unwrap_or(""), err.iter().rev().find(|l| !l.trim().is_empty()).map_or(text.trim(), String::as_str)))
    }
}

/// A shot's frames: its first, middle and last, from its seconds at its frame rate.
pub fn frames_of(s: &Shot) -> Vec<i64> {
    let n = ((s.seconds.unwrap_or(0.0) * s.fps.unwrap_or(30.0)).round() as i64).max(1);
    if s.still == Some(true) || n == 1 {
        return vec![0];
    }
    let mut v = vec![0, (n - 1) / 2, n - 1];
    v.dedup();
    v
}

fn composition(dir: &Path, s: &Shot) -> Result<PathBuf, String> {
    let c = s.composition.as_deref().ok_or_else(|| format!("shot `{}` has no composition (the .tsx that renders it)", s.name))?;
    Ok(dir.join(c))
}

/// The framing sheet of the plan's shots (or those named in `only`), written to `out` (OUT/framing.png).
pub fn framing(dir: &Path, m: &Make, only: &[String], out: &Path) -> Result<String, String> {
    let shots: Vec<&Shot> = crate::only(&m.shots, only, |s| s.name.as_str());
    if shots.is_empty() {
        return Err("raea make framing: no shots in the plan (or none named)".into());
    }
    let work = out.join("framing-frames");
    let mut rows = Vec::new();
    for s in &shots {
        let tsx = composition(dir, s)?;
        let fr = frames_of(s);
        let to = work.join(&s.name);
        let _ = std::fs::remove_dir_all(&to);
        std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
        let at = fr.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(",");
        raea(&["render".into(), tsx.display().to_string(), "-o".into(), to.display().to_string(), "--at".into(), at], &[])?;
        let fps = s.fps.unwrap_or(30.0);
        rows.push(fr.iter().enumerate().map(|(i, f)| Tile { img: to.join(format!("frame-{i:05}.png")), label: format!("{}  t={:.1}s  (frame {f})", s.name, *f as f64 / fps) }).collect());
    }
    let file = out.join("framing.png");
    sheet("Framing: each shot's first, middle and last frame", &rows, 480, &file)?;
    Ok(format!("wrote {} ({} shot{}); ask it: raea ask {} --set make  (the framing questions apply); judge it: raea judge {} --kind shot", file.display(), shots.len(), if shots.len() == 1 { "" } else { "s" }, file.display(), out.display()))
}

/// One shot's first frame at every lens of the chart, on one sheet (OUT/lens-SHOT.png).
pub fn lenschart(dir: &Path, m: &Make, name: &str, out: &Path) -> Result<String, String> {
    let s = m.shots.iter().find(|s| s.name == name).ok_or_else(|| format!("raea make lenschart: no shot `{name}` in the plan"))?;
    let tsx = composition(dir, s)?;
    let work = out.join("lens-frames").join(name);
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let mut tiles = Vec::new();
    for mm in LENSES {
        let png = work.join(format!("{mm}mm.png"));
        raea(&["render".into(), tsx.display().to_string(), "-o".into(), png.display().to_string(), "--frame".into(), "0".into()], &[("RAEA_LENS", mm.to_string())])?;
        tiles.push(Tile { img: png, label: format!("{mm} mm ({:.0}° vertical)", raea_core_review::fov_of_lens(mm as f64)) });
    }
    let rows: Vec<Vec<Tile>> = {
        let mut it = tiles.into_iter();
        vec![it.by_ref().take(3).collect(), it.collect()]
    };
    let file = out.join(format!("lens-{name}.png"));
    sheet(&format!("Lens chart: {name}, its first frame"), &rows, 480, &file)?;
    Ok(format!("wrote {}: pick the lens, then set the shot's `lens` (raea/camera lens(mm) for its fov)", file.display()))
}

