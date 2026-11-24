//! RT3 (docs/raytracing.feature): Bevy's Solari, ray-traced lighting, for a scene whose camera asks for it
//! (<SolariLighting> or <Pathtracer> in raea/3d: SolariLighting or Pathtracer under <Camera3d>). Its plugins come in
//! only then: SolariLightingPlugin makes every opaque material deferred, which would change every other scene.
//! The shared GPU device asks for every feature its adapter has (raea-gpu-device), so ray queries are on where the GPU
//! has them; where it doesn't, Solari's plugins say so and the scene draws with raster lighting.
//!
//! Solari traces a mesh only in one layout (positions, normals, UV 0, tangents, nothing else, u32 indices), so each
//! mesh gets a copy in that layout for its RaytracingMesh3d, made once per mesh. Skinned and morphed meshes (a
//! <Character>) are left raster-lit, as Solari 0.20 can't trace them (RT5).

use super::*;
use bevy::camera::CameraMainTextureUsages;
use bevy::core_pipeline::prepass::{DeferredPrepass, DepthPrepass};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::solari::pathtracer::{Pathtracer, PathtracingPlugin};
use bevy::solari::prelude::{RaytracingMesh3d, SolariLighting, SolariPlugins};

pub fn add(app: &mut App) {
    app.add_plugins((SolariPlugins, PathtracingPlugin))
        .init_resource::<Dynamic>()
        .add_systems(PostUpdate, (cameras, meshes, deferred, shadow_maps).before(bevy::camera::CameraUpdateSystems))
        .add_systems(PostUpdate, dynamic.before(meshes));
    // a twin posed after its joints are placed and before this frame's asset events go out, so the traced scene gets
    // the pose with the frame that draws it
    app.add_systems(
        PostUpdate,
        skin_twins.after(bevy::transform::TransformSystems::Propagate).before(bevy::asset::AssetEventSystems).run_if(|d: Res<Dynamic>| d.0),
    );
    // a part's visibility read once it is propagated, on every traced render (it was RT5's trial alone: SG9)
    app.add_systems(PostUpdate, hidden.after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate));
}

/// Whether the traced scene follows what moves (RT5, a trial: <SolariLighting dynamic>, or RAEA_SOLARI_DYNAMIC on every
/// camera; off, everything is as before): a skinned mesh gets a traced twin, posed on the CPU each frame as the GPU
/// skins the drawn one (it casts traced shadows, stops and bounces light). (A hidden part leaving the traced scene was
/// part of this trial until SG9; it is on for every traced render now: `hidden`.) On while any traced camera asks for
/// it; a mesh is decided the first frame it is traced, so ask from the first frame.
#[derive(Resource, Default)]
struct Dynamic(bool);

fn dynamic(mut d: ResMut<Dynamic>, cams: Query<Option<&super::SolariSettings>, Or<(With<SolariLighting>, With<Pathtracer>)>>) {
    let on = cams.iter().any(|s| super::settings::of(s).dynamic);
    if d.0 != on {
        d.0 = on;
    }
}

/// A skinned mesh's traced twin: the mesh's bind pose in Solari's layout and its skin, and the joints last posed from.
#[derive(Component)]
struct SkinTwin {
    twin: Handle<Mesh>,
    pos: Vec<Vec3>,
    nrm: Vec<Vec3>,
    tan: Vec<Vec4>,
    joints: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
    last: Vec<Mat4>,
}

