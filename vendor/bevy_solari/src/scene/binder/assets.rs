use super::{
    allocator::{RetainedBindingArray, SlotAllocator},
    instances::InstanceState,
    StandardMaterialAssets,
};
use bevy_asset::AssetId;
use bevy_color::{Alpha, ColorToComponents, LinearRgba};
use bevy_image::Image;
use bevy_material::AlphaMode;
use bevy_math::{Vec2, Vec3, Vec4};
use bevy_pbr::StandardMaterial;
use bevy_platform::collections::{HashMap, HashSet};
use bevy_render::{
    impl_atomic_pod,
    render_asset::{ExtractedAssets, RenderAssets},
    render_resource::{AtomicPod, AtomicSparseBufferVec, BufferUsages, Sampler, TextureView},
    texture::GpuImage,
};
use bevy_utils::once;
use bytemuck::{Pod, Zeroable};
use core::num::NonZeroU32;
use tracing::{info_span, warn};

pub const MAX_TEXTURE_COUNT: NonZeroU32 = NonZeroU32::new(5_000).unwrap();
const TEXTURE_MAP_NONE: u32 = u32::MAX;

/// The four textures a [`StandardMaterial`] can reference, in [`GpuMaterial`] field order.
type MaterialTextures = [Option<AssetId<Image>>; 4];

#[derive(Clone, Copy, Default, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct GpuMaterial {
    normal_map_texture_id: u32,
    base_color_texture_id: u32,
    emissive_texture_id: u32,
    metallic_roughness_texture_id: u32,

    base_color: Vec3,
    perceptual_roughness: f32,
    emissive: Vec3,
    metallic: f32,
    // raea: the base colour's alpha, how it's used (ALPHA_* below) and Mask's cutoff, in what was padding, so a ray can
    // pass through what the raster passes draw see-through
    alpha: f32,
    alpha_mode: u32,
    alpha_cutoff: f32,
    reflectance: f32,
    // raea: StandardMaterial's uv_transform (columns x and y, then the translation), applied to the mesh's UVs before
    // its textures are read, as the raster passes do
    uv_transform_xy: Vec4,
    uv_transform_t: Vec2,
    // raea: StandardMaterial's diffuse_transmission, for a ray's light through a leaf or a curtain (brdf.wesl)
    diffuse_transmission: f32,
    // raea: MATERIAL_* below
    flags: u32,
}

// raea: GpuMaterial's flags (bindings.wesl MATERIAL_*): StandardMaterial's double_sided, so a ray shades a back face
pub const MATERIAL_DOUBLE_SIDED: u32 = 1;

// raea: how a traced ray treats a material's alpha (bindings.wesl's alpha_accepts)
pub const ALPHA_OPAQUE: u32 = 0;
pub const ALPHA_MASK: u32 = 1;
pub const ALPHA_BLEND: u32 = 2;
pub const ALPHA_PREMULTIPLIED: u32 = 3;
pub const ALPHA_ADD: u32 = 4;

/// raea: a StandardMaterial's alpha mode as the tracer reads it, and its cutoff (Mask's).
fn alpha_of(mode: AlphaMode) -> (u32, f32) {
    match mode {
        AlphaMode::Opaque => (ALPHA_OPAQUE, 0.0),
        AlphaMode::Mask(cutoff) => (ALPHA_MASK, cutoff),
        AlphaMode::AlphaToCoverage => (ALPHA_MASK, 0.5),
        AlphaMode::Blend => (ALPHA_BLEND, 0.0),
        AlphaMode::Premultiplied => (ALPHA_PREMULTIPLIED, 0.0),
        AlphaMode::Add | AlphaMode::Multiply => (ALPHA_ADD, 0.0),
    }
}

impl_atomic_pod!(GpuMaterial, GpuMaterialBlob);

