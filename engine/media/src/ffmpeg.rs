//! (a) The ffmpeg CLI: the baseline every other backend is measured against. Decoding keeps one rawvideo pipe open
//! while requests move forward and restarts it with an accurate seek otherwise; frame times come from ffprobe.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::{
    Audio, Backend, EncodeOpts, EncodeStats, Frame, Info, R, VideoDecoder, VideoEncoder, as_of,
};

pub struct Ffmpeg;

/// The facts, the presentation times, and whether the colour matrix is untagged.
fn probe(path: &Path) -> R<(Info, Vec<f64>, bool)> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,width,height,r_frame_rate,color_space:stream_side_data=rotation:format=duration",
            "-show_entries",
            "packet=pts_time",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe: {e}"))?;
    let j: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("ffprobe json: {e}"))?;
    let s = &j["streams"][0];
    let (mut w, mut h) = (
        s["width"].as_u64().unwrap_or(0) as u32,
        s["height"].as_u64().unwrap_or(0) as u32,
    );
    let rot = s["side_data_list"]
        .as_array()
        .and_then(|l| l.iter().find_map(|d| d["rotation"].as_i64()))
        .unwrap_or(0);
    if rot.abs() == 90 || rot.abs() == 270 {
        std::mem::swap(&mut w, &mut h);
    }
    let fps = s["r_frame_rate"]
        .as_str()
        .and_then(|r| r.split_once('/'))
        .map(|(n, d)| n.parse::<f64>().unwrap_or(30.0) / d.parse::<f64>().unwrap_or(1.0))
        .unwrap_or(30.0);
    let mut pts: Vec<f64> = j["packets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["pts_time"].as_str().and_then(|t| t.parse().ok()))
        .collect();
    pts.sort_by(f64::total_cmp);
    let duration = j["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse().ok())
        .unwrap_or(0.0);
    let untagged = !matches!(s["color_space"].as_str(), Some(c) if c != "unknown");
    Ok((
        Info {
            codec: s["codec_name"].as_str().unwrap_or("?").into(),
            width: w,
            height: h,
            fps,
            duration,
            frames: pts.len(),
        },
        pts,
        untagged,
    ))
}

struct Decoder {
    path: PathBuf,
    info: Info,
    pts: Vec<f64>,
    pipe: Option<(Child, usize)>, // the child, and the index of the next frame it will give
    /// untagged HD (720 lines or more) is read as BT.709, as Apple, browsers and our own decoders read it (swscale
    /// would assume BT.601)
    bt709: bool,
}

impl Decoder {
    fn start(&mut self, at: usize) -> R<()> {
        if let Some((mut c, _)) = self.pipe.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        let start = self.pts[at] - self.pts[0];
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-v", "error", "-ss", &format!("{start:.6}"), "-i"]).arg(&self.path);
        if self.bt709 {
            cmd.args(["-vf", "scale=in_color_matrix=bt709:in_range=tv"]);
        }
        let child = cmd
            .args([
                "-an",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "-vsync",
                "passthrough",
                "-",
            ])
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("ffmpeg: {e}"))?;
        self.pipe = Some((child, at));
        Ok(())
    }
}

impl VideoDecoder for Decoder {
    fn info(&self) -> &Info {
        &self.info
    }
    fn index_at(&mut self, t: f64) -> Option<usize> {
        (!self.pts.is_empty()).then(|| crate::as_of(&self.pts, t + self.pts[0]))
    }
    fn frame_at(&mut self, t: f64) -> R<Frame> {
        let want = as_of(&self.pts, t + self.pts[0]);
        match &self.pipe {
            Some((_, next)) if *next <= want && want - *next < 60 => {}
            _ => self.start(want)?,
        }
        let size = (self.info.width * self.info.height * 4) as usize;
        let mut buf = vec![0u8; size];
        loop {
            let (child, next) = self.pipe.as_mut().unwrap();
            child
                .stdout
                .as_mut()
                .unwrap()
                .read_exact(&mut buf)
                .map_err(|e| format!("ffmpeg read: {e}"))?;
            *next += 1;
            if *next - 1 == want {
                return Ok(Frame {
                    pts: self.pts[want] - self.pts[0],
                    width: self.info.width,
                    height: self.info.height,
                    rgba: buf,
                });
            }
        }
    }
}

struct Encoder {
    child: Child,
    video: PathBuf,
    out: PathBuf,
    frames: usize,
}

