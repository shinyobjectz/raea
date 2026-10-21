//! The paint look's surfaces (a <Look mode="paint"> or "cartoon": frame_look 'paint' on the camera row): every
//! standard material in the scene drawn as paint.wgsl paints it, its suns' light in a few flat tones (`paint_bands`):
//! lit, its own colour; turned from the sun, times `paint_mid`; in a cast shadow (cut hard), times `paint_shadow`. No
//! specular, ambient or point lights. The frame's ragged edges, grain, sky and palette are the look's composite
//! (look.rs, mode 6).
//!
//! Each mesh keeps its standard material (`Painted::std`) for the rows to animate (sync_meshes writes it, as for any
//! mesh), and its paint copy follows it here. Models with a look of their own (`look`, `flat`) keep theirs.
//!
//! Columns on the camera row: paint_bands (keyed), paint_shadow and paint_mid (hex, the first frame's).

use super::*;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub(super) type PaintMaterial = ExtendedMaterial<StandardMaterial, Paint>;

pub(super) fn install(app: &mut App) {
    bevy::asset::embedded_asset!(app, "paint.wgsl");
    app.add_plugins(MaterialPlugin::<PaintMaterial>::default());
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub(super) struct Paint {
    #[uniform(100)]
    p: PaintParams,
}

#[derive(ShaderType, Clone, Copy, Default, PartialEq)]
pub(super) struct PaintParams {
    shadow: Vec4,
    mid: Vec4,
    bands: f32,
    _a: f32,
    _b: f32,
    _c: f32,
}

impl MaterialExtension for Paint {
    fn fragment_shader() -> ShaderRef {
        "embedded://rdb/engine/paint.wgsl".into()
    }
}

/// A painted mesh: the standard material the rows animate, and the paint copy drawn.
#[derive(Component)]
pub(super) struct Painted {
    pub(super) std: Handle<StandardMaterial>,
    paint: Handle<PaintMaterial>,
}

fn params(v: &BTreeMap<String, V>) -> PaintParams {
    PaintParams {
        shadow: Vec4::from_array(colour(v.get("paint_shadow"), [0.55, 0.58, 0.68, 1.0]).to_linear().to_f32_array()),
        mid: Vec4::from_array(colour(v.get("paint_mid"), [0.80, 0.76, 0.78, 1.0]).to_linear().to_f32_array()),
        bands: num(v, "paint_bands", 3.0).clamp(1.0, 8.0),
        ..default()
    }
}

/// Once the camera's row asks for paint: each standard-material mesh (but a looked or flat model's) swapped onto a
/// paint copy, as meshes appear; then each frame the copies follow their standard materials and the row's bands.
#[allow(clippy::type_complexity)]
pub(super) fn sync_paint(
    mut commands: Commands,
    frame: Res<FrameRows>,
    cams: Query<Entity, (With<Camera3d>, Without<look::LookCam>)>,
    fresh: Query<(Entity, &MeshMaterial3d<StandardMaterial>), (With<Mesh3d>, Without<Painted>)>,
    painted: Query<&Painted>,
    models: Query<Entity, With<scene3d::Model3d>>,
    kids: Query<&Children>,
    stds: Res<Assets<StandardMaterial>>,
    mut paints: ResMut<Assets<PaintMaterial>>,
    mut last: Local<Option<PaintParams>>,
) {
    let Ok(cam) = cams.single() else { return };
    let Some(v) = frame.vals(cam) else { return };
    if !matches!(v.get("frame_look"), Some(V::S(s)) if s == "paint") {
        return;
    }
    let p = params(&v);
    // a model with its own look or flat surfaces keeps them
    let mut keep: Vec<Entity> = Vec::new();
    for m in &models {
        if let Some(mv) = frame.vals(m) {
            if mv.contains_key("look") || mv.get("flat").is_some_and(V::truthy) {
                keep.extend(scene3d::descendants(m, &kids));
            }
        }
    }
    for (e, s) in &fresh {
        if keep.contains(&e) {
            continue;
        }
        let Some(base) = stds.get(&s.0).cloned() else { continue };
        let paint = paints.add(PaintMaterial { base, extension: Paint { p } });
        commands.entity(e).remove::<MeshMaterial3d<StandardMaterial>>().insert((MeshMaterial3d(paint.clone()), Painted { std: s.0.clone(), paint }));
    }
    let bands_changed = *last != Some(p);
    *last = Some(p);
    for pm in &painted {
        let Some(std) = stds.get(&pm.std) else { continue };
        let Some(cur) = paints.get(&pm.paint) else { continue };
        let moved = cur.base.base_color != std.base_color || cur.base.emissive != std.emissive || cur.base.alpha_mode != std.alpha_mode;
        if !moved && !bands_changed {
            continue;
        }
        let std = std.clone();
        if let Some(mut m) = paints.get_mut(&pm.paint) {
            if moved {
                m.base = std;
            }
            m.extension.p = p;
        }
    }
}