/// The twin of a skinned `src`: its copy in Solari's layout, with the bind pose and skin kept to pose it from.
fn skin_twin(src: &Mesh, mapped: bool) -> Option<(Mesh, SkinTwin)> {
    let Some(Traceable::Copy(mesh)) = traceable(src, mapped) else { return None };
    let v3 = |a: &VertexAttributeValues| match a {
        VertexAttributeValues::Float32x3(v) => Some(v.iter().map(|p| Vec3::from(*p)).collect::<Vec<_>>()),
        _ => None,
    };
    let pos = v3(mesh.attribute(Mesh::ATTRIBUTE_POSITION)?)?;
    let nrm = v3(mesh.attribute(Mesh::ATTRIBUTE_NORMAL)?)?;
    let VertexAttributeValues::Float32x4(tan) = mesh.attribute(Mesh::ATTRIBUTE_TANGENT)? else { return None };
    let tan: Vec<Vec4> = tan.iter().map(|t| Vec4::from(*t)).collect();
    let joints: Vec<[u16; 4]> = match src.attribute(Mesh::ATTRIBUTE_JOINT_INDEX)? {
        VertexAttributeValues::Uint16x4(v) => v.clone(),
        VertexAttributeValues::Uint8x4(v) => v.iter().map(|a| a.map(u16::from)).collect(),
        _ => return None,
    };
    let VertexAttributeValues::Float32x4(weights) = src.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)? else { return None };
    let n = pos.len();
    if nrm.len() != n || tan.len() != n || joints.len() != n || weights.len() != n {
        return None;
    }
    Some((mesh, SkinTwin { twin: Handle::default(), pos, nrm, tan, joints, weights: weights.clone(), last: Vec::new() }))
}

/// Each skinned mesh's twin in this frame's pose: every vertex by its joints' places, as Bevy's skinning shader does
/// (a joint's world place times its inverse bind pose, blended by weight), brought into the mesh entity's own space,
/// since the traced instance is placed by that entity's transform and the skin ignores it.
fn skin_twins(
    mut q: Query<(&GlobalTransform, &bevy::mesh::skinning::SkinnedMesh, &mut SkinTwin), With<RaytracingMesh3d>>,
    places: Query<&GlobalTransform>,
    binds: Res<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (gt, skin, mut tw) in &mut q {
        let Some(inv) = binds.get(&skin.inverse_bindposes) else { continue };
        let to_local = gt.affine().inverse();
        let mats: Vec<Mat4> = skin
            .joints
            .iter()
            .enumerate()
            .map(|(i, j)| {
                let g = places.get(*j).map(|g| g.affine()).unwrap_or(bevy::math::Affine3A::IDENTITY);
                Mat4::from(to_local * g) * inv.get(i).copied().unwrap_or(Mat4::IDENTITY)
            })
            .collect();
        if mats == tw.last {
            continue;
        }
        let n = tw.pos.len();
        let (mut pos, mut nrm, mut tan) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
        for v in 0..n {
            let (ji, w) = (tw.joints[v], tw.weights[v]);
            let mut m = Mat4::ZERO;
            let mut total = 0.0;
            for k in 0..4 {
                if w[k] != 0.0 {
                    if let Some(j) = mats.get(ji[k] as usize) {
                        m += *j * w[k];
                        total += w[k];
                    }
                }
            }
            if total <= 0.0 {
                m = Mat4::IDENTITY;
            }
            let m3 = Mat3::from_mat4(m);
            pos.push(m.transform_point3(tw.pos[v]).to_array());
            nrm.push((m3 * tw.nrm[v]).normalize_or(Vec3::Y).to_array());
            tan.push((m3 * tw.tan[v].truncate()).normalize_or(Vec3::X).extend(tw.tan[v].w).to_array());
        }
        if let Some(mut mesh) = meshes.get_mut(&tw.twin) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
            mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tan);
        }
        tw.last = mats;
    }
}

/// A traced mesh kept out of the traced scene while its part is hidden.
#[derive(Component)]
struct Untraced(Handle<Mesh>);

