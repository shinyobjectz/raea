//! K on GPU-resident arrays against native Rust on every core. Arrays go up once and stay on the GPU; each program
//! is timed to completion (GPU: dispatch through the result read back; native: the same work, hand-written, threaded).
//! Also counted: kernels dispatched, to show fusion (a chain of element-wise verbs is one kernel).

use std::time::Instant;

use raea_kgpu::gpu::Gpu;
use raea_kgpu::k::{Cpu, K};

fn threads() -> usize {
    std::thread::available_parallelism().map(|x| x.get()).unwrap_or(8)
}

fn par_map<T: Send, F: Fn(std::ops::Range<usize>) -> T + Sync>(n: usize, f: F) -> Vec<T> {
    let t = threads();
    let chunk = n.div_ceil(t).max(1);
    std::thread::scope(|s| {
        let hs: Vec<_> = (0..t).map(|c| (c * chunk..((c + 1) * chunk).min(n))).filter(|r| !r.is_empty()).map(|r| { let f = &f; s.spawn(move || f(r)) }).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

fn best(runs: usize, mut f: impl FnMut()) -> f64 {
    (0..runs).map(|_| { let t = Instant::now(); f(); t.elapsed().as_secs_f64() }).fold(f64::INFINITY, f64::min) * 1e3
}

fn main() {
    let gpu = Gpu::new().unwrap();
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(16 << 20);
    let mut s = 0x9e37_79b9_7f4a_7c15u64;
    let mut rnd = || { s ^= s << 13; s ^= s >> 7; s ^= s << 17; s };
    let x: Vec<i32> = (0..n).map(|_| (rnd() % 1_000_000) as i32).collect();
    let y: Vec<i32> = (0..n).map(|_| (rnd() % 1_000_000) as i32 - 500_000).collect();
    let mut kg = K::new(&gpu);
    kg.set("x", &x);
    kg.set("y", &y);
    gpu.wait();
    let mut kc = K::new(&Cpu);
    kc.set("x", &x);
    kc.set("y", &y);

    let progs: Vec<(&str, &str, Box<dyn Fn() -> Vec<i32>>)> = vec![
        ("fused map + reduce", "+/x*y+3", Box::new(|| vec![par_map(n, |r| r.fold(0i32, |a, i| a.wrapping_add(x[i].wrapping_mul(y[i].wrapping_add(3))))).into_iter().fold(0i32, |a, b| a.wrapping_add(b))])),
        ("chain, one kernel", "+/1000!(x*x)-y*2", Box::new(|| vec![par_map(n, |r| r.fold(0i32, |a, i| { let v = x[i].wrapping_mul(x[i]).wrapping_sub(y[i].wrapping_mul(2)); a.wrapping_add(if v == 0 { 0 } else { v.wrapping_rem(1000) }) })).into_iter().fold(0i32, |a, b| a.wrapping_add(b))])),
        ("max", "|/x-y", Box::new(|| vec![par_map(n, |r| r.fold(i32::MIN, |a, i| a.max(x[i].wrapping_sub(y[i])))).into_iter().max().unwrap()])),
        ("scan", "*|+\\x", Box::new(|| vec![x.iter().fold(0i32, |a, b| a.wrapping_add(*b))])),
        ("where", "#&x>y", Box::new(|| vec![par_map(n, |r| r.filter(|&i| x[i] > y[i]).count()).into_iter().sum::<usize>() as i32])),
        ("grade (1M)", "*<1048576#x", Box::new(|| { let v = &x[..1048576.min(n)]; let mut idx: Vec<i32> = (0..v.len() as i32).collect(); idx.sort_by(|a, b| v[*a as usize].cmp(&v[*b as usize]).then(a.cmp(b))); vec![idx[0]] })),
    ];
    println!("{{\"adapter\": \"{}\", \"n\": {n}, \"cores\": {}}}", gpu.adapter, threads());
    println!("{:22} {:20} {:>9} {:>11} {:>9} {:>10} {:>7}", "what", "K", "GPU ms", "native ms", "speedup", "kernels", "exact");
    for (what, src, native) in &progs {
        let mut got_g = vec![];
        let k0 = *gpu.dispatches.borrow();
        let _ = kg.run(src).and_then(|v| kg.read(&v)); // warm (pipeline compile)
        let kernels = *gpu.dispatches.borrow() - k0;
        let tg = best(5, || { let v = kg.run(src).unwrap(); got_g = kg.read(&v).unwrap(); });
        let mut got_n = vec![];
        let tn = best(5, || got_n = native());
        let got_c = { let v = kc.run(src).unwrap(); kc.read(&v).unwrap() };
        let exact = got_g == got_c && got_g == got_n;
        println!("{:22} {:20} {:9.2} {:11.2} {:8.1}x {:>10} {:>7}", what, src, tg, tn, tn / tg, kernels, exact);
    }
}
