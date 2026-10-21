//! GPU telemetry for raea's flight recorder (raea_native::trace): when the GPU goes wrong or slow, which part of raea it
//! was. raea has several GPU users, often on one device: the quads compositor (pages, the stage, the chrome), kgpu (K
//! kernels compiled to WGSL compute), rdb's renderer (video frames, 3D, its passes), Bevy's render graph, and video
//! (VideoToolbox frames wrapped as textures). Each labels what it makes with its name first (`quads fine`, `kgpu k`,
//! `rdb blit`, `video nv12`; Bevy's labels are its own), and wgpu names those labels in its errors, so:
//!
//! - `watch(device, owner)`: every error nothing caught, and the device lost, goes into the trace (`gpu_error`,
//!   `gpu_lost`) with its `source`: the subsystem whose label the message names, else Bevy's when one of its pass names
//!   is there, else the device's owner; a message that names no label at all is put to the driver (Metal, wgpu-hal: an
//!   out-of-memory, a command buffer error, a lost device).
//! - `submitted(queue, source, label)` after a submit: when the GPU has done it, its latency (submit to done, as the CPU
//!   sees it: the work and whatever was queued ahead of it) counts in the second's summary (perf.jsonl `gpu`, by
//!   subsystem); over SLOW_MS it's a `gpu_slow` event with its label; still not done after STALL_MS, a `gpu_stall` event
//!   (a hang: the watchdog thread notices it, as the work-done callback never comes).
//! - `scope(device, source, what, f)`: `f` run inside validation and out-of-memory error scopes; what they catch goes in
//!   as `gpu_error` with that source and what was being done (where an uncaptured error can't say).
//!
//! `raea-motion traces show` reads them back grouped by source.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::json;

/// the flight recorder itself, for the crates that report through this one
pub use raea_native::trace;

/// A submission slower than this (ms, submit to done) is a `gpu_slow` event: a backlog. Latency isn't cost: it counts
/// the frames queued ahead (a pipelined stage keeps one in flight) and wgpu calls back when the device is next polled,
/// once a frame (~33 ms in a 30 fps edit whose GPU work is 6-12 ms). A pass's own cost is PassTimer's, timestamps on
/// the GPU.
pub const SLOW_MS: f64 = 100.0;
/// A submission not done after this (ms) is a `gpu_stall`.
pub const STALL_MS: f64 = 2000.0;

/// raea's GPU subsystems, by the first word of their labels.
const PREFIXES: &[(&str, &str)] = &[("quads", "quads"), ("kgpu", "kgpu"), ("k", "kgpu"), ("rdb", "rdb"), ("video", "video"), ("chrome", "chrome"), ("backdrop", "quads"), ("filter blur", "quads"), ("group", "quads"), ("page", "quads"), ("readback", "quads")];
/// words in Bevy's own labels (its passes, pipelines and resources)
const BEVY: &[&str] = &["bevy", "main_opaque", "main_transparent", "main_alpha", "opaque_pass", "transparent_pass", "prepass", "shadow", "tonemapping", "upscaling", "bloom", "fxaa", "smaa", "taa", "mesh", "material", "ui_pass", "ui_", "sprite", "view_", "skybox", "deferred", "ssao", "oit", "post_process", "camera_driver", "pbr"];

/// The subsystem behind a wgpu message: the labels it quotes (`label = 'quads fine'`), matched to the prefixes, then to
/// Bevy's words; the owner when it quotes labels of no one's; "driver" when it quotes none.
pub fn classify(message: &str, owner: &str) -> (String, Vec<String>) {
    let mut labels = Vec::new();
    let mut rest = message;
    while let Some(i) = rest.find("label = '") {
        let after = &rest[i + 9..];
        let Some(j) = after.find('\'') else { break };
        labels.push(after[..j].to_string());
        rest = &after[j..];
    }
    for l in &labels {
        let lower = l.to_lowercase();
        let first = lower.split([' ', ':', '/', '_']).next().unwrap_or("");
        if let Some((_, s)) = PREFIXES.iter().find(|(p, _)| first == *p || lower.starts_with(&format!("{p} "))) {
            return (s.to_string(), labels);
        }
    }
    for l in &labels {
        let lower = l.to_lowercase();
        if BEVY.iter().any(|w| lower.contains(w)) {
            return ("bevy".into(), labels);
        }
    }
    if labels.is_empty() || labels.iter().all(|l| l.is_empty()) {
        return ("driver".into(), labels);
    }
    (owner.to_string(), labels)
}

