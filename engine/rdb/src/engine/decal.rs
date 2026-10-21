//! Decals (<Decal>): a picture pressed onto whatever surface its box crosses, as Bevy's forward decal draws it (a
//! 1x1 quad on its local xz, +y out of the surface, blended into the depth under it). A label on a jar, a stain on a
//! counter, a patch of tiles, a sticker on a laptop: placed as any 3D part (x, y, z, rot_*, scale_x the width,
//! scale_z the height), so it moves with the clock in a render and with play in a game.
//!
//! Columns: map (the picture), normal_map, col_r/g/b, opacity, rough, metal (keyed), unlit, fade (how
//! far off its plane, in metres, it still lies on a surface: Bevy's depth_fade_factor, 0.05 by default; Bevy's own 8 put
//! a backsplash on a fridge's side a metre off the wall).
//!
//! Each decal keeps a standard material (`DecalStd`) for the rows to animate (sync_meshes writes it, as for any mesh),
//! and its decal material follows it here. A camera gets a depth prepass while the scene has a decal.

use super::*;
use bevy::pbr::decal::{ForwardDecal, ForwardDecalMaterial, ForwardDecalMaterialExt};

/// The standard material the rows animate; the decal's own material copies it (sync_decals).
#[derive(Component)]
pub(super) struct DecalStd(pub Handle<StandardMaterial>);

pub(super) fn spawn(world: &mut World, e: Entity, t: &Tup, inserted: &[Tup], dir: &Path) -> Result<(), String> {
    let vals = &t.vals;
    if !matches!(vals.get("map"), Some(V::S(s)) if !s.is_empty()) {
        return Err(format!("decal {}: needs src (a picture)", t.id));
    }
    let mut base = scene3d::material(world, t, inserted, dir)?;
    // a picture's clear edges show the surface through it
    base.alpha_mode = AlphaMode::Blend;
    let std = world.resource_mut::<Assets<StandardMaterial>>().add(base.clone());
    let m = world.resource_mut::<Assets<ForwardDecalMaterial<StandardMaterial>>>().add(ForwardDecalMaterial {
        base,
        extension: ForwardDecalMaterialExt { depth_fade_factor: num(vals, "fade", 0.05) },
    });
    world.entity_mut(e).insert((ForwardDecal, MeshMaterial3d(m), DecalStd(std), scene3d::transform_of(vals), bevy::light::NotShadowCaster));
    Ok(())
}

/// Each decal's material follows its standard one (its colour, opacity and surface keyed), and every 3D camera has a
/// depth prepass while there's a decal to draw (the decal blends by the depth under it).
pub(super) fn sync_decals(
    mut commands: Commands,
    std: Res<Assets<StandardMaterial>>,
    mut decals: ResMut<Assets<ForwardDecalMaterial<StandardMaterial>>>,
    q: Query<(&DecalStd, &MeshMaterial3d<ForwardDecalMaterial<StandardMaterial>>)>,
    cams: Query<Entity, (With<Camera3d>, Without<bevy::core_pipeline::prepass::DepthPrepass>)>,
) {
    if q.is_empty() {
        return;
    }
    for e in &cams {
        commands.entity(e).insert(bevy::core_pipeline::prepass::DepthPrepass);
    }
    for (s, m) in &q {
        let (Some(s), Some(cur)) = (std.get(&s.0), decals.get(&m.0)) else { continue };
        let b = &cur.base;
        if b.base_color != s.base_color || b.perceptual_roughness != s.perceptual_roughness || b.metallic != s.metallic || b.emissive != s.emissive {
            if let Some(mut d) = decals.get_mut(&m.0) {
                d.base.base_color = s.base_color;
                d.base.perceptual_roughness = s.perceptual_roughness;
                d.base.metallic = s.metallic;
                d.base.emissive = s.emissive;
            }
        }
    }
}
