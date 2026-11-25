//! `raea media VERB ...`: the agent's media work, native and the same on the Mac and the iPhone (docs/media.feature M1).
//! What agents used ffmpeg and ffprobe for, on raea-media (the platform's decoders and encoders, Symphonia's audio):
//!
//!   raea media probe FILE                                   duration, size, fps, codec, audio (JSON)
//!   raea media still FILE --at S [--width W] -o OUT.png|.jpg   the frame showing at S (an image: itself), W px wide at most
//!   raea media still FILE --at S,S,... [--width W] -o DIR      a still at each, as DIR/0.jpg, DIR/1.jpg ...
//!   raea media sheet FILE... [--n 12] [--width 240] -o OUT  a contact sheet (n frames of one file, or each file's middle),
//!                                                           and OUT.labels.json: each cell's time or file, for raea show
//!   raea media audio FILE [--rate 16000] [--mono] [--from S --to S] -o OUT.wav|-   (- is f32le on stdout)
//!   raea media loudness FILE [--from S --to S]              integrated LUFS, mean and peak dB, true peak
//!   raea media motion FILE [--from S --to S] [--fps 6]      how much the picture changes (mean luma difference, YDIF)
//!   raea media cuts FILE [--threshold 0.08]                 shot changes (ffmpeg's scene score)
//!   raea media freezes FILE [--noise 0.001] [--min 2]       held pictures (ffmpeg's freezedetect)
//!   raea media proxy FILE [--height 720] -o OUT.mp4         a smaller copy
//!   raea media share FILE [--kbps 2600] [--height H] -o OUT.mp4   a copy to send: any player takes it, held to a bitrate
//!   raea media lod MODEL.glb [--level 1] [--texture 1024] -o OUT.glb   a model at a lower level of detail (triangles halved a level)
//!   raea media trim FILE --from S --to S -o OUT.mp4         a stretch, frame-exact (re-encoded)
//!   raea media concat A B ... -o OUT.mp4                    one after another (same aspect)
//!   raea media encode --size WxH [--fps 60] [--quality Q] [--audio FILE] -o OUT.mp4   raw RGBA frames on stdin, signed
//!
//! `raea ffmpeg ...` and `raea ffprobe ...` read the command lines models write by reflex and run them as these verbs; what
//! they can't read is refused with the verb or edit that does it. No ffmpeg binary is involved.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use raea_io_media::{Audio, Backend, EncodeOpts, Frame, Info};
use raea_render::image;
use serde_json::{Value, json};

// `raea media|ffmpeg|ffprobe|render|shots` as one in-process command line
mod command;
pub use command::command;
/// a model at a lower level of detail (docs/preview.feature PV3c)
mod lod;
mod fragments;
mod probe;
use probe::*;
mod picture;
use picture::*;
mod sound;
use sound::*;
mod change;
use change::*;
mod encode;
use encode::*;
mod ff;
use ff::*;
mod frames;
/// what `raea check` reads natively where there's no FFmpeg (apps/check read.rs)
pub use {change::{frames_of, freezes, times, video_span, yuv}, probe::{probe_of, standard_fps}, sound::audio_folded};

type R<T> = Result<T, String>;

pub fn backend() -> R<Box<dyn Backend>> {
    raea_io_media::backend(&raea_sys_frame::media_backend())
}

fn soft() -> R<Box<dyn Backend>> {
    raea_io_media::backend("soft")
}

/// A video's decoder from the first backend that reads it (raea_io_media::preferred: RAEA_MEDIA's, the platform's, the
/// portable one, then FFmpeg), so a video the platform's decoder can't read is read through FFmpeg: off a Mac, H.264
/// with B-frames, HEVC and VP9 (on Windows raea fetches its own FFmpeg the first time). Each backend's refusal is kept
/// for the error.
pub fn open_video(path: &Path) -> R<Box<dyn raea_io_media::VideoDecoder>> {
    let mut why = Vec::new();
    for b in raea_io_media::preferred() {
        match b.open_video(path) {
            Ok(d) => return Ok(d),
            Err(e) => why.push(format!("{}: {e}", b.name())),
        }
    }
    Err(format!("{}: no backend reads it ({})", path.display(), why.join("; ")))
}

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"];
const SWITCHES: &[&str] = &["mono"];

