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
}

impl Default for QuadsWindowRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl QuadsWindowRenderer {
    /// Presented at the display's refresh (vsync).
    pub fn new() -> Self {
        QuadsWindowRenderer { stage: None, size: (1, 1), scene: Scene::new(), vsync: true, failed: None, transparent: false }
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
        let mut shown = super::present_with(stage, w, h, !self.transparent, |rec| rec.append_scene(scene.clone(), Affine::IDENTITY));
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
