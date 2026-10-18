//! Media: which `<video>`/`<audio>` clips a composition plays and when (the probe's media plan),
//! video frames for the renderer (caller-extracted or ffmpeg-extracted, cached), and the ffmpeg
//! audio mix used when raea encodes itself.

use crate::compile::{all_elements, attr, has_attr, tag};
use crate::runtime::Resolver;
use blitz_dom::BaseDocument;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(feature = "ffmpeg")]
use std::sync::Mutex;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoClip {
    pub id: String,
    pub src: String,
    pub start: f64,
    pub end: f64,
    pub media_start: f64,
    #[serde(rename = "loop")]
    pub looping: bool,
    pub playback_rate: f64,
    pub has_audio: bool,
    /// audio of a data-has-audio video: data-volume / data-fade-in / data-fade-out (in the audio plan)
    #[serde(skip)]
    pub volume: f64,
    #[serde(skip)]
    pub fade_in: f64,
    #[serde(skip)]
    pub fade_out: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioClip {
    pub id: String,
    pub src: String,
    pub start: f64,
    pub end: f64,
    pub media_start: f64,
    pub volume: f64,
    #[serde(rename = "loop")]
    pub looping: bool,
    pub playback_rate: f64,
    pub fade_in: f64,
    pub fade_out: f64,
}

/// A media source as a file: a path as it is, an http(s) URL fetched once into `cache_root` (shared by every render
/// and by hosts that read the page's media themselves, such as rdb's GPU path).
pub fn fetch_source(net: &crate::net::SyncNet, cache_root: &Path, src: &str) -> Result<PathBuf, String> {
    if !(src.starts_with("http://") || src.starts_with("https://")) {
        return Ok(PathBuf::from(src));
    }
    // the URL's extension kept: AVFoundation (the GPU path's decoder) picks the container by it
    let ext = url::Url::parse(src).ok().and_then(|u| Path::new(u.path()).extension().map(|e| e.to_string_lossy().to_lowercase())).filter(|e| e.len() <= 5);
    let p = cache_root.join(match ext {
        Some(e) => format!("src-{:016x}.{e}", hash(src)),
        None => format!("src-{:016x}", hash(src)),
    });
    if !p.exists() {
        let url = url::Url::parse(src).map_err(|e| e.to_string())?;
        let bytes = net.load(&url)?;
        std::fs::create_dir_all(cache_root).map_err(|e| e.to_string())?;
        std::fs::write(&p, bytes).map_err(|e| e.to_string())?;
    }
    Ok(p)
}

/// The media cache under a project (where `fetch_source` keeps fetched sources).
pub fn media_cache(net: &crate::net::SyncNet) -> PathBuf {
    net.cache_dir.clone().unwrap_or_else(std::env::temp_dir).join("video")
}

fn num(doc: &BaseDocument, id: blitz_dom::NodeId, name: &str) -> Option<f64> {
    attr(doc, id, name).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
}

/// Absolute path (or URL) for a media src relative to the project.
pub fn resolve_src(project_dir: &Path, src: &str) -> String {
    if src.contains("://") || src.starts_with("data:") {
        if let Some(p) = src.strip_prefix("file://") {
            return p.to_string();
        }
        return src.to_string();
    }
    let p = src.split(['?', '#']).next().unwrap_or(src);
    let p = p.strip_prefix('/').map(|s| project_dir.join(s)).unwrap_or_else(|| project_dir.join(p));
    p.to_string_lossy().to_string()
}

pub fn discover(doc: &BaseDocument, resolver: &mut Resolver<'_>, project_dir: &Path, duration: f64) -> (Vec<VideoClip>, Vec<AudioClip>) {
    let mut videos = Vec::new();
    let mut audio = Vec::new();
    let clamp_end = |start: f64, end: Option<f64>| -> f64 {
        match end {
            Some(e) if e > start => e,
            _ => duration.max(start),
        }
    };
    let window_end = |doc: &BaseDocument, id, start: f64, resolver: &mut Resolver<'_>| -> Option<f64> {
        if let Some(e) = num(doc, id, "data-end") {
            return Some(e);
        }
        let _ = resolver;
        num(doc, id, "data-duration").filter(|d| *d > 0.0).map(|d| start + d)
    };
    for id in all_elements(doc) {
        let t = tag(doc, id);
        if let Some(vid) = attr(doc, id, "data-uv-video") {
            let Some(src) = attr(doc, id, "data-uv-src") else { continue };
            let start = resolver.start(id, 0.0);
            let end = clamp_end(start, window_end(doc, id, start, resolver));
            videos.push(VideoClip {
                id: vid,
                src: resolve_src(project_dir, &src),
                start,
                end,
                media_start: num(doc, id, "data-playback-start").or(num(doc, id, "data-media-start")).unwrap_or(0.0).max(0.0),
                looping: has_attr(doc, id, "data-uv-loop"),
                playback_rate: num(doc, id, "data-playback-rate").filter(|r| *r > 0.0).unwrap_or(1.0).clamp(0.1, 10.0),
                has_audio: attr(doc, id, "data-has-audio").as_deref() == Some("true"),
                volume: num(doc, id, "data-volume").unwrap_or(1.0).max(0.0),
                fade_in: num(doc, id, "data-fade-in").unwrap_or(0.0).max(0.0),
                fade_out: num(doc, id, "data-fade-out").unwrap_or(0.0).max(0.0),
            });
        } else if t == "audio" {
            let src = attr(doc, id, "src").or_else(|| {
                doc.get_node(id)?.children.iter().find(|c| tag(doc, **c) == "source").and_then(|c| attr(doc, *c, "src"))
            });
            let Some(src) = src else { continue };
            if has_attr(doc, id, "data-hidden") {
                continue;
            }
            let aid = attr(doc, id, "data-hf-render-id").or_else(|| attr(doc, id, "id")).unwrap_or_else(|| format!("hf-audio-{}", audio.len()));
            let start = if has_attr(doc, id, "data-start") { resolver.start(id, 0.0) } else { 0.0 };
            let end = clamp_end(start, window_end(doc, id, start, resolver));
            audio.push(AudioClip {
                id: aid,
                src: resolve_src(project_dir, &src),
                start,
                end,
                media_start: num(doc, id, "data-playback-start").or(num(doc, id, "data-media-start")).unwrap_or(0.0).max(0.0),
                volume: num(doc, id, "data-volume").unwrap_or(1.0).max(0.0),
                looping: has_attr(doc, id, "loop"),
                playback_rate: num(doc, id, "data-playback-rate").filter(|r| *r > 0.0).unwrap_or(1.0).clamp(0.1, 10.0),
                fade_in: num(doc, id, "data-fade-in").unwrap_or(0.0).max(0.0),
                fade_out: num(doc, id, "data-fade-out").unwrap_or(0.0).max(0.0),
            });
        }
    }
    // Audio carried by videos marked data-has-audio="true"
    for v in &videos {
        if v.has_audio {
            audio.push(AudioClip {
                id: format!("{}-audio", v.id),
                src: v.src.clone(),
                start: v.start,
                end: v.end,
                media_start: v.media_start,
                volume: v.volume,
                looping: v.looping,
                playback_rate: v.playback_rate,
                fade_in: v.fade_in,
                fade_out: v.fade_out,
            });
        }
    }
    // The composition format's clampFadesToDuration: fades longer than the clip shrink proportionally
    for a in &mut audio {
        let total = a.fade_in + a.fade_out;
        let duration = a.end - a.start;
        if duration.is_finite() && duration > 0.0 && total > duration {
            let scale = duration / total;
            a.fade_in *= scale;
            a.fade_out *= scale;
        }
    }
    (videos, audio)
}

