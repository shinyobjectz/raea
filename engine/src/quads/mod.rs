//! GPU paint: a page painted as instanced quads on one wgpu device instead of by vello_cpu (engine/docs and the
//! One-renderer plan, phase 2). `record` sorts Blitz's paint calls into quads, or into captured commands vello_cpu
//! rasterizes exactly; `gpu` draws them and reads the page back in vello_cpu's layout, so everything downstream is
//! unchanged. Off with RAEA_PAINT=cpu; a frame the GPU path can't paint (a backdrop filter, no adapter) goes to the
//! CPU on its own.

pub mod gpu;
pub mod record;
pub mod video;
pub mod window;
pub use window::QuadsWindowRenderer;

use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use anyrender::recording::{RenderCommand, Scene};
use anyrender::Paint;
use blitz_dom::BaseDocument;

static GPU: Mutex<Option<Result<gpu::Gpu, String>>> = Mutex::new(None);

/// Whether pages paint on the GPU: on unless RAEA_PAINT=cpu.
pub fn enabled() -> bool {
    std::env::var("RAEA_PAINT").map(|v| v != "cpu").unwrap_or(true)
}

/// What the last frame cost, for `RAEA_QUADS_STATS` and the tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct FrameStats {
    pub ops: usize,
    pub fallbacks: usize,
    pub groups: usize,
    pub caches: gpu::Stats,
    /// the frame reached the screen (false: the window was hidden, or the stage is offscreen)
    pub shown: bool,
    /// milliseconds: Blitz's paint calls recorded (rasterizing what's new), then the GPU's composite and readback (the
    /// submit, plus the wait in collect)
    pub record_ms: f64,
    pub gpu_ms: f64,
}

/// Paint `doc` at `scale` into `out` (premultiplied RGBA8, `w*h*4`, over black when `clear`). Err when the GPU path
/// can't: the caller paints on the CPU.
pub fn paint(doc: &mut BaseDocument, w: u32, h: u32, scale: f64, clear: bool, out: &mut Vec<u8>) -> Result<FrameStats, String> {
    paint_with(w, h, clear, out, |s| blitz_paint::paint_scene(s, doc, scale, w, h, 0, 0))
}

/// Paint whatever `draw` draws (any PaintScene caller) on the GPU.
pub fn paint_with(w: u32, h: u32, clear: bool, out: &mut Vec<u8>, draw: impl FnMut(&mut record::Recorder)) -> Result<FrameStats, String> {
    let t = submit_with(w, h, clear, draw)?;
    t.collect(out)
}

/// `paint`, without waiting: the document is recorded now (it can change as soon as this returns) and the GPU works
/// while the caller goes on; `Ticket::collect` hands over the pixels. Up to three frames can be in flight.
pub fn submit(doc: &mut BaseDocument, w: u32, h: u32, scale: f64, clear: bool) -> Result<Ticket, String> {
    submit_with(w, h, clear, |s| blitz_paint::paint_scene(s, doc, scale, w, h, 0, 0))
}

/// A frame submitted to the GPU, to collect.
#[must_use]
pub struct Ticket {
    slot: usize,
    w: u32,
    h: u32,
    stats: FrameStats,
}

impl Ticket {
    /// The frame's pixels (premultiplied RGBA8), waiting for its work alone.
    pub fn collect(self, out: &mut Vec<u8>) -> Result<FrameStats, String> {
        if self.w == 0 || self.h == 0 {
            out.clear();
            return Ok(self.stats);
        }
        let t0 = std::time::Instant::now();
        let mut lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
        let g = lock.as_mut().and_then(|g| g.as_mut().ok()).ok_or("no GPU")?;
        g.collect(self.slot, out)?;
        let mut stats = self.stats;
        stats.caches = g.caches.stats;
        stats.gpu_ms += t0.elapsed().as_secs_f64() * 1e3;
        if std::env::var_os("RAEA_QUADS_STATS").is_some() {
            eprintln!("quads: {stats:?}");
        }
        Ok(stats)
    }
}

