//! raea-perceive's commands from read-lines on: text, eyes and faces, the coach and camera from stills, cuts,
//! denoise, stems, speech, beats, the song map, TransNet, flow, tracking, hands and the hair check.

use super::*;

pub(crate) fn run(args: &[String], flag: &dyn Fn(&str) -> Option<String>) -> Result<(), String> {
    match args.first().map(String::as_str) {
        // (with a backend: Vision on Apple, or the ocr-portable feature)
        #[cfg(any(target_vendor = "apple", feature = "ocr-portable"))]
        Some("read-lines") => {
            // --backend apple (Vision, the default on Apple) | ocrs (portable, the ocr-portable feature)
            let backend = flag("--backend").unwrap_or_else(|| if cfg!(target_vendor = "apple") { "apple".into() } else { "ocrs".into() });
            #[cfg(feature = "ocr-portable")]
            let portable = if backend == "ocrs" { Some(raea_sys_perceive::ocr_portable::Reader::new()?) } else { None };
            for path in args[1..].iter().filter(|a| !a.starts_with("--") && *a != &backend) {
                let Ok(img) = image::open(path) else { continue };
                let lines = match backend.as_str() {
                    #[cfg(feature = "ocr-portable")]
                    "ocrs" => { let rgb = img.to_rgb8(); portable.as_ref().unwrap().lines(rgb.as_raw(), rgb.width(), rgb.height())? }
                    #[cfg(target_vendor = "apple")]
                    "apple" => { let img = img.to_rgba8(); raea_sys_vision::vision::text_lines(img.as_raw(), img.width() as usize, img.height() as usize)? }
                    other => return Err(format!("read-text: no backend {other} in this build").into()),
                };
                let lines: Vec<_> = lines.iter().map(|(t, c, b)| json!({"text": t, "confidence": c, "box": b})).collect();
                println!("{}", json!({"file": path, "lines": lines}));
            }
            Ok(())
        }
        #[cfg(target_vendor = "apple")]
        Some("eyes") => {
            // eyes IMG...: a line per picture with the largest face's eyes, their midpoint in shares of the picture
            // from the top left, or null with no face: where <Video framing="face"> holds them (sdk/react
            // runtime/page/src/faces.rs). The face mesh's iris centres (points 468 and 473); BlazeFace's eye keypoints when
            // the mesh sees no face (they sit a few pixels low on a small face: 0.327 of face.mp4's height where
            // the irises are at 0.305)
            let m = raea_sys_vision::face::models()?;
            for path in args[1..].iter().filter(|a| !a.starts_with("--")) {
                let Ok(img) = image::open(path) else { continue };
                let img = img.to_rgba8();
                let (w, h) = (img.width() as usize, img.height() as usize);
                let eyes = match raea_sys_vision::face::detect(&m, img.as_raw(), w, h, true)? {
                    None => None,
                    Some(c) => {
                        let mesh = raea_sys_vision::face::mesh(&m, img.as_raw(), w, h, c)?.filter(|m| m.points.len() > 473);
                        let (r, l) = match &mesh {
                            Some(m) => ([m.points[468][0], m.points[468][1]], [m.points[473][0], m.points[473][1]]),
                            None => (c.eyes[0], c.eyes[1]),
                        };
                        Some(json!([(r[0] + l[0]) / 2.0 / w as f64, (r[1] + l[1]) / 2.0 / h as f64]))
                    }
                };
                println!("{}", json!({"file": path, "eyes": eyes}));
            }
            Ok(())
        }
        // (with a backend: Vision on Apple, or the face-portable feature)
        #[cfg(any(target_vendor = "apple", feature = "face-portable"))]
        Some("faces") => {
            // --backend apple (Vision, the default on Apple) | yunet (portable, the face-portable feature)
            let backend = flag("--backend").unwrap_or_else(|| if cfg!(target_vendor = "apple") { "apple".into() } else { "yunet".into() });
            #[cfg(feature = "face-portable")]
            let portable = if backend == "yunet" { Some(raea_sys_perceive::face_portable::Detector::new()?) } else { None };
            #[cfg(target_vendor = "apple")]
            let v = raea_sys_vision::vision::AppleVision::default();
            for path in args[1..].iter().filter(|a| !a.starts_with("--") && *a != &backend) {
                let Ok(img) = image::open(path) else { continue };
                let faces: Vec<([f64; 4], f32)> = match backend.as_str() {
                    #[cfg(feature = "face-portable")]
                    "yunet" => { let rgb = img.to_rgb8(); portable.as_ref().unwrap().faces(rgb.as_raw(), rgb.width(), rgb.height(), 0.6)? }
                    #[cfg(target_vendor = "apple")]
                    "apple" => {
                        use raea_sys_vision::vision::Vision as _;
                        let img = img.to_rgba8();
                        let p = v.people(img.as_raw(), img.width() as usize, img.height() as usize, false)?;
                        p.faces.iter().zip(p.face_conf.iter().chain(std::iter::repeat(&1.0))).map(|(r, c)| ([r.x, r.y, r.w, r.h], *c)).collect()
                    }
                    other => return Err(format!("faces: no backend {other} in this build").into()),
                };
                let faces: Vec<_> = faces.iter().map(|(b, c)| json!({"box": b, "confidence": c})).collect();
                println!("{}", json!({"file": path, "faces": faces}));
            }
            Ok(())
        }
        Some("see-coach") => {
            let v = vision_backend(flag("--backend"))?;
            for path in args[1..].iter().filter(|a| !a.starts_with("--") && Some(a.as_str()) != flag("--backend").as_deref()) {
                let Ok(img) = image::open(path) else { continue };
                let img = img.to_rgba8();
                let c = raea_sys_vision::vision::coach(v.as_ref(), img.as_raw(), img.width() as usize, img.height() as usize, 0.0, 0.0)?;
                println!("{}", json!({"file": path, "coach": c}));
            }
            Ok(())
        }
        #[cfg(target_vendor = "apple")]
        Some("see-camera") => {
            let video = std::path::Path::new(args.get(1).ok_or("see-camera VIDEO")?);
            let v = raea_sys_vision::vision::AppleVision::default();
            let dir = std::env::temp_dir().join(format!("raea-camera-{}", std::process::id()));
            let facts = raea_sys_footage::ingest::video(video, &dir, "apple", &v, None)?;
            let _ = std::fs::remove_dir_all(&dir);
            let shots: Vec<(f64, f64)> = facts.shots.iter().map(|s| (s.start, s.end)).collect();
            let cams = raea_sys_footage::ingest::camera(video, &shots, "apple", &v)?;
            let rows: Vec<_> = shots.iter().zip(cams).map(|((a, b), c)| json!({"start": a, "end": b, "camera": c})).collect();
            println!("{}", serde_json::to_string(&rows).unwrap());
            Ok(())
        }
        #[cfg(target_vendor = "apple")]
        Some("cuts") => {
            let video = std::path::Path::new(args.get(1).ok_or("cuts VIDEO")?);
            let s = raea_sys_footage::cuts::signals(video, "apple")?;
            println!("{}", json!({"cuts": raea_sys_footage::cuts::visible_cuts(&s), "signals": s}));
            Ok(())
        }
        Some("transnet-frames") => {
            let path = args.get(1).ok_or("transnet-frames RAW.rgb")?;
            let raw = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            let frames: Vec<Vec<u8>> = raw.chunks_exact(48 * 27 * 3).map(|c| c.to_vec()).collect();
            let (single, many) = raea_sys_song::transnet::TransNet::load()?.predict(&frames)?;
            println!("{}", json!({"n": frames.len(), "single": single, "many": many}));
            Ok(())
        }
        #[cfg(feature = "denoise")]
        Some("denoise") => {
            // DeepFilterNet3: a file's sound (or raw 48 kHz mono .f32) denoised, written as raw 48 kHz mono floats
            let path = std::path::Path::new(args.get(1).ok_or("denoise FILE -o OUT.f32")?);
            let out = flag("-o").ok_or("denoise FILE -o OUT.f32")?;
            let backend = flag("--backend").unwrap_or_else(|| if cfg!(target_vendor = "apple") { "apple".into() } else { "ffmpeg".into() });
            let x: Vec<f32> = if path.extension().is_some_and(|e| e == "f32") {
                std::fs::read(path).map_err(|e| e.to_string())?.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
            } else {
                raea_sys_song::denoise::pcm48k(path, &backend)?
            };
            let t = std::time::Instant::now();
            let y = raea_sys_song::denoise::denoise(&x)?;
            std::fs::write(&out, y.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>()).map_err(|e| e.to_string())?;
            println!("{}", json!({"samples": y.len(), "rtf": t.elapsed().as_secs_f64() / (x.len() as f64 / 48_000.0)}));
            Ok(())
        }
        #[cfg(target_vendor = "apple")]
        Some("read-text") => {
            // J1: every frame's text lines (Apple Vision) tracked into rows
            let path = std::path::Path::new(args.get(1).ok_or("read-text VIDEO")?);
            let backend = flag("--backend").unwrap_or_else(|| "apple".into());
            let t = std::time::Instant::now();
            let (fps, n, rows) = raea_sys_vision::textread::read(path, &backend)?;
            if args.iter().any(|a| a == "--gherkin") {
                // J4: the overview, as Gherkin
                let (w, h) = { let d = raea_io_media::backend(&backend)?.open_video(path)?; let i = d.info().clone(); (i.width as f64, i.height as f64) };
                let name = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                print!("{}", raea_sys_vision::textread::overview(&name, &rows, fps, w, h));
                return Ok(());
            }
            println!("{}", json!({"fps": fps, "frames": n, "rows": rows, "seconds": t.elapsed().as_secs_f64()}));
            Ok(())
        }
        Some("stems") => {
            // HTDemucs: a file's sound (or raw 44.1 kHz stereo interleaved .f32) as four stems, raw 44.1 kHz stereo
            // interleaved floats in DIR (drums.f32, bass.f32, other.f32, vocals.f32)
            let path = std::path::Path::new(args.get(1).ok_or("stems FILE -o DIR")?);
            let dir = std::path::PathBuf::from(flag("-o").ok_or("stems FILE -o DIR")?);
            let backend = flag("--backend").unwrap_or_else(|| if cfg!(target_vendor = "apple") { "apple".into() } else { "ffmpeg".into() });
            let mix: [Vec<f32>; 2] = if path.extension().is_some_and(|e| e == "f32") {
                let v: Vec<f32> = std::fs::read(path).map_err(|e| e.to_string())?.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
                [v.iter().step_by(2).copied().collect(), v.iter().skip(1).step_by(2).copied().collect()]
            } else {
                let a = raea_io_media::backend(&backend)?.decode_audio(path)?;
                let a = raea_io_media::soft_resample(&a, raea_sys_song::stems::SR);
                let l = a.channels.first().cloned().unwrap_or_default();
                let r = a.channels.get(1).cloned().unwrap_or_else(|| l.clone());
                [l, r]
            };
            let t = std::time::Instant::now();
            let stems = raea_sys_song::stems::Demucs::load()?.separate(&mix)?;
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            for (name, st) in raea_sys_song::stems::SOURCES.iter().zip(&stems) {
                let inter: Vec<u8> = (0..st[0].len()).flat_map(|i| [st[0][i], st[1][i]]).flat_map(|v| v.to_le_bytes()).collect();
                std::fs::write(dir.join(format!("{name}.f32")), inter).map_err(|e| e.to_string())?;
            }
            println!("{}", json!({"samples": mix[0].len(), "rtf": t.elapsed().as_secs_f64() / (mix[0].len() as f64 / 44_100.0)}));
            Ok(())
        }
        #[cfg(target_vendor = "apple")]
        Some("vad") => {
            // Silero VAD v5: speech spans of a file's sound (a .f32 file is raw 16 kHz mono)
            let path = std::path::Path::new(args.get(1).ok_or("vad FILE [--store DIR]")?);
            let backend = flag("--backend").unwrap_or_else(|| if cfg!(target_vendor = "apple") { "apple".into() } else { "ffmpeg".into() });
            let j = raea_sys_song::vad::file(path, &backend)?;
            // --store DIR (or $RAEA_INGEST): kept as vad.json beside the source's other ingest tracks
            if args.iter().any(|a| a == "--store") || std::env::var_os("RAEA_INGEST").is_some() {
                let store = flag("--store").map(std::path::PathBuf::from).unwrap_or_else(raea_sys_footage::store::store);
                let dir = store.join(&raea_sys_footage::store::content_hash(path)?[..16]);
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                std::fs::write(dir.join("vad.json"), j.to_string()).map_err(|e| e.to_string())?;
            }
            println!("{j}");
            Ok(())
        }
        Some("beats") => {
            // Beat This! small: beats and downbeats of a file's sound; --spect OUT.f32 also writes the log-mel frames
            let path = std::path::Path::new(args.get(1).ok_or("beats FILE [--store DIR] [--spect OUT.f32] [--pcm OUT.f32]")?);
            let backend = flag("--backend").unwrap_or_else(sound_backend);
            if let Some(out) = flag("--pcm") {
                let x = raea_sys_song::beatthis::pcm(path, &backend)?;
                std::fs::write(&out, x.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>()).map_err(|e| e.to_string())?;
            }
            if let Some(out) = flag("--spect") {
                let x = raea_sys_song::beatthis::pcm(path, &backend)?;
                let s = raea_sys_song::beatthis::spectrogram(&x);
                std::fs::write(&out, s.iter().flat_map(|r| r.iter().flat_map(|v| v.to_le_bytes())).collect::<Vec<u8>>()).map_err(|e| e.to_string())?;
            }
            let j = raea_sys_song::beatthis::file(path, &backend)?;
            // --store DIR (or $RAEA_INGEST): kept as beats.json beside the source's other ingest tracks
            if args.iter().any(|a| a == "--store") || std::env::var_os("RAEA_INGEST").is_some() {
                let store = flag("--store").map(std::path::PathBuf::from).unwrap_or_else(raea_sys_footage::store::store);
                let dir = store.join(&raea_sys_footage::store::content_hash(path)?[..16]);
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                std::fs::write(dir.join("beats.json"), j.to_string()).map_err(|e| e.to_string())?;
            }
            println!("{j}");
            Ok(())
        }
        Some("song") => {
            // the song's energy map (raea_sys_song::song): sections from the file's [Section] markers when it
            // has them (--srt, a .srt beside it, or its own timed-text stream), else from novelty
            let path = std::path::Path::new(args.get(1).ok_or("song FILE [--srt S.srt] [-o OUT.json]")?);
            let backend = flag("--backend").unwrap_or_else(sound_backend);
            let srt = flag("--srt").map(std::path::PathBuf::from);
            let j = raea_sys_song::song::file(path, &backend, srt.as_deref())?;
            match flag("-o") {
                Some(out) => std::fs::write(&out, serde_json::to_string_pretty(&j).unwrap()).map_err(|e| format!("{out}: {e}"))?,
                None => println!("{j}"),
            }
            Ok(())
        }
        Some("transnet") => {
            let video = std::path::Path::new(args.get(1).ok_or("transnet VIDEO")?);
            let backend = flag("--backend").unwrap_or_else(|| if cfg!(target_vendor = "apple") { "apple".into() } else { "ffmpeg".into() });
            let (fps, frames) = raea_sys_song::transnet::frames(video, &backend)?;
            let t = std::time::Instant::now();
            let (single, many) = raea_sys_song::transnet::TransNet::load()?.predict(&frames)?;
            let ms = t.elapsed().as_secs_f64() * 1000.0 / frames.len().max(1) as f64;
            println!("{}", json!({"fps": fps, "n": frames.len(), "single": single, "many": many, "ms_per_frame": ms}));
            Ok(())
        }
        Some("flow") => {
            // optical flow per frame pair at --fps (default 8), frames squashed to --side (default 256): one JSON
            // line per pair, {"t": seconds, "grid": [9], "dx", "dy", "zoom", "mag"} (flow.rs). --backend apple
            // (Vision, the default on Apple) | lk (pyramidal Lucas-Kanade, flow_portable.rs)
            let video = args.get(1).ok_or("flow VIDEO [--fps 8] [--side 256] [--backend apple|lk]")?;
            let backend = flag("--backend").unwrap_or_else(|| if cfg!(target_vendor = "apple") { "apple".into() } else { "lk".into() });
            let opt = |k: &str, d: f64| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(d);
            let (fps, side) = (opt("--fps", 8.0), opt("--side", 256.0) as usize);
            let mut child = std::process::Command::new("ffmpeg")
                .args(["-v", "error", "-i", video, "-an", "-vf", &format!("fps={fps},scale={side}:{side}:flags=bilinear"), "-pix_fmt", "bgra", "-f", "rawvideo", "-"])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| format!("ffmpeg: {e}"))?;
            let mut out = child.stdout.take().unwrap();
            let mut prev: Option<Vec<u8>> = None;
            let mut t = 0usize;
            loop {
                let mut buf = vec![0u8; side * side * 4];
                if std::io::Read::read_exact(&mut out, &mut buf).is_err() {
                    break;
                }
                if let Some(p) = &prev {
                    let (grid, dx, dy, zoom, mag) = match backend.as_str() {
                        #[cfg(target_vendor = "apple")]
                        "apple" => {
                            let (a, b) = (raea_sys_vision::coreml::Bgra { w: side, h: side, bytes: p.clone() }, raea_sys_vision::coreml::Bgra { w: side, h: side, bytes: buf.clone() });
                            let f = raea_sys_vision::flow::flow(&a, &b)?;
                            (f.grid, f.dx, f.dy, f.zoom, f.mag)
                        }
                        "lk" => {
                            let f = raea_sys_perceive::flow_portable::flow(p, &buf, side, side);
                            (f.grid, f.dx, f.dy, f.zoom, f.mag)
                        }
                        other => return Err(format!("flow: no backend {other} in this build").into()),
                    };
                    println!("{}", serde_json::json!({"t": t as f64 / fps, "grid": grid, "dx": dx, "dy": dy, "zoom": zoom, "mag": mag}));
                }
                prev = Some(buf);
                t += 1;
            }
            let _ = child.wait();
            Ok(())
        }
        #[cfg(target_vendor = "apple")]
        Some("track") => {
            // an object tracked through frames from a box on the first (EdgeTAM): a matte per frame at the
            // frame's own size, OUTDIR/<frame's name>.png (255 on the object), and a JSON line each with its box
            const USAGE: &str = "raea-perceive track OUTDIR X,Y,W,H IMG...   (the box in 0..1 of the first frame)";
            let out = std::path::PathBuf::from(args.get(1).ok_or(USAGE)?);
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            let b: Vec<f64> = args.get(2).ok_or(USAGE)?.split(',').filter_map(|v| v.trim().parse().ok()).collect();
            if b.len() != 4 || b.iter().any(|v| !(0.0..=1.0).contains(v)) {
                return Err(format!("box: X,Y,W,H in 0..1, got {:?}", args.get(2)));
            }
            let prompt = raea_sys_vision::object::Prompt::Box([b[0], b[1], b[2], b[3]]);
            let m = raea_sys_vision::object::models()?;
            let mut tracker = raea_sys_vision::object::Tracker::new(&m);
            for (i, p) in args[3..].iter().enumerate() {
                let img = image::open(p).map_err(|e| format!("{p}: {e}"))?.to_rgba8();
                let (w, h) = (img.width() as usize, img.height() as usize);
                let matte = tracker.step(img.as_raw(), w, h, if i == 0 { Some(&prompt) } else { None }, w, h)?;
                let n = matte.iter().filter(|v| **v > 127).count();
                let stem = std::path::Path::new(p).file_stem().and_then(|s| s.to_str()).unwrap_or("frame");
                image::GrayImage::from_raw(w as u32, h as u32, matte).ok_or("matte size")?.save(out.join(format!("{stem}.png"))).map_err(|e| e.to_string())?;
                println!("{}", json!({"file": p, "area": n as f64 / (w * h) as f64}));
            }
            Ok(())
        }
        #[cfg(target_vendor = "apple")]
        Some("hands") => {
            let m = raea_sys_vision::hands::models()?;
            for path in &args[1..] {
                let Ok(img) = image::open(path) else { continue };
                let img = img.to_rgba8();
                let reads = raea_sys_vision::hands::hands(&m, img.as_raw(), img.width() as usize, img.height() as usize, 2)?;
                println!("{}", json!({"file": path, "hands": reads}));
            }
            Ok(())
        }
        #[cfg(target_vendor = "apple")]
        Some("check-hair") => {
            let dir = std::path::PathBuf::from(args.get(1).ok_or("check-hair REFERENCE_DIR")?);
            let model = raea_sys_vision::hair::model()?;
            let mut pass = true;
            let mut names: Vec<String> = std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten()
                .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix("_input.png")).map(String::from)).collect();
            names.sort();
            let grey = |p: std::path::PathBuf| -> Result<Vec<u8>, String> { Ok(image::open(&p).map_err(|e| format!("{}: {e}", p.display()))?.to_luma8().into_raw()) };
            for name in names {
                let input = image::open(dir.join(format!("{name}_input.png"))).map_err(|e| e.to_string())?.to_rgba8();
                let (w, h) = (input.width() as usize, input.height() as usize);
                let t = std::time::Instant::now();
                let ours = raea_sys_vision::hair::mask(&model, input.as_raw(), w, h)?;
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                let (mp, cm) = (grey(dir.join(format!("{name}_mediapipe.png")))?, grey(dir.join(format!("{name}_coreml.png")))?);
                let (mut both, mut either, mut sum, mut levels, mut off) = (0usize, 0usize, 0.0f64, 0i32, 0usize);
                for i in 0..ours.len() {
                    let (a, b) = (ours[i] > 127, mp[i] > 127);
                    both += (a && b) as usize;
                    either += (a || b) as usize;
                    sum += (ours[i] as f64 - mp[i] as f64).abs() / 255.0;
                    let c = (ours[i] as i32 - cm[i] as i32).abs();
                    levels = levels.max(c);
                    off += (c > 2) as usize;
                }
                let iou = if either > 0 { both as f64 / either as f64 } else { 1.0 };
                pass &= iou >= 0.997;
                println!("{name:<20} vs MediaPipe: IoU {iou:.4}  mean |d| {:.5}   vs coreml.png: max {levels} levels, {off} px off by > 2   {:.0} ms   {}",
                         sum / ours.len() as f64, ms, if iou >= 0.997 { "pass" } else { "FAIL" });
            }
            if !pass { std::process::exit(1) }
            Ok(())
        }
        _ => Err("usage: raea-perceive ingest CLIP [--store DIR] | beats FILE | vad FILE | analyze | sound | cuts | transnet | samples VIDEO [--backend B] | bounds | layout | coach | camera | performance ... (the module comment lists every command)".into()),
    }
}

/// Where a song's sound is decoded by default: AVFoundation on Apple, else in process (the soft backend: Symphonia), since
/// an install elsewhere has no ffmpeg (#43); `--backend ffmpeg` reads what Symphonia doesn't
fn sound_backend() -> String {
    if cfg!(target_vendor = "apple") { "apple" } else { "soft" }.into()
}
