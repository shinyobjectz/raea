//! Blitz's paint calls as quads (quads.wgsl): a PaintScene that sorts each call as it arrives, while the shape is
//! still a rect or a rounded rect (anyrender's own recorder flattens every shape to a path, which loses that).
//! What the kinds draw natively: rects, rounded rects and circles filled or stroked, lines, solid colour, linear and
//! concentric radial gradients, images, box shadows, glyph runs; rect and rounded-rect clips; group opacity. The rest
//! (a path, a sweep or two-point gradient, a blend mode, a mask, a filter, a clip of any other shape) is captured as
//! anyrender commands and rasterized by vello_cpu into a texture drawn in its place: one command, or a layer's whole
//! subtree. A backdrop filter needs what's under it, so it sends the whole frame to the CPU (`Recorder::cpu`).

use std::sync::Arc;

use anyrender::recording::Scene;
use anyrender::{Filter, Glyph, NormalizedCoord, Paint, PaintRef, PaintScene, RenderContext};
use peniko::kurbo::{self, Affine, Cap, Join, Rect, Shape, Stroke};
use peniko::{BlendMode, Color, Compose, Extend, Fill, FontData, GradientKind, ImageQuality, Mix, StyleRef};

use super::gpu::{Caches, Inst};

/// What a draw samples besides the glyph atlas and the gradient ramps.
#[derive(Clone)]
pub enum Tex {
    None,
    /// an image brush's picture, by its blob
    Image(peniko::ImageData),
    /// a vello_cpu raster of captured commands (Op::Raster's)
    Raster(u64),
    /// a group's offscreen picture (Op::Group's, by its place in the frame)
    Group(usize),
    /// a video's current frame (set_video): its luma and chroma planes, in two slots
    Video(u64),
    /// the page as composited before a backdrop (Op::Backdrop): drawn back first in the pass after it
    Prev,
    /// a backdrop's blurred region of `Prev`, at the page's own coordinates
    Blur,
}

pub enum Op {
    Draw(Inst, Tex),
    /// a layer with opacity or a blur filter: its ops rendered offscreen, blurred by `blur` (device px, 0 none) over
    /// `inst`'s quad, then `inst` (a texture quad) composites the result
    Group { ops: Vec<Op>, inst: Inst, slot: usize, blur: f32 },
    /// commands vello_cpu rasterizes at `bbox` (device pixels; the commands are already moved to its origin), keyed by
    /// their content so an unchanged one is rasterized once; `inst` draws the result
    Raster { key: u64, bbox: [i32; 4], scene: Scene, inst: Inst },
    /// a backdrop filter (a blur) on the GPU, at the page's own level: the page so far is copied (`prev` draws it back),
    /// `region` (device px) of it blurred by `sigma` (then put through `matrix`, the filter's colour steps, when it has
    /// any), and `blurred` draws that clipped to the layer's shape
    Backdrop { region: [f32; 4], sigma: f32, matrix: Option<[f32; 20]>, prev: Inst, blurred: Inst },
}

/// The clip in force: a device rect and its corner radii (rounded only from one rounded clip; anything more is
/// captured).
#[derive(Clone, Copy)]
struct Clip {
    rect: [f32; 4],
    radii: [f32; 4],
    /// a device rect (and its corner radii) cut out of it: a box shadow's clip to outside its box; none when empty
    hole: [f32; 4],
    holer: [f32; 4],
}

impl Clip {
    fn rounded(&self) -> bool {
        self.radii.iter().any(|r| *r > 0.0)
    }

    fn holed(&self) -> bool {
        self.hole[2] > self.hole[0] && self.hole[3] > self.hole[1]
    }

    /// An instance clipped by this.
    fn apply(&self, i: &mut Inst) {
        i.clip = self.rect;
        i.clipr = self.radii;
        i.hole = self.hole;
        i.holer = self.holer;
    }
}

/// An open isolated layer: its place in `layers`, where its commands start in the shadow recording, how many ops its
/// level held when it began, and its bounds.
struct Iso {
    layer: usize,
    cmd: usize,
    ops: usize,
    limit: Rect,
}

enum Layer {
    /// a native clip: what to restore on pop
    Clip(Clip),
    /// group opacity or a blur filter: the clip and ops to restore, the opacity, and the blur (device px, 0 none)
    Group { clip: Clip, ops: Vec<Op>, alpha: f32, blur: f32 },
}

/// Commands captured for vello_cpu: the scene, how many of its layers are open, and the clip it composites under.
struct Capture {
    scene: Scene,
    depth: usize,
    clip: Clip,
    /// device bounds of what was drawn, and the bound the layer itself sets (its clip, unless a filter spreads)
    bounds: Rect,
    limit: Option<Rect>,
    spread: f64,
    /// how the raster composites (fine.wgsl's texture modes): 1 a paint, 2 a popped layer, 3 a solid fill's coverage
    /// (vello's path), 4 a solid fill's coverage as a Skia mask; for 3 and 4, the fill's colour
    mode: f32,
    color: [f32; 4],
    /// the rect and rounded-rect clips open inside it (the depth each opened at, its transform, rect and radii; the
    /// first four), for the GPU textures drawn over its raster (`held`): vello_cpu can't draw those
    clips: [(usize, Affine, Rect, [f32; 4]); 4],
    nclips: usize,
}

impl Capture {
    fn new(scene: Scene, depth: usize, clip: Clip, bounds: Rect, limit: Option<Rect>, spread: f64, mode: f32) -> Self {
        Capture { scene, depth, clip, bounds, limit, spread, mode, color: [1.0; 4], clips: [(0, Affine::IDENTITY, Rect::ZERO, NO_RADII); 4], nclips: 0 }
    }

    /// A clip opened at the current depth, noted when it's a rect, rounded rect or circle
    fn note_clip(&mut self, t: Affine, clip: &impl Shape) {
        if let (Some((r, radii)), true) = (Recorder::native_shape(clip), self.nclips < self.clips.len()) {
            self.clips[self.nclips] = (self.depth, t, r, radii);
            self.nclips += 1;
        }
    }

    /// The clips noted inside it on `i`, a texture's rect `r` (radii `radii`) drawn by `t`: a clip at the texture's own
    /// rotation (a rotated box clipping the canvas or video it holds) cuts the rect itself; any other bounds the device
    /// clip by its bounding box. False when nothing of the rect is left.
    fn clip_held(&self, i: &mut Inst, t: Affine, mut r: Rect, mut radii: [f32; 4]) -> bool {
        let inv = t.inverse();
        for &(_, ct, cr, cradii) in &self.clips[..self.nclips] {
            let d = ct.transform_rect_bbox(cr);
            i.clip = [i.clip[0].max(d.x0 as f32), i.clip[1].max(d.y0 as f32), i.clip[2].min(d.x1 as f32), i.clip[3].min(d.y1 as f32)];
            let rel = inv * ct;
            let c = rel.as_coeffs();
            if c[1].abs() > 1e-4 * c[0].abs().max(c[3].abs()) || c[2].abs() > 1e-4 * c[0].abs().max(c[3].abs()) {
                continue;
            }
            let local = rel.transform_rect_bbox(cr);
            let cut = r.intersect(local);
            if cut.width() <= 0.0 || cut.height() <= 0.0 {
                return false;
            }
            let eps = 1e-3;
            let inside = |a: Rect, b: Rect| a.x0 >= b.x0 - eps && a.y0 >= b.y0 - eps && a.x1 <= b.x1 + eps && a.y1 <= b.y1 + eps;
            // the corners: the texture's own while the clip holds it whole; the clip's (in the texture's units) when the
            // clip lies inside it; square where they cross
            if !inside(r, local) {
                radii = if inside(local, r) { cradii.map(|x| x * c[0].abs().min(c[3].abs()) as f32) } else { NO_RADII };
            }
            r = cut;
        }
        i.shape = [r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32];
        i.radii = radii;
        let m = 1.5 / i.xf1[2] as f64;
        let q = r.inflate(m, m);
        i.quad = [q.x0 as f32, q.y0 as f32, q.x1 as f32, q.y1 as f32];
        i.clip[2] > i.clip[0] && i.clip[3] > i.clip[1]
    }
}

