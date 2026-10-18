//! J2b's overlay segmenter (the roadmap J2b): which pixels of a rendered frame are the edit's overlays (titles, captions,
//! boxes, stickers) and which the footage under them. The patch model (tabular/gbm layers.rs) ranks well on real edits
//! but stops at F 0.36-0.65 even at the best cut: one patch's pixel statistics can't tell a caption from footage that
//! looks like one. This sees shape and context: a small U-Net over each frame at 1/8 size (5 channels: colour, the change
//! since the frame before and since 8 frames before), trained in Rust (burn, on the Mac's GPU through Metal) on rdb's
//! generated render pairs, where every pair is a perfect mask (the edit rendered again with its overlays at opacity 0).
//! Scored the patch model's way on the real edits: a 48 px patch is overlay when over half its pixels differ by more than
//! 40 levels, the network's score for it the mean over its cells.
//!
//!   raea-overlay-seg [--epochs 12] GEN_DIR REAL_FULL REAL_BARE ...    train, keep the best epoch, score; saved to
//!                                                                     $RAEA_MODELS/overlay-seg/ (overlay-seg.bpk, cut.json)
//!   raea-overlay-seg --load DIR GEN_DIR REAL_FULL REAL_BARE ...         score saved weights again, no training

use burn::module::Module;
use burn::nn::conv::{Conv2d, Conv2dConfig};
use burn::nn::interpolate::{Interpolate2d, Interpolate2dConfig};
use burn::nn::PaddingConfig2d;
use burn::grad_clipping::GradientClippingConfig;
use burn::optim::{AdamConfig, GradientsParams};
use burn::tensor::activation::{relu, sigmoid, softplus};
use burn::tensor::{Device, Tensor, TensorData};
use serde_json::json;
use std::path::{Path, PathBuf};

/// Cell size (full-resolution pixels) and the network's grid: 1080x1920 frames as 135x240 cells, padded to 136 wide.
const CELL: usize = 8;
const W: usize = 136;
const H: usize = 240;
const CH: usize = 5;

/// One frame's input (CH x H x W, u8) and its cells' overlay share (H x W, 0-255).
struct Sample {
    x: Vec<u8>,
    share: Vec<u8>,
}

/// A render pair's frames (every `step`th from the 9th, as the patch model), cached beside the render.
fn samples(full: &str, bare: &str, step: usize) -> Result<Vec<Sample>, String> {
    let cache = PathBuf::from(format!("{full}.seg-{step}.bin"));
    let per = CH * H * W + H * W;
    if let Ok(b) = std::fs::read(&cache) {
        if b.len() % per == 0 {
            return Ok(b.chunks(per).map(|c| Sample { x: c[..CH * H * W].to_vec(), share: c[CH * H * W..].to_vec() }).collect());
        }
    }
    let be = raea_media::backend("apple")?;
    let (mut a, mut b) = (be.open_video(Path::new(full))?, be.open_video(Path::new(bare))?);
    let info = a.info().clone();
    let n = (info.duration * info.fps).round() as usize;
    let mut history: Vec<Vec<f32>> = Vec::new();
    let mut out = Vec::new();
    for f in 0..n {
        let t = (f as f64 + 0.5) / info.fps;
        let fa = a.frame_at(t)?;
        let (fw, fh) = (fa.width as usize, fa.height as usize);
        // cells: mean colour and grey
        let mut rgb = vec![0f32; 3 * H * W];
        let mut grey = vec![0f32; H * W];
        let gfull = |px: &[u8], i: usize| 0.299 * px[i] as f32 + 0.587 * px[i + 1] as f32 + 0.114 * px[i + 2] as f32;
        for cy in 0..H {
            for cx in 0..W {
                let (mut s, mut k) = ([0f32; 3], 0f32);
                for yy in 0..CELL {
                    for xx in 0..CELL {
                        let (x, y) = (cx * CELL + xx, cy * CELL + yy);
                        if x < fw && y < fh {
                            let i = (y * fw + x) * 4;
                            for c in 0..3 {
                                s[c] += fa.rgba[i + c] as f32;
                            }
                            k += 1.0;
                        }
                    }
                }
                let k = k.max(1.0);
                for c in 0..3 {
                    rgb[c * H * W + cy * W + cx] = s[c] / k;
                }
                grey[cy * W + cx] = 0.299 * s[0] / k + 0.587 * s[1] / k + 0.114 * s[2] / k;
            }
        }
        history.push(grey.clone());
        if history.len() > 9 {
            history.remove(0);
        }
        if f % step != 0 || history.len() < 9 {
            continue;
        }
        let fb = b.frame_at(t)?;
        let mut share = vec![0u8; H * W];
        for cy in 0..H {
            for cx in 0..W {
                let (mut d, mut k) = (0usize, 0usize);
                for yy in 0..CELL {
                    for xx in 0..CELL {
                        let (x, y) = (cx * CELL + xx, cy * CELL + yy);
                        if x < fw && y < fh {
                            let i = (y * fw + x) * 4;
                            d += ((gfull(&fa.rgba, i) - gfull(&fb.rgba, i)).abs() > 40.0) as usize;
                            k += 1;
                        }
                    }
                }
                share[cy * W + cx] = if k == 0 { 0 } else { (d * 255 / k) as u8 };
            }
        }
        let mut x = Vec::with_capacity(CH * H * W);
        x.extend(rgb.iter().map(|v| *v as u8));
        let (prev, back8) = (&history[history.len() - 2], &history[0]);
        x.extend((0..H * W).map(|i| ((grey[i] - prev[i]).abs() * 2.0).min(255.0) as u8));
        x.extend((0..H * W).map(|i| ((grey[i] - back8[i]).abs() * 2.0).min(255.0) as u8));
        out.push(Sample { x, share });
    }
    let mut bytes = Vec::with_capacity(out.len() * per);
    for s in &out {
        bytes.extend_from_slice(&s.x);
        bytes.extend_from_slice(&s.share);
    }
    let _ = std::fs::write(&cache, bytes);
    Ok(out)
}

