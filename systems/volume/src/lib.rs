//! Simulated volumes (`<Smoke>`, `<Cloud>` and `<Contrail>` in raea/3d): a smoke plume, a cumulus, a contrail
//! as a fluid simulated on a grid, raymarched and lit by the sun with its own shadow (volume.wesl). Under a paint look
//! (<Look mode="paint" | "cartoon">) the march stops at the volume's surface and the light there is cut into the look's
//! flat tones, with the surface eroded by noise so its edges tear; without one it's drawn as a soft lit volume.
//!
//! The simulation: velocity, density and temperature on a grid of cells (the box's longest side cut into `res`), a
//! step every 1/30 s: sources (a plume's burner, a contrail's moving engine, a cloud's puffs at the start), buoyancy
//! (heat rises, density sinks), the wind, vorticity confinement (curls kept curling), a slowly drifting turbulent force,
//! semi-Lagrangian advection, a pressure projection (Jacobi), dissipation and cooling. Everything is a function of the
//! step number and the spec (sources jittered by a hash of the step, not a clock), so the volume at time t is the same
//! however it's reached: a render of frame N, the frames before it, or play. It steps forward from where it is, and
//! from the start when asked for an earlier time.
//!
//! A volume takes part in the scene's light as a mesh does: it's drawn into the lights' shadow maps (volume_shadow.wesl
//! marches each texel's ray through the field and writes the depth where the volume stops the light), so it throws soft
//! moving shadows on meshes and models, and the volumetric fog, which walks the same maps, shows the sun's shafts
//! through the gaps between clouds. A map's texel is lit or not: it stops where the volume has stopped half the light,
//! and the light's shadow filter (PCSS with `softShadow`) softens the edge. The row's `shadow` scales the density the
//! shadow sees (1 by default: as drawn; more reaches into thinner parts, less keeps to the dense cores; 0 casts none).
//!
//! A row is a fire row (`<Fire>` and these share it: fire.rs hands a row with `sim` here) whose columns are fire's
//! (x y z the box's foot, scale_x/y/z its size, time) plus `sim`: the spec as JSON (see `Spec`), and `shadow`.

use std::collections::BTreeMap;

use bevy::prelude::*;
use raea_core_program::ir::V;
use raea_sys_frame::*;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::pbr::{Material, MaterialPlugin, MeshMaterial3d};
use bevy::render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use serde_json::Value as J;

mod sim;
pub use sim::*;
#[cfg(test)]
mod tests;

pub fn install(app: &mut App) {
    bevy::asset::embedded_asset!(app, "volume.wesl");
    bevy::asset::embedded_asset!(app, "volume_shadow.wesl");
    app.add_plugins(MaterialPlugin::<VolumeMaterial>::default())
        .add_systems(Update, sync_volumes.after(FrameSystems::Rows).before(FrameSystems::Tag));
}

/// Steps a second of simulated time.
const RATE: f32 = 30.0;