pub struct Recorder<'a> {
    pub caches: &'a mut Caches,
    pub ops: Vec<Op>,
    w: f32,
    h: f32,
    clip: Clip,
    layers: Vec<Layer>,
    capture: Option<Capture>,
    /// set when the frame needs the CPU whole (a backdrop filter, a blend over the page itself)
    pub cpu: bool,
    /// the isolated layers open (push_layer's, innermost last) and every command since the outermost began: a blend
    /// layer needs what's under it inside its isolated layer, so that layer turns into one capture (see `isolate`)
    iso: Vec<Iso>,
    shadow: Scene,
    pub groups: usize,
    /// what each kind cost this frame, for the tests and `RAEA_QUADS_STATS`
    pub fallbacks: usize,
    /// textures met inside a vello_cpu stretch (a capture), drawn natively right after its raster
    held: Vec<(Inst, Tex)>,
    /// backdrop blurs on the GPU (Op::Backdrop) instead of the frame going to the CPU, whose vello_cpu ignores backdrop
    /// filters; set by every paint (renders, the stage, windows) so they all draw a glass sheet the same way
    pub backdrop_gpu: bool,
}

const NO_RADII: [f32; 4] = [0.0; 4];

/// Why stretches went to vello_cpu, counted over the process (the playtest's cpu_paint.why): a curved solid fill kept
/// on Skia's coverage, any other path fill, a stroke, glyphs, a box shadow, a layer captured whole (mask, filter, odd
/// clip, blend), an image or gradient brush the GPU path doesn't draw.
pub const WHY: [&str; 7] = ["skia_curve", "path_fill", "stroke", "glyphs", "box_shadow", "layer", "brush"];
pub static WHY_COUNT: [std::sync::atomic::AtomicU64; 7] = [const { std::sync::atomic::AtomicU64::new(0) }; 7];
fn why(i: usize) {
    WHY_COUNT[i].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
/// anyrender_vello_cpu's limit for Skia path masks (MAX_SKIA_PATH_PIXELS): larger shapes stay on vello.
const SKIA_PATH_PIXELS: f64 = 131_072.0;

impl<'a> Recorder<'a> {
    pub fn new(caches: &'a mut Caches, w: u32, h: u32) -> Self {
        // no clip is an unbounded one (the frame's edge isn't a clip edge to vello, so it makes no strips)
        let clip = Clip { rect: [-1e6, -1e6, 1e6, 1e6], radii: NO_RADII, hole: [0.0; 4], holer: NO_RADII };
        Recorder { caches, ops: Vec::new(), w: w as f32, h: h as f32, clip, layers: Vec::new(), capture: None, cpu: false, iso: Vec::new(), shadow: Scene::new(), groups: 0, fallbacks: 0, backdrop_gpu: false, held: Vec::new() }
    }

    fn viewport(&self) -> Rect {
        Rect::new(0.0, 0.0, self.w as f64, self.h as f64)
    }

    fn base(&self, t: Affine) -> Inst {
        let c = t.as_coeffs();
        let i = t.inverse().as_coeffs();
        let s = t.determinant().abs().sqrt() as f32;
        Inst {
            xf0: [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32],
            xf1: [c[4] as f32, c[5] as f32, s.max(1e-6), 0.0],
            xi0: [i[0] as f32, i[1] as f32, i[2] as f32, i[3] as f32],
            xi1: [i[4] as f32, i[5] as f32, 0.0, 0.0],
            clip: self.clip.rect,
            clipr: self.clip.radii,
            hole: self.clip.hole,
            holer: self.clip.holer,
            m0: [1.0, 0.0, 0.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            ..Default::default()
        }
    }

    /// A device-pixel texture quad (glyphs, rasters, groups): identity transform, texel origin `at`.
    fn tex_inst(&self, kind: f32, quad: [f32; 4], at: [f32; 2], color: [f32; 4]) -> Inst {
        let mut i = self.base(Affine::IDENTITY);
        i.quad = quad;
        i.a = [at[0], at[1], 0.0, 0.0];
        i.color = color;
        i.p[0] = kind;
        i
    }

    /// A backdrop filter on the GPU (Op::Backdrop) when it can be: `backdrop_gpu`, a single uniform blur, at the page's
    /// own level (no group or capture open), under an axis-aligned transform and a rect or rounded rect clip.
    fn backdrop(&mut self, bf: &Filter, transform: Affine, clip: &impl Shape) -> bool {
        if !self.backdrop_gpu || self.capture.is_some() || self.layers.iter().any(|l| matches!(l, Layer::Group { .. })) || !axis_aligned(&transform) {
            return false;
        }
        // one uniform blur, with colour-matrix steps around it (saturate(), brightness(), ...: `saturate(160%)
        // blur(28px)`, the glass sheets), each node taking the one before as its input; a colour step is per pixel and
        // linear, so it's applied after the blur
        let (mut blur, mut matrix) = (None, None::<[f32; 20]>);
        for (k, n) in bf.nodes().iter().enumerate() {
            let chained = match n.inputs.primary {
                None => k == 0,
                Some(anyrender::filters::FilterInput::Result(id)) => k > 0 && id.0 as usize == k - 1,
                Some(_) => false,
            };
            if !chained || n.inputs.secondary.is_some() || n.subregion.is_some() || n.linear_rgb {
                return false;
            }
            match &n.effect {
                anyrender::filters::FilterEffect::GaussianBlur(b) if blur.is_none() && (b.std_deviation - b.std_deviation_y).abs() < 1e-3 => blur = Some(b.std_deviation),
                anyrender::filters::FilterEffect::ColorMatrix(m) => matrix = Some(matrix.map_or(m.0, |a| compose_matrix(&a, &m.0))),
                _ => return false,
            }
        }
        let Some(blur) = blur else { return false };
        // a rounded rect may come as a path (a recorded Scene keeps every clip as one: the window renderer's frames)
        let Some((r, radii)) = Self::native_shape(clip).or_else(|| clip.as_path_slice().and_then(path_rounded_rect)) else { return false };
        if radii.iter().any(|x| *x > 0.0) && self.clip.rounded() {
            return false;
        }
        let scale = transform.determinant().abs().sqrt() as f32;
        let rect = transform.transform_rect_bbox(r);
        let q = [rect.x0.floor().max(0.0) as f32, rect.y0.floor().max(0.0) as f32, (rect.x1.ceil() as f32).min(self.w), (rect.y1.ceil() as f32).min(self.h)];
        if q[2] <= q[0] || q[3] <= q[1] {
            return true;
        }
        // the page so far, drawn back whole as a popped layer (truncating blend over a clear page: the same bytes)
        let mut prev = self.tex_inst(3.0, [0.0, 0.0, self.w, self.h], [0.0, 0.0], [1.0; 4]);
        prev.clip = [-1e6, -1e6, 1e6, 1e6];
        prev.clipr = NO_RADII;
        prev.pad[1] = 2.0;
        // the blurred region, clipped to the layer's shape within the clip in force
        let mut blurred = self.tex_inst(3.0, q, [q[0], q[1]], [1.0; 4]);
        let c = intersect_clip(self.clip, rect, radii, transform);
        c.apply(&mut blurred);
        blurred.pad[1] = 2.0;
        self.ops.push(Op::Backdrop { region: q, sigma: blur * scale, matrix, prev, blurred });
        true
    }

    fn clip_rect(&self) -> Rect {
        let r = self.clip.rect;
        Rect::new(r[0] as f64, r[1] as f64, r[2] as f64, r[3] as f64)
    }

    // ---- fallback: commands for vello_cpu

    fn begin_capture(&mut self, limit: Option<Rect>, spread: f64) {
        self.capture = Some(Capture::new(Scene::new(), 0, self.clip, Rect::ZERO, limit, spread, 1.0));
    }

    /// The captured commands become one raster, placed where they drew (cut to the clip and the frame).
    fn end_capture(&mut self) {
        self.end_capture_raster();
        // the textures held while vello_cpu's stretch was open, over its raster
        let held = std::mem::take(&mut self.held);
        self.ops.extend(held.into_iter().map(|(i, t)| Op::Draw(i, t)));
    }

    fn end_capture_raster(&mut self) {
        let Some(c) = self.capture.take() else { return };
        let mut b = c.bounds.inflate(c.spread, c.spread);
        if let Some(l) = c.limit {
            b = b.intersect(l);
        }
        let cr = c.clip.rect;
        b = b.intersect(Rect::new(cr[0] as f64, cr[1] as f64, cr[2] as f64, cr[3] as f64)).intersect(self.viewport());
        if b.width() <= 0.0 || b.height() <= 0.0 || c.scene.commands.is_empty() {
            return;
        }
        // the raster's origin on vello_cpu's wide-tile grid (256 x 4), so what it aligns to its tiles (filter layers)
        // lands where it would in the whole frame
        let (w, h) = (self.w as i32, self.h as i32);
        let bbox = [((b.x0.floor() as i32) >> 8) << 8, ((b.y0.floor() as i32) >> 2) << 2, ((b.x1.ceil() as i32 + 3) & !3).min(w.max(b.x1.ceil() as i32)), ((b.y1.ceil() as i32 + 3) & !3).min(h.max(b.y1.ceil() as i32))];
        let mut scene = Scene::new();
        scene.append_scene(c.scene, Affine::translate((-bbox[0] as f64, -bbox[1] as f64)));
        let key = super::hash_scene(&scene, bbox[2] - bbox[0], bbox[3] - bbox[1]);
        let mut inst = self.tex_inst(3.0, [bbox[0] as f32, bbox[1] as f32, bbox[2] as f32, bbox[3] as f32], [0.0, 0.0], c.color);
        c.clip.apply(&mut inst);
        inst.pad[1] = c.mode;
        self.fallbacks += 1;
        self.ops.push(Op::Raster { key, bbox, scene, inst });
    }

    /// One command vello_cpu draws alone: captured, then placed.
    fn fallback(&mut self, bounds: Rect, draw: impl FnOnce(&mut Scene)) {
        self.begin_capture(None, 0.0);
        let c = self.capture.as_mut().unwrap();
        draw(&mut c.scene);
        c.bounds = bounds;
        self.end_capture();
    }

    /// One solid fill or stroke vello_cpu draws alone, as coverage (drawn in white): the GPU shades it in `col` by the
    /// solid-fill rules, as vello would composite the fill itself. `skia_mask`: the build draws this shape as a Skia mask.
    fn fallback_cov(&mut self, bounds: Rect, col: Color, skia_mask: bool, draw: impl FnOnce(&mut Scene)) {
        self.begin_capture(None, 0.0);
        let c = self.capture.as_mut().unwrap();
        draw(&mut c.scene);
        c.bounds = bounds;
        c.mode = if skia_mask { 4.0 } else { 3.0 };
        c.color = color(col);
        self.end_capture();
    }

    fn grow(&mut self, bounds: Rect) {
        if let Some(c) = self.capture.as_mut() {
            c.bounds = if c.bounds.area() == 0.0 { bounds } else { c.bounds.union(bounds) };
        }
    }

    // ---- blend backdrops

    /// Run `f` on the shadow recording while an isolated layer is open.
    fn mirror(&mut self, f: impl FnOnce(&mut Scene)) {
        if !self.iso.is_empty() {
            f(&mut self.shadow);
        }
    }

    /// The innermost isolated layer becomes one capture from its start (a blend inside it needs its backdrop): its
    /// native ops go, its commands (from the shadow recording) go to vello_cpu, and the layers above it stay open in
    /// the capture. False when no isolated layer is open: the blend is over the page, which only the CPU has.
    fn isolate(&mut self) -> bool {
        let Some(l) = self.iso.pop() else { return false };
        let open = self.layers.len() - l.layer;
        let mut clip = self.clip;
        while self.layers.len() > l.layer {
            match self.layers.pop() {
                Some(Layer::Clip(prev)) => clip = prev,
                Some(Layer::Group { clip: prev, ops, .. }) => {
                    clip = prev;
                    self.ops = ops;
                }
                None => {}
            }
        }
        self.ops.truncate(l.ops);
        self.clip = clip;
        self.iso.retain(|i| i.layer < l.layer);
        let scene = Scene { tolerance: self.shadow.tolerance, commands: self.shadow.commands[l.cmd..].to_vec() };
        if self.iso.is_empty() {
            self.shadow.commands.clear();
        }
        self.capture = Some(Capture::new(scene, open, clip, l.limit, Some(l.limit), 0.0, 2.0));
        true
    }

    // ---- native shapes

    /// A rect, rounded rect or circle, as (rect, radii tl tr br bl).
    fn native_shape(shape: &impl Shape) -> Option<(Rect, [f32; 4])> {
        if let Some(r) = shape.as_rect() {
            return Some((r, NO_RADII));
        }
        if let Some(rr) = shape.as_rounded_rect() {
            let k = rr.radii();
            return Some((rr.rect(), [k.top_left as f32, k.top_right as f32, k.bottom_right as f32, k.bottom_left as f32]));
        }
        if let Some(c) = shape.as_circle() {
            let r = c.radius;
            return Some((Rect::new(c.center.x - r, c.center.y - r, c.center.x + r, c.center.y + r), [r as f32; 4]));
        }
        // an axis-aligned rectangle given as a path (Blitz draws some border sides and backgrounds this way)
        let path = shape.as_path_slice()?;
        path_rect(path).map(|r| (r, NO_RADII)).or_else(|| path_rounded_rect(path))
    }

    /// The paint for a native draw; false when the brush needs the fallback.
    fn set_paint(&mut self, i: &mut Inst, paint: &PaintRef<'_>, transform: Affine, brush_transform: Option<Affine>) -> bool {
        let m = brush_transform.unwrap_or(Affine::IDENTITY).inverse().as_coeffs();
        i.m0 = [m[0] as f32, m[1] as f32, m[2] as f32, m[3] as f32];
        i.m1 = [m[4] as f32, m[5] as f32, 0.0, 0.0];
        match paint {
            Paint::Solid(c) => {
                i.color = color(*c);
                true
            }
            Paint::Gradient(g) => {
                let row = |caches: &mut Caches| caches.ramp(g);
                match &g.kind {
                    GradientKind::Linear(l) => {
                        i.p[2] = 1.0;
                        i.g = [l.start.x as f32, l.start.y as f32, l.end.x as f32, l.end.y as f32];
                    }
                    GradientKind::Radial(r) if r.start_center == r.end_center && r.start_radius < r.end_radius => {
                        i.p[2] = 2.0;
                        i.g = [r.start_center.x as f32, r.start_center.y as f32, r.end_radius, r.start_radius];
                    }
                    _ => return false,
                }
                let Some(v) = row(self.caches) else { return false };
                i.a = [v, extend(g.extend), 0.0, 0.0];
                true
            }
            Paint::Image(b) => {
                if b.image.width == 0 || b.image.height == 0 {
                    return false;
                }
                i.p[2] = 3.0;
                // vello_cpu's filters: nearest for Low, and for Medium under a whole-pixel translation; bilinear for
                // Medium; bicubic for High
                let whole = {
                    let c = (transform * brush_transform.unwrap_or(Affine::IDENTITY)).as_coeffs();
                    (c[0] - 1.0).abs() < 1e-5 && c[1].abs() < 1e-5 && c[2].abs() < 1e-5 && (c[3] - 1.0).abs() < 1e-5 && (c[4] - c[4].floor()).abs() < 1e-5 && (c[5] - c[5].floor()).abs() < 1e-5
                };
                i.p[3] = match b.sampler.quality {
                    ImageQuality::Low => 1.0,
                    ImageQuality::Medium if whole => 1.0,
                    ImageQuality::Medium => 0.0,
                    _ => 2.0,
                };
                i.a = [extend(b.sampler.x_extend), extend(b.sampler.y_extend), b.image.width as f32, b.image.height as f32];
                i.color = [1.0, 1.0, 1.0, b.sampler.alpha];
                // straight alpha: the shader premultiplies each texel as vello's pixmap does
                i.pad[2] = if b.image.alpha_type == peniko::ImageAlphaType::Alpha { 1.0 } else { 0.0 };
                true
            }
            // a video frame the GPU holds (blitz-paint draw_texture): NV12 sampled and converted as it composites
            Paint::Resource(b) => {
                let Some(v) = self.caches.videos.get(&b.image.into_ffi()) else { return false };
                i.a = [0.0, 0.0, v.w as f32, v.h as f32];
                i.color = [1.0, 1.0, 1.0, b.sampler.alpha];
                // NV12 planes (converted as they're sampled) or VideoToolbox's BGRA (an image, opaque so premultiplied):
                // either filtered as vello filters the stills, nearest, bilinear or bicubic by the brush's quality
                i.p[2] = if v.nv12 { 4.0 } else { 3.0 };
                let whole = {
                    let c = (transform * brush_transform.unwrap_or(Affine::IDENTITY)).as_coeffs();
                    (c[0] - 1.0).abs() < 1e-5 && c[1].abs() < 1e-5 && c[2].abs() < 1e-5 && (c[3] - 1.0).abs() < 1e-5 && (c[4] - c[4].floor()).abs() < 1e-5 && (c[5] - c[5].floor()).abs() < 1e-5
                };
                i.p[3] = match b.sampler.quality {
                    ImageQuality::Low => 1.0,
                    ImageQuality::Medium if whole => 1.0,
                    ImageQuality::Medium => 0.0,
                    _ => 2.0,
                };
                true
            }
            _ => false,
        }
    }

    fn tex_of(paint: &PaintRef<'_>) -> Tex {
        match paint {
            Paint::Image(b) => Tex::Image(b.image.clone()),
            Paint::Resource(b) => Tex::Video(b.image.into_ffi()),
            _ => Tex::None,
        }
    }
}

fn color(c: Color) -> [f32; 4] {
    let c = c.components;
    [c[0], c[1], c[2], c[3]]
}

fn extend(e: Extend) -> f32 {
    match e {
        Extend::Pad => 0.0,
        Extend::Repeat => 1.0,
        Extend::Reflect => 2.0,
    }
}

/// Two 4x5 colour matrices (rows R G B A of four factors and an offset) as one: `a`, then `b`.
fn compose_matrix(a: &[f32; 20], b: &[f32; 20]) -> [f32; 20] {
    let mut o = [0.0; 20];
    for r in 0..4 {
        for c in 0..5 {
            let mut v = if c == 4 { b[r * 5 + 4] } else { 0.0 };
            for k in 0..4 {
                v += b[r * 5 + k] * a[k * 5 + c];
            }
            o[r * 5 + c] = v;
        }
    }
    o
}

/// An axis-aligned rectangle given as a path: one closed subpath of four corners.
/// A closed path that traces a rounded rectangle (kurbo's RoundedRect as a path: straight sides, arcs as cubics), as
/// its rect and corner radii (top-left, top-right, bottom-right, bottom-left): the corners read off where the straight
/// sides begin, accepted only if every point on the path lies on that rounded rect's edge.
fn path_rounded_rect(path: &[kurbo::PathEl]) -> Option<(Rect, [f32; 4])> {
    use kurbo::PathEl::*;
    // its shape checked first, then its points read where they are (a Vec of them, and one per side, were made for each
    // fill: ZeroHeap)
    for (k, el) in path.iter().enumerate() {
        match el {
            MoveTo(_) if k == 0 => {}
            LineTo(_) | CurveTo(..) | QuadTo(..) => {}
            // a fill closes its one subpath itself: Blitz's border-radius clips end on the last corner, no ClosePath
            ClosePath if k + 1 == path.len() => {}
            _ => return None,
        }
    }
    let pts = || path.iter().filter_map(|el| match el {
        MoveTo(p) | LineTo(p) | CurveTo(_, _, p) | QuadTo(_, p) => Some(*p),
        ClosePath => None,
    });
    if pts().count() < 4 {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in pts() {
        (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
    }
    let e = 1e-3;
    let along = |on: &dyn Fn(&kurbo::Point) -> bool, key: &dyn Fn(&kurbo::Point) -> f64| -> Option<(f64, f64)> {
        let (mut lo, mut hi, mut any) = (f64::MAX, f64::MIN, false);
        for p in pts().filter(|p| on(p)) {
            let v = key(&p);
            (lo, hi, any) = (lo.min(v), hi.max(v), true);
        }
        any.then_some((lo, hi))
    };
    let (tl, tr) = along(&|p| (p.y - y0).abs() < e, &|p| p.x)?;
    let (bl, br) = along(&|p| (p.y - y1).abs() < e, &|p| p.x)?;
    let (lt, lb) = along(&|p| (p.x - x0).abs() < e, &|p| p.y)?;
    let (rt, rb) = along(&|p| (p.x - x1).abs() < e, &|p| p.y)?;
    // each corner's radius from both sides that meet there (they agree for a circular corner)
    let radius = |a: f64, b: f64| if (a - b).abs() < 0.05 { Some(a.max(0.0)) } else { None };
    let r = [radius(tl - x0, lt - y0)?, radius(x1 - tr, rt - y0)?, radius(x1 - br, y1 - rb)?, radius(bl - x0, y1 - lb)?];
    let rr = kurbo::RoundedRect::new(x0, y0, x1, y1, (r[0], r[1], r[2], r[3]));
    // every point on the edge of that rounded rect (a corner's points at its radius from the corner's centre)
    let on_edge = |p: &kurbo::Point| {
        let c = [(x0 + r[0], y0 + r[0], r[0]), (x1 - r[1], y0 + r[1], r[1]), (x1 - r[2], y1 - r[2], r[2]), (x0 + r[3], y1 - r[3], r[3])];
        for (k, (cx, cy, rad)) in c.iter().enumerate() {
            let inside = match k {
                0 => p.x < *cx && p.y < *cy,
                1 => p.x > *cx && p.y < *cy,
                2 => p.x > *cx && p.y > *cy,
                _ => p.x < *cx && p.y > *cy,
            };
            if inside && *rad > 0.0 {
                return ((p.x - cx).hypot(p.y - cy) - rad).abs() < 0.05;
            }
        }
        (p.x - x0).abs() < e || (p.x - x1).abs() < e || (p.y - y0).abs() < e || (p.y - y1).abs() < e
    };
    if !pts().all(|p| on_edge(&p)) {
        return None;
    }
    Some((rr.rect(), r.map(|x| x as f32)))
}

/// Two subpaths, each a rect or rounded rect, one inside the other and cut out of it by the fill rule (even-odd, or
/// non-zero wound the other way): as (outer, hole). Blitz paints borders this way.
fn path_ring(path: &[kurbo::PathEl], style: Fill) -> Option<((Rect, [f32; 4]), (Rect, [f32; 4]))> {
    use kurbo::PathEl::*;
    let split = (1..path.len()).find(|&k| matches!(path[k], MoveTo(_)))?;
    if path[split + 1..].iter().any(|el| matches!(el, MoveTo(_))) {
        return None;
    }
    let (a, b) = (&path[..split], &path[split..]);
    let shape = |p: &[kurbo::PathEl]| path_rect(p).map(|r| (r, NO_RADII)).or_else(|| path_rounded_rect(p));
    let (sa, sb) = (shape(a)?, shape(b)?);
    let ((outer, op), (hole, hp)) = if sa.0.contains_rect(sb.0) { ((sa, a), (sb, b)) } else if sb.0.contains_rect(sa.0) { ((sb, b), (sa, a)) } else { return None };
    if style == Fill::NonZero && (signed_area(op) > 0.0) == (signed_area(hp) > 0.0) {
        return None;
    }
    // the hole's edge inside the outer shape's (its rounded corners included)
    let (h, hr) = hole;
    let c = [(h.x0 + hr[0] as f64, h.y0 + hr[0] as f64, hr[0], -1.0, -1.0), (h.x1 - hr[1] as f64, h.y0 + hr[1] as f64, hr[1], 1.0, -1.0), (h.x1 - hr[2] as f64, h.y1 - hr[2] as f64, hr[2], 1.0, 1.0), (h.x0 + hr[3] as f64, h.y1 - hr[3] as f64, hr[3], -1.0, 1.0)];
    let inside = c.iter().all(|&(cx, cy, r, sx, sy)| {
        (0..=8).all(|k| {
            let t = k as f64 / 8.0 * std::f64::consts::FRAC_PI_2;
            sd_rrect(kurbo::Point::new(cx + sx * r as f64 * t.cos(), cy + sy * r as f64 * t.sin()), outer.0, outer.1) <= 1e-3
        })
    });
    inside.then_some((outer, hole))
}

/// Twice the signed area of a subpath's polygon (its points, curves by their ends): the sign is its winding.
fn signed_area(path: &[kurbo::PathEl]) -> f64 {
    // the points read where they are, in order, the last joined to the first (no Vec of them: ZeroHeap)
    let mut pts = path.iter().filter_map(|el| el.end_point());
    let Some(first) = pts.next() else { return 0.0 };
    let (mut sum, mut p) = (0.0, first);
    for q in pts {
        sum += p.x * q.y - q.x * p.y;
        p = q;
    }
    sum + (p.x * first.y - first.x * p.y)
}

/// The signed distance from `p` to a rounded rect's edge (negative inside), as fine.wgsl's sd_rrect.
fn sd_rrect(p: kurbo::Point, r: Rect, radii: [f32; 4]) -> f64 {
    let c = r.center();
    let half = kurbo::Vec2::new(r.width() / 2.0, r.height() / 2.0);
    let d = p - c;
    let rr = (if d.x > 0.0 { if d.y > 0.0 { radii[2] } else { radii[1] } } else if d.y > 0.0 { radii[3] } else { radii[0] }) as f64;
    let rr = rr.min(half.x.min(half.y));
    let q = kurbo::Vec2::new(d.x.abs() - half.x + rr, d.y.abs() - half.y + rr);
    q.x.max(q.y).min(0.0) + kurbo::Vec2::new(q.x.max(0.0), q.y.max(0.0)).hypot() - rr
}

fn path_rect(path: &[kurbo::PathEl]) -> Option<Rect> {
    use kurbo::PathEl::*;
    // up to five points in place (a Vec of them, and of their xs and ys, were made for each fill: ZeroHeap)
    let mut pts = [kurbo::Point::ZERO; 5];
    let mut n = 0usize;
    for (i, el) in path.iter().enumerate() {
        match (i, el) {
            (0, MoveTo(p)) | (_, LineTo(p)) if (i == 0) == matches!(el, MoveTo(_)) => {
                if n == 5 {
                    return None;
                }
                pts[n] = *p;
                n += 1;
            }
            (_, ClosePath) if i == path.len() - 1 => {}
            _ => return None,
        }
    }
    if n == 5 && pts[4] == pts[0] {
        n -= 1;
    }
    if n != 4 {
        return None;
    }
    let pts = &pts[..4];
    let xs = [pts[0].x, pts[1].x, pts[2].x, pts[3].x];
    let ys = [pts[0].y, pts[1].y, pts[2].y, pts[3].y];
    let axis = (0..4).all(|k| {
        let (a, b) = (pts[k], pts[(k + 1) % 4]);
        a.x == b.x || a.y == b.y
    });
    let (x0, x1) = (xs.iter().cloned().fold(f64::MAX, f64::min), xs.iter().cloned().fold(f64::MIN, f64::max));
    let (y0, y1) = (ys.iter().cloned().fold(f64::MAX, f64::min), ys.iter().cloned().fold(f64::MIN, f64::max));
    let corners = pts.iter().all(|p| (p.x == x0 || p.x == x1) && (p.y == y0 || p.y == y1));
    (axis && corners && x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1, y1))
}

/// Whether anyrender_vello_cpu (with Skia) takes Skia's coverage for this solid fill: a rotated transform, a curve or
/// a slanted line (its `needs_skia`).
fn skia_coverage(t: Affine, shape: &impl Shape) -> bool {
    let c = t.as_coeffs();
    if c[1] != 0.0 || c[2] != 0.0 {
        return true;
    }
    let mut last = kurbo::Point::ZERO;
    shape.path_elements(0.1).any(|el| match el {
        kurbo::PathEl::MoveTo(p) => {
            last = p;
            false
        }
        kurbo::PathEl::LineTo(p) => {
            let diagonal = p.x != last.x && p.y != last.y;
            last = p;
            diagonal
        }
        kurbo::PathEl::QuadTo(..) | kurbo::PathEl::CurveTo(..) => true,
        kurbo::PathEl::ClosePath => false,
    })
}

/// A transform with no rotation or skew.
fn axis_aligned(t: &Affine) -> bool {
    let c = t.as_coeffs();
    c[1] == 0.0 && c[2] == 0.0
}

fn stroke_bounds(t: Affine, shape: &impl Shape, style: &Stroke) -> Rect {
    let w = style.width * style.miter_limit.max(1.0);
    t.transform_rect_bbox(shape.bounding_box().inflate(w, w))
}

impl RenderContext for Recorder<'_> {}

impl PaintScene for Recorder<'_> {
    fn reset(&mut self) {
        self.ops.clear();
        self.layers.clear();
        self.capture = None;
        self.iso.clear();
        self.shadow.commands.clear();
    }

    fn push_layer(
        &mut self,
        fill: Fill,
        blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
        filter: Option<Arc<Filter>>,
        backdrop_filter: Option<Arc<Filter>>,
    ) {
        let blend = blend.into();
        let backdrop_filter = match backdrop_filter {
            Some(bf) => {
                if !self.backdrop(&bf, transform, clip) {
                    self.cpu = true;
                }
                // on the GPU the backdrop is drawn (Op::Backdrop); the layer goes on as one without it
                if self.cpu { Some(bf) } else { None }
            }
            None => None,
        };
        let normal = blend.mix == Mix::Normal && blend.compose == Compose::SrcOver;
        // a blend composites with what's under it in its isolated layer: that layer goes to vello_cpu whole
        if !normal && self.capture.is_none() && !self.isolate() {
            self.cpu = true;
        }
        let cmd = self.shadow.commands.len();
        self.mirror(|s| s.push_layer(fill, blend, alpha, transform, clip, filter.clone(), backdrop_filter.clone()));
        if let Some(c) = self.capture.as_mut() {
            c.depth += 1;
            c.note_clip(transform, clip);
            c.scene.push_layer(fill, blend, alpha, transform, clip, filter, backdrop_filter);
            return;
        }
        let native = Self::native_shape(clip).filter(|_| axis_aligned(&transform));
        let can_clip = native.is_some_and(|(_, r)| !(r.iter().any(|x| *x > 0.0) && self.clip.rounded()));
        // a filter that's one uniform blur of the layer (CSS `filter: blur()`): a group the GPU blurs before it
        // composites (Blitz sizes the clip for the blur's spread)
        let blur = match filter.as_ref().map(|f| f.nodes()) {
            None => Some(0.0),
            Some([n]) => match &n.effect {
                anyrender::filters::FilterEffect::GaussianBlur(b) if (b.std_deviation - b.std_deviation_y).abs() < 1e-3 && n.subregion.is_none() => {
                    Some(b.std_deviation * transform.determinant().abs().sqrt() as f32)
                }
                _ => None,
            },
            Some(_) => None,
        };
        if let (true, Some(blur), true) = (normal, blur, can_clip) {
            let (r, radii) = native.unwrap();
            let bounds = transform.transform_rect_bbox(r);
            if self.iso.is_empty() {
                self.shadow.commands.clear();
                self.shadow.push_layer(fill, blend, alpha, transform, clip, None, None);
            }
            let start = if self.iso.is_empty() { 0 } else { cmd };
            self.iso.push(Iso { layer: self.layers.len(), cmd: start, ops: self.ops.len(), limit: bounds });
            let prev = self.clip;
            self.clip = intersect_clip(prev, bounds, radii, transform);
            if alpha >= 0.999 && blur <= 0.0 {
                self.layers.push(Layer::Clip(prev));
            } else {
                let ops = std::mem::take(&mut self.ops);
                self.layers.push(Layer::Group { clip: prev, ops, alpha, blur });
            }
            return;
        }
        // a mask, a filter or an odd clip: the layer and all inside it go to vello_cpu (its clip bounds the result, the
        // filter's spread included: Blitz sizes the clip for it)
        let limit = Some(transform.transform_rect_bbox(clip.bounding_box()));
        let spread = filter.as_ref().map(|f| {
            // the spread under the layer's scale and rotation only (bounds_expansion applies the whole transform)
            let c = transform.as_coeffs();
            let e = f.bounds_expansion(&Affine::new([c[0], c[1], c[2], c[3], 0.0, 0.0]));
            e.x0.abs().max(e.x1.abs()).max(e.y0.abs()).max(e.y1.abs())
        }).unwrap_or(0.0);
        why(5);
        self.begin_capture(limit, spread);
        let c = self.capture.as_mut().unwrap();
        c.depth = 1;
        c.mode = 2.0;
        c.note_clip(transform, clip);
        c.scene.push_layer(fill, blend, alpha, transform, clip, filter, None);
    }

    fn push_clip_layer(&mut self, fill: Fill, transform: Affine, clip: &impl Shape) {
        self.mirror(|s| s.push_clip_layer(fill, transform, clip));
        if let Some(c) = self.capture.as_mut() {
            c.depth += 1;
            c.note_clip(transform, clip);
            c.scene.push_clip_layer(fill, transform, clip);
            return;
        }
        let native = Self::native_shape(clip).filter(|_| axis_aligned(&transform));
        if let Some((r, radii)) = native.filter(|(_, r)| !(r.iter().any(|x| *x > 0.0) && self.clip.rounded())) {
            let prev = self.clip;
            self.clip = intersect_clip(prev, transform.transform_rect_bbox(r), radii, transform);
            self.layers.push(Layer::Clip(prev));
            return;
        }
        // a rect with a rect or rounded rect cut out of it (Blitz clips a box shadow to outside its box this way)
        let ring = clip.as_path_slice().and_then(|p| path_ring(p, fill)).filter(|((_, r), _)| r.iter().all(|x| *x == 0.0));
        if let Some(((o, _), (h, hr))) = ring.filter(|_| axis_aligned(&transform) && !self.clip.holed()) {
            let prev = self.clip;
            let s = transform.as_coeffs()[0].abs().min(transform.as_coeffs()[3].abs()) as f32;
            let mut c = intersect_clip(prev, transform.transform_rect_bbox(o), NO_RADII, transform);
            let hd = transform.transform_rect_bbox(h);
            c.hole = [hd.x0 as f32, hd.y0 as f32, hd.x1 as f32, hd.y1 as f32];
            c.holer = hr.map(|x| x * s);
            self.clip = c;
            self.layers.push(Layer::Clip(prev));
            return;
        }
        why(5);
        self.begin_capture(Some(transform.transform_rect_bbox(clip.bounding_box())), 0.0);
        let c = self.capture.as_mut().unwrap();
        c.depth = 1;
        c.note_clip(transform, clip);
        c.scene.push_clip_layer(fill, transform, clip);
    }

    fn pop_layer(&mut self) {
        self.mirror(|s| s.pop_layer());
        if let Some(c) = self.capture.as_mut() {
            c.scene.pop_layer();
            while c.nclips > 0 && c.clips[c.nclips - 1].0 >= c.depth {
                c.nclips -= 1;
            }
            c.depth -= 1;
            if c.depth == 0 {
                self.end_capture();
            }
            return;
        }
        if self.iso.last().is_some_and(|i| i.layer + 1 == self.layers.len()) {
            self.iso.pop();
            if self.iso.is_empty() {
                self.shadow.commands.clear();
            }
        }
        match self.layers.pop() {
            Some(Layer::Clip(prev)) => self.clip = prev,
            Some(Layer::Group { clip, ops, alpha, blur }) => {
                let inner = std::mem::replace(&mut self.ops, ops);
                let bounds = self.clip.rect;
                self.clip = clip;
                let slot = self.groups;
                self.groups += 1;
                let q = [bounds[0].floor().max(0.0), bounds[1].floor().max(0.0), bounds[2].ceil().min(self.w), bounds[3].ceil().min(self.h)];
                if q[2] > q[0] && q[3] > q[1] && !inner.is_empty() {
                    let mut inst = self.tex_inst(3.0, q, [q[0], q[1]], [1.0, 1.0, 1.0, alpha]);
                    // a popped layer: vello composites it with the truncating blend
                    inst.pad[1] = 2.0;
                    self.ops.push(Op::Group { ops: inner, inst, slot, blur });
                }
            }
            None => {}
        }
    }

    fn stroke<'b>(&mut self, style: &Stroke, transform: Affine, brush: impl Into<PaintRef<'b>>, brush_transform: Option<Affine>, shape: &impl Shape) {
        let brush: PaintRef<'b> = brush.into();
        let bounds = stroke_bounds(transform, shape, style);
        self.mirror(|s| s.stroke(style, transform, brush.clone(), brush_transform, shape));
        if let Some(c) = self.capture.as_mut() {
            c.scene.stroke(style, transform, brush, brush_transform, shape);
            self.grow(bounds);
            return;
        }
        let w = style.width as f32;
        let plain = style.dash_pattern.is_empty() && w > 0.0;
        // a rect or rounded rect stroked: the ring between its outer and inner edges
        if let Some((r, radii)) = Self::native_shape(shape).filter(|_| plain) {
            let rounded = radii.iter().any(|x| *x > 0.0);
            let join_ok = rounded || style.join == Join::Miter || style.join == Join::Round;
            if join_ok {
                let h = w as f64 / 2.0;
                let outer = r.inflate(h, h);
                let oradii = if rounded { radii.map(|x| if x > 0.0 { x + w / 2.0 } else { 0.0 }) } else if style.join == Join::Round { [w / 2.0; 4] } else { NO_RADII };
                let mut i = self.base(transform);
                let q = outer.inflate(2.0, 2.0);
                i.quad = [q.x0 as f32, q.y0 as f32, q.x1 as f32, q.y1 as f32];
                i.shape = [outer.x0 as f32, outer.y0 as f32, outer.x1 as f32, outer.y1 as f32];
                i.radii = oradii;
                i.p[1] = w;
                let inner = outer.inflate(-(w as f64), -(w as f64));
                i.inner = [inner.x0 as f32, inner.y0 as f32, inner.x1 as f32, inner.y1 as f32];
                i.innerr = oradii.map(|x| (x - w).max(0.0));
                if self.set_paint(&mut i, &brush, transform, brush_transform) {
                    let tex = Self::tex_of(&brush);
                    self.ops.push(Op::Draw(i, tex));
                    return;
                }
            }
        }
        // a straight line with butt or square caps: a rect along it
        if let Some(line) = shape.as_line().filter(|_| plain && style.start_cap == style.end_cap && style.start_cap != Cap::Round) {
            let d = line.p1 - line.p0;
            let len = d.hypot();
            if len > 0.0 {
                let ext = if style.start_cap == Cap::Square { w as f64 / 2.0 } else { 0.0 };
                let along = transform * Affine::translate(line.p0.to_vec2()) * Affine::rotate(d.y.atan2(d.x));
                let r = Rect::new(-ext, -(w as f64) / 2.0, len + ext, w as f64 / 2.0);
                let mut i = self.base(along);
                let q = r.inflate(2.0, 2.0);
                i.quad = [q.x0 as f32, q.y0 as f32, q.x1 as f32, q.y1 as f32];
                i.shape = [r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32];
                if let Paint::Solid(c) = brush {
                    i.color = color(c);
                    self.ops.push(Op::Draw(i, Tex::None));
                    return;
                }
            }
        }
        if let Paint::Solid(col) = brush {
            why(2);
            self.fallback_cov(bounds, col, false, |s| s.stroke(style, transform, Color::WHITE, None, shape));
            return;
        }
        why(2);
        self.fallback(bounds, |s| s.stroke(style, transform, brush, brush_transform, shape));
    }

    fn fill<'b>(&mut self, style: Fill, transform: Affine, brush: impl Into<PaintRef<'b>>, brush_transform: Option<Affine>, shape: &impl Shape) {
        let brush: PaintRef<'b> = brush.into();
        let bounds = transform.transform_rect_bbox(shape.bounding_box());
        self.mirror(|s| s.fill(style, transform, brush.clone(), brush_transform, shape));
        // a texture the GPU holds, inside a stretch vello_cpu draws (a clip it can't do natively): vello_cpu can't draw
        // it, so it's held and drawn natively right over that stretch's raster, in the clip in force when it began and
        // the clips opened inside it (a rotated box's own, at the texture's rotation: the 3D in a camera roll; it was
        // dropped whenever the texture wasn't axis-aligned)
        if self.capture.is_some() && matches!(brush, Paint::Resource(_)) {
            if let Some((r, radii)) = Self::native_shape(shape) {
                let mut i = self.base(transform);
                let Some(c) = self.capture.as_ref() else { return };
                c.clip.apply(&mut i);
                if c.clip_held(&mut i, transform, r, radii) && self.set_paint(&mut i, &brush, transform, brush_transform) {
                    let tex = Self::tex_of(&brush);
                    self.held.push((i, tex));
                }
            }
            return;
        }
        if let Some(c) = self.capture.as_mut() {
            c.scene.fill(style, transform, brush, brush_transform, shape);
            self.grow(bounds);
            return;
        }
        if bounds.intersect(self.clip_rect()).area() <= 0.0 {
            return;
        }
        // a rect, rounded rect or circle, or one with a rect or rounded rect cut out of it (a border, a frame): the GPU's
        // own coverage, never the CPU's (vello_cpu painted small curved ones by Skia's scan converter; within a level)
        let ring = shape.as_path_slice().and_then(|p| path_ring(p, style));
        if let Some(((r, radii), hole)) = Self::native_shape(shape).map(|s| (s, None)).or(ring.map(|(o, h)| (o, Some(h)))) {
            let mut i = self.base(transform);
            if let Some((h, hr)) = hole {
                i.inner = [h.x0 as f32, h.y0 as f32, h.x1 as f32, h.y1 as f32];
                i.innerr = hr;
            }
            // a whole pixel of margin past the edge (in device pixels) for the antialiasing
            let m = 1.5 / i.xf1[2] as f64;
            let q = r.inflate(m, m);
            i.quad = [q.x0 as f32, q.y0 as f32, q.x1 as f32, q.y1 as f32];
            i.shape = [r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32];
            i.radii = radii;
            if self.set_paint(&mut i, &brush, transform, brush_transform) {
                let tex = Self::tex_of(&brush);
                self.ops.push(Op::Draw(i, tex));
                return;
            }
        }
        if let Paint::Solid(col) = brush {
            let skia = cfg!(feature = "skia") && skia_coverage(transform, shape) && bounds.area() <= SKIA_PATH_PIXELS;
            why(if skia { 0 } else { 1 });
            self.fallback_cov(bounds, col, skia, |s| s.fill(style, transform, Color::WHITE, None, shape));
            return;
        }
        // a texture the GPU holds that isn't set yet (the stage's, before its first frame): nothing this frame. vello_cpu
        // can't draw one, and its empty raster, cached by the texture's id, was drawn ever after (a blank stage box)
        if matches!(brush, Paint::Resource(_)) {
            return;
        }
        why(6);
        self.fallback(bounds, |s| s.fill(style, transform, brush, brush_transform, shape));
    }

    fn draw_glyphs<'b, 's: 'b>(
        &'s mut self,
        font: &'b FontData,
        font_size: f32,
        hint: bool,
        normalized_coords: &'b [NormalizedCoord],
        embolden: kurbo::Vec2,
        style: impl Into<StyleRef<'b>>,
        brush: impl Into<PaintRef<'b>>,
        brush_alpha: f32,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: impl Iterator<Item = Glyph> + Clone,
    ) {
        let style: StyleRef<'b> = style.into();
        let brush: PaintRef<'b> = brush.into();
        let em = font_size as f64 * transform.determinant().abs().sqrt().max(1e-3);
        let run_bounds = || {
            let mut b: Option<Rect> = None;
            for g in glyphs.clone() {
                let p = transform * kurbo::Point::new(g.x as f64, g.y as f64);
                let r = Rect::new(p.x - em * 1.5, p.y - em * 2.0, p.x + em * 2.5, p.y + em);
                b = Some(b.map_or(r, |b| b.union(r)));
            }
            b.unwrap_or(Rect::ZERO)
        };
        self.mirror(|s| s.draw_glyphs(font, font_size, hint, normalized_coords, embolden, style.clone(), brush.clone(), brush_alpha, transform, glyph_transform, glyphs.clone()));
        if let Some(c) = self.capture.as_mut() {
            c.scene.draw_glyphs(font, font_size, hint, normalized_coords, embolden, style, brush, brush_alpha, transform, glyph_transform, glyphs.clone());
            let b = run_bounds();
            self.grow(b);
            return;
        }
        let solid = match (&style, &brush) {
            (StyleRef::Fill(_), Paint::Solid(c)) => Some(*c),
            _ => None,
        };
        let Some(col) = solid else {
            let b = run_bounds();
            why(3);
            self.fallback(b, |s| s.draw_glyphs(font, font_size, hint, normalized_coords, embolden, style, brush, brush_alpha, transform, glyph_transform, glyphs.clone()));
            return;
        };
        // as vello_cpu places glyphs: y snapped to whole device pixels when the run is axis-aligned and unhinted and
        // the text isn't moving; otherwise to 1/SUBPIXEL of a pixel (the atlas keeps one raster per offset used)
        let c = transform.as_coeffs();
        let snap_y = !hint && axis_aligned(&transform) && c[3] != 0.0 && glyph_transform.is_none_or(|g| g.as_coeffs()[1] == 0.0) && !anyrender::moving_text::get();
        let linear = [c[0], c[1], c[2], c[3]];
        let clip = self.clip_rect();
        // each glyph's draw pushed as it's placed (a list of them a run was made first: ZeroHeap)
        let mut colored = Vec::new();
        for g in glyphs.clone() {
            let p = transform * kurbo::Point::new(g.x as f64, g.y as f64);
            let k = super::gpu::SUBPIXEL;
            let (qx, qy) = ((p.x * k).round() / k, if snap_y { p.y.round() } else { (p.y * k).round() / k });
            let (ix, iy) = (qx.floor(), qy.floor());
            if ix + em * 3.0 < clip.x0 || ix - em * 3.0 > clip.x1 || iy + em * 3.0 < clip.y0 || iy - em * 3.0 > clip.y1 {
                continue;
            }
            let sub = [((qx - ix) * k).round() as u8, ((qy - iy) * k).round() as u8];
            let key = super::gpu::GlyphKey::new(font, g.id, font_size, hint, normalized_coords, embolden, linear, glyph_transform, sub);
            let Some(e) = self.caches.glyph(&key, font, g.id, font_size, hint, normalized_coords, embolden, linear, glyph_transform, sub) else { continue };
            if e.color {
                colored.push(g);
                continue;
            }
            if e.w == 0 {
                continue;
            }
            let x0 = ix as f32 + e.dx;
            let y0 = iy as f32 + e.dy;
            let i = self.tex_inst(2.0, [x0, y0, x0 + e.w as f32, y0 + e.h as f32], [e.x as f32, e.y as f32], color(col));
            self.ops.push(Op::Draw(i, Tex::None));
        }
        // colour glyphs drawn by vello_cpu as they are, each placed where it falls
        for g in colored {
            let p = transform * kurbo::Point::new(g.x as f64, g.y as f64);
            let b = Rect::new(p.x - em * 1.5, p.y - em * 2.0, p.x + em * 2.5, p.y + em);
            why(3);
            self.fallback(b, |s| s.draw_glyphs(font, font_size, hint, normalized_coords, embolden, style.clone(), brush.clone(), brush_alpha, transform, glyph_transform, std::iter::once(g)));
        }
    }

    fn draw_box_shadow(&mut self, transform: Affine, rect: Rect, brush: Color, radius: f64, std_dev: f64) {
        let spread = 2.5 * std_dev + 1.0;
        let bounds = transform.transform_rect_bbox(rect.inflate(spread, spread));
        self.mirror(|s| s.draw_box_shadow(transform, rect, brush, radius, std_dev));
        if let Some(c) = self.capture.as_mut() {
            c.scene.draw_box_shadow(transform, rect, brush, radius, std_dev);
            self.grow(bounds);
            return;
        }
        // with Skia built in, vello_cpu takes Skia's blur mask (Chrome's tails): the rounded rect blurred by Skia's three
        // boxes. So this blurs it the same way on the GPU: the box filled in a group the filter blur runs over
        if cfg!(feature = "skia") && std::env::var_os("RAEA_CPU_SHADOWS").is_none() {
            let q = [bounds.x0.floor().max(0.0) as f32, bounds.y0.floor().max(0.0) as f32, (bounds.x1.ceil() as f32).min(self.w), (bounds.y1.ceil() as f32).min(self.h)];
            if q[2] <= q[0] || q[3] <= q[1] || bounds.intersect(self.clip_rect()).area() <= 0.0 {
                return;
            }
            let mut i = self.base(transform);
            let m = 1.5 / i.xf1[2] as f64;
            let qq = rect.inflate(m, m);
            i.quad = [qq.x0 as f32, qq.y0 as f32, qq.x1 as f32, qq.y1 as f32];
            i.shape = [rect.x0 as f32, rect.y0 as f32, rect.x1 as f32, rect.y1 as f32];
            i.radii = [radius as f32; 4];
            i.color = color(brush);
            // the box whole, before the blur: the clip applies to the blurred result
            Clip { rect: [-1e6, -1e6, 1e6, 1e6], radii: NO_RADII, hole: [0.0; 4], holer: NO_RADII }.apply(&mut i);
            let mut inst = self.tex_inst(3.0, q, [q[0], q[1]], [1.0; 4]);
            inst.pad[1] = 2.0;
            let slot = self.groups;
            self.groups += 1;
            let sigma = std_dev as f32 * transform.determinant().abs().sqrt() as f32;
            self.ops.push(Op::Group { ops: vec![Op::Draw(i, Tex::None)], inst, slot, blur: sigma });
            return;
        }
        if cfg!(feature = "skia") {
            why(4);
            self.fallback(bounds, |s| s.draw_box_shadow(transform, rect, brush, radius, std_dev));
            return;
        }
        let r = [rect.x0 as f32, rect.y0 as f32, rect.x1 as f32, rect.y1 as f32];
        let (a, g) = super::gpu::shadow_params(r, radius as f32, std_dev as f32);
        let mut i = self.base(transform);
        let q = rect.inflate(spread, spread);
        i.quad = [q.x0 as f32, q.y0 as f32, q.x1 as f32, q.y1 as f32];
        i.shape = r;
        i.a = a;
        i.g = g;
        i.color = color(brush);
        i.p[0] = 1.0;
        self.ops.push(Op::Draw(i, Tex::None));
    }
}

/// The clip in force cut by a new rect clip with these radii (a rounded one only when the old clip is plain).
fn intersect_clip(prev: Clip, r: Rect, radii: [f32; 4], t: Affine) -> Clip {
    let s = t.as_coeffs()[0].abs().min(t.as_coeffs()[3].abs()) as f32;
    let rect = [prev.rect[0].max(r.x0 as f32), prev.rect[1].max(r.y0 as f32), prev.rect[2].min(r.x1 as f32), prev.rect[3].min(r.y1 as f32)];
    let radii = if radii.iter().any(|x| *x > 0.0) { radii.map(|x| x * s) } else { prev.radii };
    Clip { rect, radii, ..prev }
}
