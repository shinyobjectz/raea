//! A render's video encoded as segments: each one its own encoder run, started on a keyframe at a cut (split.rs) and
//! written as an MP4 of its own, all joined when the render finishes (join.rs). To whoever drives it, it's one encoder
//! from one backend, so a signer wraps it as one: every frame carries one export's watermark, and the joined file is
//! signed once.

use std::path::{Path, PathBuf};

use raea_io_media::{Audio, Backend, EncodeOpts, EncodeStats, R, VideoDecoder, VideoEncoder};

use crate::split::Cuts;

/// The backend a segmented encoder comes from: `name`'s encoders, unsigned, one a segment.
pub(crate) struct Segs {
    pub name: String,
    pub cuts: Cuts,
    /// the shortest segment, in frames (a second's): a cut sooner than this after the segment's first frame is passed
    /// over, so a run of short shots shares one (a keyframe a few frames apart buys a cache nothing and costs bits)
    pub min: usize,
}

impl Backend for Segs {
    fn name(&self) -> &'static str {
        "segments"
    }
    fn open_video(&self, path: &Path) -> R<Box<dyn VideoDecoder>> {
        raea_io_media::backend_unsigned(&self.name)?.open_video(path)
    }
    fn decode_audio(&self, path: &Path) -> R<Audio> {
        raea_io_media::backend_unsigned(&self.name)?.decode_audio(path)
    }
    fn encoder(&self, out: &Path, opts: &EncodeOpts) -> R<Box<dyn VideoEncoder>> {
        Segmented::new(self, out, opts, false)
    }
    fn encoder_10bit(&self, out: &Path, opts: &EncodeOpts) -> R<Box<dyn VideoEncoder>> {
        Segmented::new(self, out, opts, true)
    }
}

/// Frames pushed in order from frame 0, each to its segment's encoder.
struct Segmented {
    name: String,
    opts: EncodeOpts,
    ten: bool,
    out: PathBuf,
    /// the segments' folder, gone with the encoder (RAEA_KEEP_SEGMENTS=1 keeps it)
    dir: PathBuf,
    cuts: Cuts,
    min: usize,
    cur: Option<Box<dyn VideoEncoder>>,
    /// frames in the current segment, and pushed in all
    len: usize,
    n: usize,
    paths: Vec<PathBuf>,
    /// the segments' video, as their encoder named it
    video: String,
}

impl Segmented {
    fn new(s: &Segs, out: &Path, opts: &EncodeOpts, ten: bool) -> R<Box<dyn VideoEncoder>> {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let k = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("raea-segments-{}-{k}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let (name, cuts, min) = (s.name.clone(), s.cuts.clone(), s.min);
        Ok(Box::new(Segmented { name, opts: opts.clone(), ten, out: out.into(), dir, cuts, min, cur: None, len: 0, n: 0, paths: Vec::new(), video: String::new() }))
    }

    /// The encoder frame `n` goes to: a new segment's at a cut (once the one it ends has `min` frames) or at the first.
    fn next(&mut self) -> R<&mut Box<dyn VideoEncoder>> {
        if self.cur.is_some() && self.cuts.starts(self.n as i64, self.len, self.min) {
            self.close()?;
        }
        if self.cur.is_none() {
            let path = self.dir.join(format!("segment-{:04}.mp4", self.paths.len()));
            let b = raea_io_media::backend_unsigned(&self.name)?;
            self.cur = Some(if self.ten { b.encoder_10bit(&path, &self.opts)? } else { b.encoder(&path, &self.opts)? });
            self.paths.push(path);
            self.len = 0;
        }
        self.len += 1;
        self.n += 1;
        Ok(self.cur.as_mut().expect("an encoder was opened above"))
    }

    fn close(&mut self) -> R<()> {
        if let Some(e) = self.cur.take() {
            self.video = e.finish(None)?.video_codec;
        }
        Ok(())
    }
}

impl VideoEncoder for Segmented {
    fn push(&mut self, rgba: &[u8]) -> R<()> {
        self.next()?.push(rgba)
    }
    fn push_nv12(&mut self, nv12: &[u8], w: usize, h: usize) -> R<()> {
        self.next()?.push_nv12(nv12, w, h)
    }
    fn push_p010(&mut self, p010: &[u8], w: usize, h: usize) -> R<()> {
        self.next()?.push_p010(p010, w, h)
    }
    fn finish(mut self: Box<Self>, audio: Option<&Audio>) -> R<EncodeStats> {
        self.close()?;
        let sound = match audio {
            Some(a) => Some(raea_io_media::backend_unsigned(&self.name)?.sound(a)?),
            None => None,
        };
        let t = std::time::Instant::now();
        let frames = crate::join::join(&self.paths, sound.as_ref(), self.opts.fps, &self.out)?;
        if raea_io_trace::stat("RAEA_SEGMENTS_TRACE") {
            eprintln!("raea segments: {frames} frames in {} segments joined in {:.1} ms (cuts {:?})", self.paths.len(), t.elapsed().as_secs_f64() * 1e3, self.cuts.all());
        }
        let audio_codec = sound.map_or(String::new(), |s| if s.aac { "aac".into() } else { "opus".into() });
        let bytes = std::fs::metadata(&self.out).map(|m| m.len()).unwrap_or(0);
        Ok(EncodeStats { frames, bytes, video_codec: format!("{} in {} segments", self.video, self.paths.len()), audio_codec })
    }
}

impl Drop for Segmented {
    fn drop(&mut self) {
        if std::env::var("RAEA_KEEP_SEGMENTS").as_deref() != Ok("1") {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}
