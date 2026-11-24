//! raea's motion blur (docs/raytracing.feature RT29): what a camera records while its shutter is open. The longest
//! motion in each 20-pixel tile (across its rows, then down), then in each tile's 3x3 neighbourhood (shutter_tiles.wesl); each pixel gathered along
//! the longest motion near it, every tap weighted as a box shutter spreads a moving pixel's light (shutter.wesl). Bevy's
//! MotionBlur gathered each pixel along its own motion, so a still pixel beside a moving one took none of its light:
//! a ball crossing a still frame was smeared inward only, its streak 64 px where the shutter's is 86 and 8% of its
//! light gone. RAEA_MOTION_BLUR=bevy draws Bevy's still, to compare.

use bevy::{
    core_pipeline::{
        prepass::{DepthPrepass, MotionVectorPrepass, ViewPrepassTextures},
        schedule::Core3d,
        Core3dSystems, FullscreenShader,
    },
    diagnostic::FrameCount,
    post_process::bloom::bloom,
    prelude::*,
    render::{
        camera::ExtractedCamera,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_resource::{
            binding_types::{texture_2d, texture_2d_multisampled, texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer_sized},
            *,
        },
        renderer::{RenderContext, RenderDevice, ViewQuery},
        texture::{CachedTexture, TextureCache},
        view::ViewTarget,
        Render, RenderApp, RenderStartup, RenderSystems,
    },
};

use crate::cut::CameraCut;

/// A camera drawn with its shutter open for `open` of each frame, gathered by `taps` taps a pixel. Brings the depth
/// and motion prepasses it reads.
#[derive(Component, Clone, Copy, PartialEq, Debug, ExtractComponent)]
#[extract_app(bevy::render::RenderApp)]
#[require(DepthPrepass, MotionVectorPrepass)]
pub struct Shutter {
    pub open: f32,
    pub taps: u32,
}

/// a tile's side in pixels, and the farthest a pixel's light is carried each way while the shutter is open (a motion
/// longer than two tiles is drawn two tiles long)
pub const TILE: u32 = 20;
/// the tiles' format: each tile's longest motion while the shutter is open, in pixels (rg)
const TILES: TextureFormat = TextureFormat::Rgba16Float;

pub fn install(app: &mut App) {
    bevy::asset::embedded_asset!(app, "shutter.wesl");
    bevy::asset::embedded_asset!(app, "shutter_tiles.wesl");
    app.add_plugins(ExtractComponentPlugin::<Shutter>::default());
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    render_app.add_systems(RenderStartup, init_pipeline);
    render_app.add_systems(Render, textures.in_set(RenderSystems::PrepareResources));
    render_app.add_systems(Core3d, draw.before(bloom).in_set(Core3dSystems::PostProcess));
}

/// the number of tiles across a side of `px` pixels
fn tiles(px: u32) -> u32 {
    px.div_ceil(TILE).max(1)
}

/// a view's tiles (each tile row's longest motion, each tile's, then each neighbourhood's) and its parameters
#[derive(Component)]
struct ShutterFrame {
    rows: CachedTexture,
    longest: CachedTexture,
    near: CachedTexture,
    params: Buffer,
}

fn textures(mut commands: Commands, mut cache: ResMut<TextureCache>, device: Res<RenderDevice>, frame: Res<FrameCount>, views: Query<(Entity, &ExtractedCamera, &Shutter)>) {
    for (e, camera, s) in &views {
        let Some(size) = camera.physical_target_size else { continue };
        let desc = |label: &'static str, height: u32| TextureDescriptor {
            label: Some(label),
            size: Extent3d { width: tiles(size.x), height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TILES,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        };
        let rows = cache.get(&device, desc("shutter_rows", size.y.max(1)));
        let (longest, near) = (cache.get(&device, desc("shutter_longest", tiles(size.y))), cache.get(&device, desc("shutter_near", tiles(size.y))));
        let words: Vec<u8> = [s.open.max(0.0).to_bits(), s.taps.max(2), TILE, frame.0].iter().flat_map(|w| w.to_le_bytes()).collect();
        let params = device.create_buffer_with_data(&BufferInitDescriptor { label: Some("shutter_params"), contents: &words, usage: BufferUsages::UNIFORM });
        commands.entity(e).insert(ShutterFrame { rows, longest, near, params });
    }
}

/// the passes that read the prepasses, for single-sampled prepasses or multisampled ones (MSAA)
struct Reads {
    tiles_layout: BindGroupLayoutDescriptor,
    gather_layout: BindGroupLayoutDescriptor,
    rows: CachedRenderPipelineId,
    /// one gather pipeline a view target format: an HDR camera's (Rgba16Float) and an LDR one's
    hdr: CachedRenderPipelineId,
    ldr: CachedRenderPipelineId,
}

#[derive(Resource)]
struct ShutterPipeline {
    single: Reads,
    multi: Reads,
    /// the tiles' and neighbourhoods' passes, which read only the pass before's tiles
    near_layout: BindGroupLayoutDescriptor,
    longest: CachedRenderPipelineId,
    near: CachedRenderPipelineId,
}

