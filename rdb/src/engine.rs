//! The runtime: one long-lived Bevy app whose World mirrors the edit's tables. 3D kinds render with Bevy; 2D kinds
//! are raea rows drawn by Blitz (real CSS) in the same World. `load` re-mirrors the tables after a move without
//! restarting the app; `frame` makes one frame: the inserted rows, then keys, then every rule (the timeline),
//! written onto the entities, then rendered. Offline rendering has no pipelining, and a readback is only taken when
//! its tag row carries the frame, so a frame is a function of the tables in any order.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_resource::{TextureFormat, TextureUsages};
use bevy::window::ExitCondition;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::storage::{GpuShaderBuffer, ShaderBuffer};
use bevy::render::texture::GpuImage;

use crate::eval::{self, Tup};
use crate::store::{self, RowKey};
use crate::ir::{Program, V};

mod scene3d;

/// What a load spawned besides the rows (overlay, tag), despawned by the next load; the rows go through the store.
#[derive(Component)]
struct Mirror;

#[derive(Component)]
struct FrameTag;

/// The loaded edit: its program, its base as columns (gathered from the store once per load: row r is the store's
/// r-th row) and its media rows (for resolving sources).
#[derive(Resource, Clone)]
struct Edit {
    prog: Program,
    /// every frame, evaluated once per load from the base gathered out of the store (row r is the store's r-th row)
    timeline: std::sync::Arc<crate::timeline::Timeline>,
    media: Vec<Tup>,
}

/// The frame's values: the base after the keys, rules and transitions at the clock, by row; and each row's entity.
#[derive(Resource, Default)]
struct FrameRows {
    cols: eval::Cols,
    ix: HashMap<Entity, usize>,
}

impl FrameRows {
    /// A row's values this frame.
    fn vals(&self, e: Entity) -> Option<BTreeMap<String, V>> {
        self.ix.get(&e).map(|&r| self.cols.tup(r).vals)
    }
}

#[derive(Resource, Default)]
struct Clock(i64);

#[derive(Resource)]
struct Overlay(Handle<Image>);

/// Readbacks as they arrive (several can land in one update once frames are pipelined); each carries its tag.
#[derive(Resource, Default)]
struct Out(Vec<Vec<u8>>);

/// Which load the World is on: the tag carries it, so a readback in flight from before a move is never taken.
#[derive(Resource, Default, Clone, Copy)]
struct Gen(u8);

#[derive(Resource, Default)]
struct Error(Option<String>);

/// A clip row's picture: a texture the decoded frame is written into, under the Blitz overlay.
#[derive(Component)]
struct Clip {
    image: Handle<Image>,
    /// (source, slot, source time in microseconds) last written, so an update that asks again does no work
    last: Option<(String, usize, i64)>,
}

/// The clips' decoders, one per (source, slot): a slot per clip of the same source showing at once. Decoders answer
/// "the frame showing at t" by as-of lookup, so a frame doesn't depend on which frames were asked for before it.
struct Media {
    backend: Box<dyn raea_media::Backend>,
    decoders: HashMap<(String, usize), Box<dyn raea_media::VideoDecoder>>,
    dir: PathBuf,
}

/// The overlay's state digest: every 2D row's values and whether it shows. A 2D row's pixels are a function of
/// these, so when the digest hasn't changed the Blitz overlay (style, layout, raster, upload) is reused as it is.
#[derive(Resource, Default)]
struct OverlayKey {
    last: Option<u64>,
    dirty: bool,
    /// overlay redraws and reuses since the load (for the bench)
    drawn: u64,
    reused: u64,
}

fn overlay_key(clock: Res<Clock>, generation: Res<Gen>, frame: Res<FrameRows>, mut key: ResMut<OverlayKey>, q: Query<(Entity, &RowKey), With<Base2d>>) {
    use std::hash::{Hash, Hasher};
    let mut rows: Vec<(&RowKey, BTreeMap<String, V>)> = q.iter().filter_map(|(e, k)| frame.vals(e).map(|v| (k, v))).collect();
    rows.sort_by(|a, b| (&a.0.kind, &a.0.id).cmp(&(&b.0.kind, &b.0.id)));
    let mut h = std::collections::hash_map::DefaultHasher::new();
    generation.0.hash(&mut h);
    for (row, v) in &rows {
        let v = v;
        let on = clock.0 >= num(v, "from", f32::MIN) as i64 && v.get("to").map(|t| clock.0 < t.num() as i64).unwrap_or(true);
        (&row.kind, &row.id, on).hash(&mut h);
        if on {
            format!("{v:?}").hash(&mut h);
        }
    }
    let d = h.finish();
    key.dirty = key.last != Some(d) || std::env::var_os("RAEA_NO_MEMO").is_some();
    key.last = Some(d);
    if key.dirty { key.drawn += 1 } else { key.reused += 1 }
}

fn overlay_dirty(key: Res<OverlayKey>) -> bool {
    key.dirty
}

/// The NV12 readback (G8): after the cameras have drawn, a K kernel (raea-kgpu/kernels/nv12.k) reads the target
/// through a Unorm view and writes NV12 plus the frame tag into `out`, which is what comes back to the CPU (3.1 MB a
/// frame at 1080x1920, not 8.4 MB of padded RGBA).
#[derive(Resource, Clone, ExtractResource)]
struct Nv12Job {
    target: Handle<Image>,
    out: Handle<ShaderBuffer>,
    w: u32,
    h: u32,
}

/// The compiled kernel, made once in the render world.
#[derive(Resource, Default)]
struct Nv12Gpu(Option<wgpu::ComputePipeline>);

const NV12_K: &str = include_str!("../../kgpu/kernels/nv12.k");

fn nv12_words(w: u32, h: u32) -> usize {
    (w * h / 4 + w * h / 8 + 1) as usize
}

fn nv12_pass(world: &mut World) {
    let Some(job) = world.get_resource::<Nv12Job>().cloned() else { return };
    let device = world.resource::<bevy::render::renderer::RenderDevice>().wgpu_device().clone();
    if world.resource::<Nv12Gpu>().0.is_none() {
        let k = raea_kgpu::parse(NV12_K).expect("nv12.k parses");
        let sc: std::collections::HashMap<&str, i32> = [("w", job.w as i32), ("h", job.h as i32), ("T_w", job.w as i32)].into();
        let src = raea_kgpu::wgsl_tex(&k, &[], &["T"], &sc, &["px"]).expect("nv12.k compiles");
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("nv12.k"), source: wgpu::ShaderSource::Wgsl(src.into()) });
        let p = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("nv12.k"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        world.resource_mut::<Nv12Gpu>().0 = Some(p);
    }
    let (Some(img), Some(out)) = (world.resource::<RenderAssets<GpuImage>>().get(&job.target), world.resource::<RenderAssets<GpuShaderBuffer>>().get(&job.out)) else {
        return;
    };
    let view = img.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(wgpu::TextureFormat::Rgba8Unorm), ..Default::default() });
    let n = nv12_words(job.w, job.h) as u32;
    let groups = n.div_ceil(256);
    let (gx, gy) = (groups.min(65535), groups.div_ceil(groups.min(65535)));
    let shape = wgpu::util::DeviceExt::create_buffer_init(&device, &wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[n.to_le_bytes(), (gx * 256).to_le_bytes()].concat(),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let pipeline = world.resource::<Nv12Gpu>().0.clone().unwrap();
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 1, resource: out.buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: shape.as_entire_binding() },
        ],
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("rdb engine") });
    {
        let mut pass = enc.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(gx, gy, 1);
    }
    // an encoder, not a finished buffer: pending encoders are submitted after the cameras' work
    world.resource_mut::<bevy::render::renderer::PendingCommandBuffers>().push_encoder(enc);
}

/// The GPU frame path (G8): clips are NV12 planes in a GPU pool, composited by kernels/composite.k over the camera's
/// target with the Blitz overlay, then kernels/nv12buf.k makes the NV12 readback. Built fresh each update by the
/// main world: what to draw, what to upload, what to evict, and the overlay when it changed.
type PoolKey = (String, i64);

#[derive(Resource, Clone, Default, ExtractResource)]
struct GpuFrame {
    on: bool,
    slots: Vec<(PoolKey, raea_kgpu::Slot)>,
    uploads: Vec<(PoolKey, std::sync::Arc<Vec<u8>>)>,
    evict: Vec<PoolKey>,
    overlay: Option<std::sync::Arc<Vec<u8>>>,
    /// kgpu's mask M for the masked clips (their slots' on = 2), when any clip has a mask
    mask: Option<std::sync::Arc<Vec<u32>>>,
}

/// The main world's book of what the GPU pool holds (bytes, last frame used), so uploads happen once per source
/// frame and the least recently used go when the budget (1 GB) is passed.
#[derive(Resource, Default)]
struct PoolBook {
    resident: HashMap<PoolKey, (usize, u64)>,
    bytes: usize,
    tick: u64,
    uploads: u64,
    hits: u64,
}

/// The GPU frame pool's budget: RAEA_POOL_MB, else 32 MiB for a render that walks its frames in order (it reuses
/// almost nothing: the UGC edit uploads 929 frames at every budget), else 1 GiB on the Mac and 256 MiB on phones (iOS,
/// Android) for scrubbing, where it sits beside the app. Shared with gpuview.
pub fn pool_budget() -> usize {
    static ENV: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    if let Some(mb) = *ENV.get_or_init(|| std::env::var("RAEA_POOL_MB").ok().and_then(|v| v.parse::<usize>().ok())) {
        return mb << 20;
    }
    if LINEAR_RENDER.load(std::sync::atomic::Ordering::Relaxed) {
        return 32 << 20;
    }
    (if cfg!(any(target_os = "ios", target_os = "android")) { 256 } else { 1024 }) << 20
}

