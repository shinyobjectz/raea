//! raea-perceive: perception in Rust, from a shell.
//!
//!   raea-perceive samples VIDEO [--backend apple|ffmpeg|soft]   samples and shot bounds, as `ugc-perceive samples` prints
//!   raea-perceive bounds SAMPLES.json                           shot bounds from someone else's samples (the Swift's)
//!   raea-perceive layout INPUTS.json                            a frame's layout from `ugc-perceive layout-inputs`
//!   raea-perceive coach INPUTS.json PIXELS.rgb                  the framing coach from `ugc-perceive coach-inputs`
//!   raea-perceive camera INPUTS.json                            each shot's camera from `ugc-perceive camera VIDEO --inputs`
//!   raea-perceive check-hair REFERENCE_DIR                      the hair matte through Core ML against MediaPipe's (IoU bar 0.997)
//!   raea-perceive check-face REFERENCE_DIR                      the face models through Core ML against MediaPipe's (bar 0.6% of face width)
//!   raea-perceive check-object REFERENCE_DIR                    EdgeTAM through Core ML against the official predictor (mean IoU 0.99, min 0.97)
//!   raea-perceive denoise FILE -o OUT.f32                      DeepFilterNet3 (feature denoise): the sound denoised, 48 kHz
//!   raea-perceive read-text VIDEO [--gherkin]                  J1: text read off every frame, tracked into rows (J4: as Gherkin)
//!   raea-perceive stems FILE -o DIR                            HTDemucs: drums, bass, other, vocals (raw 44.1 kHz stereo)
//!   raea-perceive vad FILE [--store DIR]                       Silero VAD v5: speech spans (kept as vad.json)
//!   raea-perceive beats FILE [--store DIR]                     Beat This! small: beats and downbeats (kept as beats.json)
//!   raea-perceive song FILE [--srt S.srt|none] [-o OUT.json]    the song's energy map: beats, bars, per-beat energy, sections,
//!                                                               builds, drops, stops, hits, dips, pacing (song.rs)
//!   raea-perceive ingest CLIP [--store DIR]                     the clip ingested once, by content hash: facts, sound, performance
//!   raea-perceive analyze VIDEO DIR                             a clip's facts (facts.json and a still per shot), as `ugc-perceive VIDEO DIR`
//!   raea-perceive performance-video VIDEO [FPS]                 the creator's performance second by second, all in Rust
//!   raea-perceive sound VIDEO                                   what's heard, in spans (YAMNet), as facts.json's sound
//!   raea-perceive see-layout IMAGE...                          each picture's layout end to end in Rust (Vision through objc2), one JSON line each
//!   raea-perceive read-lines IMAGE... [--backend apple|ocrs]   every text line read in each picture (text, confidence, box), one JSON line each
//!   raea-perceive see-coach IMAGE...                           the framing coach end to end in Rust, one JSON line each
//!   raea-perceive see-camera VIDEO                             each shot's camera end to end in Rust (shots from analyze)
//!   raea-perceive cuts VIDEO                                   every frame's scene score and YDIF, and the visible cuts
//!   raea-perceive transnet-frames RAW.rgb                       TransNetV2 on frames already at 48 x 27 RGB (the 48x27 sketch previz wrote before f1b4a17e)
//!   raea-perceive transnet VIDEO                               TransNetV2's per-frame shot-end probabilities (ONNX through ort)
//!   raea-perceive flow VIDEO [--fps 8] [--side 256]             optical flow per frame pair (Vision): a 3x3 grid, pan, zoom
//!   raea-perceive track OUTDIR X,Y,W,H IMAGE...               an object tracked from a box (EdgeTAM): a matte per frame
//!   raea-perceive hands IMAGE...                               each picture's hands through Core ML, one JSON line each
//!   raea-perceive depth-values IMAGE OUT.f32 [IMAGE OUT.f32...] 1/depth through Core ML, as `ugc-perceive depth-values` writes it
//!   raea-perceive performance INPUTS.json                       Looks from the logged meshes, and the performance, from
//!                                                               `ugc-perceive performance VIDEO --inputs`

use raea_sys_vision::camera;
use raea_sys_layout::coach;
use raea_sys_layout::layout;
use raea_sys_footage::performance;
use raea_sys_footage::shots;
use serde_json::json;

