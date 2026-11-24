//! RT9 (docs/raytracing.feature): a view drawn below its size and brought up to it, so a real-time frame traces fewer
//! pixels (photo 2's tracing costs 2.6 ms at 368x490, 8 ms at 736x981, 12 ms at 1104x1472; RT8). <SolariLighting
//! scale={s}> (RenderScale on the camera; RAEA_SOLARI_SCALE=s for every Solari camera) draws the main pass (the
//! G-buffer, Solari's tracing, MetalFX's denoiser, the forward meshes) at s of the view's size a side, in the top-left
//! corner of its textures: Bevy's MainPassResolutionOverride, as its DLSS uses. Then, where Bevy's DLSS upscales
//! (EarlyPostProcess, before the post-processing, which runs at the full size), MetalFX's temporal scaler takes that
//! corner's colour, depth and motion up to the whole view.
//!
//! The camera's projection is jittered by a Halton (2, 3) sequence of sub-pixel offsets, so the frames the scaler
//! gathers sample between the smaller pixels, and textures take a finer mip to match (MipBias, log2 s).
//! MetalFX's denoiser, at the main pass's size, is told the jitter and gives back a frame without it, so the scaler
//! after it is told none (told it again, the frame blurred: 0.68 of the full size's fine detail against 0.77 still).
//! RAEA_SOLARI_JITTER picks which pass is told it, to compare: denoiser (unset), scaler, both, flip (the denoiser, with
//! the offset unnegated), 0 (the projection held still).
//!
//! On the Mac, where MetalFX has a temporal scaler for the GPU. Elsewhere, or where it has none, the view draws at its
//! full size and raea says so once.

use super::*;
use bevy::camera::MainPassResolutionOverride;
use bevy::core_pipeline::prepass::{DepthPrepass, MotionVectorPrepass, ViewPrepassTextures};
use bevy::core_pipeline::schedule::{Core3d, Core3dSystems};
use bevy::diagnostic::FrameCount;
use bevy::render::camera::{MipBias, TemporalJitter};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::sync_world::RenderEntity;
use bevy::render::view::ViewTarget;
use bevy::render::{Extract, ExtractSchedule, RenderApp};
use bevy::solari::prelude::SolariLighting;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::Message;
use objc2_metal::{MTLBlitCommandEncoder, MTLCommandBuffer, MTLCommandEncoder, MTLPixelFormat};
use objc2_metal_fx::{MTLFXTemporalScaler, MTLFXTemporalScalerBase, MTLFXTemporalScalerDescriptor};
use std::sync::atomic::{AtomicBool, Ordering};
use wgpu::hal::api::Metal;
use raea_sys_post::steady::halton;

/// No temporal scaler on this GPU: every view draws at its full size.
static NONE: AtomicBool = AtomicBool::new(false);

/// After app.finish(): the jitter and the main pass's size in the main world, the scaler after the main pass.
pub(crate) fn add(app: &mut App) {
    app.add_systems(PostUpdate, jitter);
    let Some(r) = app.get_sub_app_mut(RenderApp) else { return };
    r.add_systems(ExtractSchedule, extract).add_systems(Core3d, upscale.in_set(Core3dSystems::EarlyPostProcess));
}

/// The fraction of the view's size a side its main pass draws at, 1/3 to 1: RAEA_SOLARI_SCALE on a Solari camera, else
/// its RenderScale; 1 where there's no scaler.
fn scale(rs: Option<&RenderScale>, solari: bool) -> f32 {
    static FORCED: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    let forced = *FORCED.get_or_init(|| std::env::var("RAEA_SOLARI_SCALE").ok().and_then(|v| v.parse::<f32>().ok()));
    let s = if solari && let Some(f) = forced { f } else { rs.map_or(1.0, |r| r.scale) };
    if NONE.load(Ordering::Relaxed) || !s.is_finite() || s >= 0.999 { 1.0 } else { s.clamp(1.0 / 3.0, 1.0) }
}

