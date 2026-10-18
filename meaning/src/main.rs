//! raea-meaning: frame-embedding columns and the jobs that fall out of them (docs/relational-plan.feature, Phase 3).
//!
//!   raea-meaning ingest VIDEO -o COL [--fps N] [--int8|--coreml]   every frame (or N per second) -> SigLIP 2 -> COL.f32 + COL.json
//!                                                            (--coreml: the phone's package on the Neural Engine, ~20x faster)
//!   raea-meaning dist COL                                    neighbour distance per frame (JSON)
//!   raea-meaning search COL "text" [-k 10]                   the frames most like a phrase, with times and scores
//!   raea-meaning find LIBDIR "text" [-k 5]                   the same over every column in a folder (a footage library)
//!
//! The vision model is raea-embed's (formerly the host app's ugc-embed: SigLIP 2 base p16-256, 768 floats, ONNX); models run once at ingest and
//! everything after is the K kernels in src/k.rs.

mod hdc_time;
mod k;
mod record;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use k::Col;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Meta {
    source: String,
    model: String,
    /// rows per second of video
    fps: f64,
    n: usize,
    dim: usize,
}

fn paths(col: &str) -> (PathBuf, PathBuf) {
    let base = col.trim_end_matches(".json").trim_end_matches(".f32");
    (PathBuf::from(format!("{base}.f32")), PathBuf::from(format!("{base}.json")))
}

fn save(col: &str, meta: &Meta, c: &Col) -> Result<(), String> {
    let (f, j) = paths(col);
    let bytes: Vec<u8> = c.data.iter().flat_map(|x| x.to_le_bytes()).collect();
    std::fs::write(&f, bytes).map_err(|e| format!("{}: {e}", f.display()))?;
    std::fs::write(&j, serde_json::to_string_pretty(meta).unwrap()).map_err(|e| format!("{}: {e}", j.display()))
}

fn load(col: &str) -> Result<(Meta, Col), String> {
    let (f, j) = paths(col);
    let meta: Meta = serde_json::from_str(&std::fs::read_to_string(&j).map_err(|e| format!("{}: {e}", j.display()))?).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&f).map_err(|e| format!("{}: {e}", f.display()))?;
    let data: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    if data.len() != meta.n * meta.dim {
        return Err(format!("{}: {} floats, expected {} x {}", f.display(), data.len(), meta.n, meta.dim));
    }
    let d = meta.dim;
    Ok((meta, Col { d, data }))
}

fn video_fps(video: &Path) -> Result<f64, String> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=avg_frame_rate", "-of", "csv=p=0"])
        .arg(video)
        .output()
        .map_err(|e| format!("ffprobe: {e}"))?;
    // the first field of the first line: a rotated clip's side data adds fields ("30/1,")
    let s = String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or("").split(',').next().unwrap_or("").trim().to_string();
    let (n, d) = s.split_once('/').unwrap_or((&s, "1"));
    let (n, d): (f64, f64) = (n.parse().map_err(|_| format!("fps {s:?}"))?, d.parse().map_err(|_| format!("fps {s:?}"))?);
    Ok(n / d.max(1.0))
}

const SIDE: u32 = 256;

