//! raea GI's world radiance cache, its first form (RAEA_GI=1; docs/realism.feature LT2): what a frame of probe rays
//! costs on this GPU, and the light they give through Bevy's irradiance volume. A grid of probes around the camera
//! (RAEA_GI_GRID, 32x16x32 by default; RAEA_GI_SPACING metres apart, 2) is refreshed by a ray budget a frame
//! (RAEA_GI_RAYS, 131072): 64 rays a probe, so the budget sets how many probes a frame (gi.wesl). The grid moves with
//! the camera a probe at a time, each probe kept by its place in the world, so what the probes still covered have
//! learned stays; a probe new to the grid traces RAEA_GI_WARM rounds (16) when first reached. When the sun or the sky
//! changes, every mean starts again and every probe traces the warm rounds that frame, so a still is lit from its
//! first frame. Those frames aren't timed.
//! The probes' rays read the world cache where they hit (cache.rs, LT3), which runs first each frame.
//! RAEA_GI_STATS=FILE: the timed frames' GPU milliseconds, the cache's passes and the probes' apart (each pass's own
//! timestamps), and the cache's live cells, written there once a second.
//! RAEA_GI_SHOW=volume: the probes light the scene through Bevy's irradiance volume (see `install`). Unset, nothing
//! changes.

use std::sync::atomic::Ordering;

use bevy::{
    asset::RenderAssetUsages,
    camera::Camera3d,
    image::Image,
    light::{EnvironmentMapLight, IrradianceVolume, LightProbe},
    prelude::*,
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_resource::{
            binding_types::{acceleration_structure, sampler, storage_buffer_read_only_sized, storage_buffer_sized, texture_cube, texture_storage_3d, uniform_buffer},
            *,
        },
        renderer::{RenderDevice, RenderQueue},
        texture::{FallbackImage, GpuImage},
        Render, RenderApp, RenderStartup, RenderSystems,
    },
};

use crate::timer::Timer;

/// rays a probe each round (gi.wesl's RAYS: a workgroup a probe, a ray a thread)
const RAYS: u32 = 64;

/// raea-sys-gi's screen passes' GPU milliseconds (the screen probes, their filter and the screen), each timed frame's,
/// reported beside the cache's (`report`)
pub static SCREEN_MS: std::sync::Mutex<Vec<f64>> = std::sync::Mutex::new(Vec::new());

fn env<T: std::str::FromStr>(name: &str, or: T) -> T {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(or)
}

fn grid_res() -> UVec3 {
    let v: Vec<u32> = std::env::var("RAEA_GI_GRID").unwrap_or_default().split('x').filter_map(|s| s.parse().ok()).collect();
    if let [x, y, z] = v[..] { UVec3::new(x, y, z).max(UVec3::ONE) } else { UVec3::new(32, 16, 32) }
}

