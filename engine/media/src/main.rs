//! raea-media: the bake-off bench.
//!
//!   raea-media info BACKEND FILE
//!   raea-media decode BACKEND FILE [--n 300]            sequential decode speed
//!   raea-media seek BACKEND FILE --ref REF               forward vs shuffled frames identical; SSIM vs REF (ffmpeg)
//!   raea-media encode BACKEND FILE -o OUT.mp4 [--n 300] [--kbps 8000] [--audio A]   encode frames decoded (by ffmpeg) from FILE
//!   raea-media audio FILE...                             decode with every backend; loudness, peak

use std::path::{Path, PathBuf};
use std::time::Instant;

use raea_media::{EncodeOpts, R, backend, ssim};

fn hash(b: &[u8]) -> u64 {
    // FNV-1a: enough to compare frames
    let mut h = 0xcbf29ce484222325u64;
    for x in b {
        h ^= *x as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |n: &str| {
        args.iter()
            .position(|a| a == n)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let r = run(&args, &flag);
    if let Err(e) = r {
        eprintln!("raea-media: {e}");
        std::process::exit(1);
    }
}

fn run(args: &[String], flag: &dyn Fn(&str) -> Option<String>) -> R<()> {
    let a = |i: usize| {
        args.get(i)
            .cloned()
            .ok_or("missing argument (see src/main.rs)".to_string())
    };
    match a(0)?.as_str() {
        "info" => {
            let d = backend(&a(1)?)?.open_video(Path::new(&a(2)?))?;
            println!("{}", serde_json::to_string(d.info()).unwrap());
        }
        "decode" => {
            let b = backend(&a(1)?)?;
            let t0 = Instant::now();
            let mut d = b.open_video(Path::new(&a(2)?))?;
            let open = t0.elapsed().as_secs_f64();
            let n: usize = flag("--n")
                .and_then(|x| x.parse().ok())
                .unwrap_or(300)
                .min(d.info().frames);
            let fps = d.info().fps;
            let t1 = Instant::now();
            let mut h = 0u64;
            for i in 0..n {
                let f = d.frame_at(i as f64 / fps)?;
                h ^= hash(&f.rgba).rotate_left((i % 64) as u32);
            }
            let s = t1.elapsed().as_secs_f64();
            println!(
                "{}",
                serde_json::json!({"backend": b.name(), "frames": n, "open_s": open, "decode_s": s, "fps": n as f64 / s, "hash": format!("{h:016x}")})
            );
        }
        "stress" => {
            // many random seeks in one process (a leak shows as growth in the decoder or its service)
            let b = backend(&a(1)?)?;
            let mut d = b.open_video(Path::new(&a(2)?))?;
            let n: usize = flag("--n").and_then(|x| x.parse().ok()).unwrap_or(600);
            let dur = d.info().duration;
            let mut s = 0x9e37_79b9u64;
            let t0 = Instant::now();
            for k in 0..n {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let t = (s >> 11) as f64 / (1u64 << 53) as f64 * (dur - 0.1);
                let _ = d.frame_at(t)?;
                if k % 100 == 99 {
                    eprintln!("stress {} seeks", k + 1);
                }
            }
            println!("{}", serde_json::json!({"backend": b.name(), "seeks": n, "seconds": t0.elapsed().as_secs_f64()}));
        }
        "playable" => {
            #[cfg(all(feature = "apple", target_vendor = "apple"))]
            for f in &args[1..] {
                println!("{}", raea_media::apple::playable(Path::new(f))?);
            }
            #[cfg(not(all(feature = "apple", target_vendor = "apple")))]
            return Err("playable needs the apple backend".into());
        }
        "nv12" => {
            // NV12 decode: any-order identity, and against the same backend's RGBA (NV12 -> RGBA on the CPU), and speed
            let b = backend(&a(1)?)?;
            let file = PathBuf::from(a(2)?);
            let mut d = b.open_video(&file)?;
            let (dur, fps) = (d.info().duration, d.info().fps);
            let (w, h) = (d.info().width as usize, d.info().height as usize);
            let times: Vec<f64> = (0..24).map(|k| ((k * 37 % 24) as f64 / 24.0 * (dur - 2.0 / fps)).max(0.0)).collect();
            let mut sorted = times.clone();
            sorted.sort_by(f64::total_cmp);
            let mut fwd = std::collections::HashMap::new();
            for t in &sorted {
                fwd.insert(t.to_bits(), hash(&d.frame_nv12_at(*t)?.rgba));
            }
            let mut d2 = b.open_video(&file)?;
            let same = times.iter().filter(|t| d2.frame_nv12_at(**t).map(|f| hash(&f.rgba)).ok() == fwd.get(&t.to_bits()).copied()).count();
            let mut d3 = b.open_video(&file)?;
            let mut ss = Vec::new();
            for t in sorted.iter().step_by(4) {
                let n = d2.frame_nv12_at(*t)?;
                let r = d3.frame_at(*t)?;
                ss.push(ssim(&raea_media::yuv::nv12_to_rgba(&n.rgba, w, h), &r.rgba, w, h));
            }
            let mut d4 = b.open_video(&file)?;
            let n = 300.min(d4.info().frames);
            let c0 = raea_media::NV12_COPY_NS.load(std::sync::atomic::Ordering::Relaxed);
            let t0 = Instant::now();
            for i in 0..n {
                d4.frame_nv12_at(i as f64 / fps)?;
            }
            let copy_ms = (raea_media::NV12_COPY_NS.load(std::sync::atomic::Ordering::Relaxed) - c0) as f64 / 1e6 / n as f64;
            println!("{}", serde_json::json!({"backend": b.name(), "file": file.display().to_string(), "nv12_any_order": format!("{same}/24"),
                "ssim_nv12_vs_rgba_min": ss.iter().cloned().fold(1.0, f64::min), "nv12_decode_fps": n as f64 / t0.elapsed().as_secs_f64(), "nv12_copy_out_ms_per_frame": copy_ms}));
        }
        "index" => {
            // index_at (no decoding) against the decoded frame's own position, at 60 random times
            let b = backend(&a(1)?)?;
            let mut d = b.open_video(Path::new(&a(2)?))?;
            let dur = d.info().duration;
            let mut s = 7u64;
            let mut agree = 0;
            let mut seen: std::collections::HashMap<usize, u64> = Default::default();
            for _ in 0..60 {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let t = (s >> 11) as f64 / (1u64 << 53) as f64 * (dur - 0.1);
                let i = d.index_at(t).ok_or("no index_at")?;
                let fr = d.frame_nv12_at(t)?;
                let h = hash(&fr.rgba);
                // and the index's own time must be the decoded frame's time
                let j = d.index_at(fr.pts).ok_or("no index_at")?;
                if j != i { println!("t {t:.4}: index {i}, decoded frame at {:.4} has index {j}", fr.pts); }
                // the same index must always mean the same picture
                if *seen.entry(i).or_insert(h) == h { agree += 1; }
            }
            let t0 = Instant::now();
            for k in 0..1000 { d.index_at(k as f64 * dur / 1000.0); }
            println!("{}", serde_json::json!({"backend": b.name(), "consistent": format!("{agree}/60"), "index_at_us": t0.elapsed().as_secs_f64() * 1e3}));
        }
        "offset" => {
            // which reference frame each of the backend's frames matches best (k frames away), at 12 times
            let b = backend(&a(1)?)?;
            let file = PathBuf::from(a(2)?);
            let mut d = b.open_video(&file)?;
            let mut r = backend("ffmpeg")?.open_video(&file)?;
            let (dur, fps) = (d.info().duration, d.info().fps);
            let (w, h) = (d.info().width as usize, d.info().height as usize);
            let mut out = Vec::new();
            for k in 0..12 {
                let t = (k as f64 + 0.5) / 12.0 * (dur - 0.2);
                let f = d.frame_at(t)?;
                let scores: Vec<f64> = (-2i32..=2).map(|o| r.frame_at((t + o as f64 / fps).max(0.0)).map(|g| ssim(&f.rgba, &g.rgba, w, h)).unwrap_or(0.0)).collect();
                let best = (0..5).max_by(|&i, &j| scores[i].total_cmp(&scores[j])).unwrap() as i32 - 2;
                out.push(serde_json::json!({"t": t, "best_offset": best, "ssim_at_0": scores[2], "ssim_best": scores[(best + 2) as usize]}));
            }
            println!("{}", serde_json::Value::Array(out));
        }
        "seek" => {
            let b = backend(&a(1)?)?;
            let file = PathBuf::from(a(2)?);
            let reference = flag("--ref");
            let mut d = b.open_video(&file)?;
            let (dur, fps) = (d.info().duration, d.info().fps);
            // 24 times across the clip, including keyframes, mid-GOP frames and the last frame
            let times: Vec<f64> = (0..24)
                .map(|k| ((k * 37 % 24) as f64 / 24.0 * (dur - 2.0 / fps)).max(0.0))
                .collect();
            let mut forward: Vec<(f64, u64, Vec<u8>)> = Vec::new();
            let mut sorted = times.clone();
            sorted.sort_by(f64::total_cmp);
            for t in &sorted {
                let f = d.frame_at(*t)?;
                forward.push((*t, hash(&f.rgba), f.rgba));
            }
            let mut d2 = b.open_video(&file)?;
            let mut same = 0;
            for t in &times {
                let f = d2.frame_at(*t)?;
                if forward.iter().find(|x| x.0 == *t).map(|x| x.1) == Some(hash(&f.rgba)) {
                    same += 1;
                }
            }
            let mut out = serde_json::json!({"backend": b.name(), "seeks": times.len(), "identical_any_order": same});
            if let Some(r) = reference {
                let mut rd = backend("ffmpeg")?.open_video(Path::new(&r))?;
                let (w, h) = (d.info().width as usize, d.info().height as usize);
                let mut vals = Vec::new();
                for (t, _, px) in &forward {
                    let f = rd.frame_at(*t)?;
                    if f.width as usize == w && f.height as usize == h {
                        vals.push(ssim(px, &f.rgba, w, h));
                    }
                }
                let min = vals.iter().cloned().fold(1.0, f64::min);
                let mean = vals.iter().sum::<f64>() / vals.len().max(1) as f64;
                out["ssim_vs_ref_mean"] = serde_json::json!(mean);
                out["ssim_vs_ref_min"] = serde_json::json!(min);
            }
            println!("{out}");
        }
        "encode" => {
            let b = backend(&a(1)?)?;
            let file = PathBuf::from(a(2)?);
            let out = PathBuf::from(flag("-o").ok_or("encode needs -o")?);
            let kbps: u32 = flag("--kbps").and_then(|x| x.parse().ok()).unwrap_or(8000);
            // the source frames, decoded once by ffmpeg, held in memory: the encoder alone is timed
            let mut src = backend("ffmpeg")?.open_video(&file)?;
            let info = src.info().clone();
            let n: usize = flag("--n")
                .and_then(|x| x.parse().ok())
                .unwrap_or(300)
                .min(info.frames);
            let frames: Vec<Vec<u8>> = (0..n)
                .map(|i| src.frame_at(i as f64 / info.fps).map(|f| f.rgba))
                .collect::<R<_>>()?;
            let opts = EncodeOpts {
                width: info.width,
                height: info.height,
                fps: info.fps.round(),
                kbps,
                nv12: args.iter().any(|x| x == "--nv12"),
                realtime: false,
            };
            // --nv12: the frames converted first (untimed), pushed as NV12 as the GPU path does
            let (w, h) = (info.width as usize, info.height as usize);
            let pushed: Vec<Vec<u8>> = if opts.nv12 { frames.iter().map(|f| raea_media::yuv::rgba_to_nv12(f, w, h)).collect() } else { Vec::new() };
            let t0 = Instant::now();
            let mut e = b.encoder(&out, &opts)?;
            if opts.nv12 {
                for f in &pushed {
                    e.push_nv12(f, w, h)?;
                }
            } else {
                for f in &frames {
                    e.push(f)?;
                }
            }
            let audio = match flag("--audio") {
                Some(f) => Some(
                    backend("soft")
                        .or_else(|_| backend("ffmpeg"))?
                        .decode_audio(Path::new(&f))?,
                ),
                None => None,
            };
            let stats = e.finish(audio.as_ref())?;
            let s = t0.elapsed().as_secs_f64();
            // quality: decode the output with ffmpeg and compare with the uncompressed source frames
            let mut back = backend("ffmpeg")?.open_video(&out)?;
            let mut vals = Vec::new();
            for i in (0..n).step_by((n / 12).max(1)) {
                let f = back.frame_at(i as f64 / info.fps)?;
                vals.push(ssim(
                    &frames[i],
                    &f.rgba,
                    info.width as usize,
                    info.height as usize,
                ));
            }
            let mean = vals.iter().sum::<f64>() / vals.len().max(1) as f64;
            let kbps_actual = stats.bytes as f64 * 8.0 / 1000.0 / (n as f64 / info.fps);
            println!(
                "{}",
                serde_json::json!({"backend": b.name(), "frames": n, "encode_s": s, "fps": n as f64 / s, "bytes": stats.bytes,
                    "kbps": kbps_actual, "ssim_mean": mean, "codec": stats.video_codec})
            );
        }
        "audio" => {
            for f in &args[1..] {
                for name in ["ffmpeg", "soft", "apple", "av1"] {
                    let Ok(b) = backend(name) else { continue };
                    let t0 = Instant::now();
                    match b.decode_audio(Path::new(f)) {
                        Ok(au) => println!(
                            "{}",
                            serde_json::json!({"file": f, "backend": name, "rate": au.rate, "channels": au.channels.len(), "seconds": au.duration(),
                                "lufs": raea_media::audio::lufs(&au), "peak_db": raea_media::audio::peak_db(&au), "decode_s": t0.elapsed().as_secs_f64()})
                        ),
                        Err(e) => println!(
                            "{}",
                            serde_json::json!({"file": f, "backend": name, "error": e})
                        ),
                    }
                }
            }
        }
        "mixcheck" => {
            // the mix kernel against ffmpeg's (atrim/adelay/volume/amix, normalize off) on the same lanes, as f32
            let (voice, music) = (a(1)?, a(2)?);
            // the same decoded samples on both sides (ffmpeg's decoder), so this measures the mix alone; `audio` and
            // `decodecheck` measure the decoders
            let dec = backend(&flag("--decoder").unwrap_or("ffmpeg".into()))?;
            let (v, m) = (
                dec.decode_audio(Path::new(&voice))?,
                dec.decode_audio(Path::new(&music))?,
            );
            use raea_media::audio::{Lane, mix, residual_db};
            let lane = |au: &raea_media::Audio, at: f64, from: f64, len: f64, g: f64| Lane {
                audio: au.clone(),
                at,
                from,
                len,
                gain_db: vec![(0.0, g)],
                fade_in: 0.0,
                fade_out: 0.0,
            };
            let lanes = [
                lane(&v, 0.0, 0.24, 4.2, 0.0),
                lane(&v, 4.2, 4.44, 0.96, 0.0),
                lane(&m, 0.0, 0.0, 6.0, -20.0),
            ];
            let ours = mix(&lanes, 6.0, 1);
            let dir = std::env::temp_dir().join("raea-mixcheck");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let raw = dir.join("ffmix.f32");
            let graph = "[0:a]aresample=48000,atrim=0.24:4.44,asetpts=PTS-STARTPTS[a];[0:a]aresample=48000,atrim=4.44:5.4,asetpts=PTS-STARTPTS,adelay=4200:all=1[b];\
                         [1:a]aresample=48000,atrim=0:6,asetpts=PTS-STARTPTS,volume=-20dB[c];[a][b][c]amix=inputs=3:normalize=0:duration=longest[o]";
            let st = std::process::Command::new("ffmpeg")
                .args([
                    "-y",
                    "-v",
                    "error",
                    "-i",
                    &voice,
                    "-i",
                    &music,
                    "-filter_complex",
                    graph,
                    "-map",
                    "[o]",
                    "-ac",
                    "1",
                    "-f",
                    "f32le",
                    "-ar",
                    "48000",
                ])
                .arg(&raw)
                .status()
                .map_err(|e| e.to_string())?;
            if !st.success() {
                return Err("ffmpeg mix failed".into());
            }
            let b = std::fs::read(&raw).map_err(|e| e.to_string())?;
            let ff: Vec<f32> = b
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            let theirs = raea_media::Audio {
                rate: 48000,
                channels: vec![ff],
            };
            // the sample-exact alignment: search a few samples of offset (decoder priming differs)
            let mut best = (0i64, f64::INFINITY);
            for off in -2048i64..=2048 {
                if off % 1 != 0 {
                    continue;
                }
                let shift = |x: &raea_media::Audio, o: i64| raea_media::Audio {
                    rate: 48000,
                    channels: vec![x.channels[0][o.max(0) as usize..].to_vec()],
                };
                let (p, q) = if off >= 0 {
                    (shift(&ours, off), shift(&theirs, 0))
                } else {
                    (shift(&ours, 0), shift(&theirs, -off))
                };
                // a quick score on the first second, then the whole for the best
                let n = 48000.min(p.len()).min(q.len());
                let e: f64 = (0..n)
                    .map(|i| ((p.channels[0][i] - q.channels[0][i]) as f64).powi(2))
                    .sum();
                if e < best.1 {
                    best = (off, e);
                }
            }
            let off = best.0;
            let (p, q) = if off >= 0 {
                (
                    raea_media::Audio {
                        rate: 48000,
                        channels: vec![ours.channels[0][off as usize..].to_vec()],
                    },
                    theirs.clone(),
                )
            } else {
                (
                    ours.clone(),
                    raea_media::Audio {
                        rate: 48000,
                        channels: vec![theirs.channels[0][(-off) as usize..].to_vec()],
                    },
                )
            };
            // judge away from the cut edges' first/last 2 ms? no: whole mix
            println!(
                "{}",
                serde_json::json!({"offset_samples": off, "residual_db": residual_db(&p, &q), "ours_lufs": raea_media::audio::lufs(&ours), "ffmpeg_lufs": raea_media::audio::lufs(&theirs)})
            );
        }
        "decodecheck" => {
            // each backend's decode of FILE against ffmpeg's: the sample offset (priming) and the residual once aligned
            let f = a(1)?;
            let theirs = backend("ffmpeg")?.decode_audio(Path::new(&f))?;
            for name in ["soft", "apple"] {
                let Ok(b) = backend(name) else { continue };
                let ours = match b.decode_audio(Path::new(&f)) {
                    Ok(x) => x,
                    Err(e) => {
                        println!("{}", serde_json::json!({"backend": name, "error": e}));
                        continue;
                    }
                };
                let ours = raea_media::soft_resample(&ours, theirs.rate);
                let (p, q) = (&ours.channels[0], &theirs.channels[0]);
                let mut best = (0i64, f64::INFINITY);
                for off in -4096i64..=4096 {
                    let n = 48000usize.min(p.len()).min(q.len()) - 4200;
                    let e: f64 = (0..n)
                        .map(|i| {
                            let (x, y) = ((i as i64 + 4100 + off) as usize, i + 4100);
                            ((p[x] - q[y]) as f64).powi(2)
                        })
                        .sum();
                    if e < best.1 {
                        best = (off, e);
                    }
                }
                let off = best.0;
                let n =
                    (p.len() as i64 - off.max(0)).min(q.len() as i64 + off.min(0)) as usize - 8400;
                let (mut e, mut r) = (0f64, 0f64);
                for i in 4200..n {
                    let (x, y) = ((i as i64 + off) as usize, i);
                    e += ((p[x] - q[y]) as f64).powi(2);
                    r += (q[y] as f64).powi(2);
                }
                println!(
                    "{}",
                    serde_json::json!({"file": f, "backend": name, "rate": ours.rate, "samples": p.len(), "ffmpeg_samples": q.len(), "offset_samples": off, "residual_db": 10.0 * (e / r).log10()})
                );
            }
        }
        other => return Err(format!("unknown command {other}")),
    }
    Ok(())
}
