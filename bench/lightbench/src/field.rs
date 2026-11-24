//! LB2's scene (--scene field): knots receding from the camera, each drawing the clusters of one shared hierarchy
//! that its distance and the view ask for (lod.rs), chosen on the CPU each frame and rebuilt into its mesh when the
//! choice changes. Instances outside the view are culled. --detail full draws every finest triangle instead, and
//! --view lod colours each cluster by its level.

use crate::{lod, scene, Opts};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use std::sync::Arc;

/// The hierarchy every knot shares, in the knot's own space.
#[derive(Resource)]
pub struct Field {
    pos: Vec<Vec3>,
    nor: Vec<Vec3>,
    dag: Arc<Vec<lod::Cluster>>,
    /// the whole knot's bounds
    center: Vec3,
    radius: f32,
    finest: usize,
}

/// A knot of the field: its mesh, and the clusters it drew last.
#[derive(Component)]
pub struct Knot {
    mesh: Handle<Mesh>,
    drawn: Vec<usize>,
}

/// The knots' colour.
pub const COLOUR: Color = Color::srgb(0.85, 0.42, 0.18);

const LEVEL_COLOURS: [[f32; 3]; 8] = [[1.0, 1.0, 1.0], [0.3, 0.9, 0.3], [0.95, 0.9, 0.2], [1.0, 0.55, 0.1], [0.95, 0.2, 0.2], [0.85, 0.3, 0.9], [0.3, 0.5, 1.0], [0.2, 0.9, 0.9]];

/// The knots' places: rows from 3 m to about 60 m away, each a little different in size and turn.
pub fn places(rows: u32) -> Vec<Transform> {
    let mut out = Vec::new();
    for r in 0..rows {
        let z = 1.0 - (r as f32).powf(1.6) * 2.2;
        let across = 3 + r / 2;
        for c in 0..across {
            let x = (c as f32 - (across - 1) as f32 * 0.5) * (2.6 + r as f32 * 0.35);
            let k = (r * 31 + c * 17) as f32;
            let s = 0.3 + 0.25 * (0.5 + 0.5 * (k * 1.7).sin());
            out.push(Transform::from_xyz(x, s * 3.4, z).with_rotation(Quat::from_rotation_y(k)).with_scale(Vec3::splat(s)));
        }
    }
    out
}

/// The field's knots: their meshes start as the full knot and the BVH holds every finest triangle of every knot.
pub fn spawn(commands: &mut Commands, opts: &Opts, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, tris: &mut Vec<[Vec3; 3]>) {
    let (pos, nor, idx) = scene::knot_data(2, 3, opts.detail, opts.detail / 32, 0.33);
    let t0 = std::time::Instant::now();
    let dag = lod::dag(&pos, &idx);
    let levels = dag.iter().map(|c| c.level).max().unwrap_or(0) + 1;
    info!("lightbench: the knot's hierarchy: {} clusters in {levels} levels from {} triangles, built in {:.0} ms", dag.len(), idx.len() / 3, t0.elapsed().as_secs_f64() * 1000.0);
    let (mut lo, mut hi) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
    pos.iter().for_each(|p| {
        lo = lo.min(*p);
        hi = hi.max(*p);
    });
    let center = (lo + hi) * 0.5;
    let radius = pos.iter().map(|p| p.distance(center)).fold(0.0, f32::max);
    let material = mats.add(StandardMaterial { base_color: COLOUR, perceptual_roughness: 0.6, ..default() });
    let lod_material = mats.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.6, ..default() });
    for at in places(opts.rows) {
        let m = at.to_matrix();
        tris.extend(idx.chunks_exact(3).map(|t| [0, 1, 2].map(|k| m.transform_point3(pos[t[k] as usize]))));
        let mesh = meshes.add(build(&pos, &nor, &dag, &(0..dag.len()).filter(|&c| dag[c].level == 0).collect::<Vec<_>>(), opts.view == 6));
        let mat = if opts.view == 6 { lod_material.clone() } else { material.clone() };
        commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), at, Knot { mesh, drawn: Vec::new() }));
    }
    commands.insert_resource(Field { pos, nor, dag: Arc::new(dag), center, radius, finest: idx.len() / 3 });
}

