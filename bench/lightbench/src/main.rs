//! LightBench (docs/lightbench.feature): a small Rust + Bevy renderer that proves lighting and geometry techniques
//! against its own all-lights reference before they move into raea. `lightbench --help` lists the switches; with
//! `--shot FILE` it draws headless, saves frame `--frames` as a PNG and exits, so every claim is a repeatable run.

mod bvh;
mod field;
mod grid;
mod lod;
mod pass;
mod scene;

use bevy::{
    camera::{CameraMainTextureUsages, Hdr, RenderTarget},
    core_pipeline::prepass::{DeferredPrepass, DepthPrepass},
    pbr::DefaultOpaqueRendererMethod,
    prelude::*,
    render::{
        render_resource::{TextureFormat, TextureUsages},
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    window::ExitCondition,
};

const HELP: &str = "lightbench [switches]
  --mode all|uniform|weighted   every light (the reference), or K chosen uniformly or by their light (all)
  --k K                         light samples a pixel, 1 to 16 (4)
  --lights N                    point lights, the pillar's first (16)
  --view lit|normal|albedo|depth|rays   what to show (lit)
  --bounce N                    diffuse bounces each pixel follows through the BVH, each hit lit by the same light
                                sampling as the pixel (one sample; every light with --mode all) (0)
  --view bounce                 the bounced light alone
  --brute                       test every triangle for each shadow ray instead of the BVH
  --detail N                    the knot's segments; its triangles are N * N / 16 (2048; the field's knots 512)
  --size WxH                    pixels (1280x720)
  --t SECONDS                   hold the lights where they are at this time (they move otherwise)
  --exposure X                  linear exposure (1)
  --shot FILE                   draw headless, save frame --frames as a PNG, exit
  --frames N                    the lit frame --shot saves (4)
  --bench N                     with --shot: time N more lit frames first and print the mean (frame time, its own run)
  --no-pillar                   leave out the pillar outside the view (its shadow should go with it)
  --raw                         no temporal accumulation: each frame's samples alone
  --history N                   frames the history holds at most (32)
  --camera still|orbit|cut      the camera holds, circles slowly, or jumps to a second view at lit frame --cut-at (still)
  --cut-at N                    the lit frame --camera cut jumps on (8)
  --view history                frames of history at each pixel, white when full
  --scene knot|field|room       one knot by a wall (lighting), rows of knots into the distance (detail), or the knot
                                and wall in a closed room lit by the sun through a window (bounce: --sun on, --sky
                                0.25, --bounce 2 unless given) (knot)
  --sun on|off                  a low sun from the left, one shadow ray to a point on its disc a pixel (off)
  --sun-size DEG                the sun's angular diameter: larger, softer shadows (0.53)
  --cache on|off                 the bounce reads a world radiance cache at its first hit instead of tracing on, and
                                feeds it: every bounce for two rays a pixel (off)
  --bounce-res full|half        the bounce traced at every pixel, or at one of each 2 x 2 block (a different one each
                                frame) and upsampled by depth and normal (full)
  --cell M                      the cache's cell size at 1 m from the camera, doubling with each doubling (0.1)
  --sky X                       the sky's radiance (a blue-white), brought by bounce rays that leave the scene (0)
  --rows N                      the field's rows (6)
  --detail full|adaptive        every finest triangle, or the clusters each knot's distance asks for (adaptive)
  --tau PX                      the error in pixels a cluster may show (1)
  --fov DEG                     the vertical field of view (45)
  --view lod                    each cluster coloured by its level (white the finest), unlit
  --grid on|off                 the sampled modes weigh only the lights whose range reaches the pixel's cell (on)";

#[derive(Resource, Clone)]
pub struct Opts {
    pub mode: u32,
    pub k: u32,
    pub lights: u32,
    pub view: u32,
    pub brute: bool,
    pub detail: u32,
    pub size: UVec2,
    pub t: Option<f32>,
    pub exposure: f32,
    pub shot: Option<String>,
    pub frames: u32,
    pub bench: u32,
    pub pillar: bool,
    pub raw: bool,
    pub history: u32,
    /// 0 still, 1 orbit, 2 cut
    pub camera: u32,
    pub cut_at: u32,
    /// 0 knot, 1 field
    pub scene: u32,
    pub rows: u32,
    pub full: bool,
    pub tau: f32,
    pub fov: f32,
    pub grid: bool,
    pub bounces: u32,
    pub sun: bool,
    pub sun_size: f32,
    pub sky: f32,
    pub cache: bool,
    pub cell: f32,
    pub half: bool,
}

