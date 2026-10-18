//! The GPU half of quads/: one wgpu device, the instanced pipeline (quads.wgsl), and the caches the recorder fills as
//! it goes: glyphs and gradient ramps rasterized by vello_cpu (so text and gradients match the CPU path), image
//! textures by blob, fallback rasters by content, and offscreen textures for group opacity. A frame is a few render
//! passes (each group's first, then the page) and one readback, in vello_cpu's layout: premultiplied RGBA8.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use anyrender::{ImageRenderer, PaintScene};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use bytemuck::{Pod, Zeroable};
use peniko::kurbo::{self, Affine, Rect};
use peniko::{Color, Fill, FontData, Gradient, GradientKind, LinearGradientPosition};

use super::record::{Op, Tex};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default, Debug)]
pub struct Inst {
    pub xf0: [f32; 4],
    pub xf1: [f32; 4],
    pub xi0: [f32; 4],
    pub xi1: [f32; 4],
    pub quad: [f32; 4],
    pub shape: [f32; 4],
    pub radii: [f32; 4],
    pub color: [f32; 4],
    pub p: [f32; 4],
    pub g: [f32; 4],
    pub a: [f32; 4],
    pub m0: [f32; 4],
    pub m1: [f32; 4],
    pub clip: [f32; 4],
    pub clipr: [f32; 4],
    pub pad: [f32; 4],
    pub inner: [f32; 4],
    pub innerr: [f32; 4],
    pub hole: [f32; 4],
    pub holer: [f32; 4],
}

/// vello_common's blurred rounded rect, as its encoder sets it up: (exponent, scale, 1/sigma, min edge), (w, h, r1).
pub fn shadow_params(rect: [f32; 4], radius: f32, sigma: f32) -> ([f32; 4], [f32; 4]) {
    let (width, height) = (rect[2] - rect[0], rect[3] - rect[1]);
    let radius = radius.min(0.5 * width.min(height));
    let sd = sigma.max(1e-6);
    let min_edge = width.min(height);
    let rmax = 0.5 * min_edge;
    let r0 = radius.hypot(sd * 1.15).min(rmax);
    let r1 = radius.hypot(sd * 2.0).min(rmax);
    let exponent = 2.0 * r1 / r0;
    let inv = 1.0 / sd;
    let delta = 1.25 * sd * ((-(0.5 * inv * width).powi(2)).exp() - (-(0.5 * inv * height).powi(2)).exp());
    let w = width + delta.min(0.0);
    let h = height - delta.max(0.0);
    let erf7 = |x: f32| {
        let x = x.clamp(-10.0, 10.0) * std::f32::consts::FRAC_2_SQRT_PI;
        let xx = x * x;
        let x = x + (0.24295 + (0.03395 + 0.0104 * xx) * xx) * (x * xx);
        x / (1.0 + x * x).sqrt()
    };
    let scale = 0.5 * erf7(inv * 0.5 * (w.max(h) - 0.5 * radius));
    ([exponent, scale, inv, min_edge], [w, h, r1, 0.0])
}

/// A glyph as rasterized: its font, id, size, the run's linear transform and a subpixel offset.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey(u64);

impl GlyphKey {
    #[allow(clippy::too_many_arguments)]
    pub fn new(font: &FontData, id: u32, size: f32, hint: bool, coords: &[anyrender::NormalizedCoord], embolden: kurbo::Vec2, linear: [f64; 4], gt: Option<Affine>, sub: [u8; 2]) -> GlyphKey {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        font.data.id().hash(&mut h);
        font.index.hash(&mut h);
        id.hash(&mut h);
        size.to_bits().hash(&mut h);
        hint.hash(&mut h);
        coords.hash(&mut h);
        embolden.x.to_bits().hash(&mut h);
        embolden.y.to_bits().hash(&mut h);
        for v in linear {
            ((v * 1e4).round() as i64).hash(&mut h);
        }
        gt.map(|g| g.as_coeffs().map(|v| (v * 1e4).round() as i64)).hash(&mut h);
        sub.hash(&mut h);
        GlyphKey(h.finish())
    }
}

#[derive(Clone, Copy)]
pub struct GlyphEntry {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// where the box sits from the glyph's whole-pixel origin
    pub dx: f32,
    pub dy: f32,
    /// a colour glyph (emoji: COLR or bitmap): its own colours, which a coverage atlas can't hold
    pub color: bool,
}

const ATLAS: u32 = 4096;
/// Glyph offsets kept per pixel: a glyph lands within 1/(2*SUBPIXEL) px of where vello_cpu puts it. With Skia's glyph
/// masks (the engine's default build) it's Skia's own grid, a quarter pixel, so each glyph rounds as Skia rounds it.
pub const SUBPIXEL: f64 = if cfg!(feature = "skia") { 4.0 } else { 32.0 };
const RAMPS: u32 = 1024;

struct Cached {
    _tex: wgpu::Texture,
    view: wgpu::TextureView,
    used: u64,
}

pub struct Caches {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    atlas: wgpu::Texture,
    glyphs: HashMap<GlyphKey, GlyphEntry>,
    shelf: (u32, u32, u32),
    /// the atlas filled during this frame: the frame is recorded again on a cleared atlas
    pub atlas_full: bool,
    ramp: wgpu::Texture,
    ramps: HashMap<String, u32>,
    images: HashMap<u64, Cached>,
    rasters: HashMap<u64, Cached>,
    /// each video's current frame (set_video), and CoreVideo's texture cache on this device
    pub videos: HashMap<u64, super::video::Frame>,
    /// textures a page is painted into for another page to draw (copy_page_to), by id
    owned: HashMap<u64, wgpu::Texture>,
    /// frames just replaced, kept while a frame in flight may still read them (the decoder's pool reuses buffers)
    old_videos: std::collections::VecDeque<super::video::Frame>,
    #[cfg(target_vendor = "apple")]
    video_cache: Option<super::video::Cache>,
    frame: u64,
    pub stats: Stats,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Stats {
    pub glyph_misses: usize,
    pub raster_misses: usize,
    pub raster_hits: usize,
    pub image_uploads: usize,
    /// milliseconds in render: uploads and rasters, binning, the GPU's work and the wait for it, the copy out
    pub prepare_ms: f32,
    pub bin_ms: f32,
    pub wait_ms: f32,
    pub copy_ms: f32,
}

impl Caches {
    /// A glyph's place in the atlas, rasterized by vello_cpu on a miss (None when the atlas is full).
    #[allow(clippy::too_many_arguments)]
    pub fn glyph(&mut self, key: &GlyphKey, font: &FontData, id: u32, size: f32, hint: bool, coords: &[anyrender::NormalizedCoord], embolden: kurbo::Vec2, linear: [f64; 4], gt: Option<Affine>, sub: [u8; 2]) -> Option<GlyphEntry> {
        if let Some(e) = self.glyphs.get(key) {
            return Some(*e);
        }
        if self.atlas_full {
            return None;
        }
        self.stats.glyph_misses += 1;
        let scale = (linear[0] * linear[3] - linear[1] * linear[2]).abs().sqrt().max(linear[0].hypot(linear[1])).max(linear[2].hypot(linear[3]));
        let em = (size as f64 * scale * gt.map(|g| g.determinant().abs().sqrt().max(1.0)).unwrap_or(1.0)).max(1.0);
        let (ox, oy) = ((em * 1.5).ceil() + 2.0, (em * 2.0).ceil() + 2.0);
        let (bw, bh) = ((em * 4.0).ceil() as u32 + 4, (em * 3.0).ceil() as u32 + 4);
        let mut r = VelloCpuImageRenderer::new(bw, bh);
        let mut px = Vec::new();
        let t = Affine::new([linear[0], linear[1], linear[2], linear[3], ox + sub[0] as f64 / SUBPIXEL, oy + sub[1] as f64 / SUBPIXEL]);
        r.render_to_vec(
            |s| s.draw_glyphs(font, size, hint, coords, embolden, Fill::NonZero, Color::WHITE, 1.0, t, gt, std::iter::once(anyrender::Glyph { id, x: 0.0, y: 0.0 })),
            &mut px,
        );
        // trim to what was drawn
        let (mut x0, mut y0, mut x1, mut y1) = (bw, bh, 0u32, 0u32);
        for y in 0..bh {
            for x in 0..bw {
                if px[((y * bw + x) * 4 + 3) as usize] != 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        if x1 <= x0 {
            let e = GlyphEntry { x: 0, y: 0, w: 0, h: 0, dx: 0.0, dy: 0.0, color: false };
            self.glyphs.insert(*key, e);
            return Some(e);
        }
        // drawn in white, a plain glyph's colour channels equal its alpha; a colour glyph's don't
        let color = px.chunks_exact(4).any(|p| p[0].abs_diff(p[3]) > 2 || p[1].abs_diff(p[3]) > 2 || p[2].abs_diff(p[3]) > 2);
        if color {
            let e = GlyphEntry { x: 0, y: 0, w: 0, h: 0, dx: 0.0, dy: 0.0, color: true };
            self.glyphs.insert(*key, e);
            return Some(e);
        }
        let (w, h) = (x1 - x0, y1 - y0);
        let (sx, sy, row) = &mut self.shelf;
        if *sx + w > ATLAS {
            *sx = 0;
            *sy += *row;
            *row = 0;
        }
        if *sy + h > ATLAS {
            self.atlas_full = true;
            return None;
        }
        let (ax, ay) = (*sx, *sy);
        *sx += w + 1;
        *row = (*row).max(h + 1);
        let mut buf = vec![0u8; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                buf[(y * w + x) as usize] = px[(((y + y0) * bw + x + x0) * 4 + 3) as usize];
            }
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.atlas, mip_level: 0, origin: wgpu::Origin3d { x: ax, y: ay, z: 0 }, aspect: wgpu::TextureAspect::All },
            &buf,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w), rows_per_image: None },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let e = GlyphEntry { x: ax, y: ay, w, h, dx: x0 as f32 - ox as f32, dy: y0 as f32 - oy as f32, color: false };
        self.glyphs.insert(*key, e);
        Some(e)
    }