/// The denoiser ran on the view this frame (metalfx.rs sets it; the scaler, after it, reads and clears it).
pub(crate) static DENOISED: AtomicBool = AtomicBool::new(false);
/// The denoiser denoised the view and brought it up to its full size this frame (metalfx.rs sets it; the scaler,
/// after it, reads and clears it and has nothing to do).
pub(crate) static BROUGHT_UP: AtomicBool = AtomicBool::new(false);
/// The denoiser is running (metalfx.rs sets it as it encodes, clears it if it stops): a full-size view is jittered only
/// then, as the raw frame would shimmer half a pixel.
pub(crate) static DENOISING: AtomicBool = AtomicBool::new(false);

/// What MetalFX's denoiser (`denoiser`) or scaler is told of the jitter, times Bevy's offset negated (as its DLSS gives
/// it): 1, 0 or -1, by the view's SolariSettings.jitter (`mode`). With no denoiser before it, the scaler is told it.
pub(crate) fn jitter_gain(mode: &str, denoiser: bool, denoised: bool) -> f32 {
    match (mode, denoiser) {
        ("0", _) => 0.0,
        ("both", _) => 1.0,
        ("scaler", true) => 0.0,
        ("scaler", false) => 1.0,
        ("flip", true) => -1.0,
        ("flip", false) => if denoised { 0.0 } else { -1.0 },
        (_, true) => 1.0,
        (_, false) => if denoised { 0.0 } else { 1.0 },
    }
}

/// A camera this module jittered, so only its own jitter is taken off when the scale goes back to 1.
#[derive(Component)]
struct Jittered;

/// How many frames the jitter takes to come round at render scale `s`: 8 / s², as DLSS asks.
pub(crate) fn cycle(s: f32, frames: f32) -> u32 {
    (frames / (s * s)).ceil().max(1.0) as u32
}

/// Each drawn-small camera's sub-pixel offset this frame (a Halton (2, 3) cycle, longer the smaller it draws, as
/// DLSS asks: 8 / s² frames, its settings' `jitter_frames`), its mip bias, and the prepasses the scaler reads (depth, motion). A Solari camera at its
/// full size is jittered too unless its settings' `aa` is off, so MetalFX's denoiser anti-aliases its edges as it
/// gathers frames: photo 2's frame 95 had stepped edges held still, and jittered it came 15.9 -> 15.6 off the path
/// tracer.
#[allow(clippy::type_complexity)]
fn jitter(mut cmd: Commands, frame: Res<FrameCount>, mut cams: Query<(Entity, Option<&RenderScale>, Has<SolariLighting>, Option<&mut TemporalJitter>, Has<Jittered>, Option<&super::SolariSettings>), With<Camera3d>>) {
    for (e, rs, solari, j, mine, st) in &mut cams {
        let s = scale(rs, solari);
        let st = super::settings::of(st);
        if s < 1.0 || (solari && st.aa && st.denoise && DENOISING.load(Ordering::Relaxed)) {
            let n = cycle(s, st.jitter_frames);
            let i = frame.0 % n + 1;
            let offset = if st.jitter == "0" { Vec2::ZERO } else { Vec2::new(halton(i, 2), halton(i, 3)) - 0.5 };
            match j {
                Some(mut j) if mine => j.offset = offset,
                Some(_) => {}
                None => {
                    cmd.entity(e).insert((TemporalJitter { offset }, MipBias(s.log2()), Jittered)).insert_if_new((DepthPrepass, MotionVectorPrepass));
                }
            }
        } else if mine {
            cmd.entity(e).remove::<(TemporalJitter, MipBias, Jittered)>();
        }
    }
}

/// A view the scaler brings up (it has the main pass's size in MainPassResolutionOverride).
#[derive(Component)]
pub(crate) struct Upscaled;

