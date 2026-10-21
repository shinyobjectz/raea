//! Volumetric fog as one medium in a grid along the view (`<VolumetricFog mode="froxel">`: vfog_mode on the camera
//! row). Every fog volume is written into one grid of cells (froxels: the frame cut into `tile`-pixel squares, each
//! square's view cut into `slices` cells spaced evenly in log depth), each cell lit once, and the grid integrated
//! front to back once; each pixel then reads its fog at its depth. Wronski's (Assassin's Creed IV, SIGGRAPH 2014) and
//! Hillaire's (Frostbite, SIGGRAPH 2015) design, without their blending of earlier frames: render equals play, so a
//! frame's fog is a function of that frame, and the grid is kept smooth by lighting several points a cell and reading
//! it through a tricubic B-spline instead.
//!
//! Three passes (more with hero lights, below), after the opaque pass and before the transparent one (see-through surfaces and fires fog
//! themselves at their own depth from the same grid: froxel_glass.rs):
//!
//! 1. light (froxel_light.wgsl, a fragment shader with Bevy's view bindings, so lights, cascades and shadow maps are
//!    sampled as Bevy's march samples them): each cell's medium (every volume holding it, summed: extinction, and the
//!    light it scatters toward the eye per metre) into an atlas, the slices laid out as tiles of one 2D texture;
//! 2. integrate (froxel_grid.wgsl, compute): each column of cells front to back, Hillaire's energy-conserving step
//!    (the light scattered across a cell, dimmed by the cell itself), into a 3D texture of what light has gathered and
//!    what of the scene still shows at each cell's far side;
//! 3. apply (froxel_grid.wgsl, fragment): each pixel reads the grid at its depth, blended onto the frame as
//!    scene x transmittance + light, as Bevy's fog blends.
//!
//! With hero lights (`hero`, the default: the first two volumetric spots and directional lights), their shafts are
//! left out of the grid and integrated exactly (froxel_hero.wgsl): a min-max pyramid of their shadow maps
//! (froxel_minmax.wgsl), samples along epipolar lines integrated off the frame, and the apply's draw reads each
//! pixel's shafts from them as it reads the grid.
//!
//! The model is Bevy's march's (its scattering, absorption, light tint and attenuation of light through a volume,
//! Henyey-Greenstein for directional lights, none for point and spot lights), so the reference march
//! (RAEA_FOG_REFERENCE, sdk/motion/bench/bevy/fog-score.py) is what it's scored against. What Bevy's march can't draw (a
//! volume's falloff, a <Fire>'s light) is scored against the grid's own ground truth instead: RAEA_FOG_REFERENCE=grid
//! (fog-score.py --ref grid) marches the same medium and lights per pixel, 1024 steps and no grid or hero sampling
//! (froxel_light.wgsl's `reference`), in the apply's place. Two differences, both on purpose:
//! overlapping volumes are one medium, not composited one over another in an order; and the ambient light is
//! scattered by the fog (so it goes with density, and a volume with none shows nothing), where Bevy adds it over the
//! whole box whatever its density.