/// Two 3x3 convolutions with ReLU.
#[derive(Module, Debug)]
struct Block {
    a: Conv2d,
    b: Conv2d,
}

impl Block {
    fn new(cin: usize, cout: usize, stride: usize, dev: &Device) -> Self {
        let c = |i, o, s| Conv2dConfig::new([i, o], [3, 3]).with_stride([s, s]).with_padding(PaddingConfig2d::Explicit(1, 1, 1, 1)).init(dev);
        Block { a: c(cin, cout, stride), b: c(cout, cout, 1) }
    }
    fn forward(&self, x: Tensor<4>) -> Tensor<4> {
        relu(self.b.forward(relu(self.a.forward(x))))
    }
}

/// A small U-Net: 16, 32, 64 and 64 channels at 1, 1/2, 1/4 and 1/8 of the cell grid, back up with skips.
#[derive(Module, Debug)]
struct Net {
    e1: Block,
    e2: Block,
    e3: Block,
    e4: Block,
    d3: Block,
    d2: Block,
    d1: Block,
    head: Conv2d,
    up: Interpolate2d,
}

impl Net {
    fn new(dev: &Device) -> Self {
        Net {
            e1: Block::new(CH, 16, 1, dev),
            e2: Block::new(16, 32, 2, dev),
            e3: Block::new(32, 64, 2, dev),
            e4: Block::new(64, 64, 2, dev),
            d3: Block::new(128, 64, 1, dev),
            d2: Block::new(96, 32, 1, dev),
            d1: Block::new(48, 16, 1, dev),
            head: Conv2dConfig::new([16, 1], [1, 1]).init(dev),
            up: Interpolate2dConfig::new().with_scale_factor(Some([2.0, 2.0])).init(),
        }
    }
    /// Logits, one per cell.
    fn forward(&self, x: Tensor<4>) -> Tensor<4> {
        let x1 = self.e1.forward(x);
        let x2 = self.e2.forward(x1.clone());
        let x3 = self.e3.forward(x2.clone());
        let x4 = self.e4.forward(x3.clone());
        let y3 = self.d3.forward(Tensor::cat(vec![self.up.forward(x4), x3], 1));
        let y2 = self.d2.forward(Tensor::cat(vec![self.up.forward(y3), x2], 1));
        let y1 = self.d1.forward(Tensor::cat(vec![self.up.forward(y2), x1], 1));
        self.head.forward(y1)
    }
}

fn batch(s: &[&Sample], dev: &Device) -> (Tensor<4>, Tensor<4>) {
    let x: Vec<f32> = s.iter().flat_map(|q| q.x.iter().map(|v| *v as f32 / 255.0)).collect();
    let y: Vec<f32> = s.iter().flat_map(|q| q.share.iter().map(|v| (*v > 127) as u8 as f32)).collect();
    (Tensor::from_data(TensorData::new(x, [s.len(), CH, H, W]), dev), Tensor::from_data(TensorData::new(y, [s.len(), 1, H, W]), dev))
}

/// Cell probabilities for every sample, in batches.
fn predict(net: &Net, s: &[Sample], dev: &Device) -> Vec<Vec<f32>> {
    let mut out = Vec::new();
    for ch in s.chunks(16) {
        let refs: Vec<&Sample> = ch.iter().collect();
        let (x, _) = batch(&refs, dev);
        let p: Vec<f32> = sigmoid(net.forward(x)).into_data().try_to_vec::<f32>().unwrap_or_default();
        out.extend(p.chunks(H * W).map(|c| c.to_vec()));
    }
    out
}

