//! The look kernels on real proxy frames (frames from the edit's three sources, decoded and shrunk): GPU = native =
//! the CPU interpreter of the same K, for every frame (grid, projection), every pair (Hamming) and a shuffled pairing
//! (look_pair). The interpreter runs per frame on its own slice, spread over the cores.
use std::collections::HashMap;
use std::time::Instant;
use raea_kgpu::{Cpu, Gpu, Kernel, look_grid_reference, look_proj_reference, parse};

/// The interpreter on `parts` independent slices of `buf` (each `stride` words, `per` threads), in parallel.
fn interp(k: &Kernel, name: &str, buf: &[u32], stride: usize, per: usize, parts: usize, sc: &HashMap<&str, i32>) -> Result<Vec<u32>, String> {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let chunk = parts.div_ceil(cores);
    let outs: Vec<Result<Vec<u32>, String>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..parts).step_by(chunk).map(|p0| {
            s.spawn(move || {
                let mut out = Vec::new();
                for p in p0..(p0 + chunk).min(parts) {
                    let c = Cpu { buffers: [(name, &buf[p * stride..(p + 1) * stride])].into(), scalars: sc.clone() };
                    out.extend(c.run(k, per, &["v"])?.remove(0));
                }
                Ok(out)
            })
        }).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut v = Vec::new();
    for o in outs { v.extend(o?); }
    Ok(v)
}

