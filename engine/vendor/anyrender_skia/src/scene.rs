use std::sync::Arc;

use anyrender::filters::component_transfer::TransferFunction;
use anyrender::filters::composite::CompositeOperator;
use anyrender::filters::displacement::ColorChannel;
use anyrender::filters::lighting::{DistantLightSource, LightSource};
use anyrender::filters::morphology::MorphologyOperator;
use anyrender::filters::turbulence::TurbulenceType;
use anyrender::filters::{Filter, FilterEffect, FilterId, FilterInput};
use anyrender::{PaintScene, RenderContext};
use peniko::color::AlphaColor;
use skia_safe::{
    BlendMode as SkBlendMode, BlurStyle, Canvas, Color, ColorSpace, Font, FontArguments,
    FontHinting, FontMgr, GlyphId, ImageFilter, MaskFilter, Paint, PaintCap, PaintJoin, PaintStyle,
    PathEffect, Point, Point3, RRect, Rect, Shader, Typeface,
    canvas::{GlyphPositions, SaveLayerRec},
    font::Edging,
    font_arguments::{VariationPosition, variation_position::Coordinate},
};

use crate::cache::{
    FontCacheKey, FontCacheKeyBorrowed, GenerationalCache, NormalizedTypefaceCacheKey,
    NormalizedTypefaceCacheKeyBorrowed,
};
use crate::scene::sk_peniko::color4f_from_alpha_color;

/// raea: an 8-bit coverage mask of a glyph run at device position (`x`, `y`)
/// (see [`SkiaSceneCache::glyph_run_mask`]).
pub struct GlyphMask {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub coverage: Vec<u8>,
}

pub struct SkiaSceneCache {
    paint: Paint,
    dash_intervals: Vec<f32>,
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    extracted_font_data: GenerationalCache<(u64, u32), peniko::FontData>,
    typeface: GenerationalCache<(u64, u32), Typeface>,
    normalized_typeface: GenerationalCache<NormalizedTypefaceCacheKey, Typeface>,
    // The `Blob` is held alongside the `Shader` because the shader references the
    // image pixels without owning them (see `shader_from_image_brush`).
    image_shader: GenerationalCache<u64, (Shader, peniko::Blob<u8>)>,
    font: GenerationalCache<FontCacheKey, Font>,
    font_mgr: FontMgr,
    glyph_id_buf: Vec<GlyphId>,
    glyph_pos_buf: Vec<Point>,
}

impl SkiaSceneCache {
    /// Create a new `SkiaSceneCache`
    pub fn new() -> Self {
        Default::default()
    }

    /// raea: an outset box shadow's 8-bit coverage, drawn exactly as `draw_box_shadow` draws it
    /// (Skia's blur mask filter on a nine-patch rounded rect, as Chrome's software raster does), for
    /// the vello backend. `transform` must be an axis-aligned scale + translate; the mask is clipped to
    /// `clip` (device x0, y0, x1, y1).
    pub fn box_shadow_mask(
        &mut self,
        transform: kurbo::Affine,
        rect: kurbo::Rect,
        radius: f64,
        std_dev: f64,
        clip: (i32, i32, i32, i32),
    ) -> Option<GlyphMask> {
        use skia_safe::{AlphaType, ColorType, ImageInfo, surfaces};
        let [a, b, c, d, _, _] = transform.as_coeffs();
        if b != 0.0 || c != 0.0 || a <= 0.0 || d <= 0.0 {
            return None;
        }
        // Skia's blur reaches ~3 sigma past the shape; a little more for the nine-patch rounding
        let reach = 3.0 * std_dev + 2.0;
        let dev = transform.transform_rect_bbox(rect.inflate(reach, reach));
        let bx = (dev.x0.floor() as i32).max(clip.0);
        let by = (dev.y0.floor() as i32).max(clip.1);
        let bx1 = (dev.x1.ceil() as i32).min(clip.2);
        let by1 = (dev.y1.ceil() as i32).min(clip.3);
        let empty = GlyphMask { x: 0, y: 0, width: 0, height: 0, coverage: Vec::new() };
        if bx1 <= bx || by1 <= by {
            return Some(empty);
        }
        let (w, h) = (bx1 - bx, by1 - by);
        let info = ImageInfo::new((w, h), ColorType::Alpha8, AlphaType::Premul, None);
        let mut surface = surfaces::raster(&info, None, None)?;
        surface.canvas().clear(Color::TRANSPARENT);
        {
            let mut painter = SkiaScenePainter { inner: surface.canvas(), cache: &mut *self };
            painter.draw_box_shadow(
                kurbo::Affine::translate((-bx as f64, -by as f64)) * transform,
                rect,
                peniko::color::palette::css::WHITE,
                radius,
                std_dev,
            );
        }
        let mut coverage = vec![0u8; (w * h) as usize];
        if !surface.read_pixels(&info, &mut coverage, w as usize, (0, 0)) {
            return None;
        }
        Some(GlyphMask { x: bx, y: by, width: w, height: h, coverage })
    }

    /// raea: 8-bit antialiased coverage of `path` filled under `transform`, as Skia's scan
    /// converter produces it (Chrome's software raster), clipped to `clip` (device x0, y0, x1, y1).
    pub fn path_mask(
        &mut self,
        transform: kurbo::Affine,
        path: &kurbo::BezPath,
        fill: peniko::Fill,
        clip: (i32, i32, i32, i32),
    ) -> Option<GlyphMask> {
        use skia_safe::{AlphaType, ColorType, ImageInfo, surfaces};
        let dev = transform.transform_rect_bbox(kurbo::Shape::bounding_box(path));
        let bx = (dev.x0.floor() as i32 - 1).max(clip.0);
        let by = (dev.y0.floor() as i32 - 1).max(clip.1);
        let bx1 = (dev.x1.ceil() as i32 + 1).min(clip.2);
        let by1 = (dev.y1.ceil() as i32 + 1).min(clip.3);
        if bx1 <= bx || by1 <= by {
            return Some(GlyphMask { x: 0, y: 0, width: 0, height: 0, coverage: Vec::new() });
        }
        let (w, h) = (bx1 - bx, by1 - by);
        let info = ImageInfo::new((w, h), ColorType::Alpha8, AlphaType::Premul, None);
        let mut surface = surfaces::raster(&info, None, None)?;
        surface.canvas().clear(Color::TRANSPARENT);
        {
            let mut painter = SkiaScenePainter { inner: surface.canvas(), cache: &mut *self };
            sk_kurbo::ARCS_AS_CONICS.with(|a| a.set(true));
            anyrender::PaintScene::fill(
                &mut painter,
                fill,
                kurbo::Affine::translate((-bx as f64, -by as f64)) * transform,
                peniko::color::palette::css::WHITE,
                None,
                path,
            );
            sk_kurbo::ARCS_AS_CONICS.with(|a| a.set(false));
        }
        let mut coverage = vec![0u8; (w * h) as usize];
        if !surface.read_pixels(&info, &mut coverage, w as usize, (0, 0)) {
            return None;
        }
        Some(GlyphMask { x: bx, y: by, width: w, height: h, coverage })
    }

    /// raea: rasterizes a glyph run exactly as `draw_glyphs` draws it (Skia's glyph masks:
    /// CoreText on Apple platforms, like Chrome) into 8-bit coverage, for the vello backend's text.
    /// `transform` must be an axis-aligned scale + translate and `glyph_transform` absent or a pure
    /// horizontal skew; otherwise `None`. The mask is clipped to `clip` (device x0, y0, x1, y1).
    pub fn glyph_run_mask(
        &mut self,
        font: &peniko::FontData,
        font_size: f32,
        normalized_coords: &[anyrender::NormalizedCoord],
        embolden: kurbo::Vec2,
        transform: kurbo::Affine,
        glyph_transform: Option<kurbo::Affine>,
        glyphs: &[anyrender::Glyph],
        clip: (i32, i32, i32, i32),
    ) -> Option<GlyphMask> {
        use skia_safe::{AlphaType, ColorType, ImageInfo, PixelGeometry, SurfaceProps, surfaces};
        use std::cell::RefCell;
        thread_local! {
            static SCRATCH: RefCell<Option<skia_safe::Surface>> = const { RefCell::new(None) };
        }
        let [a, b, c, d, _, _] = transform.as_coeffs();
        if b != 0.0 || c != 0.0 || a <= 0.0 || d <= 0.0 {
            return None;
        }
        let font_skew = match glyph_transform {
            None => None,
            Some(g) => match g.as_coeffs() {
                [1.0, 0.0, k, 1.0, 0.0, 0.0] => Some(k as f32),
                _ => return None,
            },
        };
        // the font draw_glyphs would use, for the run's bounds
        let mut sk_font = SCRATCH.with(|scratch| {
            let mut scratch = scratch.borrow_mut();
            let surface = scratch.get_or_insert_with(|| surfaces::raster_n32_premul((1, 1)).unwrap());
            let mut painter = SkiaScenePainter { inner: surface.canvas(), cache: &mut *self };
            painter.get_or_cache_font(font, normalized_coords, font_size, false)
        })?;
        if embolden.x > 0.0 || embolden.y > 0.0 {
            sk_font.set_embolden(true);
        }
        if let Some(k) = font_skew {
            sk_font.set_skew_x(k);
        }
        let ids: Vec<GlyphId> = glyphs.iter().map(|g| GlyphId::from(g.id as u16)).collect();
        let mut rects = vec![Rect::default(); ids.len()];
        sk_font.get_bounds(&ids, &mut rects, None);
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for (r, g) in rects.iter().zip(glyphs) {
            if r.is_empty() {
                continue;
            }
            let p0 = transform * kurbo::Point::new((g.x + r.left) as f64, (g.y + r.top) as f64);
            let p1 = transform * kurbo::Point::new((g.x + r.right) as f64, (g.y + r.bottom) as f64);
            x0 = x0.min(p0.x);
            y0 = y0.min(p0.y);
            x1 = x1.max(p1.x);
            y1 = y1.max(p1.y);
        }
        let empty = GlyphMask { x: 0, y: 0, width: 0, height: 0, coverage: Vec::new() };
        if x0 > x1 {
            return Some(empty);
        }
        // a couple of pixels for antialiasing and fake bold
        let bx = ((x0 - 2.0).floor() as i32).max(clip.0);
        let by = ((y0 - 2.0).floor() as i32).max(clip.1);
        let bx1 = ((x1 + 2.0).ceil() as i32).min(clip.2);
        let by1 = ((y1 + 2.0).ceil() as i32).min(clip.3);
        if bx1 <= bx || by1 <= by {
            return Some(empty);
        }
        let (w, h) = (bx1 - bx, by1 - by);
        let info = ImageInfo::new((w, h), ColorType::Alpha8, AlphaType::Premul, None);
        // linear text blending, as the image renderer and Chrome's software raster use
        let props = SurfaceProps::new_with_text_properties(Default::default(), PixelGeometry::Unknown, 0.0, 1.0);
        let mut surface = surfaces::raster(&info, None, Some(&props))?;
        surface.canvas().clear(Color::TRANSPARENT);
        {
            let mut painter = SkiaScenePainter { inner: surface.canvas(), cache: &mut *self };
            painter.draw_glyphs(
                font,
                font_size,
                false,
                normalized_coords,
                embolden,
                peniko::Fill::NonZero,
                peniko::color::palette::css::WHITE,
                1.0,
                kurbo::Affine::translate((-bx as f64, -by as f64)) * transform,
                glyph_transform,
                glyphs.iter().copied(),
            );
        }
        let mut coverage = vec![0u8; (w * h) as usize];
        if !surface.read_pixels(&info, &mut coverage, w as usize, (0, 0)) {
            return None;
        }
        Some(GlyphMask { x: bx, y: by, width: w, height: h, coverage })
    }