// ---------------------------------------------------------------------------------------------
// Video frames

#[derive(Clone, Debug)]
pub struct VideoEntry {
    pub dir: PathBuf,
    pub start: f64,
    pub end: f64,
    pub count: usize,
    pub ext: String,
    /// frames come from the host (C API) instead of `dir`
    pub provider: Option<FrameProvider>,
}

/// Host-supplied decoded frames (the C API's frame provider): `(video id, clip-local frame index)`
/// -> `(width, height, RGBA8 straight alpha, rows packed)`; `None` renders the video empty.
/// Called from render worker threads, possibly concurrently.
#[derive(Clone)]
pub struct FrameProvider(pub Arc<dyn Fn(&str, usize) -> Option<(u32, u32, Vec<u8>)> + Send + Sync>);

impl std::fmt::Debug for FrameProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FrameProvider")
    }
}

/// Frames by exact clip-local time (seconds): the built-in decoder, so a snapshot between output
/// frames shows the source frame on screen at that instant, as the composition format's snapshot does.
#[derive(Clone)]
pub struct TimedProvider(pub Arc<dyn Fn(&str, f64) -> Option<(u32, u32, Vec<u8>)> + Send + Sync>);

impl std::fmt::Debug for TimedProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TimedProvider")
    }
}

#[derive(Clone, Debug, Default)]
pub struct VideoFrames {
    pub fps: f64,
    pub entries: HashMap<String, VideoEntry>,
    pub timed: Option<TimedProvider>,
}