/// Stable material and texture slots, plus the retry state for assets that aren't ready yet.
pub struct AssetState {
    pub textures: RetainedBindingArray<AssetId<Image>, (TextureView, Sampler)>,
    pub materials: AtomicSparseBufferVec<GpuMaterial>,
    pub material_slots: SlotAllocator<AssetId<StandardMaterial>>,
    material_textures: HashMap<AssetId<StandardMaterial>, MaterialTextures>,
    pub emissive_materials: HashSet<AssetId<StandardMaterial>>,
    /// raea: the materials a ray may pass through (any alpha mode but Opaque), whose instances go in the TLAS under
    /// their own mask so only the alpha query tests them
    pub alpha_materials: HashSet<AssetId<StandardMaterial>>,
    /// Materials to retry because at least one required texture is not on the GPU yet.
    unresolved_materials: HashSet<AssetId<StandardMaterial>>,
    /// Bound images whose replacement GPU data has not landed yet.
    pending_texture_updates: HashSet<AssetId<Image>>,
}

impl AssetState {
    pub fn new() -> Self {
        Self {
            textures: RetainedBindingArray::new(),
            materials: AtomicSparseBufferVec::new(BufferUsages::STORAGE, "solari_materials".into()),
            material_slots: SlotAllocator::new(),
            material_textures: HashMap::default(),
            emissive_materials: HashSet::default(),
            alpha_materials: HashSet::default(),
            unresolved_materials: HashSet::default(),
            pending_texture_updates: HashSet::default(),
        }
    }

    pub fn update_materials(
        &mut self,
        instances: &mut InstanceState,
        material_assets: &StandardMaterialAssets,
        texture_assets: &RenderAssets<GpuImage>,
    ) {
        let _span = info_span!("update_materials").entered();

        for material_id in &material_assets.removed {
            self.remove_material(*material_id, instances);
        }
        for material_id in &material_assets.changed {
            self.update_material(*material_id, instances, material_assets, texture_assets);
        }
    }

    /// Swaps in uploaded replacements and retries materials waiting on images.
    pub fn update_textures(
        &mut self,
        instances: &mut InstanceState,
        extracted_images: &ExtractedAssets<GpuImage>,
        texture_assets: &RenderAssets<GpuImage>,
        material_assets: &StandardMaterialAssets,
    ) {
        let _span = info_span!("update_textures").entered();

        let mut pending = core::mem::take(&mut self.pending_texture_updates);
        pending.extend(extracted_images.added.iter().copied());

        for image_id in pending {
            if !self.textures.contains(&image_id) {
                continue;
            }

            match texture_assets.get(image_id) {
                Some(image) => self.textures.replace(
                    &image_id,
                    (image.texture_view.clone(), image.sampler.clone()),
                ),
                None => {
                    self.pending_texture_updates.insert(image_id);
                }
            }
        }

        for material_id in core::mem::take(&mut self.unresolved_materials) {
            self.update_material(material_id, instances, material_assets, texture_assets);
        }
    }

