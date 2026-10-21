//! The scene as the GPU's ray tracer sees it (wgpu's ray queries; on Apple silicon the hardware's): a BLAS a mesh,
//! built from the mesh's own vertex and index buffers where Bevy keeps them, and a TLAS of every visible, opaque mesh
//! instance each frame. Shadows traced against the real geometry, not a shadow map or the screen: passes.rs's `sun`
//! (how much of the sun's disc each point sees) and `sky` (how much of its hemisphere is open) read it.
//! Glass and other see-through materials stay out of it, so the sun and the sky reach through windows.
//! Only built when a traced pass is asked for (passes::wanted), and only where the device has ray queries.

use bevy::{
    asset::AssetId,
    camera::visibility::InheritedVisibility,
    mesh::{Mesh, Mesh3d, PrimitiveTopology},
    platform::collections::HashMap,
    prelude::*,
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        mesh::{
            allocator::{allocate_and_free_meshes, MeshAllocator, MeshAllocatorSettings},
            RenderMesh,
        },
        render_asset::{prepare_assets, ExtractedAssets},
        render_resource::*,
        renderer::{RenderDevice, RenderQueue},
        Render, RenderApp, RenderSystems,
    },
};

/// Every visible opaque mesh instance (its mesh and world transform), from the main world each frame.
#[derive(Resource, Default, Clone, ExtractResource)]
struct Instances(Vec<(AssetId<Mesh>, Mat4, u8)>);

/// an instance's ray mask: every instance has ALL; CASTS where it casts shadows (no NotShadowCaster), which is all a
/// sun ray tests, so a film's shadow-only stand-ins and its non-casting walls shade the sun as its shadow maps do
pub(super) const CASTS: u8 = 1;
pub(super) const ALL: u8 = 2;

/// The sun: the direction to it, from the first directional light that casts shadows.
#[derive(Resource, Default, Clone, Copy, ExtractResource)]
pub(super) struct Sun(pub Vec3);

#[derive(Resource, Default)]
struct Blases(HashMap<AssetId<Mesh>, Blas>);

/// This frame's TLAS, for the traced passes to bind.
#[derive(Resource)]
pub(super) struct Scene(pub Tlas);

pub(super) fn install(app: &mut App) {
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

fn gather(
    meshes: Query<(&Mesh3d, &GlobalTransform, &InheritedVisibility, Option<&MeshMaterial3d<StandardMaterial>>, Has<bevy::light::NotShadowCaster>)>,
    lights: Query<(&DirectionalLight, &GlobalTransform)>,
    mats: Res<Assets<StandardMaterial>>,
    mut instances: ResMut<Instances>,
    mut sun: ResMut<Sun>,
) {
    instances.0.clear();
    for (mesh, t, vis, mat, no_shadow) in &meshes {
        if !vis.get() || mat.and_then(|m| mats.get(&m.0)).is_some_and(see_through) {
            continue;
        }
        instances.0.push((mesh.id(), t.to_matrix(), if no_shadow { ALL } else { ALL | CASTS }));
    }
    if let Some((_, t)) = lights.iter().find(|(l, _)| l.shadow_maps_enabled).or_else(|| lights.iter().next()) {
        sun.0 = t.back().as_vec3();
    }
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
    for id in extracted.removed.iter().chain(extracted.modified.iter()) {
        blases.0.remove(id);
    }
    let Some(instances) = instances else { return };
    let mut wanted: Vec<AssetId<Mesh>> = instances.0.iter().map(|(id, _, _)| *id).filter(|id| !blases.0.contains_key(id)).collect();
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
}

/// the TLAS: every instance whose mesh has a BLAS, rebuilt each frame (things move)
fn build_tlas(mut commands: Commands, instances: Option<Res<Instances>>, blases: Res<Blases>, device: Res<RenderDevice>, queue: Res<RenderQueue>) {
    let Some(instances) = instances else { return };
    if !device.features().contains(WgpuFeatures::EXPERIMENTAL_RAY_QUERY) {
        return;
    }
    let present: Vec<_> = instances.0.iter().filter_map(|(id, m, mask)| blases.0.get(id).map(|b| (b, m, *mask))).collect();
    if std::env::var_os("RAEA_RT_DEBUG").is_some() {
        for (id, _, _) in instances.0.iter().filter(|(id, _, _)| !blases.0.contains_key(id)) {
            eprintln!("raea rt: no BLAS for {id:?}");
        }
    }
    let mut tlas = device.wgpu_device().create_tlas(&CreateTlasDescriptor {
        label: Some("raea rt tlas"),
        flags: AccelerationStructureFlags::PREFER_FAST_TRACE,
        update_mode: AccelerationStructureUpdateMode::Build,
        max_instances: present.len().max(1) as u32,
    });
    for (i, (blas, m, mask)) in present.iter().enumerate() {
        let t: [f32; 12] = m.transpose().to_cols_array()[..12].try_into().unwrap();
        *tlas.get_mut_single(i).unwrap() = Some(TlasInstance::new(blas, t, 0, *mask));
    }
    let mut enc = device.create_command_encoder(&CommandEncoderDescriptor { label: Some("raea rt tlas") });
    enc.build_acceleration_structures(&[], [&tlas]);
    queue.submit([enc.finish()]);
    if std::env::var_os("RAEA_RT_DEBUG").is_some() {
        eprintln!("raea rt: {} instances, {} with a BLAS ({} BLAS)", instances.0.len(), present.len(), blases.0.len());
    }
    commands.insert_resource(Scene(tlas));
}