pub fn install(app: &mut App) {
    if std::env::var("RAEA_GI").as_deref() != Ok("1") {
        return;
    }
    bevy::shader::load_shader_library!(app, "cache.wesl");
    bevy::asset::embedded_asset!(app, "compact.wesl");
    // a library too: raea-sys-gi's screen probes trace with its functions (first_hit, irradiance_at, sky_radiance)
    bevy::shader::load_shader_library!(app, "gi.wesl");
    crate::install(app);
    let res = grid_res();
    let mut voxels = Image::new_fill(
        Extent3d { width: res.x, height: 2 * res.y, depth_or_array_layers: 3 * res.z },
        TextureDimension::D3,
        &[0u8; 8],
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    voxels.texture_descriptor.usage |= TextureUsages::STORAGE_BINDING;
    let voxels = app.world_mut().resource_mut::<Assets<Image>>().add(voxels);
    let spacing = env("RAEA_GI_SPACING", 2.0f32);
    // Shown through Bevy's irradiance volume only when asked: it takes one more sampler in every material's fragment
    // stage, and a hooked material over StandardMaterial already has Metal's 16 (Bevy's view 9, StandardMaterial 6, the
    // hook's own), so on the Mac the pipeline fails to build (11-17, Berlin). The deferred lighting pass (D4) has room.
    if std::env::var("RAEA_GI_SHOW").as_deref() == Ok("volume") {
        app.world_mut().spawn((Name::new("raea gi"), LightProbe::default(), IrradianceVolume { voxels: voxels.clone(), intensity: 1.0, affects_lightmapped_meshes: false }, Transform::from_scale(res.as_vec3() * spacing)));
    } else {
        eprintln!("raea gi: effect dropped (gi-not-shown): the probes are traced and timed but light nothing; Bevy's irradiance volume would take a 17th fragment sampler on Metal (RAEA_GI_SHOW=volume tries it)");
    }
    app.insert_resource(Grid { center: Vec3::ZERO, eye: Vec3::ZERO, spacing, res, voxels, sky: None, sun: Vec4::ZERO, sun_light: Vec3::ZERO })
        .add_plugins(ExtractResourcePlugin::<Grid>::default())
        .add_systems(PostUpdate, follow.after(bevy::transform::TransformSystems::Propagate));
    let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
    render.init_resource::<State>().add_systems(RenderStartup, init).add_systems(Render, update.in_set(RenderSystems::PrepareBindGroups));
}

/// The grid this frame: where it stands, its voxels, the sky and the sun its rays see.
#[derive(Resource, Clone, ExtractResource)]
#[extract_app(bevy::render::RenderApp)]
struct Grid {
    center: Vec3,
    /// the camera, where the cache's cells are smallest
    eye: Vec3,
    spacing: f32,
    res: UVec3,
    voxels: Handle<Image>,
    /// the camera's environment: its specular and diffuse cubemaps, intensity, and the turn into their frame
    sky: Option<(AssetId<Image>, AssetId<Image>, f32, Quat)>,
    /// toward the sun, and 1 when a sky is bound
    sun: Vec4,
    /// the sun's illuminance times its linear colour (lux)
    sun_light: Vec3,
}

/// the volume kept around the active 3D camera, a probe's spacing at a time (a quarter of the grid's height below the
/// eye, three quarters above: the street and the fronts, not the ground under it); the camera's sky and the sun
fn follow(
    cams: Query<(&Camera, &GlobalTransform, Option<&EnvironmentMapLight>), With<Camera3d>>,
    lights: Query<(&DirectionalLight, &GlobalTransform)>,
    mut volume: Query<&mut Transform, With<IrradianceVolume>>,
    mut grid: ResMut<Grid>,
) {
    let Some((_, cam, env)) = cams.iter().find(|(c, _, _)| c.is_active) else { return };
    let extent = grid.res.as_vec3() * grid.spacing;
    let eye = cam.translation() + Vec3::Y * extent.y * 0.25;
    let center = (eye / grid.spacing).round() * grid.spacing;
    if grid.center != center {
        grid.center = center;
    }
    if grid.eye != cam.translation() {
        grid.eye = cam.translation();
    }
    for mut t in &mut volume {
        if t.translation != center {
            t.translation = center;
        }
    }
    let sky = env.map(|e| (e.specular_map.id(), e.diffuse_map.id(), e.intensity, e.rotation.inverse()));
    if grid.sky != sky {
        grid.sky = sky;
    }
    let (dir, light) = lights
        .iter()
        .find(|(l, _)| l.shadow_maps_enabled)
        .or_else(|| lights.iter().next())
        .map(|(l, t)| (t.back().as_vec3(), Vec3::from_slice(&l.color.to_linear().to_f32_array()[..3]) * l.illuminance))
        .unwrap_or((Vec3::Y, Vec3::ZERO));
    let sun = dir.extend(if sky.is_some() { 1.0 } else { 0.0 });
    if grid.sun != sun || grid.sun_light != light {
        grid.sun = sun;
        grid.sun_light = light;
    }
}

#[derive(ShaderType, Clone, Copy)]
struct Params {
    grid: Vec4,
    res: UVec4,
    /// the first probe, rounds each, the frame's salt
    run: UVec4,
    sun_dir: Vec4,
    sun: Vec4,
    sky_turn: Vec4,
}

#[derive(Resource)]
struct Pipeline {
    layout: BindGroupLayoutDescriptor,
    /// the probes' rays (gi.wesl `main`), and their cubes into the volume (`resolve`)
    id: CachedComputePipelineId,
    resolve: CachedComputePipelineId,
    sampler: Sampler,
    /// the world cache's group 1, and its passes
    cache_layout: BindGroupLayoutDescriptor,
    cache: crate::cache::Passes,
    /// whether the probes light anything (RAEA_GI_SHOW=volume): raea-sys-gi's screen probes stand in for them, so
    /// otherwise they aren't traced
    shown: bool,
}

/// gi.wesl's group 0: the probes' parameters, the TLAS, the sky, the probes' cubes, volume and tags, and the records.
/// Passes outside raea-sys-rt that trace with gi.wesl's functions bind WorldCache::rays here.
pub fn layout() -> BindGroupLayoutDescriptor {
    BindGroupLayoutDescriptor::new(
        "raea_gi_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                uniform_buffer::<Params>(false),
                acceleration_structure(),
                texture_cube(TextureSampleType::Float { filterable: true }),
                texture_cube(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                storage_buffer_sized(false, None),
                texture_storage_3d(TextureFormat::Rgba16Float, StorageTextureAccess::WriteOnly),
                storage_buffer_sized(false, None),
                // each instance's record (records.rs)
                storage_buffer_read_only_sized(false, None),
            ),
        ),
    )
}