impl VideoEncoder for Encoder {
    fn push(&mut self, rgba: &[u8]) -> R<()> {
        self.child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(rgba)
            .map_err(|e| format!("ffmpeg stdin: {e}"))?;
        self.frames += 1;
        Ok(())
    }
    fn finish(mut self: Box<Self>, audio: Option<&Audio>) -> R<EncodeStats> {
        drop(self.child.stdin.take());
        let st = self.child.wait().map_err(|e| e.to_string())?;
        if !st.success() {
            return Err(format!("ffmpeg encode exited {st}"));
        }
        let mut audio_codec = String::new();
        if let Some(a) = audio {
            // mux: the video stream as is, the audio columns as AAC
            let raw = self.video.with_extension("f32");
            let mut bytes = Vec::with_capacity(a.len() * a.channels.len() * 4);
            for i in 0..a.len() {
                for ch in &a.channels {
                    bytes.extend_from_slice(&ch[i].to_le_bytes());
                }
            }
            std::fs::write(&raw, bytes).map_err(|e| e.to_string())?;
            let st = Command::new("ffmpeg")
                .args(["-v", "error", "-y", "-i"])
                .arg(&self.video)
                .args([
                    "-f",
                    "f32le",
                    "-ar",
                    &a.rate.to_string(),
                    "-ac",
                    &a.channels.len().to_string(),
                    "-i",
                ])
                .arg(&raw)
                .args([
                    "-map",
                    "0:v",
                    "-map",
                    "1:a",
                    "-c:v",
                    "copy",
                    "-c:a",
                    "aac",
                    "-b:a",
                    "128k",
                    "-movflags",
                    "+faststart",
                    "-shortest",
                ])
                .arg(&self.out)
                .status()
                .map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(&raw);
            if !st.success() {
                return Err("ffmpeg mux failed".into());
            }
            audio_codec = "aac".into();
        } else {
            std::fs::rename(&self.video, &self.out).map_err(|e| e.to_string())?;
        }
        let _ = std::fs::remove_file(&self.video);
        Ok(EncodeStats {
            frames: self.frames,
            bytes: std::fs::metadata(&self.out).map(|m| m.len()).unwrap_or(0),
            video_codec: "h264 (libx264)".into(),
            audio_codec,
        })
    }
}

impl Backend for Ffmpeg {
    fn name(&self) -> &'static str {
        "ffmpeg"
    }
    fn open_video(&self, path: &Path) -> R<Box<dyn VideoDecoder>> {
        let (info, pts, untagged) = probe(path)?;
        let bt709 = untagged && info.width.min(info.height) >= 720;
        if pts.is_empty() {
            return Err(format!("{}: no video frames", path.display()));
        }
        Ok(Box::new(Decoder {
            path: path.to_path_buf(),
            info,
            pts,
            pipe: None,
            bt709,
        }))
    }
    fn decode_audio(&self, path: &Path) -> R<Audio> {
        let ch = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=channels",
                "-of",
                "csv=p=0",
            ])
            .arg(path)
            .output()
            .map_err(|e| e.to_string())?;
        let channels: usize = String::from_utf8_lossy(&ch.stdout)
            .trim()
            .parse()
            .unwrap_or(1);
        let out = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-vn", "-f", "f32le", "-ar", "48000", "-"])
            .output()
            .map_err(|e| format!("ffmpeg: {e}"))?;
        let samples: Vec<f32> = out
            .stdout
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        let mut cols = vec![Vec::with_capacity(samples.len() / channels.max(1)); channels.max(1)];
        for (i, s) in samples.into_iter().enumerate() {
            cols[i % channels.max(1)].push(s);
        }
        Ok(Audio {
            rate: 48000,
            channels: cols,
        })
    }
    fn encoder(&self, out: &Path, o: &EncodeOpts) -> R<Box<dyn VideoEncoder>> {
        let video = out.with_extension("video.mp4");
        let child = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "-s",
                &format!("{}x{}", o.width, o.height),
                "-framerate",
                &o.fps.to_string(),
                "-i",
                "-",
            ])
            .args([
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-b:v",
                &format!("{}k", o.kbps),
                "-maxrate",
                &format!("{}k", o.kbps * 3 / 2),
                "-bufsize",
                &format!("{}k", o.kbps * 2),
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ])
            .arg(&video)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| format!("ffmpeg: {e}"))?;
        Ok(Box::new(Encoder {
            child,
            video,
            out: out.to_path_buf(),
            frames: 0,
        }))
    }
}