fn opts() -> Result<Opts, String> {
    let mut o = Opts { mode: 0, k: 4, lights: 16, view: 0, brute: false, detail: 0, size: UVec2::new(1280, 720), t: None, exposure: 1.0, shot: None, frames: 4, bench: 0, pillar: true, raw: false, history: 32, camera: 0, cut_at: 8, scene: 0, rows: 6, full: false, tau: 1.0, fov: 45.0, grid: true, bounces: 0, sun: false, sun_size: 0.53, sky: 0.0, cache: false, cell: 0.1, half: false };
    // what --scene room turns on, unless the switches say otherwise
    let (mut sun, mut sky, mut bounces) = (None, None, None);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().ok_or(format!("{a} needs a value"));
        let num = |v: String| v.parse::<f32>().map_err(|_| format!("{a}: {v} isn't a number"));
        match a.as_str() {
            "--mode" => {
                let v = val()?;
                o.mode = ["all", "uniform", "weighted"].iter().position(|m| *m == v).ok_or("--mode: all, uniform or weighted")? as u32;
            }
            "--view" => {
                let v = val()?;
                o.view = ["lit", "normal", "albedo", "depth", "rays", "history", "lod", "bounce"].iter().position(|m| *m == v).ok_or("--view: lit, normal, albedo, depth, rays, history, lod or bounce")? as u32;
            }
            "--k" => o.k = (num(val()?)? as u32).clamp(1, 16),
            "--lights" => o.lights = (num(val()?)? as u32).max(1),
            "--brute" => o.brute = true,
            "--detail" => {
                let v = val()?;
                match v.as_str() {
                    "full" => o.full = true,
                    "adaptive" => o.full = false,
                    n => o.detail = (num(n.into())? as u32).max(64),
                }
            }
            "--scene" => {
                let v = val()?;
                o.scene = ["knot", "field", "room"].iter().position(|m| *m == v).ok_or("--scene: knot, field or room")? as u32;
            }
            "--rows" => o.rows = (num(val()?)? as u32).max(1),
            "--tau" => o.tau = num(val()?)?.max(0.0),
            "--fov" => o.fov = num(val()?)?.clamp(1.0, 170.0),
            "--grid" => o.grid = val()? != "off",
            "--bounce" => bounces = Some((num(val()?)? as u32).min(8)),
            "--sun" => sun = Some(val()? != "off"),
            "--sun-size" => o.sun_size = num(val()?)?.clamp(0.0, 30.0),
            "--sky" => sky = Some(num(val()?)?.max(0.0)),
            "--cache" => o.cache = val()? != "off",
            "--cell" => o.cell = num(val()?)?.clamp(0.01, 10.0),
            "--bounce-res" => o.half = val()? == "half",
            "--size" => {
                let v = val()?;
                let (w, h) = v.split_once('x').ok_or("--size: WxH")?;
                o.size = UVec2::new(num(w.into())? as u32, num(h.into())? as u32);
            }
            "--t" => o.t = Some(num(val()?)?),
            "--exposure" => o.exposure = num(val()?)?,
            "--shot" => o.shot = Some(val()?),
            "--frames" => o.frames = (num(val()?)? as u32).max(1),
            "--bench" => o.bench = num(val()?)? as u32,
            "--no-pillar" => o.pillar = false,
            "--raw" => o.raw = true,
            "--history" => o.history = (num(val()?)? as u32).max(1),
            "--cut-at" => o.cut_at = num(val()?)? as u32,
            "--camera" => {
                let v = val()?;
                o.camera = ["still", "orbit", "cut"].iter().position(|m| *m == v).ok_or("--camera: still, orbit or cut")? as u32;
            }
            "--help" | "-h" => return Err(HELP.into()),
            _ => return Err(format!("unknown switch {a}\n{HELP}")),
        }
    }
    let room = o.scene == 2;
    (o.sun, o.sky, o.bounces) = (sun.unwrap_or(room), sky.unwrap_or(if room { 0.25 } else { 0.0 }), bounces.unwrap_or(if room { 2 } else { 0 }));
    if o.detail == 0 {
        o.detail = if o.scene == 1 { 512 } else { 2048 };
    }
    Ok(o)
}

/// Whether this run times its frames (--bench), read before the app is built.
pub fn timing() -> bool {
    std::env::args().any(|a| a == "--bench")
}