use bevy::{
    camera::Camera3d,
    core_pipeline::{schedule::Core3d, FullscreenShader},
    light::FogVolume,
    math::{Affine3A, Vec3A},
    pbr::{ExtractedDirectionalLight, ExtractedPointLight, GlobalClusterableObjectMeta, MeshPipelineViewLayoutKey, ViewShadowBindings, MeshPipelineViewLayouts, MeshViewBindGroup, ViewKeyCache},
    prelude::*,
    render::{
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_resource::{
            binding_types::{sampler, storage_buffer_read_only, texture_2d, texture_2d_array, texture_3d, texture_depth_2d, texture_depth_2d_multisampled, texture_storage_2d_array, texture_storage_3d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        texture::{CachedTexture, TextureCache},
        view::{ExtractedView, ViewDepthTexture, ViewTarget},
        Extract, ExtractSchedule, Render, RenderApp, RenderStartup, RenderSystems,
    },
    shader::ShaderDefVal,
};

/// The camera's fog as a grid: the ambient light the fog scatters (linear, times its intensity), and the grid's
/// slices along the view, its tile in pixels and the points lit a cell.
#[derive(Component, Clone, Copy, ExtractComponent)]
pub(super) struct FroxelFog {
    pub ambient: Vec3,
    pub slices: u32,
    pub tile: u32,
    pub samples: u32,
    /// the volumetric spot and directional lights integrated exactly per pixel (froxel_hero.wgsl), not in the grid
    pub hero: bool,
}

pub(super) fn insert(world: &mut World, e: Entity, fog: FroxelFog) {
    // the apply pass reads the frame's depth
    if let Some(mut cam) = world.get_mut::<Camera3d>(e) {
        cam.depth_texture_usages.0 |= TextureUsages::TEXTURE_BINDING.bits();
    }
    world.entity_mut(e).insert(fog);
}

pub(super) fn install(app: &mut App) {
    bevy::asset::embedded_asset!(app, "froxel_light.wgsl");
    bevy::asset::embedded_asset!(app, "froxel_grid.wgsl");
    bevy::asset::embedded_asset!(app, "froxel_hero.wgsl");
    bevy::asset::embedded_asset!(app, "froxel_minmax.wgsl");
    app.add_plugins(ExtractComponentPlugin::<FroxelFog>::default());
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    render_app
        .init_resource::<Volumes>()
        .init_resource::<GridFires>()
        .add_systems(RenderStartup, init_pipelines.after(bevy::pbr::MeshPipelineSystems))
        .add_systems(ExtractSchedule, (extract_volumes, super::fire::extract_grid_fires))
        .add_systems(Render, ((prepare_pipelines, prepare_volumes, prepare_fires, prepare_views).in_set(RenderSystems::Prepare), prepare_textures.in_set(RenderSystems::PrepareResources)))
        .add_systems(Core3d, draw.after(bevy::core_pipeline::core_3d::main_opaque_pass_3d).before(bevy::core_pipeline::core_3d::main_transparent_pass_3d));
}

/// One fog volume as the shaders read it (froxel_light.wgsl's `Volume`).
#[derive(ShaderType, Clone, Copy, Default)]
struct GpuVolume {
    local_from_world: Mat4,
    /// rgb: the light it scatters toward the eye per metre, per unit of a light's colour (Bevy's fog colour x light
    /// tint x light intensity x scattering x density x the light's attenuation through the volume); a: extinction
    /// per metre (density x (absorption + scattering))
    scatter: Vec4,
    /// rgb: what of the ambient light it scatters per metre (fog colour x scattering x density); a: anisotropy
    ambient: Vec4,
    /// x: falloff per metre up from the box's floor; y: the box's height in metres
    shape: Vec4,
}

/// The frame's fog volumes, in rank order (their row ids), so the medium sums them in one order every frame.
#[derive(Resource, Default)]
struct Volumes {
    list: Vec<GpuVolume>,
    /// world-space corners, for the grid's depth range
    corners: Vec<[Vec3; 8]>,
    buffer: StorageBuffer<Vec<GpuVolume>>,
}

fn extract_volumes(mut volumes: ResMut<Volumes>, q: Extract<Query<(&FogVolume, &GlobalTransform, Option<&super::scene3d::FogRank>, Option<&super::scene3d::FogThinning>)>>) {
    let mut found: Vec<(String, GpuVolume, [Vec3; 8])> = Vec::new();
    for (fv, t, rank, falloff) in &q {
        let affine = t.affine();
        // a hidden volume is shrunk to nothing (scene3d::fog_transform) and has no density
        if fv.density_factor <= 0.0 || affine.matrix3.determinant().abs() < 1e-9 {
            continue;
        }
        let local_from_world = affine.inverse();
        // Bevy's bounding radius (the half-diagonal, in metres) and its attenuation of light through the volume
        let radius = affine.transform_vector3a(Vec3A::splat(0.5)).length();
        let extinction = fv.density_factor * (fv.absorption + fv.scattering);
        let colour = fv.fog_color.to_linear().to_vec3();
        let tint = fv.light_tint.to_linear().to_vec3();
        let light = colour * tint * fv.light_intensity * fv.scattering * fv.density_factor * (-extinction * radius).exp();
        let height = affine.transform_vector3a(Vec3A::Y).length();
        let corners = std::array::from_fn(|k| {
            let c = Vec3A::new(if k & 1 == 0 { -0.5 } else { 0.5 }, if k & 2 == 0 { -0.5 } else { 0.5 }, if k & 4 == 0 { -0.5 } else { 0.5 });
            Vec3::from(affine.transform_point3a(c))
        });
        found.push((
            rank.map(|r| r.0.clone()).unwrap_or_default(),
            GpuVolume {
                local_from_world: Mat4::from(local_from_world),
                scatter: light.extend(extinction),
                ambient: (colour * fv.scattering * fv.density_factor).extend(fv.scattering_asymmetry),
                shape: Vec4::new(falloff.map_or(0.0, |f| f.0), height, 0.0, 0.0),
            },
            corners,
        ));
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    volumes.list = found.iter().map(|f| f.1).collect();
    volumes.corners = found.iter().map(|f| f.2).collect();
}

fn prepare_volumes(mut volumes: ResMut<Volumes>, device: Res<RenderDevice>, queue: Res<RenderQueue>) {
    // never empty: a storage buffer needs a size
    let mut list = volumes.list.clone();
    if list.is_empty() {
        list.push(GpuVolume::default());
    }
    *volumes.buffer.get_mut() = list;
    volumes.buffer.write_buffer(&device, &queue);
}

/// A fire as the grid's light pass reads it (froxel_light.wgsl's `GridFire`): it lights the fog around it, as its
/// flame's light flickers (the SDK's Fire flickers its own PointLight the same way). The flame itself is its material's
/// (fire.rs), drawn after the grid's apply pass and fogged by the grid in front of it (froxel_fog.wgsl).
#[derive(ShaderType, Clone, Copy, Default)]
pub(super) struct GpuFire {
    pub center_time: Vec4,
    pub size_intensity: Vec4,
    pub color_seed: Vec4,
    pub speed: Vec4,
}

/// The fires shown this frame (fire::extract_grid_fires), and their buffer.
#[derive(Resource, Default)]
pub(super) struct GridFires(pub Vec<GpuFire>, StorageBuffer<Vec<GpuFire>>);

fn prepare_fires(mut fires: ResMut<GridFires>, device: Res<RenderDevice>, queue: Res<RenderQueue>) {
    let mut list = fires.0.clone();
    if list.is_empty() {
        list.push(GpuFire::default());
    }
    *fires.1.get_mut() = list;
    fires.1.write_buffer(&device, &queue);
}

/// What every pass reads of the grid (froxel_light.wgsl's and froxel_grid.wgsl's `Params`).
#[derive(ShaderType, Clone, Copy)]
struct Params {
    view_from_clip: Mat4,
    /// x, y, z: the grid's cells; w: the atlas's tiles a row
    grid: UVec4,
    /// x: tile (pixels); y: the grid's near depth; z: its far depth (metres); w: ln(far / near)
    slab: Vec4,
    /// the view's viewport in the frame (pixels)
    viewport: Vec4,
    /// rgb: the ambient light the fog scatters
    ambient: Vec4,
    /// x: volumes; y: points lit a cell; z: fires
    counts: UVec4,
    /// x, y: the hero spot lights' clustered ids (u32::MAX: none); z: the hero directional lights (the first z)
    heroes: UVec4,
    /// the hero lights' epipolar sampling (froxel_hero.wgsl): lines from each light to the border, samples along each,
    /// every how many integrated, the lights
    epi: UVec4,
    /// x: how far two ends of a stretch may differ, relative, and y: in display units, for what's between them to be
    /// interpolated; z: the relative depth difference taken as a jump; w: pixels between samples along a line
    epi_tol: Vec4,
    /// each hero light's rectangle of the frame (pixels: x0, y0, x1, y1) where it can light fog (hero_rect), and its
    /// epipolar lines and samples along each at most
    hero_rect: [Vec4; 4],
    hero_lines: UVec4,
    hero_along: UVec4,
}

/// A view's grid: its params, its atlas (pass 1) and its 3D texture (pass 2).
#[derive(Component)]
struct ViewGrid {
    params: Params,
    buffer: UniformBuffer<Params>,
    /// the shadow map layers the hero lights read, the only ones the min-max pyramid is built for
    layers: Vec<u32>,
}

#[derive(Component)]
struct GridTextures {
    atlas: CachedTexture,
    /// froxel_glass.rs's grid image when it's this view's size (so materials read what this view integrates), else
    /// the cache's
    grid: TextureView,
    /// the shadow maps' blocks, nearest and farthest depth (froxel_minmax.wgsl), for the hero lights' walk: a pyramid
    /// of BLOCKS
    blocks: Option<[(CachedTexture, TextureView); 4]>,
    /// the hero lights' epipolar samples: the coarse samples' light, every sample's light and depth, and the mask of
    /// those integrated one by one (froxel_hero.wgsl)
    epi: Option<[CachedTexture; 3]>,
}

/// texels a side of the hero lights' shadow maps' blocks, each level (froxel_minmax.wgsl's and froxel_hero.wgsl's)
const BLOCKS: [u32; 4] = [4, 16, 64, 256];

/// the epipolar samples integrated outright, every how many along a line (the rest interpolated where that's right)
const EPI_EVERY: u32 = 16;

/// The grid for each view: cells across the frame, slices from the nearest volume (or the camera's near plane) to the
/// farthest volume's farthest corner along the view.
#[allow(clippy::too_many_arguments)]
fn prepare_views(
    mut commands: Commands,
    views: Query<(Entity, &ExtractedView, &FroxelFog)>,
    volumes: Res<Volumes>,
    fires: Res<GridFires>,
    spots: Query<(Entity, &ExtractedPointLight)>,
    suns: Query<(Entity, &ExtractedDirectionalLight)>,
    clustered: Res<GlobalClusterableObjectMeta>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    // the hero lights: the first two volumetric spots (in the clustered lights' order, which prepare_lights has set)
    // and the first two volumetric directional lights (prepare_lights puts the volumetric ones first)
    let mut heroes: Vec<(u32, &ExtractedPointLight)> = spots.iter().filter(|(_, l)| l.volumetric && l.spot_light_angles.is_some()).filter_map(|(e, l)| clustered.entity_to_index.get(&e).map(|i| (*i as u32, l))).collect();
    heroes.sort_unstable_by_key(|(i, _)| *i);
    let ids: Vec<u32> = heroes.iter().map(|(i, _)| *i).collect();
    // the shadow map layers they read, as prepare_lights lays them out: each shadowed directional light's cascades in
    // its order (volumetric last, then shadowed, then by entity), then the spots' maps from their clustered index past
    // the point lights
    let mut order: Vec<(Entity, &ExtractedDirectionalLight)> = suns.iter().collect();
    order.sort_by_key(|(e, l)| (l.volumetric, l.shadow_maps_enabled, *e));
    // (each with the view depth its cascade starts at: froxel_hero.wgsl's walk reads cascade c from the last's far
    // bound, so one starting past the fog's far depth is never read)
    let (mut sun_layers, mut cascades) = (Vec::new(), 0u32);
    for (_, l) in &order {
        let bounds = &l.cascade_shadow_config.bounds;
        let n = if l.shadow_maps_enabled { bounds.len().min(bevy::pbr::MAX_CASCADES_PER_LIGHT) as u32 } else { 0 };
        if l.volumetric && sun_layers.len() < 2 {
            sun_layers.push((0..n).map(|c| (cascades + c, if c == 0 { 0.0 } else { bounds[c as usize - 1] })).collect::<Vec<_>>());
        }
        cascades += n;
    }
    let points = spots.iter().filter(|(_, l)| l.spot_light_angles.is_none()).count() as u32;
    let suns = order.iter().filter(|(_, l)| l.volumetric).count().min(2) as u32;
    for (e, view, fog) in &views {
        let vp = view.viewport.as_vec4();
        // RAEA_FOG_GRID=tile,slices,samples (tuning)
        let tune: Vec<u32> = std::env::var("RAEA_FOG_GRID").unwrap_or_default().split(',').filter_map(|v| v.parse().ok()).collect();
        let tile = tune.first().copied().unwrap_or(fog.tile).max(1);
        let gx = (vp.z as u32).div_ceil(tile).max(1);
        let gy = (vp.w as u32).div_ceil(tile).max(1);
        let gz = tune.get(1).copied().unwrap_or(fog.slices).clamp(8, 256);
        let samples = tune.get(2).copied().unwrap_or(fog.samples);
        // the atlas: slices as tiles, as square as fits under 16384 a side
        let cols = gz.min((16384 / gx).max(1));
        let view_from_world = view.world_from_view.affine().inverse();
        // the camera's near plane (Bevy's perspective keeps it in clip_from_view's w column)
        let cam_near = view.clip_from_view.w_axis.z.abs().max(0.01);
        let (mut near, mut far) = (f32::MAX, 0.0f32);
        for corners in &volumes.corners {
            let depths: Vec<f32> = corners.iter().map(|c| -view_from_world.transform_point3(*c).z).collect();
            let (lo, hi) = depths.iter().fold((f32::MAX, f32::MIN), |(lo, hi), d| (lo.min(*d), hi.max(*d)));
            if hi <= 0.0 {
                continue;
            }
            near = near.min(lo);
            far = far.max(hi);
        }
        // a camera inside a volume, or a volume reaching behind it: from just past the camera (a quarter metre, or its
        // near plane if that's farther; the first slice runs from the camera itself)
        let near = near.max(cam_near).max(0.25);
        let far = far.max(near * 2.0);
        let params = Params {
            view_from_clip: view.clip_from_view.inverse(),
            grid: UVec4::new(gx, gy, gz, cols),
            slab: Vec4::new(tile as f32, near, far, (far / near).ln()),
            viewport: vp,
            ambient: fog.ambient.extend(0.0),
            counts: UVec4::new(volumes.list.len() as u32, samples.max(1), fires.0.len() as u32, reference_steps().unwrap_or(0)),
            epi: UVec4::ZERO,
            epi_tol: Vec4::ZERO,
            hero_rect: [Vec4::ZERO; 4],
            hero_lines: UVec4::ZERO,
            hero_along: UVec4::ZERO,
            heroes: if fog.hero { UVec4::new(ids.first().copied().unwrap_or(u32::MAX), ids.get(1).copied().unwrap_or(u32::MAX), suns, 1) } else { UVec4::new(u32::MAX, u32::MAX, 0, 0) },
        };
        let mut params = params;
        if fog.hero {
            // RAEA_FOG_EPI=line spacing,sample spacing,K,rel,abs,depth (tuning)
            let tune: Vec<f32> = std::env::var("RAEA_FOG_EPI").unwrap_or_default().split(',').filter_map(|v| v.parse().ok()).collect();
            let t = |i: usize, d: f32| tune.get(i).copied().unwrap_or(d);
            let (line_gap, spacing) = (t(0, 8.0).max(0.5), t(1, 6.0).max(0.5));
            // the fog's box in the world, which every hero light's rectangle is inside
            let fog_box = volumes.corners.iter().flatten().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), c| (lo.min(*c), hi.max(*c)));
            let clip_from_world = view.clip_from_view * Mat4::from(view_from_world);
            let mut slot = 0;
            // the hero lights in the shader's slot order: the spots, then the directional lights
            for l in heroes.iter().take(2).map(|(_, l)| Some(*l)).chain((0..suns).map(|_| None)) {
                let r = match l {
                    Some(l) => spot_rect(l, fog_box, clip_from_world, vp),
                    None => hero_rect(clip_from_world, fog_box, vp),
                };
                let size = (r.zw() - r.xy()).max(Vec2::ZERO);
                params.hero_rect[slot] = r;
                if size.x >= 1.0 && size.y >= 1.0 {
                    // lines `line_gap` pixels apart round the rectangle's border, samples `spacing` apart along each
                    params.hero_lines[slot] = ((2.0 * (size.x + size.y) / line_gap).ceil() as u32).clamp(16, 8192);
                    params.hero_along[slot] = ((size.length() / spacing).ceil() as u32 + 2).clamp(4, 8192);
                }
                slot += 1;
            }
            let n = slot as u32;
            let lines = (0..4).map(|i| params.hero_lines[i]).max().unwrap_or(0).max(1);
            let along = (0..4).map(|i| params.hero_along[i]).max().unwrap_or(0).max(4);
            params.epi = UVec4::new(lines, along, t(2, EPI_EVERY as f32) as u32, n);
            params.epi_tol = Vec4::new(t(3, 0.2), t(4, 2.0 / 255.0), t(5, 0.02), spacing);
            if std::env::var_os("RAEA_FOG_DEBUG").is_some() {
                eprintln!("froxel heroes: rects {:?} lines {:?} along {:?} layers {:?}", &params.hero_rect[..slot], params.hero_lines, params.hero_along, (cascades, points));
            }
        }
        let mut buffer = UniformBuffer::from(params);
        buffer.write_buffer(&device, &queue);
        let mut layers = Vec::new();
        if fog.hero {
            layers.extend(ids.iter().take(2).map(|id| cascades + id - points));
            layers.extend(sun_layers.iter().take(suns as usize).flatten().filter(|(_, near)| *near < far).map(|(layer, _)| *layer));
        }
        commands.entity(e).insert(ViewGrid { params, buffer, layers });
    }
}

/// a spot light's part of the frame: its cone out to its range, inside the fog's box. The box round both is cut into
/// 12 x 12 x 12 cells, and each cell the cone (widened by a cell) reaches is put on the frame whole
fn spot_rect(l: &ExtractedPointLight, fog: (Vec3, Vec3), clip_from_world: Mat4, vp: Vec4) -> Vec4 {
    hero_rect(clip_from_world, spot_box(l, fog), vp)
}

/// a spot's cone inside the fog's box, as a box
fn spot_box(l: &ExtractedPointLight, fog: (Vec3, Vec3)) -> (Vec3, Vec3) {
    let apex = l.transform.translation();
    let axis = l.transform.forward().as_vec3();
    let outer = l.spot_light_angles.map_or(std::f32::consts::FRAC_PI_4, |a| a.1).min(1.55);
    // the cone's box: the apex and the disc at its range along the axis (whose radius takes in every point that near)
    let centre = apex + axis * l.range;
    let reach = l.range * outer.tan() * (Vec3::ONE - axis * axis).max(Vec3::ZERO).map(f32::sqrt);
    let lo = apex.min(centre - reach).max(fog.0);
    let hi = apex.max(centre + reach).min(fog.1);
    if lo.cmpgt(hi).any() {
        return (Vec3::MAX, Vec3::MIN);
    }
    const N: usize = 12;
    let cell = (hi - lo) / N as f32;
    let half_diag = 0.5 * cell.length();
    let (mut a, mut b) = (Vec3::MAX, Vec3::MIN);
    for i in 0..N * N * N {
        let c = lo + cell * (Vec3::new((i % N) as f32, ((i / N) % N) as f32, (i / (N * N)) as f32) + 0.5);
        let to = c - apex;
        let d = to.length();
        if d > l.range + half_diag {
            continue;
        }
        // in the cone widened by the cell: its angle off the axis less the angle the cell's half-diagonal spans
        let off = (to.dot(axis) / d.max(1e-6)).clamp(-1.0, 1.0).acos();
        if d > half_diag && off - (half_diag / d).min(1.0).asin() > outer {
            continue;
        }
        a = a.min(c - 0.5 * cell);
        b = b.max(c + 0.5 * cell);
    }
    (a, b)
}

/// a box in the world as a rectangle of the frame (pixels: x0, y0, x1, y1), two pixels wider each way; the whole frame
/// if it reaches behind the camera, none (zero size) if it's empty or off the frame
fn hero_rect(clip_from_world: Mat4, (lo, hi): (Vec3, Vec3), vp: Vec4) -> Vec4 {
    let (f0, f1) = (vp.xy(), vp.xy() + vp.zw());
    if lo.cmpgt(hi).any() {
        return Vec4::ZERO;
    }
    let (mut a, mut b) = (Vec2::MAX, Vec2::MIN);
    for k in 0..8 {
        let c = Vec3::new(if k & 1 == 0 { lo.x } else { hi.x }, if k & 2 == 0 { lo.y } else { hi.y }, if k & 4 == 0 { lo.z } else { hi.z });
        let clip = clip_from_world * c.extend(1.0);
        if clip.w <= 1e-4 {
            return Vec4::new(f0.x, f0.y, f1.x, f1.y);
        }
        let px = vp.xy() + (clip.xy() / clip.w * Vec2::new(0.5, -0.5) + 0.5) * vp.zw();
        a = a.min(px);
        b = b.max(px);
    }
    let (a, b) = ((a - 2.0).max(f0), (b + 2.0).min(f1));
    if b.cmple(a).any() {
        return Vec4::ZERO;
    }
    Vec4::new(a.x, a.y, b.x, b.y)
}

#[allow(clippy::too_many_arguments)]
fn prepare_textures(
    mut commands: Commands,
    mut cache: ResMut<TextureCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    views: Query<(Entity, &ViewGrid, Option<&ViewShadowBindings>)>,
    images: Option<Res<super::froxel_glass::GridImages>>,
    gpu: Res<bevy::render::render_asset::RenderAssets<bevy::render::texture::GpuImage>>,
    volumes: Res<Volumes>,
) {
    let mut shared_taken = false;
    for (e, grid, shadows) in &views {
        let g = grid.params.grid;
        // the materials' grid, when it's this view's size and on the GPU, and its layout's texels (froxel_fog.wgsl):
        // cells, slices, tile; near, far, ln(far / near), drawn; viewport
        let shared = images.as_ref().filter(|i| !shared_taken && i.size == UVec3::new(g.x, g.y, g.z)).and_then(|i| Some((gpu.get(&i.grid)?, gpu.get(&i.params)?)));
        if let Some((_, params)) = shared {
            let sl = grid.params.slab;
            let texels = [
                Vec4::new(g.x as f32, g.y as f32, g.z as f32, sl.x),
                Vec4::new(sl.y, sl.z, sl.w, if volumes.list.is_empty() { 0.0 } else { 1.0 }),
                grid.params.viewport,
                Vec4::ZERO,
            ];
            queue.write_texture(
                params.texture.as_image_copy(),
                bytemuck::cast_slice(&texels),
                TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * 16), rows_per_image: None },
                Extent3d { width: 4, height: 1, depth_or_array_layers: 1 },
            );
        }
        let shared = shared.map(|(grid, _)| grid.texture_view.clone());
        shared_taken |= shared.is_some();
        let rows = g.z.div_ceil(g.w);
        let atlas = cache.get(
            &device,
            TextureDescriptor {
                label: Some("froxel_atlas"),
                size: Extent3d { width: g.x * g.w, height: g.y * rows, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        );
        let grid3 = shared.unwrap_or_else(|| cache.get(
            &device,
            TextureDescriptor {
                label: Some("froxel_grid"),
                size: Extent3d { width: g.x, height: g.y, depth_or_array_layers: g.z },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D3,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        ).default_view);
        let h = grid.params.heroes;
        let heroes = h.w != 0 && (h.x != u32::MAX || h.y != u32::MAX || h.z > 0);
        let blocks = shadows.filter(|_| heroes).map(|s| {
            let t = &s.directional_light_depth_texture;
            let layers = t.depth_or_array_layers();
            BLOCKS.map(|block| {
            let texture = cache.get(
                &device,
                TextureDescriptor {
                    label: Some("froxel_shadow_blocks"),
                    size: Extent3d { width: t.width().div_ceil(block), height: t.height().div_ceil(block), depth_or_array_layers: layers },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: TextureFormat::Rg32Float,
                    usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
            );
            let view = texture.texture.create_view(&TextureViewDescriptor { label: Some("froxel_shadow_blocks_array"), dimension: Some(TextureViewDimension::D2Array), ..default() });
            (texture, view)
            })
        });
        let ep = grid.params.epi;
        // the coarse samples: every K-th and the last, a light's band of rows (froxel_hero.wgsl's coarse_rows)
        let coarse_rows = (ep.y.max(2) - 1).div_ceil(ep.z.max(1)) + 1;
        let epi = (blocks.is_some() && ep.w > 0).then(|| {
            [(TextureFormat::Rgba16Float, coarse_rows), (TextureFormat::Rgba16Float, ep.y), (TextureFormat::Depth32Float, ep.y)].map(|(format, rows)| {
                cache.get(
                    &device,
                    TextureDescriptor {
                        label: Some("froxel_epipolar"),
                        size: Extent3d { width: ep.x, height: rows * ep.w, depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: TextureDimension::D2,
                        format,
                        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    },
                )
            })
        });
        commands.entity(e).insert(GridTextures { atlas, grid: grid3, blocks, epi });
    }
}

/// The three passes' layouts and shaders.
#[derive(Resource)]
struct Pipelines {
    view_layouts: MeshPipelineViewLayouts,
    light_layout: BindGroupLayoutDescriptor,
    /// the light pass's and the frame's depth, for the reference [single-sampled, multisampled]
    reference_layouts: [BindGroupLayoutDescriptor; 2],
    integrate_layout: BindGroupLayoutDescriptor,
    /// [single-sampled, multisampled]
    apply_layouts: [BindGroupLayoutDescriptor; 2],
    hero_layouts: [BindGroupLayoutDescriptor; 2],
    /// the epipolar passes' samples read back (froxel_hero.wgsl's group 2)
    epi_layout: BindGroupLayoutDescriptor,
    /// [the 4 x 4 blocks from the shadow maps, a level's blocks from the level below's]
    minmax_layouts: [BindGroupLayoutDescriptor; 2],
    light_shader: Handle<Shader>,
    hero_shader: Handle<Shader>,
    minmax_shader: Handle<Shader>,
    grid_shader: Handle<Shader>,
    fullscreen: VertexState,
    sampler: Option<Sampler>,
    /// nearest, for gathering the shadow maps' texels
    nearest: Option<Sampler>,
}

fn init_pipelines(mut commands: Commands, view_layouts: Res<MeshPipelineViewLayouts>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>, device: Res<RenderDevice>) {
    let light_layout = BindGroupLayoutDescriptor::new(
        "froxel_light_layout",
        &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (uniform_buffer::<Params>(false), storage_buffer_read_only::<GpuVolume>(false), storage_buffer_read_only::<GpuFire>(false))),
    );
    let reference = |ms: bool| {
        let depth = if ms { texture_depth_2d_multisampled() } else { texture_depth_2d() };
        BindGroupLayoutDescriptor::new(
            "froxel_reference_layout",
            &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (uniform_buffer::<Params>(false), storage_buffer_read_only::<GpuVolume>(false), storage_buffer_read_only::<GpuFire>(false), depth)),
        )
    };
    let integrate_layout = BindGroupLayoutDescriptor::new(
        "froxel_integrate_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (uniform_buffer::<Params>(false), texture_2d(TextureSampleType::Float { filterable: false }), texture_storage_3d(TextureFormat::Rgba16Float, StorageTextureAccess::WriteOnly)),
        ),
    );
    let apply = |ms: bool| {
        let depth = if ms { texture_depth_2d_multisampled() } else { texture_depth_2d() };
        BindGroupLayoutDescriptor::new(
            "froxel_apply_layout",
            &BindGroupLayoutEntries::with_indices(
                ShaderStages::FRAGMENT,
                ((0, uniform_buffer::<Params>(false)), (3, texture_3d(TextureSampleType::Float { filterable: true })), (4, sampler(SamplerBindingType::Filtering)), (5, depth)),
            ),
        )
    };
    let hero = |ms: bool| {
        let depth = if ms { texture_depth_2d_multisampled() } else { texture_depth_2d() };
        BindGroupLayoutDescriptor::new(
            "froxel_hero_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    uniform_buffer::<Params>(false),
                    storage_buffer_read_only::<GpuVolume>(false),
                    texture_3d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    depth,
                    texture_2d_array(TextureSampleType::Float { filterable: false }),
                    texture_2d_array(TextureSampleType::Float { filterable: false }),
                    texture_2d_array(TextureSampleType::Float { filterable: false }),
                    texture_2d_array(TextureSampleType::Float { filterable: false }),
                    sampler(SamplerBindingType::NonFiltering),
                ),
            ),
        )
    };
    let minmax_layouts = [
        BindGroupLayoutDescriptor::new(
            "froxel_minmax4_layout",
            &BindGroupLayoutEntries::with_indices(
                ShaderStages::COMPUTE,
                (
                    (0, BindingType::Texture { sample_type: TextureSampleType::Depth, view_dimension: TextureViewDimension::D2Array, multisampled: false }),
                    (1, sampler(SamplerBindingType::NonFiltering)),
                    (2, texture_storage_2d_array(TextureFormat::Rg32Float, StorageTextureAccess::WriteOnly)),
                    (5, texture_storage_2d_array(TextureFormat::Rg32Float, StorageTextureAccess::WriteOnly)),
                ),
            ),
        ),
        BindGroupLayoutDescriptor::new(
            "froxel_minmax_reduce_layout",
            &BindGroupLayoutEntries::with_indices(ShaderStages::COMPUTE, ((3, texture_2d_array(TextureSampleType::Float { filterable: false })), (4, texture_storage_2d_array(TextureFormat::Rg32Float, StorageTextureAccess::WriteOnly)))),
        ),
    ];
    let nearest = device.create_sampler(&SamplerDescriptor {
        label: Some("froxel_nearest_sampler"),
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        ..default()
    });
    let sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("froxel_grid_sampler"),
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        address_mode_w: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    commands.insert_resource(Pipelines {
        view_layouts: view_layouts.clone(),
        light_layout,
        reference_layouts: [reference(false), reference(true)],
        integrate_layout,
        apply_layouts: [apply(false), apply(true)],
        hero_layouts: [hero(false), hero(true)],
        epi_layout: BindGroupLayoutDescriptor::new(
            "froxel_epipolar_layout",
            &BindGroupLayoutEntries::single(ShaderStages::FRAGMENT, texture_2d(TextureSampleType::Float { filterable: false })),
        ),
        minmax_layouts,
        light_shader: assets.load("embedded://rdb/engine/froxel_light.wgsl"),
        hero_shader: assets.load("embedded://rdb/engine/froxel_hero.wgsl"),
        minmax_shader: assets.load("embedded://rdb/engine/froxel_minmax.wgsl"),
        grid_shader: assets.load("embedded://rdb/engine/froxel_grid.wgsl"),
        fullscreen: fullscreen.to_vertex_state(),
        sampler: Some(sampler),
        nearest: Some(nearest),
    });
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
enum Key {
    Light(MeshPipelineViewLayoutKey),
    /// multisampled, the target's format, its samples
    Apply(bool, TextureFormat, u32),
    Hero(MeshPipelineViewLayoutKey, TextureFormat, u32),
    /// the epipolar passes into their samples: place, coarse, mark, fine
    Epi(MeshPipelineViewLayoutKey, u8),
    Epipolar(MeshPipelineViewLayoutKey, TextureFormat, u32),
    Reference(MeshPipelineViewLayoutKey, TextureFormat, u32),
}

