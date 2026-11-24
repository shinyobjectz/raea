//! RT4 (docs/raytracing.feature): MetalFX's temporal denoiser on Solari's lighting, on the Mac. No denoiser of Bevy's
//! runs on Metal (Solari's is DLSS Ray Reconstruction, NVIDIA's), so Solari draws its few rays a pixel as noise there;
//! MTLFXTemporalDenoisedScaler (macOS 26) takes that noisy colour and the frame's guide data and gives it back clean.
//!
//! Each frame, after Solari has lit the view and before the opaque pass draws the forward meshes, a compute pass writes the guide textures MetalFX reads from Bevy's G-buffer (diffuse and specular albedo,
//! world normal, roughness); then a command buffer of its own, recorded through wgpu's Metal backend, runs the
//! denoiser on the view's colour, depth and motion and copies its output back over the colour, before the forward
//! meshes and the tonemapping. The two command buffers go to wgpu's queue in that order, after Solari's.
//!
//! Drawn below its size (RT9, upscale.rs), the view's main pass holds only the top-left corner of its textures. Then
//! the denoiser waits for the forward meshes and runs where Bevy's DLSS does (EarlyPostProcess): from that corner, with
//! the projection's jitter, out to the whole view, denoising and upscaling in one pass, as MetalFX's denoised scaler is
//! made to. A pixel with nothing in the G-buffer (a forward mesh, the sky) gives the denoiser its own colour as its
//! albedo, so it has nothing there to smooth. RAEA_SOLARI_UPSCALE=two denoises the corner before the forward meshes and
//! leaves the scaling to MetalFX's temporal scaler (upscale.rs), the first way, to compare: the denoiser settled each
//! frame on the small grid, so the scaler gained nothing from the jitter (0.7 of the full size's fine detail at 0.67).
//!
//! Where there's no MetalFX denoiser (not macOS 26, not Metal, a device it doesn't support) the lighting draws as it
//! is and raea says so once. A view's SolariSettings choose how it runs (lib.rs; docs/nobuild.feature NB1a): `denoise`
//! false (RAEA_SOLARI_DENOISE=0) leaves its lighting un-denoised, to see the noise.

use super::*;
use bevy::camera::MainPassResolutionOverride;
use bevy::core_pipeline::core_3d::main_opaque_pass_3d;
use bevy::ecs::schedule::Schedules;
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::schedule::{Core3d, Core3dSystems};
use bevy::render::render_resource::TextureFormat;
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::camera::{ExtractedCamera, TemporalJitter};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::solari::prelude::SolariLighting;
use bevy::solari::realtime::SolariGuideTextures;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::Message;
use objc2_metal::{MTLBlitCommandEncoder, MTLCommandBuffer, MTLCommandEncoder, MTLOrigin, MTLPixelFormat, MTLSize, MTLTexture};
use objc2_metal_fx::{MTLFXTemporalDenoisedScaler, MTLFXTemporalDenoisedScalerBase, MTLFXTemporalDenoisedScalerDescriptor};
use wgpu::hal::api::Metal;

/// After app.finish(), when Solari's plugin has put its lighting system in Core3d: the denoiser right after that system
/// and before the opaque pass, as forward-drawn meshes (a <Character>'s skin, an extended material) come after it with
/// nothing in the G-buffer, and denoising them would smear them. Bevy doesn't export Solari's system, so it's found by
/// name and ordered against by its system type set.
pub(crate) fn add(app: &mut App) {
    let Some(r) = app.get_sub_app_mut(bevy::render::RenderApp) else { return };
    let mut schedules = r.world_mut().resource_mut::<Schedules>();
    let Some(core) = schedules.get_mut(Core3d) else { return };
    let mut solari = None;
    for (_, s, _) in core.graph().systems.iter() {
        if format!("{}", s.name()).ends_with("solari_lighting") {
            solari = s.default_system_sets().into_iter().next();
            break;
        }
    }
    let Some(solari) = solari else {
        eprintln!("raea: <SolariLighting>: Solari's lighting isn't in the render schedule; no MetalFX denoiser");
        return;
    };
    core.add_systems(denoise.after(solari).before(main_opaque_pass_3d).in_set(Core3dSystems::MainPass));
    core.add_systems(denoise_up.before(super::upscale::upscale).in_set(Core3dSystems::EarlyPostProcess));
    // Solari's own guides (primary surface replacement: a mirror's pixel takes what it reflects, and a glossy one is as
    // far off as what it reflects, RT34), written for every view; a view's `guides` and `reflection_depth` choose whether
    // its denoiser reads them
    r.world_mut().insert_resource(bevy::solari::realtime::SolariGuideBuffers { reflection_depth: true });
}

