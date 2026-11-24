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
//! 1. light (froxel_light.wesl, a fragment shader with Bevy's view bindings, so lights, cascades and shadow maps are
//!    sampled as Bevy's march samples them): each cell's medium (every volume holding it, summed: extinction, and the
//!    light it scatters toward the eye per metre) into an atlas, the slices laid out as tiles of one 2D texture;
//! 2. integrate (froxel_grid.wesl, compute): each column of cells front to back, Hillaire's energy-conserving step
//!    (the light scattered across a cell, dimmed by the cell itself), into a 3D texture of what light has gathered and
//!    what of the scene still shows at each cell's far side;
//! 3. apply (froxel_grid.wesl, fragment): each pixel reads the grid at its depth, blended onto the frame as
//!    scene x transmittance + light, as Bevy's fog blends.
//!
//! With hero lights (`hero`, the default: the first two volumetric spots and directional lights), their shafts are
//! left out of the grid and integrated exactly (froxel_hero.wesl): a min-max pyramid of their shadow maps
//! (froxel_minmax.wgsl), samples along epipolar lines integrated off the frame, and the apply's draw reads each
//! pixel's shafts from them as it reads the grid.
//!
//! The model is Bevy's march's (its scattering, absorption, light tint and attenuation of light through a volume,
//! Henyey-Greenstein for directional lights, none for point and spot lights), so the reference march
//! (RAEA_FOG_REFERENCE, sdk/raea/bench/bevy/fog-score.py) is what it's scored against. What Bevy's march can't draw (a
//! volume's falloff, a <Fire>'s light) is scored against the grid's own ground truth instead: RAEA_FOG_REFERENCE=grid
//! (fog-score.py --ref grid) marches the same medium and lights per pixel, 1024 steps and no grid or hero sampling
//! (froxel_light.wesl's `reference`), in the apply's place. Two differences, both on purpose:
//! overlapping volumes are one medium, not composited one over another in an order; and the ambient light is
//! scattered by the fog (so it goes with density, and a volume with none shows nothing), where Bevy adds it over the
//! whole box whatever its density.

use bevy::{
    camera::Camera3d,
    core_pipeline::{schedule::Core3d, FullscreenShader},
    light::FogVolume,
    math::Vec3A,
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
        view::{ExtractedView, ViewDepthStencilTexture, ViewTarget},
        Extract, ExtractSchedule, Render, RenderApp, RenderStartup, RenderSystems,
    },
    shader::ShaderDefVal,
    solari::pathtracer::PathtracerMedia,
};

mod views;
pub use views::*;
mod pipelines;
pub use pipelines::*;
mod solari;
pub mod tuning;

/// The camera's fog as a grid: the ambient light the fog scatters (linear, times its intensity), and the grid's
/// slices along the view, its tile in pixels and the points lit a cell.
#[derive(Component, Clone, Copy, ExtractComponent)]
#[extract_app(bevy::render::RenderApp)]
pub struct FroxelFog {
    pub ambient: Vec3,
    pub slices: u32,
    pub tile: u32,
    pub samples: u32,
    /// the volumetric spot and directional lights integrated exactly per pixel (froxel_hero.wesl), not in the grid
    pub hero: bool,
    /// what <VolumetricFog> tunes beyond its look (tuning.rs)
    pub tune: tuning::Tuning,
}

pub fn insert(world: &mut World, e: Entity, fog: FroxelFog) {
    // the apply pass reads the frame's depth
    if let Some(mut cam) = world.get_mut::<Camera3d>(e) {
        cam.depth_texture_usages.0 |= TextureUsages::TEXTURE_BINDING.bits();
    }
    world.entity_mut(e).insert(fog);
}

pub fn install(app: &mut App) {
    bevy::shader::load_shader_library!(app, "froxel_volume.wesl");
    bevy::asset::embedded_asset!(app, "froxel_light.wesl");
    bevy::asset::embedded_asset!(app, "froxel_grid.wesl");
    bevy::asset::embedded_asset!(app, "froxel_hero.wesl");
    bevy::asset::embedded_asset!(app, "froxel_minmax.wgsl");
    bevy::asset::embedded_asset!(app, "froxel_solari.wesl");
    app.add_plugins(ExtractComponentPlugin::<FroxelFog>::default());
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    render_app
        .init_resource::<Volumes>()
        .init_resource::<GridFires>()
        .add_systems(RenderStartup, init_pipelines.after(bevy::pbr::MeshPipelineSystems))
        .add_systems(ExtractSchedule, (extract_volumes, super::fire::extract_grid_fires))
        .add_systems(Render, ((prepare_pipelines, prepare_volumes, prepare_fires, prepare_views).in_set(RenderSystems::Prepare), (prepare_textures, solari::prepare).in_set(RenderSystems::PrepareResources)))
        .add_systems(Core3d, draw.after(bevy::core_pipeline::core_3d::main_opaque_pass_3d).before(bevy::core_pipeline::core_3d::main_transparent_pass_3d));
}