#[derive(Component)]
struct ViewPipelines {
    light: CachedRenderPipelineId,
    integrate: CachedComputePipelineId,
    apply: CachedRenderPipelineId,
    hero: CachedRenderPipelineId,
    /// coarse, mark, fine, then over the frame
    epi: [CachedRenderPipelineId; 4],
    /// [level4, reduce]
    minmax: [CachedComputePipelineId; 2],
    /// the ground truth instead of the grid (RAEA_FOG_REFERENCE=grid)
    reference: Option<CachedRenderPipelineId>,
}

#[derive(Default)]
struct PipelineIds {
    render: bevy::platform::collections::HashMap<Key, CachedRenderPipelineId>,
    compute: Option<CachedComputePipelineId>,
    minmax: Option<[CachedComputePipelineId; 2]>,
}

fn light_descriptor(p: &Pipelines, view_key: MeshPipelineViewLayoutKey) -> RenderPipelineDescriptor {
    let mut defs: Vec<ShaderDefVal> = vec!["SHADOW_FILTER_METHOD_HARDWARE_2X2".into()];
    if view_key.contains(MeshPipelineViewLayoutKey::MULTISAMPLED) {
        defs.push("MULTISAMPLED".into());
    }
    if view_key.contains(MeshPipelineViewLayoutKey::ATMOSPHERE) {
        defs.push("ATMOSPHERE".into());
    }
    RenderPipelineDescriptor {
        label: Some("froxel_light".into()),
        layout: vec![p.view_layouts.get_view_layout(view_key).main_layout, p.light_layout.clone()],
        vertex: p.fullscreen.clone(),
        fragment: Some(FragmentState {
            shader: p.light_shader.clone(),
            shader_defs: defs,
            entry_point: Some("light".into()),
            targets: vec![Some(ColorTargetState { format: TextureFormat::Rgba16Float, blend: None, write_mask: ColorWrites::ALL })],
        }),
        ..default()
    }
}