/// Set by a render that walks its frames in order (rdb::render): the pool's budget drops to 32 MiB.
pub static LINEAR_RENDER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The render world's half: the pooled planes and the kernels.
#[derive(Resource, Default)]
struct PoolGpu {
    /// RAEA_GPU_TIMES (with RAEA_SERIAL): timestamps per frame, [2,3] composite.k, [4,5] nv12buf.k, [6,7] the
    /// overlay's upload, [8..16] clip uploads (each a staging copy); resolved into the first buffer, read via the second
    ts: Option<(wgpu::QuerySet, wgpu::Buffer, wgpu::Buffer)>,
    ts_uploads: u32,
    ts_overlay: bool,
    ts_pending: bool,
    /// ms summed: clip uploads, overlay upload, composite, nv12; and frames timed
    pub ts_ms: [f64; 4],
    pub ts_frames: u64,
    /// said once that timestamps are unsupported
    ts_unsupported_said: bool,
    bufs: HashMap<PoolKey, wgpu::Buffer>,
    overlay: Option<wgpu::Buffer>,
    frame: Option<wgpu::Buffer>,
    /// M when no clip is masked: zeros (composite.k reads it for every pixel)
    no_mask: Option<wgpu::Buffer>,
    dummy: Option<wgpu::Buffer>,
    comp: Option<wgpu::ComputePipeline>,
    nv12: Option<wgpu::ComputePipeline>,
}

const COMPOSITE_K: &str = include_str!("../../kgpu/kernels/composite.k");
const NV12BUF_K: &str = include_str!("../../kgpu/kernels/nv12buf.k");

fn compile_k(device: &wgpu::Device, src: &str, buffers: &[&str], w: u32, h: u32) -> wgpu::ComputePipeline {
    let k = raea_kgpu::parse(src).expect("kernel parses");
    let sc: std::collections::HashMap<&str, i32> = [("w", w as i32), ("h", h as i32), ("T_w", w as i32)].into();
    let wgsl = raea_kgpu::wgsl_tex(&k, buffers, &["T"], &sc, &["px"]).expect("kernel compiles");
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("k"), source: wgpu::ShaderSource::Wgsl(wgsl.into()) });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("k"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

fn storage(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: size.div_ceil(4) * 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

fn shape(device: &wgpu::Device, n: u32) -> (wgpu::Buffer, u32, u32) {
    let groups = n.div_ceil(256);
    let gx = groups.min(65535);
    let gy = groups.div_ceil(gx);
    let b = wgpu::util::DeviceExt::create_buffer_init(device, &wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[n.to_le_bytes(), (gx * 256).to_le_bytes()].concat(),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    (b, gx, gy)
}

/// Upload, evict, composite, convert: one encoder after the cameras.
fn gpu_frame_pass(world: &mut World) {
    let Some(job) = world.get_resource::<Nv12Job>().cloned() else { return };
    let Some(frame) = world.get_resource::<GpuFrame>().cloned() else { return };
    if !frame.on {
        return;
    }
    let device = world.resource::<bevy::render::renderer::RenderDevice>().wgpu_device().clone();
    let queue = world.resource::<bevy::render::renderer::RenderQueue>().0.clone();
    let (w, h) = (job.w, job.h);
    {
        let mut pool = world.resource_mut::<PoolGpu>();
        if pool.comp.is_none() {
            pool.comp = Some(compile_k(&device, COMPOSITE_K, &["P0", "P1", "P2", "P3", "O", "C", "M"], w, h));
            pool.no_mask = Some(wgpu::util::DeviceExt::create_buffer_init(&device, &wgpu::util::BufferInitDescriptor { label: None, contents: &vec![0u8; (w * h * 4) as usize], usage: wgpu::BufferUsages::STORAGE }));
            pool.nv12 = Some(compile_k(&device, NV12BUF_K, &["F"], w, h));
            pool.overlay = Some(storage(&device, (w * h * 4) as u64));
            pool.frame = Some(storage(&device, (w * h * 4) as u64));
            pool.dummy = Some(storage(&device, 24));
        }
        let want = std::env::var_os("RAEA_GPU_TIMES").is_some();
        let timing = want && device.features().contains(wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);
        if want && !timing && !pool.ts_unsupported_said {
            pool.ts_unsupported_said = true;
            eprintln!("RAEA_GPU_TIMES: this device has no timestamp queries inside encoders: GPU stage times unsupported");
        }
        if timing && pool.ts.is_none() {
            let q = device.create_query_set(&wgpu::QuerySetDescriptor { label: None, ty: wgpu::QueryType::Timestamp, count: 16 });
            let r = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 128, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
            let m = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 128, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            pool.ts = Some((q, r, m));
        }
        pool.ts_uploads = 0;
        pool.ts_overlay = false;
        for k in &frame.evict {
            pool.bufs.remove(k);
        }
        // timed, an upload is a staging buffer and a copy between two timestamps (what write_buffer does underneath)
        let upload = |pool: &mut PoolGpu, dst: &wgpu::Buffer, data: &[u8], pair: Option<u32>| {
            let mut d = data.to_vec();
            // NV12 is w*h*1.5 bytes: pad to a word for write_buffer
            d.resize(d.len().div_ceil(4) * 4, 0);
            match (pool.ts.as_ref(), pair) {
                (Some((q, _, _)), Some(i)) => {
                    let st = wgpu::util::DeviceExt::create_buffer_init(&device, &wgpu::util::BufferInitDescriptor { label: None, contents: &d, usage: wgpu::BufferUsages::COPY_SRC });
                    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("rdb engine") });
                    enc.write_timestamp(q, i);
                    enc.copy_buffer_to_buffer(&st, 0, dst, 0, d.len() as u64);
                    enc.write_timestamp(q, i + 1);
                    queue.submit([enc.finish()]);
                    raea_gputrace::submitted(&queue, "rdb", "rdb engine");
                }
                _ => queue.write_buffer(dst, 0, &d),
            }
        };
        for (k, data) in &frame.uploads {
            let b = storage(&device, data.len() as u64);
            let pair = (pool.ts_uploads < 4).then(|| 8 + 2 * pool.ts_uploads);
            if pair.is_some() && pool.ts.is_some() {
                pool.ts_uploads += 1;
            }
            upload(&mut pool, &b, data, pair);
            pool.bufs.insert(k.clone(), b);
        }
        if let Some(o) = &frame.overlay {
            let ob = pool.overlay.clone().unwrap();
            pool.ts_overlay = pool.ts.is_some();
            upload(&mut pool, &ob, o, Some(6));
        }
    }
    let (Some(img), Some(out)) = (world.resource::<RenderAssets<GpuImage>>().get(&job.target), world.resource::<RenderAssets<GpuShaderBuffer>>().get(&job.out)) else {
        return;
    };
    let view = img.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(wgpu::TextureFormat::Rgba8Unorm), ..Default::default() });
    let pool = world.resource::<PoolGpu>();
    let mut words: Vec<u8> = Vec::with_capacity(4 * 12 * 4);
    let mut planes: Vec<&wgpu::Buffer> = Vec::new();
    for s in 0..4 {
        match frame.slots.get(s) {
            Some((k, slot)) if pool.bufs.contains_key(k) => {
                planes.push(&pool.bufs[k]);
                words.extend(slot.words().iter().flat_map(|v| v.to_le_bytes()));
            }
            _ => {
                planes.push(pool.dummy.as_ref().unwrap());
                words.extend(raea_kgpu::Slot::off().words().iter().flat_map(|v| v.to_le_bytes()));
            }
        }
    }
    let params = wgpu::util::DeviceExt::create_buffer_init(&device, &wgpu::util::BufferInitDescriptor { label: None, contents: &words, usage: wgpu::BufferUsages::STORAGE });
    let (comp, nv12) = (pool.comp.clone().unwrap(), pool.nv12.clone().unwrap());
    let (fbuf, obuf) = (pool.frame.as_ref().unwrap(), pool.overlay.as_ref().unwrap());
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("rdb engine") });
    {
        let (sh, gx, gy) = shape(&device, w * h);
        let mut e: Vec<wgpu::BindGroupEntry> = planes.iter().enumerate().map(|(k, b)| wgpu::BindGroupEntry { binding: k as u32, resource: b.as_entire_binding() }).collect();
        let mask_buf = frame.mask.as_ref().map(|m| wgpu::util::DeviceExt::create_buffer_init(&device, &wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(m), usage: wgpu::BufferUsages::STORAGE }));
        let mbuf = mask_buf.as_ref().unwrap_or(pool.no_mask.as_ref().unwrap());
        e.push(wgpu::BindGroupEntry { binding: 4, resource: obuf.as_entire_binding() });
        e.push(wgpu::BindGroupEntry { binding: 5, resource: params.as_entire_binding() });
        e.push(wgpu::BindGroupEntry { binding: 6, resource: mbuf.as_entire_binding() });
        e.push(wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&view) });
        e.push(wgpu::BindGroupEntry { binding: 8, resource: fbuf.as_entire_binding() });
        e.push(wgpu::BindGroupEntry { binding: 9, resource: sh.as_entire_binding() });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &comp.get_bind_group_layout(0), entries: &e });
        let timestamp_writes = pool.ts.as_ref().map(|(q, _, _)| wgpu::ComputePassTimestampWrites { query_set: q, beginning_of_pass_write_index: Some(2), end_of_pass_write_index: Some(3) });
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes });
        pass.set_pipeline(&comp);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(gx, gy, 1);
    }
    {
        let (sh, gx, gy) = shape(&device, nv12_words(w, h) as u32);
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &nv12.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: fbuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: out.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: sh.as_entire_binding() },
            ],
        });
        let timestamp_writes = pool.ts.as_ref().map(|(q, _, _)| wgpu::ComputePassTimestampWrites { query_set: q, beginning_of_pass_write_index: Some(4), end_of_pass_write_index: Some(5) });
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes });
        pass.set_pipeline(&nv12);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(gx, gy, 1);
    }
    if let Some((q, r, m)) = pool.ts.as_ref() {
        enc.resolve_query_set(q, 0..16, r, 0);
        enc.copy_buffer_to_buffer(r, 0, m, 0, 128);
    }
    let timed = pool.ts.is_some();
    world.resource_mut::<bevy::render::renderer::PendingCommandBuffers>().push_encoder(enc);
    if timed {
        world.resource_mut::<PoolGpu>().ts_pending = true;
    }
}

