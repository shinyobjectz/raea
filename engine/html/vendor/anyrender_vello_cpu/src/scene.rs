use std::sync::Arc;

use anyrender::{Filter, NormalizedCoord, Paint, PaintRef, PaintScene, RenderContext};
use glifo::FontEmbolden;
use kurbo::{Affine, Diagonal2, Rect, Shape, Stroke};
use peniko::{BlendMode, Color, Fill, FontData, ImageBrush, StyleRef};
use vello_cpu::{PaintType, Pixmap};

use crate::image_cache::{ImageCache, ImageCacheConfig};

const DEFAULT_TOLERANCE: f64 = 0.1;

pub struct VelloCpuScenePainter {
    pub(crate) render_ctx: vello_cpu::RenderContext,
    pub(crate) resources: vello_cpu::Resources,
    pub(crate) image_cache: ImageCache,
    /// raea: per pushed layer, whether it is a clip path (`push_clip_path`) rather than a layer
    clip_stack: Vec<bool>,
}

/// raea: device-space bounding box area up to which solid fills take Skia's coverage.
#[cfg(feature = "skia-text")]
const MAX_SKIA_PATH_PIXELS: f64 = 131_072.0;

impl VelloCpuScenePainter {
    pub fn new(width: u16, height: u16) -> Self {
        Self::with_image_cache_config(width, height, ImageCacheConfig::default())
    }

    pub fn with_image_cache_config(width: u16, height: u16, config: ImageCacheConfig) -> Self {
        Self {
            render_ctx: vello_cpu::RenderContext::new(width, height),
            resources: vello_cpu::Resources::new(),
            image_cache: ImageCache::new(config),
            clip_stack: Vec::new(),
        }
    }

    fn convert_paint(&mut self, paint: PaintRef<'_>) -> PaintType {
        match paint {
            Paint::Solid(alpha_color) => PaintType::Solid(alpha_color),
            Paint::Gradient(gradient) => PaintType::Gradient(gradient.clone()),
            Paint::Image(image) => PaintType::Image(ImageBrush {
                image: self
                    .image_cache
                    .get_or_register(&mut self.resources, image.image),
                sampler: image.sampler,
            }),
            // TODO: custom paint
            Paint::Resource(_) => PaintType::Solid(peniko::color::palette::css::TRANSPARENT),
            Paint::Custom(_) => PaintType::Solid(peniko::color::palette::css::TRANSPARENT),
        }
    }

    /// Advance the image cache's frame counter and evict stale entries.
    ///
    /// Should be called once per frame, after rendering.
    pub fn maintain(&mut self) {
        self.image_cache.maintain(&mut self.resources);
    }

    /// Drop all cached image conversions.
    pub fn clear_image_cache(&mut self) {
        self.image_cache.clear(&mut self.resources);
    }

    pub fn finish(mut self) -> Pixmap {
        let mut pixmap = Pixmap::new(self.render_ctx.width(), self.render_ctx.height());
        self.render_ctx.render(pixmap.as_mut(), &mut self.resources);
        pixmap
    }
}

impl VelloCpuScenePainter {
    /// raea: composites a Skia glyph-run coverage mask in `color` (premultiplied per pixel),
    /// at its device position.
    #[cfg(feature = "skia-text")]
    fn fill_glyph_mask(&mut self, mask: &anyrender_skia::GlyphMask, color: Color) {
        use peniko::{Extend, ImageAlphaType, ImageQuality, ImageSampler};
        use vello_common::paint::{Image, ImageSource};
        use vello_common::pixmap::PixelMetadata;
        if mask.width <= 0 || mask.height <= 0 {
            return;
        }
        let rgba = color.to_rgba8();
        let a = rgba.a as u32;
        let div255 = |v: u32| (v + 128 + ((v + 128) >> 8)) >> 8;
        let (r, g, b) = (div255(rgba.r as u32 * a), div255(rgba.g as u32 * a), div255(rgba.b as u32 * a));
        let mut data = Vec::with_capacity(mask.coverage.len() * 4);
        for &m in &mask.coverage {
            let m = m as u32;
            data.extend_from_slice(&[div255(r * m) as u8, div255(g * m) as u8, div255(b * m) as u8, div255(a * m) as u8]);
        }
        let pixmap = Pixmap::from_parts(
            data,
            mask.width as u16,
            mask.height as u16,
            PixelMetadata { may_have_transparency: true, alpha_type: ImageAlphaType::AlphaPremultiplied },
        );
        self.render_ctx.set_transform(Affine::IDENTITY);
        self.render_ctx.set_paint(PaintType::Image(Image {
            image: ImageSource::Pixmap(Arc::new(pixmap)),
            sampler: ImageSampler {
                x_extend: Extend::Pad,
                y_extend: Extend::Pad,
                quality: ImageQuality::Low,
                alpha: 1.0,
            },
        }));
        let (x, y) = (mask.x as f64, mask.y as f64);
        self.render_ctx.set_paint_transform(Affine::translate((x, y)));
        self.render_ctx.fill_rect(&Rect::new(x, y, x + mask.width as f64, y + mask.height as f64));
        self.render_ctx.reset_paint_transform();
    }
}

