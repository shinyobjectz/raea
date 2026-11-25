//! `media proxy`, `media trim` and `media concat`: files re-encoded.

use super::*;

/// A picture at w x h: scaled, and when its shape differs from the output's, fitted inside it on black (concat of a
/// 480x848 take after a 480x864 one; ffmpeg would join them as they are and leave the player to cope).
pub(crate) fn fit(img: image::RgbaImage, w: u32, h: u32) -> image::RgbaImage {
    if img.width() == w && img.height() == h {
        return img;
    }
    let k = (w as f64 / img.width() as f64).min(h as f64 / img.height() as f64);
    let (fw, fh) = (((img.width() as f64 * k).round() as u32).clamp(1, w), ((img.height() as f64 * k).round() as u32).clamp(1, h));
    // bicubic, as ffmpeg's scale filter
    let small = image::imageops::resize(&img, fw, fh, image::imageops::FilterType::CatmullRom);
    if fw == w && fh == h {
        return small;
    }
    let mut out = image::RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 255]));
    image::imageops::overlay(&mut out, &small, ((w - fw) / 2) as i64, ((h - fh) / 2) as i64);
    out
}

pub(crate) fn even(x: f64) -> u32 {
    ((x / 2.0).round() as u32 * 2).max(2)
}

/// One stretch after another into OUT.mp4 at the first's frame rate and size (or `height`, never upscaled), with sound.
pub fn encode(srcs: &[(PathBuf, Option<f64>, Option<f64>)], height: Option<u32>, out: &Path, kbps: Option<u32>) -> R<Value> {
    // a smaller copy is for looking at (a judge, a phone preview): fewer bits a pixel than a full-size one (VideoToolbox
    // at 0.05 lost 0.03 SSIM to x264 -crf 28 on take04; 0.08 matches it)
    let bits = if height.is_some() { 0.08 } else { 0.1 };
    let be = backend()?;
    let mut decs = Vec::new();
    for (p, ..) in srcs {
        decs.push(be.open_video(p).map_err(|e| format!("{}: {e}", p.display()))?);
    }
    let first = decs[0].info().clone();
    let (sw, sh) = (first.width as f64, first.height as f64);
    let h = height.map(|h| (h as f64).min(sh)).unwrap_or(sh);
    let (w, h) = (even(sw * h / sh), even(h));
    // the output's rate: the first source's average (its track's), at the standard rate it's near
    let fps = standard_fps(video_track(&srcs[0].0).map(|t| t.avg_fps()).unwrap_or(first.fps));
    let kbps = kbps.unwrap_or(((w as f64 * h as f64 * fps * bits) / 1000.0).clamp(300.0, 20000.0) as u32);
    if let Some(d) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let mut enc = be.encoder(out, &EncodeOpts { width: w, height: h, fps, kbps, quality: None, nv12: false, realtime: false })?;
    let mut frames = 0usize;
    let mut lanes: Vec<Audio> = Vec::new();
    let mut any_sound = false;
    let mut src_channels = 0usize;
    for (d, (p, from, to)) in decs.iter_mut().zip(srcs) {
        let info = d.info().clone();
        let ts = times(video_span(p, &info), *from, *to, fps);
        for (_, at) in &ts {
            let img = to_image(d.frame_at(*at)?)?;
            let img = fit(img, w, h);
            enc.push(img.as_raw())?;
        }
        frames += ts.len();
        // the stretch's sound (silence where there is none), stereo at the mix rate
        let secs = ts.len() as f64 / fps;
        let a = soft()?.decode_audio(p).ok().filter(|a| a.len() > 0);
        any_sound |= a.is_some();
        let a = a.map(|a| raea_io_media::soft_resample(&slice(&a, Some(from.unwrap_or(0.0)), None), raea_io_media::audio::RATE));
        let n = (secs * raea_io_media::audio::RATE as f64).round() as usize;
        let mut ch = vec![vec![0f32; n]; 2];
        if let Some(a) = &a {
            src_channels = src_channels.max(a.channels.len().min(2));
        }
        if let Some(a) = a {
            for (c, col) in ch.iter_mut().enumerate() {
                let src = &a.channels[c.min(a.channels.len() - 1)];
                let k = n.min(src.len());
                col[..k].copy_from_slice(&src[..k]);
            }
        }
        lanes.push(Audio { rate: raea_io_media::audio::RATE, channels: ch });
    }
    // as many channels as the sources have (mono stays mono, as ffmpeg keeps it: two copies of one channel would
    // measure 3 dB louder)
    let out_ch = src_channels.clamp(1, 2);
    let sound = Audio { rate: raea_io_media::audio::RATE, channels: (0..out_ch).map(|c| lanes.iter().flat_map(|l| l.channels[c].iter().copied()).collect()).collect() };
    let stats = enc.finish(if any_sound { Some(&sound) } else { None })?;
    Ok(json!({"wrote": out, "frames": frames, "seconds": round3(frames as f64 / fps), "width": w, "height": h, "fps": round3(fps), "kbps": kbps, "bytes": stats.bytes, "sound": any_sound}))
}

/// `media share`: a copy any phone, chat and upload form takes (H.264 and AAC in an MP4, its index first), held to
/// `cap` kbit/s (2600: a 1080p minute in about 20 MB) and never given more than a full-size copy would get (#219).
pub fn share(file: &Path, height: Option<u32>, out: &Path, cap: u32) -> R<Value> {
    std::fs::metadata(file).map_err(|e| format!("{}: {e}", file.display()))?; // the platform's decoder panics on no path
    let i = backend()?.open_video(file).map_err(|e| format!("{}: {e}", file.display()))?.info().clone();
    let k = height.map_or(1.0, |h| (h as f64 / i.height as f64).min(1.0));
    let full = (i.width as f64 * k * i.height as f64 * k * standard_fps(i.fps) * 0.1 / 1000.0).max(300.0) as u32;
    encode(&[(file.to_path_buf(), None, None)], height, out, Some(cap.min(full)))
}

// ---- ffmpeg and ffprobe, read
