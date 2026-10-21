//! A song's energy map from the command line (ai/perception/src/song.rs; @raea/motion's song(), useSong() and <SongPlot>).
//!
//!   raea-motion song FILE [-o OUT.song.json] [--srt S.srt | --srt none] [--plot OUT.png] [--size 2400x1350]
//!
//! The map is raea-perceive's (`raea-perceive song`: Beat This! for beats, downbeats and bars; per-beat loudness,
//! onsets, brightness, low end and energy; sections; builds, drops, stops, hits, dips and breaks; cuts per bar),
//! written to OUT (default FILE's name with .song.json, beside it). Sections take the file's `[Section]` markers when
//! it has them: --srt, else an .srt beside the file, else its own timed-text stream (an m4a's lyrics track); `--srt
//! none` ignores them and finds sections by novelty. --plot renders <SongPlot> over the map to a PNG, through the
//! same engine a composition's render uses, so a human can check the map against the song.

use std::path::{Path, PathBuf};

type R<T> = Result<T, String>;

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

const USAGE: &str = "raea-motion song FILE [-o OUT.song.json] [--srt S.srt|none] [--plot OUT.png] [--size 2400x1350]";
const WITH_VALUE: [&str; 5] = ["-o", "--srt", "--plot", "--size", "--backend"];

/// raea-perceive: $RAEA_PERCEIVE, else the perception crate's release build (as rdb's matchmove finds it).
fn perceive_bin() -> PathBuf {
    std::env::var_os("RAEA_PERCEIVE")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../perception/target/release/raea-perceive")))
}

/// The song file and the map's path from the arguments, checked.
pub fn parse(args: &[String]) -> R<(PathBuf, PathBuf)> {
    let mut file = None;
    let mut i = 1;
    while i < args.len() {
        let a = args[i].as_str();
        if WITH_VALUE.contains(&a) {
            if i + 1 >= args.len() {
                return Err(format!("{a} needs a value\n{USAGE}"));
            }
            i += 2;
            continue;
        }
        if a.starts_with("--") {
            return Err(format!("unknown flag {a}\n{USAGE}"));
        }
        if file.replace(PathBuf::from(a)).is_some() {
            return Err(format!("one song at a time\n{USAGE}"));
        }
        i += 1;
    }
    let file = file.ok_or(USAGE)?;
    let out = flag(args, "-o").map(PathBuf::from).unwrap_or_else(|| default_out(&file));
    Ok((file, out))
}

/// FILE's name with .song.json, beside it: song4.wav -> song4.song.json.
pub fn default_out(file: &Path) -> PathBuf {
    let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "song".into());
    file.with_file_name(format!("{stem}.song.json"))
}