pub fn submit_with(w: u32, h: u32, clear: bool, mut draw: impl FnMut(&mut record::Recorder)) -> Result<Ticket, String> {
    if w == 0 || h == 0 {
        return Ok(Ticket { slot: 0, w, h, stats: FrameStats::default() });
    }
    let mut lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let g = lock.get_or_insert_with(gpu::Gpu::new).as_mut().map_err(|e| e.clone())?;
    g.begin(w, h)?;
    for attempt in 0..2 {
        let t0 = std::time::Instant::now();
        let mut rec = record::Recorder::new(&mut g.caches, w, h);
        // backdrop blurs on the GPU, as the live stage and windows draw them: vello_cpu, the fallback, ignores backdrop
        // filters (anyrender_vello_cpu), so a render of a glass sheet came out unblurred while play blurred it
        rec.backdrop_gpu = true;
        draw(&mut rec);
        if rec.cpu {
            return Err("a backdrop filter or a blend over the page: this frame paints on the CPU".into());
        }
        let (ops, fallbacks, groups) = (std::mem::take(&mut rec.ops), rec.fallbacks, rec.groups);
        drop(rec);
        if g.caches.atlas_full {
            g.caches.clear_atlas();
            if attempt == 0 {
                continue;
            }
            return Err("the glyph atlas can't hold this frame's text".into());
        }
        let t1 = std::time::Instant::now();
        let slot = g.submit(&ops, clear)?;
        let stats = FrameStats { shown: false, ops: ops.len(), fallbacks, groups, caches: g.caches.stats, record_ms: (t1 - t0).as_secs_f64() * 1e3, gpu_ms: t1.elapsed().as_secs_f64() * 1e3 };
        count_cpu(&stats);
        return Ok(Ticket { slot, w, h, stats });
    }
    unreachable!()
}

/// A window's surface on the paint device: where `present` puts a page (the live stage, `raea-motion play`, void).
pub struct Stage {
    /// None: an offscreen stage (composited, not shown: the harness without a window)
    surface: Option<wgpu::Surface<'static>>,
    pub w: u32,
    pub h: u32,
    pub vsync: bool,
    /// see-through where the page paints nothing (`set_transparent`): what's under the window's layer shows there
    pub transparent: bool,
}

/// A stage over a window (anything wgpu takes as a surface target: a winit window).
pub fn stage(target: impl Into<wgpu::SurfaceTarget<'static>>, w: u32, h: u32, vsync: bool) -> Result<Stage, String> {
    let mut lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let g = lock.get_or_insert_with(gpu::Gpu::new).as_mut().map_err(|e| e.clone())?;
    let surface = g.instance.create_surface(target).map_err(|e| format!("surface: {e}"))?;
    g.configure(&surface, w, h, vsync, false)?;
    Ok(Stage { surface: Some(surface), w, h, vsync, transparent: false })
}

/// A stage with no window: frames are composited on the GPU and waited for, not shown (what a frame costs).
pub fn stage_offscreen(w: u32, h: u32) -> Result<Stage, String> {
    let mut lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
    lock.get_or_insert_with(gpu::Gpu::new).as_mut().map_err(|e| e.clone())?;
    Ok(Stage { surface: None, w, h, vsync: false, transparent: false })
}

/// A stage over a raw layer (void's CAMetalLayer in its window).
///
/// # Safety
/// The target must outlive the stage.
pub unsafe fn stage_raw(target: wgpu::SurfaceTargetUnsafe, w: u32, h: u32, vsync: bool) -> Result<Stage, String> {
    let mut lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let g = lock.get_or_insert_with(gpu::Gpu::new).as_mut().map_err(|e| e.clone())?;
    let surface = unsafe { g.instance.create_surface_unsafe(target) }.map_err(|e| format!("surface: {e}"))?;
    g.configure(&surface, w, h, vsync, false)?;
    Ok(Stage { surface: Some(surface), w, h, vsync, transparent: false })
}

/// A stage over a CAMetalLayer of its own, in no window: presents as void's stage does (a drawable per frame, the
/// same blit), for the harness to check what a layer is given without taking anyone's screen (RAEA_STAGE_READ).
#[cfg(target_vendor = "apple")]
pub fn stage_layer(w: u32, h: u32) -> Result<Stage, String> {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    unsafe {
        let layer: *mut AnyObject = msg_send![class!(CAMetalLayer), layer];
        let _: *mut AnyObject = msg_send![layer, retain];
        let _: () = msg_send![layer, setOpaque: true];
        stage_raw(wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(layer as *mut std::ffi::c_void), w, h, true)
    }
}