/// Each drawn-small camera's main pass size onto its view; taken off when its scale goes back to 1.
fn extract(mut cmd: Commands, cams: Extract<Query<(RenderEntity, &Camera, Option<&RenderScale>, Has<SolariLighting>), With<Camera3d>>>, upscaled: Query<(), With<Upscaled>>) {
    for (re, cam, rs, solari) in &cams {
        let s = scale(rs, solari);
        match cam.physical_viewport_size() {
            Some(size) if s < 1.0 && cam.is_active => {
                let part = (size.as_vec2() * s).round().as_uvec2().max(UVec2::ONE);
                cmd.entity(re).insert((MainPassResolutionOverride(part), Upscaled));
            }
            _ if upscaled.contains(re) => {
                cmd.entity(re).remove::<(MainPassResolutionOverride, Upscaled)>();
            }
            _ => {}
        }
    }
}

/// A view's scaler and its output, made again when the view's or the main pass's size changes.
struct View {
    full: UVec2,
    part: UVec2,
    scaler: Retained<ProtocolObject<dyn MTLFXTemporalScaler>>,
    out: wgpu::Texture,
    fresh: bool,
}

#[derive(Default)]
pub(crate) struct Kit {
    views: HashMap<Entity, View>,
}
// the Metal objects are only touched on the render thread, in this system
unsafe impl Send for Kit {}
unsafe impl Sync for Kit {}

impl View {
    /// A scaler from any corner of a `full` texture whose side is a third of it or more (MetalFX's dynamic resolution:
    /// the content's size is set each frame), out to `full`.
    fn new(dev: &wgpu::Device, full: UVec2, part: UVec2) -> Result<View, String> {
        let hal = unsafe { dev.as_hal::<Metal>() }.ok_or("the GPU isn't on Metal")?;
        let device = hal.raw_device();
        unsafe {
            if !MTLFXTemporalScalerDescriptor::supportsDevice(device) {
                return Err("MetalFX has no temporal scaler for this GPU".into());
            }
            let d = MTLFXTemporalScalerDescriptor::new();
            d.setColorTextureFormat(MTLPixelFormat::RGBA16Float);
            d.setDepthTextureFormat(MTLPixelFormat::Depth32Float);
            d.setMotionTextureFormat(MTLPixelFormat::RG16Float);
            d.setOutputTextureFormat(MTLPixelFormat::RGBA16Float);
            d.setInputWidth(full.x as _);
            d.setInputHeight(full.y as _);
            d.setOutputWidth(full.x as _);
            d.setOutputHeight(full.y as _);
            d.setInputContentPropertiesEnabled(true);
            d.setInputContentMinScale(1.0);
            d.setInputContentMaxScale(3.0);
            // the colour is linear and unbounded (the exposure is applied, the tone mapping isn't yet)
            d.setAutoExposureEnabled(true);
            d.setRequiresSynchronousInitialization(true);
            let scaler = d.newTemporalScalerWithDevice(device).ok_or("MetalFX made no temporal scaler")?;
            drop(hal);
            use wgpu::TextureUsages as U;
            let out = dev.create_texture(&wgpu::TextureDescriptor {
                label: Some("metalfx upscaled"),
                size: wgpu::Extent3d { width: full.x, height: full.y, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: U::STORAGE_BINDING | U::TEXTURE_BINDING | U::RENDER_ATTACHMENT | U::COPY_SRC,
                view_formats: &[],
            });
            Ok(View { full, part, scaler, out, fresh: true })
        }
    }

    /// The scaler on `cb`, from the main pass's corner of the colour, depth and motion, then its output over the colour.
    fn encode(&mut self, cb: &ProtocolObject<dyn MTLCommandBuffer>, color: &wgpu::Texture, depth: &wgpu::Texture, motion: &wgpu::Texture, jitter: Vec2, reset: bool) -> Result<(), &'static str> {
        let get = |t: &wgpu::Texture| unsafe { t.as_hal::<Metal>() }.map(|t| t.raw_handle().retain()).ok_or("a texture isn't a Metal one");
        let (color, depth, motion, out) = (get(color)?, get(depth)?, get(motion)?, get(&self.out)?);
        let s = &*self.scaler;
        unsafe {
            s.setInputContentWidth(self.part.x as _);
            s.setInputContentHeight(self.part.y as _);
            s.setColorTexture(Some(&color));
            s.setDepthTexture(Some(&depth));
            s.setMotionTexture(Some(&motion));
            s.setOutputTexture(Some(&out));
            // Bevy's motion is this frame's UV minus the last's, over the main pass's viewport; MetalFX's points from
            // this pixel to where it was, in the input's pixels
            s.setMotionVectorScaleX(-(self.part.x as f32));
            s.setMotionVectorScaleY(-(self.part.y as f32));
            s.setJitterOffsetX(jitter.x);
            s.setJitterOffsetY(jitter.y);
            s.setDepthReversed(true);
            s.setReset(reset || self.fresh);
            s.encodeToCommandBuffer(cb);
            let blit = cb.blitCommandEncoder().ok_or("no blit encoder")?;
            blit.copyFromTexture_toTexture(&out, &color);
            blit.endEncoding();
        }
        self.fresh = false;
        Ok(())
    }
}

