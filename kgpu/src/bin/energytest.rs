//! The energy scans (kernels/energy_*.k): GPU = native = the CPU interpreter of the same K, on a synthetic curve and on
//! a real column (an ingest's CV delta norms: how much the picture changes row to row).
//!     cargo run --release --bin energytest [-- INGEST_DIR]
use std::collections::HashMap;
use raea_kgpu::{Cpu, Gpu, parse};

fn kernel(name: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("kernels").join(name)).expect("kernel")
}

fn main() -> Result<(), String> {
    let gpu = Gpu::new()?;
    let mut cols: Vec<(String, Vec<i32>)> = vec![("synthetic".into(), (0..600).map(|i| {
        let t = i as f64 / 30.0;
        let v = 0.5 + 0.3 * (t * 0.7).sin() + 0.15 * (t * 3.1).sin();
        (v * 65536.0) as i32 + ((i * 7919) % 1311) as i32
    }).collect())];
    if let Some(dir) = std::env::args().nth(1) {
        let norm: Vec<i32> = bytemuck::cast_slice::<u8, i32>(&std::fs::read(std::path::Path::new(&dir).join("cv/norm.i32")).map_err(|e| e.to_string())?).to_vec();
        let max = norm.iter().copied().max().unwrap_or(1).max(1) as i64;
        cols.push(("ingest delta norms".into(), norm.iter().map(|v| (*v as i64 * 65536 / max) as i32).collect()));
    }
    let mut report = serde_json::Map::new();
    let mut ok = true;
    for (name, c) in &cols {
        let n = c.len();
        let cu: Vec<u32> = c.iter().map(|v| *v as u32).collect();
        let r = 4i64;
        let sc: HashMap<&str, i32> = [("n", n as i32), ("r", r as i32), ("t", 40)].into();
        let three = |k: &str, inputs: &[(&str, &[u32])], threads: usize, native: Vec<i32>| -> Result<(bool, Vec<u32>), String> {
            let kk = parse(&kernel(k))?;
            let names: Vec<&str> = inputs.iter().map(|x| x.0).collect();
            let c = gpu.compile(&kk, &names, &sc, &["v"])?;
            let bufs: Vec<&[u32]> = inputs.iter().map(|x| x.1).collect();
            let got = gpu.run(&c, &bufs, threads)?.0.remove(0);
            let interp = Cpu { buffers: inputs.iter().map(|x| (x.0, x.1)).collect(), scalars: sc.clone() }.run(&kk, threads, &["v"])?.remove(0);
            let nat: Vec<u32> = native.iter().map(|v| *v as u32).collect();
            Ok((got == nat && got == interp, got))
        };
        let (a, smooth) = three("energy_smooth.k", &[("C", &cu)], n, raea_kgpu::energy_smooth_reference(c, r))?;
        let si: Vec<i32> = smooth.iter().map(|v| *v as i32).collect();
        let (b, nodes) = three("energy_nodes.k", &[("S", &smooth)], n, raea_kgpu::energy_nodes_reference(&si))?;
        let (cc, contrast) = three("energy_contrast.k", &[("C", &smooth)], n, raea_kgpu::energy_contrast_reference(&si, r))?;
        let ki: Vec<i32> = contrast.iter().map(|v| *v as i32).collect();
        let (d, flat) = three("energy_flat.k", &[("K", &contrast)], n, raea_kgpu::energy_flat_reference(&ki, 40))?;
        let shifted: Vec<i32> = c.iter().cycle().skip(15).take(n).copied().collect();
        let shu: Vec<u32> = shifted.iter().map(|v| *v as u32).collect();
        let (e, _) = three("energy_cov.k", &[("A", &cu), ("B", &shu)], n * 3, raea_kgpu::energy_cov_reference(c, &shifted, r))?;
        ok &= a && b && cc && d && e;
        let count = |v: &[u32], x: u32| v.iter().filter(|y| **y == x).count();
        report.insert(name.clone(), serde_json::json!({"rows": n, "smooth": a, "nodes": b, "contrast": cc, "flat": d, "cov": e,
            "peaks": count(&nodes, 1), "troughs": count(&nodes, 2), "inflections": count(&nodes, 3), "flat_rows": count(&flat, 1)}));
    }
    println!("{}", serde_json::Value::Object(report));
    if ok { Ok(()) } else { Err("an energy scan differs".into()) }
}