/// Solari traces every RaytracingMesh3d, shown or not: a hidden part (one a frame leaves out, a `visible={false}`)
/// kept shadowing and bouncing light where it last stood. Hidden, it gives its traced mesh up; shown, it has it back.
/// A part a scene leaves is hidden, not despawned (its row stays), so until 0.1.10 (SG9) every part a film had left
/// stayed in the traced scene unseen: a whole render's later places drew in the shade of all the earlier ones' parts
/// (Stay, far darker than each shot rendered alone; kept-scenes.tsx, the room 0.943 SSIM to its still, 0.997 after).
/// Only a part whose visibility changed, or just traced, is looked at.
fn hidden(
    mut cmd: Commands,
    shown: Query<(Entity, &InheritedVisibility, &RaytracingMesh3d), Or<(Changed<InheritedVisibility>, Added<RaytracingMesh3d>)>>,
    parked: Query<(Entity, &InheritedVisibility, &Untraced), (Without<RaytracingMesh3d>, Or<(Changed<InheritedVisibility>, Added<Untraced>)>)>,
) {
    for (e, v, rt) in &shown {
        if !v.get() {
            cmd.entity(e).insert(Untraced(rt.0.clone())).remove::<RaytracingMesh3d>();
        }
    }
    for (e, v, un) in &parked {
        if v.get() {
            cmd.entity(e).insert(RaytracingMesh3d(un.0.clone())).remove::<Untraced>();
        }
    }
}

/// A camera or mesh already set up (or left out).
#[derive(Component)]
struct Done;

// Solari's camera needs: no MSAA and a main texture it can write to (SolariLighting's docs); Hdr, the prepasses and
// deferred come with SolariLighting's required components. Pathtracer requires only Hdr, but SolariPlugins makes every
// StandardMaterial deferred, so a Pathtracer camera without a deferred prepass panicked on its first frame (Bevy's
// prepass queue unwraps the view's opaque deferred phase): it gets the depth and deferred prepasses too.
fn cameras(mut cmd: Commands, q: Query<(Entity, Option<&CameraMainTextureUsages>, Has<Pathtracer>), (Or<(With<SolariLighting>, With<Pathtracer>)>, Without<Done>)>) {
    for (e, u, path) in &q {
        let usages = u.map_or_else(|| CameraMainTextureUsages::default().0, |u| u.0) | bevy::render::render_resource::TextureUsages::STORAGE_BINDING;
        cmd.entity(e).insert((Msaa::Off, CameraMainTextureUsages(usages), Done));
        if path {
            cmd.entity(e).insert_if_new((DepthPrepass, DeferredPrepass));
        }
    }
}

type Pending = (With<MeshMaterial3d<StandardMaterial>>, Without<RaytracingMesh3d>, Without<Done>);

fn meshes(
    mut cmd: Commands,
    on: Query<(), Or<(With<SolariLighting>, With<Pathtracer>)>>,
    q: Query<(Entity, &Mesh3d, &MeshMaterial3d<StandardMaterial>, Has<bevy::mesh::skinning::SkinnedMesh>, Has<bevy::mesh::morph::MeshMorphWeights>), Pending>,
    mats: Res<Assets<StandardMaterial>>,
    mut assets: ResMut<Assets<Mesh>>,
    mut copies: Local<HashMap<(AssetId<Mesh>, bool), Option<Handle<Mesh>>>>,
    mut said: Local<bool>,
    dynamic: Res<Dynamic>,
) {
    if on.is_empty() {
        return;
    }
    for (e, m, mat, skinned, morphed) in &q {
        // what it adds to or tints is all it does: traced, Solari took it as an opaque card (a wind's dust and air
        // streaks, additive sprites, cut a sunlit wall's light by a quarter). Left untraced it still draws, forward
        match mats.get(&mat.0).map(|m| traced_alpha(m.alpha_mode)) {
            Some(false) => {
                cmd.entity(e).insert(Done);
                continue;
            }
            // its material not made yet: decided on a later frame
            None => continue,
            Some(true) => {}
        }
        if skinned && !morphed && dynamic.0 {
            let mapped = mats.get(&mat.0).is_some_and(|m| m.normal_map_texture.is_some());
            let Some(twin) = assets.get(&m.0).map(|src| skin_twin(src, mapped)) else { continue };
            match twin {
                Some((mesh, mut tw)) => {
                    tw.twin = assets.add(mesh);
                    cmd.entity(e).insert((RaytracingMesh3d(tw.twin.clone()), tw, Done));
                }
                None => {
                    cmd.entity(e).insert(Done);
                }
            }
            continue;
        }
        if skinned || morphed {
            if !*said {
                *said = true;
                eprintln!("raea: <SolariLighting>: a skinned or morphed mesh (a <Character>) draws raster-lit; Bevy 0.20's Solari can't trace it (docs/raytracing.feature RT5)");
            }
            cmd.entity(e).insert(Done);
            continue;
        }
        // each mesh's traceable form, made once: itself when it's in Solari's layout, else a copy; None when it can't
        // be traced (not triangles, no normals, or its data only on the GPU)
        // (a copy by whether its material has a normal map: only that reads the tangents)
        let mapped = mats.get(&mat.0).is_some_and(|m| m.normal_map_texture.is_some());
        let h = match copies.get(&(m.0.id(), mapped)) {
            Some(h) => h.clone(),
            None => {
                let Some(t) = assets.get(&m.0).map(|src| traceable(src, mapped)) else { continue };
                let h = match t {
                    Some(Traceable::Same) => Some(m.0.clone()),
                    Some(Traceable::Copy(mesh)) => Some(assets.add(mesh)),
                    None => None,
                };
                copies.insert((m.0.id(), mapped), h.clone());
                h
            }
        };
        match h {
            Some(h) => cmd.entity(e).insert((RaytracingMesh3d(h), Done)),
            None => cmd.entity(e).insert(Done),
        };
    }
}

