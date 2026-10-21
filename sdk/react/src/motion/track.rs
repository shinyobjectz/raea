//! Matchmove: a clip's camera solved, so 3D can stand in its world (sdk/motion/src/matchmove.tsx, rdb::assets::Store::matchmove).
//!
//!   raea-motion track CLIP.mp4 [--subject X,Y,W,H] [--from S] [--seconds N] [--height M] [--fov DEG] [--smooth F]
//!                     [--dir EDIT_DIR] [-o OUT.json]
//!   native: matchmove.solve {src, from?, seconds?, subject?, height?, fov?, smooth?} -> the solve
//!
//! Both go through the same asset store a composition's render does (beside EDIT_DIR/edit.tsx, default the working
//! folder), with the same params, so a solve made here is the one `<Matchmove src=... from=...>` in that edit finds
//! already made, and the reverse. The solver installs itself the first time it's used (tools/matchmove/setup.sh).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

type R<T> = Result<T, String>;

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

const USAGE: &str = "raea-motion track CLIP.mp4 [--subject X,Y,W,H] [--from S] [--seconds N] [--height M] [--fov DEG] [--smooth F] [--dir EDIT_DIR] [-o OUT.json]";
const WITH_VALUE: [&str; 9] = ["--subject", "--from", "--seconds", "--height", "--fov", "--smooth", "--dir", "-o", "--o"];

/// The CLI's clip and params (what Store::matchmove is asked), checked: (clip, params JSON).
pub fn parse(args: &[String]) -> R<(PathBuf, Value)> {
    let mut clip = None;
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
        if clip.replace(PathBuf::from(a)).is_some() {
            return Err(format!("one clip at a time\n{USAGE}"));
        }
        i += 1;
    }
    let clip = clip.ok_or(USAGE)?;
    let mut p = serde_json::Map::new();
    for k in ["from", "seconds", "height", "fov", "smooth"] {
        if let Some(v) = flag(args, &format!("--{k}")) {
            let n: f64 = v.parse().ok().filter(|n: &f64| n.is_finite()).ok_or(format!("--{k} takes a number, got `{v}`"))?;
            p.insert(k.into(), json!(n));
        }
    }
    if let Some(s) = flag(args, "--subject") {
        let b: Vec<f64> = s.split(',').map(|v| v.trim().parse::<f64>()).collect::<Result<_, _>>().map_err(|_| format!("--subject is X,Y,W,H in 0..1 of the first frame, got `{s}`"))?;
        p.insert("subject".into(), json!(b));
    }
    let params = rdb::assets::matchmove_params(&Value::Object(p))?;
    Ok((clip, params))
}

