//! The sun's far shadows (docs/realism.feature LT21; far.wesl). Past the last shadow cascade Bevy draws everything lit,
//! and raea's fade (raea-sys-lod shadow.rs) only softened the line where the city's shadows ended (Shane, 11-17: "its
//! the street it shows it clearly"). There the sun's shadow now comes from a map traced against the ray scene (lib.rs's
//! TLAS): the sun's view RAEA_SUN_FAR metres each way from the eye (1024), RAEA_SUN_FAR_TEXELS a side (4096: half a
//! metre each), each texel one ray from above everything down along the sun to its first shadow caster, holding how
//! high along the sun it hit. Bevy's lighting reads it through raea's bevy_pbr (RaeaFarSun, the mesh view group's 39
//! and 40) as one more cascade, the last and coarsest. A texel is kept at its column modulo the map's size, so as the
//! eye goes the map follows it, eight texels at a time, and only the texels new to it are traced; the whole map only on
//! the first frame and when the sun turns. After the TLAS changes (tiles streaming in, a level swapped) it's traced
//! again RAEA_SUN_FAR_ROWS rows a frame (256), in turn. RAEA_SUN_FAR_STATS: a file the traces' GPU milliseconds are
//! written to, once a second.

use std::sync::atomic::Ordering;

use bevy::{
    core_pipeline::schedule::Core3d,
    ecs::schedule::ScheduleLabel,
    pbr::{RaeaFarSun, RAEA_FAR_SUN_BYTES},
    prelude::*,
    render::{
        camera::ExtractedCamera,
        render_resource::{
            binding_types::{acceleration_structure, texture_storage_2d, uniform_buffer},
            encase, *,
        },
        renderer::{RenderDevice, RenderQueue},
        view::ExtractedView,
        Render, RenderApp, RenderStartup, RenderSystems,
    },
};

use crate::timer::Timer;

pub fn install(app: &mut App) {
    // the far map's reach from the eye each way, in metres, if RAEA_SUN_FAR asks for one (1, or anything not a number
    // over 1: 1024)
    let Some(radius) = crate::reach("RAEA_SUN_FAR", 1.0, 1024.0) else { return };
    bevy::asset::embedded_asset!(app, "far.wesl");
    crate::install(app);
    let env = |k: &str, or: u32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(or);
    let far = Far { radius, texels: env("RAEA_SUN_FAR_TEXELS", 4096).clamp(64, 8192) / 8 * 8, rows: env("RAEA_SUN_FAR_ROWS", 256).max(8) };
    let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
    // after the TLAS is built and before the mesh view groups bind the map
    render.insert_resource(far).add_systems(RenderStartup, init).add_systems(Render, trace.in_set(RenderSystems::PrepareResources).after(crate::build_tlas));
}

#[derive(Resource)]
struct Far {
    radius: f32,
    texels: u32,
    rows: u32,
}

#[derive(Resource)]
struct Pipeline {
    layout: BindGroupLayoutDescriptor,
    id: CachedComputePipelineId,
}

/// far.wesl's Far, and the lighting's RaeaFarSun: toward the sun and metres a texel; the x axis and the depth bias; the
/// y axis; where along the sun the rays start and how far they go; texels a side and the centre texel; the texels a
/// dispatch traces (the first, how many)
#[derive(ShaderType, Clone, Copy, Default)]
struct Params {
    dir: Vec4,
    u: Vec4,
    v: Vec4,
    ray: Vec4,
    map: IVec4,
    region: IVec4,
}

/// how far the eye goes, in texels, before the map follows it
const FOLLOW: i32 = 8;

/// what a trace was: the whole map, the texels new to it as it followed the eye, or rows again after the TLAS changed
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Whole,
    Follow,
    Rows,
}

fn init(mut commands: Commands, assets: Res<AssetServer>, cache: Res<PipelineCache>) {
    assert_eq!(Params::min_size().get(), RAEA_FAR_SUN_BYTES, "raea far sun: far.wesl's Far and bevy_pbr's RaeaFarSun differ");
    let layout = BindGroupLayoutDescriptor::new(
        "raea_far_sun_layout",
        &BindGroupLayoutEntries::sequential(ShaderStages::COMPUTE, (uniform_buffer::<Params>(false), acceleration_structure(), texture_storage_2d(TextureFormat::R32Float, StorageTextureAccess::WriteOnly))),
    );
    let id = cache.queue_compute_pipeline(ComputePipelineDescriptor { label: Some("raea_far_sun".into()), layout: vec![layout.clone()], shader: assets.load("embedded://raea_sys_rt/far.wesl"), entry_point: Some("main".into()), ..default() });
    commands.insert_resource(Pipeline { layout, id });
}