// the guides' shader: what it writes and why (the emission's split) is in its file
pub(crate) const DECODE_WGSL: &str = include_str!("metalfx_guides.wgsl");


/// A view's denoiser and its textures, made again when its input's or output's size changes, or its depth's kind.
struct View {
    /// the main pass's size (the guides'), and the denoised frame's
    size: UVec2,
    out_size: UVec2,
    /// whether its depth is Solari's guide depth (R32Float: what a mirror's pixel reflects, as far as the reflection
    /// goes) rather than the prepass's (Depth32Float, the glass's own)
    guide_depth: bool,
    scaler: Retained<ProtocolObject<dyn MTLFXTemporalDenoisedScaler>>,
    diffuse: wgpu::Texture,
    specular: wgpu::Texture,
    normal: wgpu::Texture,
    rough: wgpu::Texture,
    emit: wgpu::Texture,
    mask: wgpu::Texture,
    out: wgpu::Texture,
    /// the split emission's history (RAEA_SOLARI_EMISSIVE=taa, emit_taa.rs)
    taa: Option<super::emit_taa::History>,
    fresh: bool,
}

#[derive(Default)]
struct Kit {
    /// the guides (and the emissive out), the emissive back in, from this frame or gathered over frames (emit_taa.rs),
    /// and the sampler that reads the gathered emission's history
    pipeline: Option<(wgpu::ComputePipeline, wgpu::ComputePipeline, wgpu::ComputePipeline, wgpu::Sampler, wgpu::ComputePipeline)>,
    views: HashMap<Entity, View>,
    off: bool,
}
// the Metal objects are only touched on the render thread, in this system
unsafe impl Send for Kit {}
unsafe impl Sync for Kit {}

