//! G5's focus-blur fixtures (docs/the roadmap G5): a take blurred by depth the way a portrait mode does, so previz has
//! good, borderline and bad focus blurs to judge. Each frame's inverse depth (Depth Anything 3) and face (Vision) set a
//! focus plane, at the face or at the background, and each pixel is blurred by its depth's distance from that plane
//! (at most `strength` px, a box blur run twice), then encoded with Apple's encoder.
//!
//!   raea-focus VIDEO OUT.mp4 --focus face|mid|background --strength PX [--seconds 8] [--width 540]
//! (mid: the plane halfway between the face and the background, so both are a little soft: a focus that missed)

use raea_perception::vision::Vision;
use std::path::Path;

/// A box blur of radius r, run twice (close to a Gaussian), on RGBA.
fn blur(src: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
    if r == 0 {
        return src.to_vec();
    }
    let mut a: Vec<f32> = src.iter().map(|v| *v as f32).collect();
    let mut b = vec![0f32; a.len()];
    for _ in 0..2 {
        // rows
        for y in 0..h {
            for c in 0..4 {
                let mut acc = 0f32;
                let mut n = 0f32;
                for x in 0..=r.min(w - 1) {
                    acc += a[(y * w + x) * 4 + c];
                    n += 1.0;
                }
                for x in 0..w {
                    b[(y * w + x) * 4 + c] = acc / n;
                    if x + r + 1 < w {
                        acc += a[(y * w + x + r + 1) * 4 + c];
                        n += 1.0;
                    }
                    if x >= r {
                        acc -= a[(y * w + x - r) * 4 + c];
                        n -= 1.0;
                    }
                }
            }
        }
        // columns
        for x in 0..w {
            for c in 0..4 {
                let mut acc = 0f32;
                let mut n = 0f32;
                for y in 0..=r.min(h - 1) {
                    acc += b[(y * w + x) * 4 + c];
                    n += 1.0;
                }
                for y in 0..h {
                    a[(y * w + x) * 4 + c] = acc / n;
                    if y + r + 1 < h {
                        acc += b[((y + r + 1) * w + x) * 4 + c];
                        n += 1.0;
                    }
                    if y >= r {
                        acc -= b[((y - r) * w + x) * 4 + c];
                        n -= 1.0;
                    }
                }
            }
        }
    }
    a.iter().map(|v| v.round().clamp(0.0, 255.0) as u8).collect()
}

fn scale(f: &raea_media::Frame, nw: usize) -> (Vec<u8>, usize, usize) {
    let (w, h) = (f.width as usize, f.height as usize);
    let nh = (h * nw / w) & !1;
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        for x in 0..nw {
            let (sx, sy) = (x * w / nw, y * h / nh);
            out[(y * nw + x) * 4..(y * nw + x) * 4 + 4].copy_from_slice(&f.rgba[(sy * w + sx) * 4..(sy * w + sx) * 4 + 4]);
        }
    }
    (out, nw, nh)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    if args.len() < 2 {
        eprintln!("raea-focus VIDEO OUT.mp4 --focus face|background --strength PX [--seconds 8] [--width 540]");
        std::process::exit(2);
    }
    let (video, out) = (Path::new(&args[0]), Path::new(&args[1]));
    let mode = flag("--focus").unwrap_or_else(|| "face".into());
    let focus_face = mode != "background";
    let strength: f32 = flag("--strength").and_then(|v| v.parse().ok()).unwrap_or(8.0);
    let seconds: f64 = flag("--seconds").and_then(|v| v.parse().ok()).unwrap_or(8.0);
    let width: usize = flag("--width").and_then(|v| v.parse().ok()).unwrap_or(540);
    let run = || -> Result<(), String> {
        let be = raea_media::backend("apple")?;
        let mut d = be.open_video(video)?;
        let info = d.info().clone();
        let model = raea_perception::depth::model()?;
        let vision = raea_perception::vision::AppleVision::default();
        let n = (seconds.min(info.duration) * info.fps).floor() as usize;
        let mut enc: Option<Box<dyn raea_media::VideoEncoder>> = None;
        // the focus plane, eased across frames so the blur doesn't flicker with the depth model's per-frame scale
        let mut plane: Option<f32> = None;
        for i in 0..n {
            let f = d.frame_at((i as f64 + 0.5) / info.fps)?;
            let (rgba, w, h) = scale(&f, width);
            let (dv, dw, dh) = raea_perception::depth::inverse_depth(&model, &rgba, w, h)?;
            let dmax = dv.iter().cloned().fold(0f32, f32::max).max(1e-6);
            let at = |x: usize, y: usize| dv[(y * dh / h).min(dh - 1) * dw + (x * dw / w).min(dw - 1)] / dmax;
            let people = vision.people(&rgba, w, h, true)?;
            let far = {
                let mut v: Vec<f32> = dv.iter().map(|x| x / dmax).collect();
                v.sort_by(|a, b| a.total_cmp(b));
                v[v.len() / 10]
            };
            let target = if focus_face {
                match people.faces.iter().max_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h))) {
                    Some(fc) => {
                        let mut v: Vec<f32> = Vec::new();
                        for y in (fc.y * h as f64) as usize..((fc.y + fc.h) * h as f64) as usize {
                            for x in (fc.x * w as f64) as usize..((fc.x + fc.w) * w as f64) as usize {
                                v.push(at(x.min(w - 1), y.min(h - 1)));
                            }
                        }
                        v.sort_by(|a, b| a.total_cmp(b));
                        v.get(v.len() / 2).copied().unwrap_or(1.0)
                    }
                    None => plane.unwrap_or(1.0),
                }
            } else {
                // the background: the far tenth of the picture
                far
            };
            let target = if mode == "mid" { (target + far) / 2.0 } else { target };
            let p = match plane {
                Some(p) => p * 0.8 + target * 0.2,
                None => target,
            };
            plane = Some(p);
            // four blur levels, each pixel between the two nearest by its distance from the plane
            let levels: Vec<Vec<u8>> = (0..4).map(|k| blur(&rgba, w, h, (strength * k as f32 / 3.0).round() as usize)).collect();
            let mut outf = vec![0u8; rgba.len()];
            for y in 0..h {
                for x in 0..w {
                    let r = ((at(x, y) - p).abs() * 2.0).min(1.0) * 3.0;
                    let (k, t) = ((r.floor() as usize).min(2), r - r.floor().min(2.0));
                    let i = (y * w + x) * 4;
                    for c in 0..4 {
                        outf[i + c] = (levels[k][i + c] as f32 * (1.0 - t) + levels[k + 1][i + c] as f32 * t).round() as u8;
                    }
                }
            }
            if enc.is_none() {
                enc = Some(be.encoder(out, &raea_media::EncodeOpts { width: w as u32, height: h as u32, fps: info.fps, kbps: 6000, nv12: false, realtime: false })?);
            }
            enc.as_mut().unwrap().push(&outf)?;
        }
        let stats = enc.ok_or("no frames")?.finish(None)?;
        println!("{}", serde_json::json!({"out": out.display().to_string(), "frames": stats.frames, "focus": mode, "strength": strength}));
        Ok(())
    };
    if let Err(e) = run() {
        eprintln!("raea-focus: {e}");
        std::process::exit(1);
    }
}
