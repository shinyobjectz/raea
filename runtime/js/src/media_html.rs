//! A page's `video` and `audio` sources (raea-runtime-dom's media.rs draws and tells them), decoded by raea-media on a
//! thread of their own. Images are raea-sys-page's (decode.rs).

#[cfg(feature = "media")]
pub mod stream {
    //! A video's or audio's source decoded on a thread of its own by raea-media.
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::sync::mpsc::{Receiver, Sender, channel};

    /// What a decoder tells: the source's metadata, a frame at a time (after the first, a seek's or a playing one's),
    /// where a playing source has got to, that it played to its end, or its failure.
    pub enum Msg {
        Meta { w: u32, h: u32, duration: f64 },
        /// a frame to show now (the first, a seek's), with its halvings when they're asked for (`Cmd::Halvings`)
        Frame(f64, raea_io_media::Frame, Halvings),
        /// a playing video's frame, decoded ahead of its time: the frame at this time, to show at this moment
        Due(f64, raea_io_media::Frame, std::time::Instant, Halvings),
        /// a playing source's time (an audio one's, which has no frames; a video's when paused)
        Time(f64),
        /// playing from this time: the sound is on its way
        Playing(f64),
        /// played to its end (a looping source starts again instead)
        Ended(f64),
        Fail(String),
    }

    /// A frame's halvings, largest first: each a 2x2 mean of the one before (width, height, RGBA). A frame drawn at
    /// under half its size is drawn from one of them, so a 4K frame in a small player is neither sampled across
    /// four of its pixels at a time (it shimmers) nor sent whole to the GPU for every frame.
    pub type Halvings = Vec<(u32, u32, Vec<u8>)>;

    /// `n` halvings of an RGBA picture (fewer once a side is a pixel).
    pub fn halvings(w: u32, h: u32, rgba: &[u8], n: u8) -> Halvings {
        let mut out: Halvings = Vec::new();
        for _ in 0..n {
            let (sw, sh, src) = match out.last() {
                Some(last) => (last.0, last.1, last.2.as_slice()),
                None => (w, h, rgba),
            };
            if sw < 2 || sh < 2 {
                break;
            }
            let (dw, dh) = ((sw / 2) as usize, (sh / 2) as usize);
            let stride = sw as usize * 4;
            let mut px = vec![0u8; dw * dh * 4];
            for (y, row) in px.chunks_exact_mut(dw * 4).enumerate() {
                let (top, bottom) = (&src[2 * y * stride..][..dw * 8], &src[(2 * y + 1) * stride..][..dw * 8]);
                for ((d, a), b) in row.chunks_exact_mut(4).zip(top.chunks_exact(8)).zip(bottom.chunks_exact(8)) {
                    for c in 0..4 {
                        d[c] = ((a[c] as u16 + a[c + 4] as u16 + b[c] as u16 + b[c + 4] as u16 + 2) / 4) as u8;
                    }
                }
            }
            out.push((dw as u32, dh as u32, px));
        }
        out
    }

    /// What the element asks of its decoder. Play plays from `at` on the source's clock (its sound's, when it has
    /// sound and a device): frames as they fall due, each `Frame` news, and `Ended` at its end unless `looping`.
    pub enum Cmd {
        Seek(f64),
        Play { at: f64, looping: bool },
        Pause,
        Muted(bool),
        /// the element's volume, 0..1: the sound's gain while it isn't muted
        Volume(f32),
        /// how many halvings each frame should come with from here on (the page counts them from the size the
        /// video is drawn at; 0 while it is drawn at half its size or more)
        Halvings(u8),
    }

    pub struct Stream {
        tx: Sender<Cmd>,
        rx: Mutex<Receiver<Msg>>,
    }

    impl Stream {
        /// Asks the decoder to seek, play, pause or mute; a decoder that has stopped hears nothing.
        pub fn ask(&mut self, c: Cmd) {
            let _ = self.tx.send(c);
        }

        /// The decoder's next news, if it has any now.
        pub fn next(&self) -> Option<Msg> {
            self.rx.lock().unwrap().try_recv().ok()
        }
    }