/// Every error on `device` nothing caught, and its loss, into the trace, classified (and printed as before).
pub fn watch(device: &wgpu::Device, owner: &'static str) {
    device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
        let text = e.to_string();
        let (source, labels) = classify(&text, owner);
        let kind = match e {
            wgpu::Error::OutOfMemory { .. } => "out_of_memory",
            wgpu::Error::Validation { .. } => "validation",
            wgpu::Error::Internal { .. } => "internal",
        };
        eprintln!("gpu error ({source}, {kind}): {text}");
        raea_native::trace::event("gpu_error", json!({"source": source, "type": kind, "labels": labels, "owner": owner, "error": text}));
    }));
    device.set_device_lost_callback(move |reason, message| {
        let (source, labels) = classify(&message, owner);
        eprintln!("gpu device lost ({owner}, {reason:?}): {message}");
        raea_native::trace::event("gpu_lost", json!({"source": source, "labels": labels, "owner": owner, "reason": format!("{reason:?}"), "message": message}));
    });
    // the device, once, for the record
    raea_native::trace::event("gpu_device", json!({"owner": owner, "features": format!("{:?}", device.features()), "limits": {"max_texture_2d": device.limits().max_texture_dimension_2d, "max_buffer": device.limits().max_buffer_size}}));
}

/// `f` inside validation and out-of-memory error scopes: what they catch goes in as `gpu_error` with `source` and
/// `what` (the frame, the kernel, the pass), and comes back as Err for the caller to fall back on.
pub fn scope<R>(device: &wgpu::Device, source: &str, what: &str, f: impl FnOnce() -> R) -> Result<R, String> {
    let oom = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let val = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let r = f();
    let mut err = None;
    for (filter, guard) in [("validation", val), ("out_of_memory", oom)] {
        if let Some(e) = pollster_block(guard.pop()) {
            let text = e.to_string();
            let (_, labels) = classify(&text, source);
            raea_native::trace::event("gpu_error", json!({"source": source, "type": filter, "while": what, "labels": labels, "error": text}));
            err.get_or_insert(format!("{source}: {what}: {text}"));
        }
    }
    match err {
        Some(e) => Err(e),
        None => Ok(r),
    }
}

/// A future polled to its end on this thread (an error scope's pop resolves at once on native backends).
fn pollster_block<F: std::future::Future>(f: F) -> F::Output {
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    fn noop(_: *const ()) {}
    fn clone(p: *const ()) -> RawWaker {
        RawWaker::new(p, &VTABLE)
    }
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
    let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::yield_now();
    }
}

struct Pending {
    source: &'static str,
    label: &'static str,
    at: Instant,
    stalled: bool,
}

fn pending() -> &'static Mutex<HashMap<u64, Pending>> {
    static P: OnceLock<Mutex<HashMap<u64, Pending>>> = OnceLock::new();
    P.get_or_init(|| {
        // the watchdog: a submission not done after STALL_MS is a stall (its work-done callback isn't coming)
        let _ = std::thread::Builder::new().name("raea gpu watchdog".into()).spawn(|| loop {
            std::thread::sleep(Duration::from_millis(250));
            let mut p = pending().lock().unwrap();
            for (id, s) in p.iter_mut() {
                let ms = s.at.elapsed().as_secs_f64() * 1e3;
                if !s.stalled && ms > STALL_MS {
                    s.stalled = true;
                    eprintln!("gpu stall ({}): '{}' not done after {ms:.0} ms", s.source, s.label);
                    raea_native::trace::event("gpu_stall", json!({"source": s.source, "label": s.label, "ms": ms.round(), "submission": id}));
                }
            }
        });
        Mutex::new(HashMap::new())
    })
}

