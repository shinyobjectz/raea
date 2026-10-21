//! A window's renderer for anyrender (Blitz's shell, dioxus-native): what a window shows is drawn by the quads
//! compositor on the paint device and presented to the window's surface, the same path the live stage takes, so an
//! app's chrome and the stage under it share one GPU device and one renderer.
//!
//! The painter is anyrender's recording `Scene`: a frame is recorded, then replayed into the compositor's recorder
//! (which can take it twice, when the glyph atlas fills and the frame starts again).

use std::sync::Arc;

use anyrender::{ImageRenderer, PaintScene, RenderContext, Scene, WindowHandle, WindowRenderer};
use peniko::kurbo::Affine;

pub struct QuadsWindowRenderer {
    stage: Option<super::Stage>,
    size: (u32, u32),
    scene: Scene,
    vsync: bool,
    /// the last frame's error, said once until a frame draws again
    failed: Option<String>,
    /// see-through where the chrome paints nothing (a stage's own view under the window shows there)
    transparent: bool,
    /// `low_latency`: no vsync wait in the present, and the next drawable taken ahead
    low_latency: bool,
    /// `reveal`: the window's first frames come in through a pixel diffusion, once, then nothing
    reveal: Option<Reveal>,
}

/// The launch's reveal (void only, `QuadsWindowRenderer::reveal`): from the first present, each square cell of the window
/// shows once its threshold (a 4x4 Bayer order mixed with an fbm field, so it diffuses in rather than wipes, the site's
/// dither-field look) falls under t, t eased 0 -> 1 over its `ms` (void: 280); cells not yet shown are void's black, stepping
/// through the site's dark greys just before they open. Drawn as one cell-sized mask image, nearest-sampled over the
/// frame: a few thousand pixels written and uploaded a frame while it runs, nothing once it's done. `fade`: a plain
/// fade from black instead (macOS Reduce Motion).
struct Reveal {
    fade: bool,
    /// how long it runs (ms)
    ms: f64,
    /// asks for the next frame while it runs (the window draws only when asked)
    ask: fn(),
    start: Option<std::time::Instant>,
    /// the cell grid and each cell's threshold in [0, 1), made at the first frame for its size
    cols: u32,
    rows: u32,
    thresholds: Vec<f32>,
    px: Vec<u8>,
}

/// a cell's side in the window's pixels: the site's 6 px cells at 2x
const REVEAL_CELL: u32 = 12;
/// the texture id the reveal's mask is drawn by
const REVEAL_LAYER: u64 = u64::MAX - 14;

impl Reveal {
    fn thresholds(cols: u32, rows: u32) -> Vec<f32> {
        const BAYER: [f32; 16] = [0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0];
        let fract = |v: f32| v - v.floor();
        let hash = |x: f32, y: f32| fract((x * 127.1 + y * 311.7).sin() * 43758.547);
        let noise = |x: f32, y: f32| {
            let (ix, iy) = (x.floor(), y.floor());
            let (fx, fy) = (x - ix, y - iy);
            let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
            let (a, b, c, d) = (hash(ix, iy), hash(ix + 1.0, iy), hash(ix, iy + 1.0), hash(ix + 1.0, iy + 1.0));
            a * (1.0 - ux) * (1.0 - uy) + b * ux * (1.0 - uy) + c * (1.0 - ux) * uy + d * ux * uy
        };
        let fbm = |x: f32, y: f32| {
            let (mut v, mut amp, mut f) = (0.0, 0.5, 1.0);
            for _ in 0..4 {
                v += noise(x * f, y * f) * amp;
                f *= 2.02;
                amp *= 0.5;
            }
            v
        };
        let mut t: Vec<f32> = (0..rows)
            .flat_map(|y| (0..cols).map(move |x| (x, y)))
            .map(|(x, y)| 0.62 * fbm(x as f32 * 0.07, y as f32 * 0.07) + 0.38 * (BAYER[((y % 4) * 4 + x % 4) as usize] + 0.5) / 16.0)
            .collect();
        // by rank: the thresholds spread evenly over [0, 1), so the share of cells shown is t itself (the field's own
        // values bunch in the middle, and most of it opened at once)
        let mut order: Vec<usize> = (0..t.len()).collect();
        order.sort_by(|&a, &b| t[a].total_cmp(&t[b]));
        let n = t.len() as f32;
        for (rank, &i) in order.iter().enumerate() {
            t[i] = rank as f32 / n;
        }
        t
    }

