//! The scene as the GPU's ray tracer sees it (wgpu's ray queries; on Apple silicon the hardware's): a BLAS a mesh,
//! built from the mesh's own vertex and index buffers where Bevy keeps them, and a TLAS of every visible, opaque mesh
//! instance each frame. Shadows traced against the real geometry, not a shadow map or the screen: raea-render's passes.rs's `sun`
//! (how much of the sun's disc each point sees) and `sky` (how much of its hemisphere is open) read it.
//! Glass and other see-through materials stay out of it, so the sun and the sky reach through windows.
//! Only built when a traced pass is asked for (passes::wanted), and only where the device has ray queries.

// traced ambient occlusion, the TLAS's first reader; raea GI's world probes (RAEA_GI=1, docs/realism.feature LT2)
pub mod gi;
// <Pathtracer>'s samples and media, read by raea-sys-fog and the render (docs/nobuild.feature NB1a)
pub mod pathtracer;
// raea GI's world cache: a hash grid of the light on the surfaces rays hit (LT3); its layout read by raea-sys-gi
pub mod cache;
// the sun's far shadow map, past the last cascade (RAEA_SUN_FAR, LT21)
pub mod far;
// what a hit sees: each instance's averaged albedo and emission (LT4's first tier)
pub mod records;
pub mod traced_ao;
// a traced pass's GPU time, read back a frame later (raea GI's and the far sun's stats)
mod timer;

/// Every traced pass asked for: traced AO (RAEA_TRACED_AO), raea GI's probes (RAEA_GI), the sun's far shadow map
/// (RAEA_SUN_FAR). Nothing when none is.
pub fn passes(app: &mut App) {
    for install in [traced_ao::install as fn(&mut App), gi::install, far::install] {
        install(app);
    }
}

use bevy::{
    asset::AssetId,
    camera::visibility::InheritedVisibility,
    mesh::{Mesh, Mesh3d, PrimitiveTopology},
    platform::collections::HashMap,
    prelude::*,
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        mesh::{
            allocator::{MeshAllocator, MeshAllocatorSettings},
            RenderMesh,
        },
        render_asset::ExtractedAssets,
        render_resource::*,
        renderer::{RenderDevice, RenderQueue},
        Render, RenderApp, RenderSystems,
    },
};

/// Every visible opaque mesh instance (its mesh and world transform), from the main world each frame.
#[derive(Resource, Default, Clone, ExtractResource)]
#[extract_app(bevy::render::RenderApp)]
struct Instances(Vec<(AssetId<Mesh>, Mat4, u8, records::Record)>);

/// an instance's ray mask: every instance has ALL; CASTS where it casts shadows (no NotShadowCaster), which is all a
/// sun ray tests, so a film's shadow-only stand-ins and its non-casting walls shade the sun as its shadow maps do
pub const CASTS: u8 = 1;
pub const ALL: u8 = 2;

/// The sun: the direction to it, from the first directional light that casts shadows.
#[derive(Resource, Default, Clone, Copy, ExtractResource)]
#[extract_app(bevy::render::RenderApp)]
pub struct Sun(pub Vec3);

/// a BLAS a mesh, and how many batches have been built (a mesh changed keeps its id, so the TLAS knows by this)
#[derive(Resource, Default)]
struct Blases(HashMap<AssetId<Mesh>, Blas>, u64);

