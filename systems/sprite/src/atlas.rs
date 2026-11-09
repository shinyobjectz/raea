//! A texture atlas packed from pictures (raea/3d's TextureAtlasBuilder, Bevy's examples/2d/texture_atlas): the src
//! "atlas:" and its JSON, { textures: [picture, ...], padding: [x, y], sampler }, built once by Bevy's own
//! TextureAtlasBuilder, each picture added in its list's order (the packer's ties go by that order, as a LoadedFolder's
//! handles go to Bevy's). A sprite showing one of its pictures is the row's `atlas_pick`, its index in that list.

use std::collections::HashMap;

use bevy::image::{TextureAtlas, TextureAtlasBuilder, TextureAtlasLayout};
use bevy::prelude::*;

const PREFIX: &str = "atlas:";

#[derive(Clone)]
struct Built {
    image: Handle<Image>,
    size: UVec2,
    layout: Handle<TextureAtlasLayout>,
    /// each listed picture's index in the layout
    picks: Vec<usize>,
}

/// Atlases built so far, by their src: sprites naming one share its picture and layout.
#[derive(Resource, Default)]
struct Atlases(HashMap<String, Built>);

/// Whether a sprite's src is an atlas.
pub fn is_atlas(src: &str) -> bool {
    src.starts_with(PREFIX)
}

/// The atlas a src names, packed now if it is new: its picture and that picture's size. `picture` decodes one listed
/// picture to straight RGBA8, as a sprite's own.
pub fn source(world: &mut World, src: &str, mut picture: impl FnMut(&str) -> Result<((u32, u32), Vec<u8>), String>) -> Result<(Handle<Image>, UVec2), String> {
    if let Some(b) = world.get_resource_or_init::<Atlases>().0.get(src) {
        return Ok((b.image.clone(), b.size));
    }
    let spec: serde_json::Value = serde_json::from_str(&src[PREFIX.len()..]).map_err(|e| format!("atlas: {e}"))?;
    let list = spec.get("textures").and_then(|t| t.as_array()).filter(|t| !t.is_empty()).ok_or("atlas: textures is a list of one or more pictures")?;
    let pad: [u32; 2] = match spec.get("padding") {
        Some(p) => serde_json::from_value(p.clone()).map_err(|e| format!("atlas: padding is [x, y]: {e}"))?,
        None => [0, 0],
    };
    let mut images = Vec::with_capacity(list.len());
    for p in list {
        let p = p.as_str().ok_or_else(|| format!("atlas: a texture is a picture's path, got {p}"))?;
        let ((w, h), rgba) = picture(p).map_err(|e| format!("atlas texture {p}: {e}"))?;
        images.push(crate::sprite_image(w, h, rgba));
    }
    let ids: Vec<AssetId<Image>> = images.iter().map(|i| world.resource_mut::<Assets<Image>>().add(i.clone()).id()).collect();
    let mut builder = TextureAtlasBuilder::default();
    builder.padding(UVec2::from_array(pad));
    for (id, image) in ids.iter().zip(&images) {
        builder.add_texture(Some(*id), image);
    }
    let (layout, sources, image) = builder.build().map_err(|e| format!("atlas: {e}"))?;
    let mut picks = Vec::with_capacity(ids.len());
    for id in &ids {
        picks.push(sources.texture_index(*id).ok_or("atlas: a texture left out of its layout")?);
    }
    let size = layout.size;
    let built = Built { image: world.resource_mut::<Assets<Image>>().add(image), size, layout: world.resource_mut::<Assets<TextureAtlasLayout>>().add(layout), picks };
    world.resource_mut::<Atlases>().0.insert(src.to_string(), built.clone());
    Ok((built.image, built.size))
}

/// The cell of an atlas a sprite shows: its `pick` (a listed picture's index), or the whole atlas without one.
pub fn pick(world: &mut World, src: &str, pick: Option<usize>) -> Result<Option<TextureAtlas>, String> {
    let (Some(i), Some(b)) = (pick, world.get_resource::<Atlases>().and_then(|a| a.0.get(src))) else { return Ok(None) };
    let index = *b.picks.get(i).ok_or_else(|| format!("atlas_pick {i}: the atlas has {} textures", b.picks.len()))?;
    Ok(Some(TextureAtlas { layout: b.layout.clone(), index }))
}