/// The clip rows on the GPU path: each visible clip's NV12 frame (decoded once, uploaded once into the pool) and its
/// slot (cover, zoom, pan and opacity as 16.16 maps), drawn by z then id.
fn clips_gpu(
    clock: Res<Clock>,
    edit: Res<Edit>,
    mut media: NonSendMut<Media>,
    mut err: ResMut<Error>,
    mut st: ResMut<Stages>,
    mut book: ResMut<PoolBook>,
    mut gf: ResMut<GpuFrame>,
    frame: Res<FrameRows>,
) {
    let t_clips = std::time::Instant::now();
    gf.uploads.clear();
    gf.evict.clear();
    gf.overlay = None;
    gf.slots.clear();
    gf.mask = None;
    let mut masks: Vec<Option<String>> = Vec::new();
    book.tick += 1;
    let tick = book.tick;
    let (f, fps) = (clock.0, edit.prog.fps);
    let (fw, fh) = (edit.prog.width as f64, edit.prog.height as f64);
    let mut rows: Vec<eval::Tup> = frame.cols.rows_of("clip").into_iter().map(|r| frame.cols.tup(r)).collect();
    rows.sort_by(|a, b| num(&a.vals, "z", 0.0).total_cmp(&num(&b.vals, "z", 0.0)).then(a.id.cmp(&b.id)));
    let mut slots_of: HashMap<String, usize> = HashMap::new();
    for row in &rows {
        let v = &row.vals;
        let from = num(v, "from", 0.0) as i64;
        let to = v.get("to").map(|x| x.num() as i64).unwrap_or(i64::MAX);
        let opacity = num(v, "opacity", 1.0) as f64;
        if f < from || f >= to || opacity <= 0.0 {
            continue;
        }
        let Some(V::S(src)) = v.get("src") else {
            err.0 = Some(format!("clip {}: no src", row.id));
            return;
        };
        let slot = {
            let s = slots_of.entry(src.clone()).or_insert(0);
            *s += 1;
            *s - 1
        };
        let t = num(v, "src_in", 0.0) as f64 + (f - from) as f64 / fps;
        let m = &mut *media;
        let dec = match m.decoders.entry((src.clone(), slot)) {
            std::collections::hash_map::Entry::Occupied(o) => o.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => match resolve_src(&edit.media, src, &m.dir).and_then(|p| m.backend.open_video(&p)) {
                Ok(d) => e.insert(d),
                Err(x) => {
                    err.0 = Some(format!("clip {}: {x}", row.id));
                    return;
                }
            },
        };
        // keyed by the frame's index (known without decoding): a frame the pool holds isn't decoded again
        let idx = dec.index_at(t);
        let (dw, dh) = (dec.info().width, dec.info().height);
        let pooled = idx.map(|i| (src.clone(), i as i64)).filter(|k| book.resident.contains_key(k));
        let key: PoolKey = match pooled {
            Some(k) => {
                book.resident.get_mut(&k).unwrap().1 = tick;
                book.hits += 1;
                k
            }
            None => {
                let frame = match dec.frame_nv12_at(t) {
                    Ok(fr) => fr,
                    Err(x) => {
                        err.0 = Some(format!("clip {} at {t:.3}s: {x}", row.id));
                        return;
                    }
                };
                let key = (src.clone(), idx.map(|i| i as i64).unwrap_or((frame.pts * 1e6).round() as i64 + (1 << 40)));
                let n = frame.rgba.len();
                book.resident.insert(key.clone(), (n, tick));
                book.bytes += n;
                book.uploads += 1;
                gf.uploads.push((key.clone(), std::sync::Arc::new(frame.rgba)));
                key
            }
        };
        // cover the box, zoom about the centre, pan within the slack; all as 16.16 maps from output to source pixels
        let frame = (dw, dh);
        let (sw, sh) = (frame.0 as f64, frame.1 as f64);
        let (x, y, w, h) = (num(v, "x", 0.0) as f64, num(v, "y", 0.0) as f64, num(v, "w", fw as f32) as f64, num(v, "h", fh as f32) as f64);
        let zoom = (num(v, "zoom", 1.0) as f64).max(0.01);
        let (cx, cy, cw, ch) = cover(sw, sh, w, h, zoom, (num(v, "pan_x", 0.0) as f64, num(v, "pan_y", 0.0) as f64));
        let fx = |v: f64| (v * 65536.0).round() as i32;
        let s = raea_kgpu::Slot {
            on: 1,
            bx: x.round() as i32,
            by: y.round() as i32,
            ex: (x + w).round() as i32,
            ey: (y + h).round() as i32,
            sw: frame.0 as i32,
            sh: frame.1 as i32,
            cx: fx(cx - cw / 2.0),
            cy: fx(cy - ch / 2.0),
            dx: fx(cw / w),
            dy: fx(ch / h),
            al: (opacity.min(1.0) * 256.0).round() as i32,
        };
        if gf.slots.len() == 4 {
            err.0 = Some(format!("frame {f}: more than four clips at once (the composite kernel has four slots)"));
            return;
        }
        masks.push(crate::shape::mask_of(v));
        gf.slots.push((key, s));
    }
    // clips with a mask (a solid in space and time) show its slice through this frame
    if masks.iter().any(Option::is_some) {
        let mut sl: Vec<raea_kgpu::Slot> = gf.slots.iter().map(|x| x.1).collect();
        match crate::shape::slot_masks(&masks, &mut sl, f, fw as usize, fh as usize, edit.prog.frames) {
            Ok(m) => {
                for (slot, s) in gf.slots.iter_mut().zip(sl) {
                    slot.1 = s;
                }
                gf.mask = m.map(std::sync::Arc::new);
            }
            Err(e) => {
                err.0 = Some(e);
                return;
            }
        }
    }
    // over budget: the least recently used frames not drawn now go
    if book.bytes > pool_budget() {
        let mut by_age: Vec<(PoolKey, u64, usize)> = book.resident.iter().filter(|(_, (_, t))| *t != tick).map(|(k, (n, t))| (k.clone(), *t, *n)).collect();
        by_age.sort_by_key(|x| x.1);
        for (k, _, n) in by_age {
            if book.bytes <= pool_budget() {
                break;
            }
            book.resident.remove(&k);
            book.bytes -= n;
            gf.evict.push(k);
        }
    }
    gf.on = true;
    st.clips += t_clips.elapsed().as_nanos() as u64;
}

/// Where the time goes, accumulated over frames (G8's baseline): nanoseconds of CPU per stage.
#[derive(Resource, Default, Clone, Debug)]
pub struct Stages {
    pub rules: u64,
    /// clips: decode (frame_at), of which conversion is counted by raea_media::CONVERT_NS
    pub clips: u64,
    /// writing decoded pixels into the clips' textures (the CPU side of the upload)
    pub clip_prep: u64,
    /// the Blitz overlay: rows, style, layout, raster, unpremultiply and upload (only when its digest changed)
    pub overlay: u64,
    overlay_t0: Option<std::time::Instant>,
    /// the main world's Update, first system to last
    pub main: u64,
    main_t0: Option<std::time::Instant>,
    /// whole app.update() (main world + render world: extract, prepare incl. texture uploads, queue, submit)
    pub update: u64,
    /// copying readbacks out of the padded rows
    pub readback_copy: u64,
    /// serial probe (RAEA_SERIAL): waiting for the GPU to finish after each update
    pub gpu_wait: u64,
    pub updates: u64,
    pub frames: u64,
}

fn mark_main_start(mut st: ResMut<Stages>) {
    st.main_t0 = Some(std::time::Instant::now());
}

fn mark_main_end(mut st: ResMut<Stages>) {
    if let Some(t) = st.main_t0.take() {
        st.main += t.elapsed().as_nanos() as u64;
    }
}

fn mark_overlay_start(mut st: ResMut<Stages>) {
    st.overlay_t0 = Some(std::time::Instant::now());
}

fn mark_overlay_end(mut st: ResMut<Stages>) {
    if let Some(t) = st.overlay_t0.take() {
        st.overlay += t.elapsed().as_nanos() as u64;
    }
}

/// Clips and the overlay composite on the GPU (the default with NV12), or as Bevy UI nodes (RAEA_COMPOSITE=ui).
#[derive(Resource, Clone, Copy)]
struct Composite(bool);

fn on_gpu(c: Res<Composite>) -> bool {
    c.0
}

fn on_ui(c: Res<Composite>) -> bool {
    !c.0
}

/// Overlay (Blitz) above every clip.
const OVERLAY_Z: i32 = 1000;

const KINDS_2D: &[&str] = &["title", "text", "box", "caption", "image"];

fn num(v: &BTreeMap<String, V>, k: &str, d: f32) -> f32 {
    v.get(k).map(|x| x.num() as f32).unwrap_or(d)
}

/// The frame's state onto the entities: one relational step, pure.
fn relational(edit: Res<Edit>, clock: Res<Clock>, mut err: ResMut<Error>, mut st: ResMut<Stages>, mut frame: ResMut<FrameRows>) {
    let t0 = std::time::Instant::now();
    frame.cols = edit.timeline.cols(clock.0);
    st.rules += t0.elapsed().as_nanos() as u64;
    let _ = &mut err;
}

