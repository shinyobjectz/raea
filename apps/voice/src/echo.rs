//! raea's own voice taken out of what the microphone hears, so the person can talk over it.
//!
//! A computer's microphone hears its speakers. While raea speaks, what was sent to the speakers (the far side) is
//! passed through a filter that learns the way from speaker to microphone (an NLMS adaptive filter, 128 ms long, at
//! the hearing rate), and what the filter makes of it is taken from the microphone's sound. What is left is the
//! person, when they speak. Where in time the echo sits (the sound card's and the room's delay) is found first, by
//! lining up the two sides' loudness over the last second.
//!
//! Until the filter has learned the room (`sure`: a second of its voice, the echo down by 10 dB or more) nothing
//! left over counts as the person, so raea is never stopped by its own voice. Once it has, sound that stands clear
//! of the echo is the person (`near`). Two filters do it (the two-path way): the one in use never learns, the one
//! behind it always does and replaces it when it leaves less, so the person's voice is never learned as echo.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::audio::Resample;
use crate::turns::WIN;

/// The filter's length: 128 ms of the way from speaker to microphone.
const TAPS: usize = 2048;
/// Loudness is compared in blocks of 4 ms.
const BLOCK: usize = 64;
/// The echo is looked for up to 480 ms late.
const LAGS: usize = 120;
/// A second of loudness is lined up.
const ENV: usize = 250;
/// How much of the far side is kept: the latest echo the filter can be asked about.
const HIST: usize = LAGS * BLOCK + TAPS + WIN;
/// The far side counts as sounding above this mean square (about -60 dBFS).
const SOUNDING: f32 = 1e-6;
/// How fast the filter learns.
const MU: f32 = 0.3;

/// What a window turned out to hold.
#[derive(Clone, Copy, Debug, Default)]
pub struct Heard {
    /// raea's own voice was sounding in it
    pub far: bool,
    /// something stands clear of raea's own voice (always, when raea is quiet)
    pub near: bool,
    /// the filter has learned the room: what's left can be trusted
    pub sure: bool,
}

pub struct Echo {
    far: Vec<f32>,
    w: Vec<f32>,
    /// the filter behind it, always learning; and for how many windows it has done better, or worse
    behind: Vec<f32>,
    better: usize,
    worse: usize,
    pub delay: usize,
    mic_env: Vec<f32>,
    far_env: Vec<f32>,
    /// a delay seen, and how many times in a row
    vote: (usize, usize),
    /// how much quieter the echo leaves than it came, smoothed
    pub erle: f32,
    /// windows learned on since the delay was last set
    learned: usize,
    sure: bool,
    windows: usize,
}

impl Default for Echo {
    fn default() -> Echo {
        Echo { far: vec![0.0; HIST], w: vec![0.0; TAPS], behind: vec![0.0; TAPS], better: 0, worse: 0, delay: 0, mic_env: Vec::new(), far_env: Vec::new(), vote: (0, 0), erle: 1.0, learned: 0, sure: false, windows: 0 }
    }
}

fn power(x: &[f32]) -> f32 {
    let mut p = 0.0;
    for s in x {
        p += s * s;
    }
    p
}

impl Echo {
    /// The filter starts over (the echo moved in time, or the room changed).
    fn forget(&mut self) {
        self.w.fill(0.0);
        self.behind.fill(0.0);
        (self.erle, self.learned, self.sure, self.better, self.worse) = (1.0, 0, false, 0, 0);
    }

    /// Where the echo sits: the lag (in blocks) at which the microphone's loudness follows the far side's best.
    fn lag(&self) -> Option<usize> {
        if self.mic_env.len() < ENV || self.far_env.len() < ENV + LAGS {
            return None;
        }
        let mic = &self.mic_env[self.mic_env.len() - ENV..];
        let mean = |x: &[f32]| x.iter().sum::<f32>() / x.len() as f32;
        let (mm, mut best) = (mean(mic), (0.6, None));
        let mv: f32 = mic.iter().map(|a| (a - mm) * (a - mm)).sum();
        for l in 0..=LAGS {
            let at = self.far_env.len() - ENV - l;
            let far = &self.far_env[at..at + ENV];
            let fm = mean(far);
            let (mut c, mut fv) = (0.0, 0.0);
            for (a, b) in mic.iter().zip(far) {
                c += (a - mm) * (b - fm);
                fv += (b - fm) * (b - fm);
            }
            let r = c / (mv * fv).sqrt().max(1e-12);
            if r > best.0 {
                best = (r, Some(l));
            }
        }
        best.1
    }