impl Stage {
    /// See-through where the page paints nothing (a window's chrome over a stage under it): the layer is no longer
    /// opaque, the page isn't cleared to black, and the present clears to transparent. The page stays premultiplied,
    /// as Core Animation composites a layer's contents.
    pub fn set_transparent(&mut self, on: bool) -> Result<(), String> {
        self.transparent = on;
        let (w, h) = (self.w, self.h);
        self.resize(w, h)
    }

    /// The window's new size in pixels.
    pub fn resize(&mut self, w: u32, h: u32) -> Result<(), String> {
        let lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
        let g = lock.as_ref().and_then(|g| g.as_ref().ok()).ok_or("no GPU")?;
        if let Some(surface) = &self.surface {
            g.configure(surface, w, h, self.vsync, self.transparent)?;
        }
        (self.w, self.h) = (w, h);
        Ok(())
    }
}

/// A page of `w` x `h` presented on `stage`, scaled to fit and centred: the live path, no readback.
pub fn present_with(stage: &Stage, w: u32, h: u32, clear: bool, mut draw: impl FnMut(&mut record::Recorder)) -> Result<FrameStats, String> {
    // the drawable first, without the device's lock: with vsync this waits up to a refresh, and a window's chrome and
    // the stage under it (two surfaces, one device) would each wait out the other's refresh while holding it
    let frame = match &stage.surface {
        Some(surface) => match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
            // a hidden or covered window: the page is still composited (the work is the same), nothing shown
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => None,
            other => return Err(format!("the surface gave no frame: {other:?}")),
        },
        None => None,
    };
    let mut frame = frame;
    let mut lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let g = lock.get_or_insert_with(gpu::Gpu::new).as_mut().map_err(|e| e.clone())?;
    g.begin(w, h)?;
    for attempt in 0..2 {
        let t0 = std::time::Instant::now();
        let mut rec = record::Recorder::new(&mut g.caches, w, h);
        rec.backdrop_gpu = true;
        draw(&mut rec);
        if rec.cpu {
            CPU_FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(NEEDS_CPU.into());
        }
        let (ops, fallbacks, groups) = (std::mem::take(&mut rec.ops), rec.fallbacks, rec.groups);
        drop(rec);
        if g.caches.atlas_full {
            g.caches.clear_atlas();
            if attempt == 0 {
                continue;
            }
            return Err("the glyph atlas can't hold this frame's text".into());
        }
        let t1 = std::time::Instant::now();
        let shown = match (frame.take(), &stage.surface) {
            (Some(f), _) => g.present(&ops, clear && !stage.transparent, f, stage.transparent)?,
            (None, Some(_)) => g.composite_unshown(&ops, clear && !stage.transparent),
            (None, None) => {
                g.composite_only(&ops, clear)?;
                false
            }
        };
        let stats = FrameStats { shown, ops: ops.len(), fallbacks, groups, caches: g.caches.stats, record_ms: (t1 - t0).as_secs_f64() * 1e3, gpu_ms: t1.elapsed().as_secs_f64() * 1e3 };
        count_cpu(&stats);
        return Ok(stats);
    }
    unreachable!()
}

/// Where the CPU still paints, counted over the process (the playtest reports them; the goal is zeros): frames that
/// went to vello_cpu whole, stretches recorded for vello_cpu, those rasterized afresh, and those reused from the cache.
pub static CPU_FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static CPU_STRETCHES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static CPU_RASTERS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static CPU_REUSED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn count_cpu(stats: &FrameStats) {
    use std::sync::atomic::Ordering::Relaxed;
    CPU_STRETCHES.fetch_add(stats.fallbacks as u64, Relaxed);
    CPU_RASTERS.fetch_add(stats.caches.raster_misses as u64, Relaxed);
    CPU_REUSED.fetch_add(stats.caches.raster_hits as u64, Relaxed);
}

/// What `present_with` says of a frame the compositor can't do (a backdrop filter, a blend over the page): paint it
/// on the CPU and hand the pixels to `present_rgba`.
pub const NEEDS_CPU: &str = "a backdrop filter or a blend over the page: this frame needs the CPU";