    /// The page target copied into texture `id` (made or remade for its size, kept between frames) and that texture
    /// set for `Paint::Resource` id: a page painted for another page to draw (the stage inside a window's chrome).
    pub fn copy_page_to(&mut self, e: &mut wgpu::CommandEncoder, id: u64, target: &wgpu::Texture) {
        let (w, h) = (target.width(), target.height());
        let fresh = self.owned.get(&id).is_none_or(|t| t.width() != w || t.height() != h);
        if fresh {
            let t = self.device.create_texture(&wgpu::TextureDescriptor { label: Some("page texture"), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
            self.owned.insert(id, t);
        }
        let t = &self.owned[&id];
        e.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo { texture: target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyTextureInfo { texture: t, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        if fresh || !self.videos.contains_key(&id) {
            let view = t.create_view(&Default::default());
            self.set_texture(id, view, w, h);
        }
    }

    /// Pixels (premultiplied RGBA8, w x h) uploaded into texture `id` (made or remade for the size, kept) and set for
    /// `Paint::Resource` id: a CPU-painted layer a GPU frame draws (a window's chrome painted by vello_cpu over the stage).
    pub fn upload_owned(&mut self, id: u64, w: u32, h: u32, px: &[u8]) {
        let fresh = self.owned.get(&id).is_none_or(|t| t.width() != w || t.height() != h);
        if fresh {
            let t = self.device.create_texture(&wgpu::TextureDescriptor { label: Some("uploaded layer"), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
            self.owned.insert(id, t);
        }
        let t = &self.owned[&id];
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: t, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            px,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: None },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        if fresh || !self.videos.contains_key(&id) {
            let view = t.create_view(&Default::default());
            self.set_texture(id, view, w, h);
        }
    }

    /// Texture `id` (drawn by a page's `Paint::Resource` with that id): a view the paint device already holds.
    pub fn set_texture(&mut self, id: u64, view: wgpu::TextureView, w: u32, h: u32) {
        if let Some(old) = self.videos.insert(id, super::video::Frame::texture(view, w, h)) {
            self.old_videos.push_back(old);
            while self.old_videos.len() > 4 * (super::gpu::RING_KEEP) {
                self.old_videos.pop_front();
            }
        }
    }

    #[cfg(target_vendor = "apple")]
    pub fn set_video(&mut self, id: u64, buffer: objc2_core_foundation::CFRetained<objc2_core_video::CVPixelBuffer>, w: u32, h: u32) -> Result<(), String> {
        if self.video_cache.is_none() {
            self.video_cache = Some(super::video::Cache::new(&self.device)?);
        }
        let cache = self.video_cache.as_ref().unwrap();
        let f = cache.wrap(&self.device, buffer, w, h)?;
        if let Some(old) = self.videos.insert(id, f) {
            self.old_videos.push_back(old);
            while self.old_videos.len() > 4 * (super::gpu::RING_KEEP) {
                self.old_videos.pop_front();
            }
        }
        cache.flush();
        Ok(())
    }

    /// Clear the atlas (it filled up): every glyph rasterizes again as it's next drawn.
    pub fn clear_atlas(&mut self) {
        self.glyphs.clear();
        self.shelf = (0, 0, 0);
        self.atlas_full = false;
    }

    /// A gradient's ramp row (its v coordinate): its stops interpolated by vello_cpu along 256 texels, so the colour
    /// space, hue direction and premultiplication are vello's own.
    pub fn ramp(&mut self, g: &Gradient) -> Option<f32> {
        let key = format!("{:?}|{:?}|{:?}", g.stops, g.interpolation_cs, g.hue_direction);
        let row = match self.ramps.get(&key) {
            Some(r) => *r,
            None => {
                if self.ramps.len() as u32 >= RAMPS {
                    self.ramps.clear();
                }
                let row = self.ramps.len() as u32;
                let mut flat = g.clone();
                flat.kind = GradientKind::Linear(LinearGradientPosition::new((0.0, 0.5), (256.0, 0.5)));
                flat.extend = peniko::Extend::Pad;
                let mut r = VelloCpuImageRenderer::new(256, 1);
                let mut px = Vec::new();
                r.render_to_vec(|s| s.fill(Fill::NonZero, Affine::IDENTITY, &flat, None, &Rect::new(0.0, 0.0, 256.0, 1.0)), &mut px);
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo { texture: &self.ramp, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: row, z: 0 }, aspect: wgpu::TextureAspect::All },
                    &px,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256 * 4), rows_per_image: None },
                    wgpu::Extent3d { width: 256, height: 1, depth_or_array_layers: 1 },
                );
                self.ramps.insert(key, row);
                row
            }
        };
        Some((row as f32 + 0.5) / RAMPS as f32)
    }

    fn texture(&self, label: &str, w: u32, h: u32, rgba: &[u8]) -> (wgpu::Texture, wgpu::TextureView) {
        self.texture_as(label, w, h, rgba, wgpu::TextureFormat::Rgba8Unorm)
    }

    fn texture_as(&self, label: &str, w: u32, h: u32, rgba: &[u8], format: wgpu::TextureFormat) -> (wgpu::Texture, wgpu::TextureView) {
        let t = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &t, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: None },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let v = t.create_view(&Default::default());
        (t, v)
    }

    /// An image brush's texture, its bytes as they are, uploaded on first use.
    fn image(&mut self, img: &peniko::ImageData) {
        let id = img.data.id();
        if let Some(c) = self.images.get_mut(&id) {
            c.used = self.frame;
            return;
        }
        self.stats.image_uploads += 1;
        // the bytes as they are: BGRA as a BGRA texture, straight alpha premultiplied by the shader (fine.wgsl texel)
        let format = if img.format == peniko::ImageFormat::Bgra8 { wgpu::TextureFormat::Bgra8Unorm } else { wgpu::TextureFormat::Rgba8Unorm };
        let (t, v) = self.texture_as("image", img.width, img.height, img.data.data(), format);
        self.images.insert(id, Cached { _tex: t, view: v, used: self.frame });
    }

