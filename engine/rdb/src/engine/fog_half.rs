//! Volumetric fog at a fraction of the frame's resolution (`<VolumetricFog resolution={0.5}>`: vfog_resolution on the
//! camera row). Bevy's fog node marches every pixel and blends straight onto the frame; its module is private, so
//! this is a copy of its prepare and draw (bevy_pbr 0.19.1 volumetric_fog/render.rs, MIT/Apache-2.0) that marches
//! into a texture 1/k the size each way and puts that over the frame, in three passes:
//!
//! 1. downsample: each texel's depth from its k x k block of the frame's depth (nearest and farthest on alternate
//!    texels of a checkerboard), fog_half.wgsl;
//! 2. march: Bevy's fog shader (volumetric_fog.wgsl, raea's copy) with FOG_HALF, each fog volume's hull drawn at 1/k
//!    and blended as Bevy blends it, its alpha keeping what of the scene behind still shows;
//! 3. composite: each pixel the texels nearest it, weighted by how alike their depth is to the pixel's (a joint
//!    bilateral upsample), so a column against a shaft takes no fog from the wall behind it; blended onto the frame
//!    as scene x alpha + light, which is what Bevy's blend works out to.
//!
//! A camera with it carries `HalfFog` instead of Bevy's `VolumetricFog`, so Bevy's own fog systems pass it by; the
//! fog volumes and volumetric lights are Bevy's, extracted by Bevy as before. No history: each frame's fog is a
//! function of that frame (render equals play), dithered or not.

use bevy::{
    camera::Camera3d,
    core_pipeline::{schedule::Core3d, Core3dSystems, FullscreenShader},
    light::{FogVolume, VolumetricFog},
    math::{vec4, Affine3A, Vec3A},
    mesh::{MeshVertexBufferLayoutRef, Meshable},
    pbr::{MeshPipelineViewLayoutKey, MeshPipelineViewLayouts, MeshViewBindGroup, ViewKeyCache},
    prelude::*,
    shader::ShaderDefVal,
    render::{
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        mesh::{allocator::MeshAllocator, RenderMesh, RenderMeshBufferInfo},
        render_asset::RenderAssets,
        render_resource::{
            binding_types::{sampler, texture_2d, texture_3d, texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        texture::{CachedTexture, GpuImage, TextureCache},
        view::{ExtractedView, ViewDepthTexture, ViewTarget},
        Render, RenderApp, RenderStartup, RenderSystems,
    },
};

/// The camera's volumetric fog, marched at 1/`factor` of the frame each way (2 or more; 1 is Bevy's VolumetricFog).
#[derive(Component, Clone, Copy, ExtractComponent)]
pub(super) struct HalfFog {
    pub fog: VolumetricFog,
    pub factor: u32,
}

/// A camera's fog at `resolution` (a fraction of the frame, 1 / a whole number): Bevy's fog when it's 1, else this.
pub(super) fn insert(world: &mut World, e: Entity, fog: VolumetricFog, resolution: f32) {
    let factor = (1.0 / resolution.clamp(0.25, 1.0)).round() as u32;
    if factor <= 1 {
        world.entity_mut(e).insert(fog);
        return;
    }
    // the march reads the frame's depth (Bevy's fog asks for this itself, from its own component)
    if let Some(mut cam) = world.get_mut::<Camera3d>(e) {
        cam.depth_texture_usages.0 |= TextureUsages::TEXTURE_BINDING.bits();
    }
    world.entity_mut(e).insert(HalfFog { fog, factor });
}

pub(super) fn install(app: &mut App) {
    bevy::asset::embedded_asset!(app, "fog_half.wgsl");
    let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
    let plane = meshes.add(Plane3d::new(Vec3::Z, Vec2::ONE).mesh());
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0).mesh());
    app.add_plugins(ExtractComponentPlugin::<HalfFog>::default());
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    render_app
        .insert_resource(HullMeshes { plane, cube })
        .init_resource::<FogUniforms>()
        .add_systems(RenderStartup, init_pipelines.after(bevy::pbr::MeshPipelineSystems))
        .add_systems(
            Render,
            (
                (prepare_pipelines, prepare_uniforms).in_set(RenderSystems::Prepare),
                prepare_textures.in_set(RenderSystems::PrepareResources),
            ),
        )
        .add_systems(Core3d, draw.after(Core3dSystems::MainPass).before(Core3dSystems::EarlyPostProcess));
}