/// A solve's JSON with its cut-outs' pattern made absolute (they live beside it in the store).
fn answer(store: &rdb::assets::Store, a: &rdb::assets::Asset) -> R<Value> {
    let path = store.path(a);
    let mut v: Value = serde_json::from_slice(&std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(c) = v["cut"].as_str() {
        v["cut"] = json!(store.dir.join(c).display().to_string());
    }
    v["asset"] = json!(a.hash);
    v["file"] = json!(path.display().to_string());
    Ok(v)
}

/// `raea-motion track CLIP ...`: the clip's camera solved once and kept; what it found.
pub fn track(args: &[String]) -> R<String> {
    let (clip, params) = parse(args)?;
    let clip = clip.canonicalize().map_err(|e| format!("{}: {e}", clip.display()))?;
    let dir = PathBuf::from(flag(args, "--dir").unwrap_or_else(|| ".".into()));
    let store = rdb::assets::Store::beside(&dir.join("edit.tsx"));
    let t0 = std::time::Instant::now();
    let (a, fresh) = store.matchmove(&clip, &params)?;
    let v = answer(&store, &a)?;
    let at = match flag(args, "-o") {
        Some(o) => {
            if let Some(d) = Path::new(&o).parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
            }
            std::fs::write(&o, serde_json::to_string(&v).unwrap_or_default()).map_err(|e| format!("{o}: {e}"))?;
            o
        }
        None => store.path(&a).display().to_string(),
    };
    let how = if fresh { format!("solved in {:.0} s", t0.elapsed().as_secs_f64()) } else { "already solved".into() };
    let err = v["error"].as_f64().map(|e| format!("reprojection error {e:.2} px")).unwrap_or_else(|| "no reprojection error (no structure)".into());
    let mut tsx = format!("<Matchmove src={{staticFile(\"{}\")}}", clip.file_name().and_then(|n| n.to_str()).unwrap_or("clip.mp4"));
    for k in ["from", "seconds", "height", "fov", "smooth"] {
        let d = match k {
            "height" => 1.6,
            "smooth" => 1.0,
            _ => 0.0,
        };
        if let Some(n) = params[k].as_f64().filter(|n| *n != d) {
            tsx.push_str(&format!(" {k}={{{n}}}"));
        }
    }
    if let Value::Array(b) = &params["subject"] {
        tsx.push_str(&format!(" subject={{[{}]}}", b.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ")));
    }
    tsx.push_str(">...</Matchmove>");
    let notes: Vec<&str> = v["notes"].as_array().map(|n| n.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    Ok(format!(
        "APPLIED  track {} ({}): {} of {} frames registered, {err}, fov {:.1} deg, camera {} m up -> {at} ({how}; asset {})\n         {}\n         in TSX: {tsx} finds it in {}",
        clip.display(),
        v["mode"].as_str().unwrap_or("?"),
        v["registered"],
        v["frames"],
        v["fov"].as_f64().unwrap_or(0.0),
        v["camera_height"],
        &a.hash[..12],
        notes.join("; "),
        store.dir.display()
    ))
}

/// Seconds spent in solves made during this process (and the one running now): a render's wait for a frame's commit
/// (motion.rs settle) doesn't count them, so a first-use solve inside a render isn't taken for a hung composition.
static MADE_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static RUNNING_SINCE: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

pub(crate) fn making_secs() -> f64 {
    let done = MADE_NS.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e9;
    done + RUNNING_SINCE.lock().ok().and_then(|r| r.map(|t| t.elapsed().as_secs_f64())).unwrap_or(0.0)
}

/// `matchmove.solve`: the solve a <Matchmove> draws from, made on first use (the composition's folder's store).
pub(crate) fn register() {
    raea_native::register(
        "matchmove.solve",
        "{ src: string; from?: number; seconds?: number; subject?: [number, number, number, number] | null; height?: number; fov?: number; smooth?: number }",
        "{ mode: \"sfm\" | \"rotation\" | \"static\"; width: number; height: number; fps: number; from: number; seconds: number; frames: number; fov: number; focal: number; k1: number; registered: number; error: number | null; camera_height: number; scale: number; cameras: { t: number; position: [number, number, number]; quaternion: [number, number, number, number]; solved: boolean }[]; points: [number, number, number][]; ground: { y: number; normal: [number, number, number] }; cut: string | null; notes: string[]; asset: string; file: string }",
        "a clip's camera solved (COLMAP; made once per clip content and params, in the composition's asset store)",
        |v| {
            let src = v.get("src").and_then(Value::as_str).ok_or("matchmove.solve: src, the clip (staticFile(\"x.mp4\"))")?;
            let dir = crate::native::composition_dir().unwrap_or_else(|| PathBuf::from("."));
            let clip = super::media_path(&dir, src);
            let mut p = v.as_object().cloned().unwrap_or_default();
            p.remove("src");
            let store = rdb::assets::Store::beside(&dir.join("edit.tsx"));
            let t0 = std::time::Instant::now();
            if let Ok(mut r) = RUNNING_SINCE.lock() {
                *r = Some(t0);
            }
            let made = store.matchmove(&clip, &Value::Object(p));
            if let Ok(mut r) = RUNNING_SINCE.lock() {
                *r = None;
            }
            MADE_NS.fetch_add(t0.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
            let (a, _) = made?;
            answer(&store, &a)
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn track_parses_its_flags() {
        let (clip, p) = parse(&args("track media/orbit.mp4 --from 2 --seconds 5 --height 20 --subject 0.4,0.2,0.2,0.7 -o out.json")).unwrap();
        assert_eq!(clip, PathBuf::from("media/orbit.mp4"));
        assert_eq!(p, json!({"from": 2.0, "seconds": 5.0, "subject": [0.4, 0.2, 0.2, 0.7], "height": 20.0, "fov": 0.0, "smooth": 1.0}));
        let (_, p) = parse(&args("track a.mp4")).unwrap();
        assert_eq!(p["subject"], Value::Null);
        assert!(parse(&args("track")).unwrap_err().contains("raea-motion track CLIP"));
        assert!(parse(&args("track a.mp4 --from")).unwrap_err().contains("--from needs a value"));
        assert!(parse(&args("track a.mp4 --from two")).unwrap_err().contains("--from takes a number"));
        assert!(parse(&args("track a.mp4 --subject 0.4,0.2,0.2")).unwrap_err().contains("subject"));
        assert!(parse(&args("track a.mp4 --wobble 1")).unwrap_err().contains("unknown flag --wobble"));
        assert!(parse(&args("track a.mp4 b.mp4")).unwrap_err().contains("one clip"));
    }
}