/// The fog's ground truth marched per pixel (froxel_light.wgsl's `reference`), blended onto the frame as the apply.
fn reference_descriptor(p: &Pipelines, view_key: MeshPipelineViewLayoutKey, format: TextureFormat, samples: u32) -> RenderPipelineDescriptor {
    let mut d = light_descriptor(p, view_key);
    let ms = view_key.contains(MeshPipelineViewLayoutKey::MULTISAMPLED);
    d.label = Some("froxel_reference".into());
    d.layout[1] = p.reference_layouts[ms as usize].clone();
    let fragment = d.fragment.as_mut().unwrap();
    fragment.entry_point = Some("reference".into());
    fragment.targets = apply_descriptor(p, ms, format, samples).fragment.unwrap().targets;
    d.multisample = MultisampleState { count: samples, ..default() };
    d
}

/// RAEA_FOG_REFERENCE=grid: the steps of the ground truth drawn in the grid's place (RAEA_FOG_REFERENCE_STEPS, 1024)
pub(super) fn reference_steps() -> Option<u32> {
    (std::env::var("RAEA_FOG_REFERENCE").as_deref() == Ok("grid")).then(|| std::env::var("RAEA_FOG_REFERENCE_STEPS").ok().and_then(|v| v.parse().ok()).unwrap_or(1024).max(1))
}