fn init(mut commands: Commands, assets: Res<AssetServer>, cache: Res<PipelineCache>, device: Res<RenderDevice>) {
    let layout = layout();
    let shader = assets.load("embedded://raea_sys_rt/gi.wesl");
    let cache_layout = crate::cache::layout();
    let layouts = vec![layout.clone(), cache_layout.clone()];
    let [id, resolve] = ["main", "resolve"].map(|entry| crate::cache::pipeline(&cache, "raea_gi", &layouts, &shader, entry));
    let passes = crate::cache::queue(&cache, &assets, &layouts);
    let sampler = device.create_sampler(&SamplerDescriptor { label: Some("raea gi sky"), mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear, ..default() });
    let shown = std::env::var("RAEA_GI_SHOW").as_deref() == Ok("volume");
    commands.insert_resource(Pipeline { layout, id, resolve, sampler, cache_layout, cache: passes, shown });
}

/// The cache between frames: its means and which probe each slot holds (gi.wesl's `cubes` and `tags`), where the next
/// frame's probes start, what it was last lit by, and its timer.
#[derive(Resource, Default)]
struct State {
    cubes: Option<(Buffer, Buffer, u32)>,
    cache: Option<crate::cache::Buffers>,
    frame: u32,
    key: Option<[u32; 9]>,
    /// the cache's start and end, the probes' start and end; the read-back also takes the cache's live cells
    timer: Option<Timer<()>>,
    /// each timed frame's GPU milliseconds, the cache's and the probes', and the cache's live cells
    seen: Vec<(f64, f64, u32)>,
    written: Option<std::time::Instant>,
}

/// what the grid is lit from: the sun and the sky (and its size); any change starts every mean again. Not its place:
/// a probe is kept by where it is in the world, so the grid moving starts only the probes new to it (gi.wesl `tags`).
fn key(g: &Grid) -> [u32; 9] {
    let (s, i) = g.sky.map(|(s, _, i, _)| (s, i)).unwrap_or((AssetId::default(), 0.0));
    let h = { use std::hash::{Hash, Hasher}; let mut h = std::collections::hash_map::DefaultHasher::new(); s.hash(&mut h); h.finish() as u32 };
    let f = |v: f32| v.to_bits();
    [f(g.sun.x), f(g.sun.y), f(g.sun.z), f(g.sun_light.x), f(g.sun_light.y), f(g.sun_light.z), h, f(i), g.res.x ^ (g.res.y << 10) ^ (g.res.z << 20)]
}

/// This frame's world cache for passes outside raea-sys-rt (raea-sys-gi's screen pass, after this frame's update): its
/// group 1 (cache::layout), gi.wesl's group 0 to trace with (`layout`), and the sun its cells were lit by, toward it
/// and its light (lux)
#[derive(Resource, Clone)]
pub struct WorldCache {
    pub group: BindGroup,
    pub rays: BindGroup,
    pub sun_dir: Vec3,
    pub sun_light: Vec3,
}

