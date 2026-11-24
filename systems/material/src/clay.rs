//! Clay: every standard surface drawn light grey and matte, its maps, glow and metal taken off, so a review judges
//! form and assembly without surfaces (`raea render --clay`, and the clay half of `--review`; the 11-06 research: every
//! strong scene reviewer judges on clay beside colour). On when `CLAY` is set or the process has RAEA_CLAY; a
//! translucent surface keeps its alpha, so glass stays see-through. Toon and custom shader surfaces draw as they are.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;

/// Set by `raea render --clay` in this process; RAEA_CLAY does the same for a child process.
pub static CLAY: AtomicBool = AtomicBool::new(false);

const GREY: [f32; 3] = [0.62, 0.62, 0.6];

pub fn on() -> bool {
    static ENV: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    CLAY.load(Ordering::Relaxed) || *ENV.get_or_init(|| raea_io_trace::stat("RAEA_CLAY"))
}

fn is_clay(m: &StandardMaterial) -> bool {
    let c = m.base_color.to_srgba();
    m.base_color_texture.is_none() && m.emissive_texture.is_none() && m.normal_map_texture.is_none() && m.metallic_roughness_texture.is_none() && (c.red - GREY[0]).abs() < 1e-4 && (c.green - GREY[1]).abs() < 1e-4 && (c.blue - GREY[2]).abs() < 1e-4 && m.metallic == 0.0 && m.emissive == LinearRgba::BLACK
}

/// Each standard material not yet clay made clay (run after the rows set colours, each frame; one already clay is
/// left untouched, so nothing is re-uploaded).
pub fn clay_materials(mut mats: ResMut<Assets<StandardMaterial>>) {
    if !on() {
        return;
    }
    let ids: Vec<AssetId<StandardMaterial>> = mats.iter().filter(|(_, m)| !is_clay(m)).map(|(id, _)| id).collect();
    for id in ids {
        if let Some(mut m) = mats.get_mut(id) {
            let alpha = m.base_color.alpha();
            m.base_color = Color::srgba(GREY[0], GREY[1], GREY[2], alpha);
            m.base_color_texture = None;
            m.emissive = LinearRgba::BLACK;
            m.emissive_texture = None;
            m.normal_map_texture = None;
            m.metallic_roughness_texture = None;
            m.metallic = 0.0;
            m.perceptual_roughness = 0.7;
            m.reflectance = 0.5;
            m.clearcoat = 0.0;
            m.specular_transmission = 0.0;
            m.diffuse_transmission = 0.0;
            m.unlit = false;
        }
    }
}