/// A page painted on the CPU (premultiplied RGBA8, `w` x `h`) presented on `stage` as `present_with` presents.
pub fn present_rgba(stage: &Stage, w: u32, h: u32, px: &[u8]) -> Result<bool, String> {
    let frame = match &stage.surface {
        Some(surface) => match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => return Ok(false),
            other => return Err(format!("the surface gave no frame: {other:?}")),
        },
        None => return Ok(false),
    };
    let mut lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let g = lock.get_or_insert_with(gpu::Gpu::new).as_mut().map_err(|e| e.clone())?;
    g.begin(w, h)?;
    g.present_pixels(px, frame, stage.transparent)
}

/// A document painted into texture `id` instead of a window (`Paint::Resource` id draws it): the stage's page for a
/// window's chrome to draw in its own frame, so the chrome's glass blurs what's under it. `under`: a texture under the
/// page (a live 3D scene).
pub fn paint_texture(doc: &mut BaseDocument, w: u32, h: u32, scale: f64, clear: bool, id: u64, under: Option<u64>) -> Result<FrameStats, String> {
    let mut lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let g = lock.get_or_insert_with(gpu::Gpu::new).as_mut().map_err(|e| e.clone())?;
    g.begin(w, h)?;
    for attempt in 0..2 {
        let t0 = std::time::Instant::now();
        let mut rec = record::Recorder::new(&mut g.caches, w, h);
        rec.backdrop_gpu = true;
        if let Some(u) = under {
            let brush = peniko::ImageBrush { image: anyrender::ResourceId::from_ffi(u), sampler: peniko::ImageSampler::default() };
            anyrender::PaintScene::fill(&mut rec, peniko::Fill::NonZero, peniko::kurbo::Affine::IDENTITY, Paint::Resource(brush), None, &peniko::kurbo::Rect::new(0.0, 0.0, w as f64, h as f64));
        }
        blitz_paint::paint_scene(&mut rec, doc, scale, w, h, 0, 0);
        if rec.cpu {
            CPU_FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(NEEDS_CPU.into());
        }
        let (ops, fallbacks, groups) = (std::mem::take(&mut rec.ops), rec.fallbacks, rec.groups);
        drop(rec);
        if g.caches.atlas_full {
            g.caches.clear_atlas();
            if attempt == 0 {
                continue;
            }
            return Err("the glyph atlas can't hold this frame's text".into());
        }
        let t1 = std::time::Instant::now();
        g.composite_to_texture(&ops, clear, id)?;
        let stats = FrameStats { shown: false, ops: ops.len(), fallbacks, groups, caches: g.caches.stats, record_ms: (t1 - t0).as_secs_f64() * 1e3, gpu_ms: t1.elapsed().as_secs_f64() * 1e3 };
        count_cpu(&stats);
        return Ok(stats);
    }
    unreachable!()
}

/// A page on the paint device as a buffer (`page_buffer`), for crates that don't name wgpu themselves.
pub type PageBuffer = wgpu::Buffer;

/// A buffer on the paint device for `buffer_paint` to fill: a page, premultiplied RGBA8, `w*h*4`, packed rows.
pub fn page_buffer(w: u32, h: u32) -> Result<wgpu::Buffer, String> {
    let mut lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let g = lock.get_or_insert_with(gpu::Gpu::new).as_mut().map_err(|e| e.clone())?;
    Ok(g.caches.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("quads page buffer"),
        size: (w as u64 * h as u64 * 4).max(4),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    }))
}

/// A document painted into `dst` (`page_buffer`), on the paint device, with no readback: a render's page handed to a
/// kernel on the same device (rdb's direct render) as it is. Err(NEEDS_CPU) for a frame the compositor can't do: the
/// caller paints it on the CPU and puts it there with `write_buffer`.
/// `under`: a texture under the page (a 3D scene drawn for this frame, `set_texture`), filling it, as `present_under`
/// draws play's.
pub fn buffer_paint(doc: &mut BaseDocument, w: u32, h: u32, scale: f64, clear: bool, under: Option<u64>, dst: &wgpu::Buffer) -> Result<FrameStats, String> {
    buffer_paint_with(w, h, clear, dst, |s| {
        if let Some(u) = under {
            let brush = peniko::ImageBrush { image: anyrender::ResourceId::from_ffi(u), sampler: peniko::ImageSampler::default() };
            anyrender::PaintScene::fill(s, peniko::Fill::NonZero, peniko::kurbo::Affine::IDENTITY, Paint::Resource(brush), None, &peniko::kurbo::Rect::new(0.0, 0.0, w as f64, h as f64));
        }
        blitz_paint::paint_scene(s, doc, scale, w, h, 0, 0)
    })
}

