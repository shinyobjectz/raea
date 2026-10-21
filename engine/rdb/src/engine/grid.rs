//! A greybox surface: the standard material with graph paper over it. Lines every `grid_cell` metres in world space
//! (so they run on across boxes and slopes), a heavier line every `grid_major` cells, drawn on whichever pair of axes
//! the face looks along, and an outline round each face of a box (from its texture coordinates, which scene3d's
//! box_uvs spread 0..1 over every face), all a fixed width in pixels. The lines are drawn into the surface's colour
//! before the light, so shadows fall across them.
//!
//! Columns on a mesh row (all optional but grid_cell): grid_cell (metres; the grid is on when > 0), grid_major (cells
//! per heavy line, 4), grid_color (the lines, '#c8c8c3'), grid_edge (the outlines, '#3a3a38'; '' for none), grid_toon
//! (1: the light in three flat bands, toon shading).
//!
//! On a model row the same columns draw the grid in the model's own texture coordinates, taken as metres (a moving
//! body's lines stay on its surface; AnnySteps' exporter writes each face's rest-pose position there), without
//! outlines: grid_models swaps the glTF's standard materials for grid ones once it's loaded.

use super::*;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub(super) type GridMaterial = ExtendedMaterial<StandardMaterial, Grid>;

pub(super) fn install(app: &mut App) {
    bevy::asset::embedded_asset!(app, "grid.wgsl");
    app.add_plugins(MaterialPlugin::<GridMaterial>::default());
}

/// A model row with a grid: each of its loaded meshes' standard materials becomes a grid one over the same surface,
/// once (a swapped mesh has no standard material left to find). In the engine's chain after sync_models, once the
/// frame's rows are in.
pub(super) fn grid_models(
    mut commands: Commands,
    frame: Res<FrameRows>,
    models: Query<Entity, With<scene3d::Model3d>>,
    kids: Query<&Children>,
    std: Query<&MeshMaterial3d<StandardMaterial>>,
    // a see-through mesh drawn with its froxel fog copy (froxel_glass.rs) keeps its standard material here
    fogged: Query<&super::froxel_glass::Fogged>,
    stds: Res<Assets<StandardMaterial>>,
    mut grids: ResMut<Assets<GridMaterial>>,
) {
    for e in &models {
        let Some(vals) = frame.vals(e) else { continue };
        let Some(mut grid) = grid_of(&vals) else { continue };
        grid.p.mode = 1.0;
        grid.p.edge.w = 0.0;
        for d in scene3d::descendants(e, &kids) {
            let Some(h) = std.get(d).map(|m| &m.0).ok().or_else(|| fogged.get(d).ok().map(|f| &f.std)) else { continue };
            let Some(base) = stds.get(h).cloned() else { continue };
            let mat = grids.add(GridMaterial { base, extension: grid.clone() });
            commands
                .entity(d)
                .remove::<(MeshMaterial3d<StandardMaterial>, super::froxel_glass::Fogged, MeshMaterial3d<super::froxel_glass::GlassMaterial>)>()
                .insert(MeshMaterial3d(mat));
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub(super) struct Grid {
    #[uniform(100)]
    p: GridParams,
}

#[derive(ShaderType, Clone, Copy, Default, PartialEq)]
struct GridParams {
    line: Vec4,
    edge: Vec4,
    cell: f32,
    major: f32,
    line_px: f32,
    edge_px: f32,
    /// 0: world space, with outlines (a mesh); 1: the texture coordinates as metres (a model)
    mode: f32,
    /// toon shading: the light in three flat bands (1), or as the standard material lights it (0)
    toon: f32,
}

impl MaterialExtension for Grid {
    fn fragment_shader() -> ShaderRef {
        "embedded://rdb/engine/grid.wgsl".into()
    }
}

/// The grid a mesh row asks for, or None (no grid_cell, or not above 0)
pub(super) fn grid_of(vals: &BTreeMap<String, V>) -> Option<Grid> {
    let cell = num(vals, "grid_cell", 0.0);
    if cell <= 0.0 {
        return None;
    }
    let rgba = |c: Color, on: bool| {
        let l = c.to_linear();
        Vec4::new(l.red, l.green, l.blue, if on { 1.0 } else { 0.0 })
    };
    let edge_on = !matches!(vals.get("grid_edge"), Some(V::S(s)) if s.is_empty());
    Some(Grid {
        p: GridParams {
            line: rgba(colour(vals.get("grid_color"), [0.784, 0.784, 0.765, 1.0]), true),
            edge: rgba(colour(vals.get("grid_edge"), [0.227, 0.227, 0.22, 1.0]), edge_on),
            cell,
            major: num(vals, "grid_major", 4.0).max(1.0),
            line_px: 1.0,
            edge_px: 1.6,
            mode: 0.0,
            toon: if num(vals, "grid_toon", 0.0) > 0.0 { 1.0 } else { 0.0 },
        },
    })
}