    /// Increment the generation on the caches. Should be called once per frame.
    pub fn next_gen(&mut self) {
        self.typeface.next_gen();
        self.normalized_typeface.next_gen();
        self.image_shader.next_gen();
        self.font.next_gen();
    }
}

impl Default for SkiaSceneCache {
    fn default() -> Self {
        Self {
            paint: Paint::default(),
            dash_intervals: Vec::new(),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            extracted_font_data: GenerationalCache::new(10),
            typeface: GenerationalCache::new(1),
            normalized_typeface: GenerationalCache::new(1),
            image_shader: GenerationalCache::new(1),
            font: GenerationalCache::new(10),
            font_mgr: FontMgr::new(),
            glyph_id_buf: Default::default(),
            glyph_pos_buf: Default::default(),
        }
    }
}

pub struct SkiaScenePainter<'a> {
    pub(crate) inner: &'a Canvas,
    pub(crate) cache: &'a mut SkiaSceneCache,
}

impl SkiaScenePainter<'_> {
    pub fn new<'a>(canvas: &'a Canvas, cache: &'a mut SkiaSceneCache) -> SkiaScenePainter<'a> {
        SkiaScenePainter {
            inner: canvas,
            cache,
        }
    }

    fn reset_paint(&mut self) {
        self.cache.paint.reset();
        self.cache.paint.set_anti_alias(true);
    }

    fn set_paint_alpha(&mut self, alpha: f32) {
        self.cache.paint.set_alpha_f(alpha);
    }

    fn set_paint_filter(&mut self, filter: ImageFilter) {
        self.cache.paint.set_image_filter(filter);
    }

    fn set_paint_blend_mode(&mut self, blend_mode: impl Into<peniko::BlendMode>) {
        self.cache
            .paint
            .set_blend_mode(sk_peniko::blend_mode_from(blend_mode.into()));
    }

    fn set_matrix(&self, transform: kurbo::Affine) {
        self.inner.set_matrix(&sk_kurbo::m44_from_affine(transform));
    }

    fn concat_matrix(&self, transform: kurbo::Affine) {
        self.inner.concat(&sk_kurbo::matrix_from_affine(transform));
    }

    fn clip(&self, fill: peniko::Fill, shape: &impl kurbo::Shape) {
        if let Some(rect) = shape.as_rect() {
            self.inner.clip_rect(sk_kurbo::rect_from(rect), None, true);
        } else if let Some(rrect) = shape.as_rounded_rect() {
            self.inner
                .clip_rrect(sk_kurbo::rrect_from(rrect), None, true);
        } else {
            let mut path = sk_kurbo::path_from_shape(shape);
            path.set_fill_type(sk_peniko::path_fill_type_from_fill(fill));
            self.inner.clip_path(&path, None, true);
        }
    }

    fn set_paint_brush<'a>(
        &mut self,
        brush: impl Into<anyrender::PaintRef<'a>>,
        brush_transform: Option<kurbo::Affine>,
    ) {
        let brush: anyrender::PaintRef<'a> = brush.into();
        match brush {
            anyrender::Paint::Solid(alpha_color) => {
                self.cache.paint.set_color4f(
                    sk_peniko::color4f_from_alpha_color(alpha_color),
                    &ColorSpace::new_srgb(),
                );
            }
            anyrender::Paint::Gradient(gradient) => {
                self.cache
                    .paint
                    .set_shader(sk_peniko::shader_from_gradient(gradient, brush_transform));
            }
            anyrender::Paint::Image(image_brush) => {
                if let Some((shader, _)) = self.cache.image_shader.hit(&image_brush.image.data.id())
                {
                    self.cache.paint.set_shader(shader.clone());
                    return;
                }

                let image_shader = sk_peniko::shader_from_image_brush(image_brush, brush_transform);

                if let Some(shader) = &image_shader {
                    self.cache.image_shader.insert(
                        image_brush.image.data.id(),
                        (shader.clone(), image_brush.image.data.clone()),
                    );
                }

                self.cache.paint.set_shader(image_shader);
            }

            // Render unsupported custom paints as transparent
            anyrender::Paint::Resource(_) | anyrender::Paint::Custom(_) => {
                self.cache.paint.set_color4f(
                    sk_peniko::color4f_from_alpha_color(AlphaColor::TRANSPARENT),
                    &ColorSpace::new_srgb(),
                );
            }
        }
    }

    fn set_paint_style<'a>(&mut self, style: impl Into<peniko::StyleRef<'a>>) {
        match style.into() {
            peniko::StyleRef::Fill(_) => {
                self.cache.paint.set_style(PaintStyle::Fill);
            }
            peniko::StyleRef::Stroke(stroke) => {
                self.cache.paint.set_style(PaintStyle::Stroke);
                self.cache.paint.set_stroke(true);
                self.cache.paint.set_stroke_width(stroke.width as f32);
                self.cache.paint.set_stroke_miter(stroke.miter_limit as f32);
                self.cache.paint.set_stroke_join(match stroke.join {
                    kurbo::Join::Bevel => PaintJoin::Bevel,
                    kurbo::Join::Miter => PaintJoin::Miter,
                    kurbo::Join::Round => PaintJoin::Round,
                });
                self.cache.paint.set_stroke_cap(match stroke.start_cap {
                    kurbo::Cap::Butt => PaintCap::Butt,
                    kurbo::Cap::Square => PaintCap::Square,
                    kurbo::Cap::Round => PaintCap::Round,
                });
                if stroke.dash_pattern.is_empty() {
                    self.cache.paint.set_path_effect(None);
                } else {
                    self.cache.dash_intervals.clear();
                    self.cache
                        .dash_intervals
                        .extend(stroke.dash_pattern.iter().map(|dash| *dash as f32));
                    self.cache.paint.set_path_effect(PathEffect::dash(
                        self.cache.dash_intervals.as_slice(),
                        stroke.dash_offset as f32,
                    ));
                }
            }
        }
    }

    fn draw_shape(&mut self, shape: &impl kurbo::Shape) {
        self.draw_shape_with_fill(shape, None);
    }

    fn draw_shape_with_fill(
        &mut self,
        shape: &impl kurbo::Shape,
        fill: impl Into<Option<peniko::Fill>>,
    ) {
        if let Some(rect) = shape.as_rect() {
            self.inner
                .draw_rect(sk_kurbo::rect_from(rect), &self.cache.paint);
        } else if let Some(rrect) = shape.as_rounded_rect() {
            self.inner
                .draw_rrect(sk_kurbo::rrect_from(rrect), &self.cache.paint);
        } else if let Some(line) = shape.as_line() {
            self.inner.draw_line(
                (line.p0.x as f32, line.p0.y as f32),
                (line.p1.x as f32, line.p1.y as f32),
                &self.cache.paint,
            );
        } else if let Some(circle) = shape.as_circle() {
            self.inner.draw_circle(
                (circle.center.x as f32, circle.center.y as f32),
                circle.radius as f32,
                &self.cache.paint,
            );
        } else {
            let mut path = sk_kurbo::path_from_shape(shape);
            if let Some(fill) = fill.into() {
                path.set_fill_type(sk_peniko::path_fill_type_from_fill(fill));
            }
            self.inner.draw_path(&path, &self.cache.paint);
        }
    }

    fn get_or_cache_font(
        &mut self,
        font: &peniko::FontData,
        normalized_coords: &[anyrender::NormalizedCoord],
        font_size: f32,
        hint: bool,
    ) -> Option<Font> {
        let cache_key_borrowed = FontCacheKeyBorrowed {
            typeface_id: font.data.id(),
            typeface_index: font.index,
            normalized_coords,
            font_size: font_size.to_bits(),
            hint,
        };

        if let Some(cached) = self.cache.font.hit(&cache_key_borrowed) {
            return Some(cached.clone());
        }

        let typeface = self.get_or_cache_normalized_typeface(font, normalized_coords)?;

        let cache_key = FontCacheKey {
            typeface_id: font.data.id(),
            typeface_index: font.index,
            normalized_coords: normalized_coords.to_vec(),
            font_size: font_size.to_bits(),
            hint,
        };

        let mut font = Font::from_typeface(typeface, font_size);
        // raea: Chrome on macOS doesn't hint (with CoreText typefaces, Skia's Normal
        // hinting turns on font smoothing, which thickens glyphs)
        let _ = hint;
        font.set_hinting(FontHinting::None);
        font.set_subpixel(true);
        // Chrome positions glyphs at fractional baselines too (vello and Chrome agree; Skia's default
        // snaps the baseline to whole pixels)
        font.set_baseline_snap(false);
        // raea: Blink on macOS asks for subpixel AA (FontPlatformData::CreateSkFont); with no
        // LCD pixel geometry Skia turns that into A8 masks generated "from LCD", which only changes
        // glyphs drawn from paths (synthetic bold, stroked text): rasterized at 4x horizontally and
        // packed through the LCD filter, they carry more ink than plain A8. CoreText-drawn glyphs
        // without hinting come out the same either way.
        font.set_edging(Edging::SubpixelAntiAlias);

        self.cache.font.insert(cache_key, font.clone());

        Some(font)
    }

    fn get_or_cache_normalized_typeface(
        &mut self,
        font: &peniko::FontData,
        normalized_coords: &[anyrender::NormalizedCoord],
    ) -> Option<Typeface> {
        fn f2dot14_to_f32(raw_value: i16) -> f32 {
            let int = (raw_value >> 14) as f32;
            let fract = (raw_value & !(!0 << 14)) as f32 / (1 << 14) as f32;
            int + fract
        }

        // raea: a variable font drawn at its default instance still gets every axis set
        // explicitly (as Blink does): CoreText's implicit default instance of e.g. Work Sans draws
        // ~5% heavier and 0.3 px higher than wght=400 set explicitly.
        if normalized_coords.is_empty() {
            let typeface = self.get_or_cache_typeface(font)?;
            if typeface.variation_design_parameters().unwrap_or_default().is_empty() {
                return Some(typeface);
            }
        }

        let cache_key_borrowed = NormalizedTypefaceCacheKeyBorrowed {
            typeface_id: font.data.id(),
            typeface_index: font.index,
            normalized_coords,
        };

        if let Some(cached) = self.cache.normalized_typeface.hit(&cache_key_borrowed) {
            return Some(cached.clone());
        }

        let typeface = self.get_or_cache_typeface(font)?;

        let axes = typeface.variation_design_parameters().unwrap_or_default();

        if axes.is_empty() {
            return Some(typeface);
        }

        // raea: map the layout's normalized coordinates back to design values exactly (avar
        // included). The upstream lerp ran backwards below the default, so on variable fonts weight 100
        // drew at 400 and 300 near 200.
        let user = design_coords(font.data.data(), font.index, normalized_coords);
        let coordinates: Vec<Coordinate> = axes
            .iter()
            .zip(normalized_coords.iter().map(|c| f2dot14_to_f32(*c)).chain(std::iter::repeat(0.0)))
            .enumerate()
            .map(|(i, (axis, factor))| {
                let value = user.as_ref().and_then(|u| u.get(i).copied().flatten()).unwrap_or_else(|| {
                    if factor < 0.0 {
                        lerp_f32(axis.def, axis.min, -factor)
                    } else {
                        lerp_f32(axis.def, axis.max, factor)
                    }
                });
                Coordinate {
                    axis: axis.tag,
                    value,
                }
            })
            .collect();
        let variation_position = VariationPosition {
            coordinates: &coordinates,
        };

        let normalized_typeface = typeface
            .clone_with_arguments(
                &FontArguments::new().set_variation_design_position(variation_position),
            )
            .unwrap();

        self.cache.normalized_typeface.insert(
            NormalizedTypefaceCacheKey {
                typeface_id: font.data.id(),
                typeface_index: font.index,
                normalized_coords: normalized_coords.to_vec(),
            },
            normalized_typeface.clone(),
        );

        Some(normalized_typeface)
    }

    fn get_or_cache_typeface<'a>(
        &'a mut self,
        #[allow(unused_mut)] mut font: &'a peniko::FontData,
    ) -> Option<Typeface> {
        let cache_key = (font.data.id(), font.index);

        #[cfg(any(target_os = "macos", target_os = "ios"))]
        #[allow(clippy::map_entry, reason = "Cannot early-return with entry API")]
        {
            use peniko::Blob;
            use std::sync::Arc;

            if let Some(collection) = oaty::Collection::new(font.data.data()) {
                if !self.cache.extracted_font_data.contains_key(&cache_key) {
                    let Some(data) = collection
                        .get_font(font.index)
                        .and_then(|font| font.copy_data())
                    else {
                        eprintln!(
                            "WARNING: failed to extract font {} {}",
                            cache_key.0, cache_key.1
                        );
                        return None;
                    };

                    let blob = Blob::new(Arc::new(data));
                    let font_data = peniko::FontData::new(blob, 0);
                    self.cache.extracted_font_data.insert(cache_key, font_data);
                }
                font = self.cache.extracted_font_data.hit(&cache_key).unwrap()
            }
        }

        if let Some(cached) = self.cache.typeface.hit(&cache_key) {
            return Some(cached.clone());
        }

        let Some(typeface) = self
            .cache
            .font_mgr
            .new_from_bytes(font.data.data(), font.index)
        else {
            let tf = Typeface::make_deserialize(font.data.data(), None);
            eprintln!(
                "WARNING: failed to load font {} {} {} {}",
                cache_key.0,
                cache_key.1,
                tf.is_some(),
                font.index
            );
            return None;
        };

        self.cache.typeface.insert(cache_key, typeface.clone());

        Some(typeface)
    }
}

