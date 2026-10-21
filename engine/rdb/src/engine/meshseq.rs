//! Baked simulations played back by the frame, and glitches on any mesh (sdk/motion/src/physics.tsx).
//!
//! `meshseq` rows (`<MeshSequence>`, `<Goo>`): a `.rseq` file (crate::meshseq) drawn as one mesh whose triangles are
//! the sequence's frame `frame` (keyed per frame), with the standard material's columns (scene3d::material: color,
//! rough, metal, clearcoat, clearcoat_rough, map, emissive, opacity...) and `subsurface` (diffuse transmission, 0..1)
//! with `thickness`. Placed, hidden and recoloured by sync_meshes as any mesh is. The frame read is a function of the
//! row at the frame, so `render --at` any frame draws what a straight render draws there.
//!
//! Glitch columns on `model` and `mesh` rows (`<Body glitch>`, `<Model glitch>`), every one keyed per frame:
//! glitch_explode (the triangles thrown out from the centre, turning; 0 whole), glitch_scramble (0..1: that share of
//! the vertices pulled to other vertices of the part: spikes across it), glitch_jitter (metres of per-triangle noise),
//! glitch_snap (metres: vertices snapped to a grid that size), glitch_smear_x/y/z (metres, world: the trailing side
//! dragged back along it), glitch_seed (which randomness). They deform the part's own triangles (its materials and
//! textures as they are) from its rest shape each frame: the same columns give the same triangles in any order of
//! frames. With all of them 0 the part draws its own mesh again, untouched.

use super::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::render::render_resource::PrimitiveTopology;
use std::sync::Arc;

pub(super) fn install(app: &mut App) {
    app.add_systems(PostUpdate, (sync_meshseqs, sync_glitches).after(bevy::transform::TransformSystems::Propagate));
}

#[derive(Component)]
pub(super) struct MeshSeqPlay {
    seq: Arc<crate::meshseq::Seq>,
    mesh: Handle<Mesh>,
    last: Option<usize>,
}

/// A mesh from a sequence's frame (a degenerate triangle for an empty one: an empty buffer can't be drawn).
fn mesh_of(fr: &crate::meshseq::Frame) -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    if fr.idx.is_empty() {
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0f32; 3]; 3]);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0f32, 1.0, 0.0]; 3]);
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0f32, 0.0, 0.0, 0.0]; 3]);
        m.insert_indices(Indices::U32(vec![0, 1, 2]));
        return m;
    }
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, fr.pos.clone());
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, fr.nrm.clone());
    if !fr.col.is_empty() {
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, fr.col.clone());
    }
    if !fr.uv.is_empty() {
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, fr.uv.clone());
    }
    m.insert_indices(Indices::U32(fr.idx.clone()));
    m
}

/// A `meshseq` row's entity: the sequence opened, its frame's mesh, its surface.
pub(super) fn spawn(world: &mut World, e: Entity, t: &Tup, inserted: &[Tup], dir: &Path) -> Result<(), String> {
    let vals = &t.vals;
    let src = match vals.get("src") { Some(V::S(s)) => s.clone(), _ => return Err(format!("meshseq {}: needs src (a .rseq file)", t.id)) };
    let path = resolve_src(inserted, &src, dir)?;
    let seq = Arc::new(crate::meshseq::Seq::open(&path).map_err(|e| format!("meshseq {}: {e}", t.id))?);
    let k = num(vals, "frame", 0.0).round().max(0.0) as usize;
    let fr = seq.read(k).map_err(|e| format!("meshseq {}: {e}", t.id))?;
    let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh_of(&fr));
    let mut mat = scene3d::material(world, t, inserted, dir)?;
    // goo: light through a little of it, and a wet coat over it
    mat.diffuse_transmission = num(vals, "subsurface", 0.0).clamp(0.0, 1.0);
    mat.thickness = num(vals, "thickness", 0.02);
    if mat.diffuse_transmission > 0.0 {
        mat.attenuation_color = colour(vals.get("color"), [1.0, 1.0, 1.0, 1.0]);
        mat.attenuation_distance = num(vals, "thickness", 0.02).max(1e-3) * 4.0;
    }
    // a clear liquid: light through it, bent (Bevy's screen-space transmission: what's on screen behind it, one layer)
    // and tinted with depth (its colour at attenuation distance); thickness is how deep the light goes through
    let tr = num(vals, "transmission", 0.0).clamp(0.0, 1.0);
    if tr > 0.0 {
        mat.specular_transmission = tr;
        mat.ior = num(vals, "ior", 1.33);
        mat.thickness = num(vals, "thickness", 0.02);
        if vals.contains_key("atten_dist") {
            mat.attenuation_distance = num(vals, "atten_dist", 1.0).max(1e-4);
            mat.attenuation_color = colour(vals.get("atten_color"), [1.0, 1.0, 1.0, 1.0]);
        }
    }
    if flag(vals, "double_sided") {
        mat.double_sided = true;
        mat.cull_mode = None;
    }
    let mat = world.resource_mut::<Assets<StandardMaterial>>().add(mat);
    world.entity_mut(e).insert((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(mat),
        scene3d::transform_of(vals),
        Visibility::default(),
        // its bounds change with every frame: never culled by the first frame's
        bevy::camera::visibility::NoFrustumCulling,
        MeshSeqPlay { seq, mesh, last: Some(k) },
    ));
    Ok(())
}