    /// A fallback raster, by vello_cpu on a miss.
    fn raster(&mut self, key: u64, bbox: [i32; 4], scene: &anyrender::recording::Scene) {
        if let Some(c) = self.rasters.get_mut(&key) {
            c.used = self.frame;
            self.stats.raster_hits += 1;
            return;
        }
        self.stats.raster_misses += 1;
        let (w, h) = ((bbox[2] - bbox[0]) as u32, (bbox[3] - bbox[1]) as u32);
        let mut r = VelloCpuImageRenderer::new(w, h);
        let mut px = Vec::new();
        r.render_to_vec(|s| s.append_scene(scene.clone(), Affine::IDENTITY), &mut px);
        let (t, v) = self.texture("raster", w, h, &px);
        self.rasters.insert(key, Cached { _tex: t, view: v, used: self.frame });
    }

    /// Drop what the last two frames didn't use.
    fn evict(&mut self) {
        let f = self.frame;
        self.images.retain(|_, c| c.used + 2 > f);
        self.rasters.retain(|_, c| c.used + 2 > f);
    }
}

/// What backdrop blurs need: the two blur pipelines, and page-sized textures (the page as it was, the blurred page) and
/// the float scratch, made for the page's size.
struct Backdrop {
    h: wgpu::ComputePipeline,
    v: wgpu::ComputePipeline,
    /// a filter's blur (blur.wgsl)
    bh: wgpu::ComputePipeline,
    bv: wgpu::ComputePipeline,
    textures: Option<BackdropTextures>,
}