#[derive(Clone, Debug, Default)]
pub struct Source {
    /// where (metres from the box's foot), a contrail's start
    at: [f32; 3],
    /// a contrail's end, reached at `t1` from `at` at `t0` (seconds)
    to: Option<[f32; 3]>,
    t0: f32,
    t1: f32,
    radius: f32,
    /// density and heat added a second, at the centre
    rate: f32,
    heat: f32,
    /// the velocity it pushes out with (m/s)
    speed: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct Spec {
    /// cells along the box's longest side
    res: u32,
    /// heat's lift and density's weight (m/s² per unit)
    buoyancy: f32,
    weight: f32,
    /// heat lost and density lost a second (fractions)
    cooling: f32,
    dissipation: f32,
    /// the air's velocity (m/s), and how fast the volume takes it up (1/s)
    wind: [f32; 3],
    wind_pull: f32,
    /// the height the wind reaches full strength (0: everywhere)
    wind_height: f32,
    vorticity: f32,
    /// velocity lost a second to the still air (a fraction)
    drag: f32,
    /// a drifting noise force (m/s²) and its size (m)
    turbulence: f32,
    turbulence_size: f32,
    sources: Vec<Source>,
    /// puffs of density at the start: [x, y, z, radius] in metres from the foot
    puffs: Vec<[f32; 4]>,
    /// how much light a unit of density stops a metre (its shadow on itself)
    absorb: f32,
    seed: u32,
    pressure_iters: u32,
    /// drawing: the density its surface is at (paint), the erosion of that surface (amount, size in m), the soft
    /// volume's opacity a unit a metre, and its colours: lit, mid, dark (sRGB hex), and the fire's three
    threshold: f32,
    erode: f32,
    erode_size: f32,
    opacity: f32,
    lit: String,
    mid: String,
    dark: String,
    fire: Vec<String>,
    /// the temperature the fire's three colours start at
    fire_at: [f32; 3],
    steps: u32,
}

impl Default for Spec {
    fn default() -> Self {
        Spec {
            res: 64,
            buoyancy: 4.0,
            weight: 0.4,
            cooling: 0.8,
            dissipation: 0.02,
            wind: [0.0; 3],
            wind_pull: 0.6,
            wind_height: 0.0,
            vorticity: 6.0,
            drag: 0.4,
            turbulence: 0.0,
            turbulence_size: 2.0,
            sources: Vec::new(),
            puffs: Vec::new(),
            absorb: 1.2,
            seed: 1,
            pressure_iters: 30,
            threshold: 0.12,
            erode: 0.08,
            erode_size: 0.6,
            opacity: 2.0,
            lit: "#4a4a4c".into(),
            mid: "#2c2c2e".into(),
            dark: "#141415".into(),
            fire: vec!["#e2462c".into(), "#f2862e".into(), "#f7cf4a".into()],
            fire_at: [0.35, 0.6, 0.85],
            steps: 96,
        }
    }
}

use raea_sys_frame::json_f32 as f32_of;

fn v3_of(o: &J, k: &str) -> Option<[f32; 3]> {
    let a = o.get(k)?.as_array()?;
    Some([a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32])
}

fn str_of(o: &J, k: &str, d: &str) -> String {
    o.get(k).and_then(J::as_str).unwrap_or(d).to_string()
}

impl Spec {
    /// The spec from the row's JSON (camelCase keys, every one optional)
    fn from_json(o: &J) -> Spec {
        let d = Spec::default();
        let sources = o.get("sources").and_then(J::as_array).map(|a| {
            a.iter()
                .map(|s| Source {
                    at: v3_of(s, "at").unwrap_or([0.0; 3]),
                    to: v3_of(s, "to"),
                    t0: f32_of(s, "t0", 0.0),
                    t1: f32_of(s, "t1", 1.0),
                    radius: f32_of(s, "radius", 0.5),
                    rate: f32_of(s, "rate", 4.0),
                    heat: f32_of(s, "heat", 4.0),
                    speed: v3_of(s, "speed").unwrap_or([0.0; 3]),
                })
                .collect()
        });
        let puffs = o.get("puffs").and_then(J::as_array).map(|a| {
            a.iter()
                .filter_map(|p| {
                    let p = p.as_array()?;
                    Some([p.first()?.as_f64()? as f32, p.get(1)?.as_f64()? as f32, p.get(2)?.as_f64()? as f32, p.get(3)?.as_f64()? as f32])
                })
                .collect()
        });
        let fire = o.get("fire").and_then(J::as_array).map(|a| a.iter().filter_map(|c| c.as_str().map(str::to_string)).collect());
        Spec {
            res: o.get("res").and_then(J::as_u64).map_or(d.res, |x| x as u32),
            buoyancy: f32_of(o, "buoyancy", d.buoyancy),
            weight: f32_of(o, "weight", d.weight),
            cooling: f32_of(o, "cooling", d.cooling),
            dissipation: f32_of(o, "dissipation", d.dissipation),
            wind: v3_of(o, "wind").unwrap_or(d.wind),
            wind_pull: f32_of(o, "windPull", d.wind_pull),
            wind_height: f32_of(o, "windHeight", d.wind_height),
            vorticity: f32_of(o, "vorticity", d.vorticity),
            drag: f32_of(o, "drag", d.drag),
            turbulence: f32_of(o, "turbulence", d.turbulence),
            turbulence_size: f32_of(o, "turbulenceSize", d.turbulence_size),
            sources: sources.unwrap_or_default(),
            puffs: puffs.unwrap_or_default(),
            absorb: f32_of(o, "absorb", d.absorb),
            seed: o.get("seed").and_then(J::as_u64).map_or(d.seed, |x| x as u32),
            pressure_iters: o.get("pressureIters").and_then(J::as_u64).map_or(d.pressure_iters, |x| x as u32),
            threshold: f32_of(o, "threshold", d.threshold),
            erode: f32_of(o, "erode", d.erode),
            erode_size: f32_of(o, "erodeSize", d.erode_size),
            opacity: f32_of(o, "opacity", d.opacity),
            lit: str_of(o, "lit", &d.lit),
            mid: str_of(o, "mid", &d.mid),
            dark: str_of(o, "dark", &d.dark),
            fire: fire.unwrap_or(d.fire),
            fire_at: v3_of(o, "fireAt").unwrap_or(d.fire_at),
            steps: o.get("steps").and_then(J::as_u64).map_or(d.steps, |x| x as u32),
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct VolumeMaterial {
    #[uniform(0)]
    p: VolumeParams,
    #[texture(1, dimension = "3d")]
    #[sampler(2)]
    field: Handle<Image>,
}

#[derive(ShaderType, Clone, Copy, Default, PartialEq, Debug)]
struct VolumeParams {
    center: Vec3,
    time: f32,
    size: Vec3,
    steps: f32,
    lit: Vec4,
    mid: Vec4,
    dark: Vec4,
    fire0: Vec4,
    fire1: Vec4,
    fire2: Vec4,
    fire_at: Vec4,
    /// the paint look's bands (0: no paint look, a soft volume)
    bands: f32,
    threshold: f32,
    erode: f32,
    erode_size: f32,
    opacity: f32,
    seed: f32,
    /// the density its shadow sees, as a share of what's drawn (0: casts none)
    shadow: f32,
    _b: f32,
    /// toward the sun (xyz)
    sun: Vec4,
}

impl Material for VolumeMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://raea_sys_volume/volume.wesl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
    fn enable_prepass() -> bool {
        false
    }
    /// in the lights' shadow maps: volume_shadow.wesl marches each texel's ray and writes where the volume stops
    /// the light (a volume with `shadow` 0 is a NotShadowCaster, so it draws nothing there)
    fn prepass_fragment_shader() -> ShaderRef {
        "embedded://raea_sys_volume/volume_shadow.wesl".into()
    }
    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        if let Some(d) = descriptor.depth_stencil.as_mut() {
            d.depth_write_enabled = Some(true);
        }
        Ok(())
    }
}

/// A volume row's entity: its simulation, material and texture.
#[derive(Component)]
pub struct Volume {
    sim: Sim,
    mat: Handle<VolumeMaterial>,
    field: Handle<Image>,
    shown: Option<(u32, [i32; 3], u32)>,
}

fn spec_of(v: &BTreeMap<String, V>) -> Spec {
    match v.get("sim") {
        Some(V::S(s)) => match serde_json::from_str::<J>(s) {
            Ok(o) => Spec::from_json(&o),
            Err(e) => {
                eprintln!("raea: a volume's sim spec didn't read ({e}); the defaults stand");
                Spec::default()
            }
        },
        _ => Spec::default(),
    }
}

fn placed(v: &BTreeMap<String, V>) -> (Vec3, Vec3) {
    let size = Vec3::new(num(v, "scale_x", 1.0), num(v, "scale_y", 1.0), num(v, "scale_z", 1.0)).abs().max(Vec3::splat(1e-3));
    (Vec3::new(num(v, "x", 0.0), num(v, "y", 0.0) + size.y / 2.0, num(v, "z", 0.0)), size)
}

fn lin(h: &str, d: [f32; 4]) -> Vec4 {
    Vec4::from_array(colour(Some(&V::S(h.to_string())), d).to_linear().to_f32_array())
}

fn params(v: &BTreeMap<String, V>, s: &Spec, bands: f32, sun: Vec3) -> VolumeParams {
    let (center, size) = placed(v);
    let f = |k: usize| lin(s.fire.get(k).map(String::as_str).unwrap_or("#ffffff"), [1.0, 0.5, 0.1, 1.0]);
    VolumeParams {
        center,
        time: num(v, "time", 0.0),
        size,
        steps: s.steps.clamp(16, 256) as f32,
        lit: lin(&s.lit, [0.3, 0.3, 0.3, 1.0]),
        mid: lin(&s.mid, [0.2, 0.2, 0.2, 1.0]),
        dark: lin(&s.dark, [0.1, 0.1, 0.1, 1.0]),
        fire0: f(0),
        fire1: f(1),
        fire2: f(2),
        fire_at: Vec4::new(s.fire_at[0], s.fire_at[1], s.fire_at[2], if s.fire.is_empty() { 0.0 } else { 1.0 }),
        bands,
        threshold: s.threshold,
        erode: s.erode,
        erode_size: s.erode_size.max(0.01),
        opacity: s.opacity,
        seed: (s.seed % 1000) as f32,
        shadow: shadow_of(v),
        sun: sun.normalize_or(Vec3::Y).extend(0.0),
        ..default()
    }
}

/// The density a volume's shadow sees, as a share of what's drawn: its row's `shadow` (1 by default; 0 casts none;
/// up to 4).
fn shadow_of(v: &BTreeMap<String, V>) -> f32 {
    num(v, "shadow", 1.0).clamp(0.0, 4.0)
}

/// A fire row with a `sim` (fire.rs hands it here): a unit box standing on its foot, the simulation, its texture.
pub fn spawn(world: &mut World, e: Entity, vals: &BTreeMap<String, V>) {
    let spec = spec_of(vals);
    let (_, size) = placed(vals);
    let sim = Sim::new(spec.clone(), size);
    let [nx, ny, nz] = sim.n;
    let mut img = Image::new_fill(Extent3d { width: nx as u32, height: ny as u32, depth_or_array_layers: nz as u32 }, TextureDimension::D3, &[0, 255, 0, 255], TextureFormat::Rgba8Unorm, bevy::asset::RenderAssetUsages::default());
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    let field = world.resource_mut::<Assets<Image>>().add(img);
    let mesh = world.resource_mut::<Assets<Mesh>>().add(Mesh::from(Cuboid::from_length(1.0)).translated_by(Vec3::Y * 0.5));
    let mat = world.resource_mut::<Assets<VolumeMaterial>>().add(VolumeMaterial { p: params(vals, &spec, 0.0, Vec3::Y), field: field.clone() });
    world.entity_mut(e).insert((Mesh3d(mesh), MeshMaterial3d(mat.clone()), transform_of(vals), Volume { sim, mat, field, shown: None }));
    // it casts shadows (and stops the sun's shafts in the fog) unless its `shadow` is 0
    if shadow_of(vals) <= 0.0 {
        world.entity_mut(e).insert(NotShadowCaster);
    }
}

/// Each volume stepped to its row's time, its texture lit by the sun and its material told the paint look's bands.
#[allow(clippy::type_complexity)]
pub fn sync_volumes(
    frame: Res<FrameRows>,
    mut q: Query<(Entity, &mut Volume)>,
    suns: Query<(&GlobalTransform, &DirectionalLight)>,
    cams: Query<Entity, (With<Camera3d>, Without<LookCam>)>,
    mut mats: ResMut<Assets<VolumeMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    if q.is_empty() {
        return;
    }
    // toward the sun (a directional light shines along its -z): the brightest directional light, not the query's first
    // (a scene with a fill light lit its clouds from whichever came first, and that changed with the plugins added:
    // prechorus's clouds lit from its fill while RenderDiagnosticsPlugin was on)
    let sun = suns.iter().max_by(|a, b| a.1.illuminance.total_cmp(&b.1.illuminance)).map(|(g, _)| g.back().as_vec3()).unwrap_or(Vec3::Y);
    let bands = cams
        .single()
        .ok()
        .and_then(|c| frame.vals(c))
        .filter(|v| matches!(v.get("frame_look"), Some(V::S(s)) if s == "paint"))
        .map(|v| num(&v, "paint_bands", 3.0).clamp(1.0, 8.0))
        .unwrap_or(0.0);
    for (e, mut vol) in &mut q {
        let Some(v) = frame.vals(e) else { continue };
        let (_, size) = placed(&v);
        let t = num(&v, "time", 0.0);
        vol.sim.run_to(t, size);
        if raea_io_trace::stat("RAEA_VOLUME_DEBUG") {
            let sim = &vol.sim;
            let (mut sum, mut top, mut tmax) = (0.0f32, 0usize, 0.0f32);
            for (i, d) in sim.d.iter().enumerate() {
                sum += d;
                if *d > 0.1 {
                    top = top.max((i / sim.n[0]) % sim.n[1]);
                }
                tmax = tmax.max(sim.t[i]);
            }
            let vmax = sim.v.iter().cloned().fold(0.0f32, f32::max);
            eprintln!("volume t={t} step={} n={:?} cell={} sum={sum} top={top} tmax={tmax} vmax={vmax}", sim.step, sim.n, sim.cell);
        }
        let key = (vol.sim.step, [(sun.x * 1000.0) as i32, (sun.y * 1000.0) as i32, (sun.z * 1000.0) as i32], 0);
        let mut changed = false;
        if vol.shown != Some(key) {
            vol.shown = Some(key);
            let data = vol.sim.texels(sun);
            if let Some(mut img) = images.get_mut(&vol.field) {
                img.data = Some(data);
            }
            changed = true;
        }
        let np = params(&v, &vol.sim.spec, bands, sun);
        // a new texture is a new bind group too: the material is touched whenever either changes
        if changed || mats.get(&vol.mat).is_some_and(|m| m.p != np) {
            if let Some(mut m) = mats.get_mut(&vol.mat) {
                m.p = np;
            }
        }
    }
}