fn flag(vals: &BTreeMap<String, V>, k: &str) -> bool {
    vals.get(k).is_some_and(V::truthy)
}

/// Each sequence's frame for this frame's row.
fn sync_meshseqs(frame: Res<FrameRows>, mut meshes: ResMut<Assets<Mesh>>, mut q: Query<(Entity, &mut MeshSeqPlay)>, mut err: ResMut<Error>) {
    for (e, mut sp) in &mut q {
        let Some(v) = frame.vals(e) else { continue };
        let k = num(&v, "frame", 0.0).round().max(0.0) as usize;
        if sp.last == Some(k) {
            continue;
        }
        match sp.seq.read(k) {
            Ok(fr) => {
                if let Some(mut m) = meshes.get_mut(&sp.mesh) {
                    *m = mesh_of(&fr);
                }
                sp.last = Some(k);
            }
            Err(e) => err.0 = Some(format!("meshseq: {e}")),
        }
    }
}

// ---- glitches -------------------------------------------------------------------------------------------------

const GLITCH_COLS: [&str; 8] = ["glitch_explode", "glitch_scramble", "glitch_jitter", "glitch_snap", "glitch_smear_x", "glitch_smear_y", "glitch_smear_z", "glitch_seed"];

/// A glitched part: each of its meshes' own handle, its unwelded copy and the copy's rest positions and normals in
/// the part's space (the row's entity), and the last values drawn.
#[derive(Component)]
pub(super) struct Glitched {
    parts: Vec<GlitchPart>,
    centre: Vec3,
    size: f32,
    last: [f32; 8],
}

struct GlitchPart {
    entity: Entity,
    own: Handle<Mesh>,
    copy: Handle<Mesh>,
    /// mesh space -> part space, and back
    to_part: Mat4,
    from_part: Mat4,
    pos: Vec<Vec3>,
    nrm: Vec<Vec3>,
}

fn params(v: &BTreeMap<String, V>) -> [f32; 8] {
    std::array::from_fn(|i| num(v, GLITCH_COLS[i], 0.0))
}

fn h32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

/// 0..1 from a few integers
fn rnd(a: u32, b: u32, c: u32) -> f32 {
    (h32(a ^ h32(b.wrapping_add(0x9e37_79b9) ^ h32(c.wrapping_mul(0x85eb_ca6b)))) >> 8) as f32 / (1u32 << 24) as f32
}

/// A position's identity, so corners that were one vertex before unwelding move as one
fn pkey(p: Vec3) -> u32 {
    let q = (p * 1e4).round();
    h32(q.x as i32 as u32 ^ h32(q.y as i32 as u32 ^ h32(q.z as i32 as u32)))
}

fn unit3(a: u32, b: u32) -> Vec3 {
    let z = rnd(a, b, 1) * 2.0 - 1.0;
    let t = rnd(a, b, 2) * std::f32::consts::TAU;
    let r = (1.0 - z * z).max(0.0).sqrt();
    Vec3::new(r * t.cos(), z, r * t.sin())
}

