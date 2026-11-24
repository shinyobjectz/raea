//! The lighting pass (light.wgsl) in Bevy's render world: after Bevy's opaque pass has filled the depth and deferred
//! prepasses, a compute pass lights every pixel into the camera's HDR main texture, which Bevy then tonemaps. The
//! BVH's buffers are made once; the lights and the parameters each frame.

use crate::scene::{GpuLight, SceneGpu};
use bevy::{
    core_pipeline::{
        core_3d::{main_opaque_pass_3d, main_transparent_pass_3d},
        prepass::ViewPrepassTextures,
        schedule::Core3d,
        Core3dSystems,
    },
    prelude::*,
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_resource::{
            binding_types::{storage_buffer_read_only_sized, storage_buffer_sized, texture_2d, texture_depth_2d, texture_storage_2d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        view::{ExtractedView, ViewTarget},
        RenderApp, RenderStartup,
    },
};
use std::sync::{atomic::{AtomicU32, Ordering}, Arc};

/// Frames the lighting pass has drawn, so a headless shot waits for its pipeline rather than for a number of frames.
pub static DRAWN: AtomicU32 = AtomicU32::new(0);

/// The last frame read back (timing runs only): shadow rays, lit pixels, the most rays one pixel traced.
pub static RAYS: [AtomicU32; 3] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];

/// The lighting pass's GPU time (timing runs, where the GPU has timestamps): nanoseconds summed, and frames timed.
pub static GPU_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static GPU_FRAMES: AtomicU32 = AtomicU32::new(0);

/// What this frame draws: the lights where they are now, and the switches.
#[derive(Resource, Clone, Default, ExtractResource)]
#[extract_app(RenderApp)]
pub struct Frame {
    pub lights: Vec<GpuLight>,
    /// 0 every light, 1 uniform, 2 weighted
    pub mode: u32,
    pub k: u32,
    /// 0 lit, 1 normal, 2 albedo, 3 depth, 4 rays, 5 history, 6 lod, 7 bounce
    pub view: u32,
    pub frame: u32,
    pub exposure: f32,
    pub brute: bool,
    /// no history (--raw)
    pub raw: bool,
    /// the camera cut this frame: the history starts again
    pub cut: bool,
    /// the history's length in frames
    pub history: u32,
    /// the light grid (grid.rs): its corner and cell size, its cells along each axis and whether to use it, its lists
    pub grid_min: Vec4,
    pub grid_dims: UVec4,
    /// the grid's lists in one: the cells' starts (counted from the buffer's beginning), then the lights' indices
    pub grid: Vec<u32>,
    /// diffuse bounces a pixel follows (LB5)
    pub bounces: u32,
    /// the sun (LB6): the unit direction toward it and its angular radius; its irradiance (zero: none); the sky's
    /// radiance, which bounce rays that leave the scene bring back
    pub sun: Vec4,
    pub sun_e: Vec4,
    pub sky: Vec4,
    /// the bounce reads the radiance cache (LB7); a cell's size at 1 m from the camera
    pub cache: bool,
    pub cell: f32,
    /// the bounce traced for one pixel of each 2 x 2 block and upsampled (LB7)
    pub half: bool,
}

pub struct LightPass;

impl Plugin for LightPass {
    fn build(&self, app: &mut App) {
        app.init_resource::<Frame>().add_plugins((ExtractResourcePlugin::<Frame>::default(), ExtractResourcePlugin::<SceneGpu>::default()));
        let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
        // timing a run (--bench): each frame waits for the GPU, so the frame time counts the GPU's work too
        if crate::timing() {
            render.add_systems(bevy::render::Render, wait.in_set(bevy::render::RenderSystems::Cleanup));
        }
        render.init_resource::<Buffers>().add_systems(RenderStartup, init).add_systems(
            Core3d,
            light.in_set(Core3dSystems::MainPass).after(main_opaque_pass_3d).before(main_transparent_pass_3d),
        );
    }
}

#[derive(ShaderType, Clone, Copy)]
struct Params {
    world_from_clip: Mat4,
    eye: Vec4,
    size: UVec4,
    mode: UVec4,
    prev_clip_from_world: Mat4,
    prev_eye: Vec4,
    temporal: UVec4,
    grid_min: Vec4,
    grid_dims: UVec4,
    gi: UVec4,
    sun: Vec4,
    sun_e: Vec4,
    sky: Vec4,
    cache: Vec4,
}