/// The ray scene's CPU cost so far, for raea GI's stats (gi.rs `report`; LT2): microseconds and calls of `gather`
/// (the main world's instance list), `build_tlas` and `build_blas` (counted only when it builds), in that order.
pub(crate) static CPU_US: [std::sync::atomic::AtomicU64; 6] = [const { std::sync::atomic::AtomicU64::new(0) }; 6];
fn spent(k: usize, since: std::time::Instant) {
    CPU_US[2 * k].fetch_add(since.elapsed().as_micros() as u64, std::sync::atomic::Ordering::Relaxed);
    CPU_US[2 * k + 1].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// The TLAS, for the traced passes to bind: one kept from frame to frame (LT3), rebuilt only when the instances change.
#[derive(Resource)]
pub struct Scene(pub Tlas);

/// Each TLAS instance's record (records.rs), at its custom index: 32 bytes each, room for as many as the TLAS
#[derive(Resource)]
pub struct Records(pub Buffer);

/// How many times the TLAS was built and how many instances it last held, for raea GI's stats (gi.rs `report`)
pub(crate) static TLAS_BUILT: [std::sync::atomic::AtomicU64; 2] = [const { std::sync::atomic::AtomicU64::new(0) }; 2];

/// What the kept TLAS holds: each instance's BLAS (its mesh), its transform as the TLAS takes it, and its mask; and how
/// many instances it has room for
#[derive(Default)]
struct Held {
    instances: Vec<(AssetId<Mesh>, [f32; 12], u8)>,
    room: u32,
    /// Blases' batch count it was built against
    batches: u64,
    /// the records last written (Records), in the TLAS's order, and their buffer
    records: Vec<records::Record>,
    record_buf: Option<Buffer>,
}

/// A traced pass's reach from its variable, in metres, if the variable is set: its number when over `least`, else `or`.
pub(crate) fn reach(var: &str, least: f32, or: f32) -> Option<f32> {
    let v = std::env::var(var).ok()?;
    Some(v.parse::<f32>().ok().filter(|r| *r > least).unwrap_or(or))
}

pub fn install(app: &mut App) {
    // (once, whether a traced pass or traced AO asked first)
    if app.world().contains_resource::<Instances>() {
        return;
    }
    app.init_resource::<Instances>().init_resource::<Sun>().add_plugins((ExtractResourcePlugin::<Instances>::default(), ExtractResourcePlugin::<Sun>::default())).add_systems(PostUpdate, gather.after(bevy::transform::TransformSystems::Propagate));
    let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
    // the meshes' buffers as BLAS inputs (set before any mesh is allocated)
    if let Some(mut s) = render.world_mut().get_resource_mut::<MeshAllocatorSettings>() {
        s.extra_buffer_usages |= BufferUsages::BLAS_INPUT;
    } else {
        render.world_mut().insert_resource(MeshAllocatorSettings { extra_buffer_usages: BufferUsages::BLAS_INPUT, ..default() });
    }
    render.init_resource::<Blases>().add_systems(
        Render,
        (
            build_blas.in_set(RenderSystems::PrepareResources),
            build_tlas.in_set(RenderSystems::PrepareResources).after(build_blas),
        ),
    );
}

/// see-through: a base colour less than opaque, or transmitting (the glass in the windows). Not by alpha mode: raea's
/// opaque materials are drawn premultiplied at alpha 1.
fn see_through(m: &StandardMaterial) -> bool {
    m.base_color.alpha() < 0.99 || m.specular_transmission > 0.0 || m.diffuse_transmission > 0.0
}

#[allow(clippy::too_many_arguments)]
fn gather(
    meshes: Query<(&Mesh3d, &GlobalTransform, &InheritedVisibility, Option<&MeshMaterial3d<StandardMaterial>>, Has<bevy::light::NotShadowCaster>)>,
    lights: Query<(&DirectionalLight, &GlobalTransform)>,
    mats: Res<Assets<StandardMaterial>>,
    images: Res<Assets<Image>>,
    mut means: Local<HashMap<AssetId<Image>, Vec3>>,
    mut instances: ResMut<Instances>,
    mut sun: ResMut<Sun>,
) {
    let t0 = std::time::Instant::now();
    instances.0.clear();
    for (mesh, t, vis, mat, no_shadow) in &meshes {
        let mat = mat.and_then(|m| mats.get(&m.0));
        if !vis.get() || mat.is_some_and(see_through) {
            continue;
        }
        let rec = mat.map_or(records::STAND_IN, |m| records::record(m, &images, &mut means));
        instances.0.push((mesh.id(), t.to_matrix(), if no_shadow { ALL } else { ALL | CASTS }, rec));
    }
    if let Some((_, t)) = lights.iter().find(|(l, _)| l.shadow_maps_enabled).or_else(|| lights.iter().next()) {
        sun.0 = t.back().as_vec3();
    }
    spent(0, t0);
    if std::env::var_os("RAEA_RT_DEBUG").is_some() {
        let all: Vec<_> = lights.iter().map(|(l, t)| (l.shadow_maps_enabled, t.back().as_vec3())).collect();
        let skipped = meshes.iter().filter(|(_, _, v, m, _)| v.get() && m.and_then(|m| mats.get(&m.0)).is_some_and(see_through)).count();
        eprintln!("raea rt: directional lights (shadows, to light) {all:?}; see-through skipped {skipped}; hidden {}", meshes.iter().filter(|(_, _, v, _, _)| !v.get()).count());
        for (_, _, v, m, _) in &meshes {
            if let Some(m) = m.and_then(|m| mats.get(&m.0)).filter(|m| v.get() && see_through(m)) {
                eprintln!("raea rt: see-through: alpha_mode {:?} base alpha {} transmission {}", m.alpha_mode, m.base_color.alpha(), m.specular_transmission);
            }
        }
    }
}

/// a BLAS for each mesh in the scene that lacks one (a mesh changed or gone loses its own first): triangle lists, indexed
/// or not, with positions first in the vertex, built from the GPU's copy where Bevy keeps it. Asked of every instance,
/// not only of meshes extracted this frame: one extracted before its buffers were placed is picked up later.
fn build_blas(
    mut blases: ResMut<Blases>,
    instances: Option<Res<Instances>>,
    extracted: Res<ExtractedAssets<RenderMesh>>,
    meshes: Res<bevy::render::render_asset::RenderAssets<RenderMesh>>,
    alloc: Res<MeshAllocator>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    if !device.features().contains(WgpuFeatures::EXPERIMENTAL_RAY_QUERY) {
        return;
    }
    let t0 = std::time::Instant::now();
    for id in extracted.removed.iter().chain(extracted.modified.iter()) {
        blases.0.remove(id);
    }
    let Some(instances) = instances else { return };
    let mut wanted: Vec<AssetId<Mesh>> = instances.0.iter().map(|(id, ..)| *id).filter(|id| !blases.0.contains_key(id)).collect();
    wanted.sort_unstable();
    wanted.dedup();
    let mut made = Vec::new();
    for id in wanted {
        let debug = std::env::var_os("RAEA_RT_DEBUG").is_some();
        let Some(mesh) = meshes.get(id) else {
            if debug { eprintln!("raea rt: {id:?} has no RenderMesh") }
            continue;
        };
        // indexed or not (raea's own shapes come unindexed)
        let Some(v) = alloc.mesh_vertex_slice(&id) else {
            if debug { eprintln!("raea rt: {id:?} has no vertex slice") }
            continue;
        };
        let ix = mesh.index_format().and_then(|f| alloc.mesh_index_slice(&id).map(|ix| (ix, f)));
        if mesh.indexed() && ix.is_none() {
            continue;
        }
        let traceable = mesh.primitive_topology() == PrimitiveTopology::TriangleList && mesh.layout.0.attribute_ids().first() == Some(&Mesh::ATTRIBUTE_POSITION.id);
        if !traceable || v.range.is_empty() || ix.as_ref().is_some_and(|(ix, _)| ix.range.is_empty()) {
            if debug { eprintln!("raea rt: {id:?} not traceable: {:?} first {:?}", mesh.primitive_topology(), mesh.layout.0.attribute_ids().first()) }
            continue;
        }
        let size = BlasTriangleGeometrySizeDescriptor {
            vertex_format: Mesh::ATTRIBUTE_POSITION.format,
            vertex_count: v.range.len() as u32,
            index_format: ix.as_ref().map(|(_, f)| *f),
            index_count: ix.as_ref().map(|(ix, _)| ix.range.len() as u32),
            flags: AccelerationStructureGeometryFlags::OPAQUE,
        };
        let blas = device.wgpu_device().create_blas(
            &CreateBlasDescriptor { label: Some("raea rt blas"), flags: AccelerationStructureFlags::PREFER_FAST_TRACE, update_mode: AccelerationStructureUpdateMode::Build },
            BlasGeometrySizeDescriptors::Triangles { descriptors: vec![size.clone()] },
        );
        blases.0.insert(id, blas);
        made.push((id, v, ix, size, mesh.layout.0.layout().array_stride));
    }
    if made.is_empty() {
        return;
    }
    let entries: Vec<_> = made
        .iter()
        .map(|(id, v, ix, size, stride)| BlasBuildEntry {
            blas: &blases.0[id],
            geometry: BlasGeometries::TriangleGeometries(vec![BlasTriangleGeometry {
                size,
                vertex_buffer: v.buffer,
                first_vertex: v.range.start,
                vertex_stride: *stride,
                index_buffer: ix.as_ref().map(|(ix, _)| -> &wgpu::Buffer { ix.buffer }),
                first_index: ix.as_ref().map(|(ix, _)| ix.range.start),
                transform_buffer: None,
                transform_buffer_offset: None,
            }]),
        })
        .collect();
    let mut enc = device.create_command_encoder(&CommandEncoderDescriptor { label: Some("raea rt blas") });
    enc.build_acceleration_structures(&entries, &[]);
    queue.submit([enc.finish()]);
    blases.1 += 1;
    spent(2, t0);
}

/// The TLAS: every instance whose mesh has a BLAS. One is kept (LT3): built again only when an instance came, went,
/// moved or changed its mask, so a still scene costs a comparison a frame; made anew only when it runs out of room,
/// with half as much again to spare. Each instance's custom index is its place in the list, where LT4's record table
/// will keep what a hit on it sees.
fn build_tlas(mut commands: Commands, instances: Option<Res<Instances>>, blases: Res<Blases>, scene: Option<ResMut<Scene>>, mut held: Local<Held>, device: Res<RenderDevice>, queue: Res<RenderQueue>) {
    let Some(instances) = instances else { return };
    if !device.features().contains(WgpuFeatures::EXPERIMENTAL_RAY_QUERY) {
        return;
    }
    let t0 = std::time::Instant::now();
    // the instances whose mesh has its BLAS built, and those still waiting for one
    let (mut with, mut without) = (Vec::new(), Vec::new());
    for i in &instances.0 {
        if blases.0.contains_key(&i.0) { with.push(i) } else { without.push(i) }
    }
    let now: Vec<(AssetId<Mesh>, [f32; 12], u8)> = with.iter().map(|(id, m, mask, _)| (*id, m.transpose().to_cols_array()[..12].try_into().unwrap(), *mask)).collect();
    let recs: Vec<records::Record> = with.iter().map(|(.., r)| *r).collect();
    if std::env::var_os("RAEA_RT_DEBUG").is_some() {
        for (id, ..) in without {
            eprintln!("raea rt: no BLAS for {id:?}");
        }
    }
    // the records in the TLAS's order, written when they change: a material can change with no instance moving
    if held.records != recs || held.record_buf.is_none() {
        if held.record_buf.as_ref().is_none_or(|b| b.size() / 32 < recs.len() as u64) {
            let n = (recs.len() * 3 / 2).max(64) as u64;
            let b = device.create_buffer(&BufferDescriptor { label: Some("raea rt records"), size: n * 32, usage: BufferUsages::STORAGE | BufferUsages::COPY_DST, mapped_at_creation: false });
            commands.insert_resource(Records(b.clone()));
            held.record_buf = Some(b);
        }
        let bytes: Vec<u8> = recs.iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
        if let (false, Some(b)) = (bytes.is_empty(), held.record_buf.as_ref()) {
            queue.write_buffer(b, 0, &bytes);
        }
        held.records = recs;
    }
    if scene.is_some() && now == held.instances && held.batches == blases.1 {
        spent(1, t0);
        return;
    }
    // out of room (or the first): a new TLAS, half as much again as now
    let mut made = None;
    if scene.is_none() || now.len() as u32 > held.room {
        held.room = (now.len() as u32 * 3 / 2).max(64);
        made = Some(device.wgpu_device().create_tlas(&CreateTlasDescriptor { label: Some("raea rt tlas"), flags: AccelerationStructureFlags::PREFER_FAST_TRACE, update_mode: AccelerationStructureUpdateMode::Build, max_instances: held.room }));
    }
    let mut scene = scene;
    let (tlas, was) = match (made.as_mut(), scene.as_deref_mut()) {
        (Some(t), _) => (t, 0),
        (None, Some(s)) => (&mut s.0, held.instances.len()),
        (None, None) => return,
    };
    // each slot this frame's instance, or empty where last frame's list was longer
    for i in 0..now.len().max(was) {
        *tlas.get_mut_single(i).unwrap() = now.get(i).map(|(id, t, mask)| TlasInstance::new(&blases.0[id], *t, i as u32, *mask));
    }
    let mut enc = device.create_command_encoder(&CommandEncoderDescriptor { label: Some("raea rt tlas") });
    enc.build_acceleration_structures(&[], [&*tlas]);
    queue.submit([enc.finish()]);
    spent(1, t0);
    TLAS_BUILT[0].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    TLAS_BUILT[1].store(now.len() as u64, std::sync::atomic::Ordering::Relaxed);
    if std::env::var_os("RAEA_RT_DEBUG").is_some() {
        eprintln!("raea rt: {} instances, {} with a BLAS ({} BLAS)", instances.0.len(), now.len(), blases.0.len());
    }
    held.instances = now;
    held.batches = blases.1;
    if let Some(t) = made {
        commands.insert_resource(Scene(t));
    }
}