    /// One pass of a filter over a window: what's left of `mic` goes to `out`; returns (left, echo made) as power,
    /// each sample's judged before the filter learns from it.
    fn pass(far: &[f32], delay: usize, w: &mut [f32], mic: &[f32], learn: bool, out: &mut [f32]) -> (f32, f32) {
        let (mut left, mut made) = (0.0, 0.0);
        let first = far.len() - mic.len() - delay;
        let mut norm = power(&far[first + 1 - TAPS..=first]);
        for (n, d) in mic.iter().enumerate() {
            let at = first + n;
            if n > 0 {
                norm += far[at] * far[at] - far[at - TAPS] * far[at - TAPS];
            }
            let x = &far[at + 1 - TAPS..=at];
            let mut y = 0.0;
            for (a, b) in w.iter().zip(x) {
                y += a * b;
            }
            let e = d - y;
            if learn && norm > SOUNDING {
                let g = MU * e / (norm + 1e-3);
                for (a, b) in w.iter_mut().zip(x) {
                    *a += g * b;
                }
            }
            out[n] = e;
            left += e * e;
            made += y * y;
        }
        (left, made)
    }

    /// A window of the microphone and the window of the far side played with it. What's left of the microphone's
    /// goes to `out`.
    pub fn push(&mut self, mic: &[f32], far: &[f32], out: &mut Vec<f32>) -> Heard {
        self.far.drain(..far.len());
        self.far.extend_from_slice(far);
        for (m, f) in mic.chunks(BLOCK).zip(far.chunks(BLOCK)) {
            self.mic_env.push(power(m));
            self.far_env.push(power(f));
        }
        let (m, f) = (self.mic_env.len().saturating_sub(ENV), self.far_env.len().saturating_sub(ENV + LAGS));
        self.mic_env.drain(..m);
        self.far_env.drain(..f);
        self.windows += 1;
        out.clear();
        out.resize(mic.len(), 0.0);

        // is raea's voice in what the filter is asked about, and has the echo moved?
        let span = &self.far[self.far.len() - mic.len() - self.delay - TAPS..self.far.len() - self.delay];
        let sounding = power(span) / span.len() as f32 > SOUNDING;
        let loud = self.far_env[self.far_env.len().saturating_sub(ENV)..].iter().sum::<f32>() / (ENV * BLOCK) as f32 > SOUNDING;
        if loud && self.windows % 4 == 0 {
            if let Some(l) = self.lag() {
                self.vote = if l == self.vote.0 { (l, self.vote.1 + 1) } else { (l, 1) };
                // the filter starts 8 ms before where the echo is loudest
                let to = l.saturating_sub(2) * BLOCK;
                if self.vote.1 >= 3 && to.abs_diff(self.delay) > 2 * BLOCK {
                    self.delay = to;
                    self.forget();
                }
            }
        }
        if !sounding {
            out.copy_from_slice(mic);
            return Heard { far: false, near: true, sure: self.sure };
        }

        // The filter that is used only takes out; a second one behind it never stops learning. When the second
        // leaves less (the first hadn't met this sound yet, or the room changed), it becomes the first. The person's
        // voice can't be learned as echo that way: it makes the second worse, not better, and the first is untouched.
        let (left, made) = Echo::pass(&self.far, self.delay, &mut self.w, mic, false, out);
        let mut scratch = vec![0.0; mic.len()];
        // (the one behind is judged as it stood before this window, then learns from it: judged while it learns, it
        // seems to take out some of any voice, since each sample's correction carries into the next)
        let (behind, _) = Echo::pass(&self.far, self.delay, &mut self.behind, mic, false, &mut scratch);
        Echo::pass(&self.far, self.delay, &mut self.behind, mic, true, &mut scratch);
        // (better only when it also takes most of the sound out: with the person talking nothing can, so a filter
        // that chased their voice is never taken up)
        let came = power(mic);
        self.better = if behind < 0.7 * left && behind < 0.5 * came { self.better + 1 } else { 0 };
        self.worse = if behind > 2.0 * left { self.worse + 1 } else { 0 };
        // (the person's voice leaves the one behind no better off; a sound the filter hadn't met does, at once)
        let misfit = behind < 0.85 * left;
        if self.better >= 2 {
            self.w.copy_from_slice(&self.behind);
            self.better = 0;
        } else if self.worse >= 3 {
            self.behind.copy_from_slice(&self.w);
            self.worse = 0;
        }
        // clear of the echo: well above what the filter usually leaves of it (four times, and never under a sixteenth),
        // and not something the filter behind could have taken out
        // (what is louder than the whole echo can't be the echo misjudged, whatever the filter behind makes of it)
        let near = (!misfit || left > 2.0 * made) && left > made * (4.0 / self.erle).max(0.06) && left / mic.len() as f32 > SOUNDING;
        if !(self.sure && near) {
            self.erle = 0.9 * self.erle + 0.1 * (came / left.max(1e-12)).min(1000.0);
        }
        // a filter gone wrong adds sound: then the microphone's own is what's left
        if left > came {
            out.copy_from_slice(mic);
        }
        self.learned += 1;
        // a second of its own voice learned, and the echo down by 10 dB (7 dB keeps it, once reached)
        self.sure = self.learned >= 32 && self.erle >= if self.sure { 5.0 } else { 10.0 };
        Heard { far: true, near, sure: self.sure }
    }
}