#[derive(Resource)]
struct Pipeline {
    layout: BindGroupLayoutDescriptor,
    id: CachedComputePipelineId,
    /// the radiance cache's resolve, after the pixels
    resolve: CachedComputePipelineId,
    /// the half-resolution bounce, before them
    half: CachedComputePipelineId,
}

/// The BVH on the GPU, made from the SceneGpu it came from (by its pointer); the history's two pairs of textures
/// (light, surface), read one frame and written the next; and the last frame's camera.
#[derive(Resource, Default)]
struct Buffers {
    from: Option<(usize, Buffer, Buffer)>,
    history: Option<(UVec2, [(Texture, TextureView); 4])>,
    prev: Option<(Mat4, Vec3)>,
    /// the ray counts the shader adds to, and where a timing run reads them back
    stats: Option<(Buffer, Buffer)>,
    /// the pass's start and end timestamps, resolved, read back; nanoseconds a tick
    timer: Option<(wgpu::QuerySet, wgpu::Buffer, wgpu::Buffer, f64)>,
    /// the radiance cache (cache.wgsl): its cells and their light
    cache: Option<(Buffer, Buffer)>,
    /// the half-resolution bounce (half.wgsl), one vec4 a 2 x 2 block
    half: Option<Buffer>,
}

/// The radiance cache's slots (cache.wgsl's CELLS) and the words each takes.
const CELLS: u64 = 262144;

fn init(mut commands: Commands, assets: Res<AssetServer>, cache: Res<PipelineCache>) {
    let read = |n: u64| storage_buffer_read_only_sized(false, std::num::NonZeroU64::new(n));
    let layout = BindGroupLayoutDescriptor::new(
        "lightbench_light",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                uniform_buffer::<Params>(false),
                texture_depth_2d(),
                texture_2d(TextureSampleType::Uint),
                read(32),
                read(48),
                read(32),
                texture_storage_2d(TextureFormat::Rgba16Float, StorageTextureAccess::WriteOnly),
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_storage_2d(TextureFormat::Rgba32Float, StorageTextureAccess::WriteOnly),
                texture_storage_2d(TextureFormat::Rgba16Float, StorageTextureAccess::WriteOnly),
                storage_buffer_sized(false, std::num::NonZeroU64::new(16)),
                read(4),
                storage_buffer_sized(false, std::num::NonZeroU64::new(16)),
                storage_buffer_sized(false, std::num::NonZeroU64::new(24)),
                storage_buffer_sized(false, std::num::NonZeroU64::new(16)),
            ),
        ),
    );
    // the lighting, the rays it traces and the radiance cache, one module
    let shader = assets.add(bevy::shader::Shader::from_wgsl(concat!(include_str!("light.wgsl"), "\n", include_str!("trace.wgsl"), "\n", include_str!("cache.wgsl"), "\n", include_str!("half.wgsl")), "lightbench/light.wgsl"));
    // three entry points of one module, on one layout
    let base = ComputePipelineDescriptor { layout: vec![layout.clone()], shader, ..default() };
    let queue = |entry: &'static str| cache.queue_compute_pipeline(ComputePipelineDescriptor { label: Some(format!("lightbench_{entry}").into()), entry_point: Some(entry.into()), ..base.clone() });
    let (id, resolve, half) = (queue("main"), queue("resolve"), queue("trace_half"));
    commands.insert_resource(Pipeline { layout, id, resolve, half });
}

#[allow(clippy::too_many_arguments)]
/// A 16-byte buffer: the ray counts and the timer's resolve and read-back.
fn small(d: &wgpu::Device, label: &str, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    d.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: 16, usage, mapped_at_creation: false })
}