/// A light whose shadow maps this module turned off, to turn them back on.
#[derive(Component)]
struct MapsOff;

/// RT10: Solari and the path tracer trace their shadows and never read a light's shadow maps, yet Bevy still draws
/// them each frame (photo 2's sun's cascades: 1.5 ms of a 22 ms live frame). So while every active 3D camera traces its
/// light and nothing is drawn raster-lit by them (a skinned or morphed mesh, a <Character>: RT5), the lights draw none;
/// each one this turned off gets its maps back when that stops. A light marked `volumetric` keeps its maps: the fog's
/// shafts are cut by them (photo 5's skylight through steam).
fn shadow_maps(
    mut cmd: Commands,
    cams: Query<(&Camera, Has<SolariLighting>, Has<Pathtracer>), With<Camera3d>>,
    raster: Query<(), Or<(With<bevy::mesh::skinning::SkinnedMesh>, With<bevy::mesh::morph::MeshMorphWeights>)>>,
    mut dir: Query<(Entity, &mut DirectionalLight, Has<MapsOff>, Has<bevy::light::VolumetricLight>)>,
    mut point: Query<(Entity, &mut PointLight, Has<MapsOff>, Has<bevy::light::VolumetricLight>)>,
    mut spot: Query<(Entity, &mut SpotLight, Has<MapsOff>, Has<bevy::light::VolumetricLight>)>,
) {
    let (mut any, mut all) = (false, true);
    for (c, solari, path) in &cams {
        if c.is_active {
            any = true;
            all &= solari || path;
        }
    }
    let off = any && all && raster.is_empty();
    macro_rules! each {
        ($q:expr) => {
            for (e, mut l, mine, fog) in &mut $q {
                if off && !fog && l.shadow_maps_enabled {
                    l.shadow_maps_enabled = false;
                    cmd.entity(e).insert(MapsOff);
                } else if (!off || fog) && mine {
                    l.shadow_maps_enabled = true;
                    cmd.entity(e).remove::<MapsOff>();
                }
            }
        };
    }
    each!(dir);
    each!(point);
    each!(spot);
}