/// A knot's mesh from clusters: indexed into the shared vertices, or (coloured by level) a corner each.
fn build(pos: &[Vec3], nor: &[Vec3], dag: &[lod::Cluster], drawn: &[usize], coloured: bool) -> Mesh {
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    if !coloured {
        let idx = lod::indices_of(dag, drawn);
        return mesh.with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos.to_vec()).with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nor.to_vec()).with_inserted_indices(Indices::U32(idx));
    }
    let (mut p, mut n, mut c) = (Vec::new(), Vec::new(), Vec::new());
    for &k in drawn {
        let [r, g, b] = LEVEL_COLOURS[dag[k].level as usize % LEVEL_COLOURS.len()];
        // each cluster a little lighter or darker, so cluster edges show within a level
        let shade = 0.75 + 0.25 * ((k as f32 * 12.9898).sin() * 43758.5).fract().abs();
        for &i in &dag[k].indices {
            p.push(pos[i as usize]);
            n.push(nor[i as usize]);
            c.push([r * shade, g * shade, b * shade, 1.0]);
        }
    }
    mesh.with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p).with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, n).with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, c)
}

/// Triangles drawn this frame, and of how many, for the shot's line.
#[derive(Resource, Default, Clone, Copy)]
pub struct Counts {
    pub knots: usize,
    pub culled: usize,
    pub drawn: usize,
    pub full: usize,
}

/// Each knot's clusters for this frame's camera: culled when its bounds are outside the view, all the finest with
/// --detail full, else the hierarchy's choice at --tau pixels.
pub fn choose(
    field: Option<Res<Field>>,
    opts: Res<Opts>,
    cam: Query<(&GlobalTransform, &Projection), With<Camera3d>>,
    mut knots: Query<(&GlobalTransform, &mut Knot, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut counts: ResMut<Counts>,
) {
    let (Some(f), Ok((eye, proj))) = (field, cam.single()) else { return };
    let Projection::Perspective(pp) = proj else { return };
    let focal = lod::focal_px(pp.fov, opts.size.y as f32);
    let (half_y, half_x) = ((pp.fov * 0.5).tan(), (pp.fov * 0.5).tan() * pp.aspect_ratio);
    let view_from_world = eye.to_matrix().inverse();
    let at = eye.translation();
    let mut c = Counts::default();
    for (xf, mut knot, mut vis) in &mut knots {
        let (scale, _, _) = xf.to_scale_rotation_translation();
        let (s, center) = (scale.x, xf.transform_point(f.center));
        let r = f.radius * s;
        // outside the view's four sides or behind it
        let v = view_from_world.transform_point3(center);
        let depth = -v.z;
        let outside = depth < -r || v.x.abs() - half_x * depth > r * (1.0 + half_x * half_x).sqrt() || v.y.abs() - half_y * depth > r * (1.0 + half_y * half_y).sqrt();
        c.knots += 1;
        c.full += f.finest;
        if outside {
            c.culled += 1;
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        let drawn: Vec<usize> = if opts.full {
            f.dag.iter().enumerate().filter(|(_, c)| c.level == 0).map(|(k, _)| k).collect()
        } else {
            let to_obj = xf.to_matrix().inverse();
            let eye_obj = to_obj.transform_point3(at);
            // in the knot's own space distances shrink by its scale, so the error's pixels come out the same
            lod::select(&f.dag, 1.0, eye_obj, focal, opts.tau)
        };
        c.drawn += drawn.iter().map(|&k| f.dag[k].indices.len() / 3).sum::<usize>();
        if drawn != knot.drawn {
            if let Some(mut m) = meshes.get_mut(&knot.mesh) {
                *m = build(&f.pos, &f.nor, &f.dag, &drawn, opts.view == 6);
            }
            knot.drawn = drawn;
        }
    }
    *counts = c;
}
