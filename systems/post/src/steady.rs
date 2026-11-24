//! raea's temporal filter (docs/realism.feature LT12). Each frame is drawn from a slightly different place inside its
//! pixels and blended with the frames before it, moved by the depth and motion prepasses, so what one frame can only
//! draw as a pattern of whole pixels averages over frames to its true share: the levels' cross-fade dither (which moves
//! to another of its 16 places each frame under it: raea's bevy_pbr pbr_functions.wesl), the far city's roof slopes a
//! pixel wide, the far sun map's shimmer. A camera with `Steady` draws with MSAA off and its depth and motion prepasses
//! (on Berlin's fly that was faster than MSAA 4x: 42 ms against 53).
//!
//! The past frame is fetched where the pixel's surface was (the motion of its nearest neighbour, so an edge moves with
//! what's in front of it), kept inside the spread of the pixel's 3x3 neighbourhood this frame (in YCoCg, so a surface
//! that moved away or came into view leaves no trail), and blended by how long the pixel has been in view and how fast
//! it moves: in view for n frames it takes 1/n of this frame, down to 1/RAEA_STEADY_STILL (32) when still and to
//! 1/RAEA_STEADY_MOVING (8) at RAEA_STEADY_PX (4) pixels a frame or more. Bevy's TAA takes 1/10 whenever anything moves
//! at all, so on the fly its far roofs and the dither bands kept a pattern.

use std::collections::HashMap;

use bevy::{
    core_pipeline::{
        prepass::{DepthPrepass, MotionVectorPrepass, ViewPrepassTextures},
        schedule::Core3d,
        Core3dSystems, FullscreenShader,
    },
    diagnostic::FrameCount,
    prelude::*,
    render::{
        camera::{ExtractedCamera, TemporalJitter},
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_resource::{
            binding_types::{sampler, texture_2d, texture_depth_2d, uniform_buffer_sized},
            *,
        },
        renderer::{RenderContext, RenderDevice, ViewQuery},
        texture::{CachedTexture, TextureCache},
        view::ViewTarget,
        Render, RenderApp, RenderStartup, RenderSystems,
    },
};

/// A camera whose frames are steadied over time (<AntiAliasing mode="steady">, or RAEA_STEADY on every camera). Brings
/// the jitter and the prepasses it reads. Its rates (<AntiAliasing steady>, docs/nobuild.feature NB1a): the frames a
/// still pixel's history averages, a moving one's, and the motion in pixels at which a pixel counts as moving.
#[derive(Component, Clone, Copy, PartialEq, Debug, ExtractComponent)]
#[extract_app(bevy::render::RenderApp)]
#[require(TemporalJitter, DepthPrepass, MotionVectorPrepass)]
pub struct Steady {
    pub still: f32,
    pub moving: f32,
    pub px: f32,
}

impl Default for Steady {
    fn default() -> Self {
        Self { still: 32.0, moving: 8.0, px: 4.0 }
    }
}

impl Steady {
    /// the camera row's rates (`aa_steady_*`), each RAEA_STEADY_* variable over its prop
    pub fn of(vals: &std::collections::BTreeMap<String, raea_core_program::ir::V>) -> Self {
        let d = Self::default();
        let at = |col: &str, var: &str, d: f32| env(var).unwrap_or_else(|| raea_sys_frame::num(vals, col, d)).max(f32::MIN_POSITIVE);
        Self { still: at("aa_steady_still", "RAEA_STEADY_STILL", d.still), moving: at("aa_steady_moving", "RAEA_STEADY_MOVING", d.moving), px: at("aa_steady_px", "RAEA_STEADY_PX", d.px) }
    }
}

/// the history's format: this frame blended with those before (rgb, linear), and how many frames the pixel
/// has been in view (a)
const PAST: TextureFormat = TextureFormat::Rgba16Float;

pub fn install(app: &mut App) {
    bevy::asset::embedded_asset!(app, "steady.wesl");
    app.add_plugins(ExtractComponentPlugin::<Steady>::default());
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    render_app.add_systems(RenderStartup, init_pipeline);
    render_app.add_systems(Render, (jitter.in_set(RenderSystems::PrepareViews), textures.in_set(RenderSystems::PrepareResources)));
    render_app.add_systems(Core3d, draw.in_set(Core3dSystems::EarlyPostProcess));
}

/// the i-th of the Halton sequence in base b, in 0-1 (a jitter's places; raea-render-raytrace's upscale uses it too)
pub fn halton(mut i: u32, b: u32) -> f32 {
    let (mut f, mut r) = (1.0, 0.0);
    while i > 0 {
        f /= b as f32;
        r += f * (i % b) as f32;
        i /= b;
    }
    r
}

/// Each frame's place inside the pixel: 16 of the Halton (2, 3) sequence, one for each of the dither's 16 places
fn jitter(frame: Res<FrameCount>, mut q: Query<&mut TemporalJitter, With<Steady>>) {
    let i = frame.0 % 16 + 1;
    for mut j in &mut q {
        j.offset = Vec2::new(halton(i, 2), halton(i, 3)) - 0.5;
    }
}

/// a view's two histories (read last frame's, write this one's) and its parameters
#[derive(Component)]
struct SteadyFrame {
    read: CachedTexture,
    write: CachedTexture,
    params: Buffer,
}