pub(super) fn texture(dev: &wgpu::Device, size: UVec2, format: wgpu::TextureFormat, usage: wgpu::TextureUsages, label: &str) -> wgpu::Texture {
    dev.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

/// A texture's MTLTexture, retained.
fn mtl(t: &wgpu::Texture) -> Option<Retained<ProtocolObject<dyn MTLTexture>>> {
    unsafe { t.as_hal::<Metal>() }.map(|t| t.raw_handle().retain())
}

impl View {
    fn new(dev: &wgpu::Device, size: UVec2, out_size: UVec2, guide_depth: bool, taa: bool) -> Result<View, String> {
        let hal = unsafe { dev.as_hal::<Metal>() }.ok_or("the GPU isn't on Metal")?;
        let device = hal.raw_device();
        unsafe {
            if !MTLFXTemporalDenoisedScalerDescriptor::supportsDevice(device) {
                return Err("MetalFX has no denoiser for this GPU (it needs macOS 26 and Apple silicon)".into());
            }
            let d = MTLFXTemporalDenoisedScalerDescriptor::new();
            d.setColorTextureFormat(MTLPixelFormat::RGBA16Float);
            d.setDepthTextureFormat(if guide_depth { MTLPixelFormat::R32Float } else { MTLPixelFormat::Depth32Float });
            d.setMotionTextureFormat(MTLPixelFormat::RG16Float);
            d.setDiffuseAlbedoTextureFormat(MTLPixelFormat::RGBA16Float);
            d.setSpecularAlbedoTextureFormat(MTLPixelFormat::RGBA16Float);
            d.setNormalTextureFormat(MTLPixelFormat::RGBA16Float);
            d.setRoughnessTextureFormat(MTLPixelFormat::R32Float);
            d.setDenoiseStrengthMaskTextureEnabled(true);
            d.setDenoiseStrengthMaskTextureFormat(MTLPixelFormat::R32Float);
            d.setOutputTextureFormat(MTLPixelFormat::RGBA16Float);
            d.setInputWidth(size.x as _);
            d.setInputHeight(size.y as _);
            d.setOutputWidth(out_size.x as _);
            d.setOutputHeight(out_size.y as _);
            d.setRequiresSynchronousInitialization(true);
            let scaler = d.newTemporalDenoisedScalerWithDevice(device).ok_or("MetalFX made no denoiser")?;
            drop(hal);
            use wgpu::TextureUsages as U;
            let guide = U::STORAGE_BINDING | U::TEXTURE_BINDING;
            Ok(View {
                size,
                out_size,
                guide_depth,
                scaler,
                diffuse: texture(dev, size, wgpu::TextureFormat::Rgba16Float, guide, "metalfx diffuse albedo"),
                specular: texture(dev, size, wgpu::TextureFormat::Rgba16Float, guide, "metalfx specular albedo"),
                normal: texture(dev, size, wgpu::TextureFormat::Rgba16Float, guide, "metalfx normal"),
                rough: texture(dev, size, wgpu::TextureFormat::R32Float, guide, "metalfx roughness"),
                emit: texture(dev, size, wgpu::TextureFormat::Rgba16Float, guide, "metalfx split emission"),
                mask: texture(dev, size, wgpu::TextureFormat::R32Float, guide, "metalfx denoise strength"),
                out: texture(dev, out_size, wgpu::TextureFormat::Rgba16Float, guide | U::RENDER_ATTACHMENT | U::COPY_SRC, "metalfx output"),
                taa: taa.then(|| super::emit_taa::History::new(dev, out_size)),
                fresh: true,
            })
        }
    }

    /// The denoiser on `cb`, then its output copied over the colour's corner of its size. `jitter` is the
    /// camera's this frame, in the main pass's pixels, as MetalFX takes it (Bevy's TemporalJitter negated, as its DLSS
    /// gives it). `pre_exposure`: what MetalFX divides the colour by before it denoises, and multiplies its output by
    /// after (SolariSettings.preexposure, 16 by default). It denoises bright light compressed, so a rare bright sample (the
    /// sky in a glossy reflection, a lamp's light found by one path in many) counts for less than its share, and Solari's
    /// one path a pixel is made of rare bright samples: at 1, the desk kept 0.74 of the path tracer's light, the forest's
    /// haze 0.72 and its far trunks 0.87, and Russian roulette's doubled samples lost more (photo 5's windows: q 0.5 kept
    /// 0.60 of q 1's light; 3.4 stops darker, 0.97). At 16: the desk 0.86, the haze 1.03, the trunks 1.01, the hall's
    /// whole frame 0.935 -> 0.971 and q 0.5 keeping 0.98 of q 1's; flicker 0.85 -> 0.88% of pixels. Higher keeps more
    /// and lets fireflies through: at 64 the desk 0.91 and the hall's dark corners 1.2; at 256 the desk's wall 1.59
    #[allow(clippy::too_many_arguments)]
    fn encode(&mut self, cb: &ProtocolObject<dyn MTLCommandBuffer>, color: &wgpu::Texture, depth: &wgpu::Texture, motion: &wgpu::Texture, view: &ExtractedView, jitter: Vec2, reset: bool, pre_exposure: f32) -> Result<(), &'static str> {
        let get = |t: &wgpu::Texture| mtl(t).ok_or("a texture isn't a Metal one");
        let (color, depth, motion) = (get(color)?, get(depth)?, get(motion)?);
        let (diffuse, specular, normal, rough, out) = (get(&self.diffuse)?, get(&self.specular)?, get(&self.normal)?, get(&self.rough)?, get(&self.out)?);
        let mask = get(&self.mask)?;
        let s = &*self.scaler;
        unsafe {
            s.setColorTexture(Some(&color));
            s.setDepthTexture(Some(&depth));
            s.setMotionTexture(Some(&motion));
            s.setDiffuseAlbedoTexture(Some(&diffuse));
            s.setSpecularAlbedoTexture(Some(&specular));
            s.setNormalTexture(Some(&normal));
            s.setRoughnessTexture(Some(&rough));
            s.setDenoiseStrengthMaskTexture(Some(&mask));
            s.setOutputTexture(Some(&out));
            // Bevy's motion is this frame's UV minus the last's; MetalFX's points from this pixel to where it was, in
            // pixels
            s.setMotionVectorScaleX(-(self.size.x as f32));
            s.setMotionVectorScaleY(-(self.size.y as f32));
            s.setJitterOffsetX(jitter.x);
            s.setJitterOffsetY(jitter.y);
            s.setDepthReversed(true);
            s.setPreExposure(pre_exposure);
            s.setShouldResetHistory(reset || self.fresh);
            matrices(s, view.world_from_view.to_matrix().inverse(), view.clip_from_view);
            s.encodeToCommandBuffer(cb);
            let blit = cb.blitCommandEncoder().ok_or("no blit encoder")?;
            let o = MTLOrigin { x: 0, y: 0, z: 0 };
            let size = MTLSize { width: self.out_size.x as _, height: self.out_size.y as _, depth: 1 };
            blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(&out, 0, 0, o, size, &color, 0, 0, o);
            blit.endEncoding();
        }
        self.fresh = false;
        Ok(())
    }
}

