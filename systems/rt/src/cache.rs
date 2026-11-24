//! raea GI's world cache, its buffers and passes (docs/realism.feature LT3; cache.wesl, compact.wesl, and gi.wesl's
//! `learn` and `blend`): 2^20 cells of 88 bytes (88 MiB) on the surfaces rays hit. Each frame, before the probes, every
//! cell ages and the live ones are listed (compact.wesl); about RAEA_GI_UPDATES of them (40000) learn one bounce more
//! and take it into their means. The probes then read it where their rays hit (gi.wesl). The list's length becomes the
//! update's dispatch on the GPU: wgpu won't take a buffer as a dispatch's arguments while that dispatch binds it for
//! writing, so the arguments are copied out between the two passes (`args`), as Solari's world cache does.
//! RAEA_GI_CELL: the smallest cell (m, 0.5; Solari's 0.15 suits screen pixels, but 131072 probe rays a frame filled all
//! 2^20 cells on Berlin's walk, where 0.5 peaks at 480k and costs 0.51 ms against 0.93). RAEA_GI_CELL_DOUBLE: the
//! distance from the eye over which a cell's size doubles (m, 15). RAEA_GI_HISTORY: the most frames a cell's mean holds
//! (32).

use bevy::{
    prelude::*,
    render::{
        render_resource::{
            binding_types::{storage_buffer_sized, uniform_buffer},
            *,
        },
        renderer::RenderDevice,
    },
};

/// cells in all: 1024 blocks of 1024 (cache.wesl's CELLS; compact.wesl lists them in two levels)
pub(crate) const CELLS: u64 = 1 << 20;

/// the cache's buffers in binding order, each `(label, bytes)`: a cell's 88 bytes are a checksum, a life, its light,
/// where it is and its side, its light's change, its place in the list's block sum, its place in the list, its new
/// light, and how much of the time it sees the sun (so a pixel can take the sun out: raea-sys-gi, whose raster has it)
const BUFFERS: [(&str, u64); 12] = [
    ("raea gi cache checksums", 4 * CELLS),
    ("raea gi cache life", 4 * CELLS),
    ("raea gi cache radiance", 16 * CELLS),
    ("raea gi cache cells", 32 * CELLS),
    ("raea gi cache deltas", 4 * CELLS),
    ("raea gi cache a", 4 * CELLS),
    ("raea gi cache b", 4 * (CELLS / 1024)),
    ("raea gi cache live", 4 * CELLS),
    ("raea gi cache count", 4),
    ("raea gi cache dispatch", 16),
    ("raea gi cache learned", 16 * CELLS),
    ("raea gi cache sunlit", 4 * CELLS),
];
/// where the list's length and its dispatch are among BUFFERS
pub(crate) const COUNT: usize = 8;
const DISPATCH: usize = 9;

/// cache.wesl's Upkeep: the eye; the smallest cell, the distance a cell's size doubles over, the most frames a mean
/// holds; the frame's salt, the cells a frame aims to update, 1 to forget every mean
#[derive(ShaderType, Clone, Copy)]
pub(crate) struct Upkeep {
    eye: Vec4,
    size: Vec4,
    run: UVec4,
}

impl Upkeep {
    pub(crate) fn new(eye: Vec3, frame: u32, reset: bool) -> Self {
        let env = |name: &str, or: f32| std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(or);
        let updates = std::env::var("RAEA_GI_UPDATES").ok().and_then(|v| v.parse().ok()).unwrap_or(40000u32);
        Upkeep {
            eye: eye.extend(0.0),
            size: Vec4::new(env("RAEA_GI_CELL", 0.5), env("RAEA_GI_CELL_DOUBLE", 15.0), env("RAEA_GI_HISTORY", 32.0).max(1.0), 0.0),
            run: UVec4::new(frame.wrapping_mul(0x9e37_79b9), updates, reset as u32, 0),
        }
    }
}

/// the cache's bindings: group 1 of every pass that reads or keeps it (raea-sys-gi's screen pass among them)
pub fn layout() -> BindGroupLayoutDescriptor {
    let s = || storage_buffer_sized(false, None);
    BindGroupLayoutDescriptor::new("raea_gi_cache_layout", &BindGroupLayoutEntries::sequential(ShaderStages::COMPUTE, (s(), s(), s(), s(), s(), s(), s(), s(), s(), s(), s(), s(), uniform_buffer::<Upkeep>(false))))
}

/// the cache's six passes, in the order they run
pub(crate) struct Passes(pub [CachedComputePipelineId; 6]);