impl VideoFrames {
    pub fn windows(&self) -> HashMap<String, (f64, f64)> {
        self.entries.iter().map(|(k, e)| (k.clone(), (e.start, e.end))).collect()
    }

    pub fn has(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    /// Clip-local frame index for render time `t` (0 = the clip's first output frame).
    pub fn frame_index(&self, id: &str, t: f64) -> Option<usize> {
        let e = self.entries.get(id)?;
        if e.count == 0 || t < e.start - 1e-9 {
            return None;
        }
        let k = (((t - e.start) * self.fps) + 1e-6).floor().max(0.0) as usize;
        Some(k.min(e.count - 1))
    }

    /// What is on screen at render time `t`: (frame index, exact clip-local time when frames come by
    /// time). Equal keys show the same pixels.
    pub fn frame_key(&self, id: &str, t: f64) -> Option<(usize, u64)> {
        let k = self.frame_index(id, t)?;
        let local = self.timed.as_ref().map(|_| self.local_time(id, t, k).to_bits()).unwrap_or(0);
        Some((k, local))
    }

    fn local_time(&self, id: &str, t: f64, k: usize) -> f64 {
        let Some(e) = self.entries.get(id) else { return k as f64 / self.fps };
        // within frame k's slot (a time past the clip's end holds its last frame)
        (t - e.start).clamp(k as f64 / self.fps, (k as f64 + 1.0) / self.fps - 1e-6)
    }

    /// Pixels on screen at render time `t` (frame `k`): by exact time from the built-in decoder, else
    /// frame `k` from the host's provider or the frame file.
    pub fn frame_rgba_at(&self, id: &str, t: f64, k: usize) -> Option<(u32, u32, Vec<u8>)> {
        match &self.timed {
            Some(p) => (p.0)(id, self.local_time(id, t, k)),
            None => self.frame_rgba(id, k),
        }
    }

    /// Decoded RGBA8 of frame `k`: from the host's provider, else the frame file.
    pub fn frame_rgba(&self, id: &str, k: usize) -> Option<(u32, u32, Vec<u8>)> {
        let e = self.entries.get(id)?;
        match &e.provider {
            Some(p) => (p.0)(id, k),
            None => decode_rgba(&e.dir.join(format!("frame_{k:06}.{}", e.ext))).ok(),
        }
    }

    /// Every clip's frames come from the host: `provider(id, k)` for clip-local frame k.
    pub fn from_provider(fps: f64, clips: &[VideoClip], provider: FrameProvider) -> VideoFrames {
        let mut entries = HashMap::new();
        for c in clips {
            let raw = (c.end - c.start).max(0.0) * fps;
            let count = if (raw - raw.round()).abs() <= 1e-3 { raw.round() } else { raw.ceil() }.max(1.0) as usize;
            // the window on the frame grid, as the runtime shows the clip (runtime::snap_to_frame): a start a
            // hair after a frame (2.8667 s at 30 fps) showed the clip on that frame with no picture, a black
            // frame at each such cut (2026-10-14)
            let (start, end) = (crate::runtime::snap_to_frame(c.start, fps), crate::runtime::snap_to_frame(c.end, fps));
            let entry = VideoEntry { dir: PathBuf::new(), start, end, count, ext: String::new(), provider: Some(provider.clone()) };
            entries.insert(c.id.clone(), entry);
        }
        VideoFrames { fps, entries, timed: None }
    }

    /// Caller-provided frames: DIR/<videoId>/frame_%06d.(jpg|png), 0 = the clip's first frame.
    pub fn from_dir(dir: &Path, fps: f64, clips: &[VideoClip]) -> VideoFrames {
        let mut entries = HashMap::new();
        for c in clips {
            let d = dir.join(&c.id);
            if let Some((count, ext)) = count_frames(&d) {
                let (start, end) = (crate::runtime::snap_to_frame(c.start, fps), crate::runtime::snap_to_frame(c.end, fps));
                entries.insert(c.id.clone(), VideoEntry { dir: d, start, end, count, ext, provider: None });
            }
        }
        VideoFrames { fps, entries, timed: None }
    }

    /// Extract every clip with ffmpeg into the cache (reused across renders).
    #[cfg(feature = "ffmpeg")]
    /// `<video>` frames decoded on demand: one streaming ffmpeg decoder per source file, shared
    /// by every clip cut from it and by all render workers, with a small in-memory ring of recent
    /// frames. Nothing is written to disk, only frames the render asks for are decoded (so
    /// --start/--end bound the work), and the decoder seeks only when the timeline jumps.
    pub fn extract(cache_root: &Path, fps: f64, clips: &[VideoClip], net: &crate::net::SyncNet, log: &dyn Fn(&str)) -> Result<VideoFrames, String> {
        let ffmpeg = ffmpeg_path().ok_or("ffmpeg not found on PATH (needed to decode <video>; pass --video-frames instead)")?;
        cleanup_legacy_frames(cache_root);
        let mut sources: HashMap<PathBuf, Arc<Mutex<decode::Pool>>> = HashMap::new();
        let mut clip_src: HashMap<String, (Arc<Mutex<decode::Pool>>, f64, f64, bool)> = HashMap::new();
        for c in clips {
            let src_path = fetch_source(net, cache_root, &c.src)?;
            if let Err(e) = std::fs::metadata(&src_path) {
                // like a browser: a missing source renders as an empty video
                log(&format!("warning: video {}: {}: {e}; it renders empty", c.id, src_path.display()));
                continue;
            }
            let dec = match sources.get(&src_path) {
                Some(d) => d.clone(),
                None => {
                    let d = Arc::new(Mutex::new(decode::Pool::open(&ffmpeg, &src_path, fps).map_err(|e| format!("video {}: {e}", c.id))?));
                    sources.insert(src_path.clone(), d.clone());
                    d
                }
            };
            clip_src.insert(c.id.clone(), (dec, c.media_start, c.playback_rate, c.looping));
        }
        log(&format!("decoding {} video clip(s) from {} source file(s) on demand", clip_src.len(), sources.len()));
        let timed = TimedProvider(Arc::new(move |id: &str, local: f64| {
            let (dec, media_start, rate, looping) = clip_src.get(id)?;
            let mut m = media_start + local * rate;
            let mut d = dec.lock().ok()?;
            if *looping {
                if let Some(dur) = d.duration().filter(|d| *d > 0.0) {
                    m %= dur;
                }
            }
            // a snapshot between output frames seeks to that instant; render frames stay on the grid
            let slot = local * fps;
            d.frame(m, (slot - slot.round()).abs() > 1e-3)
        }));
        let by_time = timed.clone();
        let provider = FrameProvider(Arc::new(move |id: &str, k: usize| (by_time.0)(id, k as f64 / fps)));
        let mut frames = VideoFrames::from_provider(fps, clips, provider);
        frames.timed = Some(timed);
        Ok(frames)
    }

    /// Built without the `ffmpeg` feature (iOS): the host supplies frames.
    #[cfg(not(feature = "ffmpeg"))]
    pub fn extract(_cache_root: &Path, _fps: f64, _clips: &[VideoClip], _net: &crate::net::SyncNet, _log: &dyn Fn(&str)) -> Result<VideoFrames, String> {
        Err("this build cannot decode <video> (no ffmpeg); supply frames with videoFrames / --video-frames or a frame provider".into())
    }
}

fn count_frames(dir: &Path) -> Option<(usize, String)> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut count = 0;
    let mut ext = String::from("jpg");
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with("frame_") {
            count += 1;
            if let Some(x) = name.rsplit('.').next() {
                ext = x.to_string();
            }
        }
    }
    (count > 0).then_some((count, ext))
}