/// One fog volume as the shaders read it (froxel_light.wesl's `Volume`).
#[derive(ShaderType, Clone, Copy, Default)]
struct GpuVolume {
    local_from_world: Mat4,
    /// rgb: the light it scatters toward the eye per metre, per unit of a light's colour (Bevy's fog colour x light
    /// tint x light intensity x scattering x density x the light's attenuation through the volume); a: extinction
    /// per metre (density x (absorption + scattering))
    scatter: Vec4,
    /// rgb: what of the ambient light it scatters per metre (fog colour x scattering x density); a: anisotropy
    ambient: Vec4,
    /// x: falloff per metre up from the box's floor; y: the box's height in metres; z: the feathered share of the
    /// half-size at each face (`soft`); w: how much narrower the floor is than the top (`taper`); froxel_volume.wesl
    shape: Vec4,
    /// x: the extinction per metre that dims light on its way through the volume to a point of the fog
    /// (froxel_volume.wesl volume_depth), by [`ShadowRule`]
    shadow: Vec4,
}

/// How the fog dims light on its way to a point of it (<VolumetricFog shadow>). The froxel passes scatter light once, so
/// dimming it by the whole extinction loses the light the medium scatters on toward the point (white fog under a
/// uniform sky: 0.75 of the truth's); not dimming it keeps light that a grey or thick medium absorbs (grey fog: 0.60
/// against 0.40). The path tracer is the truth they're measured against (bevy_solari pathtracer/media.wesl):
/// absorption alone matched it (grey 0.411 against 0.398, white 1.000; the hall's air 0.955 of the truth's, Bevy's
/// 0.939), so it's the default.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ShadowRule {
    /// Bevy's: a constant exp(-extinction x the box's half-diagonal) on every point of it, nothing else
    Bevy,
    /// not dimmed
    Off,
    /// by what the medium absorbs (density x absorption): the default
    Absorb,
    /// by absorption and the share of scattering that turns light aside (density x (absorption + scattering x (1 - g)))
    Delta,
    /// by the whole extinction (single scattering)
    Full,
}


/// The frame's fog volumes, in rank order (their row ids), so the medium sums them in one order every frame.
#[derive(Resource, Default)]
pub struct Volumes {
    list: Vec<GpuVolume>,
    /// world-space corners, for the grid's depth range
    corners: Vec<[Vec3; 8]>,
    buffer: StorageBuffer<Vec<GpuVolume>>,
}

fn extract_volumes(mut volumes: ResMut<Volumes>, q: Extract<Query<(&FogVolume, &GlobalTransform, Option<&FogRank>, Option<&FogShape>)>>, cams: Extract<Query<&FroxelFog>>) {
    // the volumes are written once for every view: the first fog camera's rule
    let rule = cams.iter().next().map_or(ShadowRule::Absorb, |f| f.tune.shadow);
    let mut found: Vec<(String, GpuVolume, [Vec3; 8])> = Vec::new();
    for (fv, t, rank, shape) in &q {
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
        let bevy = if rule == ShadowRule::Bevy { (-extinction * radius).exp() } else { 1.0 };
        let light = colour * tint * fv.light_intensity * fv.scattering * fv.density_factor * bevy;
        let shadow = fv.density_factor
            * match rule {
                ShadowRule::Bevy | ShadowRule::Off => 0.0,
                ShadowRule::Absorb => fv.absorption,
                ShadowRule::Delta => fv.absorption + fv.scattering * (1.0 - fv.scattering_asymmetry.clamp(0.0, 1.0)),
                ShadowRule::Full => fv.absorption + fv.scattering,
            };
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
                shape: shape.map_or(Vec4::new(0.0, height, 0.0, 0.0), |s| Vec4::new(s.falloff, height, s.soft, s.taper)),
                shadow: Vec4::new(shadow, 0.0, 0.0, 0.0),
            },
            corners,
        ));
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    volumes.list = found.iter().map(|f| f.1).collect();
    volumes.corners = found.iter().map(|f| f.2).collect();
}

fn prepare_volumes(mut volumes: ResMut<Volumes>, device: Res<RenderDevice>, queue: Res<RenderQueue>, media: Option<ResMut<PathtracerMedia>>, traced: Query<&raea_sys_rt::pathtracer::PathtracerSettings>) {
    // never empty: a storage buffer needs a size
    let mut list = volumes.list.clone();
    if list.is_empty() {
        list.push(GpuVolume::default());
    }
    *volumes.buffer.get_mut() = list;
    volumes.buffer.write_buffer(&device, &queue);
    // the reference path tracer walks the same volumes (bevy_solari pathtracer/media.wesl); <Pathtracer media={false}>
    // (or RAEA_PT_MEDIA=0) leaves them out of it, for a frame without its air
    if let Some(mut media) = media {
        media.0 = if raea_sys_rt::pathtracer::media(&traced) { volumes.buffer.buffer().cloned() } else { None };
    }
}