fn ext(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// One argument as the files it names. A shell that doesn't split a variable (zsh's `$FILES`) hands every name over as
/// one argument, which read as a single path fails as "No such file", or past 255 bytes as "File name too long" (#218):
/// an argument that isn't a file, but whose words each are, is those files.
fn names(a: &str) -> Vec<String> {
    let words: Vec<&str> = a.split_whitespace().collect();
    if words.len() > 1 && !Path::new(a).exists() && words.iter().all(|w| Path::new(w).exists()) {
        return words.into_iter().map(String::from).collect();
    }
    vec![a.to_string()]
}

/// `--name value` flags (and `-o`), switches, and positional arguments.
fn parse(args: &[String]) -> (Vec<String>, BTreeMap<String, String>) {
    let (mut pos, mut fl) = (Vec::new(), BTreeMap::new());
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-o" || (a.starts_with("--") && a.len() > 2) {
            let name = if a == "-o" { "o" } else { &a[2..] };
            if SWITCHES.contains(&name) {
                fl.insert(name.to_string(), "1".into());
            } else {
                fl.insert(name.to_string(), args.get(i + 1).cloned().unwrap_or_default());
                i += 1;
            }
        } else {
            pos.extend(names(a));
        }
        i += 1;
    }
    (pos, fl)
}

fn num(fl: &BTreeMap<String, String>, k: &str) -> R<Option<f64>> {
    fl.get(k).map(|v| v.parse::<f64>().map_err(|_| format!("--{k} takes a number (seconds), not {v:?}"))).transpose()
}

/// The verbs `raea media` answers here.
pub const VERBS: &[&str] = &["probe", "still", "sheet", "audio", "loudness", "motion", "cuts", "freezes", "proxy", "share", "lod", "trim", "concat", "encode", "help"];

pub fn cli(args: &[String]) -> i32 {
    let Some(verb) = args.first() else {
        println!("usage: raea media <probe|still|sheet|audio|loudness|motion|cuts|proxy|share|lod|trim|concat|encode> FILE ... (raea media help)");
        return 2;
    };
    let (pos, fl) = parse(&args[1..]);
    let r = run(verb, &pos, &fl);
    match r {
        Ok(Some(v)) => {
            println!("{v}");
            0
        }
        Ok(None) => 0,
        Err(e) => {
            println!("REFUSED  {e}");
            1
        }
    }
}

