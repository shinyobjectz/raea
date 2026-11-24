//! raea GI's screen side (docs/realism.feature LT8, D6): the indirect diffuse light each pixel gets, drawn by the main
//! pass in place of the irradiance volume, the environment map's diffuse and the ambient light (raea's bevy_pbr
//! RaeaGiScreen, the mesh view group's binding 41, read texel for pixel: no sampler, so a hooked material at Metal's 16
//! keeps building, where Bevy's irradiance volume took a 17th).
//!
//! Screen probes (probes.wesl): one every 16 pixels, on the surface its tile shows, tracing 16 rays a frame through
//! raea-sys-rt's ray scene with gi.wesl's own functions, a hit lit by what the world cache has learned there; each
//! keeps a running mean, its history found where it stood last frame, and is filtered with its neighbours on its own
//! surface (probe space). The screen pass (screen.wesl) fills the pixels between from the four probes around each, by
//! depth and normal; where none is on a pixel's surface (a thing smaller
//! than a tile), it reads the cache at the pixel's own surface less the sun's share (LT3's measuring step), and where
//! that cell is new the main pass draws Bevy's own light. Not yet (LT8): the screen trace before the TLAS, a quarter
//! of the probes refreshed a frame.
//!
//! On with RAEA_GI=1, on every 3D camera, which gets its depth and normal prepasses, with MSAA off. The screen passes'
//! GPU milliseconds go into RAEA_GI_STATS beside the cache's (raea-sys-rt gi.rs SCREEN_MS), a frame timed at a time.

use bevy::{
    core_pipeline::{
        prepass::{DepthPrepass, NormalPrepass, ViewPrepassTextures},
        schedule::Core3d,
        Core3dSystems,
    },
    diagnostic::FrameCount,
    pbr::RaeaGiScreen,
    platform::collections::HashMap,
    prelude::*,
    render::{
        camera::ExtractedCamera,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_resource::{
            binding_types::{storage_buffer_read_only_sized, storage_buffer_sized, texture_2d, texture_depth_2d, texture_storage_2d, uniform_buffer, uniform_buffer_sized},
            *,
        },
        renderer::{RenderContext, RenderDevice, ViewQuery},
        texture::{CachedTexture, TextureCache},
        view::{ExtractedView, ViewUniform, ViewUniformOffset, ViewUniforms},
        Render, RenderApp, RenderStartup, RenderSystems,
    },
};
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};

use raea_sys_rt::gi::WorldCache;

/// A camera whose pixels take raea GI's light (RAEA_GI=1). Brings the prepasses it reads.
#[derive(Component, Clone, Copy, Default, ExtractComponent)]
#[extract_app(bevy::render::RenderApp)]
#[require(DepthPrepass, NormalPrepass)]
pub struct GiScreen;

/// the screen's format: the light arriving (rgb), and 1 where a cell had any (a)
const FORMAT: TextureFormat = TextureFormat::Rgba16Float;

pub fn install(app: &mut App) {
    if std::env::var("RAEA_GI").as_deref() != Ok("1") {
        return;
    }
    bevy::shader::load_shader_library!(app, "frame.wesl");
    bevy::asset::embedded_asset!(app, "probes.wesl");
    bevy::asset::embedded_asset!(app, "screen.wesl");
    app.add_plugins(ExtractComponentPlugin::<GiScreen>::default()).add_systems(PostUpdate, cameras);
    let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
    render
        .init_resource::<Kept>()
        .add_systems(RenderStartup, (init, clock))
        .add_systems(Render, textures.in_set(RenderSystems::PrepareResources))
        .add_systems(Core3d, draw.after(Core3dSystems::Prepass).before(Core3dSystems::MainPass));
}

/// Every 3D camera takes the screen's light, single-sampled: PrepassRead keeps MSAA off and the prepasses on whatever
/// the camera's row says (raea-sys-post's msaa_for_3d and sync).
fn cameras(mut commands: Commands, q: Query<Entity, (With<Camera3d>, Without<GiScreen>)>) {
    for e in &q {
        commands.entity(e).insert((GiScreen, raea_sys_compose::passes::PrepassRead, Msaa::Off));
    }
}

