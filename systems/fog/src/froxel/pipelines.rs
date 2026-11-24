//! The froxel passes' pipelines: built once, keyed by what a view needs (light, reference, hero, apply), and queued
//! per view.

use super::*;

/// The three passes' layouts and shaders.
#[derive(Resource)]
pub struct Pipelines {
    pub view_layouts: MeshPipelineViewLayouts,
    pub light_layout: BindGroupLayoutDescriptor,
    /// the light pass's and the frame's depth, for the reference [single-sampled, multisampled]
    pub reference_layouts: [BindGroupLayoutDescriptor; 2],
    pub integrate_layout: BindGroupLayoutDescriptor,
    /// [single-sampled, multisampled]
    pub apply_layouts: [BindGroupLayoutDescriptor; 2],
    pub hero_layouts: [BindGroupLayoutDescriptor; 2],
    /// the epipolar passes' samples read back (froxel_hero.wesl's group 2)
    pub epi_layout: BindGroupLayoutDescriptor,
    /// [the 4 x 4 blocks from the shadow maps, a level's blocks from the level below's]
    pub minmax_layouts: [BindGroupLayoutDescriptor; 2],
    pub light_shader: Handle<Shader>,
    pub hero_shader: Handle<Shader>,
    pub minmax_shader: Handle<Shader>,
    pub grid_shader: Handle<Shader>,
    pub fullscreen: VertexState,
    pub sampler: Option<Sampler>,
    /// nearest, for gathering the shadow maps' texels
    pub nearest: Option<Sampler>,
}

pub fn init_pipelines(mut commands: Commands, view_layouts: Res<MeshPipelineViewLayouts>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>, device: Res<RenderDevice>) {
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
        light_shader: assets.load("embedded://raea_sys_fog/froxel_light.wesl"),
        hero_shader: assets.load("embedded://raea_sys_fog/froxel_hero.wesl"),
        minmax_shader: assets.load("embedded://raea_sys_fog/froxel_minmax.wgsl"),
        grid_shader: assets.load("embedded://raea_sys_fog/froxel_grid.wesl"),
        fullscreen: fullscreen.to_vertex_state(),
        sampler: Some(sampler),
        nearest: Some(nearest),
    });
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub enum Key {
    Light(MeshPipelineViewLayoutKey),
    /// multisampled, the target's format, its samples
    Apply(bool, TextureFormat, u32),
    /// (each hero pass also by whether it reads the transmittance tricubic: <VolumetricFog tricubic>)
    Hero(MeshPipelineViewLayoutKey, TextureFormat, u32, bool),
    /// the epipolar passes into their samples: place, coarse, mark, fine
    Epi(MeshPipelineViewLayoutKey, u8, bool),
    Epipolar(MeshPipelineViewLayoutKey, TextureFormat, u32, bool),
    Reference(MeshPipelineViewLayoutKey, TextureFormat, u32),
}

#[derive(Component)]
pub struct ViewPipelines {
    pub light: CachedRenderPipelineId,
    pub integrate: CachedComputePipelineId,
    pub apply: CachedRenderPipelineId,
    pub hero: CachedRenderPipelineId,
    /// coarse, mark, fine, then over the frame
    pub epi: [CachedRenderPipelineId; 4],
    /// [level4, reduce]
    pub minmax: [CachedComputePipelineId; 2],
    /// the ground truth instead of the grid (RAEA_FOG_REFERENCE=grid)
    pub reference: Option<CachedRenderPipelineId>,
}

#[derive(Default)]
pub struct PipelineIds {
    pub render: bevy::platform::collections::HashMap<Key, CachedRenderPipelineId>,
    pub compute: Option<CachedComputePipelineId>,
    pub minmax: Option<[CachedComputePipelineId; 2]>,
}

pub fn light_descriptor(p: &Pipelines, view_key: MeshPipelineViewLayoutKey) -> RenderPipelineDescriptor {
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
            constants: Vec::new(),
        }),
        ..default()
    }
}