fn main() -> Result<(), String> {
    let gpu = Gpu::new()?;
    let fx = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../media/fixtures");
    let n: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(738);
    let (pw, ph) = (144usize, 256usize);
    // the edit's sources in turn: the take, then each b-roll, sequential frames at 30 fps
    let srcs = ["take-h264.mp4", "broll-screen.mov", "broll-rot.mov"];
    let mut s: Vec<u32> = Vec::with_capacity(n * pw * ph);
    let t = Instant::now();
    for (si, src) in srcs.iter().enumerate() {
        let (f0, f1) = (n * si / srcs.len(), n * (si + 1) / srcs.len());
        let mut d = raea_media::backend("ffmpeg")?.open_video(&fx.join(src))?;
        for f in 0..f1 - f0 {
            let fr = d.frame_at(f as f64 / 30.0)?;
            let (w, h) = (fr.width as usize, fr.height as usize);
            for y in 0..ph {
                for x in 0..pw {
                    let i = ((y * h / ph) * w + x * w / pw) * 4;
                    s.push(u32::from_le_bytes([fr.rgba[i], fr.rgba[i + 1], fr.rgba[i + 2], 255]));
                }
            }
        }
    }
    let decode_s = t.elapsed().as_secs_f64();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("kernels");
    let read = |f: &str| std::fs::read_to_string(root.join(f)).map_err(|e| e.to_string());
    let sc: HashMap<&str, i32> = [("pw", pw as i32), ("ph", ph as i32), ("n", n as i32)].into();
    let (kg, kp, kh, kq) = (parse(&read("look_grid.k")?)?, parse(&read("look_proj.k")?)?, parse(&read("look_ham.k")?)?, parse(&read("look_pair.k")?)?);
    let (cg, cp, ch) = (gpu.compile(&kg, &["S"], &sc, &["v"])?, gpu.compile(&kp, &["G"], &sc, &["v"])?, gpu.compile(&kh, &["H"], &sc, &["d"])?);
    let cq = gpu.compile(&kq, &["HO", "HN", "IX"], &HashMap::new(), &["d"])?;
    let sb = gpu.buffer(s.len() * 4, Some(bytemuck::cast_slice(&s)));
    let gb = gpu.buffer(n * 432 * 4, None);
    let hb = gpu.buffer(n * 64 * 4, None);
    let db = gpu.buffer(n * n * 4, None);
    let t = Instant::now();
    gpu.dispatch(&cg, &[&sb], &[], &[&gb], n * 432, false);
    gpu.dispatch(&cp, &[&gb], &[], &[&hb], n * 64, false);
    gpu.dispatch(&ch, &[&hb], &[], &[&db], n * n, true);
    let gpu_ms = t.elapsed().as_secs_f64() * 1e3;
    let g: Vec<i32> = bytemuck::cast_slice(&gpu.read(&gb, n * 432 * 4)).to_vec();
    let hv: Vec<u32> = bytemuck::cast_slice(&gpu.read(&hb, n * 64 * 4)).to_vec();
    let dist: Vec<u32> = bytemuck::cast_slice(&gpu.read(&db, n * n * 4)).to_vec();
    // look_pair: new looks = the looks reversed, each compared with a shuffled old frame (an LCG permutation)
    let hn: Vec<u32> = (0..n).rev().flat_map(|f| hv[f * 64..(f + 1) * 64].to_vec()).collect();
    let mut ix: Vec<u32> = (0..n as u32).collect();
    let mut r = 0x2545F491u64;
    for i in (1..n).rev() {
        r = r.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ix.swap(i, (r >> 33) as usize % (i + 1));
    }
    let (hnb, ixb, pb) = (gpu.buffer(n * 256, Some(bytemuck::cast_slice(&hn))), gpu.buffer(n * 4, Some(bytemuck::cast_slice(&ix))), gpu.buffer(n * 4, None));
    gpu.dispatch(&cq, &[&hb, &hnb, &ixb], &[], &[&pb], n, true);
    let pair: Vec<u32> = bytemuck::cast_slice(&gpu.read(&pb, n * 4)).to_vec();
    // native Rust
    let t = Instant::now();
    let g_ref = look_grid_reference(&s, pw, ph, n);
    let hv_ref = look_proj_reference(&g_ref, n);
    let native_ms = t.elapsed().as_secs_f64() * 1e3;
    let ham = |a: &[u32], b: &[u32]| -> u32 { (0..64).map(|k| (a[k] ^ b[k]).count_ones()).sum() };
    let dist_ref: Vec<u32> = (0..n * n).map(|i| ham(&hv_ref[(i / n) * 64..], &hv_ref[(i % n) * 64..])).collect();
    let pair_ref: Vec<u32> = (0..n).map(|i| ham(&hv_ref[ix[i] as usize * 64..], &hn[i * 64..])).collect();
    // the interpreter, every frame (each frame's slice is frame 0 to the kernel), every pair, every changed frame
    let t = Instant::now();
    let gw: Vec<u32> = g.iter().map(|v| *v as u32).collect();
    let sc1: HashMap<&str, i32> = [("pw", pw as i32), ("ph", ph as i32), ("n", 1)].into();
    let gi = interp(&kg, "S", &s, pw * ph, 432, n, &sc1)?;
    let pi = interp(&kp, "G", &gw, 432, 64, n, &sc1)?;
    let hi = Cpu { buffers: [("H", &hv[..])].into(), scalars: sc.clone() }.run(&kh, n * n, &["d"])?.remove(0);
    let qi = Cpu { buffers: [("HO", &hv[..]), ("HN", &hn[..]), ("IX", &ix[..])].into(), scalars: HashMap::new() }.run(&kq, n, &["d"])?.remove(0);
    let interp_s = t.elapsed().as_secs_f64();
    println!("{}", serde_json::json!({
        "frames": n, "sources": srcs, "decode_s": decode_s,
        "grid_gpu_eq_native": g == g_ref, "proj_gpu_eq_native": hv == hv_ref, "ham_gpu_eq_native": dist == dist_ref, "pair_gpu_eq_native": pair == pair_ref,
        "grid_gpu_eq_interpreter": gi == gw, "proj_gpu_eq_interpreter": pi == hv, "ham_gpu_eq_interpreter": hi == dist, "pair_gpu_eq_interpreter": qi == pair,
        "gpu_ms_grid_proj_ham": gpu_ms, "native_ms_grid_proj": native_ms, "interpreter_s_all": interp_s,
        "bits_set_frac": hv.iter().map(|w| w.count_ones()).sum::<u32>() as f64 / (hv.len() * 32) as f64,
        "mean_distance_bits": dist.iter().sum::<u32>() as f64 / (n * n - n) as f64}));
    Ok(())
}
