//! Tone's Convolver on the core: the sound convolved with an impulse response, with no latency (as the Web Audio
//! ConvolverNode). The response's first block is convolved directly, sample by sample; the rest by uniformly
//! partitioned FFT convolution (overlap-save), each block's tail worked out at the block's start from the blocks
//! already heard. Blocks sit on the timeline's grid, so a play and a render are the same samples.

/// The partition (and direct head) length in samples.
pub const B: usize = 512;
const N: usize = 2 * B;

#[derive(Clone, Copy, Default)]
struct C {
    re: f32,
    im: f32,
}

/// A radix-2 FFT of size N.
struct Fft {
    tw: Vec<C>,
    rev: Vec<usize>,
}

impl Fft {
    fn new() -> Fft {
        let bits = N.trailing_zeros();
        let rev = (0..N).map(|i| i.reverse_bits() >> (usize::BITS - bits)).collect();
        let tw = (0..N / 2).map(|k| {
            let a = -std::f64::consts::TAU * k as f64 / N as f64;
            C { re: a.cos() as f32, im: a.sin() as f32 }
        }).collect();
        Fft { tw, rev }
    }
    fn run(&self, x: &mut [C], inverse: bool) {
        for i in 0..N {
            let j = self.rev[i];
            if i < j {
                x.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= N {
            let step = N / len;
            for s in (0..N).step_by(len) {
                for k in 0..len / 2 {
                    let w = self.tw[k * step];
                    let w = if inverse { C { re: w.re, im: -w.im } } else { w };
                    let a = x[s + k];
                    let b = x[s + k + len / 2];
                    let bw = C { re: b.re * w.re - b.im * w.im, im: b.re * w.im + b.im * w.re };
                    x[s + k] = C { re: a.re + bw.re, im: a.im + bw.im };
                    x[s + k + len / 2] = C { re: a.re - bw.re, im: a.im - bw.im };
                }
            }
            len *= 2;
        }
        if inverse {
            let k = 1.0 / N as f32;
            for v in x.iter_mut() {
                v.re *= k;
                v.im *= k;
            }
        }
    }
}

/// One channel's convolution.
pub struct Conv {
    head: Vec<f32>,
    /// the partitions after the head, as spectra
    parts: Vec<Vec<C>>,
    /// the last B input samples, newest at `at - 1`
    hist: Vec<f32>,
    at: usize,
    /// the block being heard and the one before it
    cur: Vec<f32>,
    prev: Vec<f32>,
    /// the input spectra of past blocks, newest first
    spectra: std::collections::VecDeque<Vec<C>>,
    /// this block's tail, sample by sample
    tail: Vec<f32>,
    pos: usize,
    fft: std::sync::Arc<Fft>,
}

impl Conv {
    fn new(ir: &[f32], fft: std::sync::Arc<Fft>) -> Conv {
        let head = ir[..ir.len().min(B)].to_vec();
        let mut parts = Vec::new();
        let mut j = B;
        while j < ir.len() {
            let mut x = vec![C::default(); N];
            for (i, v) in ir[j..(j + B).min(ir.len())].iter().enumerate() {
                x[i].re = *v;
            }
            fft.run(&mut x, false);
            parts.push(x);
            j += B;
        }
        Conv { head, parts, hist: vec![0.0; B], at: 0, cur: vec![0.0; B], prev: vec![0.0; B], spectra: Default::default(), tail: vec![0.0; B], pos: 0, fft }
    }

    /// At a block's start: the block just heard becomes a spectrum, and this block's tail is the sum over the partitions
    /// of the spectra of the blocks before it.
    fn block(&mut self) {
        let mut x = vec![C::default(); N];
        for i in 0..B {
            x[i].re = self.prev[i];
            x[B + i].re = self.cur[i];
        }
        self.fft.run(&mut x, false);
        self.spectra.push_front(x);
        self.spectra.truncate(self.parts.len().max(1));
        std::mem::swap(&mut self.prev, &mut self.cur);
        let mut acc = vec![C::default(); N];
        for (h, s) in self.parts.iter().zip(self.spectra.iter()) {
            for k in 0..N {
                let (a, b) = (s[k], h[k]);
                acc[k].re += a.re * b.re - a.im * b.im;
                acc[k].im += a.re * b.im + a.im * b.re;
            }
        }
        if !self.parts.is_empty() {
            self.fft.run(&mut acc, true);
        }
        for i in 0..B {
            self.tail[i] = acc[B + i].re;
        }
        self.pos = 0;
    }

    /// One sample; `start`: this sample opens a block on the timeline's grid.
    pub fn run(&mut self, x: f32, start: bool) -> f32 {
        if start && self.pos != 0 {
            // a block cut short (never on the grid, kept safe): finish it with silence
            while self.pos < B {
                self.cur[self.pos] = 0.0;
                self.pos += 1;
            }
        }
        if self.pos >= B || start {
            self.block();
        }
        self.hist[self.at] = x;
        self.at = (self.at + 1) % B;
        let mut y = 0f32;
        for (i, h) in self.head.iter().enumerate() {
            y += h * self.hist[(self.at + B - 1 - i) % B];
        }
        self.cur[self.pos] = x;
        y += self.tail[self.pos];
        self.pos += 1;
        y
    }
}

/// Both channels' convolutions from a response (mono: the same response on both), normalized as the Web Audio
/// ConvolverNode normalizes (`normalize`).
pub fn pair(ir: &raea_media::Audio, rate: f64, normalize: bool) -> [Conv; 2] {
    let a = if (ir.rate as f64 - rate).abs() > 0.5 { raea_media::soft_resample(ir, rate as u32) } else { ir.clone() };
    let l = a.channels.first().cloned().unwrap_or_default();
    let r = a.channels.get(1).cloned().unwrap_or_else(|| l.clone());
    let mut scale = 1f32;
    if normalize {
        // the Web Audio spec's calibration: 1 / RMS power over every channel, times 0.00125, at the response's rate
        let n = (l.len() + r.len()).max(1) as f64;
        let p = ((l.iter().chain(r.iter()).map(|v| (*v as f64).powi(2)).sum::<f64>()) / n).sqrt().max(0.000125);
        scale = (1.0 / p * 0.00125 * 44100.0 / ir.rate.max(1) as f64) as f32;
    }
    let l: Vec<f32> = l.iter().map(|v| v * scale).collect();
    let r: Vec<f32> = r.iter().map(|v| v * scale).collect();
    let fft = std::sync::Arc::new(Fft::new());
    [Conv::new(&l, fft.clone()), Conv::new(&r, fft)]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// the partitioned convolution equals the direct sum
    #[test]
    fn equals_direct() {
        let ir: Vec<f32> = (0..1700).map(|i| (i as f32 * 0.37).sin() * (-(i as f32) / 400.0).exp()).collect();
        let x: Vec<f32> = (0..4000).map(|i| ((i * 7919) % 101) as f32 / 50.0 - 1.0).collect();
        let mut c = Conv::new(&ir, std::sync::Arc::new(Fft::new()));
        let mut worst = 0f32;
        for n in 0..x.len() {
            let y = c.run(x[n], n % B == 0);
            let d: f32 = (0..ir.len().min(n + 1)).map(|i| ir[i] * x[n - i]).sum();
            worst = worst.max((y - d).abs());
        }
        assert!(worst < 1e-3, "worst {worst}");
    }
}