    /// This frame's mask (premultiplied RGBA at cell resolution), and whether the reveal goes on after it.
    fn frame(&mut self, w: u32, h: u32) -> bool {
        let start = *self.start.get_or_insert_with(std::time::Instant::now);
        let p = (start.elapsed().as_secs_f64() * 1e3 / self.ms.max(1.0)).min(1.0) as f32;
        self.frame_at(p, w, h)
    }

    /// The mask `p` (0..1) of the way through.
    fn frame_at(&mut self, p: f32, w: u32, h: u32) -> bool {
        // eased in and out (smoothstep): the diffusion shows across the whole of it, not mostly in its first third
        let t = p * p * (3.0 - 2.0 * p);
        if self.fade {
            self.cols = 1;
            self.rows = 1;
            self.px = vec![0, 0, 0, ((1.0 - t) * 255.0).round() as u8];
            return p < 1.0;
        }
        let (cols, rows) = (w.div_ceil(REVEAL_CELL), h.div_ceil(REVEAL_CELL));
        if (cols, rows) != (self.cols, self.rows) {
            (self.cols, self.rows) = (cols, rows);
            self.thresholds = Self::thresholds(cols, rows);
            self.px = vec![0; (cols * rows * 4) as usize];
        }
        // the site's dark palette, the last steps before a cell opens
        const GREYS: [u8; 3] = [14, 30, 62];
        for (i, &th) in self.thresholds.iter().enumerate() {
            let d = th - t;
            let c = &mut self.px[i * 4..i * 4 + 4];
            if d <= 0.0 {
                c.copy_from_slice(&[0, 0, 0, 0]);
            } else {
                let g = if d < 0.06 { GREYS[((0.06 - d) / 0.02).min(2.0) as usize] } else { 0 };
                c.copy_from_slice(&[g, g, g, 255]);
            }
        }
        p < 1.0
    }
}

impl Default for QuadsWindowRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl QuadsWindowRenderer {
    /// Presented at the display's refresh (vsync).
    pub fn new() -> Self {
        QuadsWindowRenderer { stage: None, size: (1, 1), scene: Scene::new(), vsync: true, failed: None, transparent: false, low_latency: false, reveal: None }
    }

    /// The window's first frames revealed through a pixel diffusion (or, `fade`, a plain fade from black) over `ms`, once:
    /// void's launch. `ask` asks the window for its next frame while the reveal runs. A window only void makes; renders, exports
    /// and play never draw through this.
    pub fn reveal(mut self, fade: bool, ms: f64, ask: fn()) -> Self {
        self.reveal = Some(Reveal { fade, ms, ask, start: None, cols: 0, rows: 0, thresholds: Vec::new(), px: Vec::new() });
        self
    }

    /// Presented without the vsync wait (Metal's displaySyncEnabled off), with the next frame's drawable taken right after
    /// each present (Stage::hold_next): on Metal taking a drawable waits until the display frees one, about a refresh,
    /// with or without vsync, so a frame asked for mid-refresh (void's stage painted) drew only after the next refresh
    /// and showed a refresh later still. Held ahead, it draws and presents at once and makes the coming refresh. The
    /// wait for the next drawable paces the window as vsync did.
    ///
    /// Measured in void (2026-10-18, a 60 Hz display): with vsync the window's frame waited about 15 ms for its drawable
    /// each time, and wgpu held the device's fence lock through that wait, so the stage's submits from its own thread
    /// waited too (fixed in engine/html/vendor/wgpu-core, which void builds with); and the window's frame recorded the stage's
    /// page only after the refresh it waited for, so the page showed a refresh after that.
    pub fn low_latency(mut self) -> Self {
        self.low_latency = true;
        self.vsync = false;
        self
    }

    /// See-through where the chrome paints nothing: the window's layer isn't opaque and each frame starts transparent,
    /// so a view under it (void's stage) shows there. Partly transparent paint composites premultiplied.
    pub fn transparent(mut self) -> Self {
        self.transparent = true;
        self
    }

    /// Without the vsync wait (a benchmark, a window that paces itself).
    pub fn without_vsync() -> Self {
        QuadsWindowRenderer { vsync: false, ..Self::new() }
    }
}

impl RenderContext for QuadsWindowRenderer {}

impl WindowRenderer for QuadsWindowRenderer {
    type ScenePainter<'a>
        = Scene
    where
        Self: 'a;

