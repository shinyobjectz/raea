//! Each view's froxel grid for a frame: its parameters, the grid and its textures, and the spot lights' and hero
//! volumes' screen rectangles that bound the work.

use super::*;

/// What every pass reads of the grid (froxel_light.wesl's and froxel_grid.wesl's `Params`).
#[derive(ShaderType, Clone, Copy)]
pub struct Params {
    pub view_from_clip: Mat4,
    /// x, y, z: the grid's cells; w: the atlas's tiles a row
    pub grid: UVec4,
    /// x: tile (pixels); y: the grid's near depth; z: its far depth (metres); w: ln(far / near)
    pub slab: Vec4,
    /// the view's viewport in the frame (pixels)
    pub viewport: Vec4,
    /// rgb: the ambient light the fog scatters
    pub ambient: Vec4,
    /// x: volumes; y: points lit a cell; z: fires
    pub counts: UVec4,
    /// x, y: the hero spot lights' clustered ids (u32::MAX: none); z: the hero directional lights (the first z)
    pub heroes: UVec4,
    /// the hero lights' epipolar sampling (froxel_hero.wesl): lines from each light to the border, samples along each,
    /// every how many integrated, the lights
    pub epi: UVec4,
    /// x: how far two ends of a stretch may differ, relative, and y: in display units, for what's between them to be
    /// interpolated; z: the relative depth difference taken as a jump; w: pixels between samples along a line
    pub epi_tol: Vec4,
    /// each hero light's rectangle of the frame (pixels: x0, y0, x1, y1) where it can light fog (hero_rect), and its
    /// epipolar lines and samples along each at most
    pub hero_rect: [Vec4; 4],
    pub hero_lines: UVec4,
    pub hero_along: UVec4,
}

/// A view's grid: its params, its atlas (pass 1) and its 3D texture (pass 2).
#[derive(Component)]
pub struct ViewGrid {
    pub params: Params,
    pub buffer: UniformBuffer<Params>,
    /// the shadow map layers the hero lights read, the only ones the min-max pyramid is built for
    pub layers: Vec<u32>,
}

#[derive(Component)]
pub struct GridTextures {
    pub atlas: CachedTexture,
    /// froxel_glass.rs's grid image when it's this view's size (so materials read what this view integrates), else
    /// the cache's
    pub grid: TextureView,
    /// the shadow maps' blocks, nearest and farthest depth (froxel_minmax.wgsl), for the hero lights' walk: a pyramid
    /// of BLOCKS
    pub blocks: Option<[(CachedTexture, TextureView); 4]>,
    /// the hero lights' epipolar samples: the coarse samples' light, every sample's light and depth, and the mask of
    /// those integrated one by one (froxel_hero.wesl)
    pub epi: Option<[CachedTexture; 3]>,
}

/// texels a side of the hero lights' shadow maps' blocks, each level (froxel_minmax.wgsl's and froxel_hero.wesl's)
pub const BLOCKS: [u32; 4] = [4, 16, 64, 256];

/// the epipolar samples integrated outright, every how many along a line (the rest interpolated where that's right)
pub const EPI_EVERY: u32 = 16;

/// The grid for each view: cells across the frame, slices from the nearest volume (or the camera's near plane) to the
/// farthest volume's farthest corner along the view.
#[allow(clippy::too_many_arguments)]
pub fn prepare_views(
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
    // (each with the view depth its cascade starts at: froxel_hero.wesl's walk reads cascade c from the last's far
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
        // the props, RAEA_FOG_GRID over them (tuning.rs)
        let tile = fog.tile.max(1);
        let gx = (vp.z as u32).div_ceil(tile).max(1);
        let gy = (vp.w as u32).div_ceil(tile).max(1);
        let gz = fog.slices.clamp(8, 256);
        let samples = fog.samples;
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
            // <VolumetricFog epipolar>, RAEA_FOG_EPI over it (tuning.rs)
            let e = fog.tune.epi;
            let (line_gap, spacing) = (e.line_gap, e.spacing);
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
            params.epi = UVec4::new(lines, along, e.every, n);
            params.epi_tol = Vec4::new(e.rel, e.abs, e.depth, spacing);
            if raea_io_trace::stat("RAEA_FOG_DEBUG") {
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
pub fn spot_rect(l: &ExtractedPointLight, fog: (Vec3, Vec3), clip_from_world: Mat4, vp: Vec4) -> Vec4 {
    hero_rect(clip_from_world, spot_box(l, fog), vp)
}

/// a spot's cone inside the fog's box, as a box
pub fn spot_box(l: &ExtractedPointLight, fog: (Vec3, Vec3)) -> (Vec3, Vec3) {
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
pub fn hero_rect(clip_from_world: Mat4, (lo, hi): (Vec3, Vec3), vp: Vec4) -> Vec4 {
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
pub fn prepare_textures(
    mut commands: Commands,
    mut cache: ResMut<TextureCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    views: Query<(Entity, &ViewGrid, Option<&ViewShadowBindings>)>,
    images: Option<Res<super::super::froxel_glass::GridImages>>,
    gpu: Res<bevy::render::render_asset::RenderAssets<bevy::render::texture::GpuImage>>,
    volumes: Res<Volumes>,
) {
    let mut shared_taken = false;
    for (e, grid, shadows) in &views {
        let g = grid.params.grid;
        // the materials' grid, when it's this view's size and on the GPU, and its layout's texels (froxel_fog.wesl):
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
        // the coarse samples: every K-th and the last, a light's band of rows (froxel_hero.wesl's coarse_rows)
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