#[derive(Resource)]
struct HullMeshes {
    plane: Handle<Mesh>,
    cube: Handle<Mesh>,
}

/// Bevy's VolumetricFogUniform, field for field (the shader's `VolumetricFog`).
#[derive(ShaderType)]
struct FogUniform {
    clip_from_local: Mat4,
    uvw_from_world: Mat4,
    far_planes: [Vec4; 6],
    fog_color: Vec3,
    light_tint: Vec3,
    ambient_color: Vec3,
    ambient_intensity: f32,
    step_count: u32,
    bounding_radius: f32,
    absorption: f32,
    scattering: f32,
    density: f32,
    density_texture_offset: Vec3,
    scattering_asymmetry: f32,
    light_intensity: f32,
    jitter_strength: f32,
}

#[derive(Resource, Default)]
struct FogUniforms(DynamicUniformBuffer<FogUniform>);

static UVW_FROM_LOCAL: Mat4 = Mat4::from_cols(vec4(1.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(0.5, 0.5, 0.5, 1.0));

/// The march's layouts and the two fullscreen passes' (downsample, composite), each with and without MSAA.
#[derive(Resource)]
struct Pipelines {
    view_layouts: MeshPipelineViewLayouts,
    /// [plain, with a density texture]
    march_layouts: [BindGroupLayoutDescriptor; 2],
    /// [single-sampled, multisampled]
    down_layouts: [BindGroupLayoutDescriptor; 2],
    comp_layouts: [BindGroupLayoutDescriptor; 2],
    fog_shader: Handle<Shader>,
    pass_shader: Handle<Shader>,
    fullscreen: VertexState,
}

fn init_pipelines(mut commands: Commands, view_layouts: Res<MeshPipelineViewLayouts>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>) {
    let march = |density: bool| {
        let mut e = BindGroupLayoutEntries::single(ShaderStages::VERTEX_FRAGMENT, uniform_buffer::<FogUniform>(true)).to_vec();
        e.extend_from_slice(&BindGroupLayoutEntries::with_indices(ShaderStages::FRAGMENT, ((1, texture_2d(TextureSampleType::Float { filterable: false })),)));
        if density {
            e.extend_from_slice(&BindGroupLayoutEntries::with_indices(
                ShaderStages::FRAGMENT,
                ((2, texture_3d(TextureSampleType::Float { filterable: true })), (3, sampler(SamplerBindingType::Filtering))),
            ));
        }
        BindGroupLayoutDescriptor::new("half_fog_march_layout", &e)
    };
    let depth = |ms: bool| if ms { texture_depth_2d_multisampled() } else { texture_depth_2d() };
    let down = |ms: bool| BindGroupLayoutDescriptor::new("half_fog_down_layout", &BindGroupLayoutEntries::with_indices(ShaderStages::FRAGMENT, ((2, depth(ms)),)));
    let comp = |ms: bool| {
        let t = || texture_2d(TextureSampleType::Float { filterable: false });
        BindGroupLayoutDescriptor::new("half_fog_composite_layout", &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (t(), t(), depth(ms))))
    };
    commands.insert_resource(Pipelines {
        view_layouts: view_layouts.clone(),
        march_layouts: [march(false), march(true)],
        down_layouts: [down(false), down(true)],
        comp_layouts: [comp(false), comp(true)],
        // raea's copy of Bevy's fog shader, registered over the embedded one (scene3d::install_fog_shader)
        fog_shader: assets.load("embedded://bevy_pbr/volumetric_fog/volumetric_fog.wgsl"),
        pass_shader: assets.load("embedded://rdb/engine/fog_half.wgsl"),
        fullscreen: fullscreen.to_vertex_state(),
    });
}