impl RenderContext for VelloCpuScenePainter {}
impl PaintScene for VelloCpuScenePainter {
    fn reset(&mut self) {
        self.render_ctx.reset();
        self.clip_stack.clear();
    }

    fn push_layer(
        &mut self,
        fill: Fill,
        blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
        filter: Option<Arc<Filter>>,
        _backdrop_filter: Option<Arc<Filter>>,
    ) {
        #[cfg(feature = "filters")]
        let filter = filter
            .and_then(crate::filters::convert_filter)
            .filter(|_| cfg!(not(feature = "multithreading")));

        #[cfg(not(feature = "filters"))]
        let filter = {
            let _ = filter;
            None
        };

        self.render_ctx.set_transform(transform);
        self.render_ctx.set_fill_rule(fill);
        self.render_ctx.push_layer(
            Some(&clip.into_path(DEFAULT_TOLERANCE)),
            Some(blend.into()),
            Some(alpha),
            None,
            filter,
        );
        self.clip_stack.push(false);
    }

    fn push_clip_layer(&mut self, fill: Fill, transform: Affine, clip: &impl Shape) {
        self.render_ctx.set_transform(transform);
        self.render_ctx.set_fill_rule(fill);
        // raea: a clip path, not an isolated layer, so mix-blend-mode inside an
        // overflow/clip-path clip blends with what is below it, as in browsers (and it's cheaper).
        self.render_ctx
            .push_clip_path(&clip.into_path(DEFAULT_TOLERANCE));
        self.clip_stack.push(true);
    }

    fn pop_layer(&mut self) {
        if self.clip_stack.pop() == Some(true) {
            self.render_ctx.pop_clip();
        } else {
            self.render_ctx.pop_layer();
        }
    }