/// The far side lined up with the microphone, window by window. Both come at their own pace in pieces, so a
/// microphone window waits (a few milliseconds) until the far side that played with it, and the two windows after,
/// have arrived: the echo then always comes after the sound that made it, never before.
pub struct Sides {
    rs: Resample,
    far: Vec<f32>,
    /// how many far samples came before `far[0]`
    base: u64,
    /// the far sample that plays as the microphone's first is heard
    offset: Option<u64>,
    mic: u64,
    held: VecDeque<(Vec<f32>, Instant)>,
    room: Option<f32>,
}

impl Sides {
    /// `rate`: the far side's. `room`: a simulated room's gain (checks), in place of a real one.
    pub fn new(rate: u32, room: Option<f32>) -> Sides {
        Sides { rs: Resample::new(rate, crate::turns::RATE as u32), far: vec![0.0; SPAN + WIN], base: 0, offset: None, mic: 0, held: VecDeque::new(), room }
    }

    /// More of what went to the speakers.
    pub fn far(&mut self, x: &[f32]) {
        self.rs.push(x, &mut self.far);
    }

    /// A window from the microphone.
    pub fn mic(&mut self, win: Vec<f32>) {
        self.held.push_back((win, Instant::now()));
    }

    /// The next microphone window whose far side has come, and that far side.
    pub fn next(&mut self) -> Option<(Vec<f32>, Vec<f32>)> {
        let waited = self.held.front()?.1.elapsed();
        let total = self.base + self.far.len() as u64;
        let offset = *self.offset.get_or_insert(total);
        // where the far side stands as this window ends, and the two windows more the filter is given
        let now = offset + self.mic + WIN as u64;
        let end = now + 2 * WIN as u64;
        if total < end {
            // the speakers aren't being fed (they stopped, or never started): quiet stands in
            if waited < Duration::from_millis(300) {
                return None;
            }
            self.far.resize(self.far.len() + (end - total) as usize, 0.0);
        }
        let (mut mic, _) = self.held.pop_front()?;
        let at = |i: u64| (i - self.base) as usize;
        if let Some(gain) = self.room {
            room(&self.far[at(now) - WIN - SPAN..at(now)], gain, &mut mic);
        }
        let far = self.far[at(end) - WIN..at(end)].to_vec();
        self.mic += WIN as u64;
        // the far side behind what any later window can ask for goes
        let old = at(now).saturating_sub(WIN + SPAN);
        if old > 16_000 {
            self.far.drain(..old);
            self.base += old as u64;
        }
        Some((mic, far))
    }
}