/// pixels a probe, across and down (frame.wesl's TILE); bytes a probe (its Probe: three vec4s)
const TILE: u32 = 16;
const PROBE_BYTES: u64 = 48;

/// a view's screen, its probes (this frame's, last frame's, and this frame's filtered) and this frame's parameters
#[derive(Component)]
struct GiFrame {
    screen: CachedTexture,
    params: Buffer,
    now: Buffer,
    then: Buffer,
    soft: Buffer,
    probes: u32,
}

/// Each view's probes between frames: how many across and down, the two buffers they swap between, which is this
/// frame's, the filtered probes, and the view's clip_from_world last frame.
#[derive(Resource, Default)]
struct Kept(HashMap<Entity, Probes>);
struct Probes {
    tiles: UVec2,
    a: Buffer,
    b: Buffer,
    soft: Buffer,
    flip: bool,
    then_clip: Option<Mat4>,
}

fn probes(device: &RenderDevice, tiles: UVec2) -> Probes {
    let buffer = |label| device.create_buffer(&BufferDescriptor { label: Some(label), size: (tiles.x * tiles.y) as u64 * PROBE_BYTES, usage: BufferUsages::STORAGE, mapped_at_creation: false });
    Probes { tiles, a: buffer("raea_gi_probes_a"), b: buffer("raea_gi_probes_b"), soft: buffer("raea_gi_probes_soft"), flip: false, then_clip: None }
}

/// frame.wesl's Params: toward the sun, the sun's light (lux), the run (the frame's salt, probes across and down, 1
/// when last frame's probes are there to take from), and last frame's clip_from_world
const PARAMS_BYTES: u64 = 112;

/// Each view's screen, at its target's size, its probes and the lighting's binding to the screen. None until the world
/// cache has run (its first frame), so the main pass draws Bevy's own light until then.
#[allow(clippy::too_many_arguments)]
fn textures(
    mut commands: Commands,
    mut cache: ResMut<TextureCache>,
    mut kept: ResMut<Kept>,
    device: Res<RenderDevice>,
    frame: Res<FrameCount>,
    world: Option<Res<WorldCache>>,
    clock: Option<Res<Clock>>,
    views: Query<(Entity, &ExtractedCamera, &ExtractedView), With<GiScreen>>,
) {
    if let Some(c) = clock {
        c.read();
    }
    let Some(world) = world else { return };
    kept.0.retain(|e, _| views.contains(*e));
    let (d, l) = (world.sun_dir, world.sun_light);
    for (e, camera, view) in &views {
        let Some(size) = camera.physical_target_size else { continue };
        let tiles = (size + UVec2::splat(TILE - 1)) / TILE;
        let kept = kept.0.entry(e).or_insert_with(|| probes(&device, tiles));
        if kept.tiles != tiles {
            *kept = probes(&device, tiles);
        }
        kept.flip = !kept.flip;
        let (now, then) = if kept.flip { (kept.a.clone(), kept.b.clone()) } else { (kept.b.clone(), kept.a.clone()) };
        let clip = view.clip_from_world.unwrap_or_else(|| view.clip_from_view * Mat4::from(view.world_from_view.affine().inverse()));
        let then_clip = kept.then_clip.replace(clip);
        let mut words = vec![d.x.to_bits(), d.y.to_bits(), d.z.to_bits(), 0, l.x.to_bits(), l.y.to_bits(), l.z.to_bits(), 0, frame.0.wrapping_mul(0x9e37_79b9), tiles.x, tiles.y, then_clip.is_some() as u32];
        words.extend(then_clip.unwrap_or(clip).to_cols_array().map(f32::to_bits));
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let screen = cache.get(
            &device,
            TextureDescriptor {
                label: Some("raea_gi_screen"),
                size: Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: FORMAT,
                usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        );
        let params = device.create_buffer_with_data(&BufferInitDescriptor { label: Some("raea_gi_screen_params"), contents: &bytes, usage: BufferUsages::UNIFORM });
        commands.entity(e).insert((RaeaGiScreen(screen.default_view.clone()), GiFrame { screen, params, now, then, soft: kept.soft.clone(), probes: tiles.x * tiles.y }));
    }
}

#[derive(Resource)]
struct GiPipeline {
    layout: BindGroupLayoutDescriptor,
    probes: CachedComputePipelineId,
    filter: CachedComputePipelineId,
    screen: CachedComputePipelineId,
}

fn init(mut commands: Commands, assets: Res<AssetServer>, cache: Res<PipelineCache>) {
    let layout = BindGroupLayoutDescriptor::new(
        "raea_gi_screen_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                uniform_buffer::<ViewUniform>(true),
                texture_depth_2d(),
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_storage_2d(FORMAT, StorageTextureAccess::WriteOnly),
                storage_buffer_sized(false, None),
                storage_buffer_read_only_sized(false, None),
                uniform_buffer_sized(false, core::num::NonZero::new(PARAMS_BYTES)),
                storage_buffer_sized(false, None),
            ),
        ),
    );
    // group 0 is gi.wesl's (its TLAS, sky and records), group 1 the world cache's (raea-sys-rt cache.rs), both bound as
    // raea-sys-rt bound them this frame; group 2 is frame.wesl's
    let layouts = vec![raea_sys_rt::gi::layout(), raea_sys_rt::cache::layout(), layout.clone()];
    let [probes, filter, screen] = [("probes", "main"), ("probes", "pool"), ("screen", "main")].map(|(file, entry)| {
        cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some(format!("raea_gi_{file}_{entry}").into()),
            layout: layouts.clone(),
            shader: assets.load(format!("embedded://raea_sys_gi/{file}.wesl")),
            entry_point: Some(entry.into()),
            ..default()
        })
    });
    commands.insert_resource(GiPipeline { layout, probes, filter, screen });
}