impl RenderContext for SkiaScenePainter<'_> {}
impl PaintScene for SkiaScenePainter<'_> {
    fn reset(&mut self) {
        self.inner.clear(Color::WHITE);
    }

    fn push_layer(
        &mut self,
        fill: peniko::Fill,
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: kurbo::Affine,
        clip: &impl kurbo::Shape,
        filter: Option<Arc<Filter>>,
        backdrop_filter: Option<Arc<Filter>>,
    ) {
        let blend: peniko::BlendMode = blend.into();

        self.reset_paint();
        self.set_paint_alpha(alpha);
        self.set_paint_blend_mode(blend);

        // raea: blur radii are quantized in device pixels (chrome_sigma)
        let [a, b, c, d, _, _] = transform.as_coeffs();
        LAYER_SCALE.with(|s| s.set(((a * d - b * c).abs().sqrt() as f32).max(1e-6)));
        if let Some(filter) = filter.as_ref().and_then(|f| convert_filter(f)) {
            self.set_paint_filter(filter);
        }

        self.inner.save();

        self.set_matrix(transform);
        self.clip(fill, clip);

        let backdrop_filter = backdrop_filter.as_ref().and_then(|f| convert_backdrop_filter(f, clip.bounding_box()));
        let mut save_layer_rec = SaveLayerRec::default().paint(&self.cache.paint);
        if let Some(backdrop_filter) = backdrop_filter.as_ref() {
            if alpha < 1.0 {
                // raea: Chrome's compositor draws the filtered backdrop with the element's
                // opacity, then the element's own painting with that opacity on top, rather than both
                // as one group (backdrop-filter/filters: blur + invert at opacity 0.8)
                let mut backdrop_paint = skia_safe::Paint::default();
                backdrop_paint.set_alpha_f(alpha);
                self.inner.save_layer(&SaveLayerRec::default().paint(&backdrop_paint).backdrop(backdrop_filter));
                self.inner.restore();
            } else {
                save_layer_rec = save_layer_rec.backdrop(backdrop_filter);
            }
        }

        self.inner.save_layer(&save_layer_rec);
    }

    fn push_clip_layer(
        &mut self,
        fill: peniko::Fill,
        transform: kurbo::Affine,
        clip: &impl kurbo::Shape,
    ) {
        self.inner.save(); // we need to do two saves because of pop_layer

        self.set_matrix(transform);
        self.clip(fill, clip);
        self.inner.save();
    }

    fn pop_layer(&mut self) {
        self.inner.restore();
        self.inner.restore();
    }

    fn stroke<'a>(
        &mut self,
        style: &kurbo::Stroke,
        transform: kurbo::Affine,
        brush: impl Into<anyrender::PaintRef<'a>>,
        brush_transform: Option<kurbo::Affine>,
        shape: &impl kurbo::Shape,
    ) {
        self.set_matrix(transform);

        self.reset_paint();
        self.set_paint_brush(brush, brush_transform);
        self.set_paint_style(style);
        self.draw_shape(shape);
    }

    fn fill<'a>(
        &mut self,
        style: peniko::Fill,
        transform: kurbo::Affine,
        brush: impl Into<anyrender::PaintRef<'a>>,
        brush_transform: Option<kurbo::Affine>,
        shape: &impl kurbo::Shape,
    ) {
        self.set_matrix(transform);

        self.reset_paint();
        self.set_paint_brush(brush, brush_transform);
        self.set_paint_style(style);
        self.draw_shape_with_fill(shape, style);
    }

    fn draw_glyphs<'a, 's: 'a>(
        &'s mut self,
        #[allow(unused_mut)] mut font: &'a peniko::FontData,
        font_size: f32,
        hint: bool,
        normalized_coords: &'a [anyrender::NormalizedCoord],
        embolden: kurbo::Vec2,
        style: impl Into<peniko::StyleRef<'a>>,
        brush: impl Into<anyrender::PaintRef<'a>>,
        brush_alpha: f32,
        transform: kurbo::Affine,
        glyph_transform: Option<kurbo::Affine>,
        glyphs: impl Iterator<Item = anyrender::Glyph>,
    ) {
        // raea: axis-aligned solid fills come from the A8 glyph mask the vello backend
        // composites (it matches Chrome; drawn straight onto the RGBA surface, Skia's glyph edges
        // differ: 0.988 on bold 40 px text)
        let brush: anyrender::PaintRef<'a> = brush.into();
        let style: peniko::StyleRef<'a> = style.into();
        let glyphs: Vec<anyrender::Glyph> = glyphs.collect();
        if matches!(brush, anyrender::Paint::Solid(_))
            && matches!(style, peniko::StyleRef::Fill(_))
            && !IN_GLYPH_MASK.with(|f| f.get())
            && !is_color_font(font)
        {
            let b = self.inner.device_clip_bounds().unwrap_or_default();
            // baselines on whole device pixels, as Chrome (and the vello backend) draw them
            let c = transform.as_coeffs();
            let snapped: Vec<anyrender::Glyph> = glyphs
                .iter()
                .map(|g| {
                    let mut g = *g;
                    if c[3] > 0.0 {
                        let dev = c[3] * g.y as f64 + c[5];
                        g.y += ((dev.round() - dev) / c[3]) as f32;
                    }
                    g
                })
                .collect();
            IN_GLYPH_MASK.with(|f| f.set(true));
            let mask = self.cache.glyph_run_mask(
                font,
                font_size,
                normalized_coords,
                embolden,
                transform,
                glyph_transform,
                &snapped,
                (b.left, b.top, b.right, b.bottom),
            );
            IN_GLYPH_MASK.with(|f| f.set(false));
            if let Some(mask) = mask {
                if mask.width > 0 && mask.height > 0 {
                    let info = skia_safe::ImageInfo::new(
                        (mask.width, mask.height),
                        skia_safe::ColorType::Alpha8,
                        skia_safe::AlphaType::Premul,
                        None,
                    );
                    let data = skia_safe::Data::new_copy(&mask.coverage);
                    if let Some(image) = skia_safe::images::raster_from_data(&info, data, mask.width as usize) {
                        self.reset_paint();
                        self.set_paint_brush(brush, None);
                        if brush_alpha != 1.0 {
                            let a = self.cache.paint.alpha_f() * brush_alpha;
                            self.set_paint_alpha(a);
                        }
                        self.inner.save();
                        self.inner.reset_matrix();
                        // an alpha-only image draws in the paint's colour
                        self.inner.draw_image(&image, (mask.x as f32, mask.y as f32), Some(&self.cache.paint));
                        self.inner.restore();
                    }
                }
                return;
            }
        }
        let glyphs = glyphs.into_iter();
        self.set_matrix(transform);

        // a pure horizontal skew (synthetic oblique) is a per-glyph font skew, as in Chrome
        // (SkFont::setSkewX); concatenating it onto the canvas would also shear the glyph positions
        let font_skew = glyph_transform.and_then(|g| {
            let [a, b, c, d, e, f] = g.as_coeffs();
            (a == 1.0 && b == 0.0 && d == 1.0 && e == 0.0 && f == 0.0).then_some(c as f32)
        });
        if let Some(glyph_transform) = glyph_transform.filter(|_| font_skew.is_none()) {
            self.concat_matrix(glyph_transform);
        }

        self.reset_paint();
        self.set_paint_brush(brush, None);
        self.set_paint_style(style);
        // multiply, don't replace: the brush colour carries its own alpha (rgba text, shadows)
        if brush_alpha != 1.0 {
            let a = self.cache.paint.alpha_f() * brush_alpha;
            self.set_paint_alpha(a);
        }

        let Some(mut font) = self.get_or_cache_font(font, normalized_coords, font_size, hint) else {
            return;
        };
        // synthetic bold: Skia's fake bold, the same one Chrome uses (SkFont::setEmbolden)
        if embolden.x > 0.0 || embolden.y > 0.0 {
            font.set_embolden(true);
        }
        if let Some(k) = font_skew {
            font.set_skew_x(k);
        }

        let (min_size, _) = glyphs.size_hint();
        self.cache.glyph_id_buf.reserve(min_size);
        self.cache.glyph_pos_buf.reserve(min_size);

        for glyph in glyphs {
            self.cache.glyph_id_buf.push(GlyphId::from(glyph.id as u16));
            self.cache.glyph_pos_buf.push(Point::new(glyph.x, glyph.y));
        }

        self.inner.draw_glyphs_at(
            &self.cache.glyph_id_buf[..],
            GlyphPositions::Points(&self.cache.glyph_pos_buf[..]),
            Point::new(0.0, 0.0),
            &font,
            &self.cache.paint,
        );

        self.cache.glyph_id_buf.clear();
        self.cache.glyph_pos_buf.clear();
    }

    fn draw_box_shadow(
        &mut self,
        transform: kurbo::Affine,
        rect: kurbo::Rect,
        brush: peniko::Color,
        radius: f64,
        std_dev: f64,
    ) {
        self.set_matrix(transform);

        self.reset_paint();
        self.set_paint_brush(brush, None);
        self.cache.paint.set_style(PaintStyle::Fill);

        if std_dev > 0.0 {
            self.cache.paint.set_mask_filter(
                MaskFilter::blur(BlurStyle::Normal, std_dev as f32, true).unwrap(),
            );
        }

        let rrect = RRect::new_nine_patch(
            Rect::new(
                rect.x0 as f32,
                rect.y0 as f32,
                rect.x1 as f32,
                rect.y1 as f32,
            ),
            radius as f32,
            radius as f32,
            radius as f32,
            radius as f32,
        );

        self.inner.draw_rrect(rrect, &self.cache.paint);
    }
}

fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Design-space value of each axis whose normalized coordinate (avar applied, as layout computed it)
/// is `normalized[i]`: a bisection over the axis range through skrifa's own normalization.
fn design_coords(data: &[u8], index: u32, normalized: &[anyrender::NormalizedCoord]) -> Option<Vec<Option<f32>>> {
    let font = skrifa::FontRef::from_index(data, index).ok()?;
    let axes = skrifa::MetadataProvider::axes(&font);
    let mut out = Vec::with_capacity(axes.len());
    for (i, axis) in axes.iter().enumerate() {
        let target = normalized.get(i).copied().unwrap_or(0);
        if target == 0 {
            out.push(None);
            continue;
        }
        let at = |v: f32| -> i16 {
            let loc = axes.location([(axis.tag(), v)]);
            loc.coords().get(i).map(|c| c.to_bits()).unwrap_or(0)
        };
        let (mut lo, mut hi) = if target < 0 { (axis.min_value(), axis.default_value()) } else { (axis.default_value(), axis.max_value()) };
        for _ in 0..40 {
            let mid = (lo + hi) / 2.0;
            if at(mid) < target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        out.push(Some(hi));
    }
    Some(out)
}

thread_local! {
    /// raea: set while a glyph run is drawn into its A8 mask (that draw goes straight to the
    /// mask surface)
    static IN_GLYPH_MASK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static COLOR_FONTS: std::cell::RefCell<hashbrown::HashMap<(u64, u32), bool>> = Default::default();
}

/// raea: whether the font has colour glyphs (sbix, CBDT or COLR: Apple Color Emoji, Noto Color
/// Emoji). They keep their own colours, so their runs can't go through the A8 mask, which draws every
/// glyph as a silhouette in the text colour.
fn is_color_font(font: &peniko::FontData) -> bool {
    COLOR_FONTS.with(|c| {
        *c.borrow_mut().entry((font.data.id(), font.index)).or_insert_with(|| {
            skrifa::FontRef::from_index(font.data.data(), font.index).is_ok_and(|f| {
                [b"sbix", b"CBDT", b"COLR"].iter().any(|t| f.table_data(skrifa::Tag::new(t)).is_some())
            })
        })
    })
}

thread_local! {
    /// The device scale of the layer whose filter is being converted (blur radii quantize in device px).
    static LAYER_SCALE: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
}

/// raea: the σ Skia should blur with to look like Chrome. Skia's raster blur for σ >= 2 is the
/// SVG spec's three box blurs (window d = floor(σ·3·√(2π)/4 + 0.5)), which Chrome also shows below 2:
/// σ under ~0.8 device px doesn't blur at all and each window d covers a band of σ. Below 2 this returns
/// the σ of that d's box cascade (variance (d² − 1)/4 for odd d; (3d² + 2d − 2)/12 for even d, two
/// boxes of d and one of d + 1), so Skia's small-σ Gaussian lands on Chrome's result.
fn chrome_sigma(sigma: f32) -> f32 {
    let scale = LAYER_SCALE.with(|s| s.get());
    let dev = f64::from(sigma * scale);
    if !(dev < 2.0) || dev <= 0.0 {
        return sigma;
    }
    let d = (dev * 3.0 * (2.0 * std::f64::consts::PI).sqrt() / 4.0 + 0.5).floor();
    let var = if d <= 1.0 {
        0.0
    } else if d % 2.0 == 1.0 {
        (d * d - 1.0) / 4.0
    } else {
        (3.0 * d * d + 2.0 * d - 2.0) / 12.0
    };
    var.sqrt() as f32 / scale
}

fn convert_filter(filter: &Filter) -> Option<ImageFilter> {
    // raea: an SVG filter's result converts back to sRGB when its last primitive ran in
    // linearRGB (Blink builds the last effect with kInterpolationSpaceSRGB)
    let out = filter.output();
    let linear = filter.nodes().get(out.0 as usize).is_some_and(|n| n.linear_rgb);
    to_space(convert_filter_effect(filter, out), linear, false)
}

/// raea: Blink's TransformInterpolationSpace: a colour filter between primitives whose
/// `color-interpolation-filters` differ (the source graphic is sRGB).
fn to_space(f: Option<ImageFilter>, from_linear: bool, to_linear: bool) -> Option<ImageFilter> {
    use skia_safe::{color_filters, image_filters};
    if from_linear == to_linear {
        return f;
    }
    let cf = if to_linear { color_filters::srgb_to_linear_gamma() } else { color_filters::linear_to_srgb_gamma() };
    image_filters::color_filter(cf, f, None)
}

/// raea: a colour (flood, shadow, lighting) in the primitive's operating space, as Blink's
/// AdaptColorToOperatingInterpolationSpace converts it.
fn space_color(c: skia_safe::Color4f, linear: bool) -> skia_safe::Color4f {
    // the sRGB transfer function (what SkColorFilters::SRGBToLinearGamma applies), alpha unchanged
    let lin = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    if linear { skia_safe::Color4f::new(lin(c.r), lin(c.g), lin(c.b), c.a) } else { c }
}

thread_local! {
    /// Converting a backdrop filter, with the layer's bounds (local coordinates): Chrome crops the backdrop
    /// to the element and blurs it with mirrored edges (Blink builds backdrop filter operations with
    /// SkTileMode::kMirror), where Skia's backdrop input would reach past the bounds.
    static BACKDROP: std::cell::Cell<Option<skia_safe::Rect>> = const { std::cell::Cell::new(None) };
}

fn convert_backdrop_filter(filter: &Filter, bounds: kurbo::Rect) -> Option<ImageFilter> {
    let rect = skia_safe::Rect::new(bounds.x0 as f32, bounds.y0 as f32, bounds.x1 as f32, bounds.y1 as f32);
    BACKDROP.with(|b| b.set(Some(rect)));
    let out = convert_filter(filter);
    BACKDROP.with(|b| b.set(None));
    out
}

fn resolve_input(filter: &Filter, input: &Option<FilterInput>, linear: bool) -> Option<ImageFilter> {
    // raea: inputs convert into the consuming primitive's colour space; the source is sRGB
    let (f, from_linear) = match input {
        Some(FilterInput::Result(input_id)) => (
            convert_filter_effect(filter, *input_id),
            filter.nodes()[input_id.0 as usize].linear_rgb,
        ),
        _ => (None, false),
    };
    to_space(f, from_linear, linear)
}

fn convert_filter_effect(filter: &Filter, id: FilterId) -> Option<ImageFilter> {
    let node = &filter.nodes()[id.0 as usize];
    let input = resolve_input(filter, &node.inputs.primary, node.linear_rgb);
    // Secondary input ("in2" in SVG), used by filters that combine two inputs such as
    // composite, blend and displacement map.
    let secondary = resolve_input(filter, &node.inputs.secondary, node.linear_rgb);

    // Clone to avoid borrow checking issues. Cloning is cheap as filters are refcounted.
    let input_clone = input.clone();

    use skia_safe::{color_filters, image_filters, shaders};

    let filter: Option<ImageFilter> = match &node.effect {
        FilterEffect::Flood(color) => {
            // raea: Blink floods with the colour in the operating space (float)
            let c = space_color(color4f_from_alpha_color(*color), node.linear_rgb);
            let shader = shaders::color_in_space(c, skia_safe::ColorSpace::new_srgb());
            image_filters::shader(shader, None)
        }
        FilterEffect::GaussianBlur(blur) => match BACKDROP.with(|b| b.get()) {
            Some(bounds) => {
                let input = match input {
                    Some(i) => Some(i),
                    None => image_filters::crop(bounds, skia_safe::TileMode::Decal, None),
                };
                image_filters::blur((chrome_sigma(blur.std_deviation), chrome_sigma(blur.std_deviation_y)), skia_safe::TileMode::Mirror, input, None)
            }
            None => image_filters::blur((chrome_sigma(blur.std_deviation), chrome_sigma(blur.std_deviation_y)), None, input, None),
        },
        FilterEffect::DropShadow(drop_shadow) => image_filters::drop_shadow(
            (drop_shadow.dx, drop_shadow.dy),
            (chrome_sigma(drop_shadow.std_deviation), chrome_sigma(drop_shadow.std_deviation_y)),
            space_color(color4f_from_alpha_color(drop_shadow.color), node.linear_rgb),
            None,
            input,
            None,
        ),
        FilterEffect::ComponentTransfer(ct) => {
            fn map_transfer_function(func: &TransferFunction) -> Option<[u8; 256]> {
                match func {
                    TransferFunction::Identity => None,
                    TransferFunction::Table(table) => {
                        if table.is_empty() {
                            None
                        } else {
                            let table = table.as_slice();
                            let n = table.len();

                            let mut values = [0; 256];
                            for (i, out) in values.iter_mut().enumerate() {
                                let c = i as f64 / 255.0;
                                let k = (c * (n - 1) as f64) as usize;
                                let v1 = table[k] as f64;
                                let v2 = table[(k + 1).min(n - 1)] as f64;
                                let val =
                                    255.0 * (v1 + (c * (n - 1) as f64 - k as f64) * (v2 - v1));
                                *out = val.clamp(0.0, 255.0) as u8;
                            }
                            Some(values)
                        }
                    }
                    TransferFunction::Discrete(items) => {
                        if items.is_empty() {
                            None
                        } else {
                            let items = items.as_slice();
                            let n = items.len();

                            let mut values = [0; 256];
                            for (i, out) in values.iter_mut().enumerate() {
                                let k = (((i * n) as f64 / 255.0) as usize).min(n - 1);
                                let val = 255.0 * items[k] as f64;
                                *out = val.clamp(0.0, 255.0) as u8;
                            }
                            Some(values)
                        }
                    }
                    TransferFunction::Linear(tf) => {
                        let mut values = [0; 256];
                        for (i, out) in values.iter_mut().enumerate() {
                            let val: f64 =
                                (tf.slope as f64 * i as f64) + (255.0 * tf.intercept as f64);
                            *out = val.clamp(0.0, 255.0) as u8;
                        }
                        Some(values)
                    }
                    TransferFunction::Gamma(tf) => {
                        let mut values = [0; 256];
                        for (i, out) in values.iter_mut().enumerate() {
                            let val: f64 = 255.0
                                * (tf.amplitude as f64
                                    * (i as f64 / 255.0).powf(tf.exponent as f64))
                                + tf.offset as f64;
                            *out = val.clamp(0.0, 255.0) as u8;
                        }
                        Some(values)
                    }
                }
            }

            let color_filter = color_filters::table_argb(
                map_transfer_function(&ct.alpha_function).as_ref(),
                map_transfer_function(&ct.red_function).as_ref(),
                map_transfer_function(&ct.green_function).as_ref(),
                map_transfer_function(&ct.blue_function).as_ref(),
            );

            color_filter.and_then(|cf| image_filters::color_filter(cf, input, None))
        }
        FilterEffect::ColorMatrix(color_matrix) => {
            let color_filter =
                color_filters::matrix_row_major(&color_matrix.0, color_filters::Clamp::Yes);
            image_filters::color_filter(color_filter, input, None)
        }
        FilterEffect::Offset(offset) => {
            image_filters::offset((offset.x as f32, offset.y as f32), input, None)
        }

        FilterEffect::Composite(composite_operator) => match composite_operator {
            // Arithmetic combination: result = k1*i1*i2 + k2*i1 + k3*i2 + k4, where i1 is the
            // primary input and i2 is the secondary input. Skia's `arithmetic` computes
            // k1*fg*bg + k2*fg + k3*bg + k4, so foreground = primary, background = secondary.
            CompositeOperator::Arithmetic(a) => {
                // raea: Blink validates premultiplied colours (arithmetic can exceed alpha)
                image_filters::arithmetic(a.k1, a.k2, a.k3, a.k4, true, secondary, input, None)
            }
            // Porter-Duff operators. The primary input is the source (foreground) and the
            // secondary input is the destination (background).
            _ => {
                let mode = match composite_operator {
                    CompositeOperator::Over => SkBlendMode::SrcOver,
                    CompositeOperator::In => SkBlendMode::SrcIn,
                    CompositeOperator::Out => SkBlendMode::SrcOut,
                    CompositeOperator::Atop => SkBlendMode::SrcATop,
                    CompositeOperator::Xor => SkBlendMode::Xor,
                    CompositeOperator::Arithmetic(_) => unreachable!(),
                };
                image_filters::blend(mode, secondary, input, None)
            }
        },
        FilterEffect::Blend(mix) => {
            // The primary input is the top layer (foreground) and the secondary input is the
            // bottom layer (background).
            let blend_mode = peniko::BlendMode {
                mix: *mix,
                compose: peniko::Compose::SrcOver,
            };
            let mode = sk_peniko::blend_mode_from(blend_mode);
            image_filters::blend(mode, secondary, input, None)
        }
        FilterEffect::Morphology(morphology_filter) => {
            let radius = (morphology_filter.radius, morphology_filter.radius_y);
            match morphology_filter.operator {
                MorphologyOperator::Dilate => image_filters::dilate(radius, input, None),
                MorphologyOperator::Erode => image_filters::erode(radius, input, None),
            }
        }
        FilterEffect::ConvolveMatrix(convolution_kernel) => {
            let size = convolution_kernel.size as i32;
            // Skia applies `gain` as a multiplier, so invert the divisor.
            let gain = if convolution_kernel.divisor != 0.0 {
                1.0 / convolution_kernel.divisor
            } else {
                1.0
            };
            // Center the kernel over each pixel (SVG default targetX/Y = floor(order / 2)).
            let offset = size / 2;
            image_filters::matrix_convolution(
                skia_safe::ISize::new(size, size),
                &convolution_kernel.values,
                gain,
                convolution_kernel.bias,
                skia_safe::IPoint::new(offset, offset),
                skia_safe::TileMode::Clamp,
                !convolution_kernel.preserve_alpha,
                input,
                None,
            )
        }
        FilterEffect::Turbulence(turbulence_filter) => {
            let base_frequency = (
                turbulence_filter.base_frequency,
                turbulence_filter.base_frequency_y,
            );
            let num_octaves = turbulence_filter.num_octaves as usize;
            let seed = turbulence_filter.seed as f32;
            // raea: stitched noise tiles over the primitive subregion's size (Blink's
            // FETurbulence: SkISize::Make(subregion width, height))
            let tile = node
                .subregion
                .filter(|_| turbulence_filter.stitch_tiles)
                .map(|r| skia_safe::ISize::new(r.width() as i32, r.height() as i32));
            let shader = match turbulence_filter.turbulence_type {
                TurbulenceType::FractalNoise => {
                    shaders::fractal_noise(base_frequency, num_octaves, seed, tile)
                }
                TurbulenceType::Turbulence => {
                    shaders::turbulence(base_frequency, num_octaves, seed, tile)
                }
            };
            shader.and_then(|shader| image_filters::shader(shader, None))
        }
        FilterEffect::DisplacementMap(displacement_map_filter) => {
            // The primary input provides the pixels to displace ("in"), the secondary input
            // provides the displacement vectors ("in2").
            image_filters::displacement_map(
                (
                    to_sk_color_channel(displacement_map_filter.x_channel),
                    to_sk_color_channel(displacement_map_filter.y_channel),
                ),
                displacement_map_filter.scale,
                secondary,
                input,
                None,
            )
        }
        FilterEffect::DiffuseLighting(diffuse_lighting_filter) => {
            // raea: lighting-color in the operating space, as an 8-bit colour (Blink)
            let light_color =
                space_color(color4f_from_alpha_color(diffuse_lighting_filter.lighting_color), node.linear_rgb).to_color();
            let surface_scale = diffuse_lighting_filter.surface_scale;
            let kd = diffuse_lighting_filter.diffuse_constant;
            match &diffuse_lighting_filter.light_source {
                LightSource::Distant(distant) => image_filters::distant_lit_diffuse(
                    distant_light_direction(distant),
                    light_color,
                    surface_scale,
                    kd,
                    input,
                    None,
                ),
                LightSource::Point(point) => image_filters::point_lit_diffuse(
                    Point3::new(point.x, point.y, point.z),
                    light_color,
                    surface_scale,
                    kd,
                    input,
                    None,
                ),
                LightSource::Spot(spot) => image_filters::spot_lit_diffuse(
                    Point3::new(spot.x, spot.y, spot.z),
                    Point3::new(spot.points_at_x, spot.points_at_y, spot.points_at_z),
                    spot.specular_exponent,
                    spot.limiting_cone_angle.unwrap_or(90.0),
                    light_color,
                    surface_scale,
                    kd,
                    input,
                    None,
                ),
            }
        }
        FilterEffect::SpecularLighting(specular_lighting_filter) => {
            let light_color =
                space_color(color4f_from_alpha_color(specular_lighting_filter.lighting_color), node.linear_rgb).to_color();
            let surface_scale = specular_lighting_filter.surface_scale;
            let ks = specular_lighting_filter.specular_constant;
            let shininess = specular_lighting_filter.specular_exponent;
            match &specular_lighting_filter.light_source {
                LightSource::Distant(distant) => image_filters::distant_lit_specular(
                    distant_light_direction(distant),
                    light_color,
                    surface_scale,
                    ks,
                    shininess,
                    input,
                    None,
                ),
                LightSource::Point(point) => image_filters::point_lit_specular(
                    Point3::new(point.x, point.y, point.z),
                    light_color,
                    surface_scale,
                    ks,
                    shininess,
                    input,
                    None,
                ),
                LightSource::Spot(spot) => image_filters::spot_lit_specular(
                    Point3::new(spot.x, spot.y, spot.z),
                    Point3::new(spot.points_at_x, spot.points_at_y, spot.points_at_z),
                    spot.specular_exponent,
                    spot.limiting_cone_angle.unwrap_or(90.0),
                    light_color,
                    surface_scale,
                    ks,
                    shininess,
                    input,
                    None,
                ),
            }
        }

        // raea: an feImage of an image file (Blink's FEImage draws it into its rect with
        // high-quality sampling: Mitchell cubic); images by id from a resource registry aren't supported
        FilterEffect::Image(source) => match (&source.image, source.rect) {
            (Some(img), Some(rect)) => {
                let alpha = match img.alpha_type {
                    peniko::ImageAlphaType::Alpha => skia_safe::AlphaType::Unpremul,
                    peniko::ImageAlphaType::AlphaPremultiplied => skia_safe::AlphaType::Premul,
                };
                let info = skia_safe::ImageInfo::new(
                    (img.width as i32, img.height as i32),
                    skia_safe::ColorType::RGBA8888,
                    alpha,
                    None,
                );
                skia_safe::images::raster_from_data(&info, skia_safe::Data::new_copy(img.data.data()), img.width as usize * 4)
                    .and_then(|image| {
                        let sampling = skia_safe::SamplingOptions::from(skia_safe::CubicResampler::mitchell());
                        image_filters::image(image, None, Some(&sk_rect(rect)), sampling)
                    })
            }
            _ => None,
        },
        // raea: Blink's FETile tiles the input's subregion (the filter region for a source)
        // over this primitive's subregion
        FilterEffect::Tile => {
            let src = match &node.inputs.primary {
                Some(FilterInput::Result(i)) => filter.nodes()[i.0 as usize].subregion,
                _ => filter.region(),
            };
            match (src, node.subregion) {
                (Some(src), Some(dst)) => image_filters::tile(sk_rect(src), sk_rect(dst), input),
                _ => None,
            }
        }
    };

    // If we do not implement given type of filter then the match statement above returns None
    // In that case, if we have an input then we pass that along instead. In the case of filter chains
    // with some unimplemented filters that allows the remaining filters to work
    let out = filter.or(input_clone);
    // raea: an SVG primitive's result is cropped to its subregion (Blink's GetCropRect; a Skia
    // filter's crop rect is the same crop filter around its output)
    match node.subregion {
        Some(r) => image_filters::crop(sk_rect(r), None, out),
        None => out,
    }
}

fn sk_rect(r: kurbo::Rect) -> skia_safe::Rect {
    skia_safe::Rect::new(r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32)
}

/// Convert an anyrender displacement-map [`ColorChannel`] to the corresponding Skia channel.
fn to_sk_color_channel(channel: ColorChannel) -> skia_safe::ColorChannel {
    match channel {
        ColorChannel::Red => skia_safe::ColorChannel::R,
        ColorChannel::Green => skia_safe::ColorChannel::G,
        ColorChannel::Blue => skia_safe::ColorChannel::B,
        ColorChannel::Alpha => skia_safe::ColorChannel::A,
    }
}

/// Compute the direction vector towards a distant light source from its azimuth and elevation.
///
/// Angles are interpreted as degrees, following the SVG `feDistantLight` convention.
fn distant_light_direction(light: &DistantLightSource) -> Point3 {
    let azimuth = light.azimuth.to_radians();
    let elevation = light.elevation.to_radians();
    Point3::new(
        azimuth.cos() * elevation.cos(),
        azimuth.sin() * elevation.cos(),
        elevation.sin(),
    )
}

mod sk_peniko {
    use peniko::color::{AlphaColor, ColorSpaceTag, HueDirection, Srgb};
    use peniko::{
        BlendMode, Compose, Extend, Gradient, GradientKind, ImageAlphaType, ImageBrush, ImageData,
        ImageFormat, Mix,
    };
    use peniko::{Fill, color::DynamicColor};
    use skia_safe::AlphaType as SkAlphaType;
    use skia_safe::BlendMode as SkBlendMode;
    use skia_safe::Color4f as SkColor4f;
    use skia_safe::ColorType as SkColorType;
    use skia_safe::Data as SkData;
    use skia_safe::ImageInfo as SkImageInfo;
    use skia_safe::PathFillType as SkPathFillType;
    use skia_safe::SamplingOptions as SkSamplingOptions;
    use skia_safe::Shader as SkShader;
    use skia_safe::TileMode as SkTileMode;
    use skia_safe::gradient::Colors;
    use skia_safe::gradient_shader::interpolation::ColorSpace as SkGradientShaderColorSpace;
    use skia_safe::gradient_shader::interpolation::HueMethod as SkGradientShaderHueMethod;
    use skia_safe::shaders::{
        linear_gradient, radial_gradient, sweep_gradient, two_point_conical_gradient,
    };

    pub(super) fn shader_from_image_brush(
        image_brush: ImageBrush<&ImageData>,
        brush_transform: Option<kurbo::Affine>,
    ) -> Option<SkShader> {
        let image_data = image_brush.image;

        let image_info = SkImageInfo::new(
            (image_data.width as i32, image_data.height as i32),
            match image_data.format {
                ImageFormat::Rgba8 => SkColorType::RGBA8888,
                ImageFormat::Bgra8 => SkColorType::BGRA8888,
                _ => unreachable!(),
            },
            match image_data.alpha_type {
                ImageAlphaType::Alpha => SkAlphaType::Unpremul,
                ImageAlphaType::AlphaPremultiplied => SkAlphaType::Premul,
            },
            None,
        );
        let pixels = unsafe {
            SkData::new_bytes(image_data.data.data()) // We have to ensure the src image data lives long enough
        };
        let image =
            skia_safe::images::raster_from_data(&image_info, pixels, image_info.min_row_bytes())
                .unwrap();

        let sampling = match image_brush.sampler.quality {
            peniko::ImageQuality::Low => {
                SkSamplingOptions::new(skia_safe::FilterMode::Nearest, skia_safe::MipmapMode::None)
            }
            peniko::ImageQuality::Medium => SkSamplingOptions::new(
                skia_safe::FilterMode::Linear,
                skia_safe::MipmapMode::Nearest,
            ),
            peniko::ImageQuality::High => {
                SkSamplingOptions::new(skia_safe::FilterMode::Linear, skia_safe::MipmapMode::Linear)
            }
        };

        skia_safe::shaders::image(
            image,
            (
                tile_mode_from_extend(image_brush.sampler.x_extend),
                tile_mode_from_extend(image_brush.sampler.y_extend),
            ),
            &sampling,
            &brush_transform.map(super::sk_kurbo::matrix_from_affine),
        )
    }

    pub(super) fn shader_from_gradient(
        gradient: &Gradient,
        brush_transform: Option<kurbo::Affine>,
    ) -> SkShader {
        // raea: a gradient Skia can't build (zero-length line, zero radius) paints its last
        // stop's colour, as CSS paints a zero-length gradient line, rather than panicking
        fn degenerate(gradient: &Gradient) -> SkShader {
            let c = gradient.stops.last().map(|s| color4f_from_dynamic_color(s.color)).unwrap_or(SkColor4f::new(0.0, 0.0, 0.0, 0.0));
            skia_safe::shaders::color(c.to_color())
        }
        fn rad_to_deg(rad: f32) -> f32 {
            if rad == 0.0 {
                return 0.0;
            }

            rad * 180.0 / std::f32::consts::PI
        }

        match gradient.kind {
            GradientKind::Linear(linear_gradient_position) => {
                let mut colors: Vec<SkColor4f> = vec![];
                let mut positions: Vec<f32> = vec![];

                for color_stop in gradient.stops.iter() {
                    colors.push(color4f_from_dynamic_color(color_stop.color));
                    positions.push(color_stop.offset);
                }
                let start = super::sk_kurbo::pt_from(linear_gradient_position.start);
                let end = super::sk_kurbo::pt_from(linear_gradient_position.end);

                let interpolation = skia_safe::gradient_shader::Interpolation {
                    color_space: gradient_shader_cs_from_cs_tag(gradient.interpolation_cs),
                    in_premul: skia_safe::gradient_shader::interpolation::InPremul::Yes,
                    hue_method: gradient_shader_hue_method_from_hue_direction(
                        gradient.hue_direction,
                    ),
                };

                linear_gradient(
                    (start, end),
                    &skia_safe::gradient::Gradient::new(
                        Colors::new(
                            &colors[..],
                            Some(&positions[..]),
                            tile_mode_from_extend(gradient.extend),
                            None,
                        ),
                        interpolation,
                    ),
                    &brush_transform.map(super::sk_kurbo::matrix_from_affine),
                )
                .unwrap_or_else(|| degenerate(gradient))
            }
            GradientKind::Radial(radial_gradient_position) => {
                let mut colors: Vec<SkColor4f> = vec![];
                let mut positions: Vec<f32> = vec![];

                for color_stop in gradient.stops.iter() {
                    colors.push(color4f_from_dynamic_color(color_stop.color));
                    positions.push(color_stop.offset);
                }

                let start_center = super::sk_kurbo::pt_from(radial_gradient_position.start_center);
                let start_radius = radial_gradient_position.start_radius;
                let end_center = super::sk_kurbo::pt_from(radial_gradient_position.end_center);
                let end_radius = radial_gradient_position.end_radius;

                let interpolation = skia_safe::gradient_shader::Interpolation {
                    color_space: gradient_shader_cs_from_cs_tag(gradient.interpolation_cs),
                    in_premul: skia_safe::gradient_shader::interpolation::InPremul::Yes,
                    hue_method: gradient_shader_hue_method_from_hue_direction(
                        gradient.hue_direction,
                    ),
                };

                if start_center == end_center && start_radius == end_radius {
                    radial_gradient(
                        (start_center, start_radius),
                        &skia_safe::gradient::Gradient::new(
                            Colors::new(
                                &colors[..],
                                Some(&positions[..]),
                                tile_mode_from_extend(gradient.extend),
                                None,
                            ),
                            interpolation,
                        ),
                        &brush_transform.map(super::sk_kurbo::matrix_from_affine),
                    )
                    .unwrap_or_else(|| degenerate(gradient))
                } else {
                    two_point_conical_gradient(
                        (start_center, start_radius),
                        (end_center, end_radius),
                        &skia_safe::gradient::Gradient::new(
                            Colors::new(
                                &colors[..],
                                Some(&positions[..]),
                                tile_mode_from_extend(gradient.extend),
                                None,
                            ),
                            interpolation,
                        ),
                        &brush_transform.map(super::sk_kurbo::matrix_from_affine),
                    )
                    .unwrap_or_else(|| degenerate(gradient))
                }
            }
            GradientKind::Sweep(sweep_gradient_position) => {
                let mut colors: Vec<SkColor4f> = vec![];
                let mut positions: Vec<f32> = vec![];

                for color_stop in gradient.stops.iter() {
                    colors.push(color4f_from_dynamic_color(color_stop.color));
                    positions.push(color_stop.offset);
                }
                let center = super::sk_kurbo::pt_from(sweep_gradient_position.center);

                let interpolation = skia_safe::gradient_shader::Interpolation {
                    color_space: gradient_shader_cs_from_cs_tag(gradient.interpolation_cs),
                    in_premul: skia_safe::gradient_shader::interpolation::InPremul::Yes,
                    hue_method: gradient_shader_hue_method_from_hue_direction(
                        gradient.hue_direction,
                    ),
                };

                sweep_gradient(
                    center,
                    (
                        rad_to_deg(sweep_gradient_position.start_angle),
                        rad_to_deg(sweep_gradient_position.end_angle),
                    ),
                    &skia_safe::gradient::Gradient::new(
                        Colors::new(
                            &colors[..],
                            Some(&positions[..]),
                            tile_mode_from_extend(gradient.extend),
                            None,
                        ),
                        interpolation,
                    ),
                    &brush_transform.map(super::sk_kurbo::matrix_from_affine),
                )
                .unwrap_or_else(|| degenerate(gradient))
            }
        }
    }

    pub(super) fn path_fill_type_from_fill(fill: Fill) -> SkPathFillType {
        match fill {
            Fill::NonZero => SkPathFillType::Winding,
            Fill::EvenOdd => SkPathFillType::EvenOdd,
        }
    }

    pub(super) fn color4f_from_alpha_color(color: AlphaColor<Srgb>) -> SkColor4f {
        SkColor4f::new(
            color.components[0],
            color.components[1],
            color.components[2],
            color.components[3],
        )
    }

    pub(super) fn color4f_from_dynamic_color(color: DynamicColor) -> SkColor4f {
        let color = color.to_alpha_color::<Srgb>();
        SkColor4f::new(
            color.components[0],
            color.components[1],
            color.components[2],
            color.components[3],
        )
    }

    pub(super) fn gradient_shader_cs_from_cs_tag(
        color_space: ColorSpaceTag,
    ) -> SkGradientShaderColorSpace {
        match color_space {
            ColorSpaceTag::Srgb => SkGradientShaderColorSpace::SRGB,
            ColorSpaceTag::LinearSrgb => SkGradientShaderColorSpace::SRGBLinear,
            ColorSpaceTag::Lab => SkGradientShaderColorSpace::Lab,
            ColorSpaceTag::Lch => SkGradientShaderColorSpace::LCH,
            ColorSpaceTag::Hsl => SkGradientShaderColorSpace::HSL,
            ColorSpaceTag::Hwb => SkGradientShaderColorSpace::HWB,
            ColorSpaceTag::Oklab => SkGradientShaderColorSpace::OKLab,
            ColorSpaceTag::Oklch => SkGradientShaderColorSpace::OKLCH,
            ColorSpaceTag::DisplayP3 => SkGradientShaderColorSpace::DisplayP3,
            ColorSpaceTag::A98Rgb => SkGradientShaderColorSpace::A98RGB,
            ColorSpaceTag::ProphotoRgb => SkGradientShaderColorSpace::ProphotoRGB,
            ColorSpaceTag::Rec2020 => SkGradientShaderColorSpace::Rec2020,
            _ => SkGradientShaderColorSpace::SRGB, // ToDo: overview unsupported color space tags and possibly document it, for now just fallback
        }
    }

    pub(super) fn gradient_shader_hue_method_from_hue_direction(
        direction: HueDirection,
    ) -> SkGradientShaderHueMethod {
        match direction {
            HueDirection::Shorter => SkGradientShaderHueMethod::Shorter,
            HueDirection::Longer => SkGradientShaderHueMethod::Longer,
            HueDirection::Increasing => SkGradientShaderHueMethod::Increasing,
            HueDirection::Decreasing => SkGradientShaderHueMethod::Decreasing,
            _ => unreachable!(),
        }
    }

    pub(super) fn tile_mode_from_extend(extend: Extend) -> SkTileMode {
        match extend {
            Extend::Pad => SkTileMode::Clamp,
            Extend::Repeat => SkTileMode::Repeat,
            Extend::Reflect => SkTileMode::Mirror,
        }
    }

    pub(super) fn blend_mode_from(blend_mode: BlendMode) -> SkBlendMode {
        if blend_mode.mix == Mix::Normal {
            match blend_mode.compose {
                Compose::Clear => SkBlendMode::Clear,
                Compose::Copy => SkBlendMode::Src,
                Compose::Dest => SkBlendMode::Dst,
                Compose::SrcOver => SkBlendMode::SrcOver,
                Compose::DestOver => SkBlendMode::DstOver,
                Compose::SrcIn => SkBlendMode::SrcIn,
                Compose::DestIn => SkBlendMode::DstIn,
                Compose::SrcOut => SkBlendMode::SrcOut,
                Compose::DestOut => SkBlendMode::DstOut,
                Compose::SrcAtop => SkBlendMode::SrcATop,
                Compose::DestAtop => SkBlendMode::DstATop,
                Compose::Xor => SkBlendMode::Xor,
                Compose::Plus => SkBlendMode::Plus,
                Compose::PlusLighter => SkBlendMode::Plus,
            }
        } else {
            match blend_mode.mix {
                Mix::Normal => unreachable!(), // Handled above
                Mix::Multiply => SkBlendMode::Multiply,
                Mix::Screen => SkBlendMode::Screen,
                Mix::Overlay => SkBlendMode::Overlay,
                Mix::Darken => SkBlendMode::Darken,
                Mix::Lighten => SkBlendMode::Lighten,
                Mix::ColorDodge => SkBlendMode::ColorDodge,
                Mix::ColorBurn => SkBlendMode::ColorBurn,
                Mix::HardLight => SkBlendMode::HardLight,
                Mix::SoftLight => SkBlendMode::SoftLight,
                Mix::Difference => SkBlendMode::Difference,
                Mix::Exclusion => SkBlendMode::Exclusion,
                Mix::Hue => SkBlendMode::Hue,
                Mix::Saturation => SkBlendMode::Saturation,
                Mix::Color => SkBlendMode::Color,
                Mix::Luminosity => SkBlendMode::Luminosity,
            }
        }
    }
}

mod sk_kurbo {
    use kurbo::{Affine, PathEl, Point};
    use kurbo::{Rect, RoundedRect, Shape};
    use skia_safe::M44 as SkM44;
    use skia_safe::Matrix as SkMatrix;
    use skia_safe::Path as SkPath;
    use skia_safe::PathBuilder as SkPathBuilder;
    use skia_safe::Point as SkPoint;
    use skia_safe::RRect as SkRRect;
    use skia_safe::Rect as SkRect;

    pub(super) fn rect_from(rect: Rect) -> SkRect {
        SkRect::new(
            rect.x0 as f32,
            rect.y0 as f32,
            rect.x1 as f32,
            rect.y1 as f32,
        )
    }

    pub(super) fn rrect_from(rrect: RoundedRect) -> SkRRect {
        let rect = rect_from(rrect.rect());
        let radii = rrect.radii();
        // Corner order: upper-left, upper-right, lower-right, lower-left
        SkRRect::new_rect_radii(
            rect,
            &[
                SkPoint::new(radii.top_left as f32, radii.top_left as f32),
                SkPoint::new(radii.top_right as f32, radii.top_right as f32),
                SkPoint::new(radii.bottom_right as f32, radii.bottom_right as f32),
                SkPoint::new(radii.bottom_left as f32, radii.bottom_left as f32),
            ],
        )
    }

    pub(super) fn m44_from_affine(affine: Affine) -> SkM44 {
        let m = affine.as_coeffs();
        let scale_x = m[0] as f32;
        let shear_y = m[1] as f32;
        let shear_x = m[2] as f32;
        let scale_y = m[3] as f32;
        let translate_x = m[4] as f32;
        let translate_y = m[5] as f32;

        SkM44::col_major(&[
            scale_x,
            shear_y,
            0.0,
            0.0,
            shear_x,
            scale_y,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            translate_x,
            translate_y,
            0.0,
            1.0,
        ])
    }

    pub(super) fn matrix_from_affine(affine: Affine) -> SkMatrix {
        let m = affine.as_coeffs();
        let scale_x = m[0] as f32;
        let shear_y = m[1] as f32;
        let shear_x = m[2] as f32;
        let scale_y = m[3] as f32;
        let translate_x = m[4] as f32;
        let translate_y = m[5] as f32;

        SkMatrix::new_all(
            scale_x,
            shear_x,
            translate_x,
            shear_y,
            scale_y,
            translate_y,
            0.0,
            0.0,
            1.0,
        )
    }

    pub(super) fn pt_from(p: Point) -> SkPoint {
        SkPoint::new(p.x as f32, p.y as f32)
    }

    pub(super) fn path_from_shape(shape: &impl Shape) -> SkPath {
        let mut sk_path = SkPathBuilder::new();

        let mut current = kurbo::Point::ZERO;
        let mut start = kurbo::Point::ZERO;
        if let Some(path_els) = shape.as_path_slice() {
            for path_el in path_els {
                append_path_el_to_sk_path(path_el, &mut sk_path, &mut current, &mut start);
            }
        } else {
            for path_el in shape.path_elements(0.1) {
                append_path_el_to_sk_path(&path_el, &mut sk_path, &mut current, &mut start);
            }
        }

        sk_path.detach()
    }

    fn append_path_el_to_sk_path(
        path_el: &PathEl,
        sk_path: &mut SkPathBuilder,
        current: &mut kurbo::Point,
        start: &mut kurbo::Point,
    ) {
        match path_el {
            PathEl::MoveTo(p) => {
                *start = *p;
                _ = sk_path.move_to(pt_from(*p))
            }
            PathEl::LineTo(p) => _ = sk_path.line_to(pt_from(*p)),
            PathEl::QuadTo(p1, p2) => _ = sk_path.quad_to(pt_from(*p1), pt_from(*p2)),
            PathEl::CurveTo(p1, p2, p3) => match arc_as_conic(*current, *p1, *p2, *p3, ARCS_AS_CONICS.with(|a| a.get())) {
                Some((ctrl, w)) => _ = sk_path.conic_to(pt_from(ctrl), pt_from(*p3), w),
                None => _ = sk_path.cubic_to(pt_from(*p1), pt_from(*p2), pt_from(*p3)),
            },
            PathEl::ClosePath => {
                *current = *start;
                _ = sk_path.close();
                return;
            }
        };
        if let Some(p) = path_el.end_point() {
            *current = p;
        }
    }

    std::thread_local! {
        /// raea: set while CSS box geometry is drawn (`SkiaSceneCache::path_mask`), whose arcs
        /// (border radii, elliptical too) Chrome holds as conics. Otherwise (SVG) only exactly circular
        /// arcs built the standard way convert (usvg's <circle>/<rect rx>, which Chrome draws as
        /// ovals/rrects); authored cubics such as Lottie's 0.5519-arm ellipses stay cubics, as in Chrome.
        pub(crate) static ARCS_AS_CONICS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// raea: a cubic that is an elliptical arc (kurbo's `Arc`: arms of 4/3·tan(θ/4) along the
    /// end tangents, as CSS border radii and SVG ellipses are built) becomes the conic Skia itself would
    /// hold for it (SkPath::addRRect / addOval: control point at the tangents' intersection, weight
    /// cos(θ/2)), so Skia flattens it the way it flattens Chrome's rounded rects and circles.
    fn arc_as_conic(p0: kurbo::Point, p1: kurbo::Point, p2: kurbo::Point, p3: kurbo::Point, any_ellipse: bool) -> Option<(kurbo::Point, f32)> {
        let (d0, d1, delta) = (p1 - p0, p2 - p3, p3 - p0);
        let det = d0.cross(d1);
        if det.abs() < 1e-9 || d0.hypot2() < 1e-12 || d1.hypot2() < 1e-12 {
            return None;
        }
        // tangent intersection: p0 + s·d0 = p3 + t·d1
        let s = delta.cross(d1) / det;
        let t = delta.cross(d0) / det;
        if s <= 1.0 || t <= 1.0 {
            return None;
        }
        let (r0, r1) = (1.0 / s, 1.0 / t);
        if (r0 - r1).abs() > 1e-4 {
            return None;
        }
        let ratio = 0.5 * (r0 + r1);
        // ratio(θ) = (4/3)·tan(θ/4) / tan(θ/2) falls from 2/3 (θ -> 0) to 0 (θ -> π)
        let f = |th: f64| (4.0 / 3.0) * (th / 4.0).tan() / (th / 2.0).tan();
        if !(ratio > 0.05 && ratio < f(1e-3)) {
            return None;
        }
        let (mut lo, mut hi) = (1e-3_f64, std::f64::consts::PI - 1e-3);
        for _ in 0..48 {
            let mid = 0.5 * (lo + hi);
            if f(mid) > ratio { lo = mid } else { hi = mid }
        }
        let theta = 0.5 * (lo + hi);
        let ctrl = p0 + d0 * s;
        if !any_ellipse {
            // circular only: equal tangent lengths, and the arms match the sweep the tangents imply
            let (a0, a1) = ((ctrl - p0).hypot(), (ctrl - p3).hypot());
            if (a0 - a1).abs() > 1e-3 * a0.max(a1) {
                return None;
            }
            let (u, v) = (p0 - ctrl, p3 - ctrl);
            let sweep = std::f64::consts::PI - (u.dot(v) / (a0 * a1)).clamp(-1.0, 1.0).acos();
            if (f(sweep) - ratio).abs() > 2e-5 {
                return None;
            }
        }
        Some((ctrl, (theta / 2.0).cos() as f32))
    }
}
