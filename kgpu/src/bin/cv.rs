//! The CV columns of a clip's ingest (tabular Track C): `cv INGEST_DIR [--labels T.f32] [--seed N]` reads siglip.f32
//! (n x 768, L2-normalised) and facts.json's shots, runs kernels/cv_*.k on the GPU, and writes INGEST_DIR/cv/: proj
//! (n x 64), gram (n x m, with --labels), span (shots x d), delta (n x d), norm (n), hdc (n x 8192 bits), win (16-row
//! windows every 8 rows, 8192 bits), each beside a JSON header (model, seed, shape, dtype, scale). Each kernel is the
//! interpreter's and native's function bit for bit (cvtest).
use std::collections::HashMap;
use std::path::PathBuf;
use raea_kgpu::{Gpu, parse};

fn kernel(name: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("kernels").join(name)).expect("kernel")
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let dir = PathBuf::from(args.first().ok_or("cv INGEST_DIR [--labels T.f32] [--seed N]")?);
    let seed: u64 = flag("--seed").and_then(|s| s.parse().ok()).unwrap_or(0x5eed_c0_1);
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("siglip.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let (d, fps) = (meta["dim"].as_u64().unwrap_or(768) as usize, meta["fps"].as_f64().unwrap_or(8.0));
    let model = meta["model"].as_str().unwrap_or("siglip2").to_string();
    let f: Vec<f32> = bytemuck::cast_slice::<u8, f32>(&std::fs::read(dir.join("siglip.f32")).map_err(|e| e.to_string())?).to_vec();
    let n = f.len() / d;
    let x = raea_kgpu::cv_quantize(&f, 15);
    let xu: Vec<u32> = x.iter().map(|v| *v as u32).collect();
    // shots as row spans (row = floor(t * fps))
    let facts: serde_json::Value = std::fs::read(dir.join("facts.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let mut bounds: Vec<i32> = facts["shots"].as_array().into_iter().flatten().filter_map(|s| s["start"].as_f64()).map(|t| ((t * fps).floor() as i32).clamp(0, n as i32)).collect();
    bounds.push(n as i32);
    bounds.sort();
    bounds.dedup();
    if bounds.first() != Some(&0) {
        bounds.insert(0, 0);
    }
    let spans = bounds.len() - 1;
    let bu: Vec<u32> = bounds.iter().map(|v| *v as u32).collect();
    let out = dir.join("cv");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let gpu = Gpu::new()?;
    let t0 = std::time::Instant::now();
    let write = |name: &str, data: &[u32], shape: &[usize], dtype: &str, scale: &str| -> Result<(), String> {
        std::fs::write(out.join(format!("{name}.{dtype}")), bytemuck::cast_slice::<u32, u8>(data)).map_err(|e| e.to_string())?;
        let head = serde_json::json!({"model": model, "seed": seed, "shape": shape, "dtype": dtype, "scale": scale, "kernel": format!("kgpu/kernels/cv_{name}.k"), "rows_fps": fps});
        std::fs::write(out.join(format!("{name}.json")), serde_json::to_vec_pretty(&head).unwrap()).map_err(|e| e.to_string())
    };
    let run = |k: &str, inputs: &[&[u32]], names: &[&str], sc: &HashMap<&str, i32>, threads: usize| -> Result<Vec<u32>, String> {
        let c = gpu.compile(&parse(&kernel(k))?, names, sc, &["v"])?;
        Ok(gpu.run(&c, inputs, threads)?.0.remove(0))
    };
    let sc: HashMap<&str, i32> = [("d", d as i32)].into();
    let pm: Vec<u32> = raea_kgpu::cv_quantize(&raea_kgpu::cv_projection_matrix(seed, d, 64), 13).iter().map(|v| *v as u32).collect();
    write("proj", &run("cv_proj.k", &[&xu, &pm], &["X", "P"], &sc, n * 64)?, &[n, 64], "i32", "2^28 (X at 2^15, P = N(0,1/64) at 2^13)")?;
    if let Some(lp) = flag("--labels") {
        let lf: Vec<f32> = bytemuck::cast_slice::<u8, f32>(&std::fs::read(&lp).map_err(|e| e.to_string())?).to_vec();
        let m = lf.len() / d;
        let t: Vec<u32> = raea_kgpu::cv_quantize(&lf, 15).iter().map(|v| *v as u32).collect();
        let sc3: HashMap<&str, i32> = [("d", d as i32), ("m", m as i32)].into();
        write("gram", &run("cv_gram.k", &[&xu, &t], &["X", "T"], &sc3, n * m)?, &[n, m], "i32", "2^30")?;
    }
    write("span", &run("cv_span.k", &[&xu, &bu], &["X", "S"], &sc, spans * d)?, &[spans, d], "i32", "2^15")?;
    let dl = run("cv_delta.k", &[&xu], &["X"], &sc, n * d)?;
    write("delta", &dl, &[n, d], "i32", "2^15")?;
    write("norm", &run("cv_norm.k", &[&dl], &["D"], &sc, n)?, &[n], "i32", "squared norm at 2^16 (deltas shifted 7 bits)")?;
    let hv = run("cv_hdc.k", &[&xu], &["X"], &sc, n * 256)?;
    write("hdc", &hv, &[n, 256], "u32", "8192 bits a row (256 u32 = 128 u64)")?;
    let windows = if n >= 16 { (n - 16) / 8 + 1 } else { 0 };
    write("win", &run("cv_win.k", &[&hv], &["B"], &sc, windows * 256)?, &[windows, 256], "u32", "16-row windows every 8 rows, 8192 bits")?;
    println!("{}", serde_json::json!({"ingest": dir, "rows": n, "spans": spans, "windows": windows, "ms": t0.elapsed().as_secs_f64() * 1e3, "out": out}));
    Ok(())
}