#[derive(PartialEq, Eq, Hash, Clone)]
struct MarchKey {
    view_key: MeshPipelineViewLayoutKey,
    vertex_layout: MeshVertexBufferLayoutRef,
    density: bool,
    factor: u32,
}

/// The march: Bevy's fog pipeline into the 1/k texture, its alpha now what of the scene still shows.
fn march_descriptor(p: &Pipelines, key: &MarchKey) -> RenderPipelineDescriptor {
    let mut defs: Vec<ShaderDefVal> = vec!["SHADOW_FILTER_METHOD_HARDWARE_2X2".into(), "FOG_HALF".into(), ShaderDefVal::UInt("FOG_FACTOR".into(), key.factor)];
    if key.view_key.contains(MeshPipelineViewLayoutKey::MULTISAMPLED) {
        defs.push("MULTISAMPLED".into());
    }
    if key.view_key.contains(MeshPipelineViewLayoutKey::ATMOSPHERE) {
        defs.push("ATMOSPHERE".into());
    }
    if key.density {
        defs.push("DENSITY_TEXTURE".into());
    }
    let vertex_format = key.vertex_layout.0.get_layout(&[Mesh::ATTRIBUTE_POSITION.at_shader_location(0)]).expect("fog hull's vertex layout");
    RenderPipelineDescriptor {
        label: Some("half_fog_march".into()),
        layout: vec![p.view_layouts.get_view_layout(key.view_key).main_layout, p.march_layouts[key.density as usize].clone()],
        vertex: VertexState { shader: p.fog_shader.clone(), shader_defs: defs.clone(), buffers: vec![vertex_format], ..default() },
        primitive: PrimitiveState { cull_mode: Some(Face::Back), ..default() },
        fragment: Some(FragmentState {
            shader: p.fog_shader.clone(),
            shader_defs: defs,
            targets: vec![Some(ColorTargetState {
                format: TextureFormat::Rgba16Float,
                // colour as Bevy blends it; alpha (cleared to 1) multiplied by each volume's 1 - alpha
                blend: Some(BlendState {
                    color: BlendComponent { src_factor: BlendFactor::One, dst_factor: BlendFactor::OneMinusSrcAlpha, operation: BlendOperation::Add },
                    alpha: BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::OneMinusSrcAlpha, operation: BlendOperation::Add },
                }),
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        ..default()
    }
}

/// The downsample (target None) or the composite (target the frame's format).
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
struct PassKey {
    ms: bool,
    factor: u32,
    target: Option<TextureFormat>,
}

fn pass_descriptor(p: &Pipelines, key: PassKey) -> RenderPipelineDescriptor {
    let mut defs: Vec<ShaderDefVal> = vec![ShaderDefVal::UInt("FOG_FACTOR".into(), key.factor)];
    if key.ms {
        defs.push("MULTISAMPLED".into());
    }
    let (label, layout, entry, target) = match key.target {
        None => ("half_fog_downsample", p.down_layouts[key.ms as usize].clone(), "downsample", ColorTargetState { format: TextureFormat::R32Float, blend: None, write_mask: ColorWrites::ALL }),
        Some(format) => (
            "half_fog_composite",
            p.comp_layouts[key.ms as usize].clone(),
            "composite",
            ColorTargetState {
                format,
                // the frame x the fog's alpha + the fog's light
                blend: Some(BlendState {
                    color: BlendComponent { src_factor: BlendFactor::One, dst_factor: BlendFactor::SrcAlpha, operation: BlendOperation::Add },
                    alpha: BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::One, operation: BlendOperation::Add },
                }),
                write_mask: ColorWrites::ALL,
            },
        ),
    };
    RenderPipelineDescriptor {
        label: Some(label.into()),
        layout: vec![layout],
        vertex: p.fullscreen.clone(),
        fragment: Some(FragmentState { shader: p.pass_shader.clone(), shader_defs: defs, entry_point: Some(entry.into()), targets: vec![Some(target)] }),
        ..default()
    }
}

/// A view's pipelines: [march plain, march with density], downsample, composite.
#[derive(Component)]
struct ViewPipelines {
    march: [CachedRenderPipelineId; 2],
    down: CachedRenderPipelineId,
    comp: CachedRenderPipelineId,
}

/// Pipelines queued once a key.
#[derive(Resource, Default)]
struct PipelineIds {
    march: bevy::platform::collections::HashMap<MarchKey, CachedRenderPipelineId>,
    pass: bevy::platform::collections::HashMap<PassKey, CachedRenderPipelineId>,
}

fn prepare_pipelines(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    pipelines: Option<Res<Pipelines>>,
    hulls: Res<HullMeshes>,
    meshes: Res<RenderAssets<RenderMesh>>,
    view_keys: Res<ViewKeyCache>,
    mut ids: Local<PipelineIds>,
    views: Query<(Entity, &ExtractedView, &HalfFog, &Msaa)>,
) {
    let (Some(p), Some(plane)) = (pipelines, meshes.get(&hulls.plane)) else { return };
    for (e, view, half, msaa) in &views {
        let Some(mesh_key) = view_keys.get(&view.retained_view_entity) else { continue };
        let view_key: MeshPipelineViewLayoutKey = (*mesh_key).into();
        let ms = !matches!(msaa, Msaa::Off);
        let mut march = |density: bool| {
            let key = MarchKey { view_key, vertex_layout: plane.layout.clone(), density, factor: half.factor };
            *ids.march.entry(key.clone()).or_insert_with(|| cache.queue_render_pipeline(march_descriptor(&p, &key)))
        };
        let m = [march(false), march(true)];
        let mut pass = |target: Option<TextureFormat>| {
            let key = PassKey { ms, factor: half.factor, target };
            *ids.pass.entry(key).or_insert_with(|| cache.queue_render_pipeline(pass_descriptor(&p, key)))
        };
        let down = pass(None);
        let comp = pass(Some(view.target_format));
        commands.entity(e).insert(ViewPipelines { march: m, down, comp });
    }
}

/// One fog volume as a view sees it (Bevy's ViewFogVolume).
struct ViewVolume {
    density_texture: Option<AssetId<Image>>,
    offset: u32,
    exterior: bool,
}

#[derive(Component)]
struct ViewVolumes(Vec<ViewVolume>);

/// Bevy's prepare_volumetric_fog_uniforms, for the cameras with HalfFog. The fog volumes come in the order the
/// render world holds them (scene3d::order_fog_volumes puts them in rank order before Prepare).
fn prepare_uniforms(
    mut commands: Commands,
    mut buffer: ResMut<FogUniforms>,
    views: Query<(Entity, &ExtractedView, &HalfFog)>,
    volumes: Query<(&FogVolume, &GlobalTransform)>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let local_from_world: Vec<Affine3A> = volumes.iter().map(|(_, t)| t.affine().inverse()).collect();
    let count = views.iter().len() * local_from_world.len();
    let Some(mut writer) = buffer.0.get_writer(count, &device, &queue) else {
        for (e, ..) in &views {
            commands.entity(e).insert(ViewVolumes(Vec::new()));
        }
        return;
    };
    for (e, view, half) in &views {
        let fog = &half.fog;
        let world_from_view = view.world_from_view.affine();
        let mut list = Vec::new();
        for ((volume, _), local_from_world) in volumes.iter().zip(&local_from_world) {
            let local_from_view = *local_from_world * world_from_view;
            let view_from_local = local_from_view.inverse();
            let interior = local_from_view.translation.abs().cmple(Vec3A::splat(0.5)).all();
            let clip_from_local = hull_clip_from_local(interior, &view.clip_from_view, &view_from_local);
            let offset = writer.write(&FogUniform {
                clip_from_local,
                uvw_from_world: UVW_FROM_LOCAL * *local_from_world,
                far_planes: far_planes(&view_from_local),
                fog_color: volume.fog_color.to_linear().to_vec3(),
                light_tint: volume.light_tint.to_linear().to_vec3(),
                ambient_color: fog.ambient_color.to_linear().to_vec3(),
                ambient_intensity: fog.ambient_intensity,
                step_count: fog.step_count,
                bounding_radius: view_from_local.transform_vector3a(Vec3A::splat(0.5)).length(),
                absorption: volume.absorption,
                scattering: volume.scattering,
                density: volume.density_factor,
                density_texture_offset: volume.density_texture_offset,
                scattering_asymmetry: volume.scattering_asymmetry,
                light_intensity: volume.light_intensity,
                jitter_strength: fog.jitter,
            });
            list.push(ViewVolume { density_texture: volume.density_texture.as_ref().map(Handle::id), offset, exterior: !interior });
        }
        commands.entity(e).insert(ViewVolumes(list));
    }
}

/// Bevy's get_far_planes: the view-space planes of the volume's faces that look away from the camera.
fn far_planes(view_from_local: &Affine3A) -> [Vec4; 6] {
    let (mut planes, mut n) = ([Vec4::ZERO; 6], 0);
    for normal in [Vec3A::X, Vec3A::NEG_X, Vec3A::Y, Vec3A::NEG_Y, Vec3A::Z, Vec3A::NEG_Z] {
        let view_normal = view_from_local.transform_vector3a(normal).normalize_or_zero();
        let at = view_from_local.transform_point3a(-normal * 0.5);
        let plane = view_normal.extend(-view_normal.dot(at));
        if plane.w <= 0.0 {
            continue;
        }
        planes[n] = plane;
        n += 1;
    }
    planes
}

/// Bevy's calculate_fog_volume_clip_from_local_transforms: the cube's, or from inside it a quad on the near plane.
fn hull_clip_from_local(interior: bool, clip_from_view: &Mat4, view_from_local: &Affine3A) -> Mat4 {
    if !interior {
        return *clip_from_view * Mat4::from(*view_from_local);
    }
    let z_near = clip_from_view.w_axis[2];
    Mat4::from_cols(vec4(z_near, 0.0, 0.0, 0.0), vec4(0.0, z_near, 0.0, 0.0), vec4(0.0, 0.0, 0.0, 0.0), vec4(0.0, 0.0, z_near, z_near))
}

/// The view's two 1/k textures: the fog (light and what shows through) and each texel's depth.
#[derive(Component)]
struct HalfTextures {
    fog: CachedTexture,
    depth: CachedTexture,
    /// the frame's size over k, unrounded: the march's viewport, so texel i's centre is pixel (i + 0.5) k
    viewport: Vec2,
}

fn prepare_textures(mut commands: Commands, mut cache: ResMut<TextureCache>, device: Res<RenderDevice>, views: Query<(Entity, &ViewDepthTexture, &HalfFog)>) {
    for (e, depth, half) in &views {
        let full = depth.texture.size();
        let k = half.factor;
        let size = Extent3d { width: full.width.div_ceil(k), height: full.height.div_ceil(k), depth_or_array_layers: 1 };
        let mut make = |label: &'static str, format: TextureFormat| {
            cache.get(
                &device,
                TextureDescriptor {
                    label: Some(label),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format,
                    usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
            )
        };
        let fog = make("half_fog", TextureFormat::Rgba16Float);
        let d = make("half_fog_depth", TextureFormat::R32Float);
        commands.entity(e).insert(HalfTextures { fog, depth: d, viewport: Vec2::new(full.width as f32, full.height as f32) / k as f32 });
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    view: ViewQuery<(&ViewTarget, &ViewDepthTexture, &ViewPipelines, &ViewVolumes, &HalfTextures, &MeshViewBindGroup, &Msaa)>,
    cache: Res<PipelineCache>,
    pipelines: Option<Res<Pipelines>>,
    uniforms: Res<FogUniforms>,
    images: Res<RenderAssets<GpuImage>>,
    allocator: Res<MeshAllocator>,
    hulls: Res<HullMeshes>,
    meshes: Res<RenderAssets<RenderMesh>>,
    mut ctx: RenderContext,
) {
    let Some(p) = pipelines else { return };
    let (target, depth, ids, volumes, tex, view_group, msaa) = view.into_inner();
    if volumes.0.is_empty() {
        return;
    }
    let (Some(plain), Some(textured), Some(down), Some(comp), Some(binding)) = (
        cache.get_render_pipeline(ids.march[0]),
        cache.get_render_pipeline(ids.march[1]),
        cache.get_render_pipeline(ids.down),
        cache.get_render_pipeline(ids.comp),
        uniforms.0.binding(),
    ) else {
        return;
    };
    let ms = !matches!(msaa, Msaa::Off);
    let device = ctx.render_device().clone();
    ctx.command_encoder().push_debug_group("half_fog");

    // 1. each texel's depth
    let group = device.create_bind_group("half_fog_down", &cache.get_bind_group_layout(&p.down_layouts[ms as usize]), &BindGroupEntries::with_indices(((2, depth.view()),)));
    {
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some("half_fog_downsample"),
            color_attachments: &[Some(RenderPassColorAttachment { view: &tex.depth.default_view, depth_slice: None, resolve_target: None, ops: Operations { load: LoadOp::Clear(Default::default()), store: StoreOp::Store } })],
            ..default()
        });
        pass.set_pipeline(down);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    // 2. the march, volume by volume, into the 1/k texture (cleared to no light, everything showing through)
    let clear = wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    let mut first = true;
    for volume in &volumes.0 {
        let handle = if volume.exterior { &hulls.cube } else { &hulls.plane };
        let (Some(vertices), Some(mesh)) = (allocator.mesh_vertex_slice(&handle.id()), meshes.get(handle)) else { continue };
        let density = volume.density_texture.and_then(|t| images.get(t));
        let mut entries = DynamicBindGroupEntries::sequential((binding.clone(), BindingResource::TextureView(&tex.depth.default_view)));
        if let Some(d) = density {
            entries = entries.extend_sequential((BindingResource::TextureView(&d.texture_view), BindingResource::Sampler(&d.sampler)));
        }
        let group = device.create_bind_group(None, &cache.get_bind_group_layout(&p.march_layouts[density.is_some() as usize]), &entries);
        let load = if first { LoadOp::Clear(clear) } else { LoadOp::Load };
        first = false;
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some("half_fog_march"),
            color_attachments: &[Some(RenderPassColorAttachment { view: &tex.fog.default_view, depth_slice: None, resolve_target: None, ops: Operations { load, store: StoreOp::Store } })],
            ..default()
        });
        pass.set_viewport(0.0, 0.0, tex.viewport.x, tex.viewport.y, 0.0, 1.0);
        pass.set_vertex_buffer(0, *vertices.buffer.slice(..));
        pass.set_pipeline(if density.is_some() { textured } else { plain });
        pass.set_bind_group(0, &view_group.main, &view_group.main_offsets);
        pass.set_bind_group(1, &group, &[volume.offset]);
        match &mesh.buffer_info {
            RenderMeshBufferInfo::Indexed { index_format, count } => {
                let Some(indices) = allocator.mesh_index_slice(&handle.id()) else { continue };
                pass.set_index_buffer(*indices.buffer.slice(..), *index_format);
                pass.draw_indexed(indices.range.start..(indices.range.start + count), vertices.range.start as i32, 0..1);
            }
            RenderMeshBufferInfo::NonIndexed => pass.draw(vertices.range.clone(), 0..1),
        }
    }
    if first {
        ctx.command_encoder().pop_debug_group();
        return;
    }

    // 3. over the frame, guided by depth
    let group = device.create_bind_group(
        "half_fog_composite",
        &cache.get_bind_group_layout(&p.comp_layouts[ms as usize]),
        &BindGroupEntries::sequential((&tex.fog.default_view, &tex.depth.default_view, depth.view())),
    );
    {
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some("half_fog_composite"),
            color_attachments: &[Some(RenderPassColorAttachment { view: target.main_texture_view(), depth_slice: None, resolve_target: None, ops: Operations { load: LoadOp::Load, store: StoreOp::Store } })],
            ..default()
        });
        pass.set_pipeline(comp);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
    ctx.command_encoder().pop_debug_group();
}