/// Each view's probes, then its screen, from its prepasses and this frame's world cache, before the main pass reads it.
fn draw(
    view: ViewQuery<(&ViewPrepassTextures, &ViewUniformOffset, &GiFrame)>,
    pipeline: Option<Res<GiPipeline>>,
    world: Option<Res<WorldCache>>,
    uniforms: Res<ViewUniforms>,
    cache: Res<PipelineCache>,
    clock: Option<Res<Clock>>,
    mut told: Local<bool>,
    mut ctx: RenderContext,
) {
    let (Some(p), Some(world), Some(view_binding)) = (pipeline, world, uniforms.uniforms.binding()) else { return };
    let (prepass, offset, f) = view.into_inner();
    // a pipeline that failed to build says so once: otherwise the frame quietly draws Bevy's own light (11-18: a WGSL
    // reserved word in probes.wesl measured as raster)
    if !*told && [p.probes, p.filter, p.screen].iter().any(|id| matches!(cache.get_compute_pipeline_state(*id), CachedPipelineState::Err(_))) {
        *told = true;
        eprintln!("raea gi: effect dropped (gi-pipeline): a raea GI shader failed to build (its error is in the log above); the main pass draws Bevy's own light");
    }
    let (Some(probes), Some(filter), Some(screen)) = (cache.get_compute_pipeline(p.probes), cache.get_compute_pipeline(p.filter), cache.get_compute_pipeline(p.screen)) else { return };
    let (Some(depth), Some(normal)) = (prepass.depth_only_view(), prepass.normal_view()) else { return };
    let group = ctx.render_device().create_bind_group(
        "raea_gi_screen",
        &cache.get_bind_group_layout(&p.layout),
        &BindGroupEntries::sequential((view_binding, depth, normal, &f.screen.default_view, f.now.as_entire_binding(), f.then.as_entire_binding(), f.params.as_entire_binding(), f.soft.as_entire_binding())),
    );
    let size = f.screen.texture.size();
    // timed when the last timed frame's times are read
    let timed = clock.filter(|c| c.step.load(Ordering::Acquire) == IDLE);
    let writes = timed.as_ref().map(|c| wgpu::ComputePassTimestampWrites { query_set: &c.set, beginning_of_pass_write_index: Some(0), end_of_pass_write_index: Some(1) });
    let enc = ctx.command_encoder();
    let mut pass = enc.begin_compute_pass(&ComputePassDescriptor { label: Some("raea_gi_screen"), timestamp_writes: writes });
    pass.set_bind_group(0, &world.rays, &[]);
    pass.set_bind_group(1, &world.group, &[]);
    pass.set_bind_group(2, &group, &[offset.offset]);
    // four probes a workgroup, then a thread a probe, then a thread a pixel
    pass.set_pipeline(probes);
    pass.dispatch_workgroups(f.probes.div_ceil(4), 1, 1);
    pass.set_pipeline(filter);
    pass.dispatch_workgroups(f.probes.div_ceil(64), 1, 1);
    pass.set_pipeline(screen);
    pass.dispatch_workgroups(size.width.div_ceil(8), size.height.div_ceil(8), 1);
    drop(pass);
    if let Some(c) = timed {
        enc.resolve_query_set(&c.set, 0..2, &c.resolve, 0);
        enc.copy_buffer_to_buffer(&c.resolve, 0, &c.read, 0, 16);
        c.step.store(COPIED, Ordering::Release);
    }
}