mod more;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let r = (|| -> Result<(), String> {
        match args.first().map(String::as_str) {
            Some("samples") => {
                let video = args.get(1).ok_or("samples VIDEO")?;
                let backend = flag("--backend").unwrap_or_else(|| if cfg!(target_vendor = "apple") { "apple".into() } else { "ffmpeg".into() });
                let (duration, s) = shots::samples(std::path::Path::new(video), &backend)?;
                let bounds = shots::shot_bounds(&s, duration);
                println!("{}", json!({"duration": duration, "fps": shots::sample_rate(duration), "samples": s, "bounds": bounds}));
                Ok(())
            }
            Some("bounds") => {
                let path = args.get(1).ok_or("bounds SAMPLES.json")?;
                let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| e.to_string())?;
                let s: Vec<shots::Sample> = serde_json::from_value(j["samples"].clone()).map_err(|e| e.to_string())?;
                let bounds = shots::shot_bounds(&s, j["duration"].as_f64().unwrap_or(0.0));
                println!("{}", json!({"bounds": bounds}));
                Ok(())
            }
            Some("layout") => {
                let path = args.get(1).ok_or("layout INPUTS.json")?;
                let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| e.to_string())?;
                println!("{}", serde_json::to_string(&layout::layout(&layout::inputs_from_json(&j)?)).unwrap());
                Ok(())
            }
            Some("coach") => {
                let (path, px) = (args.get(1).ok_or("coach INPUTS.json PIXELS.rgb")?, args.get(2).ok_or("coach INPUTS.json PIXELS.rgb")?);
                let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| e.to_string())?;
                let i = coach::inputs_from_json(&j);
                let rgb = std::fs::read(px).map_err(|e| format!("{px}: {e}"))?;
                let w = i.w;
                let read = move |x: usize, y: usize| { let o = (y * w + x) * 3; (rgb[o], rgb[o + 1], rgb[o + 2]) };
                println!("{}", serde_json::to_string(&coach::coach(&i, 0.0, 0.0, Some(&read))).unwrap());
                Ok(())
            }
            Some("camera") => {
                let path = args.get(1).ok_or("camera INPUTS.json")?;
                let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                for s in j["shots"].as_array().ok_or("no shots")? {
                    let windows: Vec<camera::Window> = serde_json::from_value(s["windows"].clone()).map_err(|e| e.to_string())?;
                    let room: Option<Vec<f64>> = serde_json::from_value(s["room"].clone()).ok();
                    let n = |k: &str| s[k].as_u64().unwrap_or(0) as usize;
                    out.push(camera::summary(&windows, n("images"), n("width"), n("height"), s["side"].as_str().unwrap_or(""), room));
                }
                println!("{}", serde_json::to_string(&out).unwrap());
                Ok(())
            }
            Some("performance") => {
                let path = args.get(1).ok_or("performance INPUTS.json")?;
                let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| e.to_string())?;
                let floats = |v: &serde_json::Value| -> Vec<f32> { v.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default() };
                let mut from_meshes = Vec::new();
                for m in j["meshes"].as_array().cloned().unwrap_or_default() {
                    let points: Vec<Vec<f64>> = serde_json::from_value(m["points"].clone()).map_err(|e| e.to_string())?;
                    let last = if m["last"].is_array() { Some(floats(&m["last"])) } else { None };
                    from_meshes.push(performance::look(&points, &floats(&m["blendshapes"]), m["t"].as_f64().unwrap_or(0.0), last.as_deref()));
                }
                let looks: Vec<performance::Look> = serde_json::from_value(j["looks"].clone()).map_err(|e| e.to_string())?;
                let times: Vec<f64> = serde_json::from_value(j["frameTimes"].clone()).map_err(|e| e.to_string())?;
                let gestures: Vec<(f64, String)> = j["gestures"].as_array().cloned().unwrap_or_default().iter()
                    .map(|g| (g["t"].as_f64().unwrap_or(0.0), g["gesture"].as_str().unwrap_or("").to_string())).collect();
                let mut p = performance::summarize(&looks, &times, j["fps"].as_f64().unwrap_or(15.0), j["duration"].as_f64().unwrap_or(0.0), &floats(&j["loudness"]));
                performance::add_gestures(&mut p, &gestures, j["hands"].as_bool().unwrap_or(false));
                println!("{}", json!({"looks": from_meshes, "performance": p}));
                Ok(())
            }
            #[cfg(target_vendor = "apple")]
            Some("depth-values") => {
                // IMAGE OUT.f32 [IMAGE OUT.f32...]: the model loaded once for every pair, a size line each
                let pairs = &args[1..];
                if pairs.is_empty() || pairs.len() % 2 != 0 {
                    return Err("depth-values IMAGE OUT.f32 [IMAGE OUT.f32...]".into());
                }
                let m = raea_sys_vision::depth::model()?;
                for p in pairs.chunks(2) {
                    let rgba = image::open(&p[0]).map_err(|e| format!("{}: {e}", p[0]))?.to_rgba8();
                    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
                    let (v, ww, hh) = raea_sys_vision::depth::inverse_depth(&m, rgba.as_raw(), w, h)?;
                    std::fs::write(&p[1], v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).map_err(|e| e.to_string())?;
                    println!("{}", json!({"width": ww, "height": hh}));
                }
                Ok(())
            }
            #[cfg(target_vendor = "apple")]
            Some("check-face") => {
                use raea_sys_vision::face;
                let dir = std::path::PathBuf::from(args.get(1).ok_or("check-face REFERENCE_DIR")?);
                let m = face::models()?;
                let mut pass = true;
                let mut refs: Vec<std::path::PathBuf> = std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten().map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
                refs.sort();
                for r in refs {
                    let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&r).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                    let file = j["image"].as_str().unwrap_or("");
                    let img = image::open(dir.join(file)).map_err(|e| format!("{file}: {e}"))?.to_rgba8();
                    let (w, h) = (img.width() as usize, img.height() as usize);
                    let pts = |v: &serde_json::Value| -> Vec<[f64; 2]> { v.as_array().map(|a| a.iter().map(|p| [p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0)]).collect()).unwrap_or_default() };
                    let (mp, cm) = (pts(&j["mediapipe"]["landmarks"]), pts(&j["coreml"]["landmarks"]));
                    let width = j["coreml_vs_mediapipe"]["face_width_px"].as_f64().unwrap_or(1.0);
                    let t = std::time::Instant::now();
                    let Some(crop) = face::detect(&m, img.as_raw(), w, h, false)? else { println!("{file}: no face  FAIL"); pass = false; continue };
                    let Some(mesh) = face::mesh(&m, img.as_raw(), w, h, crop)? else { println!("{file}: no face  FAIL"); pass = false; continue };
                    let ms = t.elapsed().as_secs_f64() * 1000.0;
                    let dist = |a: &[[f64; 3]], b: &[[f64; 2]]| { let d: Vec<f64> = a.iter().zip(b).map(|(p, q)| (p[0] - q[0]).hypot(p[1] - q[1])).collect(); (d.iter().sum::<f64>() / d.len() as f64, d.iter().copied().fold(0.0, f64::max)) };
                    let (vs_mp, vs_cm) = (dist(&mesh.points, &mp), dist(&mesh.points, &cm));
                    let names = raea_sys_footage::performance::BLENDSHAPES;
                    let shapes: Vec<f64> = names.iter().zip(&mesh.blendshapes).map(|(n, v)| (*v as f64 - j["mediapipe"]["blendshapes"][n].as_f64().unwrap_or(0.0)).abs()).collect();
                    let followed = face::mesh(&m, img.as_raw(), w, h, face::crop_around(&mesh.points))?.map(|f| dist(&f.points, &mp).0);
                    let cc = &j["coreml"]["crop"];
                    let share = vs_mp.0 / width * 100.0;
                    pass &= share <= 0.6;
                    println!("{file}\n  crop vs coreml: centre {:.2} px, side {:.2} px, rotation {:.2} deg\n  points vs MediaPipe: mean {:.2} px ({:.2}% of face width {:.0} px), max {:.2} px   vs coreml: mean {:.2} px, max {:.2} px\n  blendshapes vs MediaPipe: max |d| {:.3}, mean {:.4}\n  followed: mean {:.2} px vs MediaPipe ({:.2}%)   {:.0} ms   {}",
                        (crop.cx - cc["center"][0].as_f64().unwrap_or(0.0)).hypot(crop.cy - cc["center"][1].as_f64().unwrap_or(0.0)), crop.side - cc["side"].as_f64().unwrap_or(0.0),
                        (crop.rotation - cc["rotation_rad"].as_f64().unwrap_or(0.0)).to_degrees(), vs_mp.0, share, width, vs_mp.1, vs_cm.0, vs_cm.1,
                        shapes.iter().copied().fold(0.0, f64::max), shapes.iter().sum::<f64>() / shapes.len() as f64,
                        followed.unwrap_or(-1.0), followed.unwrap_or(-1.0) / width * 100.0, ms, if share <= 0.6 { "pass" } else { "FAIL" });
                }
                if !pass { std::process::exit(1) }
                Ok(())
            }
            #[cfg(target_vendor = "apple")]
            Some("check-object") => {
                use raea_sys_vision::object;
                let dir = std::path::PathBuf::from(args.get(1).ok_or("check-object REFERENCE_DIR")?);
                let r: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("prompts.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                let (vw, vh, fps, frames) = (r["width"].as_f64().unwrap_or(0.0), r["height"].as_f64().unwrap_or(0.0), r["fps"].as_f64().unwrap_or(30.0), r["frames"].as_u64().unwrap_or(0) as usize);
                let video = dir.join(r["video"].as_str().unwrap_or(""));
                let m = object::models()?;
                let grey = |p: std::path::PathBuf| -> Result<Vec<u8>, String> {
                    let g = image::open(&p).map_err(|e| format!("{}: {e}", p.display()))?.to_luma8();
                    let g = if g.width() as f64 == vw && g.height() as f64 == vh { g } else { image::imageops::resize(&g, vw as u32, vh as u32, image::imageops::FilterType::Triangle) };
                    Ok(g.into_raw())
                };
                let iou = |a: &[u8], b: &[u8]| { let (mut both, mut either) = (0usize, 0usize); for i in 0..a.len() { let (x, y) = (a[i] > 127, b[i] > 127); both += (x && y) as usize; either += (x || y) as usize; } if either == 0 { 1.0 } else { both as f64 / either as f64 } };
                let mut pass = true;
                let mut names: Vec<String> = r["prompts"].as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
                names.sort();
                for name in names {
                    let p = &r["prompts"][&name];
                    let frame = p["frame"].as_u64().unwrap_or(0) as usize;
                    let pts: Vec<[f64; 2]> = p["points"].as_array().map(|a| a.iter().map(|q| [q[0].as_f64().unwrap_or(0.0), q[1].as_f64().unwrap_or(0.0)]).collect()).unwrap_or_default();
                    let prompt = if p.get("box").is_some() {
                        object::Prompt::Box([pts[0][0] / vw, pts[0][1] / vh, (pts[1][0] - pts[0][0]) / vw, (pts[1][1] - pts[0][1]) / vh])
                    } else {
                        object::Prompt::Point([pts[0][0] / vw, pts[0][1] / vh])
                    };
                    let mut d = raea_io_media::backend("apple")?.open_video(&video)?;
                    let mut tracker = object::Tracker::new(&m);
                    let (mut official, mut coreml, mut empties) = (Vec::new(), Vec::new(), 0usize);
                    let started = std::time::Instant::now();
                    for i in frame..frames {
                        let f = d.frame_at(i as f64 / fps)?;
                        let ours = tracker.step(&f.rgba, f.width as usize, f.height as usize, if i == frame { Some(&prompt) } else { None }, vw as usize, vh as usize)?;
                        if !ours.iter().any(|v| *v > 127) { empties += 1; }
                        let file = format!("{i:03}.png");
                        official.push(iou(&ours, &grey(dir.join(&name).join("official").join(&file))?));
                        coreml.push(iou(&ours, &grey(dir.join(&name).join("coreml").join(&file))?));
                    }
                    let ms = started.elapsed().as_secs_f64() * 1000.0 / (frames - frame).max(1) as f64;
                    let mean = official.iter().sum::<f64>() / official.len().max(1) as f64;
                    let (worst_at, worst) = official.iter().enumerate().fold((0, 1.0f64), |b, (i, v)| if *v < b.1 { (i, *v) } else { b });
                    let ok = mean >= 0.99 && worst >= 0.97 && empties == 0;
                    pass &= ok;
                    println!("{name} (prompted frame {frame}, {} frames at {vw}x{vh}, {ms:.0} ms/frame): vs official mean IoU {mean:.4}, min {worst:.4} (frame {}), {empties} empty; vs coreml mean {:.4}   {}",
                             frames - frame, worst_at + frame, coreml.iter().sum::<f64>() / coreml.len().max(1) as f64, if ok { "pass" } else { "FAIL" });
                }
                if !pass { std::process::exit(1) }
                Ok(())
            }
            #[cfg(target_vendor = "apple")]
            Some("ingest") => {
                use raea_sys_footage::ingest;
                let clip = args.get(1).ok_or("ingest CLIP [--store DIR]")?;
                let store = flag("--store").map(std::path::PathBuf::from).unwrap_or_else(ingest::store);
                let t = std::time::Instant::now();
                let (dir, m) = ingest::ingest(std::path::Path::new(clip), &store, "apple", &raea_sys_vision::vision::AppleVision::default())?;
                println!("{}", json!({"dir": dir, "hash": m.hash, "steps": m.steps, "seconds": t.elapsed().as_secs_f64()}));
                Ok(())
            }
            #[cfg(target_vendor = "apple")]
            Some("analyze") => {
                let (video, dir) = (args.get(1).ok_or("analyze VIDEO DIR [--shots S.json]")?, args.get(2).ok_or("analyze VIDEO DIR [--shots S.json]")?);
                let backend = flag("--backend").unwrap_or_else(|| "apple".into());
                // --shots S.json: the shots, [{start, end, transition}], when they're known already
                let given = match flag("--shots") {
                    Some(f) => Some(serde_json::from_slice(&std::fs::read(&f).map_err(|e| format!("{f}: {e}"))?).map_err(|e| format!("{f}: {e}"))?),
                    None => None,
                };
                let f = raea_sys_footage::ingest::video(std::path::Path::new(video), std::path::Path::new(dir), &backend, &raea_sys_vision::vision::AppleVision::default(), given)?;
                println!("{}", serde_json::to_string(&f).unwrap());
                Ok(())
            }
            #[cfg(target_vendor = "apple")]
            Some("performance-video") => {
                let video = args.get(1).ok_or("performance-video VIDEO [FPS]")?;
                let fps = args.get(2).and_then(|f| f.parse().ok()).unwrap_or(15.0);
                println!("{}", serde_json::to_string(&raea_sys_footage::ingest::performance(std::path::Path::new(video), "apple", fps)?).unwrap());
                Ok(())
            }
            #[cfg(target_vendor = "apple")]
            Some("sound") => {
                let video = args.get(1).ok_or("sound VIDEO")?;
                let spans = raea_sys_vision::sound::spans(&raea_sys_vision::sound::model()?, std::path::Path::new(video), "apple")?;
                println!("{}", json!({"sound": spans}));
                Ok(())
            }
            Some("see-layout") => {
                // --backend apple (Vision, the default on Apple) | portable (open models, the vision-portable feature)
                let v = vision_backend(flag("--backend"))?;
                for path in args[1..].iter().filter(|a| !a.starts_with("--") && Some(a.as_str()) != flag("--backend").as_deref()) {
                    let Ok(img) = image::open(path) else { continue };
                    let img = img.to_rgba8();
                    let l = raea_sys_vision::vision::layout(v.as_ref(), img.as_raw(), img.width() as usize, img.height() as usize, &[])?;
                    println!("{}", json!({"file": path, "layout": l}));
                }
                Ok(())
            }
            Some("legible") => {
                // legible RENDER.json WITH_DIR WITHOUT_DIR [--backend B]: each sampled frame painted as rendered and with
                // its glyphs hidden (RAEA_TEXT=hidden), the PNGs in frame order beside render.json's frames;
                // a line per frame with each text's contrast and cover of the focus (src/legible.rs)
                let pos: Vec<&String> = args[1..].iter().filter(|a| !a.starts_with("--") && Some(a.as_str()) != flag("--backend").as_deref()).collect();
                let [manifest, with, without] = pos[..] else { return Err("legible RENDER.json WITH_DIR WITHOUT_DIR".into()) };
                let m: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(manifest).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                let comp = (m["width"].as_f64().unwrap_or(1920.0), m["height"].as_f64().unwrap_or(1080.0));
                let frames = m["frames"].as_array().cloned().unwrap_or_default();
                let pngs = |d: &str| -> Result<Vec<std::path::PathBuf>, std::io::Error> {
                    let mut v: Vec<_> = std::fs::read_dir(d)?.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "png")).collect();
                    v.sort();
                    Ok(v)
                };
                let (a, b) = (pngs(with).map_err(|e| e.to_string())?, pngs(without).map_err(|e| e.to_string())?);
                if a.len() != frames.len() || b.len() != frames.len() {
                    return Err(format!("legible: {} frames in the manifest, {} and {} pictures", frames.len(), a.len(), b.len()).into());
                }
                let v = vision_backend(flag("--backend"))?;
                // the letterbox: rows black in every picture without its words
                let mut dark: Vec<bool> = Vec::new();
                for pb in &b {
                    let ib = image::open(pb).map_err(|e| e.to_string())?.to_rgba8();
                    let d = raea_sys_layout::legible::row_dark(ib.as_raw(), ib.width() as usize, ib.height() as usize);
                    dark = if dark.is_empty() { d } else { dark.iter().zip(d).map(|(x, y)| *x && y).collect() };
                }
                let bars = raea_sys_layout::legible::bars(&dark);
                for ((fr, pa), pb) in frames.iter().zip(&a).zip(&b) {
                    if fr["text"].as_array().is_none_or(|t| t.is_empty()) {
                        continue;
                    }
                    let (ia, ib) = (image::open(pa).map_err(|e| e.to_string())?.to_rgba8(), image::open(pb).map_err(|e| e.to_string())?.to_rgba8());
                    let (w, h) = (ib.width() as usize, ib.height() as usize);
                    let (att, seen) = (v.attention(ib.as_raw(), w, h)?, v.attention(ia.as_raw(), w, h)?);
                    let (faces, _) = v.faces(ib.as_raw(), w, h)?;
                    let lines = raea_sys_layout::legible::lines(ia.as_raw(), ib.as_raw(), w, h, fr, comp, att.as_ref(), seen.as_ref(), &faces, bars);
                    let lines: Vec<_> = lines.iter().map(|l| l.json()).collect();
                    let spread = att.as_ref().map(raea_sys_layout::legible::spread);
                    println!("{}", json!({"frame": fr["frame"], "file": pa, "faces": faces.len(), "spread": spread, "bars": [bars.0, bars.1], "fill": att.as_ref().map(|a| raea_sys_layout::legible::fill(ib.as_raw(), w, h, a)), "lines": lines}));
                }
                Ok(())
            }
            Some("framing") => {
                // framing DIR [--every S] [--cuts T,T,...] [--backend B]: a film's pictures sampled every S seconds (the
                // PNGs in DIR in time order, the first at 0), its words hidden or not; a line per picture with its
                // subject, then one per framing fault (src/framing.rs)
                let dir = args[1..].iter().find(|a| !a.starts_with("--") && ![flag("--every"), flag("--cuts"), flag("--backend")].contains(&Some(a.to_string()))).ok_or("framing DIR [--every S] [--cuts T,...]")?;
                let every: f64 = flag("--every").and_then(|s| s.parse().ok()).unwrap_or(0.5);
                let cuts: Vec<f64> = flag("--cuts").map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
                let mut pngs: Vec<_> = std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "png")).collect();
                pngs.sort();
                let v = vision_backend(flag("--backend"))?;
                let mut dark: Vec<bool> = Vec::new();
                for p in &pngs {
                    let i = image::open(p).map_err(|e| e.to_string())?.to_rgba8();
                    let d = raea_sys_layout::legible::row_dark(i.as_raw(), i.width() as usize, i.height() as usize);
                    dark = if dark.is_empty() { d } else { dark.iter().zip(d).map(|(x, y)| *x && y).collect() };
                }
                let bars = raea_sys_layout::legible::bars(&dark);
                let mut looks = Vec::new();
                for (k, p) in pngs.iter().enumerate() {
                    let i = image::open(p).map_err(|e| e.to_string())?.to_rgba8();
                    let (w, h) = (i.width() as usize, i.height() as usize);
                    let att = v.attention(i.as_raw(), w, h)?;
                    let (faces, _) = v.faces(i.as_raw(), w, h)?;
                    let l = raea_sys_layout::framing::look(k as f64 * every, i.as_raw(), w, h, att.as_ref(), &faces, bars);
                    println!("{}", json!({"look": l.json()}));
                    looks.push(l);
                }
                for f in raea_sys_layout::framing::faults(&looks, &cuts, every) {
                    println!("{}", json!({"fault": f}));
                }
                Ok(())
            }
            _ => more::run(&args, &flag),
        }
    })();
    if let Err(e) = r {
        eprintln!("raea-perceive: {e}");
        std::process::exit(1);
    }
}

/// The Vision behind see-layout and see-coach: apple (the default on Apple) or portable (open models; docs/checks.feature
/// in the host app's records which uses match Vision).
fn vision_backend(name: Option<String>) -> Result<Box<dyn raea_sys_vision::vision::Vision>, String> {
    match name.as_deref().unwrap_or(if cfg!(target_vendor = "apple") { "apple" } else { "portable" }) {
        #[cfg(target_vendor = "apple")]
        "apple" => Ok(Box::new(raea_sys_vision::vision::AppleVision::default())),
        #[cfg(feature = "vision-portable")]
        "portable" => Ok(Box::new(raea_sys_perceive::portable::PortableVision::new()?)),
        other => Err(format!("no vision backend {other} in this build")),
    }
}
