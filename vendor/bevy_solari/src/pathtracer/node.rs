use super::{prepare::PathtracerAccumulationTexture, Pathtracer, PathtracerMedia};
use crate::scene::RaytracingSceneBindings;
use bevy_asset::{load_embedded_asset, AssetServer};
use bevy_ecs::{prelude::*, resource::Resource, system::Commands};
use bevy_render::{
    camera::ExtractedCamera,
    render_resource::{
        binding_types::{storage_buffer_read_only_sized, texture_storage_2d, uniform_buffer},
        BindingResource, Buffer, BufferInitDescriptor, BufferUsages,
        BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
        CachedComputePipelineId, ComputePassDescriptor, ComputePipelineDescriptor,
        ImageSubresourceRange, PipelineCache, ShaderStages, StorageTextureAccess, TextureFormat,
    },
    renderer::{RenderContext, RenderDevice, ViewQuery},
    view::{ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms},
};
use bevy_utils::default;

/// Resource holding the pathtracer pipeline configuration.
#[derive(Resource)]
pub struct PathtracerPipelines {
    bind_group_layout: BindGroupLayoutDescriptor,
    pipeline: CachedComputePipelineId,
    // raea: the media when the scene has none: one volume of no extinction (froxel_volume.wesl's Volume, 128 bytes)
    no_media: Buffer,
}

/// Initializes the pathtracer pipelines at render startup.
pub fn init_pathtracer_pipelines(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    scene_bindings: Res<RaytracingSceneBindings>,
    asset_server: Res<AssetServer>,
    render_device: Res<RenderDevice>,
) {
    let bind_group_layout = BindGroupLayoutDescriptor::new(
        "pathtracer_bind_group_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                texture_storage_2d(TextureFormat::Rgba32Float, StorageTextureAccess::ReadWrite),
                texture_storage_2d(TextureFormat::Rgba16Float, StorageTextureAccess::WriteOnly),
                uniform_buffer::<ViewUniform>(true),
                storage_buffer_read_only_sized(false, None),
            ),
        ),
    );

    let pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("pathtracer_pipeline".into()),
        layout: vec![
            scene_bindings.bind_group_layout.clone(),
            bind_group_layout.clone(),
        ],
        shader: load_embedded_asset!(asset_server.as_ref(), "pathtracer.wesl"),
        ..default()
    });

    let no_media = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("pathtracer_no_media"),
        contents: &[0u8; 128],
        usage: BufferUsages::STORAGE,
    });
    commands.insert_resource(PathtracerPipelines {
        bind_group_layout,
        pipeline,
        no_media,
    });
}

pub fn pathtracer(
    view: ViewQuery<(
        &Pathtracer,
        &PathtracerAccumulationTexture,
        &ExtractedCamera,
        &ViewTarget,
        &ViewUniformOffset,
    )>,
    pathtracer_pipelines: Option<Res<PathtracerPipelines>>,
    pipeline_cache: Res<PipelineCache>,
    scene_bindings: Res<RaytracingSceneBindings>,
    view_uniforms: Res<ViewUniforms>,
    render_device: Res<RenderDevice>,
    media: Res<PathtracerMedia>,
    mut ctx: RenderContext,
) {
    let (pathtracer_settings, accumulation_texture, camera, view_target, view_uniform_offset) =
        view.into_inner();

    let Some(pathtracer_pipelines) = pathtracer_pipelines else {
        return;
    };

    let (Some(pipeline), Some(scene_bind_group), Some(viewport), Some(view_uniforms_binding)) = (
        pipeline_cache.get_compute_pipeline(pathtracer_pipelines.pipeline),
        &scene_bindings.bind_group,
        camera.physical_viewport_size,
        view_uniforms.uniforms.binding(),
    ) else {
        return;
    };

    let bind_group = render_device.create_bind_group(
        "pathtracer_bind_group",
        &pipeline_cache.get_bind_group_layout(&pathtracer_pipelines.bind_group_layout),
        &BindGroupEntries::sequential((
            &accumulation_texture.0.default_view,
            view_target.get_unsampled_color_attachment().view,
            view_uniforms_binding,
            BindingResource::Buffer(media.0.as_ref().unwrap_or(&pathtracer_pipelines.no_media).as_entire_buffer_binding()),
        )),
    );

    let command_encoder = ctx.command_encoder();

    if pathtracer_settings.reset {
        command_encoder.clear_texture(
            &accumulation_texture.0.texture,
            &ImageSubresourceRange::default(),
        );
    }

    let mut pass = command_encoder.begin_compute_pass(&ComputePassDescriptor {
        label: Some("pathtracer"),
        timestamp_writes: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, scene_bind_group, &[]);
    pass.set_bind_group(1, &bind_group, &[view_uniform_offset.offset]);
    pass.dispatch_workgroups(viewport.x.div_ceil(8), viewport.y.div_ceil(8), 1);
}