/// The patch model's unit: 6x6 cells (48 px), overlay when over half its pixels differ; the score the cells' mean.
fn patches(s: &[Sample], p: &[Vec<f32>]) -> (Vec<f64>, Vec<f64>) {
    let (mut sc, mut y) = (Vec::new(), Vec::new());
    for (q, pp) in s.iter().zip(p) {
        for py in 0..H / 6 {
            for px in 0..132 / 6 {
                let (mut a, mut b) = (0f64, 0f64);
                for yy in 0..6 {
                    for xx in 0..6 {
                        let i = (py * 6 + yy) * W + px * 6 + xx;
                        a += pp[i] as f64;
                        b += q.share[i] as f64 / 255.0;
                    }
                }
                sc.push(a / 36.0);
                y.push((b / 36.0 > 0.5) as u8 as f64);
            }
        }
    }
    (sc, y)
}

fn auc(y: &[f64], p: &[f64]) -> f64 {
    let mut idx: Vec<usize> = (0..y.len()).collect();
    idx.sort_by(|&a, &b| p[a].total_cmp(&p[b]));
    let (mut rank_sum, mut pos) = (0f64, 0f64);
    for (r, &i) in idx.iter().enumerate() {
        if y[i] > 0.5 {
            rank_sum += (r + 1) as f64;
            pos += 1.0;
        }
    }
    let neg = y.len() as f64 - pos;
    if pos == 0.0 || neg == 0.0 { 0.5 } else { (rank_sum - pos * (pos + 1.0) / 2.0) / (pos * neg) }
}

fn f_at(th: f64, p: &[f64], y: &[f64]) -> (f64, f64, f64) {
    let (mut tp, mut fp, mut fnn) = (0f64, 0f64, 0f64);
    for k in 0..y.len() {
        let (pr, tr) = (p[k] > th, y[k] > 0.5);
        tp += (pr && tr) as u8 as f64;
        fp += (pr && !tr) as u8 as f64;
        fnn += (!pr && tr) as u8 as f64;
    }
    let (prec, rec) = (tp / (tp + fp).max(1.0), tp / (tp + fnn).max(1.0));
    (2.0 * prec * rec / (prec + rec).max(1e-9), prec, rec)
}

fn best_cut(p: &[f64], y: &[f64]) -> f64 {
    (1..100).map(|k| k as f64 / 100.0).max_by(|a, b| f_at(*a, p, y).0.total_cmp(&f_at(*b, p, y).0)).unwrap_or(0.5)
}

fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let epochs = args.iter().position(|a| a == "--epochs").and_then(|i| args.get(i + 1)?.parse().ok()).unwrap_or(12usize);
    let load = args.iter().position(|a| a == "--load").and_then(|i| args.get(i + 1)).cloned();
    let rest: Vec<&String> = args.iter().enumerate().filter(|(i, a)| !a.starts_with("--") && (*i == 0 || (args[i - 1] != "--epochs" && args[i - 1] != "--load"))).map(|(_, a)| a).collect();
    let Some(gen) = rest.first() else {
        eprintln!("raea-overlay-seg [--epochs 12] GEN_DIR REAL_FULL REAL_BARE ...");
        std::process::exit(2);
    };
    let t0 = std::time::Instant::now();
    // generated pairs: one in ten held out (by pair) to choose the cut and check the fit
    let mut pairs: Vec<(String, String)> = std::fs::read_dir(gen.as_str()).map(|rd| rd.flatten().map(|e| e.path().display().to_string()).filter(|p| p.ends_with("-bare.mp4")).map(|b| (b.replace("-bare.mp4", ".mp4"), b)).collect()).unwrap_or_default();
    pairs.sort();
    let (mut train, mut held) = (Vec::new(), Vec::new());
    for (i, (f, b)) in pairs.iter().enumerate() {
        match samples(f, b, 2) {
            Ok(s) if i % 10 == 0 => held.extend(s),
            Ok(s) => train.extend(s),
            Err(e) => eprintln!("{f}: {e}"),
        }
    }
    eprintln!("{} training frames, {} held out, {:.0} s", train.len(), held.len(), t0.elapsed().as_secs_f64());
    let models = PathBuf::from(std::env::var("RAEA_MODELS").unwrap_or_else(|_| format!("{}/.local/share/raea-models", std::env::var("HOME").unwrap_or_default()))).join("overlay-seg");
    let dev = Device::default().autodiff();
    let mut net = Net::new(&dev);
    // clipped, and a gentler rate than 2e-3 (which diverged at the 5th epoch to a constant: loss 0.85 from 0.30)
    let mut opt = AdamConfig::new().with_grad_clipping(Some(GradientClippingConfig::Norm(1.0))).init();
    // overlays are a few percent of cells: weigh them up by the inverse share (at most 20x)
    let pos = train.iter().map(|s| s.share.iter().filter(|v| **v > 127).count()).sum::<usize>() as f64 / (train.len() * H * W).max(1) as f64;
    let w = (1.0 / pos.max(1e-3) - 1.0).clamp(1.0, 20.0);
    let mut best: Option<(f64, usize, Net)> = None;
    if let Some(dir) = &load {
        best = Some((0.0, 0, Net::new(&Device::default()).load_file(Path::new(dir).join("overlay-seg.bpk"))));
    }
    let epochs = if load.is_some() { 0 } else { epochs };
    let mut order: Vec<usize> = (0..train.len()).collect();
    let mut seed = 7u64;
    for ep in 0..epochs {
        for i in (1..order.len()).rev() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            order.swap(i, (seed >> 33) as usize % (i + 1));
        }
        let lr = 1e-3 * (1.0 - ep as f64 / epochs as f64).max(0.1);
        let mut total = 0f64;
        for ch in order.chunks(16) {
            let refs: Vec<&Sample> = ch.iter().map(|&i| &train[i]).collect();
            let (x, y) = batch(&refs, &dev);
            let z = net.forward(x);
            // weighted binary cross-entropy on logits: w*y*softplus(-z) + (1-y)*softplus(z)
            let loss = (y.clone().mul_scalar(w).mul(softplus(z.clone().neg(), 1.0)) + y.neg().add_scalar(1.0).mul(softplus(z, 1.0))).mean();
            total += loss.clone().into_scalar::<f32>() as f64;
            let grads = GradientsParams::from_grads(loss.backward(), &net);
            net = opt.step(lr, net, grads);
        }
        // the held-out generated pairs each epoch: the best epoch is the one kept
        let (hp, hy) = patches(&held, &predict(&net.clone().valid(), &held, &Device::default()));
        let a = auc(&hy, &hp);
        eprintln!("epoch {ep}: loss {:.4}, held-out AUC {a:.3} ({:.0} s)", total / (order.len() as f64 / 16.0).ceil(), t0.elapsed().as_secs_f64());
        if best.as_ref().is_none_or(|b: &(f64, usize, Net)| a > b.0) {
            best = Some((a, ep, net.clone().valid()));
        }
    }
    let (_, best_epoch, net) = best.expect("at least one epoch");
    let dev_eval = Device::default();
    let (hp, hy) = patches(&held, &predict(&net, &held, &dev_eval));
    let th = best_cut(&hp, &hy);
    let (f, prec, rec) = f_at(th, &hp, &hy);
    if load.is_none() {
        let _ = std::fs::create_dir_all(&models);
        if let Err(e) = net.clone().save_file(models.join("overlay-seg.bpk")) {
            eprintln!("not saved: {e:?}");
        }
        let _ = std::fs::write(models.join("cut.json"), json!({"threshold": th, "best_epoch": best_epoch, "grid": [W, H], "cell": CELL, "channels": CH}).to_string());
    }
    let mut real = serde_json::Map::new();
    for pair in rest[1..].chunks(2) {
        if pair.len() < 2 {
            continue;
        }
        let s = match samples(pair[0], pair[1], 2) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: {e}", pair[0]);
                continue;
            }
        };
        let (p, y) = patches(&s, &predict(&net, &s, &dev_eval));
        let (f, pr, rc) = f_at(th, &p, &y);
        let b = best_cut(&p, &y);
        let (bf, bp, br) = f_at(b, &p, &y);
        real.insert(Path::new(pair[0]).file_name().unwrap().to_string_lossy().into(), serde_json::json!({
            "patches": y.len(), "overlay_patches": y.iter().filter(|v| **v > 0.5).count(), "auc": r3(auc(&y, &p)),
            "f": r3(f), "precision": r3(pr), "recall": r3(rc), "best": {"threshold": b, "f": r3(bf), "precision": r3(bp), "recall": r3(br)}}));
    }
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "epochs": epochs, "best_epoch": best_epoch, "training_frames": train.len(), "overlay_share": r3(pos), "positive_weight": r3(w), "seconds": t0.elapsed().as_secs(),
        "held_out_generated": {"frames": held.len(), "auc": r3(auc(&hy, &hp)), "threshold": th, "f": r3(f), "precision": r3(prec), "recall": r3(rec)},
        "real_renders": real})).unwrap());
}