/// `coreml`: the phone's SigLIP package (8-bit palettized, the same as host's SigLIP2Image) through Core ML on the
/// Neural Engine and GPU, instead of the fp32 ONNX reference on the CPU; the column's model says which made it.
fn ingest(video: &Path, out: &str, fps: Option<f64>, int8: bool, coreml: bool) -> Result<(), String> {
    let rate = match fps {
        Some(f) => f,
        None => video_fps(video)?,
    };
    let mut vf = format!("scale={SIDE}:{SIDE}:flags=bilinear");
    if let Some(f) = fps {
        vf = format!("fps={f},{vf}");
    }
    let mut child = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(video)
        .args(["-an", "-vf", &vf, "-pix_fmt", "rgb24", "-f", "rawvideo", "-"])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg: {e}"))?;
    let mut stdout = child.stdout.take().unwrap();
    enum Enc {
        Onnx(raea_embed::Vision),
        CoreMl(raea_perception::coreml::Model),
    }
    let mut enc = if coreml {
        Enc::CoreMl(raea_perception::coreml::Model::load("siglip2-base-p16-256", "SigLIP2Image", raea_perception::coreml::Units::CpuAndNe)?)
    } else {
        Enc::Onnx(raea_embed::Vision::load(&raea_embed::default_dir(), int8)?)
    };
    let mut c = Col::new(raea_embed::DIM);
    let mut buf = vec![0u8; (SIDE * SIDE * 3) as usize];
    let t0 = std::time::Instant::now();
    loop {
        match stdout.read_exact(&mut buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(format!("reading frames: {e}")),
        }
        match &mut enc {
            Enc::Onnx(vision) => {
                let img = image::RgbImage::from_raw(SIDE, SIDE, buf.clone()).ok_or("bad frame")?;
                c.push(&vision.embed_rgb(&img)?);
            }
            Enc::CoreMl(m) => {
                let bytes: Vec<u8> = buf.chunks_exact(3).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
                let img = raea_perception::coreml::Bgra { w: SIDE as usize, h: SIDE as usize, bytes };
                let (v, _) = m.predict(&[("image", &img)])?.floats("embedding")?;
                c.push(&k::normalise(&v[..raea_embed::DIM]));
            }
        }
    }
    child.wait().map_err(|e| e.to_string())?;
    let meta = Meta {
        // absolute: a library is searched from anywhere, and find() cuts frames out of the source
        source: std::fs::canonicalize(video).unwrap_or_else(|_| video.to_path_buf()).display().to_string(),
        model: format!("{}{}", raea_embed::MODEL_ID, if coreml { "-coreml-palettized8" } else if int8 { "-int8" } else { "" }),
        fps: rate,
        n: c.n(),
        dim: c.d,
    };
    save(out, &meta, &c)?;
    eprintln!("{} frames in {:.1}s -> {out}", c.n(), t0.elapsed().as_secs_f64());
    Ok(())
}

/// The space's fixed parts for tools outside Rust: DIR/roles.hv (one row per slot, in roles.json's order) and
/// DIR/levels.SLOT.hv (33 rows, amounts 0, 1/32 .. 1) for CHANGE, ONSET, LOUD, CUT and WHEN.
fn space_dump(dir: &str, bits: usize) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let sp = record::Space::new(bits);
    let names: Vec<&str> = record::MOMENT_SLOTS.iter().chain(record::WINDOW_SLOTS.iter()).copied().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    record::write_hv(&format!("{dir}/roles.hv"), &names.iter().map(|n| sp.role(n).clone()).collect::<Vec<_>>())?;
    for slot in ["CHANGE", "ONSET", "LOUD", "CUT", "WHEN"] {
        record::write_hv(&format!("{dir}/levels.{slot}.hv"), &(0..=32).map(|i| sp.level(slot, i as f32 / 32.0).clone()).collect::<Vec<_>>())?;
    }
    std::fs::write(format!("{dir}/roles.json"), serde_json::json!({"bits": bits, "roles": names, "level_steps": 32}).to_string()).map_err(|e| e.to_string())
}

/// Query bits in the space: --texts FILE (one phrase a line, SigLIP's text tower, then SEEN's projection) or
/// --f32 FILE --dim 768|1024 (raw vectors, rows; 768 -> SEEN's projection, 1024 -> HEARD's). Rows of OUT.hv.
fn qhv(args: &[String], out: &str, bits: usize) -> Result<(), String> {
    let flag = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let sp = record::Space::new(bits);
    let rows: Vec<k::Hv> = if let Some(f) = flag("--texts") {
        let mut t = raea_embed::Text::load(&raea_embed::default_dir())?;
        let text = std::fs::read_to_string(&f).map_err(|e| format!("{f}: {e}"))?;
        text.lines().filter(|l| !l.trim().is_empty()).map(|l| t.embed(l.trim()).map(|v| sp.seen(&v))).collect::<Result<_, _>>()?
    } else {
        let f = flag("--f32").ok_or("qhv needs --texts or --f32")?;
        let dim: usize = flag("--dim").and_then(|d| d.parse().ok()).ok_or("--dim 768|1024")?;
        let b = std::fs::read(&f).map_err(|e| format!("{f}: {e}"))?;
        let v: Vec<f32> = b.chunks_exact(4).map(|x| f32::from_le_bytes([x[0], x[1], x[2], x[3]])).collect();
        v.chunks_exact(dim).map(|r| if dim == 768 { sp.seen(r) } else { sp.heard(r) }).collect()
    };
    record::write_hv(out, &rows)
}