/// worldToViewMatrix and viewToClipMatrix, simd_float4x4 properties objc2-metal-fx leaves out (no simd types in
/// objc2): sent by hand, as four float32x4 columns (an aggregate of four vectors, passed as simd_float4x4 is). Only on
/// Apple silicon, and only where the scaler has them; objc2 checks a message's types in a debug build and doesn't know
/// simd's, so a debug build leaves them out.
#[allow(unused_variables)]
fn matrices(s: &ProtocolObject<dyn MTLFXTemporalDenoisedScaler>, world_to_view: Mat4, view_to_clip: Mat4) {
    #[cfg(all(target_arch = "aarch64", not(debug_assertions)))]
    unsafe {
        use objc2::{msg_send, sel};
        use std::arch::aarch64::float32x4_t;
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct Float4x4([float32x4_t; 4]);
        unsafe impl objc2::Encode for Float4x4 {
            const ENCODING: objc2::Encoding = objc2::Encoding::Struct("?", &[]);
        }
        let m = |m: Mat4| Float4x4(m.to_cols_array_2d().map(|c| std::mem::transmute::<[f32; 4], float32x4_t>(c)));
        let has: bool = msg_send![s, respondsToSelector: sel!(setWorldToViewMatrix:)];
        if has {
            let _: () = msg_send![s, setWorldToViewMatrix: m(world_to_view)];
            let _: () = msg_send![s, setViewToClipMatrix: m(view_to_clip)];
        }
    }
}

type Viewed = (&'static ViewTarget, &'static ViewPrepassTextures, &'static ExtractedView, &'static ExtractedCamera, &'static SolariLighting, Option<&'static MainPassResolutionOverride>, Option<&'static TemporalJitter>, Has<super::upscale::Upscaled>, Option<&'static SolariGuideTextures>, Option<&'static super::SolariSettings>);

/// Before the forward meshes: a view at its full size, or one drawn small whose settings upscale in "two" (its corner).
/// Each view's settings go into the run's trace here, once and on a change.
fn denoise(view: ViewQuery<Viewed>, device: Res<RenderDevice>, mut kit: Local<Kit>, mut ctx: RenderContext, mut seen: Local<HashMap<Entity, super::SolariSettings>>) {
    let entity = view.entity();
    let item = view.into_inner();
    let st = super::settings::of(item.9);
    let scale = item.5.map_or(1.0, |p| p.0.x as f32 / item.0.main_texture().width() as f32);
    super::settings::record(entity, &st, scale, &mut seen);
    // a view drawn small is denoised and brought up in one pass, unless its upscale is "two"
    if !st.denoise || (item.7 && st.upscale != "two") {
        return;
    }
    run(entity, item, &st, &device, &mut kit, &mut ctx, false);
}

/// After the main pass: a view drawn small, denoised from its corner out to its whole size.
fn denoise_up(view: ViewQuery<Viewed>, device: Res<RenderDevice>, mut kit: Local<Kit>, mut ctx: RenderContext) {
    let entity = view.entity();
    let item = view.into_inner();
    let st = super::settings::of(item.9);
    if !st.denoise || !item.7 || st.upscale == "two" {
        return;
    }
    run(entity, item, &st, &device, &mut kit, &mut ctx, true);
}

type Item<'a> = (&'a ViewTarget, &'a ViewPrepassTextures, &'a ExtractedView, &'a ExtractedCamera, &'a SolariLighting, Option<&'a MainPassResolutionOverride>, Option<&'a TemporalJitter>, bool, Option<&'a SolariGuideTextures>, Option<&'a super::SolariSettings>);