fn light(
    view: ViewQuery<(&ExtractedView, &ViewTarget, Option<&ViewPrepassTextures>)>,
    p: Option<Res<Pipeline>>,
    scene: Option<Res<SceneGpu>>,
    frame: Res<Frame>,
    mut bufs: ResMut<Buffers>,
    cache: Res<PipelineCache>,
    queue: Res<RenderQueue>,
    mut ctx: RenderContext,
) {
    let (Some(p), Some(scene)) = (p, scene) else { return };
    let (extracted, target, prepass) = view.into_inner();
    let (Some(pipeline), Some(resolve), Some(half)) = (cache.get_compute_pipeline(p.id), cache.get_compute_pipeline(p.resolve), cache.get_compute_pipeline(p.half)) else { return };
    let Some(prepass) = prepass else { return };
    let (Some(depth), Some(gbuf)) = (prepass.depth_only_view(), prepass.deferred_view()) else { return };
    if target.main_texture_format() != TextureFormat::Rgba16Float || frame.lights.is_empty() {
        return;
    }
    let device: RenderDevice = ctx.render_device().clone();
    let key = Arc::as_ptr(&scene.tris) as usize;
    if bufs.from.as_ref().map(|b| b.0) != Some(key) {
        let make = |label: &str, bytes: &[u8]| device.create_buffer_with_data(&BufferInitDescriptor { label: Some(label), contents: bytes, usage: BufferUsages::STORAGE });
        let nodes = make("lightbench_nodes", bytemuck::cast_slice(&scene.nodes));
        let tris = make("lightbench_tris", bytemuck::cast_slice(&scene.tris));
        bufs.from = Some((key, nodes, tris));
    }
    let lights = device.create_buffer_with_data(&BufferInitDescriptor { label: Some("lightbench_lights"), contents: bytemuck::cast_slice(&frame.lights), usage: BufferUsages::STORAGE });
    let list = |label: &str, v: &[u32]| device.create_buffer_with_data(&BufferInitDescriptor { label: Some(label), contents: bytemuck::cast_slice(if v.is_empty() { &[0u32][..] } else { v }), usage: BufferUsages::STORAGE });
    let grid = list("lightbench_grid", &frame.grid);
    let size = target.main_texture().size();
    let want = UVec2::new(size.width, size.height);
    let fresh = bufs.history.as_ref().map(|h| h.0) != Some(want);
    if fresh {
        let make = |i: usize| {
            let t = device.create_texture(&TextureDescriptor {
                label: Some(["lightbench_hist_a", "lightbench_geo_a", "lightbench_hist_b", "lightbench_geo_b"][i]),
                size: Extent3d { width: want.x, height: want.y, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                // the light in f32 (a long history's 1 / n steps vanish in f16), the surface in f16
                format: if i % 2 == 0 { TextureFormat::Rgba32Float } else { TextureFormat::Rgba16Float },
                usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let v = t.create_view(&TextureViewDescriptor::default());
            (t, v)
        };
        bufs.history = Some((want, [make(0), make(1), make(2), make(3)]));
        // a block's bounce, a 2 x 2 block a vec4
        let blocks = (want.x.div_ceil(2) * want.y.div_ceil(2)) as u64;
        bufs.half = Some(device.create_buffer(&BufferDescriptor { label: Some("lightbench_bounce_half"), size: blocks * 16, usage: BufferUsages::STORAGE, mapped_at_creation: false }));
    }
    let world_from_clip = extracted.world_from_view.to_matrix() * extracted.clip_from_view.inverse();
    let clip_from_world = extracted.clip_from_view * extracted.world_from_view.to_matrix().inverse();
    let eye = extracted.world_from_view.translation();
    let (prev_clip_from_world, prev_eye) = bufs.prev.unwrap_or((clip_from_world, eye));
    let reset = fresh || frame.cut || bufs.prev.is_none();
    bufs.prev = Some((clip_from_world, eye));
    if bufs.stats.is_none() {
        let make = |label: &str, usage: BufferUsages| Buffer::from(small(device.wgpu_device(), label, usage));
        bufs.stats = Some((make("lightbench_stats", BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC), make("lightbench_stats_read", BufferUsages::MAP_READ | BufferUsages::COPY_DST)));
    }
    if bufs.timer.is_none() && crate::timing() && device.features().contains(WgpuFeatures::TIMESTAMP_QUERY) {
        let d = device.wgpu_device();
        let set = d.create_query_set(&wgpu::QuerySetDescriptor { label: Some("lightbench_timer"), ty: wgpu::QueryType::Timestamp, count: 2 });
        let resolve = small(d, "lightbench_timer_resolve", wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC);
        let read = small(d, "lightbench_timer_read", wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ);
        bufs.timer = Some((set, resolve, read, queue.get_timestamp_period() as f64));
    }
    if bufs.cache.is_none() {
        let make = |label: &str, size: u64| device.create_buffer(&BufferDescriptor { label: Some(label), size, usage: BufferUsages::STORAGE, mapped_at_creation: false });
        bufs.cache = Some((make("lightbench_cells", CELLS * 6 * 4), make("lightbench_cell_light", CELLS * 16)));
    }
    let (cells, cell_light) = bufs.cache.as_ref().unwrap();
    let half_res = frame.half && (frame.cache || frame.bounces > 0);
    let (stats, readback) = bufs.stats.as_ref().unwrap();
    queue.write_buffer(stats, 0, &[0u8; 16]);
    let (_, nodes, tris) = bufs.from.as_ref().unwrap();
    // even frames read pair a and write pair b, odd frames the other way
    let h = &bufs.history.as_ref().unwrap().1;
    let (r, w) = if frame.frame % 2 == 0 { (0, 2) } else { (2, 0) };
    let mut params = UniformBuffer::from(Params {
        world_from_clip,
        eye: extracted.world_from_view.translation().extend(frame.exposure),
        size: UVec4::new(size.width, size.height, frame.lights.len() as u32, frame.frame),
        mode: UVec4::new(frame.mode, frame.k, frame.view, frame.brute as u32),
        prev_clip_from_world,
        prev_eye: prev_eye.extend(0.0),
        temporal: UVec4::new(frame.raw as u32, reset as u32, frame.history.max(1), 0),
        grid_min: frame.grid_min,
        grid_dims: frame.grid_dims,
        gi: UVec4::new(frame.bounces, frame.cache as u32, half_res as u32, 0),
        sun: frame.sun,
        sun_e: frame.sun_e,
        sky: frame.sky,
        cache: Vec4::new(frame.cell, 4096.0, 0.0, 0.0),
    });
    params.write_buffer(&device, &queue);
    let group = device.create_bind_group(
        "lightbench_light",
        &cache.get_bind_group_layout(&p.layout),
        &BindGroupEntries::sequential((params.binding().unwrap(), depth, gbuf, nodes.as_entire_binding(), tris.as_entire_binding(), lights.as_entire_binding(), target.main_texture_view(), &h[r].1, &h[r + 1].1, &h[w].1, &h[w + 1].1, stats.as_entire_binding(), grid.as_entire_binding(), bufs.half.as_ref().unwrap().as_entire_binding(), cells.as_entire_binding(), cell_light.as_entire_binding())),
    );
    let encoder = ctx.command_encoder();
    // timed at the pass's own start and end: Apple's GPUs take timestamps there, not inside passes or between them
    let timer = bufs.timer.as_ref();
    let writes = timer.map(|(set, ..)| wgpu::ComputePassTimestampWrites { query_set: set, beginning_of_pass_write_index: Some(0), end_of_pass_write_index: Some(1) });
    let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor { label: Some("lightbench_light"), timestamp_writes: writes });
    pass.set_bind_group(0, &group, &[]);
    // the bounce at half resolution first, so the pixels can take it up
    if half_res {
        pass.set_pipeline(half);
        pass.dispatch_workgroups(size.width.div_ceil(2).div_ceil(8), size.height.div_ceil(2).div_ceil(8), 1);
    }
    pass.set_pipeline(pipeline);
    pass.dispatch_workgroups(size.width.div_ceil(8), size.height.div_ceil(8), 1);
    // a dispatch is its own usage scope, so the resolve sees every pixel's samples
    if frame.cache {
        pass.set_pipeline(resolve);
        pass.dispatch_workgroups((CELLS / 64) as u32, 1, 1);
    }
    drop(pass);
    encoder.copy_buffer_to_buffer(stats, 0, readback, 0, 16);
    if let Some(t) = timer {
        encoder.resolve_query_set(&t.0, 0..2, &t.1, 0);
        encoder.copy_buffer_to_buffer(&t.1, 0, &t.2, 0, 16);
    }
    DRAWN.fetch_add(1, Ordering::Relaxed);
}

fn wait(device: Res<RenderDevice>, bufs: Res<Buffers>) {
    let _ = device.poll(PollType::wait_indefinitely());
    // the frame is done: read its ray counts back
    let Some((_, readback)) = bufs.stats.as_ref() else { return };
    let slice = readback.slice(..);
    slice.map_async(MapMode::Read, |_| {});
    let _ = device.poll(PollType::wait_indefinitely());
    {
        let Ok(data) = slice.get_mapped_range() else { return };
        let v: &[u32] = bytemuck::cast_slice(&data);
        for (k, n) in RAYS.iter().enumerate() {
            n.store(v[k], Ordering::Relaxed);
        }
    }
    readback.unmap();
    let Some((_, _, read, period)) = bufs.timer.as_ref() else { return };
    let slice = read.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(PollType::wait_indefinitely());
    {
        let Ok(data) = slice.get_mapped_range() else { return };
        let t: &[u64] = bytemuck::cast_slice(&data);
        if t[1] > t[0] {
            GPU_NS.fetch_add(((t[1] - t[0]) as f64 * period) as u64, Ordering::Relaxed);
            GPU_FRAMES.fetch_add(1, Ordering::Relaxed);
        }
    }
    read.unmap();
}