/// The part's rest triangles deformed by the frame's glitch values, in part space: positions and normals per corner.
fn deform(g: &Glitched, part: &GlitchPart, p: &[f32; 8], all: &[Vec3]) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    let [explode, scramble, jitter, snap, sx, sy, sz, seed] = *p;
    let seed = seed.round() as i64 as u32;
    let smear = Vec3::new(sx, sy, sz);
    let n = part.pos.len();
    let mut pos: Vec<Vec3> = part.pos.iter().map(|q| part.to_part.transform_point3(*q)).collect();
    let mut nrm: Vec<Vec3> = part.nrm.iter().map(|q| part.to_part.transform_vector3(*q).normalize_or_zero()).collect();
    let flat = scramble > 0.0 || snap > 0.0;
    // scramble: a share of the vertices pulled to other vertices, the same pull for corners that were one vertex
    if scramble > 0.0 && !all.is_empty() {
        for q in pos.iter_mut() {
            let k = pkey(*q);
            let u = rnd(k, seed, 7);
            if u < scramble {
                let j = (h32(k ^ h32(seed.wrapping_add(31))) as usize) % all.len();
                let w = ((scramble - u) / 0.12).min(1.0);
                *q = q.lerp(all[j], w);
            }
        }
    }
    // smear: the trailing side dragged back along the motion
    let sl = smear.length();
    if sl > 1e-6 {
        let d = smear / sl;
        for q in pos.iter_mut() {
            let along = (*q - g.centre).dot(d) / g.size.max(1e-6);
            let w = (0.5 - 0.5 * along).clamp(0.0, 1.0).powf(1.5);
            *q -= smear * w;
        }
    }
    // snap: to a grid about the centre
    if snap > 0.0 {
        for q in pos.iter_mut() {
            *q = g.centre + ((*q - g.centre) / snap).round() * snap;
        }
    }
    // explode and jitter: each triangle thrown and turned about its centroid
    if explode > 0.0 || jitter > 0.0 {
        for t in 0..n / 3 {
            let (a, b, c) = (pos[t * 3], pos[t * 3 + 1], pos[t * 3 + 2]);
            let cen = (a + b + c) / 3.0;
            let id = pkey(cen) ^ (t as u32).wrapping_mul(0x27d4_eb2d);
            let out = (cen - g.centre).normalize_or(Vec3::Y);
            let dir = (out * 0.7 + unit3(id, seed) * 0.5).normalize_or(Vec3::Y);
            let mut off = dir * explode * g.size * (0.35 + 0.9 * rnd(id, seed, 3));
            off += unit3(id, seed.wrapping_add(5)) * jitter * (0.3 + rnd(id, seed, 4));
            let rot = Quat::from_axis_angle(unit3(id, seed.wrapping_add(9)), explode * std::f32::consts::PI * (0.5 + 1.5 * rnd(id, seed, 6)));
            for k in 0..3 {
                let i = t * 3 + k;
                pos[i] = cen + off + rot * (pos[i] - cen);
                nrm[i] = rot * nrm[i];
            }
        }
    }
    // a deformed part's facets catch the light as facets
    if flat {
        for t in 0..n / 3 {
            let f = (pos[t * 3 + 1] - pos[t * 3]).cross(pos[t * 3 + 2] - pos[t * 3]).normalize_or_zero();
            if f != Vec3::ZERO {
                for k in 0..3 {
                    nrm[t * 3 + k] = f;
                }
            }
        }
    }
    let back = |v: Vec3| part.from_part.transform_point3(v).to_array();
    let backn = |v: Vec3| part.from_part.transform_vector3(v).normalize_or_zero().to_array();
    (pos.into_iter().map(back).collect(), nrm.into_iter().map(backn).collect())
}