struct BackdropTextures {
    prev: (wgpu::Texture, wgpu::TextureView),
    out: (wgpu::Texture, wgpu::TextureView),
    tmp: (wgpu::Texture, wgpu::TextureView, u32, u32),
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurU {
    origin: [i32; 2],
    size: [i32; 2],
    page: [i32; 2],
    r: i32,
    sigma: f32,
    /// a colour matrix after the blur: rows R G B A (four factors each), the offsets, then whether there is one
    matrix: [[f32; 4]; 6],
}

/// A filter's colour matrix (4x5, row-major) as the backdrop shader takes it.
fn blur_matrix(m: Option<[f32; 20]>) -> [[f32; 4]; 6] {
    match m {
        None => [[0.0; 4]; 6],
        Some(m) => {
            let row = |r: usize| [m[r * 5], m[r * 5 + 1], m[r * 5 + 2], m[r * 5 + 3]];
            [row(0), row(1), row(2), row(3), [m[4], m[9], m[14], m[19]], [1.0, 0.0, 0.0, 0.0]]
        }
    }
}

/// blur.wgsl's uniform: a filter's blur (Skia's three boxes as one integer kernel)
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BoxU {
    origin: [i32; 2],
    size: [i32; 2],
    page: [i32; 2],
    r: i32,
    divider: u32,
}

/// Skia's blur for `sigma` (and so Chrome's, and raea's vello_cpu box_blur): three boxes of window
/// d = floor(σ·3·√(2π)/4 + 0.5) (for an even d two of d, half a pixel left and right, and one of d + 1) as one kernel of
/// whole weights, and the fixed-point divider of their product. None when there's nothing to blur (d ≤ 1) or the window
/// is too large (d ≥ 255), where vello_cpu falls back to its decimated Gaussian.
fn skia_box_kernel(sigma: f32) -> Option<(Vec<u32>, u32)> {
    let d = (f64::from(sigma) * 3.0 * (2.0 * std::f64::consts::PI).sqrt() / 4.0 + 0.5).floor() as i64;
    if !(2..255).contains(&d) {
        return None;
    }
    let boxes: [(i64, i64); 3] = if d % 2 == 1 { [(-(d - 1) / 2, (d - 1) / 2); 3] } else { [(-d / 2, d / 2 - 1), (-d / 2 + 1, d / 2), (-d / 2, d / 2)] };
    // the boxes convolved: offsets from the sum of their lows
    let mut k = vec![1u32];
    let mut lo = 0;
    for (a, b) in boxes {
        let n = (b - a + 1) as usize;
        let mut out = vec![0u32; k.len() + n - 1];
        for (i, v) in k.iter().enumerate() {
            for j in 0..n {
                out[i + j] += v;
            }
        }
        k = out;
        lo += a;
    }
    debug_assert_eq!(-lo as usize, (k.len() - 1) / 2);
    let weight: u64 = boxes.iter().map(|&(a, b)| (b - a + 1) as u64).product();
    Some((k, ((1u64 << 32) as f64 / weight as f64).round() as u32))
}

pub struct Gpu {
    pub caches: Caches,
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    /// the present pass for a surface's format (present)
    blit: Option<(wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::TextureFormat)>,
    /// an offscreen stage's last submission (composite_only)
    offscreen_prev: Option<wgpu::SubmissionIndex>,
    /// the page packed into a storage buffer (composite_to_buffer), made on first use
    pack: Option<wgpu::ComputePipeline>,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    tex_layout: wgpu::BindGroupLayout,
    lin: wgpu::Sampler,
    atlas_view: wgpu::TextureView,
    ramp_view: wgpu::TextureView,
    blank: wgpu::TextureView,
    insts: wgpu::Buffer,
    cap: usize,
    bins: wgpu::Buffer,
    bins_cap: usize,
    passes: wgpu::Buffer,
    size: (u32, u32),
    target: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// readback buffers in a ring, so a frame's pixels can be collected after the next frame is submitted
    ring: Vec<Readback>,
    next: usize,
    pool: Vec<(wgpu::Texture, wgpu::TextureView)>,
    /// backdrop blurs (Op::Backdrop): made at the first one
    backdrop: Option<Backdrop>,
    /// each pass's GPU time (timestamp queries), into the run's trace; whether the frame being encoded is timed
    timer: Option<raea_gputrace::PassTimer>,
    timed: bool,
}

/// Textures one dispatch can sample (images, rasters, groups); a frame with more paints on the CPU.
const MAX_TEX: u32 = 1024;
/// A pass's uniform, at 256-byte strides (the uniform offset alignment).
const PASS_STRIDE: u64 = 256;
const MAX_PASSES: u64 = 256;
const TILE: u32 = 16;
/// The page onto a surface: a fullscreen triangle sampling the page (premultiplied, as the surface takes it).
const BLIT: &str = "
@group(0) @binding(0) var page: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
struct V { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs(@builtin(vertex_index) i: u32) -> V {
  let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
  var o: V;
  o.pos = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
  o.uv = uv;
  return o;
}
@fragment fn fs(v: V) -> @location(0) vec4<f32> { return textureSampleLevel(page, samp, v.uv, 0.0); }
";
/// Frames that can be in flight (submitted, not yet collected).
const RING: usize = 3;
/// RAEA_STAGE_READ=OUT.png:N: the Nth presented drawable copied out as it went to the layer (what a window was given)
static READ: std::sync::LazyLock<Option<(String, u64)>> = std::sync::LazyLock::new(|| {
    let v = std::env::var("RAEA_STAGE_READ").ok()?;
    let (p, n) = v.rsplit_once(':')?;
    Some((p.to_string(), n.parse().ok()?))
});
static PRESENTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// `read_next_present`: the next presented drawable is copied out (RGBA, opaque) for `take_present`
pub(crate) static WANT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// surfaces configured from now can be copied from (tests reading back what a window was given)
pub(crate) static COPYABLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(crate) static TAKEN: std::sync::Mutex<Option<(u32, u32, Vec<u8>)>> = std::sync::Mutex::new(None);
/// RAEA_STAGE_TINT=1: a stage clears magenta around the page, to see in a window whether the stage's layer shows at all.
static TINT: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| std::env::var("RAEA_STAGE_TINT").as_deref() == Ok("1"));
/// Replaced video frames kept per video, at least as many as frames in flight.
pub const RING_KEEP: usize = RING;

struct Readback {
    buf: wgpu::Buffer,
    /// the submission that copies a frame into it, its size, and the frame's stats so far
    pending: Option<(wgpu::SubmissionIndex, u32, u32, Stats)>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PassU {
    size: [u32; 2],
    tiles_x: u32,
    offsets: u32,
    clear: [f32; 4],
}

impl Gpu {
    pub fn new() -> Result<Gpu, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() }))
            .map_err(|e| format!("no GPU adapter: {e}"))?;
        let need = wgpu::Features::TEXTURE_BINDING_ARRAY | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING | wgpu::Features::PARTIALLY_BOUND_BINDING_ARRAY;
        if !adapter.features().contains(need) {
            return Err(format!("the GPU lacks texture binding arrays ({:?})", need - adapter.features()));
        }
        // the device Bevy's renderer asks for itself (its Functionality priority: everything the adapter has, less
        // mappable primary buffers on a discrete GPU), so Bevy can render on this one (`handles`, rdb's engine): one
        // device for the pages and the 3D, no page crossing between two
        let mut features = adapter.features() | need;
        if adapter.get_info().device_type == wgpu::DeviceType::DiscreteGpu {
            features.remove(wgpu::Features::MAPPABLE_PRIMARY_BUFFERS);
        }
        let mut limits = adapter.limits();
        limits.max_binding_array_elements_per_shader_stage = limits.max_binding_array_elements_per_shader_stage.max(MAX_TEX);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("raea quads"),
            required_features: features,
            required_limits: limits,
            // as Bevy's own request does (bevy_render renderer::initialize_renderer)
            experimental_features: unsafe { wgpu::ExperimentalFeatures::enabled() },
            ..Default::default()
        }))
        .map_err(|e| format!("GPU device: {e}"))?;
        // GPU errors nothing caught, and the device lost, in the run's trace, put to the subsystem whose labels they name
        // (quads, Bevy rendering on this device, rdb, video; the driver when none): raea_gputrace
        raea_gputrace::watch(&device, "quads");
        // each pass timed on the GPU where the device has timestamp queries (RAEA_GPU_TIMING=0 off)
        let timer = raea_gputrace::PassTimer::new(&device, &queue, "quads", 64);
        // a shader or pipeline the driver rejects is an error to fall back on, not a panic
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let m = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("quads fine"), source: wgpu::ShaderSource::Wgsl(include_str!("fine.wgsl").into()) });
        let tex_entry = |b: u32, count: Option<std::num::NonZeroU32>| wgpu::BindGroupLayoutEntry {
            binding: b,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count,
        };
        let storage = |b: u32| wgpu::BindGroupLayoutEntry { binding: b, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quads"),
            entries: &[
                storage(0),
                storage(1),
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: std::num::NonZeroU64::new(std::mem::size_of::<PassU>() as u64) }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                tex_entry(4, None),
                tex_entry(5, None),
                wgpu::BindGroupLayoutEntry { binding: 6, visibility: wgpu::ShaderStages::COMPUTE, ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: wgpu::TextureFormat::Rgba8Unorm, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
            ],
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("quads textures"), entries: &[tex_entry(0, std::num::NonZeroU32::new(MAX_TEX))] });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout), Some(&tex_layout)], immediate_size: 0 });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("quads fine"), layout: Some(&pl), module: &m, entry_point: Some("fine"), compilation_options: wgpu::PipelineCompilationOptions { constants: &[("SKIA_TEXT", if cfg!(feature = "skia") { 1.0 } else { 0.0 })], ..Default::default() }, cache: None });
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(format!("the GPU paint shader: {e}"));
        }
        let lin = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        let mk = |label: &str, w: u32, h: u32, format: wgpu::TextureFormat| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let atlas = mk("glyph atlas", ATLAS, ATLAS, wgpu::TextureFormat::R8Unorm);
        let ramp = mk("ramps", 256, RAMPS, wgpu::TextureFormat::Rgba8Unorm);
        let blank_t = mk("blank", 1, 1, wgpu::TextureFormat::Rgba8Unorm);
        let atlas_view = atlas.create_view(&Default::default());
        let ramp_view = ramp.create_view(&Default::default());
        let blank = blank_t.create_view(&Default::default());
        let buf = |label: &str, size: u64, usage: wgpu::BufferUsages| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let cap = 4096;
        let insts = buf("insts", (cap * std::mem::size_of::<Inst>()) as u64, wgpu::BufferUsages::STORAGE);
        let bins_cap = 1 << 20;
        let bins = buf("bins", bins_cap as u64 * 4, wgpu::BufferUsages::STORAGE);
        let passes = buf("passes", PASS_STRIDE * MAX_PASSES, wgpu::BufferUsages::UNIFORM);
        let caches = Caches {
            device,
            queue,
            atlas,
            glyphs: HashMap::new(),
            shelf: (0, 0, 0),
            atlas_full: false,
            ramp,
            ramps: HashMap::new(),
            images: HashMap::new(),
            rasters: HashMap::new(),
            videos: HashMap::new(),
            owned: HashMap::new(),
            old_videos: Default::default(),
            #[cfg(target_vendor = "apple")]
            video_cache: None,
            frame: 0,
            stats: Stats::default(),
        };
        Ok(Gpu { caches, instance, adapter, blit: None, offscreen_prev: None, pack: None, pipeline, layout, tex_layout, lin, atlas_view, ramp_view, blank, insts, cap, bins, bins_cap, passes, size: (0, 0), target: None, ring: Vec::new(), next: 0, pool: Vec::new(), backdrop: None, timer, timed: false })
    }

    fn size_to(&mut self, w: u32, h: u32) {
        if self.size == (w, h) && self.target.is_some() {
            return;
        }
        self.size = (w, h);
        if let Some(b) = self.backdrop.as_mut() {
            b.textures = None;
        }
        let d = &self.caches.device;
        let t = d.create_texture(&wgpu::TextureDescriptor {
            label: Some("page"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let v = t.create_view(&Default::default());
        self.target = Some((t, v));
        // the readback ring keeps its buffers: a frame in flight keeps the one it copies into, and each is made to the
        // size of the frame that next takes it (submit)
        self.pool.clear();
    }

    fn group_target(&mut self, slot: usize) -> wgpu::TextureView {
        while self.pool.len() <= slot {
            let (w, h) = self.size;
            let t = self.caches.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("group"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let v = t.create_view(&Default::default());
            self.pool.push((t, v));
        }
        self.pool[slot].1.clone()
    }

    /// The backdrop textures for the page's size (made the first time, or after the size changed).
    fn backdrop_textures(&mut self) -> &BackdropTextures {
        if self.backdrop.is_none() {
            let d = &self.caches.device;
            let m = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("backdrop"), source: wgpu::ShaderSource::Wgsl(include_str!("backdrop.wgsl").into()) });
            let pipe = |entry: &str| d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("backdrop"), layout: None, module: &m, entry_point: Some(entry), compilation_options: Default::default(), cache: None });
            let fm = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("filter blur"), source: wgpu::ShaderSource::Wgsl(include_str!("blur.wgsl").into()) });
            let fpipe = |entry: &str| d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("filter blur"), layout: None, module: &fm, entry_point: Some(entry), compilation_options: Default::default(), cache: None });
            self.backdrop = Some(Backdrop { h: pipe("h"), v: pipe("v"), bh: fpipe("bh"), bv: fpipe("bv"), textures: None });
        }
        let (w, h) = self.size;
        let d = &self.caches.device;
        let b = self.backdrop.as_mut().unwrap();
        b.textures.get_or_insert_with(|| {
            let tex = |label: &'static str, format: wgpu::TextureFormat, usage: wgpu::TextureUsages, tw: u32, th: u32| {
                let t = d.create_texture(&wgpu::TextureDescriptor { label: Some(label), size: wgpu::Extent3d { width: tw, height: th, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[] });
                let v = t.create_view(&Default::default());
                (t, v)
            };
            let prev = tex("backdrop page", wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING, w, h);
            let out = tex("backdrop blurred", wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING, w, h);
            let (t, v) = tex("backdrop scratch", wgpu::TextureFormat::Rgba16Float, wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING, w, h);
            BackdropTextures { prev, out, tmp: (t, v, w, h) }
        })
    }

    /// Before the pass after a backdrop: the page so far copied, and `region` of it blurred by `sigma` into the blurred
    /// page.
    fn blur_backdrop(&mut self, e: &mut wgpu::CommandEncoder, region: [f32; 4], sigma: f32, matrix: Option<[f32; 20]>) {
        let (w, h) = self.size;
        let (prev, out) = {
            let t = self.backdrop_textures();
            (t.prev.clone(), t.out.1.clone())
        };
        let page = &self.target.as_ref().unwrap().0;
        e.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo { texture: page, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyTextureInfo { texture: &prev.0, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.blur(e, &prev.1, &out, region, sigma, matrix);
    }

    /// `region` of `src` blurred by `sigma` into the same region of `out` (which may be `src`: the first pass reads it
    /// whole into the scratch before the second writes), in two separable passes through a float scratch as tall as the
    /// region and its reach; the page's edge repeats.
    fn blur(&mut self, e: &mut wgpu::CommandEncoder, src: &wgpu::TextureView, out: &wgpu::TextureView, region: [f32; 4], sigma: f32, matrix: Option<[f32; 20]>) {
        let (w, h) = self.size;
        let r = (sigma * 3.0).ceil().max(1.0) as u32;
        let (rw, rh) = ((region[2] - region[0]) as u32, (region[3] - region[1]) as u32);
        if sigma <= 0.0 || rw == 0 || rh == 0 {
            return;
        }
        // the scratch grows to the tallest region and reach asked for
        let need = (rw.max(1), rh + 2 * r);
        self.backdrop_textures();
        let d = self.caches.device.clone();
        let b = self.backdrop.as_mut().unwrap();
        let t = b.textures.as_mut().unwrap();
        if t.tmp.2 < need.0 || t.tmp.3 < need.1 {
            let (tw, th) = (t.tmp.2.max(need.0), t.tmp.3.max(need.1));
            let tex = d.create_texture(&wgpu::TextureDescriptor { label: Some("backdrop scratch"), size: wgpu::Extent3d { width: tw, height: th, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
            let view = tex.create_view(&Default::default());
            t.tmp = (tex, view, tw, th);
        }
        let u = BlurU { origin: [region[0] as i32, region[1] as i32], size: [rw as i32, rh as i32], page: [w as i32, h as i32], r: r as i32, sigma, matrix: blur_matrix(matrix) };
        let ub = wgpu::util::DeviceExt::create_buffer_init(&d, &wgpu::util::BufferInitDescriptor { label: Some("blur"), contents: bytemuck::bytes_of(&u), usage: wgpu::BufferUsages::UNIFORM });
        let bg_h = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &b.h.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(src) },
                wgpu::BindGroupEntry { binding: 1, resource: ub.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&t.tmp.1) },
            ],
        });
        let bg_v = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &b.v.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&t.tmp.1) },
                wgpu::BindGroupEntry { binding: 1, resource: ub.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(out) },
            ],
        });
        let mut cp = e.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("blur h"), timestamp_writes: self.timer.as_mut().and_then(|t| t.writes(self.timed, "blur h")) });
        cp.set_pipeline(&b.h);
        cp.set_bind_group(0, &bg_h, &[]);
        cp.dispatch_workgroups(rw.div_ceil(16), (rh + 2 * r).div_ceil(16), 1);
        drop(cp);
        let mut cp = e.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("blur v"), timestamp_writes: self.timer.as_mut().and_then(|t| t.writes(self.timed, "blur v")) });
        cp.set_pipeline(&b.v);
        cp.set_bind_group(0, &bg_v, &[]);
        cp.dispatch_workgroups(rw.div_ceil(16), rh.div_ceil(16), 1);
    }

    /// A filter's blur of `region` of a group's target, in place, as Skia paints it (blur.wgsl).
    fn filter_blur(&mut self, e: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, region: [f32; 4], sigma: f32) {
        let Some((kernel, divider)) = skia_box_kernel(sigma) else {
            // past Skia's window (σ ≳ 135): a Gaussian, as vello_cpu falls back to one
            if f64::from(sigma) * 3.0 * (2.0 * std::f64::consts::PI).sqrt() / 4.0 + 0.5 >= 255.0 {
                self.blur(e, target, target, region, sigma, None);
            }
            return;
        };
        let (w, h) = self.size;
        let r = ((kernel.len() - 1) / 2) as u32;
        let (rw, rh) = ((region[2] - region[0]) as u32, (region[3] - region[1]) as u32);
        if rw == 0 || rh == 0 {
            return;
        }
        let need = (rw.max(1), rh + 2 * r);
        self.backdrop_textures();
        let d = self.caches.device.clone();
        let b = self.backdrop.as_mut().unwrap();
        let t = b.textures.as_mut().unwrap();
        if t.tmp.2 < need.0 || t.tmp.3 < need.1 {
            let (tw, th) = (t.tmp.2.max(need.0), t.tmp.3.max(need.1));
            let tex = d.create_texture(&wgpu::TextureDescriptor { label: Some("backdrop scratch"), size: wgpu::Extent3d { width: tw, height: th, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba16Float, usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
            let view = tex.create_view(&Default::default());
            t.tmp = (tex, view, tw, th);
        }
        let u = BoxU { origin: [region[0] as i32, region[1] as i32], size: [rw as i32, rh as i32], page: [w as i32, h as i32], r: r as i32, divider };
        let ub = wgpu::util::DeviceExt::create_buffer_init(&d, &wgpu::util::BufferInitDescriptor { label: Some("filter blur"), contents: bytemuck::bytes_of(&u), usage: wgpu::BufferUsages::UNIFORM });
        let kb = wgpu::util::DeviceExt::create_buffer_init(&d, &wgpu::util::BufferInitDescriptor { label: Some("filter blur kernel"), contents: bytemuck::cast_slice(&kernel), usage: wgpu::BufferUsages::STORAGE });
        let bg_h = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &b.bh.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(target) },
                wgpu::BindGroupEntry { binding: 1, resource: ub.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&t.tmp.1) },
                wgpu::BindGroupEntry { binding: 4, resource: kb.as_entire_binding() },
            ],
        });
        let bg_v = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &b.bv.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&t.tmp.1) },
                wgpu::BindGroupEntry { binding: 1, resource: ub.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(target) },
                wgpu::BindGroupEntry { binding: 4, resource: kb.as_entire_binding() },
            ],
        });
        let mut cp = e.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("filter blur h"), timestamp_writes: self.timer.as_mut().and_then(|t| t.writes(self.timed, "filter blur h")) });
        cp.set_pipeline(&b.bh);
        cp.set_bind_group(0, &bg_h, &[]);
        cp.dispatch_workgroups(rw.div_ceil(16), (rh + 2 * r).div_ceil(16), 1);
        drop(cp);
        let mut cp = e.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("filter blur v"), timestamp_writes: self.timer.as_mut().and_then(|t| t.writes(self.timed, "filter blur v")) });
        cp.set_pipeline(&b.bv);
        cp.set_bind_group(0, &bg_v, &[]);
        cp.dispatch_workgroups(rw.div_ceil(16), rh.div_ceil(16), 1);
    }

    /// A frame of `w` x `h` begins. Frames of another size may be in flight (several pages paint on one device: the
    /// tests, renders beside the stage): each keeps its readback buffer. It refused before, and the caller fell back to
    /// the CPU, whose pixels differ.
    pub fn begin(&mut self, w: u32, h: u32) -> Result<(), String> {
        self.size_to(w, h);
        self.caches.frame += 1;
        self.caches.stats = Stats::default();
        Ok(())
    }

    /// Composite the recorded ops and read the page back into `out` (premultiplied RGBA8, `w*h*4`).
    pub fn render(&mut self, ops: &[Op], clear: bool, out: &mut Vec<u8>) -> Result<(), String> {
        let slot = self.submit(ops, clear)?;
        self.collect(slot, out)
    }

    /// Composite the recorded ops on the GPU and start the copy out; `collect` the slot it returns for the pixels.
    pub fn submit(&mut self, ops: &[Op], clear: bool) -> Result<usize, String> {
        // a free readback buffer from the next one on, the ring growing when every one holds a frame not yet collected
        // (several callers each with frames in flight; it refused past RING, and the caller fell back to the CPU)
        let n = self.ring.len();
        let slot = match (0..n).map(|k| (self.next + k) % n.max(1)).find(|&k| self.ring[k].pending.is_none()) {
            Some(k) => k,
            None => {
                self.ring.push(Readback { buf: self.caches.device.create_buffer(&wgpu::BufferDescriptor { label: Some("readback"), size: 4, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false }), pending: None });
                n
            }
        };
        self.next = (slot + 1) % self.ring.len();
        let (w, h) = self.size;
        let need = ((w * 4).div_ceil(256) * 256 * h) as u64;
        if self.ring[slot].buf.size() != need {
            self.ring[slot].buf = self.caches.device.create_buffer(&wgpu::BufferDescriptor { label: Some("readback"), size: need, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        }
        let (mut e, prepare_ms, bin_ms) = self.encode(ops, clear)?;
        let (target, _) = self.target.as_ref().unwrap();
        let row = (w * 4).div_ceil(256) * 256;
        e.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &self.ring[slot].buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let idx = self.caches.queue.submit([e.finish()]);
        raea_gputrace::submitted(&self.caches.queue, "quads", "quads frame (read back)");
        if let Some(t) = self.timer.as_mut() {
            t.submitted(self.timed);
        }
        let st = &mut self.caches.stats;
        (st.prepare_ms, st.bin_ms) = (prepare_ms, bin_ms);
        self.ring[slot].pending = Some((idx, w, h, *st));
        self.caches.evict();
        Ok(slot)
    }

    /// Composite the recorded ops and present the page on `frame` (a drawable of a surface `configure` set up), scaled
    /// to fit and centred: what a window shows, with no readback.
    pub fn present(&mut self, ops: &[Op], clear: bool, frame: wgpu::SurfaceTexture, transparent: bool) -> Result<bool, String> {
        let (e, prepare_ms, bin_ms) = self.encode(ops, clear)?;
        self.show(e, frame, transparent);
        let st = &mut self.caches.stats;
        (st.prepare_ms, st.bin_ms) = (prepare_ms, bin_ms);
        self.caches.evict();
        Ok(true)
    }

    /// A page painted on the CPU (a frame the compositor can't do: a backdrop filter, a blend over the page) presented
    /// on `frame` as `present` would: premultiplied RGBA8, `w*h*4`, the page target's size.
    pub fn present_pixels(&mut self, px: &[u8], frame: wgpu::SurfaceTexture, transparent: bool) -> Result<bool, String> {
        let (w, h) = self.size;
        if px.len() != (w * h * 4) as usize {
            return Err(format!("{} bytes for a {w}x{h} page", px.len()));
        }
        let (target, _) = self.target.as_ref().unwrap();
        self.caches.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            px,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: None },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let e = self.caches.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("quads present") });
        self.show(e, frame, transparent);
        Ok(true)
    }

    /// The page target drawn onto `frame` (scaled to fit, centred), submitted with `e` and presented.
    fn show(&mut self, mut e: wgpu::CommandEncoder, frame: wgpu::SurfaceTexture, transparent: bool) {
        let view = frame.texture.create_view(&Default::default());
        let format = frame.texture.format();
        let (sw, sh) = (frame.texture.width() as f32, frame.texture.height() as f32);
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        let k = (sw / w).min(sh / h);
        let (dw, dh) = (w * k, h * k);
        self.blit_for(format);
        let blit = self.blit.as_ref().unwrap();
        let page = self.target.as_ref().unwrap().0.create_view(&Default::default());
        let bg = self.caches.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &blit.1,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&page) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.lin) }],
        });
        {
            let mut rp = e.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("present"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(if *TINT { wgpu::Color { r: 1.0, g: 0.0, b: 1.0, a: 1.0 } } else if transparent { wgpu::Color::TRANSPARENT } else { wgpu::Color::BLACK }), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rp.set_viewport((sw - dw) / 2.0, (sh - dh) / 2.0, dw, dh, 0.0, 1.0);
            rp.set_pipeline(&blit.0);
            rp.set_bind_group(0, &bg, &[]);
            rp.draw(0..3, 0..1);
        }
        let read = READ.as_ref().filter(|(_, n)| PRESENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1 == *n);
        let want = WANT.swap(false, std::sync::atomic::Ordering::SeqCst);
        let buf = (read.is_some() || want).then(|| {
            let (tw, th) = (frame.texture.width(), frame.texture.height());
            let row = (tw * 4).div_ceil(256) * 256;
            let b = self.caches.device.create_buffer(&wgpu::BufferDescriptor { label: Some("stage read"), size: (row * th) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
            e.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo { texture: &frame.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyBufferInfo { buffer: &b, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None } },
                wgpu::Extent3d { width: tw, height: th, depth_or_array_layers: 1 },
            );
            (b, tw, th, row)
        });
        self.caches.queue.submit([e.finish()]);
        raea_gputrace::submitted(&self.caches.queue, "quads", "quads present");
        if let Some(t) = self.timer.as_mut() {
            t.submitted(self.timed);
        }
        frame.present();
        if let Some((b, tw, th, row)) = buf {
            b.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            let _ = self.caches.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
            let data = b.slice(..).get_mapped_range();
            let bgra = matches!(format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
            let mut px = Vec::with_capacity((tw * th * 4) as usize);
            for y in 0..th as usize {
                for p in data[y * row as usize..y * row as usize + tw as usize * 4].chunks_exact(4) {
                    px.extend_from_slice(&if bgra { [p[2], p[1], p[0], p[3]] } else { [p[0], p[1], p[2], p[3]] });
                }
            }
            drop(data);
            if let Some((path, _)) = read {
                let _ = image::save_buffer(path, &px, tw, th, image::ColorType::Rgba8);
                eprintln!("raea stage: presented frame read to {path} ({tw}x{th}, {format:?})");
            }
            if want {
                *TAKEN.lock().unwrap() = Some((tw, th, px));
            }
        }
    }

    /// Composite the recorded ops and copy the page into texture `id` (copy_page_to): a page for another page to draw.
    pub fn composite_to_texture(&mut self, ops: &[Op], clear: bool, id: u64) -> Result<(), String> {
        let (mut e, prepare_ms, bin_ms) = self.encode(ops, clear)?;
        let target = self.target.as_ref().unwrap().0.clone();
        self.caches.copy_page_to(&mut e, id, &target);
        self.caches.queue.submit([e.finish()]);
        raea_gputrace::submitted(&self.caches.queue, "quads", "quads texture");
        if let Some(t) = self.timer.as_mut() {
            t.submitted(self.timed);
        }
        let st = &mut self.caches.stats;
        (st.prepare_ms, st.bin_ms) = (prepare_ms, bin_ms);
        self.caches.evict();
        Ok(())
    }

    /// Composite the recorded ops and pack the page into `dst` (premultiplied RGBA8, `w*h*4`, rows packed): a page
    /// for a kernel on this device to read (rdb's direct render composites it), with nothing read back.
    pub fn composite_to_buffer(&mut self, ops: &[Op], clear: bool, dst: &wgpu::Buffer) -> Result<(), String> {
        let (mut e, prepare_ms, bin_ms) = self.encode(ops, clear)?;
        self.pack_into(&mut e, dst);
        self.caches.queue.submit([e.finish()]);
        raea_gputrace::submitted(&self.caches.queue, "quads", "quads page to buffer");
        if let Some(t) = self.timer.as_mut() {
            t.submitted(self.timed);
        }
        let st = &mut self.caches.stats;
        (st.prepare_ms, st.bin_ms) = (prepare_ms, bin_ms);
        self.caches.evict();
        Ok(())
    }

    /// Pixels painted elsewhere (a frame the compositor can't do, painted on the CPU) into `dst`, as
    /// `composite_to_buffer` leaves a page.
    pub fn write_buffer(&self, px: &[u8], dst: &wgpu::Buffer) {
        self.caches.queue.write_buffer(dst, 0, px);
    }

    /// The page target packed into `dst` by a compute pass (a texture's rows copy out 256-byte aligned; a kernel reads
    /// packed rows).
    fn pack_into(&mut self, e: &mut wgpu::CommandEncoder, dst: &wgpu::Buffer) {
        let d = &self.caches.device;
        let pipe = self.pack.get_or_insert_with(|| {
            let m = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("quads pack"), source: wgpu::ShaderSource::Wgsl(PACK_WGSL.into()) });
            d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("quads pack"), layout: None, module: &m, entry_point: Some("main"), compilation_options: Default::default(), cache: None })
        });
        let (w, h) = self.size;
        let page = &self.target.as_ref().unwrap().1;
        let bg = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("quads pack"),
            layout: &pipe.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(page) }, wgpu::BindGroupEntry { binding: 1, resource: dst.as_entire_binding() }],
        });
        let mut cp = e.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("quads pack"), timestamp_writes: None });
        cp.set_pipeline(pipe);
        cp.set_bind_group(0, &bg, &[]);
        cp.dispatch_workgroups(w.div_ceil(16), h.div_ceil(16), 1);
    }

    /// Composite the recorded ops for a window that gave no drawable (hidden or covered): the work, nothing shown.
    pub fn composite_unshown(&mut self, ops: &[Op], clear: bool) -> bool {
        if let Ok((e, _, _)) = self.encode(ops, clear) {
            self.caches.queue.submit([e.finish()]);
            raea_gputrace::submitted(&self.caches.queue, "quads", "quads frame");
            if let Some(t) = self.timer.as_mut() {
                t.submitted(self.timed);
            }
        }
        self.caches.evict();
        false
    }

    /// Composite the recorded ops and wait for the GPU, showing nothing: what a frame costs, for a run with no window
    /// to show (the playtest harness, --offscreen).
    pub fn composite_only(&mut self, ops: &[Op], clear: bool) -> Result<(), String> {
        let (e, prepare_ms, bin_ms) = self.encode(ops, clear)?;
        let idx = self.caches.queue.submit([e.finish()]);
        raea_gputrace::submitted(&self.caches.queue, "quads", "quads stage");
        if let Some(t) = self.timer.as_mut() {
            t.submitted(self.timed);
        }
        // the frame before is waited for, not this one: two frames in flight, as a window's swapchain keeps them
        if let Some(prev) = self.offscreen_prev.replace(idx) {
            self.caches.device.poll(wgpu::PollType::Wait { submission_index: Some(prev), timeout: None }).map_err(|e| format!("GPU: {e}"))?;
        }
        let st = &mut self.caches.stats;
        (st.prepare_ms, st.bin_ms) = (prepare_ms, bin_ms);
        self.caches.evict();
        Ok(())
    }

    /// The page drawn onto a surface of `format` (a fullscreen triangle sampling the page), made once per format.
    fn blit_for(&mut self, format: wgpu::TextureFormat) {
        if self.blit.as_ref().is_none_or(|b| b.2 != format) {
            let d = &self.caches.device;
            let m = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("present"), source: wgpu::ShaderSource::Wgsl(BLIT.into()) });
            let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("present"),
                entries: &[
                    wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                    wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                ],
            });
            let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
            let pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("present"),
                layout: Some(&pl),
                vertex: wgpu::VertexState { module: &m, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
                fragment: Some(wgpu::FragmentState { module: &m, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
            self.blit = Some((pipeline, layout, format));
        }
    }

    /// Size a surface for presenting: the device's, the given size, no vsync wait when `vsync` is false.
    pub fn configure(&self, surface: &wgpu::Surface<'static>, w: u32, h: u32, vsync: bool, transparent: bool) -> Result<(), String> {
        let caps = surface.get_capabilities(&self.adapter);
        // bytes as the page holds them: a non-sRGB format, so nothing converts the page's colours on the way
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).or_else(|| caps.formats.first().copied()).ok_or("the surface takes no format")?;
        let present_mode = if vsync { wgpu::PresentMode::Fifo } else if caps.present_modes.contains(&wgpu::PresentMode::Immediate) { wgpu::PresentMode::Immediate } else { wgpu::PresentMode::Mailbox };
        surface.configure(&self.caches.device, &wgpu::SurfaceConfiguration {
            usage: if READ.is_some() || COPYABLE.load(std::sync::atomic::Ordering::Relaxed) { wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC } else { wgpu::TextureUsages::RENDER_ATTACHMENT },
            format,
            width: w.max(1),
            height: h.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            // see-through: Metal offers PostMultiplied (wgpu then sets the layer non-opaque); Core Animation still
            // reads the colours as premultiplied, which is how the page holds them
            alpha_mode: if transparent { [wgpu::CompositeAlphaMode::PreMultiplied, wgpu::CompositeAlphaMode::PostMultiplied].into_iter().find(|m| caps.alpha_modes.contains(m)).unwrap_or(caps.alpha_modes[0]) } else { caps.alpha_modes[0] },
            view_formats: vec![],
        });
        Ok(())
    }

    /// Composite the recorded ops into the page target: the work every output (readback, present) starts from.
    fn encode(&mut self, ops: &[Op], clear: bool) -> Result<(wgpu::CommandEncoder, f32, f32), String> {
        // the textures draws sample: images and fallback rasters
        fn prepare(c: &mut Caches, ops: &[Op]) {
            for op in ops {
                match op {
                    Op::Draw(_, Tex::Image(img)) => c.image(img),
                    Op::Draw(..) => {}
                    Op::Group { ops, .. } => prepare(c, ops),
                    Op::Raster { key, bbox, scene, .. } => c.raster(*key, *bbox, scene),
                    Op::Backdrop { .. } => {}
                }
            }
        }
        // the last timed frame's passes reported (their readback came in), and this one timed unless one is still out
        let _ = self.caches.device.poll(wgpu::PollType::Poll);
        self.timed = self.timer.as_mut().is_some_and(|t| t.begin());
        let t0 = std::time::Instant::now();
        prepare(&mut self.caches, ops);
        let t1 = std::time::Instant::now();
        // passes: each group's first (inner before outer), the page last
        struct Pass {
            target: Option<usize>,
            draws: Vec<(Inst, Tex)>,
            /// a backdrop before this pass (its region and blur): the target so far is copied and blurred, and the pass
            /// starts from a clear target, drawing the copy back first
            pre: Option<([f32; 4], f32, Option<[f32; 20]>)>,
            /// a filter's blur after this pass (a group's last): its region of the target blurred in place
            post: Option<([f32; 4], f32)>,
        }
        fn passes(ops: &[Op], target: Option<usize>, out: &mut Vec<Pass>) {
            let mut draws = Vec::with_capacity(ops.len());
            let mut pre = None;
            for op in ops {
                match op {
                    Op::Draw(i, t) => draws.push((*i, t.clone())),
                    Op::Group { ops, inst, slot, blur } => {
                        passes(ops, Some(*slot), out);
                        if *blur > 0.0 {
                            out.last_mut().unwrap().post = Some((inst.quad, *blur));
                        }
                        draws.push((*inst, Tex::Group(*slot)));
                    }
                    Op::Raster { key, inst, .. } => draws.push((*inst, Tex::Raster(*key))),
                    Op::Backdrop { region, sigma, matrix, prev, blurred } => {
                        out.push(Pass { target, draws: std::mem::take(&mut draws), pre: pre.take(), post: None });
                        pre = Some((*region, *sigma, *matrix));
                        draws.push((*prev, Tex::Prev));
                        draws.push((*blurred, Tex::Blur));
                    }
                }
            }
            out.push(Pass { target, draws, pre, post: None });
        }
        let mut list = Vec::new();
        passes(ops, None, &mut list);
        if list.len() as u64 > MAX_PASSES {
            return Err(format!("{} layers with opacity in one frame", list.len()));
        }
        let groups: Vec<usize> = list.iter().filter_map(|p| p.target).collect();
        let mut group_views = HashMap::new();
        for s in groups {
            group_views.insert(s, self.group_target(s));
        }
        // every texture a draw samples gets a slot in the binding array (0: blank)
        let mut slots: HashMap<(u8, u64), u32> = HashMap::new();
        let mut views: Vec<wgpu::TextureView> = vec![self.blank.clone()];
        let (w, h) = self.size;
        let (tx, ty) = (w.div_ceil(TILE), h.div_ceil(TILE));
        let mut flat: Vec<Inst> = Vec::new();
        let mut bins: Vec<u32> = Vec::new();
        let mut uniforms: Vec<u8> = Vec::new();
        for pass in &list {
            let base = flat.len() as u32;
            for (i, t) in &pass.draws {
                let mut i = *i;
                if !matches!(t, Tex::None) {
                    let k = tex_key(t);
                    let slot = match slots.get(&k) {
                        Some(s) => *s,
                        None => {
                            match t {
                                Tex::Image(img) => views.push(self.caches.images[&img.data.id()].view.clone()),
                                Tex::Raster(key) => views.push(self.caches.rasters[key].view.clone()),
                                Tex::Group(s) => views.push(group_views[s].clone()),
                                Tex::Prev => views.push(self.backdrop_textures().prev.1.clone()),
                                Tex::Blur => views.push(self.backdrop_textures().out.1.clone()),
                                Tex::Video(id) => {
                                    let Some(v) = self.caches.videos.get(id) else { return Err(format!("video {id} has no frame")) };
                                    let first = views.len();
                                    views.extend(v.views.iter().cloned());
                                    slots.insert(k, first as u32);
                                    i.pad[0] = first as f32;
                                    flat.push(i);
                                    continue;
                                }
                                Tex::None => unreachable!(),
                            }
                            slots.insert(k, views.len() as u32 - 1);
                            views.len() as u32 - 1
                        }
                    };
                    i.pad[0] = slot as f32;
                }
                flat.push(i);
            }
            if views.len() as u32 > MAX_TEX {
                return Err(format!("{} textures in one frame", views.len()));
            }
            // each instance to the 16x16 tiles its device bounds touch
            let n = (tx * ty) as usize;
            let mut per: Vec<Vec<u32>> = vec![Vec::new(); n];
            for (k, (i, _)) in pass.draws.iter().enumerate() {
                let Some([x0, y0, x1, y1]) = device_bounds(i, w, h) else { continue };
                for ty_ in y0 / TILE..=(y1 - 1) / TILE {
                    for tx_ in x0 / TILE..=(x1 - 1) / TILE {
                        per[(ty_ * tx + tx_) as usize].push(base + k as u32);
                    }
                }
            }
            let offsets = bins.len() as u32;
            let mut at = offsets + n as u32 + 1;
            for p in &per {
                bins.push(at);
                at += p.len() as u32;
            }
            bins.push(at);
            for p in per {
                bins.extend(p);
            }
            let clear_to = if pass.target.is_none() && clear && pass.pre.is_none() { [0.0, 0.0, 0.0, 1.0] } else { [0.0; 4] };
            let u = PassU { size: [w, h], tiles_x: tx, offsets, clear: clear_to };
            uniforms.extend_from_slice(bytemuck::bytes_of(&u));
            uniforms.resize(uniforms.len().div_ceil(PASS_STRIDE as usize) * PASS_STRIDE as usize, 0);
        }
        let t2 = std::time::Instant::now();
        let d = &self.caches.device;
        if flat.len() > self.cap {
            self.cap = flat.len().next_power_of_two();
            self.insts = d.create_buffer(&wgpu::BufferDescriptor { label: Some("insts"), size: (self.cap * std::mem::size_of::<Inst>()) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        }
        if bins.len() > self.bins_cap {
            self.bins_cap = bins.len().next_power_of_two();
            self.bins = d.create_buffer(&wgpu::BufferDescriptor { label: Some("bins"), size: self.bins_cap as u64 * 4, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        }
        let q = &self.caches.queue;
        if !flat.is_empty() {
            q.write_buffer(&self.insts, 0, bytemuck::cast_slice(&flat));
        }
        q.write_buffer(&self.bins, 0, bytemuck::cast_slice(&bins));
        q.write_buffer(&self.passes, 0, &uniforms);
        let mut e = d.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("quads frame") });
        let page = self.target.as_ref().unwrap().1.clone();
        for (k, pass) in list.iter().enumerate() {
            if let Some((region, sigma, matrix)) = pass.pre {
                self.blur_backdrop(&mut e, region, sigma, matrix);
            }
            let d = &self.caches.device;
            let target = match pass.target {
                Some(s) => group_views[&s].clone(),
                None => page.clone(),
            };
            // the pass's own target can't also be sampled: its slot reads blank
            let own = pass.target.and_then(|s| slots.get(&(3, s as u64)).copied());
            let refs: Vec<&wgpu::TextureView> = views.iter().enumerate().map(|(j, v)| if Some(j as u32) == own { &self.blank } else { v }).collect();
            let bg = d.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.insts.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: self.bins.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &self.passes, offset: k as u64 * PASS_STRIDE, size: std::num::NonZeroU64::new(std::mem::size_of::<PassU>() as u64) }) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.lin) },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&self.atlas_view) },
                    wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&self.ramp_view) },
                    wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&target) },
                ],
            });
            let tg = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.tex_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureViewArray(&refs) }] });
            let mut cp = e.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("quads fine"), timestamp_writes: self.timer.as_mut().and_then(|t| t.writes(self.timed, if pass.target.is_some() { "group" } else { "page" })) });
            cp.set_pipeline(&self.pipeline);
            cp.set_bind_group(0, &bg, &[]);
            cp.set_bind_group(1, &tg, &[]);
            cp.dispatch_workgroups(tx, ty, 1);
            drop(cp);
            if let Some((region, sigma)) = pass.post {
                self.filter_blur(&mut e, &target, region, sigma);
            }
        }
        if let Some(t) = self.timer.as_mut() {
            t.resolve(self.timed, &mut e);
        }
        let ms = |a: std::time::Instant, b: std::time::Instant| (b - a).as_secs_f32() * 1e3;
        Ok((e, ms(t0, t1), ms(t1, t2)))
    }

    /// A submitted frame's pixels (premultiplied RGBA8, `w*h*4`), waiting for that frame's work alone.
    pub fn collect(&mut self, slot: usize, out: &mut Vec<u8>) -> Result<(), String> {
        let t2 = std::time::Instant::now();
        let Some((idx, w, h, mut st)) = self.ring.get_mut(slot).and_then(|r| r.pending.take()) else { return Err("no frame painted in that slot".into()) };
        let buf = &self.ring[slot].buf;
        let slice = buf.slice(..);
        let (txc, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = txc.send(r);
        });
        self.caches.device.poll(wgpu::PollType::Wait { submission_index: Some(idx), timeout: None }).map_err(|e| format!("GPU: {e}"))?;
        rx.recv().map_err(|e| e.to_string())?.map_err(|e| format!("readback: {e}"))?;
        let t3 = std::time::Instant::now();
        let row = (w * 4).div_ceil(256) * 256;
        {
            let data = slice.get_mapped_range();
            out.clear();
            out.reserve((w * h * 4) as usize);
            for y in 0..h as usize {
                out.extend_from_slice(&data[y * row as usize..y * row as usize + w as usize * 4]);
            }
        }
        buf.unmap();
        let ms = |a: std::time::Instant, b: std::time::Instant| (b - a).as_secs_f32() * 1e3;
        (st.wait_ms, st.copy_ms) = (ms(t2, t3), ms(t3, std::time::Instant::now()));
        self.caches.stats = st;
        Ok(())
    }
}