/// The image a headless run draws into.
#[derive(Resource)]
struct Target(Handle<Image>);

fn main() {
    let o = match opts() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(if e == HELP { 0 } else { 2 });
        }
    };
    let (sky, x) = (o.sky, o.exposure);
    let mut app = App::new();
    // timing runs ask the GPU for timestamps at a pass's start and end (Apple's GPUs have no others), for its time
    let mut wgpu = bevy::render::settings::WgpuSettings::default();
    if timing() {
        wgpu.features |= bevy::render::settings::WgpuFeatures::TIMESTAMP_QUERY;
    }
    let render = bevy::render::RenderPlugin { render_creation: bevy::render::settings::RenderCreation::Automatic(Box::new(wgpu)), ..default() };
    if o.shot.is_some() {
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
                .set(render)
                .disable::<bevy::winit::WinitPlugin>(),
        )
        .add_plugins(bevy::app::ScheduleRunnerPlugin::run_loop(std::time::Duration::ZERO));
    } else {
        let window = Window { title: "LightBench".into(), resolution: (o.size.x, o.size.y).into(), ..default() };
        app.add_plugins(DefaultPlugins.set(WindowPlugin { primary_window: Some(window), ..default() }).set(render));
    }
    app.insert_resource(o)
        .insert_resource(DefaultOpaqueRendererMethod::deferred())
        // the sky seen past the scene: the bounce's sky, exposed as the lighting pass exposes, else near black
        .insert_resource(ClearColor(if sky > 0.0 { Color::linear_rgb(SKY.x * sky * x, SKY.y * sky * x, SKY.z * sky * x) } else { Color::srgb(0.02, 0.025, 0.035) }))
        .add_plugins(pass::LightPass)
        .add_systems(Startup, (scene::spawn, camera))
        .init_resource::<field::Counts>()
        .add_systems(Update, (tick, field::choose.after(tick), shoot.after(field::choose)))
        .run();
}

fn camera(mut commands: Commands, o: Res<Opts>, mut images: ResMut<Assets<Image>>) {
    let usages = TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC | TextureUsages::STORAGE_BINDING;
    let mut cam = commands.spawn((
        Camera3d::default(),
        Hdr,
        Msaa::Off,
        DepthPrepass,
        DeferredPrepass,
        CameraMainTextureUsages(usages),
        Projection::Perspective(PerspectiveProjection { fov: o.fov.to_radians(), ..default() }),
        eye(&o, 0),
    ));
    if o.shot.is_some() {
        let mut image = Image::new_target_texture(o.size.x, o.size.y, TextureFormat::Rgba8UnormSrgb, None);
        image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let handle = images.add(image);
        cam.insert(RenderTarget::Image(handle.clone().into()));
        commands.insert_resource(Target(handle));
    }
}

/// The sun (LB6): toward it from the scene, low on the left and a little behind the knot, so in --scene room it comes
/// through the window onto the floor in front of the knot; its irradiance, warm; the sky's colour, which --sky scales.
const SUN_TO: Vec3 = Vec3::new(-1.0, 0.55, -0.45);
const SUN_E: Vec3 = Vec3::new(3.2, 2.9, 2.5);
const SKY: Vec3 = Vec3::new(0.55, 0.75, 1.0);

const EYE: Vec3 = Vec3::new(0.0, 2.2, 7.0);
const LOOK: Vec3 = Vec3::new(0.0, 1.3, 0.0);

/// Where the camera is on lit frame `n`: held, circling slowly, or cut to a second view at --cut-at.
fn eye(o: &Opts, n: u32) -> Transform {
    match o.camera {
        1 => {
            let a = n as f32 * 0.004;
            Transform::from_translation(LOOK + Quat::from_rotation_y(a) * (EYE - LOOK)).looking_at(LOOK, Vec3::Y)
        }
        2 if n >= o.cut_at => Transform::from_xyz(4.5, 3.2, 4.5).looking_at(LOOK, Vec3::Y),
        _ => Transform::from_translation(EYE).looking_at(LOOK, Vec3::Y),
    }
}