/// decay, the two sums and the list (compact.wesl), then learn and blend (gi.wesl), each on the probes' group 0 and the
/// cache's group 1
pub(crate) fn queue(cache: &PipelineCache, assets: &AssetServer, layouts: &[BindGroupLayoutDescriptor]) -> Passes {
    let (compact, gi) = (assets.load("embedded://raea_sys_rt/compact.wesl"), assets.load("embedded://raea_sys_rt/gi.wesl"));
    Passes([(&compact, "decay"), (&compact, "scan_cells"), (&compact, "scan_blocks"), (&compact, "list"), (&gi, "learn"), (&gi, "blend")].map(|(shader, entry)| pipeline(cache, "raea_gi_cache", layouts, shader, entry)))
}

/// One of raea GI's compute passes: `entry` of `shader` over `layouts`, labelled `{label}_{entry}`.
pub(crate) fn pipeline(cache: &PipelineCache, label: &str, layouts: &[BindGroupLayoutDescriptor], shader: &Handle<Shader>, entry: &str) -> CachedComputePipelineId {
    cache.queue_compute_pipeline(ComputePipelineDescriptor { label: Some(format!("{label}_{entry}").into()), layout: layouts.to_vec(), shader: shader.clone(), entry_point: Some(entry.to_owned().into()), ..default() })
}

/// The cache's buffers (zeroed: every cell empty) and the update's arguments, copied from the list's.
pub(crate) struct Buffers {
    pub all: Vec<Buffer>,
    args: Buffer,
}

pub(crate) fn buffers(device: &RenderDevice) -> Buffers {
    let all = BUFFERS.map(|(label, size)| device.create_buffer(&BufferDescriptor { label: Some(label), size, usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC, mapped_at_creation: false })).to_vec();
    let args = device.create_buffer(&BufferDescriptor { label: Some("raea gi cache args"), size: 16, usage: BufferUsages::INDIRECT | BufferUsages::COPY_DST, mapped_at_creation: false });
    Buffers { all, args }
}

pub(crate) fn bind(device: &RenderDevice, layout: &BindGroupLayout, b: &Buffers, upkeep: BindingResource) -> BindGroup {
    // the buffers in order, then the upkeep uniform
    let resources = b.all.iter().map(|b| b.as_entire_binding()).chain([upkeep]);
    let entries: Vec<BindGroupEntry> = resources.enumerate().map(|(i, resource)| BindGroupEntry { binding: i as u32, resource }).collect();
    device.create_bind_group("raea_gi_cache", layout, &entries)
}

/// A frame of the cache into `enc`: the upkeep pass, the update's arguments copied out, the update pass. Both groups
/// are set on each; `times` are the timestamps at the upkeep's start and the update's end.
pub(crate) fn run(enc: &mut wgpu::CommandEncoder, p: &[&ComputePipeline; 6], groups: [&BindGroup; 2], b: &Buffers, times: Option<(&wgpu::QuerySet, u32)>) {
    let blocks = (CELLS / 1024) as u32;
    {
        let writes = times.map(|(set, at)| wgpu::ComputePassTimestampWrites { query_set: set, beginning_of_pass_write_index: Some(at), end_of_pass_write_index: None });
        let mut pass = enc.begin_compute_pass(&ComputePassDescriptor { label: Some("raea_gi_cache_upkeep"), timestamp_writes: writes });
        pass.set_bind_group(0, groups[0], &[]);
        pass.set_bind_group(1, groups[1], &[]);
        for (pipeline, n) in [(p[0], blocks), (p[1], blocks), (p[2], 1), (p[3], blocks)] {
            pass.set_pipeline(pipeline);
            pass.dispatch_workgroups(n, 1, 1);
        }
    }
    enc.copy_buffer_to_buffer(&b.all[DISPATCH], 0, &b.args, 0, 12);
    let writes = times.map(|(set, at)| wgpu::ComputePassTimestampWrites { query_set: set, beginning_of_pass_write_index: None, end_of_pass_write_index: Some(at + 1) });
    let mut pass = enc.begin_compute_pass(&ComputePassDescriptor { label: Some("raea_gi_cache_update"), timestamp_writes: writes });
    pass.set_bind_group(0, groups[0], &[]);
    pass.set_bind_group(1, groups[1], &[]);
    for pipeline in [p[4], p[5]] {
        pass.set_pipeline(pipeline);
        pass.dispatch_workgroups_indirect(&b.args, 0);
    }
}