    /// Resolves a material's textures and writes it into its stable slot.
    fn update_material(
        &mut self,
        material_id: AssetId<StandardMaterial>,
        instances: &mut InstanceState,
        material_assets: &StandardMaterialAssets,
        texture_assets: &RenderAssets<GpuImage>,
    ) {
        let Some(material) = material_assets.get(&material_id) else {
            self.remove_material(material_id, instances);
            return;
        };

        let was_resolved = self.material_slots.contains(&material_id);
        let handles = [
            &material.normal_map_texture,
            &material.base_color_texture,
            &material.emissive_texture,
            &material.metallic_roughness_texture,
        ];

        // Resolve first so a missing texture leaves no partially acquired slots
        let mut textures: MaterialTextures = [None; 4];
        for (slot, handle) in textures.iter_mut().zip(handles) {
            let Some(handle) = handle else { continue };
            let image_id = handle.id();
            if texture_assets.get(image_id).is_none() {
                self.defer_material(material_id, instances);
                return;
            }
            *slot = Some(image_id);
        }

        if self.new_texture_count(&textures) > self.textures.vacancies(MAX_TEXTURE_COUNT.get()) {
            // At the limit, release the old set once so a replacement can reuse those slots
            self.release_material_textures(material_id);
            self.material_textures.remove(&material_id);

            if self.new_texture_count(&textures) > self.textures.vacancies(MAX_TEXTURE_COUNT.get())
            {
                once!(warn!(
                    "Solari scene needs more than {} textures. Materials past that limit will not \
                     be rendered.",
                    MAX_TEXTURE_COUNT.get()
                ));
                self.defer_material(material_id, instances);
                return;
            }
        }

        let mut texture_ids = [TEXTURE_MAP_NONE; 4];
        for (texture_id, image_id) in texture_ids.iter_mut().zip(textures) {
            let Some(image_id) = image_id else { continue };
            let image = texture_assets.get(image_id).unwrap();
            if let Some(slot) = self
                .textures
                .acquire(image_id, MAX_TEXTURE_COUNT.get(), || {
                    (image.texture_view.clone(), image.sampler.clone())
                })
            {
                *texture_id = slot;
            }
        }

        self.release_material_textures(material_id);
        self.material_textures.insert(material_id, textures);
        self.unresolved_materials.remove(&material_id);

        let slot = self.material_slots.get_or_allocate(material_id);
        let emissive = material.emissive.to_vec3();
        let is_emissive = emissive != Vec3::ZERO;
        let (alpha_mode, alpha_cutoff) = alpha_of(material.alpha_mode);
        let uv = material.uv_transform;
        self.materials.grow_and_set(
            slot,
            GpuMaterial {
                normal_map_texture_id: texture_ids[0],
                base_color_texture_id: texture_ids[1],
                emissive_texture_id: texture_ids[2],
                metallic_roughness_texture_id: texture_ids[3],
                base_color: LinearRgba::from(material.base_color).to_vec3(),
                perceptual_roughness: material.perceptual_roughness.clamp(0.0, 1.0),
                emissive,
                metallic: material.metallic.clamp(0.0, 1.0),
                reflectance: material.reflectance,
                alpha: material.base_color.alpha(),
                alpha_mode,
                alpha_cutoff,
                uv_transform_xy: Vec4::new(uv.matrix2.x_axis.x, uv.matrix2.x_axis.y, uv.matrix2.y_axis.x, uv.matrix2.y_axis.y),
                uv_transform_t: uv.translation,
                diffuse_transmission: material.diffuse_transmission.clamp(0.0, 1.0),
                flags: if material.double_sided { MATERIAL_DOUBLE_SIDED } else { 0 },
            },
        );
        // raea: a material that turns see-through (or back) moves its instances between the TLAS masks
        let is_alpha = alpha_mode != ALPHA_OPAQUE;
        let was_alpha = if is_alpha { !self.alpha_materials.insert(material_id) } else { self.alpha_materials.remove(&material_id) };
        if was_resolved && was_alpha != is_alpha {
            instances.invalidate_material(material_id);
        }

        let was_emissive = if is_emissive {
            !self.emissive_materials.insert(material_id)
        } else {
            self.emissive_materials.remove(&material_id)
        };
        if !was_resolved || was_emissive != is_emissive {
            instances.invalidate_material(material_id);
        }
    }

    fn new_texture_count(&self, textures: &MaterialTextures) -> u32 {
        let mut count = 0;
        for (index, image_id) in textures.iter().enumerate() {
            let Some(image_id) = image_id else { continue };
            let counted_already = textures[..index].contains(&Some(*image_id));
            if !counted_already && !self.textures.contains(image_id) {
                count += 1;
            }
        }
        count
    }

    fn defer_material(
        &mut self,
        material_id: AssetId<StandardMaterial>,
        instances: &mut InstanceState,
    ) {
        self.remove_material(material_id, instances);
        self.unresolved_materials.insert(material_id);
    }

    fn remove_material(
        &mut self,
        material_id: AssetId<StandardMaterial>,
        instances: &mut InstanceState,
    ) {
        self.unresolved_materials.remove(&material_id);
        if self.material_slots.remove(&material_id).is_none() {
            return;
        }

        self.release_material_textures(material_id);
        self.material_textures.remove(&material_id);
        self.emissive_materials.remove(&material_id);
        self.alpha_materials.remove(&material_id);
        instances.invalidate_material(material_id);
    }

    fn release_material_textures(&mut self, material_id: AssetId<StandardMaterial>) {
        let Some(textures) = self.material_textures.get(&material_id).copied() else {
            return;
        };
        for image_id in textures.into_iter().flatten() {
            self.textures.release(&image_id);
        }
    }
}