fn run(verb: &str, pos: &[String], fl: &BTreeMap<String, String>) -> R<Option<Value>> {
    let file = || pos.first().map(PathBuf::from).ok_or_else(|| format!("raea media {verb} FILE: which file?"));
    let out = || fl.get("o").filter(|o| !o.is_empty()).map(PathBuf::from).ok_or_else(|| format!("raea media {verb} needs -o OUT"));
    let range = || -> R<(Option<f64>, Option<f64>)> { Ok((num(fl, "from")?, num(fl, "to")?)) };
    match verb {
        "probe" => probe(&file()?).map(Some),
        "still" => {
            let width = fl.get("width").and_then(|n| n.parse().ok());
            // `--at T,T,...`: a still at each, one decode, into the folder OUT as 0.jpg, 1.jpg ...
            match fl.get("at").filter(|a| a.contains(',')) {
                Some(ats) => stills(&file()?, ats, &out()?, width),
                None => still(&file()?, num(fl, "at")?.unwrap_or(0.0), &out()?, width),
            }
            .map(Some)
        }
        "sheet" => {
            let files: Vec<PathBuf> = pos.iter().map(PathBuf::from).collect();
            let n = fl.get("n").and_then(|n| n.parse().ok()).unwrap_or(12);
            let w = fl.get("width").and_then(|n| n.parse().ok()).unwrap_or(240);
            sheet(&files, n, w, &out()?).map(Some)
        }
        "audio" => {
            let rate = fl.get("rate").and_then(|r| r.parse().ok()).unwrap_or(raea_io_media::audio::RATE);
            let (from, to) = range()?;
            // a WAV is 16-bit (the integer fold); `-o -` is f32le (the float fold)
            let wav = fl.get("o").is_some_and(|o| o != "-");
            let a = audio_folded(&file()?, rate, fl.contains_key("mono"), from, to, wav)?;
            let o = out()?;
            if o.as_os_str() == "-" {
                write_f32le(&a, &mut std::io::stdout().lock())?;
                return Ok(None);
            }
            write_wav(&a, &o)?;
            Ok(Some(json!({"wrote": o, "rate": a.rate, "channels": a.channels.len(), "seconds": round3(a.duration())})))
        }
        "loudness" => {
            let (from, to) = range()?;
            loudness(&file()?, from, to).map(Some)
        }
        "motion" => {
            let (from, to) = range()?;
            let fps = num(fl, "fps")?.unwrap_or(6.0);
            motion(&file()?, from, to, fps).map(|(v, _)| Some(v))
        }
        "freezes" => {
            let n = num(fl, "noise")?.unwrap_or(0.001);
            let min = num(fl, "min")?.unwrap_or(2.0);
            let f = freezes(&file()?, n, min)?;
            Ok(Some(json!({"file": file()?, "noise": n, "min": min, "freezes": f.iter().map(|(s, e)| json!({"start": round3(*s), "duration": e.map(|e| round3(e.0)), "end": e.map(|e| round3(e.1))})).collect::<Vec<_>>()})))
        }
        "cuts" => {
            let th = num(fl, "threshold")?.unwrap_or(0.08);
            let (from, to) = range()?;
            cuts(&file()?, th, from, to).map(|(v, _)| Some(v))
        }
        "lod" => {
            let level = fl.get("level").and_then(|v| v.parse().ok()).unwrap_or(1);
            lod::lod(&file()?, level, fl.get("texture").and_then(|v| v.parse().ok()).unwrap_or(1024), &out()?).map(Some)
        }
        "proxy" => {
            let h = fl.get("height").and_then(|h| h.parse().ok()).unwrap_or(720);
            encode(&[(file()?, None, None)], Some(h), &out()?, fl.get("kbps").and_then(|k| k.parse().ok())).map(Some)
        }
        "share" => {
            let cap = fl.get("kbps").and_then(|k| k.parse().ok()).unwrap_or(2600);
            share(&file()?, fl.get("height").and_then(|h| h.parse().ok()), &out()?, cap).map(Some)
        }
        "trim" => {
            let (from, to) = range()?;
            if from.is_none() && to.is_none() {
                return Err("raea media trim FILE --from S --to S -o OUT.mp4".into());
            }
            encode(&[(file()?, from, to)], None, &out()?, fl.get("kbps").and_then(|k| k.parse().ok())).map(Some)
        }
        "concat" => {
            if pos.len() < 2 {
                return Err("raea media concat A B ... -o OUT.mp4: two files or more".into());
            }
            let srcs: Vec<_> = pos.iter().map(|p| (PathBuf::from(p), None, None)).collect();
            encode(&srcs, None, &out()?, fl.get("kbps").and_then(|k| k.parse().ok())).map(Some)
        }
        "encode" => {
            let size = fl.get("size").and_then(|s| s.split_once('x')).and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))).ok_or("raea media encode --size WxH -o OUT.mp4 (raw RGBA frames on stdin)")?;
            let quality = fl.get("quality").map(|q| raea_io_media::parse_quality(q)).transpose()?;
            frames::frames(&out()?, size, num(fl, "fps")?.unwrap_or(60.0), quality, fl.get("audio").map(Path::new)).map(Some)
        }
        "help" => {
            println!("{}", include_str!("lib.rs").lines().take_while(|l| l.starts_with("//!")).map(|l| l.trim_start_matches("//!").strip_prefix(' ').unwrap_or("")).collect::<Vec<_>>().join("\n"));
            Ok(None)
        }
        v => Err(format!("no media verb {v:?}: probe, still, sheet, audio, loudness, motion, cuts, freezes, proxy, share, trim, concat, encode")),
    }
}