/// An instance's device bounds in whole pixels, cut to its clip and the frame (None when nothing shows).
fn device_bounds(i: &Inst, w: u32, h: u32) -> Option<[u32; 4]> {
    let q = i.quad;
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in [(q[0], q[1]), (q[2], q[1]), (q[0], q[3]), (q[2], q[3])] {
        let dx = i.xf0[0] * x + i.xf0[2] * y + i.xf1[0];
        let dy = i.xf0[1] * x + i.xf0[3] * y + i.xf1[1];
        x0 = x0.min(dx);
        y0 = y0.min(dy);
        x1 = x1.max(dx);
        y1 = y1.max(dy);
    }
    let c = i.clip;
    let x0 = x0.max(c[0]).max(0.0).floor();
    let y0 = y0.max(c[1]).max(0.0).floor();
    let x1 = x1.min(c[2]).min(w as f32).ceil();
    let y1 = y1.min(c[3]).min(h as f32).ceil();
    (x1 > x0 && y1 > y0).then(|| [x0 as u32, y0 as u32, x1 as u32, y1 as u32])
}

fn tex_key(t: &Tex) -> (u8, u64) {
    match t {
        Tex::None => (0, 0),
        Tex::Image(i) => (1, i.data.id()),
        Tex::Raster(k) => (2, *k),
        Tex::Group(s) => (3, *s as u64),
        Tex::Video(id) => (4, *id),
        Tex::Prev => (5, 0),
        Tex::Blur => (6, 0),
    }
}

/// The page target (Rgba8Unorm, so a load and a pack give its bytes back exactly) into packed words.
const PACK_WGSL: &str = r#"
@group(0) @binding(0) var page: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let d = textureDimensions(page);
    if (id.x >= d.x || id.y >= d.y) { return; }
    dst[id.y * d.x + id.x] = pack4x8unorm(textureLoad(page, vec2<i32>(id.xy), 0));
}
"#;
