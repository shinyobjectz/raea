//! The camera's lens: @raea/motion/3d's Fisheye and LensDistortion, a fullscreen pass over the 3D after tone mapping
//! (lens.wgsl). The page (the 2D drawn over the 3D) is composited after it, so kinetic type is never bent. A lens's
//! vignette and colour fringes are <Grade>'s (scene3d.rs camera).
//!
//! Camera columns (all keyed per frame; a frame without a lens has lens_s 0, lens_k1 0, lens_k2 0):
//!   lens_mode (1 equidistant fisheye, 2 stereographic fisheye, 3 Brown-Conrady), lens_s (a fisheye's strength 0..1),
//!   lens_k1 lens_k2 (Brown-Conrady), lens_zoom, lens_fov (a fisheye's render field of view at strength 1, degrees:
//!   the camera's fov is mixed towards it by the strength), lens_black (1: black outside the picture, else its edge
//!   pixels)
//!
//! A frame whose lens does nothing (no bend, zoom 1) skips the pass, so strength 0 is the same pixels as no lens at
//! all.

use super::*;
use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::core_pipeline::{schedule::Core3d, tonemapping::tonemapping, Core3dSystems, FullscreenShader};
use bevy::render::{
    extract_component::{ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin},
    render_resource::{
        binding_types::{sampler, texture_2d, uniform_buffer},
        *,
    },
    renderer::{RenderContext, RenderDevice, ViewQuery},
    view::ViewTarget,
    RenderApp, RenderStartup,
};
use bevy::shader::Shader;

const LENS_SHADER: Handle<Shader> = uuid_handle!("2f8b1c4d-6e3a-4d7b-9a51-0c4e7f2b8d63");

/// The lens on the camera, as the shader reads it (lens.wgsl's `Lens`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, ExtractComponent, ShaderType)]
pub struct Lens {
    /// mode, strength, k1, k2
    pub a: Vec4,
    /// zoom, tan(vertical fov / 2), aspect, black outside
    pub b: Vec4,
}

impl Lens {
    /// Whether the pass would leave every pixel as it is.
    fn identity(&self) -> bool {
        let bend = match self.a.x.round() as i32 {
            1 | 2 => self.a.y != 0.0,
            3 => self.a.z != 0.0 || self.a.w != 0.0,
            _ => false,
        };
        !bend && self.b.x == 1.0
    }
}

/// Whether a camera row has a lens.
pub(super) fn has_lens(vals: &BTreeMap<String, V>) -> bool {
    vals.contains_key("lens_mode")
}

/// The lens for this frame's camera row (`fov`: the vertical field of view the frame renders at, radians).
pub(super) fn lens_of(vals: &BTreeMap<String, V>, fov: f32, aspect: f32) -> Lens {
    let g = |k: &str, d: f32| num(vals, k, d);
    let mode = g("lens_mode", 0.0);
    let (k1, k2) = (g("lens_k1", 0.0), g("lens_k2", 0.0));
    // a barrel's corners stay inside the picture unless a zoom is given
    let zoom = match vals.get("lens_zoom") {
        Some(z) => (z.num() as f32).max(0.05),
        None if mode.round() as i32 == 3 => (1.0 + k1 + k2).max(1.0),
        None => 1.0,
    };
    Lens {
        a: Vec4::new(mode, g("lens_s", 0.0).clamp(0.0, 1.0), k1, k2),
        b: Vec4::new(zoom, (fov * 0.5).tan(), aspect, g("lens_black", 0.0)),
    }
}

/// The vertical field of view a frame renders at (radians): a fisheye with its own `fov` widens the camera's towards
/// it by its strength, so strength 0 is the camera as it is.
pub(super) fn render_fov(vals: &BTreeMap<String, V>, cam_fov: f32) -> f32 {
    match vals.get("lens_fov") {
        Some(f) if matches!(num(vals, "lens_mode", 0.0).round() as i32, 1 | 2) => {
            let s = num(vals, "lens_s", 0.0).clamp(0.0, 1.0);
            let wide = (f.num() as f32).clamp(1.0, 170.0).to_radians();
            cam_fov + (wide - cam_fov) * s
        }
        _ => cam_fov,
    }
}

