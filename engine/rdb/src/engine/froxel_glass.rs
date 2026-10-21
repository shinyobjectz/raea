//! See-through surfaces in a froxel-fogged scene (froxel.rs). The grid's apply pass runs between Bevy's opaque and
//! transparent passes, so it fogs what's behind a blended surface at that depth; the surface then fogs itself at its
//! own depth from the same grid (froxel_glass.wgsl), as Bevy's distance fog does in the standard material's shader.
//! Without that the apply pass, after everything, fogged a pane with the whole ray's fog behind it, and the fog
//! between the pane and the wall showed through the pane's colour undimmed.
//!
//! The grid the camera integrates into is an image asset (GridImages), so a material can bind it: the grid itself and
//! a few texels of its layout (cells, slices, depths, viewport) that prepare_textures writes each frame. While a camera
//! has froxel fog, each mesh drawn with a blended standard material (blend, premultiplied, add) is drawn with a copy
//! of it extended by the fog (GlassMaterial) instead; the standard material stays the one its rows drive
//! (scene3d::sync_meshes, look::sync_flat find it through Fogged) and the copy follows it. <Fire> binds the grid the
//! same way (fire.rs).
//!
//! The hero lights (froxel_hero.wgsl) are integrated per pixel to the opaque depth only: a surface in front of a hero
//! shaft dims it by its opacity.

use super::*;
use bevy::asset::{AssetEvent, AssetEventSystems, RenderAssetUsages};
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::shader::ShaderRef;
use std::collections::HashMap;

pub(super) type GlassMaterial = ExtendedMaterial<StandardMaterial, Glass>;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub(super) struct Glass {
    #[texture(100, dimension = "3d")]
    #[sampler(101)]
    grid: Handle<Image>,
    #[texture(102, sample_type = "float", filterable = false)]
    params: Handle<Image>,
}

impl MaterialExtension for Glass {
    fn fragment_shader() -> ShaderRef {
        "embedded://rdb/engine/froxel_glass.wgsl".into()
    }
}

/// The froxel camera's grid as images a material binds (froxel_fog.wgsl): the grid (cells across, down, slices) and
/// its layout's texels. One camera's: a second froxel camera's surfaces read the first's grid.
#[derive(Resource, Clone, ExtractResource)]
pub(super) struct GridImages {
    pub grid: Handle<Image>,
    pub params: Handle<Image>,
    pub size: UVec3,
}

/// A mesh drawn with its blended standard material's fog copy: the standard material, which its rows still drive,
/// and the copy last made from it.
#[derive(Component)]
pub(super) struct Fogged {
    pub std: Handle<StandardMaterial>,
    copied: AssetId<StandardMaterial>,
}

/// Each blended standard material's fog copy, shared by the meshes drawn with it.
#[derive(Resource, Default)]
struct Copies(HashMap<AssetId<StandardMaterial>, Handle<GlassMaterial>>);

pub(super) fn install(app: &mut App) {
    bevy::shader::load_shader_library!(app, "froxel_fog.wgsl");
    bevy::asset::embedded_asset!(app, "froxel_glass.wgsl");
    let images = {
        let mut imgs = app.world_mut().resource_mut::<Assets<Image>>();
        GridImages { grid: imgs.add(grid_image(UVec3::ONE)), params: imgs.add(params_image()), size: UVec3::ONE }
    };
    app.insert_resource(images)
        .init_resource::<Copies>()
        .add_plugins((MaterialPlugin::<GlassMaterial>::default(), ExtractResourcePlugin::<GridImages>::default()))
        // before the assets' events go out, so a copy made or changed here reaches the render world this frame
        .add_systems(PostUpdate, (size_grid, fog_surfaces).chain().before(AssetEventSystems));
}