fn init_pipeline(mut commands: Commands, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>, cache: Res<PipelineCache>) {
    let float = TextureSampleType::Float { filterable: false };
    let params = || uniform_buffer_sized(false, core::num::NonZero::new(16));
    let tiles_shader: Handle<Shader> = assets.load("embedded://raea_sys_post/shutter_tiles.wesl");
    let shader: Handle<Shader> = assets.load("embedded://raea_sys_post/shutter.wesl");
    let pipeline = |label: &'static str, layout: &BindGroupLayoutDescriptor, shader: &Handle<Shader>, entry: &'static str, format: TextureFormat, multi: bool| {
        cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some(label.into()),
            layout: vec![layout.clone()],
            vertex: fullscreen.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: shader.clone(),
                shader_defs: if multi { vec!["MULTISAMPLED".into()] } else { vec![] },
                entry_point: Some(entry.into()),
                targets: vec![Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL })],
                ..default()
            }),
            ..default()
        })
    };
    let near_layout = BindGroupLayoutDescriptor::new("shutter_near_layout", &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (texture_2d(float), params())));
    let longest = pipeline("shutter_longest", &near_layout, &tiles_shader, "tile_max", TILES, false);
    let near = pipeline("shutter_near", &near_layout, &tiles_shader, "neighbour_max", TILES, false);
    let reads = |multi: bool| {
        let (motion, depth) = if multi { (texture_2d_multisampled(float), texture_depth_2d_multisampled()) } else { (texture_2d(float), texture_depth_2d()) };
        let tiles_layout = BindGroupLayoutDescriptor::new("shutter_tiles_layout", &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (motion, params())));
        // the frame, the motion prepass, the depth prepass, the neighbourhoods' longest motions
        let gather_layout = BindGroupLayoutDescriptor::new("shutter_layout", &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (texture_2d(float), motion, depth, texture_2d(float), params())));
        let rows = pipeline("shutter_rows", &tiles_layout, &tiles_shader, "tile_rows", TILES, multi);
        let hdr = pipeline("shutter", &gather_layout, &shader, "gather", super::bevy_api::HDR_FORMAT, multi);
        let ldr = pipeline("shutter", &gather_layout, &shader, "gather", TextureFormat::Rgba8UnormSrgb, multi);
        Reads { tiles_layout, gather_layout, rows, hdr, ldr }
    };
    commands.insert_resource(ShutterPipeline { single: reads(false), multi: reads(true), near_layout, longest, near });
}

/// one fullscreen triangle into `out`
fn pass(ctx: &mut RenderContext, label: &'static str, out: &TextureView, rp: &RenderPipeline, group: &BindGroup, viewport: Option<&bevy::camera::Viewport>) {
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(RenderPassColorAttachment { view: out, depth_slice: None, resolve_target: None, ops: Operations::default() })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(rp);
    pass.set_bind_group(0, group, &[]);
    if let Some(v) = viewport {
        pass.set_viewport(v.physical_position.x as f32, v.physical_position.y as f32, v.physical_size.x as f32, v.physical_size.y as f32, v.depth.start, v.depth.end);
    }
    pass.draw(0..3, 0..1);
}

fn draw(view: ViewQuery<(&ExtractedCamera, &ViewTarget, &ViewPrepassTextures, &ShutterFrame, &Msaa, Option<&CameraCut>)>, pipeline: Option<Res<ShutterPipeline>>, cache: Res<PipelineCache>, mut ctx: RenderContext) {
    let Some(p) = pipeline else { return };
    let (camera, target, prepass, f, msaa, cut) = view.into_inner();
    // a cut's motion is the jump, not anything the shutter saw (cut.rs)
    if cut.is_some_and(|c| c.0) {
        return;
    }
    let r = if msaa.samples() > 1 { &p.multi } else { &p.single };
    let id = if target.main_texture_format() == super::bevy_api::HDR_FORMAT { r.hdr } else { r.ldr };
    let (Some(rows), Some(longest), Some(near), Some(gather), Some(motion), Some(depth)) = (
        cache.get_render_pipeline(r.rows),
        cache.get_render_pipeline(p.longest),
        cache.get_render_pipeline(p.near),
        cache.get_render_pipeline(id),
        prepass.motion_vectors_view(),
        prepass.depth_only_view(),
    ) else {
        return;
    };
    let device = ctx.render_device().clone();
    let g = device.create_bind_group("shutter_rows_group", &cache.get_bind_group_layout(&r.tiles_layout), &BindGroupEntries::sequential((motion, f.params.as_entire_binding())));
    pass(&mut ctx, "shutter_rows", &f.rows.default_view, rows, &g, None);
    let g = device.create_bind_group("shutter_longest_group", &cache.get_bind_group_layout(&p.near_layout), &BindGroupEntries::sequential((&f.rows.default_view, f.params.as_entire_binding())));
    pass(&mut ctx, "shutter_longest", &f.longest.default_view, longest, &g, None);
    let g = device.create_bind_group("shutter_near_group", &cache.get_bind_group_layout(&p.near_layout), &BindGroupEntries::sequential((&f.longest.default_view, f.params.as_entire_binding())));
    pass(&mut ctx, "shutter_near", &f.near.default_view, near, &g, None);
    let post = target.post_process_write();
    let g = device.create_bind_group(
        "shutter_group",
        &cache.get_bind_group_layout(&r.gather_layout),
        &BindGroupEntries::sequential((post.source, motion, depth, &f.near.default_view, f.params.as_entire_binding())),
    );
    pass(&mut ctx, "shutter", post.destination, gather, &g, camera.viewport.as_ref());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_cover_the_frame() {
        assert_eq!((tiles(512), tiles(1920), tiles(1080), tiles(1)), (26, 96, 54, 1));
    }
}