/// Straight-alpha RGBA8 (sRGB) as premultiplied-alpha RGBA8 (the colour premultiplied in linear light, then encoded), with
/// every mip level after it (2x2 box averages of the premultiplied linear values): the data and the level count, for a
/// texture sampled trilinearly and blended premultiplied.
fn mip_chain(data: Vec<u8>, w: u32, h: u32) -> (Vec<u8>, u32) {
    let to_lin: Vec<f32> = (0..256).map(|i| {
        let c = i as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }).collect();
    let to_srgb = |l: f32| -> u8 {
        let c = if l <= 0.0031308 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
        (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    };
    // level 0, premultiplied, kept in linear floats for the averages
    let mut cur: Vec<[f32; 4]> = data.chunks_exact(4).map(|p| {
        let a = p[3] as f32 / 255.0;
        [to_lin[p[0] as usize] * a, to_lin[p[1] as usize] * a, to_lin[p[2] as usize] * a, a]
    }).collect();
    let encode = |px: &[[f32; 4]], out: &mut Vec<u8>| {
        for p in px {
            out.extend_from_slice(&[to_srgb(p[0]), to_srgb(p[1]), to_srgb(p[2]), (p[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8]);
        }
    };
    let mut out = Vec::with_capacity(data.len() * 4 / 3 + 16);
    encode(&cur, &mut out);
    let (mut cw, mut ch) = (w as usize, h as usize);
    let mut levels = 1;
    while cw > 1 || ch > 1 {
        let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
        let mut next = vec![[0f32; 4]; nw * nh];
        for y in 0..nh {
            for x in 0..nw {
                let mut acc = [0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = cur[(y * 2 + dy).min(ch - 1) * cw + (x * 2 + dx).min(cw - 1)];
                    for c in 0..4 {
                        acc[c] += p[c] / 4.0;
                    }
                }
                next[y * nw + x] = acc;
            }
        }
        encode(&next, &mut out);
        cur = next;
        (cw, ch) = (nw, nh);
        levels += 1;
    }
    (out, levels)
}

/// SQL column -> raea prop, for 2D kinds.
fn prop_of(col: &str) -> &str {
    match col {
        "top" => "y",
        "left" => "x",
        "width" => "w",
        "height" => "h",
        other => other,
    }
}

fn json_of(v: &V) -> serde_json::Value {
    match v {
        V::N(n) => serde_json::json!(n),
        V::S(s) => serde_json::json!(s),
    }
}

/// A 2D row's props as installed (its constants: what its columns started as, the image's asset path).
#[derive(Component, Clone)]
pub(crate) struct Base2d(pub serde_json::Map<String, serde_json::Value>);

/// A 2D row's props at a frame: its base under the frame's values (phase 5: the columns feed the DOM node directly).
pub(crate) fn props_2d(base: &Base2d, vals: &BTreeMap<String, V>) -> serde_json::Map<String, serde_json::Value> {
    let mut p = base.0.clone();
    for (k, x) in vals {
        if !matches!(k.as_str(), "from" | "to" | "class" | "src") {
            p.insert(prop_of(k).to_string(), json_of(x));
        }
    }
    p
}

/// Every 2D row's frame values onto its DOM node, written only where they changed (so Blitz patches only those rows).
fn to_dom(clock: Res<Clock>, frame: Res<FrameRows>, mut rc: ResMut<raea::world::Clock>, mut q: Query<(Entity, &Base2d, &mut raea::world::Resolved)>) {
    rc.0 = clock.0;
    for (e, base, mut r) in &mut q {
        let Some(v) = frame.vals(e) else { continue };
        let p = props_2d(base, &v);
        if r.0 != p {
            r.0 = p;
        }
    }
}

fn upload(frame: Res<raea::world::Frame>, overlay: Option<Res<Overlay>>, gf: Option<ResMut<GpuFrame>>, mut images: ResMut<Assets<Image>>) {
    // the GPU path takes Blitz's premultiplied pixels as they are (the composite blends premultiplied)
    if let Some(mut gf) = gf {
        if gf.on {
            gf.overlay = Some(std::sync::Arc::new(frame.rgba.clone()));
            return;
        }
    }
    let Some(overlay) = overlay else { return };
    let Some(mut img) = images.get_mut(&overlay.0) else { return };
    let mut px = frame.rgba.clone();
    for p in px.chunks_mut(4) {
        let a = p[3] as u32;
        if a > 0 && a < 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    img.data = Some(px);
}

/// Each clip row's picture at the clock: visible between its `from` and `to` frames, showing its source at
/// `src_in + (frame - from) / fps` seconds, cropped to cover its box (x, y, w, h; the whole frame by default) and
/// zoomed about the centre by `zoom`.
fn clips(
    clock: Res<Clock>,
    edit: Res<Edit>,
    mut media: NonSendMut<Media>,
    mut err: ResMut<Error>,
    mut st: ResMut<Stages>,
    mut images: ResMut<Assets<Image>>,
    frame: Res<FrameRows>,
    mut q: Query<(Entity, &RowKey, &mut Clip, &mut Node, &mut ImageNode, &mut Visibility, &mut GlobalZIndex)>,
) {
    let t_clips = std::time::Instant::now();
    let mut prep_ns = 0u64;
    let (f, fps) = (clock.0, edit.prog.fps);
    let (fw, fh) = (edit.prog.width as f32, edit.prog.height as f32);
    let mut rows: Vec<_> = q.iter_mut().collect();
    rows.sort_by(|a, b| a.1.id.cmp(&b.1.id));
    let mut slots: HashMap<String, usize> = HashMap::new();
    for (e, row, mut clip, mut node, mut inode, mut vis, mut z) in rows {
        let Some(v) = frame.vals(e) else { continue };
        let v = &v;
        let from = num(v, "from", 0.0) as i64;
        let to = v.get("to").map(|x| x.num() as i64).unwrap_or(i64::MAX);
        let opacity = num(v, "opacity", 1.0);
        if f < from || f >= to || opacity <= 0.0 {
            *vis = Visibility::Hidden;
            continue;
        }
        let Some(V::S(src)) = v.get("src") else {
            err.0 = Some(format!("clip {}: no src", row.id));
            return;
        };
        let slot = {
            let s = slots.entry(src.clone()).or_insert(0);
            *s += 1;
            *s - 1
        };
        let t = num(v, "src_in", 0.0) as f64 + (f - from) as f64 / fps;
        let key = (src.clone(), slot, (t * 1e6).round() as i64);
        if clip.last.as_ref() != Some(&key) {
            let m = &mut *media;
            let dec = match m.decoders.entry((src.clone(), slot)) {
                std::collections::hash_map::Entry::Occupied(o) => o.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => match resolve_src(&edit.media, src, &m.dir).and_then(|p| m.backend.open_video(&p)) {
                    Ok(d) => e.insert(d),
                    Err(x) => {
                        err.0 = Some(format!("clip {}: {x}", row.id));
                        return;
                    }
                },
            };
            let frame = match dec.frame_at(t) {
                Ok(fr) => fr,
                Err(x) => {
                    err.0 = Some(format!("clip {} at {t:.3}s: {x}", row.id));
                    return;
                }
            };
            let t_prep = std::time::Instant::now();
            let Some(mut img) = images.get_mut(&clip.image) else { continue };
            if img.width() != frame.width || img.height() != frame.height {
                *img = Image::new(
                    bevy::render::render_resource::Extent3d { width: frame.width, height: frame.height, depth_or_array_layers: 1 },
                    bevy::render::render_resource::TextureDimension::D2,
                    frame.rgba,
                    TextureFormat::Rgba8UnormSrgb,
                    bevy::asset::RenderAssetUsages::default(),
                );
            } else {
                img.data = Some(frame.rgba);
            }
            prep_ns += t_prep.elapsed().as_nanos() as u64;
            clip.last = Some(key);
        }
        let Some(img) = images.get(&clip.image) else { continue };
        let (sw, sh) = (img.width() as f32, img.height() as f32);
        let (x, y, w, h) = (num(v, "x", 0.0), num(v, "y", 0.0), num(v, "w", fw), num(v, "h", fh));
        node.left = Val::Px(x);
        node.top = Val::Px(y);
        node.width = Val::Px(w);
        node.height = Val::Px(h);
        // cover: the largest centred source rect with the box's aspect, then zoomed
        let zoom = num(v, "zoom", 1.0).max(0.01);
        let (cx, cy, cw, ch) = cover(sw, sh, w, h, zoom, (num(v, "pan_x", 0.0), num(v, "pan_y", 0.0)));
        inode.rect = Some(Rect::new(cx - cw / 2.0, cy - ch / 2.0, cx + cw / 2.0, cy + ch / 2.0));
        inode.color = Color::srgba(1.0, 1.0, 1.0, opacity.min(1.0));
        *z = GlobalZIndex(num(v, "z", 0.0) as i32);
        *vis = Visibility::Inherited;
    }
    st.clips += t_clips.elapsed().as_nanos() as u64;
    st.clip_prep += prep_ns;
}

fn tag_colour(f: i64, generation: u8) -> (u8, u8, u8) {
    ((f & 0xff) as u8, ((f >> 8) & 0xff) as u8, generation)
}

fn tag(clock: Res<Clock>, generation: Res<Gen>, mut q: Query<&mut BackgroundColor, With<FrameTag>>) {
    let (r, g, b) = tag_colour(clock.0, generation.0);
    for mut c in &mut q {
        *c = BackgroundColor(Color::srgb_u8(r, g, b));
    }
}

fn colour(v: Option<&V>, d: [f32; 4]) -> Color {
    let c = match v {
        Some(V::S(s)) => {
            let h = s.trim_start_matches('#');
            let b = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("ff"), 16).map(|x| x as f32 / 255.0).unwrap_or(1.0);
            if h.len() >= 6 { [b(0), b(2), b(4), if h.len() == 8 { b(6) } else { 1.0 }] } else { d }
        }
        _ => d,
    };
    Color::srgba(c[0], c[1], c[2], c[3])
}

/// A clip's or audio row's source: `media:<sha256>` names a row of the media table (content-addressed footage, music
/// and voice; the row holds the file's path and size), anything else is a path. The size is checked on every
/// resolve (cheap); the hash was checked when the row was made (`rdb media`).
pub fn resolve_src(rows: &[Tup], src: &str, dir: &Path) -> Result<PathBuf, String> {
    let Some(hash) = src.strip_prefix("media:") else { return Ok(find(dir, src)) };
    let m = rows.iter().find(|t| t.kind == "media" && t.id == hash).ok_or(format!("{src}: no such row in media"))?;
    // the portable decoders read the row's edit mezzanine when it has one (the platform's read the original)
    let portable = matches!(media_backend().as_str(), "soft" | "av1");
    let (pcol, bcol) = match (portable, m.vals.get("mezz")) {
        (true, Some(V::S(p))) if !p.is_empty() => ("mezz", "mezz_bytes"),
        _ => ("path", "bytes"),
    };
    // `src` reads as `path` (a row written by hand rather than by `media`)
    let path = match m.vals.get(pcol).or_else(|| m.vals.get("src")) {
        Some(V::S(p)) => find(dir, p),
        _ => return Err(format!("media {hash}: no path (INSERT INTO media (id, path) VALUES ('{hash}', 'takes/<take>/video.mov'))")),
    };
    let size = std::fs::metadata(&path).map_err(|e| format!("media {hash}: {}: {e}", path.display()))?.len();
    if let Some(b) = m.vals.get(bcol) {
        if b.num() as u64 != size {
            return Err(format!("media {hash}: {} has changed (size {size}, the row says {})", path.display(), b.num()));
        }
    }
    Ok(path)
}

/// A relative path from the edit's folder, or from a folder above it (a project's `takes/...` from `edits/x.rdb`);
/// the edit's folder when it's in none of them.
fn find(dir: &Path, p: &str) -> PathBuf {
    dir.ancestors().take(3).map(|d| d.join(p)).find(|f| f.exists()).unwrap_or_else(|| dir.join(p))
}

/// A clip's source rect for its box: the largest rect with the box's aspect, zoomed about the centre, then panned within
/// the slack (pan -1..1 on each axis; 0 centres). Returns its centre and size, in source pixels. The one cover rule every
/// renderer uses (the engine, gpuview, the sketch, previz); stills reframe a subject with the pan.
/// Generic over the float type so each renderer keeps its own arithmetic (the engine's UI clips are f32, the rest f64)
/// and every frame stays bit-identical.
pub(crate) fn cover<T: Real>(sw: T, sh: T, w: T, h: T, zoom: T, pan: (T, T)) -> (T, T, T, T) {
    let (cw, ch) = if sw / sh > w / h { (sh * w / h, sh) } else { (sw, sw * h / w) };
    let (cw, ch) = (cw / zoom, ch / zoom);
    (sw / T::TWO + pan.0 * (sw - cw) / T::TWO, sh / T::TWO + pan.1 * (sh - ch) / T::TWO, cw, ch)
}

/// f32 or f64, for `cover`.
pub(crate) trait Real: Copy + PartialOrd + std::ops::Add<Output = Self> + std::ops::Sub<Output = Self> + std::ops::Mul<Output = Self> + std::ops::Div<Output = Self> {
    const TWO: Self;
}
impl Real for f32 {
    const TWO: f32 = 2.0;
}
impl Real for f64 {
    const TWO: f64 = 2.0;
}

/// A 2D row (title, text, box, caption, image) as a raea row: its id ("kind:id"), kind, props and span.
/// A row with a `parent` (the id of another 2D row) sits inside it; raea times a child from its parent's start.
pub(crate) fn row2d(t: &Tup, all: &[Tup]) -> (String, serde_json::Value) {
    let vals = &t.vals;
    let parent = match vals.get("parent") {
        Some(V::S(p)) if !p.is_empty() => all.iter().find(|r| &r.id == p && is_2d(&r.kind) && r.id != t.id),
        _ => None,
    };
    let at = parent.and_then(|p| p.vals.get("from")).map(|f| f.num()).unwrap_or(0.0);
    let mut props = serde_json::Map::new();
    for (c, v) in vals {
        if matches!(c.as_str(), "from" | "to" | "class" | "parent") {
            continue;
        }
        // an image's src is a content hash: the bytes live in <edit dir>/assets
        if let (true, V::S(src)) = (c == "src", v) {
            if let Some(h) = src.strip_prefix("asset:") {
                props.insert("src".into(), serde_json::json!({"asset": format!("assets/{h}.png")}));
                continue;
            }
            // a picture's path as written (`image logo {src: "assets/logo.png"}`) was dropped and the row drew nothing
            if t.kind == "image" && !src.starts_with("media:") && !src.is_empty() {
                props.insert("src".into(), serde_json::json!({"asset": src}));
                continue;
            }
        }
        props.insert(prop_of(c).to_string(), json_of(v));
    }
    let id = format!("{}:{}", t.kind, t.id);
    let mut row = serde_json::json!({"id": id, "kind": if t.kind == "caption" { "text" } else { t.kind.as_str() }, "props": props});
    if let Some(f) = vals.get("from") {
        row["from"] = serde_json::json!((f.num() - at) as i64);
    }
    if let Some(to) = vals.get("to") {
        row["to"] = serde_json::json!((to.num() - at) as i64);
    }
    if let Some(p) = parent {
        row["parent"] = serde_json::json!(format!("{}:{}", p.kind, p.id));
    }
    (id, row)
}

pub(crate) fn is_2d(kind: &str) -> bool {
    KINDS_2D.contains(&kind)
}


/// The media backend clips decode with: RAEA_MEDIA, else platform hardware where there is one, else portable software.
pub fn media_backend() -> String {
    std::env::var("RAEA_MEDIA").unwrap_or_else(|_| if cfg!(target_vendor = "apple") { "apple".into() } else { "soft".into() })
}

pub struct Engine {
    app: App,
    target: Handle<Image>,
    w: u32,
    h: u32,
    dir: PathBuf,
    /// a frame has come back since the last load (the GPU holds the load's assets)
    warm: bool,
    /// RAEA_SERIAL: wait for the GPU after every update, to time its un-overlapped work
    serial: bool,
    /// frames come back as NV12 (the default) or as RGBA (RAEA_READBACK=rgba, the reference path)
    pub nv12: bool,
}

/// RAEA_MEM_MARKS: one line on stderr per start-up phase, "mem_mark <ms since the first mark> <phys_footprint MB>
/// <phase>", to line up with a memory timeline (the footprint is the process's, as the OS counts it).
pub fn mem_mark(what: &str) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    if !*ON.get_or_init(|| std::env::var_os("RAEA_MEM_MARKS").is_some()) {
        return;
    }
    let t = T0.get_or_init(std::time::Instant::now).elapsed().as_secs_f64() * 1e3;
    let mb = phys_footprint().map(|b| format!("{:.1}", b as f64 / 1048576.0)).unwrap_or_else(|| "?".into());
    eprintln!("mem_mark {t:.1} {mb} {what}");
}

/// The edit has 3D rows (a light or a mesh): its camera draws through the 3D pipeline, and the Apple encoder runs
/// real-time for it.
static PREVIEW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Preview quality for engines made after this call (a live preview; exports keep full quality): no MSAA and no shadow
/// maps, the bulk of a 3D frame's render-world time on a few primitives.
pub fn set_preview_quality(on: bool) {
    PREVIEW.store(on, std::sync::atomic::Ordering::Relaxed);
}

fn preview() -> bool {
    // RAEA_PREVIEW_QUALITY=1 asks for it from outside the process (a shell's preview, a measurement)
    PREVIEW.load(std::sync::atomic::Ordering::Relaxed) || std::env::var_os("RAEA_PREVIEW_QUALITY").is_some_and(|v| v != "0")
}

pub fn has_3d(rows: &[Tup]) -> bool {
    rows.iter().any(|t| matches!(t.kind.as_str(), "light" | "cube" | "sphere" | "plane" | "panel" | "mesh" | "model"))
}

/// RAEA_MEM_MARKS_EVERY: frames between render marks (default 100).
pub fn mark_every() -> usize {
    std::env::var("RAEA_MEM_MARKS_EVERY").ok().and_then(|v| v.parse().ok()).filter(|n: &usize| *n > 0).unwrap_or(100)
}

/// The process's physical footprint (task_info TASK_VM_INFO), on Apple platforms.
#[cfg(target_vendor = "apple")]
pub fn phys_footprint() -> Option<u64> {
    unsafe extern "C" {
        static mach_task_self_: u32;
        fn task_info(task: u32, flavor: u32, info: *mut i32, count: *mut u32) -> i32;
    }
    // task_vm_info_data_t: phys_footprint is the 64-bit field at byte 144
    let mut buf = [0u64; 64];
    let mut count: u32 = (std::mem::size_of_val(&buf) / 4) as u32;
    let r = unsafe { task_info(mach_task_self_, 22, buf.as_mut_ptr() as *mut i32, &mut count) };
    (r == 0 && count as usize * 4 >= 152).then(|| buf[144 / 8])
}

#[cfg(not(target_vendor = "apple"))]
pub fn phys_footprint() -> Option<u64> {
    None
}

/// Bevy's renderer on the paint device (raea::quads), so the pages and the 3D share one GPU device (One renderer:
/// one device, one frame graph); its own device where the GPU paint is off (RAEA_PAINT=cpu) or RAEA_SHARED_DEVICE=0.
fn render_creation() -> bevy::render::settings::RenderCreation {
    use bevy::render::renderer::{RenderAdapter, RenderAdapterInfo, RenderDevice, RenderInstance, RenderQueue, WgpuWrapper};
    let shared = raea::quads::enabled() && std::env::var("RAEA_SHARED_DEVICE").as_deref() != Ok("0");
    match shared.then(raea::quads::handles) {
        Some(Ok((instance, adapter, device, queue))) => {
            let info = adapter.get_info();
            bevy::render::settings::RenderCreation::manual(
                RenderDevice::from(device),
                RenderQueue(std::sync::Arc::new(WgpuWrapper::new(queue))),
                RenderAdapterInfo(WgpuWrapper::new(info)),
                RenderAdapter(std::sync::Arc::new(WgpuWrapper::new(adapter))),
                RenderInstance(std::sync::Arc::new(WgpuWrapper::new(instance))),
            )
        }
        Some(Err(e)) => {
            eprintln!("raea: the paint device isn't there ({e}); Bevy makes its own");
            default()
        }
        None => default(),
    }
}

impl Engine {
    pub fn new(prog: &Program, inserted: &[Tup], dir: &Path) -> Result<Engine, String> {
        Engine::new_shared(prog, inserted, dir, None)
    }

    /// As new, with the edit's timeline already evaluated (shared with the sound or a preview).
    pub fn new_shared(prog: &Program, inserted: &[Tup], dir: &Path, timeline: Option<std::sync::Arc<crate::timeline::Timeline>>) -> Result<Engine, String> {
        Engine::build(prog, inserted, dir, timeline, false)
    }

    /// For live play: the program drawn on the paint device into its target and handed over as a texture (`draw`,
    /// `texture`), nothing read back. A live 3D scene draws through this, the same rows and code as its render.
    pub fn new_live(prog: &Program, inserted: &[Tup], dir: &Path) -> Result<Engine, String> {
        Engine::build(prog, inserted, dir, None, true)
    }

    /// Frame `f` drawn into the target (one update: the pipeline is synchronous, so it's on the GPU's queue when this
    /// returns, ahead of anything the paint device is given next).
    pub fn draw(&mut self, f: i64) {
        self.app.world_mut().resource_mut::<Clock>().0 = f;
        self.app.update();
    }

    /// The target as a texture view on the paint device (its stored bytes, w x h and one tag row under them).
    pub fn texture(&self) -> Option<wgpu::TextureView> {
        let rw = self.app.get_sub_app(bevy::render::RenderApp)?.world();
        let images = rw.get_resource::<RenderAssets<bevy::render::texture::GpuImage>>()?;
        let gi = images.get(&self.target)?;
        let t: &wgpu::Texture = &gi.texture;
        Some(t.create_view(&wgpu::TextureViewDescriptor { format: Some(wgpu::TextureFormat::Rgba8Unorm), ..Default::default() }))
    }

    fn build(prog: &Program, inserted: &[Tup], dir: &Path, timeline: Option<std::sync::Arc<crate::timeline::Timeline>>, live: bool) -> Result<Engine, String> {
        let (w, h) = (prog.width, prog.height);
        mem_mark("engine: new");
        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, close_when_requested: false, ..default() })
                .set(bevy::log::LogPlugin { level: bevy::log::Level::WARN, ..default() })
                // glTF models (scene3d) load from the paths their rows name, which are the edit's files wherever they are
                .set(bevy::asset::AssetPlugin { unapproved_path_mode: bevy::asset::UnapprovedPathMode::Allow, meta_check: bevy::asset::AssetMetaCheck::Never, ..default() })
                // pipelines compile before the draw that needs them, so a frame never misses a mesh whose shader was
                // still compiling: the first tagged readback of a frame is the frame
                .set(bevy::render::RenderPlugin { synchronous_pipeline_compilation: true, render_creation: render_creation(), ..default() })
                .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>(),
        )
        // each render pass's time (CPU side on Metal, where wgpu has no timestamps inside passes), into the run's trace
        // once a second as `bevy_passes`: a slow frame shows which of Bevy's passes it was
        .add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin)
        .add_systems(Last, forward_render_diagnostics)
        .init_resource::<Clock>()
        .init_resource::<Out>()
        .init_resource::<Error>()
        .init_resource::<Gen>()
        .init_resource::<OverlayKey>()
        .init_resource::<Stages>()
        .add_systems(
            Update,
            (
                mark_main_start,
                relational,
                // the 3D moved by the frame's rows (meshes, models, camera, lights): scene3d
                scene3d::model_setup,
                (scene3d::sync_meshes, scene3d::sync_camera, scene3d::sync_lights, scene3d::sync_models).chain(),
                clips.run_if(on_ui),
                clips_gpu.run_if(on_gpu),
                overlay_key,
                (mark_overlay_start, to_dom, raea::world::hosted_systems(), upload, mark_overlay_end).chain().run_if(overlay_dirty),
                tag,
                mark_main_end,
            )
                .chain(),
        );
        let nv12_on = std::env::var("RAEA_READBACK").map(|v| v != "rgba").unwrap_or(true);
        let composite = nv12_on && std::env::var("RAEA_COMPOSITE").map(|v| v != "ui").unwrap_or(true);
        app.insert_resource(Composite(composite));
        app.add_plugins(ExtractResourcePlugin::<Nv12Job>::default());
        if composite {
            app.init_resource::<GpuFrame>().init_resource::<PoolBook>().add_plugins(ExtractResourcePlugin::<GpuFrame>::default());
            app.sub_app_mut(bevy::render::RenderApp).init_resource::<PoolGpu>().add_systems(
                bevy::render::renderer::RenderGraph,
                gpu_frame_pass.in_set(bevy::render::renderer::RenderGraphSystems::Render).after(bevy::core_pipeline::schedule::camera_driver),
            );
        } else {
            app.sub_app_mut(bevy::render::RenderApp).init_resource::<Nv12Gpu>().add_systems(
                bevy::render::renderer::RenderGraph,
                nv12_pass.in_set(bevy::render::renderer::RenderGraphSystems::Render).after(bevy::core_pipeline::schedule::camera_driver),
            );
        }
        // RAEA_MEM_MARKS: the render world's sets, marked through the first updates (where start-up memory goes)
        if std::env::var_os("RAEA_MEM_MARKS").is_some() {
            use bevy::render::{Render, RenderSystems as RS};
            fn at(name: &'static str) -> impl FnMut() {
                let mut n = 0u32;
                move || {
                    if n < 3 {
                        mem_mark(&format!("render world: {name} (update {n})"));
                    }
                    n += 1;
                }
            }
            app.sub_app_mut(bevy::render::RenderApp).add_systems(
                Render,
                (
                    at("start").before(RS::ExtractCommands),
                    at("assets prepared").after(RS::PrepareAssets).before(RS::CreateViews),
                    at("views made").after(RS::CreateViews).before(RS::Specialize),
                    at("specialized").after(RS::Specialize).before(RS::PrepareViews),
                    at("views prepared").after(RS::PrepareViews).before(RS::Queue),
                    at("queued").after(RS::QueueSweep).before(RS::PhaseSort),
                    at("resources prepared").after(RS::PrepareResourcesFlush).before(RS::PrepareBindGroups),
                    at("bind groups").after(RS::PrepareBindGroups).before(RS::Render),
                    at("rendered").after(RS::Render),
                ),
            );
        }
        let backend = raea_media::backend(&media_backend())?;
        app.insert_non_send_resource(Media { backend, decoders: HashMap::new(), dir: dir.to_path_buf() });
        mem_mark("engine: plugins built (DefaultPlugins, the render device)");
        app.finish();
        mem_mark("engine: plugins finished");
        app.cleanup();
        mem_mark("engine: plugins cleaned up");
        let mut image = Image::new_target_texture(w, h + 1, TextureFormat::Rgba8UnormSrgb, None);
        image.texture_descriptor.usage |= TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING;
        // a Unorm view lets the NV12 kernel read the stored bytes (an sRGB view would decode them to linear)
        image.texture_descriptor.view_formats = &[TextureFormat::Rgba8Unorm];
        let target = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        let nv12 = std::env::var("RAEA_READBACK").map(|v| v != "rgba").unwrap_or(true);
        if live {
            // nothing comes back: the target is sampled where it is (Engine::texture)
        } else if nv12 {
            let out = app
                .world_mut()
                .resource_mut::<Assets<ShaderBuffer>>()
                .add(ShaderBuffer::with_size(nv12_words(w, h) * 4, bevy::asset::RenderAssetUsages::RENDER_WORLD));
            app.world_mut().insert_resource(Nv12Job { target: target.clone(), out: out.clone(), w, h });
            app.world_mut().spawn(Readback::buffer(out)).observe(|ev: On<ReadbackComplete>, mut out: ResMut<Out>| out.0.push(ev.data.clone()));
        } else {
            app.world_mut().spawn(Readback::texture(target.clone())).observe(|ev: On<ReadbackComplete>, mut out: ResMut<Out>| out.0.push(ev.data.clone()));
        }
        let mut e = Engine { app, target, w, h, dir: dir.to_path_buf(), warm: false, serial: std::env::var_os("RAEA_SERIAL").is_some(), nv12: std::env::var("RAEA_READBACK").map(|v| v != "rgba").unwrap_or(true) };
        mem_mark("engine: target and readback made");
        e.load_shared(prog, inserted, timeline)?;
        mem_mark("engine: edit loaded into the World");
        for i in 0..8 {
            e.app.update();
            mem_mark(&format!("engine: warm-up update {i}"));
        }
        Ok(e)
    }

    /// Overlay redraws and reuses so far.
    pub fn overlay_stats(&self) -> (u64, u64) {
        let k = self.app.world().resource::<OverlayKey>();
        (k.drawn, k.reused)
    }

    /// The 2D rows: (rows, DOM syncs, row patches); a sync patches only the rows whose values changed.
    pub fn dom_stats(&mut self) -> (usize, u64, u64) {
        let world = self.app.world_mut();
        let rows = world.query_filtered::<Entity, With<Base2d>>().iter(world).count();
        let s = world.get_resource::<raea::world::DomStats>().copied().unwrap_or_default();
        (rows, s.runs, s.patched)
    }

    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    /// Re-mirror the tables into the World (after a move). The size must not change (make a new Engine for that).
    pub fn load(&mut self, prog: &Program, inserted: &[Tup]) -> Result<(), String> {
        self.load_shared(prog, inserted, None)
    }

    /// As load; a timeline passed in must be this edit's (its rows in the edit's order), and is used as it is.
    pub fn load_shared(&mut self, prog: &Program, inserted: &[Tup], timeline: Option<std::sync::Arc<crate::timeline::Timeline>>) -> Result<(), String> {
        if (prog.width, prog.height) != (self.w, self.h) {
            return Err("the edit's size changed: make a new Engine".into());
        }
        let world = self.app.world_mut();
        let g = world.resource::<Gen>().0;
        world.insert_resource(Gen(if g == 255 { 1 } else { g + 1 }));
        world.resource_mut::<Out>().0.clear();
        world.insert_resource(OverlayKey::default());
        self.warm = false;
        // the rows go through the store; what else the last load spawned (overlay, tag, raea's leftovers) goes too
        store::clear(world);
        let old: Vec<Entity> = world.query_filtered::<Entity, Or<(With<Mirror>, With<raea::world::Name>)>>().iter(world).collect();
        for e in old {
            world.despawn(e);
        }
        let ents = store::load(world, inserted);
        let mut rows2d: Vec<(String, serde_json::Value, Entity)> = Vec::new();
        let mut camera = None;
        // an edit with no 3D rows draws nothing through the 3D pipeline: its camera only clears, so it gets no MSAA
        // (4x colour and depth targets at full size were most of the start-up memory)
        let has_3d = has_3d(inserted);
        // cameras first, so UI can target them; otherwise insertion order
        let mut order: Vec<(&Tup, Entity)> = inserted.iter().zip(ents.iter().copied()).collect();
        order.sort_by_key(|(t, _)| t.kind != "camera");
        for (t, e) in order {
            let vals = &t.vals;
            let g = |k: &str, d: f32| num(vals, k, d);
            match t.kind.as_str() {
                "camera" => {
                    world.entity_mut(e).insert((
                        Camera3d::default(),
                        Camera { clear_color: ClearColorConfig::Custom(colour(vals.get("bg"), [0.05, 0.04, 0.1, 1.0])), ..default() },
                        RenderTarget::Image(self.target.clone().into()),
                        if has_3d && !preview() { Msaa::default() } else { Msaa::Off },
                        Transform::from_xyz(g("x", 0.0), g("y", 2.0), g("z", 9.0)).looking_at(Vec3::new(g("look_x", 0.0), g("look_y", 1.0), g("look_z", 0.0)), Vec3::Y),
                    ));
                    // three.js-style scenes (raea-motion's 3D): a vertical field of view in degrees, and an ambient
                    // light on the camera (brightness in cd/m²); without them the camera is as it always was
                    if let Some(fov) = vals.get("fov").map(V::num) {
                        world.entity_mut(e).insert(Projection::Perspective(PerspectiveProjection { fov: (fov as f32).to_radians(), ..default() }));
                    }
                    if let Some(b) = vals.get("ambient").map(V::num) {
                        world.entity_mut(e).insert(AmbientLight { color: colour(vals.get("ambient_color"), [1.0, 1.0, 1.0, 1.0]), brightness: b as f32, affects_lightmapped_meshes: true });
                    }
                    // the lens and post-processing (bloom, fog, tonemapping, exposure): scene3d
                    scene3d::camera(world, e, vals);
                    camera = Some(e);
                }
                // directional (as before), point or spot: scene3d
                "light" => scene3d::light(world, e, vals),
                "model" => scene3d::model(world, e, t, inserted, &self.dir)?,
                "cube" | "sphere" | "plane" => {
                    let size = g("size", 1.0);
                    let mesh = match t.kind.as_str() {
                        "cube" => Mesh::from(Cuboid::from_length(size)),
                        "sphere" => Mesh::from(Sphere::new(size / 2.0).mesh().ico(5).unwrap()),
                        _ => Mesh::from(Plane3d::default().mesh().size(size, size)),
                    };
                    let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
                    let mat = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
                        base_color: colour(vals.get("color"), [0.8, 0.8, 0.8, 1.0]),
                        perceptual_roughness: g("rough", 0.45),
                        metallic: g("metal", 0.0),
                        ..default()
                    });
                    world.entity_mut(e).insert((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_xyz(g("x", 0.0), g("y", 0.0), g("z", 0.0))));
                }
                "mesh" => {
                    // a shape as a value (docs/media.feature M2): its `shape` column, a CSG expression, built by Manifold
                    // once per expression (shape::cached), flat-shaded
                    let expr = match vals.get("shape") { Some(V::S(s)) => s.clone(), _ => return Err(format!("mesh {}: needs shape, as box(1, 1, 1) or minus(rounded_box(2, 1, 0.2, 0.1), text3d(\"SALE\", 0.45, 0.3))", t.id)) };
                    let tris = crate::shape::cached(&expr).map_err(|e| format!("mesh {}: {e}", t.id))?;
                    let mut m = Mesh::new(bevy::render::render_resource::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
                    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, tris.positions.clone());
                    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, tris.normals.clone());
                    // a picture map needs texture coordinates, which Manifold doesn't make (scene3d)
                    if vals.get("map").is_some_and(V::truthy) {
                        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, scene3d::box_uvs(&tris.positions, &tris.normals));
                    }
                    let mesh = world.resource_mut::<Assets<Mesh>>().add(m);
                    // the surface (emissive, opacity, unlit, clearcoat, a picture map...) and transform: scene3d
                    let mat = scene3d::material(world, t, inserted, &self.dir)?;
                    let mat = world.resource_mut::<Assets<StandardMaterial>>().add(mat);
                    // rot_x/y/z in radians and scale (or scale_x/y/z), as sync_meshes moves every mesh frame by frame
                    world.entity_mut(e).insert((Mesh3d(mesh), MeshMaterial3d(mat), scene3d::transform_of(vals)));
                }
                "panel" => {
                    // DOM on a surface (engine/docs/bevy.feature B4): a picture painted once (src: a PNG, such as the
                    // page a React DOM subtree painted) on a w x h quad in the scene. The GPU moves it, so its text is
                    // never re-rasterised under a changing transform (no wobble); unlit, so it reads as painted.
                    let src = match vals.get("src") { Some(V::S(s)) => s.clone(), _ => return Err(format!("panel {}: needs src (a picture)", t.id)) };
                    let path = resolve_src(inserted, &src, &self.dir)?;
                    let pic = image::open(&path).map_err(|e| format!("panel {}: {}: {e}", t.id, path.display()))?.to_rgba8();
                    let (pw, ph) = pic.dimensions();
                    // a panel is usually drawn smaller than it was painted: a full mip chain and trilinear, anisotropic
                    // sampling keep its text from shimmering as it moves (without them a 2x minified glyph flickers)
                    let (data, levels) = mip_chain(pic.into_raw(), pw, ph);
                    let mut img = Image::new(
                        bevy::render::render_resource::Extent3d { width: pw, height: ph, depth_or_array_layers: 1 },
                        bevy::render::render_resource::TextureDimension::D2,
                        data,
                        TextureFormat::Rgba8UnormSrgb,
                        bevy::asset::RenderAssetUsages::default(),
                    );
                    img.texture_descriptor.mip_level_count = levels;
                    img.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
                        anisotropy_clamp: 16,
                        ..bevy::image::ImageSamplerDescriptor::linear()
                    });
                    let tex = world.resource_mut::<Assets<Image>>().add(img);
                    let (w, h) = (g("w", 1.0), g("h", g("w", 1.0) * ph as f32 / pw.max(1) as f32));
                    let mesh = world.resource_mut::<Assets<Mesh>>().add(Mesh::from(Rectangle::new(w, h)));
                    let mat = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
                        base_color_texture: Some(tex),
                        unlit: true,
                        // premultiplied (mip_chain): interpolating straight colour and alpha apart and multiplying them
                        // after isn't linear in the sample position, so a moving edge's centroid wobbled (0.10 px)
                        alpha_mode: AlphaMode::Premultiplied,
                        cull_mode: None,
                        double_sided: true,
                        ..default()
                    });
                    world.entity_mut(e).insert((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_xyz(g("x", 0.0), g("y", 0.0), g("z", 0.0))));
                }
                "clip" if world.resource::<Composite>().0 => {} // a row only: clips_gpu reads it
                "clip" => {
                    if crate::shape::mask_of(vals).is_some() {
                        return Err(format!("clip {}: a mask needs the composite (kgpu) path, and this render draws clips as UI images", t.id));
                    }
                    let image = world.resource_mut::<Assets<Image>>().add(Image::new_fill(
                        bevy::render::render_resource::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                        bevy::render::render_resource::TextureDimension::D2,
                        &[0, 0, 0, 0],
                        TextureFormat::Rgba8UnormSrgb,
                        bevy::asset::RenderAssetUsages::default(),
                    ));
                    let Some(cam) = camera else { return Err("the edit needs a camera before its clips".into()) };
                    world.entity_mut(e).insert((
                        Node { position_type: PositionType::Absolute, ..default() },
                        ImageNode::new(image.clone()),
                        Clip { image, last: None },
                        Visibility::Hidden,
                        GlobalZIndex(0),
                        UiTargetCamera(cam),
                    ));
                }
                k if KINDS_2D.contains(&k) => {
                    let (id, row) = row2d(t, inserted);
                    rows2d.push((id, row, e));
                }
                _ => {} // data kinds (face, cut, ...): rows only, read by rules
            }
        }
        let edit = serde_json::json!({
            "fps": format!("{}", prog.fps.round() as u32), "width": prog.width, "height": prog.height, "frames": prog.frames.max(1),
            "instances": rows2d.iter().map(|(_, r, _)| r.clone()).collect::<Vec<_>>(),
        });
        let ids = raea::world::install_hosted(world, &edit, &self.dir, 1.0)?;
        world.resource_mut::<raea::world::Frame>().transparent = true;
        // a 2D row is raea's node: the row moves onto it, keeping its place in the edit
        let mut ents2d = Vec::new();
        for (id, _, tmp) in rows2d {
            let e = ids[&id];
            ents2d.push(e);
            store::adopt(world, tmp, e);
            let base = Base2d(world.get::<raea::world::Resolved>(e).map(|r| r.0.clone()).unwrap_or_default());
            world.entity_mut(e).insert(base);
        }
        let base = store::gather(world);
        let ix: HashMap<Entity, usize> = store::rows(world).iter().enumerate().map(|(r, &e)| (e, r)).collect();
        let media = inserted.iter().filter(|t| t.kind == "media").cloned().collect();
        let timeline = match timeline {
            Some(t) => t,
            None => std::sync::Arc::new(crate::timeline::Timeline::from_cols(prog, base)?),
        };
        // rows whose x or y move: their text paints smoothly (raea::world::mark_moving)
        raea::smooth_moving_text(true);
        for &e in &ents2d {
            if ix.get(&e).is_some_and(|&r| timeline.moves(r, &["x", "y"])) {
                raea::world::mark_moving(world, e);
            }
        }
        world.insert_resource(Edit { prog: prog.clone(), timeline, media });
        world.insert_resource(FrameRows { cols: eval::Cols::default(), ix });
        let Some(cam) = camera else { return Err("the edit needs a camera: INSERT INTO camera (id, ...) VALUES (...)".into()) };
        let models = world.query::<&scene3d::Model3d>().iter(world).count();
        let mut img = Image::new_target_texture(prog.width, prog.height, TextureFormat::Rgba8UnormSrgb, None);
        img.texture_descriptor.usage |= TextureUsages::COPY_DST;
        let overlay = world.resource_mut::<Assets<Image>>().add(img);
        world.insert_resource(Overlay(overlay.clone()));
        let (w, h) = (prog.width as f32, prog.height as f32);
        if !world.resource::<Composite>().0 {
            world.spawn((
                Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Px(w), height: Val::Px(h), ..default() },
                ImageNode::new(overlay),
                GlobalZIndex(OVERLAY_Z),
                UiTargetCamera(cam),
                Mirror,
            ));
        }
        world.spawn((
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(h), width: Val::Px(16.0), height: Val::Px(1.0), ..default() },
            BackgroundColor(Color::BLACK),
            FrameTag,
            UiTargetCamera(cam),
            Mirror,
        ));
        // glTF models loaded and set up before any frame draws
        if models > 0 {
            scene3d::await_models(&mut self.app)?;
        }
        Ok(())
    }

    fn tag_at(&self, d: &[u8]) -> Option<(u8, u8, u8)> {
        if self.nv12 {
            // the last word is the tag texel
            let n = nv12_words(self.w, self.h) * 4;
            return (d.len() >= n).then(|| (d[n - 4], d[n - 3], d[n - 2]));
        }
        let (w, h) = (self.w as usize, self.h as usize);
        let padded = (w * 4).div_ceil(256) * 256;
        let at = padded * h + 8 * 4;
        (d.len() >= at + 3).then(|| (d[at], d[at + 1], d[at + 2]))
    }

    /// One app.update(), timed; with RAEA_SERIAL set, then wait for the GPU and time the wait (what the GPU still
    /// had to do once the CPU had submitted: its un-overlapped cost).
    fn step(&mut self) {
        let t = std::time::Instant::now();
        self.app.update();
        let dt = t.elapsed().as_nanos() as u64;
        let mut wait = 0;
        if self.serial {
            let t = std::time::Instant::now();
            let dev = self.app.sub_app(bevy::render::RenderApp).world().resource::<bevy::render::renderer::RenderDevice>().clone();
            let _ = dev.wgpu_device().poll(bevy::render::render_resource::PollType::wait_indefinitely());
            wait = t.elapsed().as_nanos() as u64;
            // RAEA_GPU_TIMES: this frame's timestamps (outside the timed wait)
            let period = self.app.sub_app(bevy::render::RenderApp).world().resource::<bevy::render::renderer::RenderQueue>().0.get_timestamp_period() as f64;
            let rw = self.app.sub_app_mut(bevy::render::RenderApp).world_mut();
            if let Some(mut pool) = rw.get_resource_mut::<PoolGpu>() {
                if pool.ts_pending {
                    pool.ts_pending = false;
                    let m = pool.ts.as_ref().unwrap().2.clone();
                    m.slice(..).map_async(wgpu::MapMode::Read, |_| {});
                    let _ = dev.wgpu_device().poll(bevy::render::render_resource::PollType::wait_indefinitely());
                    let t: Vec<u64> = bytemuck::cast_slice(&m.slice(..).get_mapped_range()).to_vec();
                    m.unmap();
                    let ms = |a: usize, b: usize| t[b].saturating_sub(t[a]) as f64 * period / 1e6;
                    let up: f64 = (0..pool.ts_uploads as usize).map(|k| ms(8 + 2 * k, 9 + 2 * k)).sum();
                    let ov = if pool.ts_overlay { ms(6, 7) } else { 0.0 };
                    let (c, n) = (ms(2, 3), ms(4, 5));
                    pool.ts_ms[0] += up;
                    pool.ts_ms[1] += ov;
                    pool.ts_ms[2] += c;
                    pool.ts_ms[3] += n;
                    pool.ts_frames += 1;
                }
            }
        }
        let mut st = self.app.world_mut().resource_mut::<Stages>();
        st.update += dt;
        st.gpu_wait += wait;
        st.updates += 1;
    }

    /// A frame as straight RGBA, whichever way it came back.
    pub fn to_rgba(&self, px: &[u8]) -> Vec<u8> {
        if self.nv12 { raea_media::yuv::nv12_to_rgba(px, self.w as usize, self.h as usize) } else { px.to_vec() }
    }

    /// RAEA_GPU_TIMES with RAEA_SERIAL: GPU ms per timed frame for the clip uploads, the overlay's upload,
    /// composite.k and nv12buf.k, and the frames timed.
    pub fn gpu_stage_times(&self) -> Option<([f64; 4], u64)> {
        let p = self.app.sub_app(bevy::render::RenderApp).world().get_resource::<PoolGpu>()?;
        (p.ts_frames > 0).then(|| (p.ts_ms.map(|v| v / p.ts_frames as f64), p.ts_frames))
    }

    /// GPU pool: (frames uploaded, frames reused, bytes held).
    pub fn pool_stats(&self) -> (u64, u64, usize) {
        self.app.world().get_resource::<PoolBook>().map(|b| (b.uploads, b.hits, b.bytes)).unwrap_or_default()
    }

    /// The stage times so far.
    pub fn stages(&self) -> Stages {
        self.app.world().resource::<Stages>().clone()
    }

    fn unpad(&self, d: &[u8]) -> Vec<u8> {
        if self.nv12 {
            // NV12: the luma plane then the interleaved chroma plane, no padding (the tag word dropped)
            return d[..(self.w * self.h * 3 / 2) as usize].to_vec();
        }
        let (w, h) = (self.w as usize, self.h as usize);
        let (row_bytes, padded) = (w * 4, (w * 4).div_ceil(256) * 256);
        d.chunks(padded).take(h).flat_map(|r| &r[..row_bytes]).copied().collect()
    }

    /// One frame, straight RGBA8 (opaque), the careful way: two tagged readbacks in a row that agree (the first frame
    /// after a load, when meshes, materials and the overlay are new to the GPU).
    pub fn frame(&mut self, f: i64) -> Result<Vec<u8>, String> {
        self.app.world_mut().resource_mut::<Clock>().0 = f;
        let want = tag_colour(f, self.app.world().resource::<Gen>().0);
        let mut prev: Option<Vec<u8>> = None;
        for i in 0..240 {
            if std::env::var_os("RAEA_TRACE").is_some() {
                eprintln!("trace frame {f} update {i}");
            }
            self.step();
            if let Some(e) = self.app.world_mut().resource_mut::<Error>().0.take() {
                return Err(e);
            }
            let got = std::mem::take(&mut self.app.world_mut().resource_mut::<Out>().0);
            // the first frame after a load: readbacks the warm-up queued carry its tag but can predate parts that take a
            // few updates to show (a spot light's first frames drew without it), so the first few updates' are dropped
            if !self.warm && i < 4 {
                continue;
            }
            for d in got {
                if self.tag_at(&d) == Some(want) {
                    if prev.as_deref() == Some(&d[..]) {
                        if !self.warm {
                            mem_mark(&format!("engine: first frame back ({} updates)", i + 1));
                        }
                        self.warm = true;
                        return Ok(self.unpad(&d));
                    }
                    prev = Some(d);
                }
            }
        }
        Err(format!("frame {f}: no readback carried its tag"))
    }

    /// Frames in order, pipelined: frame k+1 is drawn while frame k's readback is on its way back, and each readback is
    /// matched to its frame by its tag, so the output is the same frames `frame` makes, one update per frame.
    pub fn frames(&mut self, list: &[i64], mut sink: impl FnMut(i64, Vec<u8>) -> Result<(), String>) -> Result<(), String> {
        let mut start = 0;
        if !self.warm {
            let Some(&f0) = list.first() else { return Ok(()) };
            sink(f0, self.frame(f0)?)?;
            start = 1;
        }
        let generation = self.app.world().resource::<Gen>().0;
        let mut got: HashMap<(u8, u8, u8), Vec<u8>> = HashMap::new();
        let (mut next, mut sent, mut idle) = (start, start, 0);
        while next < list.len() {
            if sent < list.len() && sent < next + 3 {
                self.app.world_mut().resource_mut::<Clock>().0 = list[sent];
                sent += 1;
            }
            if std::env::var_os("RAEA_TRACE").is_some() {
                eprintln!("trace pipelined next {} sent {sent}", list[next]);
            }
            self.step();
            if let Some(e) = self.app.world_mut().resource_mut::<Error>().0.take() {
                return Err(e);
            }
            for d in std::mem::take(&mut self.app.world_mut().resource_mut::<Out>().0) {
                if let Some(t) = self.tag_at(&d) {
                    got.insert(t, d);
                }
            }
            idle += 1;
            while next < sent {
                let Some(d) = got.remove(&tag_colour(list[next], generation)) else { break };
                let t = std::time::Instant::now();
                let px = self.unpad(&d);
                {
                    let mut st = self.app.world_mut().resource_mut::<Stages>();
                    st.readback_copy += t.elapsed().as_nanos() as u64;
                    st.frames += 1;
                }
                sink(list[next], px)?;
                if next % mark_every() == 0 {
                    mem_mark(&format!("engine: frame {} sent on", list[next]));
                }
                next += 1;
                idle = 0;
            }
            if idle > 30 {
                // a readback went missing: that frame the careful way, then carry on
                let f = list[next];
                sink(f, self.frame(f)?)?;
                next += 1;
                sent = sent.max(next);
                idle = 0;
            }
        }
        Ok(())
    }
}


/// Bevy's render diagnostics (each pass's elapsed time, `render/...`), into the run's trace once a second.
fn forward_render_diagnostics(store: Res<bevy::diagnostic::DiagnosticsStore>, mut last: Local<Option<std::time::Instant>>) {
    if last.is_some_and(|t| t.elapsed().as_secs_f64() < 1.0) {
        return;
    }
    *last = Some(std::time::Instant::now());
    let passes: serde_json::Map<String, serde_json::Value> = store
        .iter()
        .filter(|d| d.path().as_str().starts_with("render/"))
        .filter_map(|d| d.smoothed().map(|v| (d.path().as_str().trim_start_matches("render/").to_string(), serde_json::json!((v * 1000.0).round() / 1000.0))))
        .collect();
    if !passes.is_empty() {
        raea_gputrace::trace::event("bevy_passes", serde_json::json!({"ms": passes}));
    }
}