/// The fog's ground truth marched per pixel (froxel_light.wesl's `reference`), blended onto the frame as the apply.
pub fn reference_descriptor(p: &Pipelines, view_key: MeshPipelineViewLayoutKey, format: TextureFormat, samples: u32) -> RenderPipelineDescriptor {
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
pub fn reference_steps() -> Option<u32> {
    (std::env::var("RAEA_FOG_REFERENCE").as_deref() == Ok("grid")).then(|| std::env::var("RAEA_FOG_REFERENCE_STEPS").ok().and_then(|v| v.parse().ok()).unwrap_or(1024).max(1))
}

/// how a hero pass draws (hero_descriptor)
#[derive(Clone, Copy, Default)]
pub struct HeroPass {
    /// added onto the frame
    pub blend: bool,
    /// blended as the grid's apply (frame x a + rgb: the grid's fog drawn with it)
    pub fog: bool,
    /// reads the epipolar samples (group 2)
    pub epi: bool,
    /// the samples' mask: written (mark), or tested (fine: drawn at depth one half where it's 0)
    pub mask: Option<bool>,
    /// the grid's transmittance read tricubic (<VolumetricFog tricubic>)
    pub tricubic: bool,
}

/// a hero pass: `entry` into `format`, which has `samples`
pub fn hero_descriptor(p: &Pipelines, view_key: MeshPipelineViewLayoutKey, entry: &'static str, format: TextureFormat, samples: u32, how: HeroPass) -> RenderPipelineDescriptor {
    let HeroPass { blend, fog, epi, mask, tricubic } = how;
    let ms = view_key.contains(MeshPipelineViewLayoutKey::MULTISAMPLED);
    let mut defs: Vec<ShaderDefVal> = vec!["SHADOW_FILTER_METHOD_HARDWARE_2X2".into()];
    if tricubic {
        defs.push("TRICUBIC_TRANSMITTANCE".into());
    }
    if raea_io_trace::stat("RAEA_FOG_SHOW_FALLBACK") {
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
        vertex: if mask == Some(false) { VertexState { shader: p.hero_shader.clone(), shader_defs: defs.clone(), entry_point: Some("at_half".into()), buffers: vec![], constants: Vec::new() } } else { p.fullscreen.clone() },
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
            constants: Vec::new(),
        }),
        multisample: MultisampleState { count: samples, ..default() },
        ..default()
    }
}

pub fn apply_descriptor(p: &Pipelines, ms: bool, format: TextureFormat, samples: u32) -> RenderPipelineDescriptor {
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
            constants: Vec::new(),
        }),
        multisample: MultisampleState { count: samples, ..default() },
        ..default()
    }
}

pub fn prepare_pipelines(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    pipelines: Option<Res<Pipelines>>,
    view_keys: Res<ViewKeyCache>,
    mut ids: Local<PipelineIds>,
    views: Query<(Entity, &ExtractedView, &Msaa, &FroxelFog)>,
) {
    let Some(p) = pipelines else { return };
    for (e, view, msaa, fog) in &views {
        let tri = fog.tune.tricubic;
        let Some(mesh_key) = view_keys.get(&view.retained_view_entity) else { continue };
        let view_key: MeshPipelineViewLayoutKey = (*mesh_key).into();
        let ms = !matches!(msaa, Msaa::Off);
        let light = *ids.render.entry(Key::Light(view_key)).or_insert_with(|| cache.queue_render_pipeline(light_descriptor(&p, view_key)));
        // drawn into the multisampled frame, which the transparent pass after them resolves over the main texture
        let samples = msaa.samples();
        let apply = *ids.render.entry(Key::Apply(ms, view.target_format, samples)).or_insert_with(|| cache.queue_render_pipeline(apply_descriptor(&p, ms, view.target_format, samples)));
        let hero = *ids.render.entry(Key::Hero(view_key, view.target_format, samples, tri)).or_insert_with(|| cache.queue_render_pipeline(hero_descriptor(&p, view_key, "hero", view.target_format, samples, HeroPass { blend: true, tricubic: tri, ..default() })));
        let mut epi = [hero; 4];
        let passes = [
            ("coarse", TextureFormat::Rgba16Float, HeroPass { tricubic: tri, ..default() }),
            ("mark", TextureFormat::Rgba16Float, HeroPass { epi: true, mask: Some(true), tricubic: tri, ..default() }),
            ("fine", TextureFormat::Rgba16Float, HeroPass { mask: Some(false), tricubic: tri, ..default() }),
        ];
        for (i, (entry, format, how)) in passes.into_iter().enumerate() {
            epi[i] = *ids.render.entry(Key::Epi(view_key, i as u8, tri)).or_insert_with(|| cache.queue_render_pipeline(hero_descriptor(&p, view_key, entry, format, 1, how)));
        }
        epi[3] = *ids.render.entry(Key::Epipolar(view_key, view.target_format, samples, tri)).or_insert_with(|| cache.queue_render_pipeline(hero_descriptor(&p, view_key, "epipolar", view.target_format, samples, HeroPass { blend: true, fog: true, epi: true, tricubic: tri, ..default() })));
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
