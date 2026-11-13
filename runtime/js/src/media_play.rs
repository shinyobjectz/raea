//! A `video` or `audio` playing (media_html.rs's stream, on its decoder's thread): seeks answered with the frame at the
//! time asked; played, frames as they fall due on the source's clock, which is its sound's (media_sound.rs) when it has
//! sound and the Mac a device, else the wall's. A playing video's frames are decoded a little ahead of the clock and
//! sent with the moment each is due (`Msg::Due`), so how long a frame takes to decode is no part of when it is
//! shown, and a loop's first frame is ready before its last has gone. A decoder that falls behind the clock skips
//! frames rather than letting the picture lag its sound.

use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use crate::media_html::stream::{Cmd, Msg, halvings};
use crate::media_sound::Sound;

/// What was opened: a video's decoder, or an audio source's samples.
pub enum Source {
    Video(Box<dyn raea_io_media::VideoDecoder>),
    Audio(raea_io_media::Audio),
}

/// The clock while playing: the sound's, else the wall's from when play began.
enum Clock {
    Sound,
    Wall { from: f64, began: Instant },
}

pub struct Player<'a> {
    path: &'a Path,
    source: Source,
    duration: f64,
    fps: f64,
    /// the sound, opened on the first play; None after it couldn't be
    sound: Option<Option<Sound>>,
    muted: bool,
    volume: f32,
    clock: Option<Clock>,
    looping: bool,
    /// the frame (index at fps) to decode next while playing
    cursor: i64,
    /// what the cursor's frames are ahead of the clock by, in laps: `duration` once the cursor has gone round a
    /// loop the clock hasn't yet, else 0
    lap: f64,
    /// frames the clock passed before they were decoded, since the last frame sent (for the stats)
    skipped: i64,
    /// when the last Time news went (an audio source's, which has no frames)
    told: Option<Instant>,
    /// how many halvings each frame is sent with (`Cmd::Halvings`)
    halvings: u8,
    /// where the picture last sent was in time, while paused: sent again when the halvings asked for change
    still: Option<f64>,
    /// RAEA_MEDIA_STATS=1: how play is going, said a line a second
    stats: Option<Stats>,
}

/// A second of play as the decoder's thread saw it (RAEA_MEDIA_STATS=1): the frames it sent, the frames the clock
/// passed over (decoding fell behind, or the thread woke late), and how long a frame took to decode.
struct Stats {
    since: Instant,
    frames: u32,
    skipped: u32,
    sum: f64,
    max: f64,
    /// the longest wait between two frames decoded, ms
    gap: f64,
    last: Option<Instant>,
}

impl Stats {
    fn new() -> Self {
        Stats { since: Instant::now(), frames: 0, skipped: 0, sum: 0.0, max: 0.0, gap: 0.0, last: None }
    }
    fn frame(&mut self, decode: Duration, skipped: i64, fps: f64) {
        let now = Instant::now();
        if let Some(l) = self.last {
            self.gap = self.gap.max((now - l).as_secs_f64() * 1e3);
        }
        self.last = Some(now);
        let ms = decode.as_secs_f64() * 1e3;
        (self.frames, self.skipped, self.sum, self.max) = (self.frames + 1, self.skipped + skipped.max(0) as u32, self.sum + ms, self.max.max(ms));
        let took = self.since.elapsed().as_secs_f64();
        if took >= 1.0 {
            eprintln!("raea-media: play {took:.2} s: {} frames sent of {:.0} due, {} skipped, decode avg {:.1} ms max {:.1} ms, longest gap {:.0} ms", self.frames, took * fps, self.skipped, self.sum / self.frames.max(1) as f64, self.max, self.gap);
            *self = Stats { last: self.last, ..Stats::new() };
        }
    }
}

impl<'a> Player<'a> {
    pub fn new(path: &'a Path, source: Source) -> Self {
        let (duration, fps) = match &source {
            Source::Video(d) => (d.info().duration, d.info().fps),
            Source::Audio(a) => (a.duration(), 0.0),
        };
        let fps = if fps.is_finite() && fps > 1.0 { fps } else { 30.0 };
        Player { path, source, duration, fps, sound: None, muted: false, volume: 1.0, clock: None, looping: false, cursor: 0, lap: 0.0, skipped: 0, told: None, halvings: 0, still: Some(0.0), stats: (std::env::var("RAEA_MEDIA_STATS").as_deref() == Ok("1")).then(Stats::new) }
    }

