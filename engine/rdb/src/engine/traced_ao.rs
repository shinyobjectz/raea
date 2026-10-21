//! Traced ambient occlusion (RAEA_TRACED_AO=reach in metres, 0.5 by default): on a camera with `<AmbientOcclusion>`,
//! Bevy's screen-space estimate is replaced by rays against the scene itself (rt.rs's TLAS): how much of each point's
//! hemisphere is open within the reach. It's written into the texture Bevy's PBR reads its SSAO from, so it shadows the
//! environment light (its diffuse, and its specular through Bevy's specular occlusion) and nothing else: the sun keeps
//! its own shadows, and the contact shadows are the scene's real geometry, not what the screen happens to show.
//! RAEA_TRACED_AO_RAYS: rays a pixel (128). On the kitchen's moving shot the back cabinet then shimmers less than with
//! no AO at all (0.92 against 1.01, warped change a frame; 64 rays 0.98, 32 rays 1.11, Bevy's SSAO 1.17), and 90 frames
//! of 1080p render in 20 s.

use bevy::{
    core_pipeline::{core_3d::main_opaque_pass_3d, prepass::ViewPrepassTextures, schedule::Core3d, Core3dSystems},
    pbr::ScreenSpaceAmbientOcclusionResources,
    prelude::*,
    render::{
        render_resource::{
            binding_types::{acceleration_structure, texture_2d, texture_depth_2d, texture_storage_2d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        view::ExtractedView,
        RenderApp, RenderStartup,
    },
};

/// The reach RAEA_TRACED_AO asks for, in metres, if any.
pub(super) fn wanted() -> Option<f32> {
    let v = std::env::var("RAEA_TRACED_AO").ok()?;
    Some(v.parse::<f32>().ok().filter(|r| *r > 0.0).unwrap_or(0.5))
}

pub(super) fn install(app: &mut App) {
    let Some(reach) = wanted() else { return };
    bevy::asset::embedded_asset!(app, "traced_ao.wgsl");
    super::rt::install(app);
    let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
    // after Bevy's SSAO (which runs before the main pass) and before the opaque pass reads its texture
    render.insert_resource(Reach(reach)).add_systems(RenderStartup, init).add_systems(Core3d, trace.in_set(Core3dSystems::MainPass).before(main_opaque_pass_3d));
}

#[derive(Resource)]
struct Reach(f32);

#[derive(Resource)]
struct Pipeline {
    shader: Handle<Shader>,
    /// by the SSAO texture's format (R16Float where it can be stored, else R32Float)
    ids: std::sync::Mutex<bevy::platform::collections::HashMap<TextureFormat, (BindGroupLayoutDescriptor, CachedComputePipelineId)>>,
}

#[derive(ShaderType, Clone, Copy)]
struct Params {
    world_from_clip: Mat4,
    /// rays a pixel, reach (mm)
    frame: UVec4,
}

fn init(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Pipeline { shader: assets.load("embedded://rdb/engine/traced_ao.wgsl"), ids: Default::default() });
}

fn trace(
    reach: Res<Reach>,
    view: ViewQuery<(&ExtractedView, &ScreenSpaceAmbientOcclusionResources, Option<&ViewPrepassTextures>)>,
    p: Option<Res<Pipeline>>,
    cache: Res<PipelineCache>,
    queue: Res<RenderQueue>,
    scene: Option<Res<super::rt::Scene>>,
    mut ctx: RenderContext,
) {
    let (Some(p), Some(scene)) = (p, scene) else { return };
    let (extracted, ssao, prepass) = view.into_inner();
    let out = &ssao.screen_space_ambient_occlusion_texture;
    let format = out.texture.format();
    let (layout, id) = p
        .ids
        .lock()
        .unwrap()
        .entry(format)
        .or_insert_with(|| {
            let layout = BindGroupLayoutDescriptor::new(
                "raea_traced_ao_layout",
                &BindGroupLayoutEntries::sequential(
                    ShaderStages::COMPUTE,
                    (uniform_buffer::<Params>(false), texture_depth_2d(), texture_2d(TextureSampleType::Float { filterable: false }), acceleration_structure(), texture_storage_2d(format, StorageTextureAccess::WriteOnly)),
                ),
            );
            let id = cache.queue_compute_pipeline(ComputePipelineDescriptor {
                label: Some("raea_traced_ao".into()),
                layout: vec![layout.clone()],
                shader: p.shader.clone(),
                shader_defs: if format == TextureFormat::R16Float { vec!["R16FLOAT".into()] } else { vec![] },
                entry_point: Some("main".into()),
                ..default()
            });
            (layout, id)
        })
        .clone();
    let (Some(pipeline), Some(prepass)) = (cache.get_compute_pipeline(id), prepass) else { return };
    let (Some(depth), Some(normal)) = (prepass.depth_view(), prepass.normal_view()) else { return };
    let device: RenderDevice = ctx.render_device().clone();
    let rays = std::env::var("RAEA_TRACED_AO_RAYS").ok().and_then(|v| v.parse().ok()).unwrap_or(128u32);
    let mut params = UniformBuffer::from(Params { world_from_clip: extracted.world_from_view.to_matrix() * extracted.clip_from_view.inverse(), frame: UVec4::new(rays, (reach.0 * 1000.0) as u32, 0, 0) });
    params.write_buffer(&device, &queue);
    let group = device.create_bind_group("raea_traced_ao", &cache.get_bind_group_layout(&layout), &BindGroupEntries::sequential((params.binding().unwrap(), depth, normal, scene.0.as_binding(), &out.default_view)));
    let size = out.texture.size();
    let mut pass = ctx.command_encoder().begin_compute_pass(&ComputePassDescriptor { label: Some("raea_traced_ao"), timestamp_writes: None });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &group, &[]);
    pass.dispatch_workgroups(size.width.div_ceil(8), size.height.div_ceil(8), 1);
}