/// Under Solari, a material that lets light through (`diffuse_transmission`: a leaf, a sheer curtain, a lampshade)
/// is pinned to the deferred path. Left on Auto, Bevy draws it forward, as the G-buffer has no room for transmission,
/// so it skipped Solari's lighting and drew raster-lit over the traced frame, its emissive shown raw: photo 2's lamp
/// and curtain came out white (1.85x the photo's lamp), its leaves flat. Solari 0.20 traces no transmission, so as a
/// G-buffer surface it is lit like any other (the lamp 1.12x, the curtain 1.26x, as the path tracer has them).
fn deferred(on: Query<(), With<SolariLighting>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    if on.is_empty() {
        return;
    }
    use bevy::material::OpaqueRendererMethod;
    let forward: Vec<_> = mats.iter().filter(|(_, m)| m.opaque_render_method == OpaqueRendererMethod::Auto && m.diffuse_transmission > 0.0).map(|(id, _)| id).collect();
    for id in forward {
        if let Some(mut m) = mats.get_mut(id) {
            m.opaque_render_method = OpaqueRendererMethod::Deferred;
        }
    }
}

/// Whether a surface of this alpha mode is in the traced scene: not one that only adds light to what is behind it or
/// multiplies it (a glow, a spark, dust, a tint), which has nothing of its own to block or bounce.
fn traced_alpha(mode: AlphaMode) -> bool {
    !matches!(mode, AlphaMode::Add | AlphaMode::Multiply)
}

enum Traceable {
    Same,
    Copy(Mesh),
}