#[derive(Default)]
struct Held {
    /// the map, its view and its uniform, made once (and handed to the lighting as RaeaFarSun)
    map: Option<(Texture, TextureView, Buffer)>,
    /// the uniform as last written
    params: Params,
    /// the texel at the map's centre, once it's traced
    centre: Option<IVec2>,
    /// the next row to trace again, and the TLAS (its build count) the pass under way catches up with
    row: i32,
    since: u64,
    /// a trace's timestamps, with what it was and its rays while they're on their way back
    timer: Option<Timer<(Kind, u32)>>,
    /// each timed trace: what it was, its rays, GPU milliseconds
    seen: Vec<(Kind, u32, f64)>,
    written: Option<std::time::Instant>,
}

#[allow(clippy::too_many_arguments)]
fn trace(
    far: Res<Far>,
    sun: Option<Res<crate::Sun>>,
    views: Query<(&ExtractedView, &ExtractedCamera)>,
    scene: Option<Res<crate::Scene>>,
    p: Option<Res<Pipeline>>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut held: Local<Held>,
    mut commands: Commands,
) {
    let (Some(scene), Some(p), Some(sun)) = (scene, p, sun) else { return };
    let Some(pipeline) = cache.get_compute_pipeline(p.id) else { return };
    let core3d = Core3d.intern();
    let Some(eye) = views.iter().filter(|(_, c)| c.schedule == core3d).min_by_key(|(_, c)| c.order).map(|(v, _)| v.world_from_view.translation()) else { return };
    let n = far.texels;
    let (_, view, buffer) = held.map.get_or_insert_with(|| {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("raea far sun map"),
            size: Extent3d { width: n, height: n, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R32Float,
            usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        // zeros: a size of 0, lit, until the first trace
        let buffer = device.create_buffer(&BufferDescriptor { label: Some("raea far sun"), size: RAEA_FAR_SUN_BYTES, usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST, mapped_at_creation: false });
        commands.insert_resource(RaeaFarSun { map: view.clone(), params: buffer.clone() });
        (texture, view, buffer)
    });
    let (view, buffer) = (view.clone(), buffer.clone());
    let write = |params: &Params| {
        let mut bytes = encase::UniformBuffer::new(Vec::<u8>::new());
        bytes.write(params).unwrap();
        queue.write_buffer(&buffer, 0, &bytes.into_inner());
    };
    let dir = sun.0.normalize_or_zero();
    // a sun under about 2 degrees (or none): no map, as its shadows would run kilometres
    if dir.y < 0.035 {
        if held.params.map.x != 0 {
            held.params.map.x = 0;
            held.centre = None;
            write(&held.params);
        }
        return;
    }
    let texel = 2.0 * far.radius / n as f32;
    let u = dir.cross(if dir.y.abs() < 0.99 { Vec3::Y } else { Vec3::X }).normalize();
    let v = u.cross(dir);
    let (size, half) = (n as i32, n as i32 / 2);
    let now = IVec2::new((eye.dot(u) / texel).floor() as i32, (eye.dot(v) / texel).floor() as i32);
    let tlas = crate::TLAS_BUILT[0].load(Ordering::Relaxed);
    let mut traces: Vec<(Kind, IVec4)> = Vec::new();
    let turned = held.params.dir.truncate().distance(dir) > 1e-5;
    match held.centre {
        Some(c) if !turned && (now - c).abs().max_element() < size => {
            let d = now - c;
            if d.abs().max_element() >= FOLLOW {
                // the columns and rows new to the map as it follows the eye
                if d.x != 0 {
                    traces.push((Kind::Follow, IVec4::new(if d.x > 0 { c.x + half } else { now.x - half }, now.y - half, d.x.abs(), size)));
                }
                if d.y != 0 {
                    traces.push((Kind::Follow, IVec4::new(now.x - half, if d.y > 0 { c.y + half } else { now.y - half }, size, d.y.abs())));
                }
                held.centre = Some(now);
            }
            if held.row != 0 || held.since != tlas {
                // the TLAS changed: the rows again in turn, until a whole pass has followed the change
                if held.row == 0 {
                    held.since = tlas;
                }
                let (c, rows) = (held.centre.unwrap(), (far.rows as i32).min(size - held.row));
                traces.push((Kind::Rows, IVec4::new(c.x - half, c.y - half + held.row, size, rows)));
                held.row = if held.row + rows >= size { 0 } else { held.row + rows };
            }
        }
        _ => {
            traces.push((Kind::Whole, IVec4::new(now.x - half, now.y - half, size, size)));
            held.centre = Some(now);
            held.row = 0;
            held.since = tlas;
        }
    }
    if traces.is_empty() {
        return;
    }
    // the rays start over anything that can shade a point the map reaches: its reach, then 150 m of height along the
    // sun's slope; they go down to past its lowest point
    let start = eye.dot(dir) + far.radius + 150.0 / dir.y;
    let c = held.centre.unwrap();
    held.params = Params { dir: dir.extend(texel), u: u.extend(0.15), v: v.extend(0.0), ray: Vec4::new(start, 2.5 * far.radius + 150.0 / dir.y + 200.0, 0.0, 0.0), map: IVec4::new(size, c.x, c.y, 0), region: IVec4::ZERO };
    let group = device.create_bind_group("raea_far_sun", &cache.get_bind_group_layout(&p.layout), &BindGroupEntries::sequential((buffer.as_entire_binding(), scene.0.as_binding(), &view)));
    if std::env::var_os("RAEA_SUN_FAR_STATS").is_some() && held.timer.is_none() && device.features().contains(WgpuFeatures::TIMESTAMP_QUERY) {
        held.timer = Some(Timer::new(&device, &queue, "raea far sun timer", 2, 16));
    }
    // a submit each, as each has its own region in the one uniform (a write lands before the submit after it)
    for (kind, region) in traces {
        held.params.region = region;
        write(&held.params);
        collect(&mut held);
        let rays = (region.z * region.w) as u32;
        let timed = held.timer.as_ref().is_some_and(|t| t.pending.is_none());
        let mut enc = device.create_command_encoder(&CommandEncoderDescriptor { label: Some("raea far sun") });
        {
            let writes = held.timer.as_ref().filter(|_| timed).map(|t| t.writes(0));
            let mut pass = enc.begin_compute_pass(&ComputePassDescriptor { label: Some("raea_far_sun"), timestamp_writes: writes });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups((region.z as u32).div_ceil(8), (region.w as u32).div_ceil(8), 1);
        }
        if let Some(t) = held.timer.as_ref().filter(|_| timed) {
            t.resolve_into(&mut enc);
        }
        queue.submit([enc.finish()]);
        if let Some(t) = held.timer.as_mut().filter(|_| timed) {
            t.send((kind, rays));
        }
    }
    report(&mut held, &far);
}

/// the last timed trace's GPU milliseconds, once its readback is done
fn collect(held: &mut Held) {
    let Some(t) = held.timer.as_mut() else { return };
    if let Some((ms, (kind, rays))) = t.take(|ms, _| ms(0, 1)) {
        held.seen.push((kind, rays, ms));
    }
}

/// once a second, to RAEA_SUN_FAR_STATS's file: each timed trace of a kind, its count, rays a trace and GPU
/// milliseconds (p50, most)
fn report(held: &mut Held, far: &Far) {
    let Some(path) = std::env::var_os("RAEA_SUN_FAR_STATS") else { return };
    if held.written.is_some_and(|t| t.elapsed().as_secs_f64() < 1.0) || held.seen.is_empty() {
        return;
    }
    held.written = Some(std::time::Instant::now());
    let kind = |k: Kind, name: &str| {
        let mut ms: Vec<f64> = held.seen.iter().filter(|s| s.0 == k).map(|s| s.2).collect();
        let rays: u64 = held.seen.iter().filter(|s| s.0 == k).map(|s| s.1 as u64).sum();
        ms.sort_by(f64::total_cmp);
        let at = |q: f64| ms.get(((ms.len().max(1) - 1) as f64 * q).round() as usize).copied().unwrap_or(0.0);
        format!("\"{name}\":{{\"traces\":{},\"rays_a_trace\":{},\"ms_p50\":{:.3},\"ms_max\":{:.3}}}", ms.len(), rays / ms.len().max(1) as u64, at(0.5), at(1.0))
    };
    let text = format!("{{\"texels\":{},\"radius_m\":{},\"rows_a_frame\":{},{},{},{}}}\n", far.texels, far.radius, far.rows, kind(Kind::Whole, "whole"), kind(Kind::Follow, "follow"), kind(Kind::Rows, "rows"));
    let _ = std::fs::write(path, text);
}
