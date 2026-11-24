//! raea: RAEA_SOLARI_TIMES=1, the GPU time of each of Solari's stages (docs/raytracing.feature RT8), to see where a
//! real-time frame's tracing goes. node.rs runs its work as one compute pass a stage, and each pass writes timestamps
//! as it begins and ends (Metal has none inside a pass). A trailing one-workgroup timed pass follows them, as on Metal a
//! pass's end write read 0 unless another timed pass came after it, and begin-only times weren't monotonic. A stage
//! that doesn't run (spatial and shade, without ReSTIR) counts 0. The times are read back a few frames late and
//! printed to stderr as each stage's mean every 120 frames.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Solari's stages, in node.rs's order
pub const STAGES: [&str; 4] = ["light tiles", "world cache", "initial path", "spatial and shade"];
/// each stage's pass's beginning and end, then the trailing pass's
const N: u32 = 2 * STAGES.len() as u32 + 2;
const EVERY: u32 = 120;

struct Times {
    set: &'static wgpu::QuerySet,
    noop: wgpu::ComputePipeline,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    /// 0 free to copy into, 1 copied (mapped on the next frame, once the copy is submitted), 2 mapping, 3 mapped
    state: Arc<AtomicU8>,
    sums: [f64; STAGES.len()],
    frames: u32,
}

static TIMES: Mutex<Option<Times>> = Mutex::new(None);

pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("RAEA_SOLARI_TIMES").as_deref() == Ok("1"))
}

/// The query set the stages' passes write into, made on first use; None when off or the device has no timestamps.
pub fn set(device: &wgpu::Device) -> Option<&'static wgpu::QuerySet> {
    if !on() {
        return None;
    }
    let mut t = TIMES.lock().unwrap();
    if t.is_none() {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            static SAID: AtomicBool = AtomicBool::new(false);
            if !SAID.swap(true, Ordering::Relaxed) {
                eprintln!("raea: RAEA_SOLARI_TIMES: this device has no timestamp queries");
            }
            return None;
        }
        let set = Box::leak(Box::new(device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("solari times"), ty: wgpu::QueryType::Timestamp, count: N })));
        let buffer = |usage| device.create_buffer(&wgpu::BufferDescriptor { label: Some("solari times"), size: N as u64 * 8, usage, mapped_at_creation: false });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("solari times"), source: wgpu::ShaderSource::Wgsl("@compute @workgroup_size(1) fn main() {}".into()) });
        let noop = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("solari times"), layout: None, module: &module, entry_point: Some("main"), compilation_options: Default::default(), cache: None });
        *t = Some(Times {
            set,
            noop,
            resolve: buffer(wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC),
            read: buffer(wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST),
            state: Arc::new(AtomicU8::new(0)),
            sums: [0.0; STAGES.len()],
            frames: 0,
        });
    }
    t.as_ref().map(|t| t.set)
}

/// Stage `stage`'s pass's timestamp: its beginning.
pub fn writes(set: Option<&'static wgpu::QuerySet>, stage: u32) -> Option<wgpu::ComputePassTimestampWrites<'static>> {
    set.map(|query_set| wgpu::ComputePassTimestampWrites { query_set, beginning_of_pass_write_index: Some(2 * stage), end_of_pass_write_index: Some(2 * stage + 1) })
}

/// After the stages' passes: the trailing pass, the queries resolved and copied out when the readback is free, last
/// frame's copy mapped, and a mapped one folded into the means. `period` is the queue's nanoseconds a tick.
pub fn after(device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, period: f32) {
    let mut g = TIMES.lock().unwrap();
    let Some(t) = g.as_mut() else { return };
    // nothing else polls the device often enough for a map's callback to run (it stayed mapping); this doesn't wait
    let _ = device.poll(wgpu::PollType::Poll);
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("solari times"),
            timestamp_writes: Some(wgpu::ComputePassTimestampWrites { query_set: t.set, beginning_of_pass_write_index: Some(N - 2), end_of_pass_write_index: Some(N - 1) }),
        });
        pass.set_pipeline(&t.noop);
        pass.dispatch_workgroups(1, 1, 1);
    }
    match t.state.load(Ordering::Acquire) {
        3 => {
            let ticks: Option<Vec<u64>> = t.read.slice(..).get_mapped_range().ok().map(|v| v.chunks_exact(8).map(|b| u64::from_le_bytes(b.try_into().unwrap())).collect());
            t.read.unmap();
            t.state.store(0, Ordering::Release);
            let Some(ticks) = ticks else { return };
            let bounds: Vec<u64> = ticks[..2 * STAGES.len()].to_vec();
            // a stage that didn't run this frame (no ReSTIR: no spatial pass) writes nothing and counts 0
            if bounds.chunks_exact(2).all(|w| w[1] >= w[0]) {
                for (i, w) in bounds.chunks_exact(2).enumerate() {
                    t.sums[i] += if w[0] > 0 { (w[1] - w[0]) as f64 * period as f64 / 1e6 } else { 0.0 };
                }
                t.frames += 1;
                if t.frames == EVERY {
                    let ms: Vec<String> = STAGES.iter().zip(t.sums).map(|(s, v)| format!("{s} {:.2}", v / EVERY as f64)).collect();
                    eprintln!("raea: Solari GPU ms, mean of {EVERY} frames: {}; all {:.2}", ms.join(", "), t.sums.iter().sum::<f64>() / EVERY as f64);
                    t.sums = [0.0; STAGES.len()];
                    t.frames = 0;
                }
            }
        }
        1 => {
            t.state.store(2, Ordering::Release);
            let state = t.state.clone();
            t.read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                if let Err(e) = &r {
                    eprintln!("raea: RAEA_SOLARI_TIMES: the readback didn't map: {e}");
                }
                state.store(if r.is_ok() { 3 } else { 0 }, Ordering::Release)
            });
        }
        _ => {}
    }
    encoder.resolve_query_set(t.set, 0..N, &t.resolve, 0);
    if t.state.load(Ordering::Acquire) == 0 {
        encoder.copy_buffer_to_buffer(&t.resolve, 0, &t.read, 0, N as u64 * 8);
        t.state.store(1, Ordering::Release);
    }
}