/// The guides from the G-buffer and the emissive out, at the main pass's size; MetalFX's denoiser, out to the whole
/// view when `up`; its output over the colour and the emissive back in.
fn run(entity: Entity, item: Item, st: &super::SolariSettings, device: &RenderDevice, kit: &mut Kit, ctx: &mut RenderContext, up: bool) {
    if kit.off {
        return;
    }
    let (target, pre, ext, cam, solari, part, jitter, _, guides, _) = item;
    // Solari wrote its guides for every view; this one reads them unless its settings say the G-buffer's
    let guides = guides.filter(|_| st.guides);
    let (Some(depth), Some(motion), Some(gbuf)) = (pre.depth.as_ref(), pre.motion_vectors.as_ref(), pre.deferred.as_ref()) else { return };
    if target.main_texture_format() != TextureFormat::Rgba16Float {
        return;
    }
    let color: &wgpu::Texture = target.main_texture();
    let full = UVec2::new(color.width(), color.height());
    let size = part.map_or(full, |p| p.0);
    let out_size = if up { full } else { size };
    let jitter = jitter.map_or(Vec2::ZERO, |j| -j.offset * super::upscale::jitter_gain(&st.jitter, true, true));
    let dev = device.wgpu_device().clone();
    // with Solari's guides, the denoiser's depth is theirs: a mirror's pixel as far off as what it reflects, so when what
    // it reflects moves off (an orb in a monitor's glass) the wall now there is a disocclusion and its history goes
    let guide_depth = guides.is_some() && st.reflection_depth;
    // the emission taken out before the denoiser and put back after ("taa" or "split"), its history gathered over
    // frames too ("taa", emit_taa.rs)
    let (split, taa) = (st.emissive != "mask", st.emissive == "taa");
    if kit.views.get(&entity).map(|v| (v.size, v.out_size, v.guide_depth, v.taa.is_some())) != Some((size, out_size, guide_depth, taa)) {
        match View::new(&dev, size, out_size, guide_depth, taa) {
            Ok(v) => {
                kit.views.insert(entity, v);
            }
            Err(e) => {
                kit.off = true;
                eprintln!("raea: <SolariLighting>: no MetalFX denoiser ({e}); the lighting draws un-denoised (docs/raytracing.feature RT4)");
                return;
            }
        }
    }
    let (pipeline, restore, restore_taa, hist_sampler, pipeline_psr) = kit
        .pipeline
        .get_or_insert_with(|| {
            // read from a project's engine-shaders/ or the checkout when one has them (docs/nobuild.feature NB5b)
            let guides = raea_sys_engine_shaders::text("raea_render_raytrace/metalfx_guides.wgsl", DECODE_WGSL);
            let taa = raea_sys_engine_shaders::text("raea_render_raytrace/emit_taa.wgsl", super::emit_taa::WGSL);
            let source = format!("{guides}{taa}");
            let module = dev.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("metalfx guides"), source: wgpu::ShaderSource::Wgsl(source.into()) });
            let make = |entry: &str| dev.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("metalfx guides"), layout: None, module: &module, entry_point: Some(entry), compilation_options: Default::default(), cache: None });
            (make("main"), make("restore"), make("restore_taa"), super::emit_taa::sampler(&dev), make("main_psr"))
        })
        .clone();
    let color_view = color.create_view(&Default::default());
    let back = ext.world_from_view.back();
    let reset = solari.reset || kit.views.get(&entity).is_none_or(|v| v.fresh);
    // the jitter's cycle at this render scale, how long a twig glimpsed through it can stay away (emit_taa.wgsl)
    let cycle = super::upscale::cycle(size.x as f32 / full.x as f32, st.jitter_frames) as f32;
    let words: [f32; 16] = [cam.exposure, if split { 1.0 } else { 0.0 }, if reset { 1.0 } else { 0.0 }, super::emit_taa::debug(), out_size.x as f32, out_size.y as f32, size.x as f32, size.y as f32, back.x, back.y, back.z, st.emissive_sharpness, jitter.x, jitter.y, cycle, 0.0];
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_ne_bytes()).collect();
    let params = wgpu::util::DeviceExt::create_buffer_init(&dev, &wgpu::util::BufferInitDescriptor { label: Some("metalfx params"), contents: &bytes, usage: wgpu::BufferUsages::UNIFORM });
    let v = kit.views.get_mut(&entity).unwrap();
    let views = [&v.diffuse, &v.specular, &v.normal, &v.rough].map(|t| t.create_view(&Default::default()));
    let mut entries = vec![wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&*gbuf.texture.default_view) }];
    for (i, tv) in views.iter().enumerate() {
        entries.push(wgpu::BindGroupEntry { binding: i as u32 + 1, resource: wgpu::BindingResource::TextureView(tv) });
    }
    entries.push(wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&color_view) });
    entries.push(wgpu::BindGroupEntry { binding: 6, resource: params.as_entire_binding() });
    let emit = v.emit.create_view(&Default::default());
    entries.push(wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&emit) });
    let mask = v.mask.create_view(&Default::default());
    entries.push(wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(&mask) });
    // Solari's guides when it wrote them (guides.rs), else the G-buffer's
    if let Some(g) = guides {
        for (i, t) in [&g.diffuse_albedo, &g.specular_albedo, &g.normal_roughness].into_iter().enumerate() {
            entries.push(wgpu::BindGroupEntry { binding: 9 + i as u32, resource: wgpu::BindingResource::TextureView(&*t.default_view) });
        }
    }
    let pipeline = if guides.is_some() { &pipeline_psr } else { &pipeline };
    let bind = dev.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("metalfx guides"), layout: &pipeline.get_bind_group_layout(0), entries: &entries });
    {
        let mut pass = ctx.command_encoder().begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("metalfx guides"), timestamp_writes: None });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(size.x.div_ceil(8), size.y.div_ceil(8), 1);
    }
    // with Solari's guides, its motion too: a mirror's pixel moves as what it reflects does (the prepass's is the glass's,
    // and Solari can't write it there where the GPU has no read-write Rg16Float, as Apple's hasn't)
    let guide_motion: &wgpu::Texture = match guides {
        Some(g) => &g.specular_motion_vectors.texture,
        None => &motion.texture.texture,
    };
    // a command buffer of its own (wgpu won't mix its own commands and raw ones on one encoder), after the guides'
    let mut raw = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("metalfx denoise") });
    let done = unsafe {
        raw.as_hal_mut::<Metal, _, _>(|e| match e.and_then(|e| e.raw_command_buffer()) {
            Some(cb) => v.encode(cb, color, match guides { Some(g) if v.guide_depth => &g.depth.texture, _ => &depth.texture.texture }, guide_motion, ext, jitter, solari.reset, st.preexposure),
            None => Err("wgpu gave no Metal command buffer"),
        })
    };
    ctx.add_command_buffer(raw.finish());
    // the emissive back in, after the denoiser's command buffer (add_command_buffer flushed the encoder before it)
    if split {
    let mut entries = vec![
        wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&color_view) },
        wgpu::BindGroupEntry { binding: 6, resource: params.as_entire_binding() },
        wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&emit) },
    ];
    let taa_views = v.taa.as_ref().map(|h| h.views(&depth.texture.texture, &motion.texture.texture));
    if let Some(views) = &taa_views {
        entries.extend(super::emit_taa::entries(views, &hist_sampler));
    }
    let put = if taa_views.is_some() { &restore_taa } else { &restore };
    let back = dev.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("metalfx emissive back"), layout: &put.get_bind_group_layout(0), entries: &entries });
    if let Some(h) = v.taa.as_mut() {
        h.advance();
    }
    {
        let mut pass = ctx.command_encoder().begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("metalfx emissive back"), timestamp_writes: None });
        pass.set_pipeline(put);
        pass.set_bind_group(0, &back, &[]);
        pass.dispatch_workgroups(out_size.x.div_ceil(8), out_size.y.div_ceil(8), 1);
    }
    }
    use std::sync::atomic::Ordering::Relaxed;
    super::upscale::DENOISED.store(done.is_ok(), Relaxed);
    super::upscale::DENOISING.store(done.is_ok(), Relaxed);
    if up {
        super::upscale::BROUGHT_UP.store(done.is_ok(), Relaxed);
    }
    if let Err(e) = done {
        kit.off = true;
        eprintln!("raea: <SolariLighting>: the MetalFX denoiser stopped ({e}); the lighting draws un-denoised");
    }
}