#[allow(clippy::too_many_arguments)]
fn update(
    mut commands: Commands,
    grid: Option<Res<Grid>>,
    scene: Option<Res<crate::Scene>>,
    records: Option<Res<crate::Records>>,
    images: Res<RenderAssets<GpuImage>>,
    fallback: Res<FallbackImage>,
    p: Option<Res<Pipeline>>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut state: ResMut<State>,
) {
    let (Some(grid), Some(scene), Some(records), Some(p)) = (grid, scene, records, p) else { return };
    let (Some(pipeline), Some(resolve)) = (cache.get_compute_pipeline(p.id), cache.get_compute_pipeline(p.resolve)) else { return };
    let Some(passes) = p.cache.0.iter().map(|id| cache.get_compute_pipeline(*id)).collect::<Option<Vec<_>>>() else { return };
    let Ok(passes) = <[&ComputePipeline; 6]>::try_from(passes) else { return };
    let Some(voxels) = images.get(grid.voxels.id()) else { return };
    let (sky, sky_diffuse) = match grid.sky {
        Some((s, d, _, _)) => match (images.get(s), images.get(d)) {
            (Some(s), Some(d)) => (&s.texture_view, &d.texture_view),
            _ => return,
        },
        None => (&fallback.cube.texture_view, &fallback.cube.texture_view),
    };
    let total = grid.res.x * grid.res.y * grid.res.z;
    if state.cubes.as_ref().is_none_or(|(_, _, n)| *n != total) {
        let buffer = |label: &'static str, faces: u64| device.create_buffer(&BufferDescriptor { label: Some(label), size: total as u64 * faces * 16, usage: BufferUsages::STORAGE | BufferUsages::COPY_DST, mapped_at_creation: false });
        state.cubes = Some((buffer("raea gi cubes", 6), buffer("raea gi tags", 1), total));
        state.key = None;
    }
    collect(&mut state, p.shown);
    let k = key(&grid);
    let fresh = state.key != Some(k);
    state.key = Some(k);
    let budget = env("RAEA_GI_RAYS", 131072u32).max(RAYS);
    let warm = env("RAEA_GI_WARM", 16u32).max(1);
    let (start, probes, rounds) = if fresh { (0, total, warm) } else { ((state.frame.wrapping_mul(budget / RAYS)) % total, (budget / RAYS).min(total), 1) };
    state.frame = state.frame.wrapping_add(1);
    let (turn, intensity) = grid.sky.map(|(_, _, i, q)| (Vec4::from(q), i)).unwrap_or((Vec4::W, 0.0));
    let params = Params {
        grid: grid.center.extend(grid.spacing),
        res: grid.res.extend(total),
        run: UVec4::new(start, rounds, state.frame, warm),
        sun_dir: grid.sun,
        sun: grid.sun_light.extend(intensity),
        sky_turn: turn,
    };
    let mut uniform = UniformBuffer::from(params);
    uniform.write_buffer(&device, &queue);
    let mut upkeep = UniformBuffer::from(crate::cache::Upkeep::new(grid.eye, state.frame, fresh));
    upkeep.write_buffer(&device, &queue);
    let cells = state.cache.get_or_insert_with(|| crate::cache::buffers(&device));
    let cache_group = crate::cache::bind(&device, &cache.get_bind_group_layout(&p.cache_layout), cells, upkeep.binding().unwrap());
    if state.timer.is_none() && device.features().contains(WgpuFeatures::TIMESTAMP_QUERY) {
        state.timer = Some(Timer::new(&device, &queue, "raea gi timer", 4, 48));
    }
    let (cubes, tags, _) = state.cubes.as_ref().unwrap();
    let group = device.create_bind_group(
        "raea_gi",
        &cache.get_bind_group_layout(&p.layout),
        &BindGroupEntries::sequential((uniform.binding().unwrap(), scene.0.as_binding(), sky, sky_diffuse, &p.sampler, cubes.as_entire_binding(), &voxels.texture_view, tags.as_entire_binding(), records.0.as_entire_binding())),
    );
    commands.insert_resource(WorldCache { group: cache_group.clone(), rays: group.clone(), sun_dir: grid.sun.truncate(), sun_light: grid.sun_light });
    // a steady frame is timed when the last one's times are in
    let timed = !fresh && state.timer.as_ref().is_some_and(|t| t.pending.is_none());
    let mut enc = device.create_command_encoder(&CommandEncoderDescriptor { label: Some("raea gi") });
    if fresh {
        enc.clear_buffer(cubes, 0, None);
        enc.clear_buffer(tags, 0, None);
    }
    let cells = state.cache.as_ref().unwrap();
    crate::cache::run(&mut enc, &passes, [&group, &cache_group], cells, state.timer.as_ref().filter(|_| timed).map(|t| (&t.set, 0)));
    // The probes, only where they light something. Unshown, their pass keeps one workgroup of `resolve`: on the Mac the
    // cache's last pass writes its end time only when a timed pass follows it (11-18: without one it read 0).
    {
        let writes = state.timer.as_ref().filter(|_| timed).map(|t| t.writes(2));
        let mut pass = enc.begin_compute_pass(&ComputePassDescriptor { label: Some("raea_gi"), timestamp_writes: writes });
        pass.set_bind_group(0, &group, &[]);
        pass.set_bind_group(1, &cache_group, &[]);
        if p.shown {
            pass.set_pipeline(pipeline);
            pass.dispatch_workgroups(probes, 1, 1);
        }
        // every probe's cube into the volume (the grid may have moved), timed with the rays: it is the pass's cost too
        pass.set_pipeline(resolve);
        pass.dispatch_workgroups(if p.shown { total.div_ceil(64) } else { 1 }, 1, 1);
    }
    if let Some(t) = state.timer.as_ref().filter(|_| timed) {
        t.resolve_into(&mut enc);
        enc.copy_buffer_to_buffer(&cells.all[crate::cache::COUNT], 0, &t.read, 32, 4);
    }
    queue.submit([enc.finish()]);
    if let Some(t) = state.timer.as_mut().filter(|_| timed) {
        t.send(());
    }
    report(&mut state, budget, total);
}

