//! The K-on-GPU bench: three kernels from the render path, each written once in the K dialect. For each: the GPU
//! against the CPU interpreter of the same text (bit-exact), against hand-written native Rust on every core (speed and
//! bit-exact), with the GPU's time split into upload, compute and readback.

use std::collections::HashMap;
use std::time::Instant;

use raea_kgpu::{Cpu, Gpu, parse};

const YUV: &str = "
/ BT.709 limited range in 8.8 fixed point: one RGBA word per pixel (w x h, chroma planes cw wide)
x:i!w
yy:i%w
c:298*(byte[Y;(yy*w)+x])-16
u:(byte[U;((yy%2)*cw)+x%2])-128
v:(byte[V;((yy%2)*cw)+x%2])-128
r:clamp[shr[c+(459*v)+128;8];0;255]
g:clamp[shr[c-(55*u)+(136*v)-128;8];0;255]
b:clamp[shr[c+(541*u)+128;8];0;255]
px:pack[r;g;b;255]
";

const HAMMING: &str = "
/ every pair of n hypervectors of ww words: the Hamming distance (K's over: +/ pop xor)
a:i%n
b:i!n
d:sum[k;ww;pop[xor[word[H;(a*ww)+k];word[H;(b*ww)+k]]]]
";

const COMPOSITE: &str = "
/ a clip over the frame: cover-cropped from the source (crop cx0,cy0 cw x ch of an sw-wide source) by an integer
/ gather, blended at alpha al/256, one RGBA word per output pixel (W x H)
x:i!W
y:i%W
s:word[S;((cy0+((y*ch)%H))*sw)+cx0+(x*cw)%W]
d:word[D;i]
r:shr[(al*and[s;255])+(256-al)*and[d;255];8]
g:shr[(al*and[shr[s;8];255])+(256-al)*and[shr[d;8];255];8]
b:shr[(al*and[shr[s;16];255])+(256-al)*and[shr[d;16];255];8]
px:pack[r;g;b;255]
";

fn words(b: &[u8]) -> Vec<u32> {
    let mut v = b.to_vec();
    v.resize(b.len().div_ceil(4) * 4, 0);
    bytemuck::cast_slice::<u8, u32>(&v).to_vec()
}

fn threads<F: Fn(usize, &mut [u32]) + Sync>(out: &mut [u32], f: F) {
    let n = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(8);
    let chunk = out.len().div_ceil(n);
    std::thread::scope(|s| {
        for (c, part) in out.chunks_mut(chunk).enumerate() {
            let f = &f;
            s.spawn(move || f(c * chunk, part));
        }
    });
}

fn best<T>(runs: usize, mut f: impl FnMut() -> T) -> (T, f64) {
    let mut out = None;
    let mut t = f64::INFINITY;
    for _ in 0..runs {
        let t0 = Instant::now();
        let r = f();
        t = t.min(t0.elapsed().as_secs_f64());
        out = Some(r);
    }
    (out.unwrap(), t)
}

fn main() -> Result<(), String> {
    let gpu = Gpu::new()?;
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../media/fixtures");
    let ff = raea_media::backend("ffmpeg")?;
    let mut report = serde_json::json!({"adapter": gpu.adapter, "cores": std::thread::available_parallelism().map(|x| x.get()).unwrap_or(0)});

    // 1. YUV -> RGBA on a real frame of the take
    {
        let fr = ff.open_video(&fixtures.join("take-h264.mp4"))?.frame_at(5.0)?;
        let (w, h) = (fr.width as usize, fr.height as usize);
        let (y, u, v) = raea_media::yuv::rgba_to_i420(&fr.rgba, w, h);
        let (yw, uw, vw) = (words(&y), words(&u), words(&v));
        let k = parse(YUV)?;
        let sc: HashMap<&str, i32> = [("w", w as i32), ("cw", (w / 2) as i32)].into();
        let c = gpu.compile(&k, &["Y", "U", "V"], &sc, &["px"])?;
        let n = w * h;
        let _ = gpu.run(&c, &[&yw, &uw, &vw], n)?; // warm
        let ((g, t), gpu_s) = best(5, || gpu.run(&c, &[&yw, &uw, &vw], n).unwrap());
        let cpu = Cpu { buffers: [("Y", &yw[..]), ("U", &uw[..]), ("V", &vw[..])].into(), scalars: sc.clone() };
        let t0 = Instant::now();
        let interp = cpu.run(&k, n, &["px"])?;
        let interp_s = t0.elapsed().as_secs_f64();
        let (native, native_s) = best(5, || {
            let mut out = vec![0u32; n];
            threads(&mut out, |start, part| {
                for (j, o) in part.iter_mut().enumerate() {
                    let i = start + j;
                    let (x, yy) = (i % w, i / w);
                    let c = 298 * (y[yy * w + x] as i32 - 16);
                    let uu = u[(yy / 2) * (w / 2) + x / 2] as i32 - 128;
                    let vv = v[(yy / 2) * (w / 2) + x / 2] as i32 - 128;
                    let r = ((c + 459 * vv + 128) >> 8).clamp(0, 255) as u32;
                    let gg = ((c - 55 * uu - 136 * vv + 128) >> 8).clamp(0, 255) as u32;
                    let b = ((c + 541 * uu + 128) >> 8).clamp(0, 255) as u32;
                    *o = r | gg << 8 | b << 16 | 255 << 24;
                }
            });
            out
        });
        if let Some(j) = (0..n).find(|&j| g[0][j] != interp[0][j]) {
            eprintln!("yuv first diff at {j}: gpu {:08x} interp {:08x} native {:08x}", g[0][j], interp[0][j], native[j]);
        }
        // against today's float converter: how far the fixed-point kernel is from it
        let mut float = Vec::new();
        raea_media::yuv::i420_to_rgba(&y, &u, &v, w, w / 2, w / 2, w, h, &mut float);
        let gb: &[u8] = bytemuck::cast_slice(&g[0]);
        let maxdiff = gb.iter().zip(&float).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
        report["yuv_to_rgba"] = serde_json::json!({
            "pixels": n, "gpu_eq_interpreter": g[0] == interp[0], "gpu_eq_native": g[0] == native,
            "max_diff_vs_float_converter": maxdiff,
            "gpu_ms": t.json(), "gpu_total_ms": gpu_s * 1e3, "native_all_cores_ms": native_s * 1e3, "interpreter_ms": interp_s * 1e3,
        });
    }

    // 2. HDC: all pairs of 738 hypervectors of 2048 bits
    {
        let (n, ww) = (738usize, 64usize);
        let mut s = 0x1234_5678u64;
        let hv: Vec<u32> = (0..n * ww)
            .map(|_| {
                s = s.wrapping_add(0x9e37_79b9_7f4a_7c15);
                let mut z = s;
                z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
                (z ^ (z >> 31)) as u32
            })
            .collect();
        let k = parse(HAMMING)?;
        let sc: HashMap<&str, i32> = [("n", n as i32), ("ww", ww as i32)].into();
        let c = gpu.compile(&k, &["H"], &sc, &["d"])?;
        let _ = gpu.run(&c, &[&hv], n * n)?;
        let ((g, t), gpu_s) = best(5, || gpu.run(&c, &[&hv], n * n).unwrap());
        let cpu = Cpu { buffers: [("H", &hv[..])].into(), scalars: sc.clone() };
        let t0 = Instant::now();
        let interp = cpu.run(&k, n * n, &["d"])?;
        let interp_s = t0.elapsed().as_secs_f64();
        let (native, native_s) = best(5, || {
            let mut out = vec![0u32; n * n];
            threads(&mut out, |start, part| {
                for (j, o) in part.iter_mut().enumerate() {
                    let (a, b) = ((start + j) / n, (start + j) % n);
                    *o = (0..ww).map(|k| (hv[a * ww + k] ^ hv[b * ww + k]).count_ones()).sum();
                }
            });
            out
        });
        if let Some(j) = (0..n * n).find(|&j| g[0][j] != interp[0][j]) {
            eprintln!("hdc first diff at {j}: gpu {} interp {} native {}", g[0][j], interp[0][j], native[j]);
        }
        report["hdc_all_pairs"] = serde_json::json!({
            "pairs": n * n, "bits": ww * 32, "gpu_eq_interpreter": g[0] == interp[0], "gpu_eq_native": g[0] == native,
            "gpu_ms": t.json(), "gpu_total_ms": gpu_s * 1e3, "native_all_cores_ms": native_s * 1e3, "interpreter_ms": interp_s * 1e3,
        });
    }

    // 3. A clip composited at full size: the landscape b-roll cover-cropped into the portrait frame over the take
    {
        let bg = ff.open_video(&fixtures.join("take-h264.mp4"))?.frame_at(5.0)?;
        let src = ff.open_video(&fixtures.join("broll-wing.mov"))?.frame_at(2.0)?;
        let (w, h) = (bg.width as usize, bg.height as usize);
        let (sw, sh) = (src.width as usize, src.height as usize);
        // cover: the largest centred rect of the source with the frame's aspect, zoomed 1.2
        let (cw0, ch0) = if sw * h > sh * w { (sh * w / h, sh) } else { (sw, sw * h / w) };
        let (cw, ch) = (cw0 * 5 / 6, ch0 * 5 / 6);
        let (cx0, cy0) = ((sw - cw) / 2, (sh - ch) / 2);
        let (sw_, dw) = (words(&src.rgba), words(&bg.rgba));
        let k = parse(COMPOSITE)?;
        let sc: HashMap<&str, i32> = [("W", w as i32), ("H", h as i32), ("sw", sw as i32), ("cw", cw as i32), ("ch", ch as i32), ("cx0", cx0 as i32), ("cy0", cy0 as i32), ("al", 230)].into();
        let c = gpu.compile(&k, &["S", "D"], &sc, &["px"])?;
        let n = w * h;
        let _ = gpu.run(&c, &[&sw_, &dw], n)?;
        let ((g, t), gpu_s) = best(5, || gpu.run(&c, &[&sw_, &dw], n).unwrap());
        let cpu = Cpu { buffers: [("S", &sw_[..]), ("D", &dw[..])].into(), scalars: sc.clone() };
        let t0 = Instant::now();
        let interp = cpu.run(&k, n, &["px"])?;
        let interp_s = t0.elapsed().as_secs_f64();
        let (native, native_s) = best(5, || {
            let mut out = vec![0u32; n];
            threads(&mut out, |start, part| {
                for (j, o) in part.iter_mut().enumerate() {
                    let i = start + j;
                    let (x, y) = (i % w, i / w);
                    let s = sw_[(cy0 + y * ch / h) * sw + cx0 + x * cw / w];
                    let d = dw[i];
                    let mix = |sh: u32| ((230 * ((s >> sh) & 255) + 26 * ((d >> sh) & 255)) >> 8) & 255;
                    *o = mix(0) | mix(8) << 8 | mix(16) << 16 | 255 << 24;
                }
            });
            out
        });
        let bytes: Vec<u8> = bytemuck::cast_slice(&g[0]).to_vec();
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("composite.png");
        let _ = raea_media::backend("ffmpeg"); // (keeps the import for builds without image)
        std::fs::write(out.with_extension("rgba"), &bytes).map_err(|e| e.to_string())?;
        report["composite"] = serde_json::json!({
            "pixels": n, "source": [sw, sh], "gpu_eq_interpreter": g[0] == interp[0], "gpu_eq_native": g[0] == native,
            "gpu_ms": t.json(), "gpu_total_ms": gpu_s * 1e3, "native_all_cores_ms": native_s * 1e3, "interpreter_ms": interp_s * 1e3,
        });
    }
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    Ok(())
}