    /// Where a src's bytes are as a file: a data: URL written to the temp folder once, named by its content (so a
    /// process killed mid-decode leaves nothing new behind, and the same source reuses its file).
    pub(crate) fn file_of(src: &str) -> Option<PathBuf> {
        if src.starts_with("data:") {
            let (mime, bytes) = raea_io_net::data_url(src)?;
            let ext = match mime.rsplit('/').next().unwrap_or("") {
                "webm" => "webm",
                "mp4" => "mp4",
                "quicktime" => "mov",
                "ogg" => "ogg",
                "mpeg" => "mp3",
                "wav" | "x-wav" | "wave" => "wav",
                "flac" => "flac",
                "aac" => "aac",
                _ => "bin",
            };
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            bytes.hash(&mut h);
            let dir = std::env::temp_dir().join("raea-runtime-js-media");
            std::fs::create_dir_all(&dir).ok()?;
            let p = dir.join(format!("{:016x}-{}.{ext}", h.finish(), bytes.len()));
            if std::fs::metadata(&p).ok().map(|m| m.len()) != Some(bytes.len() as u64) {
                // written aside, then moved in whole: another process may be reading it
                let tmp = dir.join(format!("{:016x}.{}.part", h.finish(), std::process::id()));
                std::fs::write(&tmp, &bytes).ok()?;
                std::fs::rename(&tmp, &p).ok()?;
            }
            return Some(p);
        }
        let s = src.strip_prefix("file://").unwrap_or(src);
        (!s.is_empty()).then(|| PathBuf::from(s))
    }

    fn run(path: &Path, video: bool, ticket: u64, rx: Receiver<Cmd>, tx: Sender<Msg>) {
        let (lock, cv) = &TURN;
        let mut turn = cv.wait_while(lock.lock().unwrap(), |t| *t < ticket).unwrap();
        let done = |turn: &mut std::sync::MutexGuard<u64>| {
            **turn = (**turn).max(ticket + 1);
            cv.notify_all();
        };
        let r = first(path, video, &tx);
        done(&mut turn);
        drop(turn);
        let Some(source) = r else { return };
        crate::media_play::Player::new(path, source).run(rx, tx);
    }

    /// The source opened: a video's metadata and first frame (its decoder, to seek and play on), or audio's duration
    /// (its samples, to play).
    fn first(path: &Path, video: bool, tx: &Sender<Msg>) -> Option<crate::media_play::Source> {
        let mut errs = vec![];
        if !video {
            for b in raea_io_media::preferred() {
                match b.decode_audio(path) {
                    Ok(a) if a.len() > 0 => {
                        let _ = tx.send(Msg::Meta { w: 0, h: 0, duration: a.duration() });
                        return Some(crate::media_play::Source::Audio(a));
                    }
                    Ok(_) => errs.push(format!("{}: no sound", b.name())),
                    Err(e) => errs.push(format!("{}: {e}", b.name())),
                }
            }
            let _ = tx.send(Msg::Fail(errs.join("; ")));
            return None;
        }
        let mut dec = None;
        for b in raea_io_media::preferred() {
            match b.open_video(path) {
                Ok(d) => {
                    dec = Some(d);
                    break;
                }
                Err(e) => errs.push(format!("{}: {e}", b.name())),
            }
        }
        let Some(mut dec) = dec else {
            let _ = tx.send(Msg::Fail(errs.join("; ")));
            return None;
        };
        let i = dec.info().clone();
        let _ = tx.send(Msg::Meta { w: i.width, h: i.height, duration: i.duration });
        let _ = tx.send(match dec.frame_at(0.0) {
            Ok(f) => Msg::Frame(0.0, f, Vec::new()),
            Err(e) => Msg::Fail(e),
        });
        Some(crate::media_play::Source::Video(dec))
    }

    /// Decoders open, and send their metadata and first frame, one at a time in the order they were made (the
    /// document's), so their events go in that order as a browser's do for sources that load alike.
    static TURN: (std::sync::Mutex<u64>, std::sync::Condvar) = (std::sync::Mutex::new(0), std::sync::Condvar::new());
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    pub fn open(src: &str, video: bool) -> Option<Stream> {
        let path = file_of(src)?;
        let ticket = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (ttx, trx) = channel::<Cmd>();
        let (mtx, mrx) = channel::<Msg>();
        std::thread::Builder::new()
            .name("raea-runtime-js-media".into())
            .spawn(move || {
                run(&path, video, ticket, trx, mtx);
            })
            .ok()?;
        Some(Stream { tx: ttx, rx: Mutex::new(mrx) })
    }
}