/// how a hero pass draws (hero_descriptor)
#[derive(Clone, Copy, Default)]
struct HeroPass {
    /// added onto the frame
    blend: bool,
    /// blended as the grid's apply (frame x a + rgb: the grid's fog drawn with it)
    fog: bool,
    /// reads the epipolar samples (group 2)
    epi: bool,
    /// the samples' mask: written (mark), or tested (fine: drawn at depth one half where it's 0)
    mask: Option<bool>,
}

/// a hero pass: `entry` into `format`, which has `samples`
fn hero_descriptor(p: &Pipelines, view_key: MeshPipelineViewLayoutKey, entry: &'static str, format: TextureFormat, samples: u32, how: HeroPass) -> RenderPipelineDescriptor {
    let HeroPass { blend, fog, epi, mask } = how;
    let ms = view_key.contains(MeshPipelineViewLayoutKey::MULTISAMPLED);
    let mut defs: Vec<ShaderDefVal> = vec!["SHADOW_FILTER_METHOD_HARDWARE_2X2".into()];
    if std::env::var_os("RAEA_FOG_TRICUBIC_T").is_some() {
        defs.push("TRICUBIC_TRANSMITTANCE".into());
    }
    if std::env::var_os("RAEA_FOG_SHOW_FALLBACK").is_some() {
        defs.push("SHOW_FALLBACK".into());
    }
    if ms {
        defs.push("MULTISAMPLED".into());
    }
    if view_key.contains(MeshPipelineViewLayoutKey::ATMOSPHERE) {
        defs.push("ATMOSPHERE".into());
    }
    RenderPipelineDescriptor {
        label: Some(format!("froxel_hero_{entry}").into()),
        layout: [p.view_layouts.get_view_layout(view_key).main_layout, p.hero_layouts[ms as usize].clone()].into_iter().chain(epi.then(|| p.epi_layout.clone())).collect(),
        vertex: if mask == Some(false) { VertexState { shader: p.hero_shader.clone(), shader_defs: defs.clone(), entry_point: Some("at_half".into()), buffers: vec![] } } else { p.fullscreen.clone() },
        depth_stencil: mask.map(|write| DepthStencilState {
            format: TextureFormat::Depth32Float,
            depth_write_enabled: Some(write),
            depth_compare: Some(if write { CompareFunction::Always } else { CompareFunction::Greater }),
            stencil: default(),
            bias: default(),
        }),
        fragment: Some(FragmentState {
            shader: p.hero_shader.clone(),
            shader_defs: defs,
            entry_point: Some(entry.into()),
            targets: vec![Some(ColorTargetState {
                format,
                // the shafts' light, added (or with the grid's fog, over what still shows)
                blend: blend.then_some(BlendState {
                    color: BlendComponent { src_factor: BlendFactor::One, dst_factor: if fog { BlendFactor::SrcAlpha } else { BlendFactor::One }, operation: BlendOperation::Add },
                    alpha: BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::One, operation: BlendOperation::Add },
                }),
                write_mask: ColorWrites::ALL,
            })],
        }),
        multisample: MultisampleState { count: samples, ..default() },
        ..default()
    }
}