/// `src` in Solari's layout. A mesh with no tangents of its own gets MikkTSpace's only when `mapped` (its material has
/// a normal map: the one thing Solari reads a tangent for, scene/bindings.wesl); else any unit vector across each
/// normal does, as the layout needs the attribute. MikkTSpace on every traced mesh was most of a ray-traced render's
/// wait for its models (one thread, 0.36 s of a 3.8 s render of a 2.5 s shot).
fn traceable(src: &Mesh, mapped: bool) -> Option<Traceable> {
    if src.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    // a mesh whose data went to the GPU alone (RenderAssetUsages::RENDER_WORLD, as raea-sys-grass makes its blades) can't
    // be read here: Bevy's plain accessors panic on it, so it's read with the try_ ones and left untraced. It still
    // draws into the G-buffer, lit by Solari; it just isn't in the traced scene
    let ids: Vec<_> = src.try_attributes().ok()?.map(|(a, _)| a.id).collect();
    let indices = src.try_indices_option().ok()?;
    let layout = [Mesh::ATTRIBUTE_POSITION.id, Mesh::ATTRIBUTE_NORMAL.id, Mesh::ATTRIBUTE_UV_0.id, Mesh::ATTRIBUTE_TANGENT.id, Mesh::ATTRIBUTE_COLOR.id];
    if ids == layout && matches!(indices, Some(Indices::U32(_))) {
        return Some(Traceable::Same);
    }
    let pos = src.attribute(Mesh::ATTRIBUTE_POSITION)?.clone();
    let n = pos.len();
    let normal = src.attribute(Mesh::ATTRIBUTE_NORMAL)?.clone();
    let uv = src.attribute(Mesh::ATTRIBUTE_UV_0).cloned().unwrap_or(VertexAttributeValues::Float32x2(vec![[0.0, 0.0]; n]));
    let mut out = Mesh::new(PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
    out.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    out.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normal);
    out.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    // its vertex colours, which the base colour is multiplied by traced as in raster (vendored Solari's PackedVertex),
    // or white. Without them a forest's vertex-coloured bark and moss traced grey
    let color = match src.attribute(Mesh::ATTRIBUTE_COLOR) {
        Some(VertexAttributeValues::Float32x4(c)) if c.len() == n => c.clone(),
        _ => vec![[1.0f32; 4]; n],
    };
    out.insert_attribute(Mesh::ATTRIBUTE_COLOR, color);
    let idx: Vec<u32> = match indices {
        Some(Indices::U32(v)) => v.clone(),
        Some(Indices::U16(v)) => v.iter().map(|&i| i as u32).collect(),
        None => (0..n as u32).collect(),
    };
    out.insert_indices(Indices::U32(idx));
    match src.attribute(Mesh::ATTRIBUTE_TANGENT) {
        Some(t) => out.insert_attribute(Mesh::ATTRIBUTE_TANGENT, t.clone()),
        None if mapped => out.generate_tangents().ok()?,
        None => {
            let VertexAttributeValues::Float32x3(normals) = out.attribute(Mesh::ATTRIBUTE_NORMAL)? else { return None };
            let across: Vec<[f32; 4]> = normals.iter().map(|n| Vec3::from(*n).any_orthonormal_vector().extend(1.0).to_array()).collect();
            out.insert_attribute(Mesh::ATTRIBUTE_TANGENT, across);
        }
    }
    Some(Traceable::Copy(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;

    fn quad(usage: RenderAssetUsages) -> Mesh {
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, usage);
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; 3]);
        m.insert_indices(Indices::U16(vec![0, 1, 2]));
        m
    }

    /// A mesh with no tangents and no normal map gets a unit tangent across each normal without MikkTSpace (which
    /// needs UVs' derivatives and was most of a render's wait for its models); with a normal map, MikkTSpace's.
    #[test]
    fn a_mesh_without_a_normal_map_gets_tangents_across_its_normals() {
        let tangents = |mapped| match traceable(&quad(RenderAssetUsages::default()), mapped) {
            Some(Traceable::Copy(m)) => match m.attribute(Mesh::ATTRIBUTE_TANGENT) {
                Some(VertexAttributeValues::Float32x4(t)) => t.clone(),
                _ => panic!("no tangents"),
            },
            _ => panic!("not a copy"),
        };
        for t in tangents(false) {
            let v = Vec3::new(t[0], t[1], t[2]);
            assert!((v.length() - 1.0).abs() < 1e-5 && v.dot(Vec3::Z).abs() < 1e-5 && t[3] == 1.0, "{t:?}");
        }
        assert_eq!(tangents(true).len(), 3);
    }

    /// A mesh's vertex colours go into its traced copy, which the tracer multiplies the base colour by; a mesh with none
    /// is traced white.
    #[test]
    fn vertex_colours_are_traced() {
        let colour = |m: &Mesh| match traceable(m, false) {
            Some(Traceable::Copy(c)) => match c.attribute(Mesh::ATTRIBUTE_COLOR) {
                Some(VertexAttributeValues::Float32x4(v)) => v.clone(),
                _ => panic!("no colour in the copy"),
            },
            _ => panic!("not a copy"),
        };
        assert_eq!(colour(&quad(RenderAssetUsages::default())), vec![[1.0f32; 4]; 3]);
        let mut green = quad(RenderAssetUsages::default());
        green.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.1f32, 0.3, 0.05, 1.0]; 3]);
        assert_eq!(colour(&green), vec![[0.1f32, 0.3, 0.05, 1.0]; 3]);
    }

    #[test]
    fn an_additive_or_multiplying_surface_is_not_traced() {
        // additive dust cards traced as opaque shaded a sunlit wall; opaque, masked and blended surfaces stay traced
        assert!(!traced_alpha(AlphaMode::Add) && !traced_alpha(AlphaMode::Multiply));
        assert!(traced_alpha(AlphaMode::Opaque) && traced_alpha(AlphaMode::Blend) && traced_alpha(AlphaMode::Mask(0.5)));
    }

    #[test]
    fn a_mesh_on_the_gpu_alone_is_left_untraced_not_a_panic() {
        // raea-sys-grass's blades: RENDER_WORLD only, their data taken when Bevy extracts them (RR0's film panicked on them)
        let mut grass = quad(RenderAssetUsages::RENDER_WORLD);
        let _ = grass.take_gpu_data().expect("a fresh mesh's data can be taken");
        assert!(traceable(&grass, true).is_none());
        // a mesh still in the main world is copied into Solari's layout, its u16 indices widened
        match traceable(&quad(RenderAssetUsages::default()), true) {
            Some(Traceable::Copy(m)) => assert!(matches!(m.indices(), Some(Indices::U32(v)) if v == &vec![0, 1, 2])),
            _ => panic!("a readable triangle mesh is traceable"),
        }
    }
}
