use bevy_asset::Handle;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{component::Component, prelude::ReflectComponent, template::FromTemplate};
use bevy_mesh::Mesh;
use bevy_pbr::{MeshMaterial3d, StandardMaterial};
use bevy_reflect::{prelude::ReflectDefault, Reflect};
use bevy_render::sync_world::SyncToRenderWorld;
use bevy_transform::components::Transform;
use derive_more::derive::From;

/// A mesh component used for raytracing.
///
/// The mesh used in this component must have [`Mesh::enable_raytracing`] set to true,
/// use the following set of vertex attributes: `{POSITION, NORMAL, UV_0, TANGENT}`, use [`bevy_mesh::PrimitiveTopology::TriangleList`],
/// and use [`bevy_mesh::Indices::U32`].
///
/// The material used for this entity must be [`MeshMaterial3d<StandardMaterial>`].
#[derive(
    Component, FromTemplate, Clone, Debug, Default, Deref, DerefMut, Reflect, PartialEq, Eq, From,
)]
#[reflect(Component, Default, Clone, PartialEq)]
#[require(MeshMaterial3d<StandardMaterial>, Transform, SyncToRenderWorld)]
pub struct RaytracingMesh3d(pub Handle<Mesh>);

/// raea: the sun in a camera's environment map, found in its panorama (a small disk far brighter than the sky). Solari
/// samples it as a directional light, so glossy and diffuse surfaces get its highlights and shadows, which reflection
/// rays alone find too rarely for the denoiser to keep; inside its disk the environment map then shows `sky`, so its
/// light isn't counted twice. Rays that don't sample lights (the path tracer's camera rays and mirror bounces) still see
/// the panorama's own sun.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct EnvironmentSun {
    /// Toward the sun, in the panorama's own space: the world's before the environment map light's rotation.
    pub direction: bevy_math::Vec3,
    /// The sun disk's angular diameter, in radians.
    pub angular_size: f32,
    /// The illuminance it adds, linear RGB, before the environment map light's intensity.
    pub illuminance: bevy_math::Vec3,
    /// The sky's radiance around it, linear RGB, before the intensity: what the environment map shows inside the disk.
    pub sky: bevy_math::Vec3,
}

/// raea: the openings a camera's environment map lights the scene through (a skylight, a window), in world space: each
/// a parallelogram, a corner and its two neighbours. Solari samples each as a light (points on the opening, lit by the
/// environment map along the way through it), so a room lit by the sky through a small opening gets that light as
/// direct light, which rays bounced at random find too rarely for the denoiser to keep; a ray that leaves through an
/// opening is weighted against that sampling (MIS), so the light counts once. Light passes both ways; up to
/// MAX_SKY_PORTALS are used.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct SkyPortals(pub Vec<[bevy_math::Vec3; 3]>);

/// raea: how many sky portals the environment uniform holds (bindings.wesl's array)
pub const MAX_SKY_PORTALS: usize = 8;

// raea: a traced vertex's size in bytes: position, normal, uv, tangent and the vertex colour (scene/bindings.wesl
// PackedVertex). The BLAS's stride and the binding array's dummy buffer both read it: the dummy left at 48 when the
// colour came in made the GPU refuse the frame after a part's mesh slab was freed (Stay, frame 1699)
pub(crate) const VERTEX_BYTES: u64 = 64;