fn apply_descriptor(p: &Pipelines, ms: bool, format: TextureFormat, samples: u32) -> RenderPipelineDescriptor {
    let mut defs: Vec<ShaderDefVal> = Vec::new();
    if ms {
        defs.push("MULTISAMPLED".into());
    }
    RenderPipelineDescriptor {
        label: Some("froxel_apply".into()),
        layout: vec![p.apply_layouts[ms as usize].clone()],
        vertex: p.fullscreen.clone(),
        fragment: Some(FragmentState {
            shader: p.grid_shader.clone(),
            shader_defs: defs,
            entry_point: Some("apply".into()),
            targets: vec![Some(ColorTargetState {
                format,
                // the frame x what still shows + the light gathered
                blend: Some(BlendState {
                    color: BlendComponent { src_factor: BlendFactor::One, dst_factor: BlendFactor::SrcAlpha, operation: BlendOperation::Add },
                    alpha: BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::One, operation: BlendOperation::Add },
                }),
                write_mask: ColorWrites::ALL,
            })],
        }),
        multisample: MultisampleState { count: samples, ..default() },
        ..default()
    }
}

fn prepare_pipelines(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    pipelines: Option<Res<Pipelines>>,
    view_keys: Res<ViewKeyCache>,
    mut ids: Local<PipelineIds>,
    views: Query<(Entity, &ExtractedView, &Msaa), With<FroxelFog>>,
) {
    let Some(p) = pipelines else { return };
    for (e, view, msaa) in &views {
        let Some(mesh_key) = view_keys.get(&view.retained_view_entity) else { continue };
        let view_key: MeshPipelineViewLayoutKey = (*mesh_key).into();
        let ms = !matches!(msaa, Msaa::Off);
        let light = *ids.render.entry(Key::Light(view_key)).or_insert_with(|| cache.queue_render_pipeline(light_descriptor(&p, view_key)));
        // drawn into the multisampled frame, which the transparent pass after them resolves over the main texture
        let samples = msaa.samples();
        let apply = *ids.render.entry(Key::Apply(ms, view.target_format, samples)).or_insert_with(|| cache.queue_render_pipeline(apply_descriptor(&p, ms, view.target_format, samples)));
        let hero = *ids.render.entry(Key::Hero(view_key, view.target_format, samples)).or_insert_with(|| cache.queue_render_pipeline(hero_descriptor(&p, view_key, "hero", view.target_format, samples, HeroPass { blend: true, ..default() })));
        let mut epi = [hero; 4];
        let passes = [
            ("coarse", TextureFormat::Rgba16Float, HeroPass::default()),
            ("mark", TextureFormat::Rgba16Float, HeroPass { epi: true, mask: Some(true), ..default() }),
            ("fine", TextureFormat::Rgba16Float, HeroPass { mask: Some(false), ..default() }),
        ];
        for (i, (entry, format, how)) in passes.into_iter().enumerate() {
            epi[i] = *ids.render.entry(Key::Epi(view_key, i as u8)).or_insert_with(|| cache.queue_render_pipeline(hero_descriptor(&p, view_key, entry, format, 1, how)));
        }
        epi[3] = *ids.render.entry(Key::Epipolar(view_key, view.target_format, samples)).or_insert_with(|| cache.queue_render_pipeline(hero_descriptor(&p, view_key, "epipolar", view.target_format, samples, HeroPass { blend: true, fog: true, epi: true, ..default() })));
        let integrate = *ids.compute.get_or_insert_with(|| {
            cache.queue_compute_pipeline(ComputePipelineDescriptor {
                label: Some("froxel_integrate".into()),
                layout: vec![p.integrate_layout.clone()],
                shader: p.grid_shader.clone(),
                entry_point: Some("integrate".into()),
                ..default()
            })
        });
        let minmax = *ids.minmax.get_or_insert_with(|| {
            [("level4", 0), ("reduce", 1)].map(|(entry, i)| {
                cache.queue_compute_pipeline(ComputePipelineDescriptor {
                    label: Some(format!("froxel_minmax_{entry}").into()),
                    layout: vec![p.minmax_layouts[i].clone()],
                    shader: p.minmax_shader.clone(),
                    entry_point: Some(entry.into()),
                    ..default()
                })
            })
        });
        let reference = reference_steps().map(|_| *ids.render.entry(Key::Reference(view_key, view.target_format, samples)).or_insert_with(|| cache.queue_render_pipeline(reference_descriptor(&p, view_key, view.target_format, samples))));
        commands.entity(e).insert(ViewPipelines { light, integrate, apply, hero, epi, minmax, reference });
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    view: ViewQuery<(&ViewTarget, &ViewDepthTexture, &ViewPipelines, &ViewGrid, &GridTextures, &MeshViewBindGroup, &Msaa, Option<&ViewShadowBindings>)>,
    cache: Res<PipelineCache>,
    pipelines: Option<Res<Pipelines>>,
    volumes: Res<Volumes>,
    fires: Res<GridFires>,
    mut ctx: RenderContext,
) {
    let Some(p) = pipelines else { return };
    let (target, depth, ids, grid, tex, view_group, msaa, shadows) = view.into_inner();
    if volumes.list.is_empty() {
        return;
    }
    let (Some(light), Some(integrate), Some(apply), Some(params), Some(vols), Some(fire_list), Some(sampler)) = (
        cache.get_render_pipeline(ids.light),
        cache.get_compute_pipeline(ids.integrate),
        cache.get_render_pipeline(ids.apply),
        grid.buffer.binding(),
        volumes.buffer.binding(),
        fires.1.binding(),
        p.sampler.as_ref(),
    ) else {
        return;
    };
    let ms = !matches!(msaa, Msaa::Off);
    let device = ctx.render_device().clone();
    // RAEA_FOG_SKIP=pass,...: those passes not drawn (timing their cost)
    let skip = std::env::var("RAEA_FOG_SKIP").unwrap_or_default();
    let g = grid.params.grid;
    ctx.command_encoder().push_debug_group("froxel_fog");


    // 1. each cell's medium and the light it scatters, into the atlas
    let group = device.create_bind_group("froxel_light", &cache.get_bind_group_layout(&p.light_layout), &BindGroupEntries::sequential((params.clone(), vols.clone(), fire_list.clone())));
    {
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some("froxel_light"),
            color_attachments: &[Some(RenderPassColorAttachment { view: &tex.atlas.default_view, depth_slice: None, resolve_target: None, ops: Operations { load: LoadOp::Clear(Default::default()), store: StoreOp::Store } })],
            ..default()
        });
        pass.set_pipeline(light);
        pass.set_bind_group(0, &view_group.main, &view_group.main_offsets);
        pass.set_bind_group(1, &group, &[]);
        if !skip.contains("light") {
            pass.draw(0..3, 0..1);
        }
    }

    // 2. each column front to back
    let group = device.create_bind_group("froxel_integrate", &cache.get_bind_group_layout(&p.integrate_layout), &BindGroupEntries::sequential((params.clone(), &tex.atlas.default_view, &tex.grid)));
    {
        let mut pass = ctx.command_encoder().begin_compute_pass(&ComputePassDescriptor { label: Some("froxel_integrate"), timestamp_writes: None });
        pass.set_pipeline(integrate);
        pass.set_bind_group(0, &group, &[]);
        if !skip.contains("integrate") {
            pass.dispatch_workgroups(g.x.div_ceil(8), g.y.div_ceil(8), 1);
        }
    }

    // the ground truth in the grid's place over the frame (the grid still made: transparent surfaces read their fog
    // from it, froxel_glass.rs)
    if let Some(id) = ids.reference {
        let Some(pipeline) = cache.get_render_pipeline(id) else {
            ctx.command_encoder().pop_debug_group();
            return;
        };
        let group = device.create_bind_group("froxel_reference", &cache.get_bind_group_layout(&p.reference_layouts[ms as usize]), &BindGroupEntries::sequential((params, vols, fire_list, depth.view())));
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor { label: Some("froxel_reference"), color_attachments: &[Some(target.get_color_attachment())], ..default() });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &view_group.main, &view_group.main_offsets);
        pass.set_bind_group(1, &group, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        ctx.command_encoder().pop_debug_group();
        return;
    }

    // 3. the hero lights' shadow pyramid and their epipolar samples (each pass off the frame)
    let mut heroes: Option<(BindGroup, &RenderPipeline, Option<BindGroup>)> = None;
    // the epipolar draw draws the grid's fog too
    let mut fused = false;
    if let (Some(blocks), Some(shadows), Some(hero), Some(m4), Some(reduce), Some(nearest)) =
        (tex.blocks.as_ref(), shadows, cache.get_render_pipeline(ids.hero), cache.get_compute_pipeline(ids.minmax[0]), cache.get_compute_pipeline(ids.minmax[1]), p.nearest.as_ref())
    {
        {
            // the hero lights' layers only, each through views of that one layer
            let t = &shadows.directional_light_depth_texture;
            let one = |tex: &Texture, layer: u32, aspect: TextureAspect| tex.create_view(&TextureViewDescriptor { label: Some("froxel_shadow_layer"), dimension: Some(TextureViewDimension::D2Array), aspect, base_array_layer: layer, array_layer_count: Some(1), ..default() });
            let mut groups = Vec::new();
            for &layer in grid.layers.iter().filter(|l| **l < t.depth_or_array_layers()) {
                let views: Vec<TextureView> = blocks.iter().map(|b| one(&b.0.texture, layer, TextureAspect::All)).collect();
                // the 4s and 16s in one pass, 32 x 32 texels a workgroup (its dispatch: the 4s' size / 8)
                groups.push((m4, device.create_bind_group("froxel_minmax4", &cache.get_bind_group_layout(&p.minmax_layouts[0]), &BindGroupEntries::with_indices(((0, &one(t, layer, TextureAspect::DepthOnly)), (1, nearest), (2, &views[0]), (5, &views[1])))), blocks[0].0.texture.size()));
                for l in 2..blocks.len() {
                    let g = device.create_bind_group("froxel_minmax_reduce", &cache.get_bind_group_layout(&p.minmax_layouts[1]), &BindGroupEntries::with_indices(((3, &views[l - 1]), (4, &views[l]))));
                    groups.push((reduce, g, blocks[l].0.texture.size()));
                }
            }
            let mut pass = ctx.command_encoder().begin_compute_pass(&ComputePassDescriptor { label: Some("froxel_minmax"), timestamp_writes: None });
            for (pipeline, group, size) in groups.iter().filter(|_| !skip.contains("minmax")) {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, group, &[]);
                pass.dispatch_workgroups(size.width.div_ceil(8), size.height.div_ceil(8), 1);
            }
        }
        let group = device.create_bind_group(
            "froxel_hero",
            &cache.get_bind_group_layout(&p.hero_layouts[ms as usize]),
            &BindGroupEntries::sequential((grid.buffer.binding().unwrap(), volumes.buffer.binding().unwrap(), &tex.grid, sampler, depth.view(), &blocks[0].1, &blocks[1].1, &blocks[2].1, &blocks[3].1, nearest)),
        );
        let epi = [0, 1, 2, 3].map(|i| cache.get_render_pipeline(ids.epi[i]));
        match (tex.epi.as_ref(), epi) {
            (Some(t), [Some(coarse), Some(mark), Some(fine), Some(over)]) if std::env::var_os("RAEA_FOG_HERO_EXACT").is_none() => {
                // each light's samples along its lines: their depth; the coarse ones' light; every one's, interpolated
                // or marked; the marked integrated
                let reads = |a: &CachedTexture| device.create_bind_group("froxel_epipolar", &cache.get_bind_group_layout(&p.epi_layout), &BindGroupEntries::single(&a.default_view));
                let clear = |i: usize| RenderPassColorAttachment { view: &t[i].default_view, depth_slice: None, resolve_target: None, ops: Operations { load: LoadOp::Clear(Default::default()), store: StoreOp::Store } };
                let mask = |load: bool| RenderPassDepthStencilAttachment { view: &t[2].default_view, depth_ops: Some(Operations { load: if load { LoadOp::Load } else { LoadOp::Clear(1.0) }, store: StoreOp::Store }), stencil_ops: None };
                let steps: [(&str, &RenderPipeline, RenderPassColorAttachment, Option<RenderPassDepthStencilAttachment>, Option<BindGroup>); 3] = [
                    ("coarse", coarse, clear(0), None, None),
                    ("mark", mark, clear(1), Some(mask(false)), Some(reads(&t[0]))),
                    ("fine", fine, RenderPassColorAttachment { view: &t[1].default_view, depth_slice: None, resolve_target: None, ops: Operations { load: LoadOp::Load, store: StoreOp::Store } }, Some(mask(true)), None),
                ];
                for (name, pipeline, attachment, depth_attachment, reads) in steps {
                    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
                        label: Some("froxel_hero_epipolar"),
                        color_attachments: &[Some(attachment)],
                        depth_stencil_attachment: depth_attachment,
                        ..default()
                    });
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, &view_group.main, &view_group.main_offsets);
                    pass.set_bind_group(1, &group, &[]);
                    if let Some(r) = &reads {
                        pass.set_bind_group(2, r, &[]);
                    }
                    if !skip.contains("hero") && !skip.contains(name) {
                        pass.draw(0..3, 0..1);
                    }
                }
                // over the frame: the samples read at each pixel
                heroes = Some((group, over, Some(reads(&t[1]))));
                fused = true;
            }
            _ => heroes = Some((group, hero, None)),
        }
    }

    // 4. over the frame, in one pass (each pass over it loads and stores it whole): the grid's fog at each pixel's
    // depth, then the hero lights' shafts added
    let group = device.create_bind_group(
        "froxel_apply",
        &cache.get_bind_group_layout(&p.apply_layouts[ms as usize]),
        &BindGroupEntries::with_indices(((0, params), (3, &tex.grid), (4, sampler), (5, depth.view()))),
    );
    {
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some("froxel_apply"),
            color_attachments: &[Some(target.get_color_attachment())],
            ..default()
        });
        if !fused {
            pass.set_pipeline(apply);
            pass.set_bind_group(0, &group, &[]);
            if !skip.contains("apply") {
                pass.draw(0..3, 0..1);
            }
        }
        if let Some((hero_group, pipeline, reads)) = &heroes {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &view_group.main, &view_group.main_offsets);
            pass.set_bind_group(1, hero_group, &[]);
            if let Some(r) = reads {
                pass.set_bind_group(2, r, &[]);
            }
            if !skip.contains("hero") && !skip.contains("epipolar") {
                pass.draw(0..3, 0..1);
            }
        }
    }
    ctx.command_encoder().pop_debug_group();
}
