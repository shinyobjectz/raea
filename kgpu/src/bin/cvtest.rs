//! The CV column kernels (kernels/cv_*.k) on real SigLIP rows: GPU = native = the CPU interpreter of the same K.
//!     cargo run --release --bin cvtest -- ROWS.f32 [dim]
use std::collections::HashMap;
use raea_kgpu::{Cpu, Gpu, parse};

fn k(name: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("kernels").join(name)).expect("kernel")
}

fn main() -> Result<(), String> {
    let path = std::env::args().nth(1).ok_or("cvtest ROWS.f32 [dim]")?;
    let d: usize = std::env::args().nth(2).and_then(|x| x.parse().ok()).unwrap_or(768);
    let f: Vec<f32> = bytemuck::cast_slice::<u8, f32>(&std::fs::read(&path).map_err(|e| e.to_string())?).to_vec();
    let n = f.len() / d;
    let x: Vec<i32> = f.iter().map(|v| (v * 32768.0).round().clamp(-2147483647.0, 2147483647.0) as i32).collect();
    let xu: Vec<u32> = x.iter().map(|v| *v as u32).collect();
    let spans: Vec<i32> = (0..=n).step_by(20).map(|v| v as i32).chain(std::iter::once(n as i32)).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    let spu: Vec<u32> = spans.iter().map(|v| *v as u32).collect();
    let gpu = Gpu::new()?;
    let sc: HashMap<&str, i32> = [("d", d as i32)].into();
    let mut report = serde_json::Map::new();
    let check = |report: &mut serde_json::Map<String, serde_json::Value>, name: &str, got: &[u32], native: &[u32], interp: &[u32]| {
        let gi = got[..interp.len()] == *interp;
        report.insert(name.into(), serde_json::json!({"gpu_eq_native": got == native, "gpu_eq_interpreter": gi, "n": got.len()}));
        got == native && gi
    };
    let mut ok = true;
    // span means
    let kk = parse(&k("cv_span.k"))?;
    let c = gpu.compile(&kk, &["X", "S"], &sc, &["v"])?;
    let got = gpu.run(&c, &[&xu, &spu], (spans.len() - 1) * d)?.0.remove(0);
    let native: Vec<u32> = raea_kgpu::cv_span_reference(&x, d, &spans).iter().map(|v| *v as u32).collect();
    let interp = Cpu { buffers: [("X", &xu[..]), ("S", &spu[..])].into(), scalars: sc.clone() }.run(&kk, (spans.len() - 1) * d, &["v"])?.remove(0);
    ok &= check(&mut report, "span", &got, &native, &interp);
    // deltas and norms
    let kk = parse(&k("cv_delta.k"))?;
    let c = gpu.compile(&kk, &["X"], &sc, &["v"])?;
    let dl = gpu.run(&c, &[&xu], n * d)?.0.remove(0);
    let native: Vec<u32> = raea_kgpu::cv_delta_reference(&x, d, n).iter().map(|v| *v as u32).collect();
    let interp = Cpu { buffers: [("X", &xu[..])].into(), scalars: sc.clone() }.run(&kk, n * d, &["v"])?.remove(0);
    ok &= check(&mut report, "delta", &dl, &native, &interp);
    let kk = parse(&k("cv_norm.k"))?;
    let c = gpu.compile(&kk, &["D"], &sc, &["v"])?;
    let got = gpu.run(&c, &[&dl], n)?.0.remove(0);
    let dli: Vec<i32> = dl.iter().map(|v| *v as i32).collect();
    let native: Vec<u32> = raea_kgpu::cv_norm_reference(&dli, d, n).iter().map(|v| *v as u32).collect();
    let interp = Cpu { buffers: [("D", &dl[..])].into(), scalars: sc.clone() }.run(&kk, n, &["v"])?.remove(0);
    ok &= check(&mut report, "norm", &got, &native, &interp);
    // hypervectors: GPU and native on every row, the interpreter on the first 16 (it's slow: 8192 x d a row)
    let kk = parse(&k("cv_hdc.k"))?;
    let c = gpu.compile(&kk, &["X"], &sc, &["v"])?;
    let t = std::time::Instant::now();
    let hv = gpu.run(&c, &[&xu], n * 256)?.0.remove(0);
    let gpu_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = std::time::Instant::now();
    let native = raea_kgpu::cv_hdc_reference(&x, d, n);
    let native_ms = t.elapsed().as_secs_f64() * 1e3;
    let interp = Cpu { buffers: [("X", &xu[..16 * d])].into(), scalars: sc.clone() }.run(&kk, 16 * 256, &["v"])?.remove(0);
    ok &= check(&mut report, "hdc", &hv, &native, &interp);
    report.insert("hdc_ms".into(), serde_json::json!({"gpu": gpu_ms, "native": native_ms}));
    // windows
    let windows = if n >= 16 { (n - 16) / 8 + 1 } else { 0 };
    let kk = parse(&k("cv_win.k"))?;
    let c = gpu.compile(&kk, &["B"], &sc, &["v"])?;
    let got = gpu.run(&c, &[&hv], windows * 256)?.0.remove(0);
    let native = raea_kgpu::cv_win_reference(&hv, windows);
    let interp = Cpu { buffers: [("B", &hv[..])].into(), scalars: sc.clone() }.run(&kk, windows * 256, &["v"])?.remove(0);
    ok &= check(&mut report, "win", &got, &native, &interp);
    // the projection (seeded N(0,1/64), 2^12) and the label gram (8 rows of the take as stand-in labels), against
    // their exact f32 products for the error the quantization costs
    let pf = raea_kgpu::cv_projection_matrix(0x5eed_c0_1, d, 64);
    let pm = raea_kgpu::cv_quantize(&pf, 13);
    let pmu: Vec<u32> = pm.iter().map(|v| *v as u32).collect();
    let sc2: HashMap<&str, i32> = [("d", d as i32)].into();
    let kk = parse(&k("cv_proj.k"))?;
    let c = gpu.compile(&kk, &["X", "P"], &sc2, &["v"])?;
    let got = gpu.run(&c, &[&xu, &pmu], n * 64)?.0.remove(0);
    let native: Vec<u32> = raea_kgpu::cv_proj_reference(&x, &pm, d, 64, n).iter().map(|v| *v as u32).collect();
    let interp = Cpu { buffers: [("X", &xu[..]), ("P", &pmu[..])].into(), scalars: sc2.clone() }.run(&kk, n * 64, &["v"])?.remove(0);
    ok &= check(&mut report, "proj", &got, &native, &interp);
    let rel = |q: &[u32], scale: f64, exact: &dyn Fn(usize) -> f64| -> f64 {
        let (mut num, mut den) = (0.0f64, 0.0f64);
        for (i, v) in q.iter().enumerate() {
            let e = exact(i);
            num += (*v as i32 as f64 / scale - e).powi(2);
            den += e * e;
        }
        (num / den.max(1e-30)).sqrt()
    };
    let proj_err = rel(&got, (1u64 << 28) as f64, &|i| { let (r, j) = (i / 64, i % 64); (0..d).map(|kk| f[r * d + kk] as f64 * pf[kk * 64 + j] as f64).sum() });
    let m = 8usize;
    let labels: Vec<u32> = (0..m).flat_map(|l| xu[(l * 16) * d..(l * 16 + 1) * d].to_vec()).collect();
    let sc3: HashMap<&str, i32> = [("d", d as i32), ("m", m as i32)].into();
    let kk = parse(&k("cv_gram.k"))?;
    let c = gpu.compile(&kk, &["X", "T"], &sc3, &["v"])?;
    let got = gpu.run(&c, &[&xu, &labels], n * m)?.0.remove(0);
    let li: Vec<i32> = labels.iter().map(|v| *v as i32).collect();
    let native: Vec<u32> = raea_kgpu::cv_gram_reference(&x, &li, d, m, n).iter().map(|v| *v as u32).collect();
    let interp = Cpu { buffers: [("X", &xu[..]), ("T", &labels[..])].into(), scalars: sc3.clone() }.run(&kk, n * m, &["v"])?.remove(0);
    ok &= check(&mut report, "gram", &got, &native, &interp);
    let gram_err = rel(&got, (1u64 << 30) as f64, &|i| { let (r, l) = (i / m, i % m); (0..d).map(|kk| f[r * d + kk] as f64 * f[(l * 16) * d + kk] as f64).sum() });
    report.insert("relative_error".into(), serde_json::json!({"proj": proj_err, "gram": gram_err}));
    report.insert("rows".into(), serde_json::json!(n));
    report.insert("windows".into(), serde_json::json!(windows));
    println!("{}", serde_json::Value::Object(report));
    if ok { Ok(()) } else { Err("a CV kernel differs".into()) }
}
