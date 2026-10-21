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
//!   raea-perceive ingest CLIP [--store DIR]                     the clip ingested once, by content hash: facts, sound, performance, SigLIP
//!   raea-perceive analyze VIDEO DIR                             a clip's facts (facts.json and a still per shot), as `ugc-perceive VIDEO DIR`
//!   raea-perceive performance-video VIDEO [FPS]                 the creator's performance second by second, all in Rust
//!   raea-perceive sound VIDEO                                   what's heard, in spans (YAMNet), as facts.json's sound
//!   raea-perceive see-layout IMAGE...                          each picture's layout end to end in Rust (Vision through objc2), one JSON line each
//!   raea-perceive read-lines IMAGE... [--backend apple|ocrs]   every text line read in each picture (text, confidence, box), one JSON line each
//!   raea-perceive see-coach IMAGE...                           the framing coach end to end in Rust, one JSON line each
//!   raea-perceive see-camera VIDEO                             each shot's camera end to end in Rust (shots from analyze)
//!   raea-perceive cuts VIDEO                                   every frame's scene score and YDIF, and the visible cuts
//!   raea-perceive transnet-frames RAW.rgb                       TransNetV2 on frames already at 48 x 27 RGB (rdb previz's sketch-48x27.rgb)
//!   raea-perceive transnet VIDEO                               TransNetV2's per-frame shot-end probabilities (ONNX through ort)
//!   raea-perceive flow VIDEO [--fps 8] [--side 256]             optical flow per frame pair (Vision): a 3x3 grid, pan, zoom
//!   raea-perceive track OUTDIR X,Y,W,H IMAGE...               an object tracked from a box (EdgeTAM): a matte per frame
//!   raea-perceive hands IMAGE...                               each picture's hands through Core ML, one JSON line each
//!   raea-perceive depth-values IMAGE OUT.f32                    1/depth through Core ML, as `ugc-perceive depth-values` writes it
//!   raea-perceive performance INPUTS.json                       Looks from the logged meshes, and the performance, from
//!                                                               `ugc-perceive performance VIDEO --inputs`