    fn stroke<'a>(
        &mut self,
        style: &Stroke,
        transform: Affine,
        paint: impl Into<PaintRef<'a>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        self.render_ctx.set_transform(transform);
        self.render_ctx.set_stroke(style.clone());
        let paint = self.convert_paint(paint.into());
        self.render_ctx.set_paint(paint);
        self.render_ctx
            .set_paint_transform(brush_transform.unwrap_or(Affine::IDENTITY));
        self.render_ctx
            .stroke_path(&shape.into_path(DEFAULT_TOLERANCE));
    }

    fn fill<'a>(
        &mut self,
        style: Fill,
        transform: Affine,
        paint: impl Into<PaintRef<'a>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        let paint: PaintRef<'a> = paint.into();
        // raea: Chrome rasterizes paths with Skia's scan converter, whose coverage differs
        // from vello's analytic area along curves and slanted edges (rounded corners, circles, rotated
        // boxes). Solid fills with any of those take Skia's coverage mask (see skia_text.rs);
        // axis-aligned rectangles stay on vello, where both agree.
        #[cfg(feature = "skia-text")]
        if let Paint::Solid(color) = &paint {
            let path = shape.into_path(DEFAULT_TOLERANCE);
            let c = transform.as_coeffs();
            let mut last = kurbo::Point::ZERO;
            let needs_skia = c[1] != 0.0
                || c[2] != 0.0
                || path.elements().iter().any(|el| match *el {
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
                });
            // Large shapes (a full-frame card) stay on vello: their edges are a sliver of their area and
            // re-rasterizing them every frame they animate costs more than it buys.
            let dev = transform.transform_rect_bbox(path.bounding_box());
            if needs_skia && dev.area() <= MAX_SKIA_PATH_PIXELS {
                let clip = (0, 0, self.render_ctx.width() as i32, self.render_ctx.height() as i32);
                if let Some(mask) = crate::skia_text::path_mask(transform, &path, style, clip) {
                    self.fill_glyph_mask(&mask, *color);
                    return;
                }
            }
        }
        self.render_ctx.set_transform(transform);
        self.render_ctx.set_fill_rule(style);
        let paint = self.convert_paint(paint);
        self.render_ctx.set_paint(paint);
        self.render_ctx
            .set_paint_transform(brush_transform.unwrap_or(Affine::IDENTITY));
        self.render_ctx
            .fill_path(&shape.into_path(DEFAULT_TOLERANCE));
    }

    fn draw_glyphs<'a, 's: 'a>(
        &'a mut self,
        font: &'a FontData,
        font_size: f32,
        hint: bool,
        normalized_coords: &'a [NormalizedCoord],
        embolden: kurbo::Vec2,
        style: impl Into<StyleRef<'a>>,
        paint: impl Into<PaintRef<'a>>,
        _brush_alpha: f32,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: impl Iterator<Item = anyrender::Glyph> + Clone,
    ) {
        let paint: PaintRef<'a> = paint.into();
        let style: StyleRef<'a> = style.into();
        // raea: filled, solid-colour runs under an axis-aligned transform come from Skia's
        // glyph masks, as in Chrome (see skia_text.rs)
        #[cfg(feature = "skia-text")]
        if let (false, StyleRef::Fill(_), Paint::Solid(color)) = (anyrender::moving_text::get(), &style, &paint) {
            let c = transform.as_coeffs();
            if c[1] == 0.0 && c[2] == 0.0 && c[0] > 0.0 && c[3] > 0.0 {
                let snapped: Vec<anyrender::Glyph> = glyphs
                    .clone()
                    .map(|mut g| {
                        let dev = c[3] * g.y as f64 + c[5];
                        g.y += ((dev.round() - dev) / c[3]) as f32;
                        g
                    })
                    .collect();
                let clip = (0, 0, self.render_ctx.width() as i32, self.render_ctx.height() as i32);
                if let Some(mask) = crate::skia_text::glyph_run_mask(
                    font,
                    font_size,
                    normalized_coords,
                    embolden,
                    transform,
                    glyph_transform,
                    &snapped,
                    clip,
                ) {
                    self.fill_glyph_mask(&mask, *color);
                    return;
                }
            }
        }
        self.render_ctx.set_transform(transform);
        let paint = self.convert_paint(paint);
        self.render_ctx.set_paint(paint);

        // raea: glifo caches unhinted outlines at units-per-em size and applies the embolden
        // there, so a pixel amount would shrink by font_size / upem; pre-scale it into font units.
        let embolden = if !hint && (embolden.x != 0.0 || embolden.y != 0.0) {
            use skrifa::raw::TableProvider as _;
            let upem = skrifa::FontRef::from_index(font.data.data(), font.index)
                .ok()
                .and_then(|f| f.head().ok())
                .map(|h| h.units_per_em() as f64)
                .unwrap_or(font_size as f64);
            embolden * (upem / (font_size as f64).max(1e-3))
        } else {
            embolden
        };
        // raea: Chrome's Skia rounds each glyph's baseline to whole device pixels when the
        // transform is axis-aligned (horizontal subpixel positioning only); unhinted outlines here keep
        // their fractional y, so snap it the same way.
        let c = transform.as_coeffs();
        let axis_aligned = c[1] == 0.0 && c[2] == 0.0 && c[3] != 0.0 && glyph_transform.is_none_or(|g| {
            let g = g.as_coeffs();
            g[1] == 0.0
        });
        let snap_y = !hint && axis_aligned && !anyrender::moving_text::get();
        let glyphs = glyphs.map(move |mut g| {
            if snap_y {
                let dev = c[3] * g.y as f64 + c[5];
                g.y += ((dev.round() - dev) / c[3]) as f32;
            }
            g
        });
        match style {
            StyleRef::Fill(fill) => {
                self.render_ctx.set_fill_rule(fill);
                let _ = self
                    .render_ctx
                    .glyph_run(&mut self.resources, font)
                    .font_size(font_size)
                    .hint(hint)
                    .normalized_coords(normalized_coords)
                    .font_embolden(FontEmbolden::new(Diagonal2::new(embolden.x, embolden.y)))
                    .glyph_transform(glyph_transform.unwrap_or_default())
                    .fill_glyphs(glyphs.map(|g| vello_cpu::Glyph {
                        id: g.id,
                        x: g.x,
                        y: g.y,
                    }));
            }
            StyleRef::Stroke(stroke) => {
                self.render_ctx.set_stroke(stroke.clone());
                let _ = self
                    .render_ctx
                    .glyph_run(&mut self.resources, font)
                    .font_size(font_size)
                    .hint(hint)
                    .normalized_coords(normalized_coords)
                    .glyph_transform(glyph_transform.unwrap_or_default())
                    .stroke_glyphs(glyphs.map(|g| vello_cpu::Glyph {
                        id: g.id,
                        x: g.x,
                        y: g.y,
                    }));
            }
        }
    }
    fn draw_box_shadow(
        &mut self,
        transform: Affine,
        rect: Rect,
        color: Color,
        radius: f64,
        std_dev: f64,
    ) {
        // raea: Skia's blur mask filter (Chrome's software raster) when Skia is built in;
        // vello's analytic blurred rounded rect has different tails
        #[cfg(feature = "skia-text")]
        {
            let clip = (0, 0, self.render_ctx.width() as i32, self.render_ctx.height() as i32);
            if let Some(mask) = crate::skia_text::box_shadow_mask(transform, rect, radius, std_dev, clip) {
                self.fill_glyph_mask(&mask, color);
                return;
            }
        }
        self.render_ctx.set_transform(transform);
        self.render_ctx.set_paint(PaintType::Solid(color));
        self.render_ctx.reset_paint_transform();
        self.render_ctx
            .fill_blurred_rounded_rect(&rect, radius as f32, std_dev as f32, false);
    }
}