/// A room, for checks with no microphone: what the far side sounds like to a microphone 40 ms and a few surfaces
/// away, at `gain`, added to `mic`. `far` ends where `mic` does and is `SPAN` longer.
pub fn room(far: &[f32], gain: f32, mic: &mut [f32]) {
    const WAYS: [(usize, f32); 6] = [(640, 1.0), (688, -0.45), (816, 0.3), (1008, -0.2), (1360, 0.12), (1840, -0.07)];
    let first = far.len() - mic.len();
    for (n, m) in mic.iter_mut().enumerate() {
        let mut e = 0.0;
        for (late, by) in WAYS {
            e += by * far[first + n - late];
        }
        *m += gain * e;
    }
}
/// How much more of the far side `room` needs than the window it fills.
pub const SPAN: usize = 1840;

#[cfg(test)]
mod tests {
    use super::*;

    /// Something like speech: noise that swells and falls a few times a second.
    fn babble(n: usize, seed: u32) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|i| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (s >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
                noise * 0.4 * (0.55 + 0.45 * (i as f32 * 0.0021).sin()) * if (i / 4000) % 3 == 2 { 0.05 } else { 1.0 }
            })
            .collect()
    }

    /// Runs `secs` of raea talking in the simulated room, the person talking over it from `over` (a window) on.
    fn run(secs: usize, over: Option<usize>) -> (Echo, Vec<Heard>, Vec<f32>) {
        let n = secs * 16_000 / WIN;
        let far = babble(n * WIN + SPAN, 7);
        let person = babble(n * WIN, 99);
        let (mut echo, mut heard, mut ratio, mut out) = (Echo::default(), Vec::new(), Vec::new(), Vec::new());
        for k in 0..n {
            let mut mic = vec![0.0; WIN];
            room(&far[k * WIN..(k + 1) * WIN + SPAN], 0.8, &mut mic);
            if over.is_some_and(|o| k >= o) {
                for (m, p) in mic.iter_mut().zip(&person[k * WIN..]) {
                    *m += p * 0.8;
                }
            }
            let came = power(&mic);
            heard.push(echo.push(&mic, &far[SPAN + k * WIN..SPAN + (k + 1) * WIN], &mut out));
            ratio.push(power(&out) / came.max(1e-12));
        }
        (echo, heard, ratio)
    }

    #[test]
    fn its_own_voice_is_learned_and_taken_out() {
        let (echo, heard, ratio) = run(6, None);
        // the echo's direct sound is 640 samples late: the filter starts just before it
        assert!(echo.delay <= 640 && echo.delay + 256 >= 640, "delay {}", echo.delay);
        let sure = heard.iter().position(|h| h.sure).expect("never sure");
        assert!(sure < 3 * 16_000 / WIN, "sure only after {sure} windows");
        // once sure, nothing is taken for the person, and the echo is down to a twentieth or less in most windows
        let after = sure + 16;
        // (some windows stand out while the filter in use waits for the one behind; the session also wants them to
        // sound like speech, which noise doesn't)
        let own = heard[after..].iter().filter(|h| h.far && h.near).count();
        assert!(own * 5 <= heard.len() - after, "its own voice taken for the person in {own} of {} windows", heard.len() - after);
        let mut late: Vec<f32> = heard[after..].iter().zip(&ratio[after..]).filter(|(h, _)| h.far).map(|(_, r)| *r).collect();
        late.sort_by(f32::total_cmp);
        assert!(late[late.len() / 2] < 0.05, "half the windows keep more than a twentieth: {}", late[late.len() / 2]);
    }

    #[test]
    fn the_person_talking_over_it_stands_clear() {
        let at = 5 * 16_000 / WIN;
        let (_, heard, ratio) = run(6, Some(at));
        assert!(heard[at - 1].sure);
        let over = &heard[at..];
        let clear = over.iter().filter(|h| h.near).count();
        assert!(clear * 10 >= over.len() * 6, "clear in {clear} of {} windows", over.len());
        // (their voice has pauses) and it is left in: most of what came is still there
        assert!(ratio[at..].iter().sum::<f32>() / over.len() as f32 > 0.3);
    }

    #[test]
    fn quiet_passes_through() {
        let (mut echo, mut out) = (Echo::default(), Vec::new());
        let mic: Vec<f32> = (0..WIN).map(|i| (i as f32 * 0.1).sin() * 0.2).collect();
        let h = echo.push(&mic, &[0.0; WIN], &mut out);
        assert!(!h.far && h.near && out == mic);
    }
}
