//! Fire (`<Fire>` in @raea/motion/3d): a flame drawn as a volume. Each fire is a box, and fire.wgsl marches every
//! pixel's ray through it, adding the light of a flame whose shape is a closed-form function of the point and the
//! row's `time`: a tapering envelope broken into tongues by rising, domain-warped 3D noise. No particles, no history:
//! a frame is a function of its rows, so a render of frame N equals frame N of play.
//!
//! Columns of a fire row (all but x y z optional):
//!   x y z (the flame's foot), scale_x scale_y scale_z (its box: width, height, depth in metres; 1 by default),
//!   time (seconds; the noise's clock), intensity (how bright the core is, as a material's emissiveIntensity: 6 by
//!   default), color | col_r col_g col_b (the flame's body, sRGB; orange by default), seed (which flame), speed (how
//!   fast it rises, 1), steps (samples along each ray, 32), opacity (how much of what's behind its body hides, 1),
//!   visible
//!
//! With a depth prepass on the camera (an `<AmbientOcclusion>` brings one), each ray stops at the scene's surface, so
//! a log or a wall inside the box hides the flame behind it.

use super::*;
use bevy::light::NotShadowCaster;
use bevy::pbr::{Material, MaterialPlugin, MeshMaterial3d};
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub(super) fn install(app: &mut App) {
    bevy::asset::embedded_asset!(app, "fire.wgsl");
    app.add_plugins(MaterialPlugin::<FireMaterial>::default())
        .add_systems(Update, sync_fires.after(super::relational).before(super::tag));
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub(super) struct FireMaterial {
    #[uniform(0)]
    p: FireParams,
    /// the froxel grid the flame is fogged by (froxel_glass.rs GridImages; drawn after the grid's apply pass)
    #[texture(100, dimension = "3d")]
    #[sampler(101)]
    pub grid: Handle<Image>,
    #[texture(102, sample_type = "float", filterable = false)]
    params: Handle<Image>,
}

#[derive(ShaderType, Clone, Copy, Default, PartialEq, Debug)]
struct FireParams {
    /// the box's centre and size (world, metres)
    center: Vec3,
    time: f32,
    size: Vec3,
    intensity: f32,
    /// the flame's body colour, linear
    color: Vec4,
    seed: f32,
    speed: f32,
    steps: f32,
    /// how much of what's behind the flame its body hides (1 by default)
    opacity: f32,
}

impl Material for FireMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://rdb/engine/fire.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        // light added over what's behind, which the flame's own smoke dims a little (premultiplied)
        AlphaMode::Premultiplied
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    /// The flame writes its box's depth where it shows (fire.wgsl discards the rest), so depth of field focuses on
    /// the fire where it stands, not on the wall behind it (a transparent pass writes no depth by default).
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

/// A fire row's entity: its material, so the sync can update it in place.
#[derive(Component)]
pub(super) struct Fire(Handle<FireMaterial>);

/// The box a fire's row puts it in (its foot at x y z, so its centre is half its height up) as the shader takes it:
/// centre and size. The entity's own transform, visibility and placement are sync_meshes's, as any mesh's.
fn placed(v: &BTreeMap<String, V>) -> (Vec3, Vec3) {
    let size = Vec3::new(num(v, "scale_x", 1.0), num(v, "scale_y", 1.0), num(v, "scale_z", 1.0)).abs().max(Vec3::splat(1e-4));
    (Vec3::new(num(v, "x", 0.0), num(v, "y", 0.0) + size.y / 2.0, num(v, "z", 0.0)), size)
}

fn params(v: &BTreeMap<String, V>) -> FireParams {
    let (center, size) = placed(v);
    let c = if v.contains_key("col_r") {
        Color::srgb(num(v, "col_r", 1.0), num(v, "col_g", 0.48), num(v, "col_b", 0.12))
    } else {
        colour(v.get("color"), [1.0, 0.48, 0.12, 1.0])
    }
    .to_linear();
    FireParams {
        center,
        time: num(v, "time", 0.0),
        size,
        intensity: num(v, "intensity", 6.0).max(0.0),
        color: Vec4::new(c.red, c.green, c.blue, 1.0),
        seed: num(v, "seed", 0.0),
        speed: num(v, "speed", 1.0),
        steps: num(v, "steps", 32.0).round().clamp(4.0, 256.0),
        opacity: num(v, "opacity", 1.0).max(0.0),
    }
}

/// A fire row: a unit box standing on its foot (y 0 to 1), placed and scaled as a mesh is, drawn with the flame's
/// material and casting no shadow.
pub(super) fn spawn(world: &mut World, e: Entity, vals: &BTreeMap<String, V>) {
    let mesh = world.resource_mut::<Assets<Mesh>>().add(Mesh::from(Cuboid::from_length(1.0)).translated_by(Vec3::Y * 0.5));
    let fog = world.resource::<super::froxel_glass::GridImages>().clone();
    let mat = world.resource_mut::<Assets<FireMaterial>>().add(FireMaterial { p: params(vals), grid: fog.grid, params: fog.params });
    world.entity_mut(e).insert((Mesh3d(mesh), MeshMaterial3d(mat.clone()), scene3d::transform_of(vals), NotShadowCaster, Fire(mat)));
}

/// The fires shown this frame, for the froxel grid (froxel.rs), which lights its fog with them: centre and time, size
/// and brightness, colour and seed, speed.
pub(super) fn extract_grid_fires(
    mut out: ResMut<super::froxel::GridFires>,
    q: bevy::render::Extract<Query<(&Fire, &InheritedVisibility)>>,
    mats: bevy::render::Extract<Res<Assets<FireMaterial>>>,
) {
    out.0.clear();
    for (f, vis) in &q {
        let Some(m) = mats.get(&f.0).filter(|_| vis.get()) else { continue };
        let p = m.p;
        if p.intensity <= 0.0 {
            continue;
        }
        out.0.push(super::froxel::GpuFire {
            center_time: p.center.extend(p.time),
            size_intensity: p.size.extend(p.intensity),
            color_seed: p.color.truncate().extend(p.seed),
            speed: Vec4::new(p.speed, 0.0, 0.0, 0.0),
        });
    }
}

/// Each fire's flame at its row's place, time, brightness and colour (sync_meshes moves and hides its box).
pub(super) fn sync_fires(frame: Res<FrameRows>, mut mats: ResMut<Assets<FireMaterial>>, q: Query<(Entity, &Fire)>) {
    for (e, f) in &q {
        let Some(v) = frame.vals(e) else { continue };
        let np = params(&v);
        if mats.get(&f.0).is_some_and(|m| m.p != np) {
            if let Some(mut m) = mats.get_mut(&f.0) {
                m.p = np;
            }
        }
    }
}