/// `buffer_paint` for whatever `draw` draws (any PaintScene caller).
pub fn buffer_paint_with(w: u32, h: u32, clear: bool, dst: &wgpu::Buffer, mut draw: impl FnMut(&mut record::Recorder)) -> Result<FrameStats, String> {
    let mut lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let g = lock.get_or_insert_with(gpu::Gpu::new).as_mut().map_err(|e| e.clone())?;
    g.begin(w, h)?;
    for attempt in 0..2 {
        let t0 = std::time::Instant::now();
        let mut rec = record::Recorder::new(&mut g.caches, w, h);
        rec.backdrop_gpu = true;
        draw(&mut rec);
        if rec.cpu {
            CPU_FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(NEEDS_CPU.into());
        }
        let (ops, fallbacks, groups) = (std::mem::take(&mut rec.ops), rec.fallbacks, rec.groups);
        drop(rec);
        if g.caches.atlas_full {
            g.caches.clear_atlas();
            if attempt == 0 {
                continue;
            }
            return Err("the glyph atlas can't hold this frame's text".into());
        }
        let t1 = std::time::Instant::now();
        g.composite_to_buffer(&ops, clear, dst)?;
        let stats = FrameStats { shown: false, ops: ops.len(), fallbacks, groups, caches: g.caches.stats, record_ms: (t1 - t0).as_secs_f64() * 1e3, gpu_ms: t1.elapsed().as_secs_f64() * 1e3 };
        count_cpu(&stats);
        return Ok(stats);
    }
    unreachable!()
}

/// A `page_buffer`'s bytes, read back (waits for the device): for checks.
pub fn read_buffer(b: &PageBuffer) -> Result<Vec<u8>, String> {
    let (_, _, d, q) = handles()?;
    let rb = d.create_buffer(&wgpu::BufferDescriptor { label: Some("page buffer read"), size: b.size(), usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    let mut e = d.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("page buffer read") });
    e.copy_buffer_to_buffer(b, 0, &rb, 0, b.size());
    q.submit([e.finish()]);
    rb.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    d.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| format!("GPU: {e}"))?;
    let px = rb.slice(..).get_mapped_range().to_vec();
    Ok(px)
}

/// Pixels (premultiplied RGBA8, `w*h*4`) into a `page_buffer`, ordered with the paint device's work.
pub fn write_buffer(px: &[u8], dst: &wgpu::Buffer) -> Result<(), String> {
    let mut lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let g = lock.get_or_insert_with(gpu::Gpu::new).as_mut().map_err(|e| e.clone())?;
    g.write_buffer(px, dst);
    Ok(())
}

/// A document presented on `stage` over texture `under` (`set_texture`), which fills the page: a live 3D scene under
/// the page's elements, as a render draws its 3D under the page.
pub fn present_under(doc: &mut BaseDocument, stage: &Stage, w: u32, h: u32, scale: f64, clear: bool, under: u64) -> Result<FrameStats, String> {
    present_with(stage, w, h, clear, |s| {
        let brush = peniko::ImageBrush { image: anyrender::ResourceId::from_ffi(under), sampler: peniko::ImageSampler::default() };
        anyrender::PaintScene::fill(s, peniko::Fill::NonZero, peniko::kurbo::Affine::IDENTITY, Paint::Resource(brush), None, &peniko::kurbo::Rect::new(0.0, 0.0, w as f64, h as f64));
        blitz_paint::paint_scene(s, doc, scale, w, h, 0, 0)
    })
}

/// A document presented on `stage`.
pub fn present(doc: &mut BaseDocument, stage: &Stage, w: u32, h: u32, scale: f64, clear: bool) -> Result<FrameStats, String> {
    present_with(stage, w, h, clear, |s| blitz_paint::paint_scene(s, doc, scale, w, h, 0, 0))
}

/// The paint device's wgpu handles (made now if they aren't yet): what Bevy's renderer is given so the pages and the
/// 3D share one device (rdb's engine, RenderCreation::manual).
pub fn handles() -> Result<(wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue), String> {
    let mut lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let g = lock.get_or_insert_with(gpu::Gpu::new).as_ref().map_err(|e| e.clone())?;
    Ok((g.instance.clone(), g.adapter.clone(), g.caches.device.clone(), g.caches.queue.clone()))
}