/// A fire as the grid's light pass reads it (froxel_light.wesl's `GridFire`): it lights the fog around it, as its
/// flame's light flickers (the SDK's Fire flickers its own PointLight the same way). The flame itself is its material's
/// (fire.rs), drawn after the grid's apply pass and fogged by the grid in front of it (froxel_fog.wesl).
#[derive(ShaderType, Clone, Copy, Default)]
pub struct GpuFire {
    pub center_time: Vec4,
    pub size_intensity: Vec4,
    pub color_seed: Vec4,
    pub speed: Vec4,
}

/// The fires shown this frame (fire::extract_grid_fires), and their buffer.
#[derive(Resource, Default)]
pub struct GridFires(pub Vec<GpuFire>, StorageBuffer<Vec<GpuFire>>);

fn prepare_fires(mut fires: ResMut<GridFires>, device: Res<RenderDevice>, queue: Res<RenderQueue>) {
    let mut list = fires.0.clone();
    if list.is_empty() {
        list.push(GpuFire::default());
    }
    *fires.1.get_mut() = list;
    fires.1.write_buffer(&device, &queue);
}

#[allow(clippy::too_many_arguments)]
fn draw(
    view: ViewQuery<(
        &ViewTarget,
        &ViewDepthStencilTexture,
        &ViewPipelines,
        &ViewGrid,
        &GridTextures,
        &MeshViewBindGroup,
        &Msaa,
        Option<&ViewShadowBindings>,
        Option<&solari::SolariFog>,
        Option<&bevy::render::view::ViewUniformOffset>,
        &FroxelFog,
    )>,
    cache: Res<PipelineCache>,
    pipelines: Option<Res<Pipelines>>,
    volumes: Res<Volumes>,
    fires: Res<GridFires>,
    (solari_pipelines, scene, view_uniforms): (Option<Res<solari::SolariFogPipelines>>, Option<Res<bevy::solari::scene::RaytracingSceneBindings>>, Res<bevy::render::view::ViewUniforms>),
    mut ctx: RenderContext,
) {
    let Some(p) = pipelines else { return };
    let (target, depth, ids, grid, tex, view_group, msaa, shadows, solari_fog, view_offset, fog) = view.into_inner();
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
    // the channel fog_skip=pass,... (stats.on, or RAEA_FOG_SKIP): those passes not drawn (timing their cost)
    let skip = raea_io_trace::stat_value("RAEA_FOG_SKIP").unwrap_or_default();
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

    // 1b. under Solari, what its emissive meshes and the sky scatter at each cell (froxel/solari.rs), which the
    // integrate pass then adds to the atlas's light
    let with_solari = if skip.contains("solari") {
        None
    } else {
        solari::draw(&mut ctx, &cache, solari_pipelines.as_deref(), scene.as_deref(), &view_uniforms, solari_fog, view_offset, params.clone(), vols.clone(), &tex.atlas.default_view, &tex.grid)
    };

    // 2. each column front to back
    let (integrate, group) = with_solari.unwrap_or_else(|| {
        (integrate, device.create_bind_group("froxel_integrate", &cache.get_bind_group_layout(&p.integrate_layout), &BindGroupEntries::sequential((params.clone(), &tex.atlas.default_view, &tex.grid))))
    });
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
        let group = device.create_bind_group("froxel_reference", &cache.get_bind_group_layout(&p.reference_layouts[ms as usize]), &BindGroupEntries::sequential((params, vols, fire_list, super::bevy_api::depth_binding(depth))));
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
            &BindGroupEntries::sequential((grid.buffer.binding().unwrap(), volumes.buffer.binding().unwrap(), &tex.grid, sampler, super::bevy_api::depth_binding(depth), &blocks[0].1, &blocks[1].1, &blocks[2].1, &blocks[3].1, nearest)),
        );
        let epi = [0, 1, 2, 3].map(|i| cache.get_render_pipeline(ids.epi[i]));
        match (tex.epi.as_ref(), epi) {
            (Some(t), [Some(coarse), Some(mark), Some(fine), Some(over)]) if !fog.tune.hero_exact => {
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
        &BindGroupEntries::with_indices(((0, params), (3, &tex.grid), (4, sampler), (5, super::bevy_api::depth_binding(depth)))),
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

/// A fog volume's shape inside its box (froxel_volume.wesl): how fast it thins going up, per metre from the box's
/// floor (`falloff`; 0: even); the share of the box's half-size over which its density ramps in from every face
/// (`soft`, 0 to 1; 0: a hard box); how much narrower its floor is than its top (`taper`, 0 to 1; a plume). Only the
/// froxel grid draws it (froxel.rs); Bevy's march fills the box evenly.
#[derive(Component, Clone, Copy, Default)]
pub struct FogShape {
    pub falloff: f32,
    pub soft: f32,
    pub taper: f32,
}

/// Where a fog volume comes in the order Bevy composites them (its row's id). Overlapping volumes blend in the order
/// Bevy's render world iterates them, which is the order their render entities first got a FogVolume: a render of
/// frame 59 alone and a play that reached it from frame 0 got them in different orders (as did a load that spawned
/// one more resource or readback entity), so the overlap of two volumes differed between them. order_fog_volumes
/// re-inserts them in this order every frame, so it's the same whatever came before.
#[derive(Component, Clone)]
pub struct FogRank(pub String);