    fn resume<F: FnOnce() + 'static>(&mut self, window: Arc<dyn WindowHandle>, width: u32, height: u32, on_ready: F) {
        self.size = (width.max(1), height.max(1));
        match super::stage(wgpu::SurfaceTarget::from(window), self.size.0, self.size.1, self.vsync) {
            Ok(mut s) => {
                if self.transparent {
                    if let Err(e) = s.set_transparent(true) {
                        eprintln!("raea quads window: {e}");
                    }
                }
                self.stage = Some(s);
            }
            Err(e) => eprintln!("raea quads window: {e}"),
        }
        // native: the surface is made inline, so it's ready (or failed) now
        on_ready();
    }

    fn complete_resume(&mut self) -> bool {
        self.stage.is_some()
    }

    fn suspend(&mut self) {
        self.stage = None;
    }

    fn is_active(&self) -> bool {
        self.stage.is_some()
    }

    fn set_size(&mut self, width: u32, height: u32) {
        self.size = (width.max(1), height.max(1));
        if let Some(s) = self.stage.as_mut() {
            if let Err(e) = s.resize(self.size.0, self.size.1) {
                eprintln!("raea quads window: {e}");
            }
        }
    }

    fn render<F: FnOnce(&mut Self::ScenePainter<'_>)>(&mut self, draw_fn: F) {
        let Some(stage) = self.stage.as_ref() else { return };
        self.scene.reset();
        draw_fn(&mut self.scene);
        let scene = &self.scene;
        let (w, h) = self.size;
        // the launch's reveal: this frame's mask drawn last, over everything (the composited stage too)
        let more = self.reveal.as_mut().map(|r| r.frame(w, h));
        let mask = self.reveal.as_ref().filter(|_| more.is_some());
        let mut shown = super::present_with(stage, w, h, !self.transparent, |rec| {
            rec.append_scene(scene.clone(), Affine::IDENTITY);
            if let Some(r) = mask {
                draw_reveal(rec, r, w, h);
            }
        });
        match more {
            Some(true) => (self.reveal.as_ref().unwrap().ask)(),
            // done: nothing of it is drawn again
            Some(false) => self.reveal = None,
            None => {}
        }
        // low latency: the next frame's drawable taken now, while nothing waits on this thread (Stage::hold_next)
        if self.low_latency && shown.is_ok() {
            stage.hold_next();
        }
        if shown.as_ref().is_err_and(|e| e == super::NEEDS_CPU) {
            // the frame painted by vello_cpu (a backdrop in an opacity group, a blend over the page). vello_cpu can't draw
            // a texture the GPU holds (the stage, composited into the chrome), so those are drawn on the GPU first, in
            // their clips, and the CPU's frame over them: its pixels are clear where the textures are
            let textures = texture_commands(scene);
            let mut r = anyrender_vello_cpu::VelloCpuImageRenderer::new(w, h);
            let mut px = Vec::new();
            let base = !self.transparent && textures.commands.is_empty();
            r.render_to_vec(
                |p| {
                    if base {
                        p.fill(peniko::Fill::NonZero, Affine::IDENTITY, peniko::Color::BLACK, None, &peniko::kurbo::Rect::new(0.0, 0.0, w as f64, h as f64));
                    }
                    p.append_scene(scene.clone(), Affine::IDENTITY)
                },
                &mut px,
            );
            shown = if textures.commands.is_empty() {
                super::present_rgba(stage, w, h, &px).map(|_| super::FrameStats::default())
            } else {
                super::present_with(stage, w, h, !self.transparent, |rec| {
                    rec.append_scene(textures.clone(), Affine::IDENTITY);
                    rec.caches.upload_owned(CPU_LAYER, w, h, &px);
                    let brush = peniko::ImageBrush { image: anyrender::ResourceId::from_ffi(CPU_LAYER), sampler: peniko::ImageSampler::default() };
                    rec.fill(peniko::Fill::NonZero, Affine::IDENTITY, anyrender::Paint::Resource(brush), None, &peniko::kurbo::Rect::new(0.0, 0.0, w as f64, h as f64));
                })
            };
        }
        match shown {
            Ok(_) => self.failed = None,
            Err(e) => {
                if self.failed.as_deref() != Some(e.as_str()) {
                    eprintln!("raea quads window: {e}");
                }
                self.failed = Some(e);
            }
        }
    }
}