/// the last timed frame's GPU milliseconds, once its readback is done
fn collect(state: &mut State, shown: bool) {
    let Some(t) = state.timer.as_mut() else { return };
    // unshown, the cache runs to the end of the marker pass after it (its own end time reads 0 there)
    let Some(((cache_ms, probes_ms, cells), ())) = t.take(|ms, view| {
        (if shown { ms(0, 1) } else { ms(0, 3) }, if shown { ms(2, 3) } else { 0.0 }, u32::from_le_bytes(view[32..36].try_into().unwrap()))
    }) else {
        return;
    };
    // a frame whose times came back (the probes' pass may be empty: not shown)
    if cache_ms > 0.0 {
        state.seen.push((cache_ms, probes_ms, cells));
    }
}

/// once a second: the timed frames so far, to RAEA_GI_STATS's file
fn report(state: &mut State, budget: u32, total: u32) {
    let Some(path) = std::env::var_os("RAEA_GI_STATS") else { return };
    if state.written.is_some_and(|t| t.elapsed().as_secs_f64() < 1.0) || state.seen.is_empty() {
        return;
    }
    state.written = Some(std::time::Instant::now());
    // each column sorted on its own: the cache's ms, the probes', both, the live cells
    let col = |f: &dyn Fn(&(f64, f64, u32)) -> f64| {
        let mut v: Vec<f64> = state.seen.iter().map(f).collect();
        v.sort_by(f64::total_cmp);
        v
    };
    let (c, pr, all, live) = (col(&|s| s.0), col(&|s| s.1), col(&|s| s.0 + s.1), col(&|s| s.2 as f64));
    let at = |v: &[f64], q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    let mut screen = SCREEN_MS.lock().map(|v| v.clone()).unwrap_or_default();
    screen.sort_by(f64::total_cmp);
    let screen_at = |q: f64| if screen.is_empty() { 0.0 } else { at(&screen, q) };
    // the ray scene's CPU milliseconds a call: the instance list, the TLAS, a BLAS batch (lib.rs CPU_US)
    let cpu = |k: usize| {
        let (us, n) = (crate::CPU_US[2 * k].load(Ordering::Relaxed), crate::CPU_US[2 * k + 1].load(Ordering::Relaxed));
        format!("{:.3}", us as f64 / n.max(1) as f64 / 1000.0)
    };
    let text = format!(
        "{{\"screen_ms_p50\":{:.3},\"screen_ms_p90\":{:.3},\"rays_a_frame\":{},\"probes\":{total},\"probes_a_frame\":{},\"frames\":{},\"gpu_ms_p50\":{:.3},\"gpu_ms_p90\":{:.3},\"gpu_ms_max\":{:.3},\"cache_ms_p50\":{:.3},\"cache_ms_p90\":{:.3},\"probes_ms_p50\":{:.3},\"probes_ms_p90\":{:.3},\"cache_cells_p50\":{},\"cache_cells_max\":{},\"cpu_ms_gather\":{},\"cpu_ms_tlas\":{},\"cpu_ms_blas_batch\":{},\"blas_batches\":{},\"tlas_builds\":{},\"tlas_instances\":{}}}\n",
        screen_at(0.5),
        screen_at(0.9),
        (budget / RAYS).min(total) * RAYS,
        (budget / RAYS).min(total),
        all.len(),
        at(&all, 0.5),
        at(&all, 0.9),
        at(&all, 1.0),
        at(&c, 0.5),
        at(&c, 0.9),
        at(&pr, 0.5),
        at(&pr, 0.9),
        at(&live, 0.5),
        at(&live, 1.0),
        cpu(0),
        cpu(1),
        cpu(2),
        crate::CPU_US[5].load(Ordering::Relaxed),
        crate::TLAS_BUILT[0].load(Ordering::Relaxed),
        crate::TLAS_BUILT[1].load(Ordering::Relaxed)
    );
    let _ = std::fs::write(path, text);
}