/// A submission of `source`'s work (`label`) just made on `queue`: its latency, slowness or stall into the trace.
pub fn submitted(queue: &wgpu::Queue, source: &'static str, label: &'static str) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    pending().lock().unwrap().insert(id, Pending { source, label, at: Instant::now(), stalled: false });
    queue.on_submitted_work_done(move || {
        let Some(s) = pending().lock().unwrap().remove(&id) else { return };
        let ms = s.at.elapsed().as_secs_f64() * 1e3;
        raea_native::trace::gpu(s.source, ms);
        if ms > SLOW_MS {
            raea_native::trace::event("gpu_slow", json!({"source": s.source, "label": s.label, "ms": (ms * 100.0).round() / 100.0, "stalled_first": s.stalled}));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::classify;

    #[test]
    fn errors_are_put_to_their_subsystem() {
        assert_eq!(classify("Validation Error: In a CommandEncoder, label = 'quads frame': texture too small", "bevy").0, "quads");
        assert_eq!(classify("In ComputePipeline, label = 'kgpu k': shader error", "quads").0, "kgpu");
        assert_eq!(classify("In a RenderPass, label = 'main_opaque_pass_3d'", "quads").0, "bevy");
        assert_eq!(classify("In a RenderPass, label = 'rdb video'", "quads").0, "rdb");
        assert_eq!(classify("Out of memory", "quads").0, "driver");
        assert_eq!(classify("In a Buffer, label = 'something'", "kgpu").0, "kgpu");
    }
}

/// GPU time per pass, measured on the GPU: each compute pass of a frame gets begin and end timestamps (`writes`), they're
/// resolved into a buffer at the end of the frame (`resolve`), and read back a frame later without waiting (`collect`),
/// so the frame's passes, by name, go to the trace (`trace::gpu_passes`: the second's summary, and `gpu_busy` when the
/// frame's GPU work outlasts the budget). None where the device has no timestamp queries.
pub struct PassTimer {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    cap: u32,
    names: Vec<&'static str>,
    /// the frame being read back: its pass names, and whether its map is done
    reading: Option<(Vec<&'static str>, Arc<std::sync::atomic::AtomicBool>)>,
    /// kept between frames (ZeroHeap): the names list not in use, and the read-back times
    spare: Vec<&'static str>,
    passes: Vec<(&'static str, f64)>,
    period: f32,
    source: &'static str,
    pub budget_ms: f64,
}

impl PassTimer {
    /// Room for `passes` passes a frame, for `source`'s frames (`quads`, `kgpu`, ...).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, source: &'static str, passes: u32) -> Option<PassTimer> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) || std::env::var("RAEA_GPU_TIMING").as_deref() == Ok("0") {
            return None;
        }
        let cap = passes * 2;
        let set = device.create_query_set(&wgpu::QuerySetDescriptor { label: Some(&format!("{source} pass timer")), ty: wgpu::QueryType::Timestamp, count: cap });
        let size = cap as u64 * 8;
        let resolve = device.create_buffer(&wgpu::BufferDescriptor { label: Some(&format!("{source} pass timer resolve")), size, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let read = device.create_buffer(&wgpu::BufferDescriptor { label: Some(&format!("{source} pass timer read")), size, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        Some(PassTimer { set, resolve, read, cap, names: Vec::new(), reading: None, spare: Vec::new(), passes: Vec::new(), period: queue.get_timestamp_period(), source, budget_ms: 1000.0 / 120.0 })
    }

    /// A frame begins: the last frame's times reported if they're in (else that frame goes unreported). False while a
    /// readback is still pending: this frame isn't timed.
    pub fn begin(&mut self) -> bool {
        self.names.clear();
        if let Some((names, done)) = &self.reading {
            if !done.load(Ordering::Acquire) {
                return false;
            }
            {
                let data = self.read.slice(..names.len() as u64 * 16).get_mapped_range();
                let ts: &[u64] = bytemuck_cast(&data);
                self.passes.clear();
                self.passes.extend(names.iter().enumerate().map(|(i, n)| (*n, ts[2 * i + 1].saturating_sub(ts[2 * i]) as f64 * self.period as f64 / 1e6)));
                raea_native::trace::gpu_passes(self.source, &self.passes, self.budget_ms);
            }
            self.read.unmap();
            if let Some((mut names, _)) = self.reading.take() {
                names.clear();
                self.spare = names;
            }
        }
        true
    }

    /// The timestamp writes for a pass called `name` (None when the frame isn't timed or is full).
    pub fn writes(&mut self, timed: bool, name: &'static str) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        let i = self.names.len() as u32;
        if !timed || 2 * i + 2 > self.cap {
            return None;
        }
        self.names.push(name);
        Some(wgpu::ComputePassTimestampWrites { query_set: &self.set, beginning_of_pass_write_index: Some(2 * i), end_of_pass_write_index: Some(2 * i + 1) })
    }

    /// The frame's timestamps resolved and copied out, at the end of its encoder.
    pub fn resolve(&mut self, timed: bool, e: &mut wgpu::CommandEncoder) {
        let n = self.names.len() as u32;
        if !timed || n == 0 {
            return;
        }
        e.resolve_query_set(&self.set, 0..2 * n, &self.resolve, 0);
        e.copy_buffer_to_buffer(&self.resolve, 0, &self.read, 0, n as u64 * 16);
    }

    /// After the frame's submit: its readback started (reported at a later `begin`).
    pub fn submitted(&mut self, timed: bool) {
        if !timed || self.names.is_empty() {
            return;
        }
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let d = done.clone();
        self.read.slice(..self.names.len() as u64 * 16).map_async(wgpu::MapMode::Read, move |r| {
            if r.is_ok() {
                d.store(true, Ordering::Release);
            }
        });
        let spare = std::mem::take(&mut self.spare);
        self.reading = Some((std::mem::replace(&mut self.names, spare), done));
    }
}

/// Mapped bytes as u64s (the timestamps: 8-byte aligned in a mapped buffer).
fn bytemuck_cast(b: &[u8]) -> &[u64] {
    unsafe { std::slice::from_raw_parts(b.as_ptr() as *const u64, b.len() / 8) }
}