pub(crate) type Viewed = (&'static ViewTarget, &'static ViewPrepassTextures, &'static MainPassResolutionOverride, Option<&'static TemporalJitter>, Option<&'static SolariLighting>, Option<&'static super::SolariSettings>);

pub(crate) fn upscale(view: ViewQuery<Viewed, With<Upscaled>>, device: Res<RenderDevice>, mut kit: Local<Kit>, mut ctx: RenderContext) {
    // the denoiser brought it up already, in the same pass (metalfx.rs)
    if NONE.load(Ordering::Relaxed) || BROUGHT_UP.swap(false, Ordering::Relaxed) {
        return;
    }
    let entity = view.entity();
    let (target, pre, part, jitter, solari, st) = view.into_inner();
    let (Some(depth), Some(motion)) = (pre.depth.as_ref(), pre.motion_vectors.as_ref()) else { return };
    let color: &wgpu::Texture = target.main_texture();
    if target.main_texture_format() != bevy::render::render_resource::TextureFormat::Rgba16Float {
        return;
    }
    let full = UVec2::new(color.width(), color.height());
    let dev = device.wgpu_device().clone();
    if kit.views.get(&entity).map(|v| (v.full, v.part)) != Some((full, part.0)) {
        match View::new(&dev, full, part.0) {
            Ok(v) => {
                kit.views.insert(entity, v);
            }
            Err(e) => {
                NONE.store(true, Ordering::Relaxed);
                eprintln!("raea: <SolariLighting scale>: no MetalFX scaler ({e}); the view draws at its full size (docs/raytracing.feature RT9)");
                return;
            }
        }
    }
    let v = kit.views.get_mut(&entity).unwrap();
    let denoised = DENOISED.swap(false, Ordering::Relaxed);
    let jitter = jitter.map_or(Vec2::ZERO, |j| -j.offset * jitter_gain(&super::settings::of(st).jitter, false, denoised));
    let mut raw = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("metalfx upscale") });
    let done = unsafe {
        raw.as_hal_mut::<Metal, _, _>(|e| match e.and_then(|e| e.raw_command_buffer()) {
            Some(cb) => v.encode(cb, color, &depth.texture.texture, &motion.texture.texture, jitter, solari.is_some_and(|s| s.reset)),
            None => Err("wgpu gave no Metal command buffer"),
        })
    };
    ctx.add_command_buffer(raw.finish());
    if let Err(e) = done {
        NONE.store(true, Ordering::Relaxed);
        eprintln!("raea: <SolariLighting scale>: the MetalFX scaler stopped ({e}); the view draws at its full size");
    }
}