    /// Answers the element's asks until it's gone (its stream dropped).
    pub fn run(mut self, rx: Receiver<Cmd>, tx: Sender<Msg>) {
        loop {
            // playing: wake for the next frame; paused: wait for an ask
            let first = if self.clock.is_some() {
                match rx.recv_timeout(self.until_next()) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return self.stop(),
                }
            } else {
                match rx.recv() {
                    Ok(c) => Some(c),
                    Err(_) => return self.stop(),
                }
            };
            // the asks waiting, the latest of each kind
            let (mut seek, mut play, mut pause) = (None, None, false);
            for c in first.into_iter().chain(std::iter::from_fn(|| rx.try_recv().ok())) {
                match c {
                    Cmd::Seek(t) => seek = Some(t),
                    Cmd::Play { at, looping } => (play, pause) = (Some((at, looping)), false),
                    Cmd::Pause => (play, pause) = (None, true),
                    Cmd::Muted(m) => {
                        self.muted = m;
                        if let Some(Some(s)) = &self.sound {
                            s.set_muted(m);
                        }
                    }
                    Cmd::Volume(v) => {
                        self.volume = v;
                        if let Some(Some(s)) = &self.sound {
                            s.set_volume(v);
                        }
                    }
                    Cmd::Halvings(n) => {
                        // paused, the picture showing is sent again with what it is now drawn from
                        if n > self.halvings && self.clock.is_none() && seek.is_none() {
                            seek = self.still;
                        }
                        self.halvings = n;
                    }
                }
            }
            if pause && self.clock.is_some() {
                let t = self.now();
                self.halt();
                if tx.send(Msg::Time(t)).is_err() {
                    return;
                }
            }
            if let Some(t) = seek {
                // the frame at exactly the time asked (the element's `seeked`), and a playing clock goes on from it
                if self.clock.is_some() {
                    self.go(t);
                }
                if !self.frame(t, &tx) {
                    return self.stop();
                }
            }
            if let Some((at, looping)) = play {
                self.looping = looping;
                if self.clock.is_none() {
                    self.go(seek.unwrap_or(at));
                    if tx.send(Msg::Playing(seek.unwrap_or(at))).is_err() {
                        return self.stop();
                    }
                }
            }
            if self.clock.is_some() && !self.tick(&tx) {
                return self.stop();
            }
        }
    }

    /// How long until there is something to do: the cursor's frame comes within the lead, or the clock reaches the
    /// end. (Frames were once sent when they fell due, each after its own decode: 22 to 29 ms apart at 60 fps where
    /// 16.7 is even, a 4K frame 6.6 ms late, and a loop's first frame behind a seek; RAEA_MEDIA_STATS, 11-13.)
    fn until_next(&self) -> Duration {
        let frame = 1.0 / self.fps;
        let wait = match &self.source {
            Source::Video(_) => {
                let now = self.now();
                let decode = if self.at_end() { f64::INFINITY } else { self.cursor_time() - self.lead() - now };
                decode.min(self.duration - now).clamp(0.0005, frame)
            }
            Source::Audio(_) => 0.5 * frame,
        };
        Duration::from_secs_f64(wait)
    }

    /// The cursor's frame's time on the clock.
    fn cursor_time(&self) -> f64 {
        self.cursor as f64 / self.fps + self.lap
    }

    /// The cursor is past the source's last frame (a looping source's cursor goes round instead).
    fn at_end(&self) -> bool {
        !self.looping && self.duration.is_finite() && self.cursor as f64 / self.fps >= self.duration - 0.5 / self.fps
    }

    /// How far ahead of the clock frames are decoded: two frames, 50 ms at the least; and nearing a loop's end 150 ms,
    /// since going back to the start is a seek (70 ms measured) that must be over before the first frame is due. No
    /// more than 200 MB of frames wait at once (a 4K frame is 33 MB).
    fn lead(&self) -> f64 {
        let frame = 1.0 / self.fps;
        let bytes = match &self.source {
            Source::Video(d) => (d.info().width as f64 * d.info().height as f64 * 4.0).max(1.0),
            Source::Audio(_) => 1.0,
        };
        let most = (200e6 / bytes).floor().max(2.0) * frame;
        let near_wrap = self.looping && self.duration.is_finite() && self.duration - self.cursor as f64 * frame < 0.25;
        let want = if near_wrap || self.lap > 0.0 { 0.15 } else { (2.0 * frame).max(0.05) };
        want.min(most)
    }

    /// The clock's time now.
    fn now(&self) -> f64 {
        match (&self.clock, &self.sound) {
            (Some(Clock::Sound), Some(Some(s))) => s.time(),
            (Some(Clock::Wall { from, began }), _) => from + began.elapsed().as_secs_f64(),
            _ => 0.0,
        }
    }

    /// Plays from `t`: the sound when there is one (opened the first time), else the wall.
    fn go(&mut self, t: f64) {
        if self.sound.is_none() {
            let audio = match &self.source {
                Source::Audio(a) => Some(raea_io_media::Audio { rate: a.rate, channels: a.channels.clone() }),
                // a video's sound track, decoded once, on the first play (a video only looked at never pays for it)
                Source::Video(_) => raea_io_media::preferred().into_iter().find_map(|b| b.decode_audio(self.path).ok()).filter(|a| a.len() > 0),
            };
            let volume = self.volume;
            self.sound = Some(audio.and_then(|a| Sound::open(a, self.muted).inspect(|s| s.set_volume(volume)).inspect_err(|e| bevy::log::debug!("raea-runtime-js: media plays on the wall's clock: {e}")).ok()));
        }
        (self.cursor, self.lap, self.skipped) = ((t * self.fps).floor() as i64, 0.0, 0);
        self.told = None;
        self.restart(t);
    }

    /// The clock started at `t` (the sound from there, else the wall).
    fn restart(&mut self, t: f64) {
        self.clock = Some(match &self.sound {
            Some(Some(s)) => {
                s.start(t);
                Clock::Sound
            }
            _ => Clock::Wall { from: t, began: Instant::now() },
        });
    }

    fn halt(&mut self) {
        if let Some(Some(s)) = &self.sound {
            s.stop();
        }
        self.clock = None;
    }

    fn stop(mut self) {
        self.halt();
    }

    /// The frame at `t`, sent as the frame at exactly `t`; false when the element's gone.
    fn frame(&mut self, t: f64, tx: &Sender<Msg>) -> bool {
        match &mut self.source {
            Source::Video(dec) => {
                self.still = Some(t);
                let n = self.halvings;
                tx.send(dec.frame_at(t).map_or_else(Msg::Fail, |f| {
                    let halves = halvings(f.width, f.height, &f.rgba, n);
                    Msg::Frame(t, f, halves)
                }))
                .is_ok()
            }
            Source::Audio(_) => tx.send(Msg::Time(t)).is_ok(),
        }
    }

    /// One step of playing: the end (the clock again from the start when looping), or the cursor's frame decoded
    /// and sent with the moment it is due, once it is within the lead. One frame a step, so an ask is never kept
    /// waiting behind more than one decode.
    fn tick(&mut self, tx: &Sender<Msg>) -> bool {
        let mut t = self.now();
        let frame = 1.0 / self.fps;
        if self.duration.is_finite() && self.looping && t >= self.duration {
            // round again: the frames already sent for this lap were timed on the old clock's end, which is now
            if self.lap > 0.0 {
                self.lap = 0.0;
            } else {
                self.cursor = 0;
            }
            self.restart(t - self.duration);
            t = self.now();
        } else if self.duration.is_finite() && !self.looping && t >= self.duration - 0.5 * frame {
            self.halt();
            return tx.send(Msg::Ended(self.duration)).is_ok();
        }
        match &self.source {
            Source::Video(_) => {
                // a looping source's cursor goes round ahead of the clock
                if self.looping && self.duration.is_finite() && self.lap == 0.0 && self.cursor as f64 * frame >= self.duration - 0.5 * frame {
                    (self.cursor, self.lap) = (0, self.duration);
                }
                if self.at_end() || self.cursor_time() - self.lead() > t {
                    return true;
                }
                // behind the clock by more than a frame: the frames it passed are skipped
                if self.cursor_time() + frame < t {
                    let i = ((t - self.lap) * self.fps).floor() as i64;
                    self.skipped += i - self.cursor;
                    self.cursor = i;
                }
                let (at, due_in) = (self.cursor as f64 * frame, self.cursor_time() - t);
                let began = Instant::now();
                let Source::Video(dec) = &mut self.source else { return true };
                // asked for mid-frame: a source whose frames aren't exactly on fps's grid gives the frame showing then
                let n = self.halvings;
                let got = dec.frame_at(at + 0.5 * frame).map(|f| {
                    let halves = halvings(f.width, f.height, &f.rgba, n);
                    (f, halves)
                });
                let took = began.elapsed();
                let due = began + Duration::from_secs_f64(due_in.max(0.0));
                self.cursor += 1;
                if let Some(s) = self.stats.as_mut() {
                    s.frame(took, std::mem::take(&mut self.skipped), self.fps);
                }
                self.still = Some(at);
                tx.send(got.map_or_else(Msg::Fail, |(f, halves)| Msg::Due(at, f, due, halves))).is_ok()
            }
            Source::Audio(_) => {
                if self.told.is_some_and(|w| w.elapsed() < Duration::from_millis(100)) {
                    return true;
                }
                self.told = Some(Instant::now());
                tx.send(Msg::Time(t)).is_ok()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::media_html::stream::{self, Cmd, Msg, Stream};
    use std::time::{Duration, Instant};

    /// The next news within `within`, polled as the page host polls it (each frame).
    fn next(st: &Stream, within: Duration) -> Option<Msg> {
        let t = Instant::now();
        while t.elapsed() < within {
            if let Some(m) = st.next() {
                return Some(m);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        None
    }

    /// A second of 30 fps picture and a tone (ffmpeg's test sources), or None without ffmpeg.
    fn clip() -> Option<std::path::PathBuf> {
        let p = std::env::temp_dir().join(format!("raea-media-play-{}.mp4", std::process::id()));
        let ok = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=160x90:rate=30", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000", "-t", "1", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
            .arg(&p)
            .status()
            .is_ok_and(|s| s.success());
        ok.then_some(p)
    }

    /// Played on the wall's clock (RAEA_AUDIO_MUTE=1: no device), a video's frames come as they fall due and it ends
    /// at its end; a seek answers with the frame at exactly its time; looping, it goes round; paused, it stops.
    #[test]
    fn plays_seeks_loops_and_pauses() {
        // SAFETY: set before any stream reads it, and nothing else in this test binary reads it concurrently
        unsafe { std::env::set_var("RAEA_AUDIO_MUTE", "1") };
        let Some(path) = clip() else { return eprintln!("no ffmpeg: skipped") };
        let mut st = stream::open(path.to_str().unwrap(), true).expect("a stream");
        assert!(matches!(next(&st, Duration::from_secs(10)), Some(Msg::Meta { w: 160, h: 90, .. })));
        assert!(matches!(next(&st, Duration::from_secs(5)), Some(Msg::Frame(t, ..)) if t == 0.0));

        let began = Instant::now();
        st.ask(Cmd::Play { at: 0.0, looping: false });
        assert!(matches!(next(&st, Duration::from_secs(1)), Some(Msg::Playing(t)) if t == 0.0));
        let (mut frames, mut last, mut ended) = (0, -1.0, None);
        while let Some(m) = next(&st, Duration::from_secs(3)) {
            match m {
                Msg::Due(t, f, due, _) => {
                    assert!(t > last, "frames in order: {t} after {last}");
                    assert_eq!((f.width, f.height), (160, 90));
                    // each is due when its time comes on the clock (the wall's, here), sent before then
                    let on_clock = due.duration_since(began).as_secs_f64();
                    assert!((on_clock - t).abs() < 0.06, "the frame at {t} is due at {on_clock}");
                    (frames, last) = (frames + 1, t);
                }
                Msg::Ended(t) => {
                    ended = Some((t, began.elapsed().as_secs_f64()));
                    break;
                }
                _ => {}
            }
        }
        let (end, took) = ended.expect("it ended");
        assert!((end - 1.0).abs() < 0.05, "ended at its duration: {end}");
        assert!((0.85..1.4).contains(&took), "a second's clip took {took} s");
        assert!(frames >= 25, "{frames} frames of 30");

        st.ask(Cmd::Seek(0.5));
        assert!(matches!(next(&st, Duration::from_secs(2)), Some(Msg::Frame(t, ..)) if t == 0.5));

        st.ask(Cmd::Play { at: 0.5, looping: true });
        let (mut wrapped, mut last, until) = (false, 0.5, Instant::now() + Duration::from_millis(1200));
        while Instant::now() < until {
            match next(&st, Duration::from_millis(200)) {
                Some(Msg::Due(t, _, due, _)) => {
                    // the first frame of the next lap is sent before the last frame of this one has gone
                    if t < last {
                        wrapped = true;
                        assert!(due > Instant::now(), "the loop's first frame came after it was due");
                    }
                    last = t;
                }
                Some(Msg::Ended(_)) => panic!("a looping source doesn't end"),
                _ => {}
            }
        }
        assert!(wrapped, "looping went round");

        st.ask(Cmd::Pause);
        // paused, asked for halvings: the picture showing comes again with them, each half the one before
        while next(&st, Duration::from_millis(300)).is_some() {}
        st.ask(Cmd::Halvings(2));
        match next(&st, Duration::from_secs(2)) {
            Some(Msg::Frame(_, f, halves)) => {
                assert_eq!((f.width, f.height), (160, 90));
                assert_eq!(halves.iter().map(|(w, h, px)| (*w, *h, px.len())).collect::<Vec<_>>(), vec![(80, 45, 80 * 45 * 4), (40, 22, 40 * 22 * 4)]);
                // a level's pixel is the mean of the four it stands for
                let mean = |c: usize| ((f.rgba[c] as u16 + f.rgba[4 + c] as u16 + f.rgba[160 * 4 + c] as u16 + f.rgba[160 * 4 + 4 + c] as u16 + 2) / 4) as u8;
                assert_eq!(&halves[0].2[..4], &[mean(0), mean(1), mean(2), mean(3)]);
            }
            _ => panic!("no frame with its halvings"),
        }
        // what was on its way comes; then nothing more
        while next(&st, Duration::from_millis(300)).is_some() {}
        assert!(next(&st, Duration::from_millis(300)).is_none(), "frames after pausing");
        let _ = std::fs::remove_file(path);
    }
}