fn grid_image(size: UVec3) -> Image {
    let mut img = Image::new_uninit(
        Extent3d { width: size.x, height: size.y, depth_or_array_layers: size.z },
        TextureDimension::D3,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    // the integrate pass writes it
    img.texture_descriptor.usage |= TextureUsages::STORAGE_BINDING;
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
    img
}

fn params_image() -> Image {
    // zeros: no grid drawn (texel 1's w), until prepare_textures writes the frame's
    Image::new(
        Extent3d { width: 4, height: 1, depth_or_array_layers: 1 },
        TextureDimension::D2,
        vec![0; 4 * 16],
        TextureFormat::Rgba32Float,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// The images the materials bind for the grid the camera's fog makes (froxel.rs prepare_views's size), remade when
/// that size changes, and every fog copy and fire pointed at the new grid.
fn size_grid(
    cams: Query<(&Camera, &super::froxel::FroxelFog)>,
    mut images: ResMut<GridImages>,
    mut imgs: ResMut<Assets<Image>>,
    mut glasses: ResMut<Assets<GlassMaterial>>,
    mut fires: ResMut<Assets<super::fire::FireMaterial>>,
) {
    let Some((cam, fog)) = cams.iter().find(|(c, _)| c.is_active) else { return };
    let Some(vp) = cam.physical_viewport_size() else { return };
    let tile = fog.tile.max(1);
    let size = UVec3::new(vp.x.div_ceil(tile).max(1), vp.y.div_ceil(tile).max(1), fog.slices.clamp(8, 256));
    if size == images.size {
        return;
    }
    images.grid = imgs.add(grid_image(size));
    images.size = size;
    for (_, g) in glasses.iter_mut() {
        g.extension.grid = images.grid.clone();
    }
    for (_, f) in fires.iter_mut() {
        f.grid = images.grid.clone();
    }
}

fn blended(m: &StandardMaterial) -> bool {
    matches!(m.alpha_mode, AlphaMode::Blend | AlphaMode::Premultiplied | AlphaMode::Add)
}

/// While a camera has froxel fog, each mesh with a blended standard material drawn with its fog copy instead (and
/// back to the standard material when the fog goes or the material stops blending); each copy remade from its
/// standard material when that changes (a frame behind, from its assets' events; sync_meshes keeps the copy of what
/// the rows animate in step itself).
#[allow(clippy::too_many_arguments)]
fn fog_surfaces(
    mut commands: Commands,
    cams: Query<(), With<super::froxel::FroxelFog>>,
    plain: Query<(Entity, &MeshMaterial3d<StandardMaterial>)>,
    mut fogged: Query<(Entity, &mut Fogged)>,
    stds: Res<Assets<StandardMaterial>>,
    mut glasses: ResMut<Assets<GlassMaterial>>,
    mut copies: ResMut<Copies>,
    images: Res<GridImages>,
    mut events: MessageReader<AssetEvent<StandardMaterial>>,
) {
    let on = !cams.is_empty();
    for ev in events.read() {
        if let AssetEvent::Modified { id } = ev {
            if let (Some(h), Some(std)) = (copies.0.get(id), stds.get(*id)) {
                if let Some(mut g) = glasses.get_mut(h) {
                    g.base = std.clone();
                }
            }
        }
    }
    let mut copy = |id: AssetId<StandardMaterial>, std: &StandardMaterial| {
        copies.0.entry(id).or_insert_with(|| glasses.add(GlassMaterial { base: std.clone(), extension: Glass { grid: images.grid.clone(), params: images.params.clone() } })).clone()
    };
    for (e, mut f) in &mut fogged {
        let std = stds.get(&f.std);
        if !on || !std.is_some_and(blended) {
            commands.entity(e).remove::<(Fogged, MeshMaterial3d<GlassMaterial>)>().insert(MeshMaterial3d(f.std.clone()));
        } else if f.copied != f.std.id() {
            // the rows put another standard material on it (sync_flat's unlit copy)
            let id = f.std.id();
            f.copied = id;
            commands.entity(e).insert(MeshMaterial3d(copy(id, std.unwrap())));
        }
    }
    if !on {
        return;
    }
    for (e, m) in &plain {
        let Some(std) = stds.get(&m.0).filter(|s| blended(s)) else { continue };
        let glass = copy(m.0.id(), std);
        commands.entity(e).remove::<MeshMaterial3d<StandardMaterial>>().insert((MeshMaterial3d(glass), Fogged { std: m.0.clone(), copied: m.0.id() }));
    }
}