/// The camera and the lights where they are this frame, and the switches, for the render world. A cut is a jump of
/// more than a metre or 20 degrees in one frame: the history starts again.
fn tick(o: Res<Opts>, time: Res<Time>, mut frame: ResMut<pass::Frame>, mut cam: Query<&mut Transform, With<Camera3d>>) {
    let t = o.t.unwrap_or(time.elapsed_secs());
    let n = pass::DRAWN.load(std::sync::atomic::Ordering::Relaxed);
    let mut cut = false;
    if let Ok(mut at) = cam.single_mut() {
        let to = eye(&o, n);
        cut = at.translation.distance(to.translation) > 1.0 || at.rotation.angle_between(to.rotation) > 20f32.to_radians();
        *at = to;
    }
    let lights = scene::lights(o.lights, t, o.scene == 1);
    let g = grid::build(&lights, 2.0);
    *frame = pass::Frame {
        grid_min: g.min.extend(g.cell),
        grid_dims: g.dims.extend(o.grid as u32),
        // the starts count from the buffer's beginning, the indices following them
        grid: g.start.iter().map(|s| s + g.start.len() as u32).chain(g.idx.iter().copied()).collect(),
        lights,
        mode: o.mode,
        k: o.k,
        view: o.view,
        frame: frame.frame.wrapping_add(1),
        exposure: o.exposure,
        brute: o.brute,
        raw: o.raw,
        cut,
        history: o.history,
        bounces: o.bounces,
        sun: SUN_TO.normalize().extend((o.sun_size * 0.5).to_radians()),
        sun_e: if o.sun { SUN_E.extend(0.0) } else { Vec4::ZERO },
        sky: (SKY * o.sky).extend(0.0),
        cache: o.cache,
        cell: o.cell,
        half: o.half,
    };
}

fn shoot(mut commands: Commands, o: Res<Opts>, counts: Res<field::Counts>, target: Option<Res<Target>>, mut done: Local<bool>, mut clock: Local<Option<(std::time::Instant, u32)>>, mut gpu0: Local<Option<(u64, u32)>>) {
    let (Some(path), Some(target)) = (o.shot.clone(), target) else { return };
    // counted from the lighting pass's first frame, not from startup (its pipeline compiles meanwhile)
    let drawn = pass::DRAWN.load(std::sync::atomic::Ordering::Relaxed);
    if *done || drawn < o.frames + o.bench {
        if drawn >= o.frames && clock.is_none() {
            *clock = Some((std::time::Instant::now(), drawn));
            *gpu0 = Some((pass::GPU_NS.load(std::sync::atomic::Ordering::Relaxed), pass::GPU_FRAMES.load(std::sync::atomic::Ordering::Relaxed)));
        }
        return;
    }
    *done = true;
    if o.scene == 1 {
        let c = *counts;
        println!("LOD {} knots, {} culled, {} of {} triangles drawn ({:.1}%), detail {}, tau {} px, fov {} deg", c.knots, c.culled, c.drawn, c.full, c.drawn as f64 * 100.0 / c.full.max(1) as f64, if o.full { "full" } else { "adaptive" }, o.tau, o.fov);
    }
    if o.bench > 0 {
        let (t0, d0) = clock.unwrap();
        let ms = t0.elapsed().as_secs_f64() * 1000.0 / (drawn - d0).max(1) as f64;
        let r = |k: usize| pass::RAYS[k].load(std::sync::atomic::Ordering::Relaxed);
        println!("RAYS {:.2} a lit pixel (most {}), {} lit pixels", r(0) as f64 / r(1).max(1) as f64, r(2), r(1));
        let (ns, n) = (pass::GPU_NS.load(std::sync::atomic::Ordering::Relaxed), pass::GPU_FRAMES.load(std::sync::atomic::Ordering::Relaxed));
        let (ns0, n0) = gpu0.unwrap_or((0, 0));
        if n > n0 {
            println!("GPU lighting pass {:.2} ms ({} frames timed)", (ns - ns0) as f64 / 1e6 / (n - n0) as f64, n - n0);
        }
        println!("FRAME {ms:.2} ms, the GPU waited for ({} frames, {}x{}, {} lights, mode {} k {}, bounces {}{}{})", drawn - d0, o.size.x, o.size.y, o.lights, ["all", "uniform", "weighted"][o.mode as usize], o.k, o.bounces, if o.cache { ", cache" } else { "" }, if o.half { ", half-res" } else { "" });
    }
    commands.spawn(Screenshot::image(target.0.clone())).observe(move |shot: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
        match shot.image.clone().try_into_dynamic() {
            Ok(img) => match img.to_rgb8().save(&path) {
                Ok(()) => println!("SHOT {path}"),
                Err(e) => eprintln!("lightbench: saving {path}: {e}"),
            },
            Err(e) => eprintln!("lightbench: the frame: {e:?}"),
        }
        exit.write(AppExit::Success);
    });
}