pub fn hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub fn decode_rgba(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let rgba = img.to_rgba8();
    Ok((rgba.width(), rgba.height(), rgba.into_raw()))
}

/// Built without the `ffmpeg` feature (iOS): there is no ffmpeg to run.
#[cfg(not(feature = "ffmpeg"))]
pub fn ffmpeg_path() -> Option<PathBuf> {
    None
}

#[cfg(feature = "ffmpeg")]
pub fn ffmpeg_path() -> Option<PathBuf> {
    if let Some(p) = crate::env_os("FFMPEG").or_else(|| std::env::var_os("UGC_FFMPEG")) {
        return Some(PathBuf::from(p));
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path).chain([PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin")]) {
        let p = dir.join("ffmpeg");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// ffmpeg inputs + filter graph mixing the audio plan into `[aout]`, inputs numbered from `first_input`.
/// Whether `src`'s first audio stream is mono, from ffmpeg's stream summary ("Audio: ..., mono, ...").
fn audio_is_mono(src: &str) -> bool {
    let Some(ffmpeg) = ffmpeg_path() else { return false };
    let Ok(out) = std::process::Command::new(ffmpeg).args(["-hide_banner", "-i", src]).output() else { return false };
    let text = String::from_utf8_lossy(&out.stderr);
    text.lines().find(|l| l.contains("Audio:")).is_some_and(|l| l.split(',').any(|t| t.trim() == "mono"))
}

pub fn audio_filter(audio: &[AudioClip], first_input: usize, duration: f64) -> (Vec<String>, String) {
    let mut args = Vec::new();
    let mut chains = Vec::new();
    let mut labels = Vec::new();
    // a clip starting at or after the end contributes nothing
    for (i, a) in audio.iter().filter(|a| a.start < duration).enumerate() {
        let input = first_input + i;
        if a.looping {
            args.extend(["-stream_loop".into(), "-1".into()]);
        }
        args.extend(["-i".into(), a.src.clone()]);
        let len = (a.end.min(duration) - a.start).max(0.0);
        let mut f = format!(
            "[{input}:a]atrim=start={}:duration={},asetpts=PTS-STARTPTS",
            a.media_start,
            len * a.playback_rate
        );
        // The composition format's mono rule (engine audioMixer.ts STEREO_CHANNEL_FILTER): the voice goes into both
        // channels at full level; ffmpeg's default mono->stereo rematrix is 3 dB down
        if audio_is_mono(&a.src) {
            f.push_str(",pan=stereo|FL=FL+FC|FR=FR+FC");
        }
        if (a.playback_rate - 1.0).abs() > 1e-6 {
            f.push_str(&format!(",atempo={}", a.playback_rate.clamp(0.5, 2.0)));
        }
        if (a.volume - 1.0).abs() > 1e-6 {
            f.push_str(&format!(",volume={}", a.volume));
        }
        if a.fade_in > 0.0 {
            f.push_str(&format!(",afade=t=in:st=0:d={}", a.fade_in));
        }
        if a.fade_out > 0.0 {
            f.push_str(&format!(",afade=t=out:st={}:d={}", (len - a.fade_out).max(0.0), a.fade_out));
        }
        // one rate/layout for every input (open-ended apad + mixed rates corrupted the AAC track's
        // timestamps when a 44.1 kHz bed met a 48 kHz voice), padded to the rest of the render and only
        // then delayed into place, so each stream is exactly `duration` long. Delaying first and
        // trimming the padded stream to `duration` moved every clip back to 0 (the trim counts from the
        // delayed timestamps), so multi-clip edits lost their voice after the first clip.
        let delay = (a.start * 1000.0).round() as i64;
        let pad = (duration - delay as f64 / 1000.0).max(0.0);
        f.push_str(&format!(
            ",aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,apad=whole_dur={pad},adelay={delay}:all=1[a{i}]"
        ));
        chains.push(f);
        labels.push(format!("[a{i}]"));
    }
    // The mix is brought to -14 LUFS (what TikTok, Reels and Shorts play at; true peak under -1.5 dB): a voice with a
    // ducked bed came out near -19 and played quiet next to other ads (eval video-editing-make-music, 2026-10-12).
    // loudnorm works at 192 kHz inside, so it's resampled back to 48 kHz.
    let graph = format!(
        "{};{}amix=inputs={}:normalize=0:duration=longest,loudnorm=I=-14:TP=-1.5:LRA=11,aresample=48000,asetpts=N/SR/TB[aout]",
        chains.join(";"),
        labels.join(""),
        labels.len()
    );
    (args, graph)
}

/// Remove frame directories left by the old extract-to-disk decoder (untouched for an hour, so a
/// render still running on an older binary keeps its frames).
fn cleanup_legacy_frames(cache_root: &Path) {
    let Ok(rd) = std::fs::read_dir(cache_root) else { return };
    for e in rd.flatten() {
        if !e.file_name().to_string_lossy().starts_with("frames-") {
            continue;
        }
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|m| m.elapsed().ok()).is_some_and(|age| age.as_secs() > 3600);
        if old {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

#[cfg(feature = "ffmpeg")]
mod decode {
    use std::collections::VecDeque;
    use std::io::Read;
    use std::path::{Path, PathBuf};
    use std::process::{Child, ChildStdout, Command, Stdio};

    /// Longest side frames are decoded at (4K sources are scaled down; outputs are at most 4K).
    const MAX_SIDE: u32 = 2160;
    /// Frames kept behind the newest one, for render workers that run a few frames apart.
    const RING: usize = 48;
    /// A request further ahead than this (seconds) restarts the decoder there instead of decoding forward.
    const JUMP: f64 = 2.0;

    /// A few decoders on one source: render workers a few frames apart straddle cuts, and one
    /// decoder would seek back and forth between the two sides.
    pub struct Pool {
        decoders: Vec<Decoder>,
        clock: u64,
        used: Vec<u64>,
    }

    impl Pool {
        pub fn open(ffmpeg: &Path, src: &Path, fps: f64) -> Result<Pool, String> {
            Ok(Pool { decoders: vec![Decoder::open(ffmpeg, src, fps)?], clock: 0, used: vec![0] })
        }

        pub fn duration(&self) -> Option<f64> {
            self.decoders[0].duration()
        }

        /// `exact`: show the source frame on screen at `m` itself rather than at the decoder's sample
        /// at or before it (a seek unless `m` is on that decoder's sample grid).
        pub fn frame(&mut self, m: f64, exact: bool) -> Option<(u32, u32, Vec<u8>)> {
            self.clock += 1;
            let pick = match self.decoders.iter().position(|d| d.can_serve(m)) {
                Some(i) => i,
                None if self.decoders.len() < 3 => {
                    let d = self.decoders[0].sibling();
                    self.decoders.push(d);
                    self.used.push(0);
                    self.decoders.len() - 1
                }
                // least recently used restarts at m
                None => (0..self.decoders.len()).min_by_key(|&i| self.used[i]).unwrap_or(0),
            };
            self.used[pick] = self.clock;
            self.decoders[pick].frame(m, exact)
        }
    }

    pub struct Decoder {
        ffmpeg: PathBuf,
        src: PathBuf,
        fps: f64,
        width: u32,
        height: u32,
        duration: Option<f64>,
        child: Option<(Child, ChildStdout)>,
        /// media time of decoder frame 0
        origin: f64,
        /// index of the next frame the decoder will produce
        next: i64,
        ring: VecDeque<(i64, Vec<u8>)>,
        failed: bool,
    }

    impl Decoder {
        pub fn open(ffmpeg: &Path, src: &Path, fps: f64) -> Result<Decoder, String> {
            let (w, h, duration) = probe(ffmpeg, src)?;
            let scale = (MAX_SIDE as f64 / w.max(h) as f64).min(1.0);
            let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
            Ok(Decoder {
                ffmpeg: ffmpeg.to_path_buf(),
                src: src.to_path_buf(),
                fps,
                width: even(w as f64 * scale),
                height: even(h as f64 * scale),
                duration,
                child: None,
                origin: 0.0,
                next: 0,
                ring: VecDeque::new(),
                failed: false,
            })
        }

        pub fn duration(&self) -> Option<f64> {
            self.duration
        }

        fn sibling(&self) -> Decoder {
            Decoder {
                ffmpeg: self.ffmpeg.clone(),
                src: self.src.clone(),
                fps: self.fps,
                width: self.width,
                height: self.height,
                duration: self.duration,
                child: None,
                origin: 0.0,
                next: 0,
                ring: VecDeque::new(),
                failed: false,
            }
        }

        fn index(&self, m: f64) -> i64 {
            ((m - self.origin) * self.fps + 1e-6).floor() as i64
        }

        /// Whether `m` is in the ring or a short decode ahead (no seek needed).
        fn can_serve(&self, m: f64) -> bool {
            if self.child.is_none() && self.ring.is_empty() {
                return false;
            }
            let idx = self.index(m);
            let lo = self.ring.front().map(|(i, _)| *i).unwrap_or(self.next);
            idx >= lo && (idx < self.next || (self.child.is_some() && ((idx - self.next) as f64) <= JUMP * self.fps))
        }

        fn spawn(&mut self, at: f64) {
            self.stop();
            // The composition format's sampling (engine videoFrameExtractor; snapshot picks the same "containing" frame):
            // output frame k shows the last source frame starting at or before at + k/fps, and the inexact
            // seek keeps the frame already on screen at `at`. Untagged sources convert with swscale's
            // default matrix like the composition format's snapshot frames (its render path guesses BT.709 for HD).
            let filters = format!(
                "settb=intb/2,setpts=PTS-lte(TB\\,0.001),fps={}:start_time=0:round=up,scale={}:{}:flags=bicubic,format=rgba",
                self.fps, self.width, self.height
            );
            let child = Command::new(&self.ffmpeg)
                .args(["-v", "error", "-nostdin", "-noaccurate_seek", "-ss", &format!("{:.6}", at.max(0.0))])
                .arg("-i")
                .arg(&self.src)
                .args([
                    "-an",
                    "-sn",
                    "-vf",
                    &filters,
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgba",
                    "-",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            match child {
                Ok(mut c) => {
                    let out = c.stdout.take();
                    match out {
                        Some(o) => self.child = Some((c, o)),
                        None => self.failed = true,
                    }
                }
                Err(_) => self.failed = true,
            }
            self.origin = at.max(0.0);
            self.next = 0;
            self.ring.clear();
        }

        fn stop(&mut self) {
            if let Some((mut c, _)) = self.child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
        }

        /// The frame at media time `m` (seconds), straight RGBA8.
        pub fn frame(&mut self, m: f64, exact: bool) -> Option<(u32, u32, Vec<u8>)> {
            if self.failed {
                return None;
            }
            let m = match self.duration {
                Some(d) if d > 0.0 => m.min((d - 0.5 / self.fps).max(0.0)),
                _ => m,
            };
            let pos = (m - self.origin) * self.fps;
            let mut idx = (pos + 1e-6).floor() as i64;
            let behind = self.ring.front().map(|(i, _)| idx < *i).unwrap_or(idx < self.next);
            let off_grid = exact && (pos - pos.round()).abs() > 1e-3;
            // An off-grid request starts the decoder at its exact time, which shifts the ring's frame
            // numbering off the grid; an on-grid request after it must restart on the grid, or it reads
            // the off-grid picture (seek determinism: a frame can't depend on the one rendered before it).
            let origin_frames = self.origin * self.fps;
            let misaligned = !off_grid && (origin_frames - origin_frames.round()).abs() > 1e-3;
            if self.child.is_none() || idx < 0 || behind || off_grid || misaligned || (idx - self.next) as f64 > JUMP * self.fps {
                self.spawn(m);
                idx = 0;
            }
            if let Some((_, f)) = self.ring.iter().find(|(i, _)| *i == idx) {
                return Some((self.width, self.height, f.clone()));
            }
            let size = (self.width * self.height * 4) as usize;
            while self.next <= idx {
                let mut buf = vec![0u8; size];
                let ok = match self.child.as_mut() {
                    Some((_, out)) => out.read_exact(&mut buf).is_ok(),
                    None => false,
                };
                if !ok {
                    // past the end of the file: hold the last frame
                    self.stop();
                    return self.ring.back().map(|(_, f)| (self.width, self.height, f.clone()));
                }
                self.ring.push_back((self.next, buf));
                self.next += 1;
                while self.ring.len() > RING {
                    self.ring.pop_front();
                }
            }
            self.ring.iter().find(|(i, _)| *i == idx).map(|(_, f)| (self.width, self.height, f.clone()))
        }
    }

    impl Drop for Decoder {
        fn drop(&mut self) {
            self.stop();
        }
    }

    /// (width, height) after rotation, and duration, from ffmpeg's stream summary.
    fn probe(ffmpeg: &Path, src: &Path) -> Result<(u32, u32, Option<f64>), String> {
        let ffprobe = ffmpeg.with_file_name("ffprobe");
        if ffprobe.is_file() {
            let out = Command::new(&ffprobe)
                .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height:stream_side_data=rotation:stream_tags=rotate:format=duration", "-of", "json"])
                .arg(src)
                .output()
                .map_err(|e| format!("ffprobe: {e}"))?;
            let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("ffprobe output: {e}"))?;
            let st = &v["streams"][0];
            let (w, h) = (st["width"].as_u64().unwrap_or(0) as u32, st["height"].as_u64().unwrap_or(0) as u32);
            if w == 0 || h == 0 {
                return Err(format!("{}: no video stream", src.display()));
            }
            let rot = st["side_data_list"][0]["rotation"].as_f64().or_else(|| st["tags"]["rotate"].as_str().and_then(|r| r.parse().ok())).unwrap_or(0.0);
            let quarter = ((rot / 90.0).round() as i64).rem_euclid(2) == 1;
            let duration = v["format"]["duration"].as_str().and_then(|d| d.parse().ok());
            return Ok(if quarter { (h, w, duration) } else { (w, h, duration) });
        }
        // no ffprobe: parse "Stream #0:0...: Video: ..., 1920x1080" and "Duration: 00:00:05.00" from ffmpeg -i
        let out = Command::new(ffmpeg).arg("-hide_banner").arg("-i").arg(src).output().map_err(|e| format!("ffmpeg: {e}"))?;
        let text = String::from_utf8_lossy(&out.stderr);
        let line = text.lines().find(|l| l.contains("Video:")).ok_or_else(|| format!("{}: no video stream", src.display()))?;
        let dims = line
            .split([',', ' '])
            .find_map(|t| {
                let (a, b) = t.split_once('x')?;
                Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?))
            })
            .ok_or("could not read the video size")?;
        let rotated = text.contains("rotation of -90") || text.contains("rotation of 90") || text.contains("rotate          : 90") || text.contains("rotate          : 270");
        let duration = text.lines().find_map(|l| {
            let d = l.trim().strip_prefix("Duration: ")?.split(',').next()?;
            let p: Vec<f64> = d.split(':').filter_map(|x| x.parse().ok()).collect();
            (p.len() == 3).then(|| p[0] * 3600.0 + p[1] * 60.0 + p[2])
        });
        Ok(if rotated { (dims.1, dims.0, duration) } else { (dims.0, dims.1, duration) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cut_between_frames_shows_the_incoming_clip() {
        // 2.8667 s is 86.001 frames at 30 fps: the runtime shows the second clip from frame 86, and so must its frames
        let v = |id: &str, start: f64, end: f64| VideoClip { id: id.into(), src: String::new(), start, end, media_start: 0.0,
            looping: false, playback_rate: 1.0, has_audio: false, volume: 1.0, fade_in: 0.0, fade_out: 0.0 };
        let frames = VideoFrames::from_provider(30.0, &[v("a", 0.0, 2.8667), v("b", 2.8667, 5.0)], FrameProvider(Arc::new(|_, _| None)));
        assert_eq!(frames.frame_index("b", 86.0 / 30.0), Some(0));
        assert_eq!(frames.frame_index("b", 85.0 / 30.0), None);
    }

    fn clip(id: &str, start: f64, end: f64, media_start: f64) -> AudioClip {
        AudioClip {
            id: id.into(),
            src: "/nonexistent/clip.wav".into(),
            start,
            end,
            media_start,
            volume: 1.0,
            looping: false,
            playback_rate: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
        }
    }

    #[test]
    fn audio_clips_are_padded_then_delayed_into_place() {
        let (args, graph) = audio_filter(&[clip("a", 0.0, 3.0, 0.0), clip("b", 9.1, 13.09, 15.75), clip("late", 21.0, 22.0, 0.0)], 1, 20.0);
        // the clip past the end gets no input
        assert_eq!(args.iter().filter(|a| *a == "-i").count(), 2);
        // padded to the rest of the render, then delayed: the stream is exactly 20 s and the clip sits at 9.1 s
        assert!(graph.contains("[2:a]atrim=start=15.75:duration=3.99"), "{graph}");
        assert!(graph.contains("apad=whole_dur=10.9,adelay=9100:all=1[a1]"), "{graph}");
        assert!(graph.contains("apad=whole_dur=20,adelay=0:all=1[a0]"), "{graph}");
        // no trim after the delay (it moved delayed clips back to 0)
        assert!(!graph.contains("atrim=duration=20"), "{graph}");
        assert!(graph.contains("amix=inputs=2:"), "{graph}");
        assert!(graph.contains("loudnorm=I=-14:TP=-1.5") && graph.contains("aresample=48000,asetpts"), "{graph}");
    }
}
