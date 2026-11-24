//! raea: Solari's denoiser guides for a denoiser other than DLSS-RR (MetalFX's, on a Mac).
//!
//! A denoiser smooths each pixel's lighting against guides: the surface's albedo, normal, roughness, depth and motion.
//! Drawn from the G-buffer, a mirror's guides are the glass's own (flat, one colour, one normal), so the denoiser takes
//! everything the mirror shows for noise and smears it (photo 4's arched mirror: its dunes, ripples and rock a blur).
//! Solari already works out better guides for DLSS-RR, primary surface replacement: a mirror's pixel takes the
//! albedo, normal, roughness, depth and motion of what it reflects, as if the camera saw that directly. They were
//! built only with the `dlss` feature. With [`SolariGuideBuffers`] in the render world, every Solari view gets them
//! in [`SolariGuideTextures`], whatever denoiser reads them.
use bevy_ecs::{component::Component, resource::Resource};
use bevy_math::UVec2;
use bevy_render::{
    render_resource::{
        Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
        TextureViewDescriptor,
    },
    renderer::RenderDevice,
    texture::CachedTexture,
};

/// In the render world: every Solari view writes the guides (primary surface replacement included) to its
/// [`SolariGuideTextures`]. Inserted by the denoiser that reads them, before the render app starts.
#[derive(Resource, Clone, Copy, Default)]
pub struct SolariGuideBuffers {
    /// a glossy pixel that moves as its reflection does (perceptual roughness up to 0.25) and mostly reflects is as far
    /// off as its reflection in the guide depth, as a mirror's is (resolve_dlss_rr_textures.wesl PSR_DEPTH_FOLLOWS_MOTION)
    pub reflection_depth: bool,
}

/// A Solari view's guides, at its main pass's size, written each frame by its lighting pass, as DLSS-RR's are:
/// diffuse and specular albedo (Rgba8Unorm, linear), world normal and perceptual roughness (Rgba16Float), depth
/// (R32Float, reverse-z, the reflected surface's for a mirror) and the specular motion (Rg16Float).
#[derive(Component)]
pub struct SolariGuideTextures {
    pub diffuse_albedo: CachedTexture,
    pub specular_albedo: CachedTexture,
    pub normal_roughness: CachedTexture,
    pub depth: CachedTexture,
    pub specular_motion_vectors: CachedTexture,
}

pub(super) fn make(device: &RenderDevice, size: UVec2) -> SolariGuideTextures {
    let texture = |label: &'static str, format: TextureFormat| {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some(label),
            size: Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let default_view = texture.create_view(&TextureViewDescriptor::default());
        CachedTexture { texture, default_view }
    };
    SolariGuideTextures {
        diffuse_albedo: texture("solari_guide_diffuse_albedo", TextureFormat::Rgba8Unorm),
        specular_albedo: texture("solari_guide_specular_albedo", TextureFormat::Rgba8Unorm),
        normal_roughness: texture("solari_guide_normal_roughness", TextureFormat::Rgba16Float),
        depth: texture("solari_guide_depth", TextureFormat::R32Float),
        specular_motion_vectors: texture("solari_guide_specular_motion_vectors", TextureFormat::Rg16Float),
    }
}