pub fn song(args: &[String]) -> R<String> {
    let (file, out) = parse(args)?;
    if !file.exists() {
        return Err(format!("{}: no such file", file.display()));
    }
    let bin = perceive_bin();
    if !bin.exists() {
        return Err(format!("song: the map is raea-perceive's, not at {}: build it with `cd perception && cargo build --release --bin raea-perceive` or set RAEA_PERCEIVE", bin.display()));
    }
    let mut cmd = std::process::Command::new(&bin);
    cmd.arg("song").arg(&file).arg("-o").arg(&out);
    for f in ["--srt", "--backend"] {
        if let Some(v) = flag(args, f) {
            cmd.arg(f).arg(v);
        }
    }
    let o = cmd.output().map_err(|e| format!("{}: {e}", bin.display()))?;
    if !o.status.success() {
        return Err(format!("raea-perceive song: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    let text = std::fs::read_to_string(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let map: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut report = summary(&map);
    report.insert(0, format!("APPLIED  {} mapped to {}", file.display(), out.display()));
    if let Some(png) = flag(args, "--plot") {
        plot(&out, Path::new(&png), flag(args, "--size").as_deref())?;
        report.push(format!("APPLIED  plotted to {png}"));
    }
    Ok(report.join("\n"))
}

/// A few lines a person reads: the tempo, the sections, the drops and builds.
fn summary(m: &serde_json::Value) -> Vec<String> {
    let t = |v: &serde_json::Value| v.as_f64().map(|x| format!("{x:.2}")).unwrap_or_else(|| "?".into());
    let mut lines = vec![format!("tempo {} BPM, {}/4, {} beats, {} bars, sections from {}", t(&m["tempo"]), m["beatsPerBar"], m["beats"].as_array().map_or(0, Vec::len),
        m["bars"].as_array().map_or(0, Vec::len), m["sectionsFrom"].as_str().unwrap_or("?"))];
    for s in m["sections"].as_array().into_iter().flatten() {
        lines.push(format!("  {:>7}  {:<20} {:<5} energy {}  {} cuts/bar", t(&s["time"]), s["name"].as_str().unwrap_or(""), s["level"].as_str().unwrap_or(""), t(&s["energy"]), s["cuts"]));
    }
    for kind in ["build", "drop", "stop", "dip", "break", "hit"] {
        let ev: Vec<String> = m["events"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e["kind"] == kind)
            .map(|e| if e["end"].is_number() { format!("{}-{}", t(&e["time"]), t(&e["end"])) } else { t(&e["time"]) })
            .collect();
        if !ev.is_empty() {
            lines.push(format!("{kind}s ({}): {}", ev.len(), ev.join(" ")));
        }
    }
    lines
}

/// <SongPlot> over the map, rendered to one PNG frame by the engine, from a composition written beside a copy of
/// the map in a scratch folder.
fn plot(map: &Path, png: &Path, size: Option<&str>) -> R<()> {
    let (w, h) = match size {
        Some(s) => {
            let p: Vec<u32> = s.split('x').filter_map(|v| v.trim().parse().ok()).collect();
            if p.len() != 2 || p[0] < 320 || p[1] < 180 {
                return Err(format!("--size is WxH, at least 320x180, got `{s}`"));
            }
            (p[0], p[1])
        }
        None => (2400, 1350),
    };
    let dir = std::env::temp_dir().join(format!("raea-song-plot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::copy(map, dir.join("song.json")).map_err(|e| format!("{}: {e}", map.display()))?;
    let tsx = dir.join("plot.tsx");
    std::fs::write(
        &tsx,
        format!(
            "import {{ Composition, SongPlot, registerRoot, type SongMap }} from \"@raea/motion\";\n\
             import map from \"./song.json\";\n\
             const Plot = () => <SongPlot map={{map as unknown as SongMap}} width={{{w}}} height={{{h}}} />;\n\
             registerRoot(() => <Composition id=\"song-plot\" component={{Plot}} width={{{w}}} height={{{h}}} fps={{30}} durationInFrames={{1}} />);\n"
        ),
    )
    .map_err(|e| e.to_string())?;
    let png = if png.is_absolute() { png.to_path_buf() } else { std::env::current_dir().map_err(|e| e.to_string())?.join(png) };
    let args: Vec<String> = ["render", &tsx.to_string_lossy(), "-o", &png.to_string_lossy(), "--frame", "0"].iter().map(|s| s.to_string()).collect();
    let r = super::run(&args);
    let _ = std::fs::remove_dir_all(&dir);
    r.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_map_lands_beside_the_song_unless_told() {
        let (f, o) = parse(&a(&["song", "media/song4.wav"])).unwrap();
        assert_eq!(f, PathBuf::from("media/song4.wav"));
        assert_eq!(o, PathBuf::from("media/song4.song.json"));
        let (_, o) = parse(&a(&["song", "x.m4a", "-o", "map.json", "--srt", "none"])).unwrap();
        assert_eq!(o, PathBuf::from("map.json"));
    }

    #[test]
    fn bad_arguments_say_how() {
        assert!(parse(&a(&["song"])).unwrap_err().contains("raea-motion song FILE"));
        assert!(parse(&a(&["song", "a.wav", "b.wav"])).unwrap_err().contains("one song"));
        assert!(parse(&a(&["song", "a.wav", "--loud"])).unwrap_err().contains("unknown flag --loud"));
        assert!(parse(&a(&["song", "a.wav", "--plot"])).unwrap_err().contains("--plot needs a value"));
    }
}
