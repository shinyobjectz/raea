use anyrender::{ImageRenderer, RenderContext};
use debug_timer::debug_timer;
use skia_safe::{AlphaType, Color, ColorType, ImageInfo, SurfaceProps, graphics, surfaces};

use crate::{SkiaScenePainter, scene::SkiaSceneCache};

pub struct SkiaImageRenderer {
    image_info: ImageInfo,
    surface_props: SurfaceProps,
    scene_cache: SkiaSceneCache,
    /// raea: Skia on Metal (Ganesh) when built with `metal` and RAEA_SKIA_GPU is set: the same drawing, on the GPU
    #[cfg(feature = "metal")]
    gpu: Option<Gpu>,
}

/// raea: a Ganesh Metal context and the frame's render target (one per renderer, so per render thread).
#[cfg(feature = "metal")]
struct Gpu {
    ctx: skia_safe::gpu::DirectContext,
    surface: skia_safe::Surface,
    _queue: metal::CommandQueue,
    _device: metal::Device,
}

#[cfg(feature = "metal")]
impl Gpu {
    fn new(info: &ImageInfo, props: &SurfaceProps) -> Option<Gpu> {
        use foreign_types_shared::ForeignType;
        use skia_safe::gpu;
        let device = metal::Device::system_default()?;
        let queue = device.new_command_queue();
        // SAFETY: the device and queue are kept alive in the struct for as long as the context
        let backend = unsafe { gpu::mtl::BackendContext::new(device.as_ptr() as gpu::mtl::Handle, queue.as_ptr() as gpu::mtl::Handle) };
        let mut ctx = gpu::direct_contexts::make_metal(&backend, None)?;
        let surface = gpu::surfaces::render_target(&mut ctx, gpu::Budgeted::Yes, info, None, gpu::SurfaceOrigin::TopLeft, Some(props), false, None)?;
        Some(Gpu { ctx, surface, _queue: queue, _device: device })
    }
}

#[cfg(feature = "metal")]
fn gpu_wanted() -> bool {
    std::env::var_os("RAEA_SKIA_GPU").is_some_and(|v| v != "0")
}

impl RenderContext for SkiaImageRenderer {}
impl ImageRenderer for SkiaImageRenderer {
    type ScenePainter<'a>
        = SkiaScenePainter<'a>
    where
        Self: 'a;

    fn new(width: u32, height: u32) -> Self {
        graphics::set_font_cache_count_limit(100);
        graphics::set_typeface_cache_count_limit(100);
        graphics::set_resource_cache_total_bytes_limit(10485760);

        Self {
            image_info: ImageInfo::new(
                (width as i32, height as i32),
                ColorType::RGBA8888,
                AlphaType::Premul,
                None,
            ),
            // raea: Chrome on macOS blends glyph coverage linearly (no text gamma or contrast
            // boost); Skia's default preblend left text ~10% lighter than Chrome's
            surface_props: SurfaceProps::new_with_text_properties(Default::default(), skia_safe::PixelGeometry::Unknown, 0.0, 1.0),
            scene_cache: SkiaSceneCache::default(),
            #[cfg(feature = "metal")]
            gpu: None,
        }
        .with_gpu()
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.image_info = ImageInfo::new(
            (width as i32, height as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        #[cfg(feature = "metal")]
        if self.gpu.is_some() {
            self.gpu = Gpu::new(&self.image_info, &self.surface_props);
        }
    }

    fn reset(&mut self) {}

    fn render_to_vec<F: FnOnce(&mut Self::ScenePainter<'_>)>(
        &mut self,
        draw_fn: F,
        buffer: &mut Vec<u8>,
    ) {
        debug_timer!(timer, feature = "log_frame_times");

        buffer.resize(self.image_info.compute_min_byte_size(), 0);

        #[cfg(feature = "metal")]
        if self.gpu.is_some() {
            self.render_gpu(draw_fn, &mut buffer[..]);
            return;
        }

        let mut surface = surfaces::wrap_pixels(
            &self.image_info,
            &mut buffer[..],
            None,
            Some(&self.surface_props),
        )
        .unwrap();

        // Clear surface with transparent background to allow transparency in rendered images
        surface.canvas().clear(Color::TRANSPARENT);

        draw_fn(&mut SkiaScenePainter {
            inner: surface.canvas(),
            cache: &mut self.scene_cache,
        });
        timer.record_time("render");
        drop(surface);
        unpremultiply(&mut buffer[..]);

        self.scene_cache.next_gen();
        timer.record_time("cache next gen");

        timer.print_times("skia_raster: ");
    }

    fn render<F: FnOnce(&mut Self::ScenePainter<'_>)>(&mut self, draw_fn: F, buffer: &mut [u8]) {
        debug_timer!(timer, feature = "log_frame_times");

        #[cfg(feature = "metal")]
        if self.gpu.is_some() {
            self.render_gpu(draw_fn, buffer);
            return;
        }

        let mut surface = surfaces::wrap_pixels(
            &self.image_info,
            &mut buffer[..],
            None,
            Some(&self.surface_props),
        )
        .unwrap();

        // Clear surface with transparent background to allow transparency in rendered images
        surface.canvas().clear(Color::TRANSPARENT);

        draw_fn(&mut SkiaScenePainter {
            inner: surface.canvas(),
            cache: &mut self.scene_cache,
        });
        timer.record_time("render");
        drop(surface);
        unpremultiply(buffer);

        self.scene_cache.next_gen();
        timer.record_time("cache next gen");

        timer.print_times("skia_raster: ");
    }
}

/// The surface is premultiplied (an opaque surface would drop transparency); callers get straight
/// RGBA like the vello_cpu renderer's output. Opaque pixels, the common case, are left as they are.
fn unpremultiply(buffer: &mut [u8]) {
    for px in buffer.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a == 255 || a == 0 {
            continue;
        }
        for c in &mut px[..3] {
            *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
        }
    }
}

impl SkiaImageRenderer {
    /// raea: the GPU context when it's wanted and a Metal device exists (falls back to the CPU otherwise).
    fn with_gpu(self) -> Self {
        #[cfg(feature = "metal")]
        {
            let mut this = self;
            if gpu_wanted() {
                this.gpu = Gpu::new(&this.image_info, &this.surface_props);
            }
            this
        }
        #[cfg(not(feature = "metal"))]
        self
    }

    /// raea: draw on the GPU surface, then read the frame back (premultiplied RGBA8, as the CPU path leaves it before
    /// unpremultiplying).
    #[cfg(feature = "metal")]
    fn render_gpu<F: FnOnce(&mut SkiaScenePainter<'_>)>(&mut self, draw_fn: F, buffer: &mut [u8]) {
        let info = self.image_info.clone();
        let gpu = self.gpu.as_mut().expect("render_gpu needs the GPU");
        let canvas = gpu.surface.canvas();
        canvas.clear(Color::TRANSPARENT);
        draw_fn(&mut SkiaScenePainter { inner: canvas, cache: &mut self.scene_cache });
        gpu.ctx.flush_and_submit();
        let row = info.min_row_bytes();
        gpu.surface.read_pixels(&info, buffer, row, (0, 0));
        unpremultiply(buffer);
        self.scene_cache.next_gen();
    }
}