/// Glitched parts: made ready the first frame a row asks for a glitch (its meshes copied, unwelded), then deformed
/// from rest each frame its values change; back on their own meshes when every value is 0.
#[allow(clippy::too_many_arguments)]
fn sync_glitches(
    mut commands: Commands,
    frame: Res<FrameRows>,
    mut meshes: ResMut<Assets<Mesh>>,
    rows: Query<(Entity, &GlobalTransform), Or<(With<Mesh3d>, With<scene3d::Model3d>)>>,
    kids: Query<&Children>,
    mut mesh3d: Query<(&mut Mesh3d, &GlobalTransform)>,
    mut glitched: Query<&mut Glitched>,
) {
    for (e, root_gt) in &rows {
        let Some(v) = frame.vals(e) else { continue };
        if !GLITCH_COLS.iter().any(|k| v.contains_key(*k)) {
            continue;
        }
        let p = params(&v);
        let active = p[..7].iter().any(|x| *x != 0.0);
        let mut fresh = None;
        let mut held = glitched.get_mut(e).ok();
        if held.is_none() {
            if !active {
                continue;
            }
            // ready the part: its meshes (its own, or a model's), copied and unwelded
            let mut ents = vec![e];
            ents.extend(scene3d::descendants(e, &kids));
            let inv = root_gt.to_matrix().inverse();
            let mut parts = Vec::new();
            for x in ents {
                let Ok((m3, gt)) = mesh3d.get(x) else { continue };
                let Some(src) = meshes.get(&m3.0) else { continue };
                if src.primitive_topology() != PrimitiveTopology::TriangleList {
                    continue;
                }
                let mut copy = src.clone().with_duplicated_vertices();
                if copy.attribute(Mesh::ATTRIBUTE_NORMAL).is_none() {
                    copy.compute_flat_normals();
                }
                let pos: Vec<Vec3> = match copy.attribute(Mesh::ATTRIBUTE_POSITION) { Some(VertexAttributeValues::Float32x3(v)) => v.iter().map(|q| Vec3::from_array(*q)).collect(), _ => continue };
                let nrm: Vec<Vec3> = match copy.attribute(Mesh::ATTRIBUTE_NORMAL) { Some(VertexAttributeValues::Float32x3(v)) => v.iter().map(|q| Vec3::from_array(*q)).collect(), _ => vec![Vec3::Y; pos.len()] };
                let to_part = inv * gt.to_matrix();
                let own = m3.0.clone();
                let copy = meshes.add(copy);
                parts.push(GlitchPart { entity: x, own, copy, to_part, from_part: to_part.inverse(), pos, nrm });
            }
            if parts.is_empty() {
                // a model not spawned yet: next frame
                continue;
            }
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for part in &parts {
                for q in &part.pos {
                    let w = part.to_part.transform_point3(*q);
                    lo = lo.min(w);
                    hi = hi.max(w);
                }
            }
            // deformed now, in this update (a single frame rendered on its own draws its glitch)
            fresh = Some(Glitched { parts, centre: (lo + hi) / 2.0, size: ((hi - lo).length() / 2.0).max(1e-4), last: [f32::NAN; 8] });
        }
        let g: &mut Glitched = match (&mut held, &mut fresh) {
            (Some(h), _) => &mut *h,
            (None, Some(f)) => f,
            _ => continue,
        };
        if g.last == p {
            continue;
        }
        g.last = p;
        if !active {
            for part in &g.parts {
                if let Ok((mut m3, _)) = mesh3d.get_mut(part.entity) {
                    if m3.0 != part.own {
                        m3.0 = part.own.clone();
                    }
                }
            }
            continue;
        }
        // the smear is given in the world; the part's space turns it
        let mut pp = p;
        let s = root_gt.to_matrix().inverse().transform_vector3(Vec3::new(p[4], p[5], p[6]));
        pp[4] = s.x;
        pp[5] = s.y;
        pp[6] = s.z;
        let all: Vec<Vec3> = g.parts.iter().flat_map(|part| part.pos.iter().step_by(7).map(|q| part.to_part.transform_point3(*q))).collect();
        for part in &g.parts {
            let (pos, nrm) = deform(g, part, &pp, &all);
            if let Some(mut m) = meshes.get_mut(&part.copy) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
                m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
            }
            if let Ok((mut m3, _)) = mesh3d.get_mut(part.entity) {
                if m3.0 != part.copy {
                    m3.0 = part.copy.clone();
                }
            }
            commands.entity(part.entity).insert(bevy::camera::visibility::NoFrustumCulling);
        }
        if let Some(f) = fresh {
            commands.entity(e).insert(f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part() -> (Glitched, GlitchPart) {
        let pos = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 1.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)];
        let p = GlitchPart { entity: Entity::PLACEHOLDER, own: Handle::default(), copy: Handle::default(), to_part: Mat4::IDENTITY, from_part: Mat4::IDENTITY, nrm: vec![Vec3::Z; 6], pos };
        (Glitched { parts: vec![], centre: Vec3::new(0.5, 0.5, 0.0), size: 0.7, last: [0.0; 8] }, p)
    }

    #[test]
    fn glitches_are_a_function_of_their_values() {
        let (g, p) = part();
        let all = p.pos.clone();
        let v = [0.6, 0.4, 0.01, 0.0, 0.1, 0.0, 0.0, 3.0];
        let a = deform(&g, &p, &v, &all);
        let b = deform(&g, &p, &v, &all);
        assert_eq!(a, b);
        assert_ne!(a.0, deform(&g, &p, &[0.6, 0.4, 0.01, 0.0, 0.1, 0.0, 0.0, 4.0], &all).0, "another seed, other triangles");
        // nothing asked: the rest shape
        let rest = deform(&g, &p, &[0.0; 8], &all);
        assert_eq!(rest.0, p.pos.iter().map(|q| q.to_array()).collect::<Vec<_>>());
    }

    #[test]
    fn a_scrambled_vertex_moves_with_its_twin() {
        let (g, p) = part();
        let all = p.pos.clone();
        let (pos, _) = deform(&g, &p, &[0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0], &all);
        // corners 1 and 4 were one vertex, as were 2 and 5
        assert_eq!(pos[1], pos[4]);
        assert_eq!(pos[2], pos[5]);
    }

    #[test]
    fn snap_puts_vertices_on_the_grid() {
        let (g, p) = part();
        let (pos, _) = deform(&g, &p, &[0.0, 0.0, 0.0, 0.25, 0.0, 0.0, 0.0, 0.0], &p.pos.clone());
        for q in pos {
            for a in 0..2 {
                let k = (q[a] - 0.5) / 0.25;
                assert!((k - k.round()).abs() < 1e-5);
            }
        }
    }
}
