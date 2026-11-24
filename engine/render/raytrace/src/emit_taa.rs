//! What a surface emits, kept out of MetalFX's denoiser and gathered over frames on its own (the default;
//! RAEA_SOLARI_EMISSIVE=mask gives it to the denoiser as before, =split puts back each frame's alone).
//!
//! MetalFX's denoiser clamps its history to the neighbourhood it sees each frame, which is what keeps Solari's rare
//! bright paths from flaring, but a bright detail smaller than a pixel (sky through a spruce's twigs, a lit wire) shows
//! in only some of the jittered frames, and the clamp throws it away: photo 3's canopy came out at 0.47 of the path
//! tracer's light (raw Solari, 1.02). Emission has no noise to suppress, so it needs no clamp. metalfx.rs's guide pass
//! takes it out of the colour (as `split` does), and `restore_taa` here gathers it at the output's size: each frame's
//! jittered samples near an output pixel, weighted by their distance to its centre, added to the pixel's history
//! reprojected by the motion of the nearest surface about it, read through a Catmull-Rom filter. Where the pixel moved
//! and the history's depth isn't among those about it (a disocclusion), the history counts as one frame's; a cut
//! drops it; it holds at most 16 frames' weight. Measured against the path tracer, drawn at half size: the forest's
//! canopy 0.99 of its light (MetalFX alone 0.52), and as a person sees it, tone mapped, 1.03 (0.47); photo 2's sunlit
//! room and photo 1's lamp unchanged (0.87, 0.79-0.82); photo 2's window edges 2 pixels a frame jumping 80 levels
//! (MetalFX alone 0, each frame's alone 1088); a screen's fine detail 0.44 of the tracer's (MetalFX 0.52); about 1 ms
//! at 1080 x 1920. Through MetalFX's own temporal scaler instead, its clamp dropped the canopy again (0.50).
//!
//! Without a clamp, what a pixel showed stayed once it had gone (RT32): a 30000-nit orb crossing the desk left a white
//! tube along its path, and one taken or switched off stayed white for seconds. Each history now keeps the depths it
//! was gathered over and, for the nearest, how often it is about, how long it hasn't been, and whether it was moving.
//! A still pixel's history is dropped where none of those depths is about it. It's held to the emission about it
//! (3 x 3 samples) where its nearest surface has gone: away longer than a surface about nearly every frame can be, away
//! the jitter's whole cycle, or moving when last seen. It's also held where every surface it saw is about. Where the view
//! moves, it's held only above four times the brightest within the history read's reach (5 x 5). A twig glimpsed in some
//! jittered frames is none of these, so the canopy keeps its light (1.09 of the tracer's before, 1.11 after).

use bevy::prelude::*;

pub(crate) const WGSL: &str = include_str!("emit_taa.wgsl");

/// RAEA_EMIT_TAA_DEBUG=1: the pass paints its weights instead; 2 never takes the history, 3 always does; 4 paints which
/// of RT32's rules held it (emit_taa.wgsl)
pub(crate) fn debug() -> f32 {
    static ON: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("RAEA_EMIT_TAA_DEBUG").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0))
}

/// A view's emission history at the output's size: the gathered emission and its weight, and the depths it was gathered
/// over with the evidence for its nearest surface, two of each, one read as the other is written.
pub(crate) struct History {
    color: [wgpu::Texture; 2],
    depth: [wgpu::Texture; 2],
    /// which of the two this frame writes
    next: usize,
}

impl History {
    pub(crate) fn new(dev: &wgpu::Device, out_size: UVec2) -> History {
        use wgpu::TextureUsages as U;
        let make = |format, label| super::metalfx::texture(dev, out_size, format, U::STORAGE_BINDING | U::TEXTURE_BINDING, label);
        History {
            color: [make(wgpu::TextureFormat::Rgba16Float, "emission history"), make(wgpu::TextureFormat::Rgba16Float, "emission history")],
            // each pixel's depth, the nearest moving surface's about it, and the depths its history was gathered over
            // (emit_taa.wgsl)
            depth: [make(wgpu::TextureFormat::Rgba32Float, "emission history depth"), make(wgpu::TextureFormat::Rgba32Float, "emission history depth")],
            next: 0,
        }
    }

    /// The pass's own bindings (9 to 15): the view's depth and motion, last frame's history to read and this frame's to
    /// write. Call `advance` once the pass is recorded.
    pub(crate) fn views(&self, depth: &wgpu::Texture, motion: &wgpu::Texture) -> [wgpu::TextureView; 6] {
        let v = |t: &wgpu::Texture| t.create_view(&Default::default());
        let (r, w) = (1 - self.next, self.next);
        let depth = depth.create_view(&wgpu::TextureViewDescriptor { aspect: wgpu::TextureAspect::DepthOnly, ..Default::default() });
        [depth, v(motion), v(&self.color[r]), v(&self.color[w]), v(&self.depth[r]), v(&self.depth[w])]
    }

    pub(crate) fn advance(&mut self) {
        self.next = 1 - self.next;
    }
}

/// The bilinear clamp sampler the history is read with.
pub(crate) fn sampler(dev: &wgpu::Device) -> wgpu::Sampler {
    dev.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("emission history"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}

/// The bind group entries 9 to 15 from `views` and the sampler.
pub(crate) fn entries<'a>(views: &'a [wgpu::TextureView; 6], sampler: &'a wgpu::Sampler) -> [wgpu::BindGroupEntry<'a>; 7] {
    let tv = |b: u32, v: &'a wgpu::TextureView| wgpu::BindGroupEntry { binding: b, resource: wgpu::BindingResource::TextureView(v) };
    [
        tv(9, &views[0]),
        tv(10, &views[1]),
        tv(11, &views[2]),
        wgpu::BindGroupEntry { binding: 12, resource: wgpu::BindingResource::Sampler(sampler) },
        tv(13, &views[3]),
        tv(14, &views[4]),
        tv(15, &views[5]),
    ]
}