/// The pass, on every engine (a camera without a Lens never runs it).
pub(super) fn install(app: &mut App) {
    load_internal_asset!(app, LENS_SHADER, "lens.wgsl", Shader::from_wgsl);
    app.add_plugins((ExtractComponentPlugin::<Lens>::default(), UniformComponentPlugin::<Lens>::default()));
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    render_app.add_systems(RenderStartup, init_pipeline);
    render_app.add_systems(Core3d, lens_pass.after(tonemapping).in_set(Core3dSystems::PostProcess));
}

#[derive(Resource)]
struct LensPipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    /// one per main-texture format: an LDR camera's sRGB target, an HDR one's float target (Bloom)
    ids: Vec<(TextureFormat, CachedRenderPipelineId)>,
}

fn init_pipeline(mut commands: Commands, device: Res<RenderDevice>, fullscreen: Res<FullscreenShader>, cache: Res<PipelineCache>) {
    let layout = BindGroupLayoutDescriptor::new(
        "raea_lens",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (texture_2d(TextureSampleType::Float { filterable: true }), sampler(SamplerBindingType::Filtering), uniform_buffer::<Lens>(true)),
        ),
    );
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        ..default()
    });
    // queued at start-up, so wait_pipelines has them made before the first frame draws: a frame never shows unbent for
    // want of its pipeline
    let ids = [TextureFormat::Rgba8UnormSrgb, TextureFormat::Rgba16Float].into_iter().map(|format| {
        let id = cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("raea_lens".into()),
            layout: vec![layout.clone()],
            vertex: fullscreen.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: LENS_SHADER,
                targets: vec![Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL })],
                ..default()
            }),
            ..default()
        });
        (format, id)
    }).collect();
    commands.insert_resource(LensPipeline { layout, sampler, ids });
}

fn lens_pass(
    view: ViewQuery<(&ViewTarget, &Lens, &DynamicUniformIndex<Lens>)>,
    pipeline: Option<Res<LensPipeline>>,
    cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<Lens>>,
    mut ctx: RenderContext,
) {
    let Some(pipeline) = pipeline else { return };
    let (target, lens, index) = view.into_inner();
    if lens.identity() {
        return;
    }
    let format = target.main_texture_format();
    let Some(id) = pipeline.ids.iter().find(|(f, _)| *f == format).map(|(_, id)| *id) else {
        // a target format the pass wasn't made for: say so once rather than draw unbent in silence
        static SAID: std::sync::Once = std::sync::Once::new();
        SAID.call_once(|| eprintln!("raea lens: no pipeline for the camera's {format:?} target; the lens is off"));
        return;
    };
    let Some(render_pipeline) = cache.get_render_pipeline(id) else { return };
    let Some(binding) = uniforms.uniforms().binding() else { return };
    let post = target.post_process_write();
    let bind = ctx.render_device().create_bind_group(
        "raea_lens",
        &cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((post.source, &pipeline.sampler, binding.clone())),
    );
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("raea_lens"),
        color_attachments: &[Some(RenderPassColorAttachment { view: post.destination, depth_slice: None, resolve_target: None, ops: Operations::default() })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(render_pipeline);
    pass.set_bind_group(0, &bind, &[index.index()]);
    pass.draw(0..3, 0..1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_zero_is_no_lens_and_barrels_zoom_to_their_corners() {
        let mut v = BTreeMap::new();
        v.insert("lens_mode".to_string(), V::N(1.0));
        v.insert("lens_s".to_string(), V::N(0.0));
        v.insert("lens_fov".to_string(), V::N(120.0));
        assert!(lens_of(&v, 0.7, 16.0 / 9.0).identity());
        assert_eq!(render_fov(&v, 0.7), 0.7);
        v.insert("lens_s".to_string(), V::N(1.0));
        assert!(!lens_of(&v, 0.7, 16.0 / 9.0).identity());
        assert!((render_fov(&v, 0.7) - 120f32.to_radians()).abs() < 1e-6);
        let mut b = BTreeMap::new();
        b.insert("lens_mode".to_string(), V::N(3.0));
        b.insert("lens_k1".to_string(), V::N(0.3));
        b.insert("lens_k2".to_string(), V::N(0.1));
        assert!((lens_of(&b, 0.7, 1.0).b.x - 1.4).abs() < 1e-6);
        b.insert("lens_k1".to_string(), V::N(-0.2));
        b.insert("lens_k2".to_string(), V::N(0.0));
        assert_eq!(lens_of(&b, 0.7, 1.0).b.x, 1.0);
    }
}