use raea_perception::{camera, coach, layout, performance, shots};
use serde_json::json;

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
                let (img, out) = (args.get(1).ok_or("depth-values IMAGE OUT.f32")?, args.get(2).ok_or("depth-values IMAGE OUT.f32")?);
                let rgba = image::open(img).map_err(|e| format!("{img}: {e}"))?.to_rgba8();
                let (w, h) = (rgba.width() as usize, rgba.height() as usize);
                let (v, ww, hh) = raea_perception::depth::inverse_depth(&raea_perception::depth::model()?, rgba.as_raw(), w, h)?;
                std::fs::write(out, v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()).map_err(|e| e.to_string())?;
                println!("{}", json!({"width": ww, "height": hh}));
                Ok(())
            }
            #[cfg(target_vendor = "apple")]
            Some("check-face") => {
                use raea_perception::face;
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
                    let names = raea_perception::performance::BLENDSHAPES;
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
                use raea_perception::object;
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
                    let mut d = raea_media::backend("apple")?.open_video(&video)?;
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
                use raea_perception::ingest;
                let clip = args.get(1).ok_or("ingest CLIP [--store DIR]")?;
                let store = flag("--store").map(std::path::PathBuf::from).unwrap_or_else(ingest::store);
                let t = std::time::Instant::now();
                let (dir, m) = ingest::ingest(std::path::Path::new(clip), &store, "apple", &raea_perception::vision::AppleVision::default())?;
                println!("{}", json!({"dir": dir, "hash": m.hash, "steps": m.steps, "seconds": t.elapsed().as_secs_f64()}));
                Ok(())
            }
            #[cfg(target_vendor = "apple")]
            Some("analyze") => {
                let (video, dir) = (args.get(1).ok_or("analyze VIDEO DIR")?, args.get(2).ok_or("analyze VIDEO DIR")?);
                let backend = flag("--backend").unwrap_or_else(|| "apple".into());
                let f = raea_perception::ingest::video(std::path::Path::new(video), std::path::Path::new(dir), &backend, &raea_perception::vision::AppleVision::default())?;
                println!("{}", serde_json::to_string(&f).unwrap());
                Ok(())
            }
            #[cfg(target_vendor = "apple")]
            Some("performance-video") => {
                let video = args.get(1).ok_or("performance-video VIDEO [FPS]")?;
                let fps = args.get(2).and_then(|f| f.parse().ok()).unwrap_or(15.0);
                println!("{}", serde_json::to_string(&raea_perception::ingest::performance(std::path::Path::new(video), "apple", fps)?).unwrap());
                Ok(())
            }
            #[cfg(target_vendor = "apple")]
            Some("sound") => {
                let video = args.get(1).ok_or("sound VIDEO")?;
                let spans = raea_perception::sound::spans(&raea_perception::sound::model()?, std::path::Path::new(video), "apple")?;
                println!("{}", json!({"sound": spans}));
                Ok(())
            }
            Some("see-layout") => {
                // --backend apple (Vision, the default on Apple) | portable (open models, the vision-portable feature)
                let v = vision_backend(flag("--backend"))?;
                for path in args[1..].iter().filter(|a| !a.starts_with("--") && Some(a.as_str()) != flag("--backend").as_deref()) {
                    let Ok(img) = image::open(path) else { continue };
                    let img = img.to_rgba8();
                    let l = raea_perception::vision::layout(v.as_ref(), img.as_raw(), img.width() as usize, img.height() as usize, &[])?;
                    println!("{}", json!({"file": path, "layout": l}));
                }
                Ok(())
            }
            Some("legible") => {
                // legible RENDER.json WITH_DIR WITHOUT_DIR [--backend B]: each sampled frame painted as rendered and with
                // its glyphs hidden (RAEA_MOTION_TEXT=hidden), the PNGs in frame order beside render.json's frames;
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
                    let d = raea_perception::legible::row_dark(ib.as_raw(), ib.width() as usize, ib.height() as usize);
                    dark = if dark.is_empty() { d } else { dark.iter().zip(d).map(|(x, y)| *x && y).collect() };
                }
                let bars = raea_perception::legible::bars(&dark);
                for ((fr, pa), pb) in frames.iter().zip(&a).zip(&b) {
                    if fr["text"].as_array().is_none_or(|t| t.is_empty()) {
                        continue;
                    }
                    let (ia, ib) = (image::open(pa).map_err(|e| e.to_string())?.to_rgba8(), image::open(pb).map_err(|e| e.to_string())?.to_rgba8());
                    let (w, h) = (ib.width() as usize, ib.height() as usize);
                    let (att, seen) = (v.attention(ib.as_raw(), w, h)?, v.attention(ia.as_raw(), w, h)?);
                    let (faces, _) = v.faces(ib.as_raw(), w, h)?;
                    let lines = raea_perception::legible::lines(ia.as_raw(), ib.as_raw(), w, h, fr, comp, att.as_ref(), seen.as_ref(), &faces, bars);
                    let lines: Vec<_> = lines.iter().map(|l| l.json()).collect();
                    let spread = att.as_ref().map(raea_perception::legible::spread);
                    println!("{}", json!({"frame": fr["frame"], "file": pa, "faces": faces.len(), "spread": spread, "bars": [bars.0, bars.1], "fill": att.as_ref().map(|a| raea_perception::legible::fill(ib.as_raw(), w, h, a)), "lines": lines}));
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
                    let d = raea_perception::legible::row_dark(i.as_raw(), i.width() as usize, i.height() as usize);
                    dark = if dark.is_empty() { d } else { dark.iter().zip(d).map(|(x, y)| *x && y).collect() };
                }
                let bars = raea_perception::legible::bars(&dark);
                let mut looks = Vec::new();
                for (k, p) in pngs.iter().enumerate() {
                    let i = image::open(p).map_err(|e| e.to_string())?.to_rgba8();
                    let (w, h) = (i.width() as usize, i.height() as usize);
                    let att = v.attention(i.as_raw(), w, h)?;
                    let (faces, _) = v.faces(i.as_raw(), w, h)?;
                    let l = raea_perception::framing::look(k as f64 * every, i.as_raw(), w, h, att.as_ref(), &faces, bars);
                    println!("{}", json!({"look": l.json()}));
                    looks.push(l);
                }
                for f in raea_perception::framing::faults(&looks, &cuts, every) {
                    println!("{}", json!({"fault": f}));
                }
                Ok(())
            }
            Some("read-lines") => {
                // --backend apple (Vision, the default on Apple) | ocrs (portable, the ocr-portable feature)
                let backend = flag("--backend").unwrap_or_else(|| if cfg!(target_vendor = "apple") { "apple".into() } else { "ocrs".into() });
                #[cfg(feature = "ocr-portable")]
                let portable = if backend == "ocrs" { Some(raea_perception::ocr_portable::Reader::new()?) } else { None };
                for path in args[1..].iter().filter(|a| !a.starts_with("--") && *a != &backend) {
                    let Ok(img) = image::open(path) else { continue };
                    let lines = match backend.as_str() {
                        #[cfg(feature = "ocr-portable")]
                        "ocrs" => { let rgb = img.to_rgb8(); portable.as_ref().unwrap().lines(rgb.as_raw(), rgb.width(), rgb.height())? }
                        #[cfg(target_vendor = "apple")]
                        "apple" => { let img = img.to_rgba8(); raea_perception::vision::text_lines(img.as_raw(), img.width() as usize, img.height() as usize)? }
                        other => return Err(format!("read-text: no backend {other} in this build").into()),
                    };
                    let lines: Vec<_> = lines.iter().map(|(t, c, b)| json!({"text": t, "confidence": c, "box": b})).collect();
                    println!("{}", json!({"file": path, "lines": lines}));
                }
                Ok(())
            }
            Some("faces") => {
                // --backend apple (Vision, the default on Apple) | yunet (portable, the face-portable feature)
                let backend = flag("--backend").unwrap_or_else(|| if cfg!(target_vendor = "apple") { "apple".into() } else { "yunet".into() });
                #[cfg(feature = "face-portable")]
                let portable = if backend == "yunet" { Some(raea_perception::face_portable::Detector::new()?) } else { None };
                #[cfg(target_vendor = "apple")]
                let v = raea_perception::vision::AppleVision::default();
                for path in args[1..].iter().filter(|a| !a.starts_with("--") && *a != &backend) {
                    let Ok(img) = image::open(path) else { continue };
                    let faces: Vec<([f64; 4], f32)> = match backend.as_str() {
                        #[cfg(feature = "face-portable")]
                        "yunet" => { let rgb = img.to_rgb8(); portable.as_ref().unwrap().faces(rgb.as_raw(), rgb.width(), rgb.height(), 0.6)? }
                        #[cfg(target_vendor = "apple")]
                        "apple" => {
                            use raea_perception::vision::Vision as _;
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
                    let c = raea_perception::vision::coach(v.as_ref(), img.as_raw(), img.width() as usize, img.height() as usize, 0.0, 0.0)?;
                    println!("{}", json!({"file": path, "coach": c}));
                }
                Ok(())
            }
            #[cfg(target_vendor = "apple")]
            Some("see-camera") => {
                let video = std::path::Path::new(args.get(1).ok_or("see-camera VIDEO")?);
                let v = raea_perception::vision::AppleVision::default();
                let dir = std::env::temp_dir().join(format!("raea-camera-{}", std::process::id()));
                let facts = raea_perception::ingest::video(video, &dir, "apple", &v)?;
                let _ = std::fs::remove_dir_all(&dir);
                let shots: Vec<(f64, f64)> = facts.shots.iter().map(|s| (s.start, s.end)).collect();
                let cams = raea_perception::ingest::camera(video, &shots, "apple", &v)?;
                let rows: Vec<_> = shots.iter().zip(cams).map(|((a, b), c)| json!({"start": a, "end": b, "camera": c})).collect();
                println!("{}", serde_json::to_string(&rows).unwrap());
                Ok(())
            }
            #[cfg(target_vendor = "apple")]
            Some("cuts") => {
                let video = std::path::Path::new(args.get(1).ok_or("cuts VIDEO")?);
                let s = raea_perception::cuts::signals(video, "apple")?;
                println!("{}", json!({"cuts": raea_perception::cuts::visible_cuts(&s), "signals": s}));
                Ok(())
            }
            Some("transnet-frames") => {
                let path = args.get(1).ok_or("transnet-frames RAW.rgb")?;
                let raw = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
                let frames: Vec<Vec<u8>> = raw.chunks_exact(48 * 27 * 3).map(|c| c.to_vec()).collect();
                let (single, many) = raea_perception::transnet::TransNet::load()?.predict(&frames)?;
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
                    raea_perception::denoise::pcm48k(path, &backend)?
                };
                let t = std::time::Instant::now();
                let y = raea_perception::denoise::denoise(&x)?;
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
                let (fps, n, rows) = raea_perception::textread::read(path, &backend)?;
                if args.iter().any(|a| a == "--gherkin") {
                    // J4: the overview, as Gherkin
                    let (w, h) = { let d = raea_media::backend(&backend)?.open_video(path)?; let i = d.info().clone(); (i.width as f64, i.height as f64) };
                    let name = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                    print!("{}", raea_perception::textread::overview(&name, &rows, fps, w, h));
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
                    let a = raea_media::backend(&backend)?.decode_audio(path)?;
                    let a = raea_media::soft_resample(&a, raea_perception::stems::SR);
                    let l = a.channels.first().cloned().unwrap_or_default();
                    let r = a.channels.get(1).cloned().unwrap_or_else(|| l.clone());
                    [l, r]
                };
                let t = std::time::Instant::now();
                let stems = raea_perception::stems::Demucs::load()?.separate(&mix)?;
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                for (name, st) in raea_perception::stems::SOURCES.iter().zip(&stems) {
                    let inter: Vec<u8> = (0..st[0].len()).flat_map(|i| [st[0][i], st[1][i]]).flat_map(|v| v.to_le_bytes()).collect();
                    std::fs::write(dir.join(format!("{name}.f32")), inter).map_err(|e| e.to_string())?;
                }
                println!("{}", json!({"samples": mix[0].len(), "rtf": t.elapsed().as_secs_f64() / (mix[0].len() as f64 / 44_100.0)}));
                Ok(())
            }
            Some("vad") => {
                // Silero VAD v5: speech spans of a file's sound (a .f32 file is raw 16 kHz mono)
                let path = std::path::Path::new(args.get(1).ok_or("vad FILE [--store DIR]")?);
                let backend = flag("--backend").unwrap_or_else(|| if cfg!(target_vendor = "apple") { "apple".into() } else { "ffmpeg".into() });
                let j = raea_perception::vad::file(path, &backend)?;
                // --store DIR (or $RAEA_INGEST): kept as vad.json beside the source's other ingest tracks
                if args.iter().any(|a| a == "--store") || std::env::var_os("RAEA_INGEST").is_some() {
                    let store = flag("--store").map(std::path::PathBuf::from).unwrap_or_else(raea_perception::ingest::store);
                    let dir = store.join(&raea_perception::ingest::content_hash(path)?[..16]);
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    std::fs::write(dir.join("vad.json"), j.to_string()).map_err(|e| e.to_string())?;
                }
                println!("{j}");
                Ok(())
            }
            Some("beats") => {
                // Beat This! small: beats and downbeats of a file's sound; --spect OUT.f32 also writes the log-mel frames
                let path = std::path::Path::new(args.get(1).ok_or("beats FILE [--store DIR] [--spect OUT.f32] [--pcm OUT.f32]")?);
                let backend = flag("--backend").unwrap_or_else(|| if cfg!(target_vendor = "apple") { "apple".into() } else { "ffmpeg".into() });
                if let Some(out) = flag("--pcm") {
                    let x = raea_perception::beatthis::pcm(path, &backend)?;
                    std::fs::write(&out, x.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>()).map_err(|e| e.to_string())?;
                }
                if let Some(out) = flag("--spect") {
                    let x = raea_perception::beatthis::pcm(path, &backend)?;
                    let s = raea_perception::beatthis::spectrogram(&x);
                    std::fs::write(&out, s.iter().flat_map(|r| r.iter().flat_map(|v| v.to_le_bytes())).collect::<Vec<u8>>()).map_err(|e| e.to_string())?;
                }
                let j = raea_perception::beatthis::file(path, &backend)?;
                // --store DIR (or $RAEA_INGEST): kept as beats.json beside the source's other ingest tracks
                if args.iter().any(|a| a == "--store") || std::env::var_os("RAEA_INGEST").is_some() {
                    let store = flag("--store").map(std::path::PathBuf::from).unwrap_or_else(raea_perception::ingest::store);
                    let dir = store.join(&raea_perception::ingest::content_hash(path)?[..16]);
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    std::fs::write(dir.join("beats.json"), j.to_string()).map_err(|e| e.to_string())?;
                }
                println!("{j}");
                Ok(())
            }
            Some("song") => {
                // the song's energy map (raea_perception::song): sections from the file's [Section] markers when it
                // has them (--srt, a .srt beside it, or its own timed-text stream), else from novelty
                let path = std::path::Path::new(args.get(1).ok_or("song FILE [--srt S.srt] [-o OUT.json]")?);
                let backend = flag("--backend").unwrap_or_else(|| if cfg!(target_vendor = "apple") { "apple".into() } else { "ffmpeg".into() });
                let srt = flag("--srt").map(std::path::PathBuf::from);
                let j = raea_perception::song::file(path, &backend, srt.as_deref())?;
                match flag("-o") {
                    Some(out) => std::fs::write(&out, serde_json::to_string_pretty(&j).unwrap()).map_err(|e| format!("{out}: {e}"))?,
                    None => println!("{j}"),
                }
                Ok(())
            }
            Some("transnet") => {
                let video = std::path::Path::new(args.get(1).ok_or("transnet VIDEO")?);
                let backend = flag("--backend").unwrap_or_else(|| if cfg!(target_vendor = "apple") { "apple".into() } else { "ffmpeg".into() });
                let (fps, frames) = raea_perception::transnet::frames(video, &backend)?;
                let t = std::time::Instant::now();
                let (single, many) = raea_perception::transnet::TransNet::load()?.predict(&frames)?;
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
                                let (a, b) = (raea_perception::coreml::Bgra { w: side, h: side, bytes: p.clone() }, raea_perception::coreml::Bgra { w: side, h: side, bytes: buf.clone() });
                                let f = raea_perception::flow::flow(&a, &b)?;
                                (f.grid, f.dx, f.dy, f.zoom, f.mag)
                            }
                            "lk" => {
                                let f = raea_perception::flow_portable::flow(p, &buf, side, side);
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
                let prompt = raea_perception::object::Prompt::Box([b[0], b[1], b[2], b[3]]);
                let m = raea_perception::object::models()?;
                let mut tracker = raea_perception::object::Tracker::new(&m);
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
            Some("hands") => {
                let m = raea_perception::hands::models()?;
                for path in &args[1..] {
                    let Ok(img) = image::open(path) else { continue };
                    let img = img.to_rgba8();
                    let reads = raea_perception::hands::hands(&m, img.as_raw(), img.width() as usize, img.height() as usize, 2)?;
                    println!("{}", json!({"file": path, "hands": reads}));
                }
                Ok(())
            }
            #[cfg(target_vendor = "apple")]
            Some("check-hair") => {
                let dir = std::path::PathBuf::from(args.get(1).ok_or("check-hair REFERENCE_DIR")?);
                let model = raea_perception::hair::model()?;
                let mut pass = true;
                let mut names: Vec<String> = std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten()
                    .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix("_input.png")).map(String::from)).collect();
                names.sort();
                let grey = |p: std::path::PathBuf| -> Result<Vec<u8>, String> { Ok(image::open(&p).map_err(|e| format!("{}: {e}", p.display()))?.to_luma8().into_raw()) };
                for name in names {
                    let input = image::open(dir.join(format!("{name}_input.png"))).map_err(|e| e.to_string())?.to_rgba8();
                    let (w, h) = (input.width() as usize, input.height() as usize);
                    let t = std::time::Instant::now();
                    let ours = raea_perception::hair::mask(&model, input.as_raw(), w, h)?;
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
    })();
    if let Err(e) = r {
        eprintln!("raea-perceive: {e}");
        std::process::exit(1);
    }
}

/// The Vision behind see-layout and see-coach: apple (the default on Apple) or portable (open models; docs/checks.feature
/// in host records which uses match Vision).
fn vision_backend(name: Option<String>) -> Result<Box<dyn raea_perception::vision::Vision>, String> {
    match name.as_deref().unwrap_or(if cfg!(target_vendor = "apple") { "apple" } else { "portable" }) {
        #[cfg(target_vendor = "apple")]
        "apple" => Ok(Box::new(raea_perception::vision::AppleVision::default())),
        #[cfg(feature = "vision-portable")]
        "portable" => Ok(Box::new(raea_perception::portable::PortableVision::new()?)),
        other => Err(format!("no vision backend {other} in this build")),
    }
}
