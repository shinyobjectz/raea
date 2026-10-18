//! B1's check: the look on the GPU (kgpu look_grid.k + look_proj.k) and on the CPU (previz::look_words) are one
//! function. Every frame of an edit is sketched on the GPU, its look computed there, the sketch read back, and the CPU
//! look computed from the same pixels; the words must be equal.
//!
//!     cargo run --release --example look_parity -- examples/ugc/ugc.sql
use rdb::gpuview::GpuView;

fn main() -> Result<(), String> {
    let path = std::env::args().nth(1).unwrap_or("examples/ugc/ugc.sql".into());
    let (prog, rows) = rdb::open(&path)?;
    let dir = rdb::dir_of(&path);
    let mut v = GpuView::new(&prog, &rows, &dir)?;
    let frames: Vec<i64> = (0..prog.frames).collect();
    let (sketch, _) = v.sketch(&frames)?;
    let n = frames.len();
    let (hv, _) = v.looks(&sketch, n)?;
    let (pw, ph) = (v.pw as usize, v.ph as usize);
    let gpu: Vec<u32> = bytemuck::cast_slice(&v.gpu.read(&hv, n * 64 * 4)).to_vec();
    let px: Vec<u32> = bytemuck::cast_slice(&v.gpu.read(&sketch, n * pw * ph * 4)).to_vec();
    let mut equal = 0;
    let mut worst = 0u32;
    for f in 0..n {
        let base = f * pw * ph;
        let cpu = rdb::previz::look_words(pw, ph, |x, y| {
            let w = px[base + y * pw + x];
            ((w & 255) as i32, (w >> 8 & 255) as i32, (w >> 16 & 255) as i32)
        });
        let g = &gpu[f * 64..f * 64 + 64];
        let diff: u32 = cpu.iter().zip(g).map(|(a, b)| (a ^ b).count_ones()).sum();
        if diff == 0 {
            equal += 1;
        }
        worst = worst.max(diff);
    }
    println!("{}", serde_json::json!({"edit": path, "frames": n, "equal": equal, "worst_bits": worst, "pw": pw, "ph": ph}));
    if equal == n { Ok(()) } else { Err(format!("{} of {n} frames differ", n - equal)) }
}