/// A clip's records (src/record.rs, the roadmap Track L): OUT.moments.hv and OUT.windows.hv (u64 words, bits/64 a
/// row), OUT.json (slots, rates, window starts, per-moment levels); with --fillers, each slot's filler beside them
/// (OUT.m.SLOT.hv, OUT.w.SLOT.hv). YAMNet COL is raea-embed audio-frames' (COL.emb.f32 + COL.json); log-mel COL is
/// 64 bands every 10 ms (COL.f32 + COL.json).
fn encode(siglip: &str, yamnet: Option<String>, logmel: Option<String>, out: &str, bits: usize, fillers: bool, tracks: Vec<String>) -> Result<(), String> {
    // --track NAME=FILE: a JSON list of per-moment amounts in 0..1 (at the SigLIP column's rate)
    let tracks: Vec<(String, Vec<f32>)> = tracks
        .iter()
        .map(|t| {
            let (name, file) = t.split_once('=').ok_or(format!("--track NAME=FILE, not {t}"))?;
            let v: Vec<f32> = serde_json::from_str(&std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?).map_err(|e| format!("{file}: {e}"))?;
            Ok((name.to_string(), v))
        })
        .collect::<Result<_, String>>()?;
    let (meta, c) = load(siglip)?;
    let read_f32 = |p: &str| -> Result<Vec<f32>, String> {
        let b = std::fs::read(p).map_err(|e| format!("{p}: {e}"))?;
        Ok(b.chunks_exact(4).map(|x| f32::from_le_bytes([x[0], x[1], x[2], x[3]])).collect())
    };
    let y = match &yamnet {
        Some(col) => Some(read_f32(&format!("{col}.emb.f32"))?),
        None => None,
    };
    let l = match &logmel {
        Some(col) => Some(read_f32(&format!("{}.f32", col.trim_end_matches(".f32")))?),
        None => None,
    };
    let d = k::neighbour_distance(&c);
    let cuts = k::spikes(&d, 12, 4.0, 0.05, (0.25 * meta.fps).round().max(1.0) as usize);
    let t0 = std::time::Instant::now();
    let space = record::Space::new(bits);
    let src = record::Sources { siglip: &c, fps: meta.fps, yamnet: y.as_deref().filter(|v| !v.is_empty()), logmel: l.as_deref().filter(|v| !v.is_empty()), cuts: &cuts, tracks, seen_bits: None };
    let e = space.encode(&src, fillers);
    record::write_hv(&format!("{out}.moments.hv"), &e.moments)?;
    record::write_hv(&format!("{out}.windows.hv"), &e.windows)?;
    for (slot, rows) in &e.moment_fillers {
        record::write_hv(&format!("{out}.m.{slot}.hv"), rows)?;
    }
    for (slot, rows) in &e.window_fillers {
        record::write_hv(&format!("{out}.w.{slot}.hv"), rows)?;
    }
    let slots = |f: &Vec<(&'static str, Vec<k::Hv>)>| f.iter().map(|(s, _)| *s).collect::<Vec<&'static str>>();
    let meta_out = serde_json::json!({
        "source": meta.source, "siglip": siglip, "yamnet": yamnet, "logmel": logmel, "bits": bits, "fps": meta.fps,
        "moments": e.moments.len(), "windows": e.windows.len(), "window_frames": (2.0 * meta.fps).round(), "window_starts": e.window_starts,
        "moment_slots": if fillers { serde_json::json!(slots(&e.moment_fillers)) } else { serde_json::json!(record::MOMENT_SLOTS) },
        "window_slots": if fillers { serde_json::json!(slots(&e.window_fillers)) } else { serde_json::json!(record::WINDOW_SLOTS) },
        "cuts_s": cuts.iter().map(|&i| i as f64 / meta.fps).collect::<Vec<_>>(),
        "levels": e.levels.iter().map(|l| l.iter().map(|x| if x.is_nan() { serde_json::Value::Null } else { serde_json::json!((x * 1000.0).round() / 1000.0) }).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "level_columns": ["CHANGE", "ONSET", "LOUD", "CUT", "WHEN"],
    });
    std::fs::write(format!("{out}.json"), serde_json::to_string(&meta_out).unwrap()).map_err(|e| e.to_string())?;
    eprintln!("{} moments, {} windows in {:.2}s -> {out}", e.moments.len(), e.windows.len(), t0.elapsed().as_secs_f64());
    Ok(())
}

/// Times the HDC sandwich's layers on a real frame column (src/hdc_time.rs, shared with the phone probe ios-hdc).
fn hdc_time(col: &str) -> Result<(), String> {
    let (meta, c) = load(col)?;
    println!("{}", hdc_time::run(meta.dim, &c.data, col));
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let num = |n: &str, d: f64| flag(n).and_then(|v| v.parse().ok()).unwrap_or(d);
    let r = (|| -> Result<(), String> {
        match args.first().map(String::as_str) {
            Some("projection") => {
                // the projection's planes as f32 [bits][dim] little-endian, for a Core ML model that ends in it
                let dim = num("--dim", 768.0) as usize;
                let bits = num("--bits", 8192.0) as usize;
                let seed = num("--seed", if dim == 1024 { 11.0 } else { 7.0 }) as u64;
                let p = k::Projection::new(dim, bits, seed);
                let out = flag("-o").ok_or("projection [--dim 768] [--bits 8192] [--seed 7] -o OUT.f32")?;
                std::fs::write(&out, p.planes().iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).map_err(|e| e.to_string())
            }
            Some("space") => space_dump(&flag("-o").ok_or("space -o DIR")?, num("--bits", 8192.0) as usize),
            Some("qhv") => qhv(&args, &flag("-o").ok_or("qhv (--texts FILE | --f32 FILE --dim 768|1024) -o OUT.hv")?, num("--bits", 8192.0) as usize),
            Some("encode") => encode(
                args.get(1).ok_or("encode SIGLIP_COL [--yamnet COL] [--logmel COL] -o OUT [--bits 8192] [--fillers]")?,
                flag("--yamnet"),
                flag("--logmel"),
                &flag("-o").ok_or("encode needs -o OUT")?,
                num("--bits", 8192.0) as usize,
                args.iter().any(|a| a == "--fillers"),
                args.windows(2).filter(|w| w[0] == "--track").map(|w| w[1].clone()).collect(),
            ),
            Some("hdc-time") => hdc_time(args.get(1).ok_or("hdc-time COL")?),
            Some("ingest") => {
                let video = args.get(1).ok_or("ingest VIDEO -o COL")?;
                let out = flag("-o").ok_or("ingest VIDEO -o COL")?;
                ingest(Path::new(video), &out, flag("--fps").and_then(|f| f.parse().ok()), args.iter().any(|a| a == "--int8"), args.iter().any(|a| a == "--coreml"))
            }
            Some("dist") => {
                let (meta, c) = load(args.get(1).ok_or("dist COL")?)?;
                println!("{}", serde_json::json!({"fps": meta.fps, "d": k::neighbour_distance(&c)}));
                Ok(())
            }
            Some("search") => {
                let (meta, c) = load(args.get(1).ok_or("search COL TEXT")?)?;
                let text = args.get(2).ok_or("search COL TEXT")?;
                let mut model = raea_embed::Text::load(&raea_embed::default_dir())?;
                let q = model.embed(text)?;
                let s = k::dots(&c, &q);
                let hits: Vec<_> = k::top_k(&s, num("-k", 10.0) as usize)
                    .into_iter()
                    .map(|i| serde_json::json!({"frame": i, "t": i as f64 / meta.fps, "score": s[i]}))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&hits).unwrap());
                Ok(())
            }
            Some("find") => {
                // every column in a folder against one phrase: the library's best frames, best first
                let dir = args.get(1).ok_or("find LIBDIR TEXT")?;
                let text = args.get(2).ok_or("find LIBDIR TEXT")?;
                let mut model = raea_embed::Text::load(&raea_embed::default_dir())?;
                let q = model.embed(text)?;
                let mut hits: Vec<(f32, String, usize, f64)> = Vec::new();
                let mut cols: Vec<PathBuf> = std::fs::read_dir(dir).map_err(|e| format!("{dir}: {e}"))?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
                cols.sort();
                for c in cols {
                    let (meta, col) = load(&c.display().to_string())?;
                    let s = k::dots(&col, &q);
                    // one hit per clip: its best frame
                    if let Some(&i) = k::top_k(&s, 1).first() {
                        hits.push((s[i], meta.source.clone(), i, i as f64 / meta.fps));
                    }
                }
                hits.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
                hits.truncate(num("-k", 5.0) as usize);
                let out: Vec<_> = hits.iter().map(|(s, src, i, t)| serde_json::json!({"score": s, "source": src, "frame": i, "t": t})).collect();
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
                Ok(())
            }
            _ => Err("usage: raea-meaning ingest|dist|search|find ... (see src/main.rs)".into()),
        }
    })();
    if let Err(e) = r {
        eprintln!("raea-meaning: {e}");
        std::process::exit(1);
    }
}