/// The screen passes' timer: a frame's two timestamps, on their way back. `step` goes IDLE (time the next frame),
/// COPIED (its times are in a submitted frame), MAPPING, MAPPED, then IDLE again once read.
#[derive(Resource)]
struct Clock {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    period: f64,
    step: Arc<AtomicU8>,
}
const IDLE: u8 = 0;
const COPIED: u8 = 1;
const MAPPING: u8 = 2;
const MAPPED: u8 = 3;

/// a clock where the GPU has timestamps and RAEA_GI_STATS asks for them
fn clock(mut commands: Commands, device: Res<RenderDevice>, queue: Res<bevy::render::renderer::RenderQueue>) {
    if std::env::var_os("RAEA_GI_STATS").is_none() || !device.features().contains(WgpuFeatures::TIMESTAMP_QUERY) {
        return;
    }
    let d = device.wgpu_device();
    let set = d.create_query_set(&wgpu::QuerySetDescriptor { label: Some("raea gi screen timer"), ty: wgpu::QueryType::Timestamp, count: 2 });
    let resolve = d.create_buffer(&wgpu::BufferDescriptor { label: Some("raea gi screen timer resolve"), size: 16, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
    let read = d.create_buffer(&wgpu::BufferDescriptor { label: Some("raea gi screen timer read"), size: 16, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    commands.insert_resource(Clock { set, resolve, read, period: queue.get_timestamp_period() as f64, step: Arc::new(AtomicU8::new(IDLE)) });
}

impl Clock {
    /// Each frame before the passes: map last frame's copy (it has been submitted since), or take its times once mapped.
    fn read(&self) {
        match self.step.load(Ordering::Acquire) {
            COPIED => {
                self.step.store(MAPPING, Ordering::Release);
                let step = self.step.clone();
                self.read.slice(..).map_async(wgpu::MapMode::Read, move |r| step.store(if r.is_ok() { MAPPED } else { IDLE }, Ordering::Release));
            }
            MAPPED => {
                let ms = {
                    let Ok(view) = self.read.slice(..).get_mapped_range() else { return };
                    let ts = |i: usize| u64::from_le_bytes(view[8 * i..8 * i + 8].try_into().unwrap());
                    ts(1).saturating_sub(ts(0)) as f64 * self.period / 1e6
                };
                self.read.unmap();
                if ms > 0.0
                    && let Ok(mut v) = raea_sys_rt::gi::SCREEN_MS.lock()
                {
                    v.push(ms);
                }
                self.step.store(IDLE, Ordering::Release);
            }
            _ => {}
        }
    }
}