/// What a window was given, for checks: surfaces made from now can be read, and the next frame presented on any of
/// them is copied out for `take_present` (RGBA as the layer got it, premultiplied, the surface's size).
pub fn read_next_present() {
    gpu::COPYABLE.store(true, std::sync::atomic::Ordering::Relaxed);
    gpu::WANT.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// The frame `read_next_present` asked for, once it has been presented.
pub fn take_present() -> Option<(u32, u32, Vec<u8>)> {
    gpu::TAKEN.lock().unwrap().take()
}

/// Video `id`'s current frame: a decoder's NV12 pixel buffer, wrapped as textures on the paint device (no copy). A
/// page's `<img data-raea-texture=id data-raea-w data-raea-h>` draws it.
#[cfg(target_vendor = "apple")]
pub fn set_video(id: u64, buffer: objc2_core_foundation::CFRetained<objc2_core_video::CVPixelBuffer>, w: u32, h: u32) -> Result<(), String> {
    let mut lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let g = lock.get_or_insert_with(gpu::Gpu::new).as_mut().map_err(|e| e.clone())?;
    g.caches.set_video(id, buffer, w, h)
}

/// Texture `id` for a page's `Paint::Resource` (a live 3D scene's target on this device): drawn as an opaque image.
pub fn set_texture(id: u64, view: wgpu::TextureView, w: u32, h: u32) -> Result<(), String> {
    let mut lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let g = lock.get_or_insert_with(gpu::Gpu::new).as_mut().map_err(|e| e.clone())?;
    g.caches.set_texture(id, view, w, h);
    Ok(())
}

/// Video `id` gone (its element removed): its frame let go.
pub fn drop_video(id: u64) {
    let mut lock = GPU.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(Ok(g)) = lock.as_mut() {
        g.caches.videos.remove(&id);
    }
}

/// A fallback raster's identity: its size and its commands (images by their blob, fonts by theirs), so an unchanged
/// one is rasterized once.
pub fn hash_scene(scene: &Scene, w: i32, h: i32) -> u64 {
    let mut s = std::collections::hash_map::DefaultHasher::new();
    (w, h).hash(&mut s);
    let brush = |s: &mut std::collections::hash_map::DefaultHasher, b: &Paint| match b {
        Paint::Solid(c) => format!("{c:?}").hash(s),
        Paint::Gradient(g) => format!("{g:?}").hash(s),
        Paint::Image(i) => {
            i.image.data.id().hash(s);
            (i.image.width, i.image.height).hash(s);
            format!("{:?}", i.sampler).hash(s);
        }
        Paint::Resource(r) => format!("{:?}", r.image).hash(s),
        Paint::Custom(_) => 0u8.hash(s),
    };
    for c in &scene.commands {
        match c {
            RenderCommand::PushLayer(l) => format!("L{:?}{:?}{}{:?}{:?}{:?}", l.fill, l.blend, l.alpha, l.transform, l.clip, l.filter).hash(&mut s),
            RenderCommand::PushClipLayer(l) => format!("C{:?}{:?}{:?}", l.fill, l.transform, l.clip).hash(&mut s),
            RenderCommand::PopLayer => 'P'.hash(&mut s),
            RenderCommand::Stroke(k) => {
                format!("S{:?}{:?}{:?}{:?}", k.style, k.transform, k.brush_transform, k.shape).hash(&mut s);
                brush(&mut s, &k.brush);
            }
            RenderCommand::Fill(k) => {
                format!("F{:?}{:?}{:?}{:?}", k.fill, k.transform, k.brush_transform, k.shape).hash(&mut s);
                brush(&mut s, &k.brush);
            }
            RenderCommand::GlyphRun(k) => {
                k.font_data.data.id().hash(&mut s);
                k.font_data.index.hash(&mut s);
                format!("G{}{}{:?}{:?}{:?}{}{:?}{:?}{:?}", k.font_size, k.hint, k.normalized_coords, k.embolden, k.style, k.brush_alpha, k.transform, k.glyph_transform, k.glyphs).hash(&mut s);
                brush(&mut s, &k.brush);
            }
            RenderCommand::BoxShadow(k) => format!("B{k:?}").hash(&mut s),
        }
    }
    s.finish()
}

#[cfg(test)]
mod tests;