/// The reveal's mask over the frame: its cells, nearest-sampled at REVEAL_CELL pixels each (one pixel over the whole
/// frame for the fade).
fn draw_reveal(rec: &mut super::record::Recorder, r: &Reveal, w: u32, h: u32) {
    rec.caches.upload_owned(REVEAL_LAYER, r.cols, r.rows, &r.px);
    let brush = peniko::ImageBrush { image: anyrender::ResourceId::from_ffi(REVEAL_LAYER), sampler: peniko::ImageSampler::default().with_quality(peniko::ImageQuality::Low) };
    let cell = if r.cols == 1 { Affine::scale_non_uniform(w as f64, h as f64) } else { Affine::scale(REVEAL_CELL as f64) };
    rec.fill(peniko::Fill::NonZero, cell, anyrender::Paint::Resource(brush), None, &peniko::kurbo::Rect::new(0.0, 0.0, r.cols as f64, r.rows as f64));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reveal over a page, as void's window draws it, through the quads compositor: black at its start, all page at
    /// its end, part way between (and diffused, not wiped: both kinds of cell across every band of the frame). With
    /// RAEA_REVEAL_OUT=DIR its frames are written there as PNGs, to look at.
    #[test]
    fn reveal_diffuses_in_and_leaves_nothing() {
        let (w, h) = (640u32, 400u32);
        let page = |rec: &mut super::super::record::Recorder| {
            rec.fill(peniko::Fill::NonZero, Affine::IDENTITY, anyrender::Paint::Solid(peniko::Color::from_rgb8(200, 120, 40)), None, &peniko::kurbo::Rect::new(0.0, 0.0, w as f64, h as f64));
            rec.fill(peniko::Fill::NonZero, Affine::IDENTITY, anyrender::Paint::Solid(peniko::Color::from_rgb8(240, 240, 235)), None, &peniko::kurbo::Rect::new(60.0, 60.0, 580.0, 340.0));
        };
        let mut r = Reveal { fade: false, ms: 280.0, ask: || {}, start: None, cols: 0, rows: 0, thresholds: Vec::new(), px: Vec::new() };
        let out = std::env::var_os("RAEA_REVEAL_OUT").map(std::path::PathBuf::from);
        let mut shown_at = Vec::new();
        for (i, p) in [0.0f32, 0.15, 0.35, 0.6, 1.0].into_iter().enumerate() {
            let t0 = std::time::Instant::now();
            r.frame_at(p, w, h);
            let mask_us = t0.elapsed().as_secs_f64() * 1e6;
            let mut px = Vec::new();
            super::super::paint_with(w, h, true, &mut px, |rec| {
                page(rec);
                draw_reveal(rec, &r, w, h);
            })
            .unwrap();
            // the page's share of the frame: pixels not black
            let lit = px.chunks(4).filter(|c| c[0] > 70 || c[1] > 70).count() as f64 / (w * h) as f64;
            shown_at.push(lit);
            eprintln!("reveal p {p:.2}: page {:.1}% shown, mask {mask_us:.0} us ({}x{} cells)", lit * 100.0, r.cols, r.rows);
            if let Some(d) = &out {
                image::save_buffer(d.join(format!("reveal-{i}.png")), &px, w, h, image::ColorType::Rgba8).unwrap();
            }
        }
        assert!(shown_at[0] < 0.01, "black at its start: {:.3}", shown_at[0]);
        assert!(shown_at[4] > 0.999, "all page at its end: {:.3}", shown_at[4]);
        assert!(shown_at[2] > 0.2 && shown_at[2] < 0.95, "part way at 0.35: {:.3}", shown_at[2]);
    }
}

/// The texture id a CPU-painted frame is drawn by over the GPU's textures (QuadsWindowRenderer's fallback).
const CPU_LAYER: u64 = u64::MAX - 13;

/// A frame's texture paints alone (`Paint::Resource`, the textures the GPU holds), each under the clips it was drawn in,
/// layers kept only as clips: what the GPU draws under a CPU-painted frame.
fn texture_commands(scene: &Scene) -> Scene {
    use anyrender::recording::RenderCommand;
    let mut out = Scene::new();
    let mut any = false;
    for cmd in &scene.commands {
        match cmd {
            RenderCommand::PushLayer(l) => out.push_clip_layer(l.fill, l.transform, &l.clip),
            RenderCommand::PushClipLayer(l) => out.push_clip_layer(l.fill, l.transform, &l.clip),
            RenderCommand::PopLayer => out.pop_layer(),
            RenderCommand::Fill(f) if matches!(f.brush, anyrender::Paint::Resource(_)) => {
                any = true;
                out.commands.push(cmd.clone());
            }
            _ => {}
        }
    }
    if !any {
        out.commands.clear();
    }
    out
}
