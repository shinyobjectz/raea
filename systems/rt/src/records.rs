//! What a ray hit on an instance sees, D5's first tier (docs/realism.feature LT4): each instance's record, its averaged
//! albedo and its emission, in the TLAS's order (an instance's custom index is its place there, lib.rs build_tlas).
//! Read from its StandardMaterial: the base colour times its texture's mean, less what is metal, and the emissive
//! colour times its texture's mean as the radiance it sends (cd/m2, as the reference's tracer and Bevy's lights take
//! it: FLOOR3's lenses are 4500 nits). Not as raster draws the emitter itself: Bevy's emissive_exposure_weight of 0
//! (its default) draws it unexposed, a display value, and the light it gives would be that over the exposure, nine
//! times FLOOR3's reference (11-18, 3.2 stops bright). An instance with no StandardMaterial (a hooked or
//! custom one) keeps LT2's stand-in, albedo 0.5 and no emission, until the cache learns its surface from the screen
//! (D5's second tier).

use bevy::{
    asset::AssetId,
    image::Image,
    platform::collections::HashMap,
    prelude::*,
    render::render_resource::TextureFormat,
};

/// one instance's record, gi.wesl's Record: albedo (rgb), emission (rgb, the radiance raster's units give it)
pub type Record = [f32; 8];

/// LT2's stand-in: a flat 0.5, no emission
pub const STAND_IN: Record = [0.5, 0.5, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0];

/// The record of an instance drawn with `m`; `means` keeps each texture's mean colour.
pub fn record(m: &StandardMaterial, images: &Assets<Image>, means: &mut HashMap<AssetId<Image>, Vec3>) -> Record {
    let mut mean = |h: &Option<Handle<Image>>| h.as_ref().map_or(Vec3::ONE, |h| *means.entry(h.id()).or_insert_with(|| images.get(h).map_or(Vec3::ONE, texture_mean)));
    let base = Vec3::from_slice(&m.base_color.to_linear().to_f32_array()[..3]) * mean(&m.base_color_texture) * (1.0 - m.metallic);
    let emission = Vec3::new(m.emissive.red, m.emissive.green, m.emissive.blue) * mean(&m.emissive_texture);
    [base.x, base.y, base.z, 0.0, emission.x, emission.y, emission.z, 0.0]
}

/// An image's mean linear colour, from at most 4096 of its texels; white where its texels aren't on the CPU or aren't
/// 8-bit RGBA (a compressed texture keeps its material's colour).
fn texture_mean(image: &Image) -> Vec3 {
    let Some(data) = image.data.as_ref() else { return Vec3::ONE };
    let srgb = match image.texture_descriptor.format {
        TextureFormat::Rgba8UnormSrgb => true,
        TextureFormat::Rgba8Unorm => false,
        _ => return Vec3::ONE,
    };
    let texels = data.len() / 4;
    if texels == 0 {
        return Vec3::ONE;
    }
    let step = texels.div_ceil(4096).max(1);
    let lin = |v: u8| {
        let c = v as f32 / 255.0;
        if !srgb { c } else if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let (mut sum, mut n) = (Vec3::ZERO, 0.0);
    for t in (0..texels).step_by(step) {
        let p = &data[t * 4..t * 4 + 3];
        sum += Vec3::new(lin(p[0]), lin(p[1]), lin(p[2]));
        n += 1.0;
    }
    sum / n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_is_raster_s_colour_and_light() {
        let (images, mut means) = (Assets::<Image>::default(), HashMap::default());
        let m = StandardMaterial { base_color: Color::linear_rgb(0.8, 0.4, 0.2), metallic: 0.5, emissive: LinearRgba::rgb(4500.0, 0.0, 0.0), ..default() };
        let r = record(&m, &images, &mut means);
        assert_eq!(&r[..3], &[0.4, 0.2, 0.1]);
        // the radiance it sends, whatever its exposure weight
        assert_eq!(r[4], 4500.0);
        assert_eq!(record(&StandardMaterial { emissive_exposure_weight: 1.0, ..m }, &images, &mut means)[4], 4500.0);
    }
}