fn env(name: &str) -> Option<f32> {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).filter(|v: &f32| *v > 0.0)
}

/// Each steadied view's histories, swapped each frame, and its parameters: the slowest and fastest rates a frame, the
/// motion in pixels where the fastest is reached, and whether the history starts again (a view's first frame, a new
/// size, or a cut: cut.rs).
fn textures(
    mut commands: Commands,
    mut cache: ResMut<TextureCache>,
    device: Res<RenderDevice>,
    frame: Res<FrameCount>,
    views: Query<(Entity, &ExtractedCamera, Option<&crate::cut::CameraCut>, &Steady)>,
    mut seen: Local<HashMap<Entity, UVec2>>,
) {
    for (e, camera, cut, st) in &views {
        let (still, moving, px) = (1.0 / st.still, 1.0 / st.moving, st.px);
        let Some(size) = camera.physical_target_size else { continue };
        let desc = |label: &'static str| TextureDescriptor {
            label: Some(label),
            size: Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: PAST,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        };
        let (a, b) = (cache.get(&device, desc("steady_a")), cache.get(&device, desc("steady_b")));
        let (read, write) = if frame.0 % 2 == 0 { (b, a) } else { (a, b) };
        let reset = seen.insert(e, size) != Some(size) || cut.is_some_and(|c| c.0);
        let words: Vec<u8> = [still.to_bits(), moving.to_bits(), px.to_bits(), reset as u32].iter().flat_map(|w| w.to_le_bytes()).collect();
        let params = device.create_buffer_with_data(&BufferInitDescriptor { label: Some("steady_params"), contents: &words, usage: BufferUsages::UNIFORM });
        commands.entity(e).insert(SteadyFrame { read, write, params });
    }
}

#[derive(Resource)]
struct SteadyPipeline {
    layout: BindGroupLayoutDescriptor,
    linear: Sampler,
    /// one pipeline a view target format: an HDR camera's (Rgba16Float) and an LDR one's
    hdr: CachedRenderPipelineId,
    ldr: CachedRenderPipelineId,
}

fn init_pipeline(mut commands: Commands, device: Res<RenderDevice>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>, cache: Res<PipelineCache>) {
    let layout = BindGroupLayoutDescriptor::new(
        "steady_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                // this frame, the history, the motion prepass, the depth prepass
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_depth_2d(),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer_sized(false, core::num::NonZero::new(16)),
            ),
        ),
    );
    let shader = assets.load("embedded://raea_sys_post/steady.wesl");
    let pipeline = |format: TextureFormat| {
        let target = |format: TextureFormat| Some(ColorTargetState::from(format));
        cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("steady".into()),
            layout: vec![layout.clone()],
            vertex: fullscreen.to_vertex_state(),
            fragment: Some(FragmentState { shader: shader.clone(), targets: vec![target(format), target(PAST)], ..default() }),
            ..default()
        })
    };
    let (hdr, ldr) = (pipeline(super::bevy_api::HDR_FORMAT), pipeline(TextureFormat::Rgba8UnormSrgb));
    let linear = device.create_sampler(&SamplerDescriptor { mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear, ..default() });
    commands.insert_resource(SteadyPipeline { layout, linear, hdr, ldr });
}

fn draw(
    view: ViewQuery<(&ExtractedCamera, &ViewTarget, &ViewPrepassTextures, &SteadyFrame)>,
    pipeline: Option<Res<SteadyPipeline>>,
    cache: Res<PipelineCache>,
    mut ctx: RenderContext,
) {
    let Some(p) = pipeline else { return };
    let (camera, target, prepass, f) = view.into_inner();
    let id = if target.main_texture_format() == super::bevy_api::HDR_FORMAT { p.hdr } else { p.ldr };
    let (Some(rp), Some(motion), Some(depth)) = (cache.get_render_pipeline(id), prepass.motion_vectors_view(), prepass.depth_only_view()) else {
        return;
    };
    let post = target.post_process_write();
    let group = ctx.render_device().create_bind_group(
        "steady_group",
        &cache.get_bind_group_layout(&p.layout),
        &BindGroupEntries::sequential((post.source, &f.read.default_view, motion, depth, &p.linear, f.params.as_entire_binding())),
    );
    let attach = |view| Some(RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: Operations::default() });
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("steady"),
        color_attachments: &[attach(post.destination), attach(&f.write.default_view)],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(rp);
    pass.set_bind_group(0, &group, &[]);
    if let Some(v) = camera.viewport.as_ref() {
        pass.set_viewport(v.physical_position.x as f32, v.physical_position.y as f32, v.physical_size.x as f32, v.physical_size.y as f32, v.depth.start, v.depth.end);
    }
    pass.draw(0..3, 0..1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_jitter_takes_16_places_spread_over_the_pixel() {
        let places: Vec<Vec2> = (1..=16).map(|i| Vec2::new(halton(i, 2), halton(i, 3)) - 0.5).collect();
        assert!(places.iter().all(|p| p.x.abs() < 0.5 && p.y.abs() < 0.5));
        // every quarter of the pixel gets 4 of them
        for q in 0..4 {
            let n = places.iter().filter(|p| (p.x >= 0.0) == (q & 1 == 1) && (p.y >= 0.0) == (q & 2 == 2)).count();
            assert!((3..=5).contains(&n), "quarter {q}: {n}");
        }
    }
}