#[cfg(test)]
mod clip_rule_tests {
    use anyrender::{PaintScene, recording::Scene, render_to_buffer};
    use kurbo::{Affine, BezPath, Rect};
    use peniko::{Fill, Mix, color::palette::css::RED};

    use crate::VelloCpuImageRenderer;

    fn render_clip(fill: Fill, compositing_layer: bool, replay: bool) -> Vec<u8> {
        let path = BezPath::from_svg("M0 0H100V100H0Z M25 25H75V75H25Z").unwrap();
        let draw = |scene: &mut crate::VelloCpuScenePainter| {
            if replay {
                let mut recording = Scene::new();
                draw_clipped(&mut recording, fill, compositing_layer, &path);
                scene.append_scene(recording, Affine::IDENTITY);
            } else {
                draw_clipped(scene, fill, compositing_layer, &path);
            }
        };
        render_to_buffer::<VelloCpuImageRenderer, _>(draw, 100, 100)
    }

    fn draw_clipped(
        scene: &mut impl PaintScene,
        fill: Fill,
        compositing_layer: bool,
        path: &BezPath,
    ) {
        if compositing_layer {
            scene.push_layer(fill, Mix::Normal, 1.0, Affine::IDENTITY, path, None, None);
        } else {
            scene.push_clip_layer(fill, Affine::IDENTITY, path);
        }
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            RED,
            None,
            &Rect::new(0.0, 0.0, 100.0, 100.0),
        );
        scene.pop_layer();
    }

    fn assert_pixels(compositing_layer: bool, replay: bool) {
        for fill in [Fill::NonZero, Fill::EvenOdd] {
            let buffer = render_clip(fill, compositing_layer, replay);
            let pixel = |x: usize, y: usize| &buffer[(y * 100 + x) * 4..(y * 100 + x) * 4 + 4];
            assert_eq!(pixel(10, 10), &[255, 0, 0, 255]);
            assert_eq!(
                pixel(50, 50),
                if fill == Fill::EvenOdd {
                    &[0, 0, 0, 0]
                } else {
                    &[255, 0, 0, 255]
                }
            );
        }
    }

    #[test]
    fn clip_layers_respect_fill_rule() {
        assert_pixels(false, false);
    }

    #[test]
    fn compositing_layers_respect_fill_rule() {
        assert_pixels(true, false);
    }

    #[test]
    fn scene_replay_preserves_clip_rules() {
        assert_pixels(false, true);
        assert_pixels(true, true);
    }
}
