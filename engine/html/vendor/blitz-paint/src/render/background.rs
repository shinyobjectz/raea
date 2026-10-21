use super::{ElementCx, to_image_quality, to_peniko_image};
use crate::color::{Color, ToColorColor};
use crate::gradient::to_peniko_gradient;
use anyrender::PaintScene;
use blitz_dom::NodeId;
use blitz_dom::node::{ImageData, ImageResourceData, SpecialElementData};
use kurbo::{self, Affine, BezPath, Point, Rect, Shape, Size, Vec2};
use peniko::{self, Fill};
use style::{
    computed_values::border_collapse::T as BorderCollapse,
    properties::{
        generated::longhands::{
            background_attachment::single_value::computed_value::T as StyloBackgroundAttachment,
            background_clip::single_value::computed_value::T as StyloBackgroundClip,
            background_origin::single_value::computed_value::T as StyloBackgroundOrigin,
            mask_origin::single_value::computed_value::T as StyloMaskOrigin,
        },
        style_structs::{Background, SVG},
    },
    values::{
        computed::{
            BackgroundRepeat, Gradient as StyloGradient, Image as ComputedImage, LengthPercentage,
            background::BackgroundSize,
        },
        generics::image::GenericImage,
        specified::background::BackgroundRepeatKeyword,
    },
};

#[cfg(feature = "tracing")]
use tracing::warn;

/// A box from the CSS box model. Abstracts over the (structurally identical)
/// computed value types of the `background-clip`/`background-origin` and
/// `mask-clip`/`mask-origin` properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)] // The variants are named after the CSS keywords
pub(super) enum BoxModelBox {
    BorderBox,
    PaddingBox,
    ContentBox,
}

// Also covers MaskClip as the type is the same
impl From<StyloBackgroundClip> for BoxModelBox {
    fn from(value: StyloBackgroundClip) -> Self {
        match value {
            StyloBackgroundClip::BorderBox => Self::BorderBox,
            StyloBackgroundClip::PaddingBox => Self::PaddingBox,
            StyloBackgroundClip::ContentBox => Self::ContentBox,

            // TODO: support BorderArea
            StyloBackgroundClip::BorderArea => Self::BorderBox,
        }
    }
}

impl From<StyloBackgroundOrigin> for BoxModelBox {
    fn from(value: StyloBackgroundOrigin) -> Self {
        match value {
            StyloBackgroundOrigin::BorderBox => Self::BorderBox,
            StyloBackgroundOrigin::PaddingBox => Self::PaddingBox,
            StyloBackgroundOrigin::ContentBox => Self::ContentBox,
        }
    }
}

impl From<StyloMaskOrigin> for BoxModelBox {
    fn from(value: StyloMaskOrigin) -> Self {
        match value {
            StyloMaskOrigin::BorderBox => Self::BorderBox,
            StyloMaskOrigin::PaddingBox => Self::PaddingBox,
            StyloMaskOrigin::ContentBox => Self::ContentBox,
        }
    }
}

/// The styles and image data for a single layer of a CSS image layer list
/// (`background-image` or `mask-image`). The `background-*` and `mask-*`
/// properties share computed value types, which allows the layer painting code
/// to be shared.
pub(super) struct ImageLayerStyles<'a> {
    /// The computed value of the `background-image`/`mask-image` layer
    pub stylo_image: &'a ComputedImage,
    /// The loaded image resource if `stylo_image` is a `url()` image
    pub image_data: Option<&'a ImageResourceData>,
    pub position_x: &'a LengthPercentage,
    pub position_y: &'a LengthPercentage,
    pub repeat: &'a BackgroundRepeat,
    pub size: &'a BackgroundSize,
    pub clip: BoxModelBox,
    pub origin: BoxModelBox,
    pub attachment: StyloBackgroundAttachment,
}

impl<'a> ImageLayerStyles<'a> {
    pub(super) fn from_background(
        bg_styles: &'a Background,
        image_data: &'a [Option<ImageResourceData>],
        idx: usize,
    ) -> Self {
        Self {
            stylo_image: &bg_styles.background_image.0[idx],
            image_data: image_data.get(idx).and_then(Option::as_ref),
            position_x: get_cyclic(&bg_styles.background_position_x.0, idx),
            position_y: get_cyclic(&bg_styles.background_position_y.0, idx),
            repeat: get_cyclic(&bg_styles.background_repeat.0, idx),
            size: get_cyclic(&bg_styles.background_size.0, idx),
            clip: (*get_cyclic(&bg_styles.background_clip.0, idx)).into(),
            origin: (*get_cyclic(&bg_styles.background_origin.0, idx)).into(),
            attachment: *get_cyclic(&bg_styles.background_attachment.0, idx),
        }
    }

    pub(super) fn from_svg(
        svg_styles: &'a SVG,
        image_data: &'a [Option<ImageResourceData>],
        idx: usize,
    ) -> Self {
        Self {
            stylo_image: &svg_styles.mask_image.0[idx],
            image_data: image_data.get(idx).and_then(Option::as_ref),
            position_x: get_cyclic(&svg_styles.mask_position_x.0, idx),
            position_y: get_cyclic(&svg_styles.mask_position_y.0, idx),
            repeat: get_cyclic(&svg_styles.mask_repeat.0, idx),
            size: get_cyclic(&svg_styles.mask_size.0, idx),
            clip: (*get_cyclic(&svg_styles.mask_clip.0, idx)).into(),
            origin: (*get_cyclic(&svg_styles.mask_origin.0, idx)).into(),
            // There is no `mask-attachment` property
            attachment: StyloBackgroundAttachment::Scroll,
        }
    }
}

impl ElementCx<'_, '_> {
    pub(super) fn draw_background(&self, scene: &mut impl PaintScene) {
        let bg_styles = &self.style.get_background();
        let image_data = &self.element.background_images;
        let layer_count = bg_styles.background_image.0.len();

        // The background color is clipped by the clip of the last layer in the list
        let background_clip: BoxModelBox =
            (*get_cyclic(&bg_styles.background_clip.0, layer_count - 1)).into();

        // Draw background color (if any)
        self.draw_solid_bg(scene, background_clip);

        for idx in (0..layer_count).rev() {
            let layer = ImageLayerStyles::from_background(bg_styles, image_data, idx);
            if layer_paints_nothing(&layer) {
                continue;
            }
            let background_clip_path = self.background_box_path(layer.clip);

            self.context.layer_manager.maybe_with_layer(
                scene,
                true,
                1.0,
                self.transform,
                &background_clip_path,
                None,
                None,
                |scene| {
                    self.draw_image_layer(scene, &layer);
                },
            );
        }
    }

    /// background-clip:text (local change). Stylo's servo build drops `background-clip: text`, so
    /// raea's compile step mirrors it into the non-inherited marker `--uv-bgclip`.
    pub(super) fn background_clips_to_text(&self) -> bool {
        // (raea) the atom kept alive: made and dropped here, a dynamic atom was freed and interned again per element
        // per frame (ZeroHeap)
        static UV_BGCLIP: std::sync::LazyLock<style::Atom> = std::sync::LazyLock::new(|| style::Atom::from("uv-bgclip"));
        self.style
            .custom_properties()
            .get_for_cssom(&UV_BGCLIP)
            .is_some()
    }

    /// background-clip:text (local change): the background composited (DestIn) through the glyphs
    /// of the element and its descendants. The isolation layer's opacity is just below 1 so
    /// renderers can't fold it into a non-isolated clip (see `ALMOST_OPAQUE` in mask.rs).
    pub(super) fn draw_background_clipped_to_text(
        &self,
        scene: &mut impl PaintScene,
        content_position: Point,
        clip_rect: Rect,
    ) {
        let area = self.frame.border_box_path();
        scene.push_layer(Fill::NonZero, peniko::Mix::Normal, 1.0 - f32::EPSILON, self.transform, &area, None, None);
        self.draw_background(scene);
        scene.push_layer(
            Fill::NonZero,
            peniko::BlendMode::new(peniko::Mix::Normal, peniko::Compose::DestIn),
            1.0,
            self.transform,
            &area,
            None,
            None,
        );
        let was_masking = self.context.text_mask.replace(true);
        self.draw_inline_layout(scene, content_position);
        self.draw_children(scene, self.transform, clip_rect);
        self.context.text_mask.set(was_masking);
        scene.pop_layer();
        scene.pop_layer();
    }

    /// The path of the given CSS box model box for this element
    /// raea: Blink's kBackgroundBleedShrinkBackground. Under a rounded border that is
    /// opaque on every side, a border-box background is inset by half the border widths (radii
    /// shrunk to match) so its antialiased edge can't show through the border's
    /// (BoxDecorationData::ComputeBleedAvoidance, BackgroundContouredRectAdjustedForBleedAvoidance).
    fn background_box_path(&self, css_box: BoxModelBox) -> BezPath {
        let mut path = BezPath::new();
        self.background_box_path_into(css_box, &mut path);
        path
    }

    /// (raea) `background_box_path` written into `out`
    fn background_box_path_into(&self, css_box: BoxModelBox, out: &mut BezPath) {
        if matches!(css_box, BoxModelBox::BorderBox)
            && self.frame.has_border_radius()
            && self.border_obscures_background_edge()
        {
            let w = self.frame.border_width;
            let fraction = if self.has_double_border() { 1.0 / 6.0 } else { 0.5 };
            let inset = kurbo::Insets::new(w.x0 * fraction, w.y0 * fraction, w.x1 * fraction, w.y1 * fraction);
            let r = &self.frame.border_radii;
            let shrink = |v: Vec2, dx: f64, dy: f64| Vec2::new((v.x - dx).max(0.0), (v.y - dy).max(0.0));
            let radii = crate::kurbo_css::NonUniformRoundedRectRadii {
                top_left: shrink(r.top_left, inset.x0, inset.y0),
                top_right: shrink(r.top_right, inset.x1, inset.y0),
                bottom_right: shrink(r.bottom_right, inset.x1, inset.y1),
                bottom_left: shrink(r.bottom_left, inset.x0, inset.y1),
            };
            let shrunk = crate::kurbo_css::CssBox::new(
                self.frame.border_box - inset,
                kurbo::Insets::ZERO,
                kurbo::Insets::ZERO,
                0.0,
                radii,
            );
            return shrunk.box_path_into(crate::kurbo_css::CssBoxKind::BorderBox, out);
        }
        let kind = match css_box {
            BoxModelBox::BorderBox => crate::kurbo_css::CssBoxKind::BorderBox,
            BoxModelBox::PaddingBox => crate::kurbo_css::CssBoxKind::PaddingBox,
            BoxModelBox::ContentBox => crate::kurbo_css::CssBoxKind::ContentBox,
        };
        self.frame.box_path_into(kind, out)
    }

    /// Blink's BorderEdge::ObscuresBackgroundEdge on all four sides: present (width and a visible
    /// style), opaque, and neither dotted nor dashed.
    fn border_obscures_background_edge(&self) -> bool {
        use style::values::computed::BorderStyle;
        let b = self.style.get_border();
        let current = self.style.clone_color();
        let side = |w: &style::values::computed::BorderSideWidth,
                    st: BorderStyle,
                    c: &style::values::computed::Color| {
            w.0 .0 > 0
                && !matches!(
                    st,
                    BorderStyle::None | BorderStyle::Hidden | BorderStyle::Dotted | BorderStyle::Dashed
                )
                && c.resolve_to_absolute(&current).alpha >= 1.0
        };
        side(&b.border_top_width, b.border_top_style, &b.border_top_color)
            && side(&b.border_right_width, b.border_right_style, &b.border_right_color)
            && side(&b.border_bottom_width, b.border_bottom_style, &b.border_bottom_color)
            && side(&b.border_left_width, b.border_left_style, &b.border_left_color)
    }

    fn has_double_border(&self) -> bool {
        use style::values::computed::BorderStyle;
        let b = self.style.get_border();
        [b.border_top_style, b.border_right_style, b.border_bottom_style, b.border_left_style]
            .contains(&BorderStyle::Double)
    }

    pub(super) fn box_path(&self, css_box: BoxModelBox) -> BezPath {
        match css_box {
            BoxModelBox::BorderBox => self.frame.border_box_path(),
            BoxModelBox::PaddingBox => self.frame.padding_box_path(),
            BoxModelBox::ContentBox => self.frame.content_box_path(),
        }
    }

    /// The rect of the given CSS box model box for this element
    fn box_rect(&self, css_box: BoxModelBox) -> Rect {
        match css_box {
            BoxModelBox::BorderBox => self.frame.border_box,
            BoxModelBox::PaddingBox => self.frame.padding_box,
            BoxModelBox::ContentBox => self.frame.content_box,
        }
    }

    /// Draw a single layer of a CSS image layer list (`background-image` or `mask-image`)
    pub(super) fn draw_image_layer(&self, scene: &mut impl PaintScene, layer: &ImageLayerStyles) {
        match layer.stylo_image {
            GenericImage::None => {
                // Do nothing
            }
            GenericImage::Gradient(gradient) => self.draw_gradient_layer(scene, gradient, layer),
            GenericImage::Url(_) => {
                self.draw_raster_image_layer(scene, layer);
                #[cfg(feature = "svg")]
                self.draw_svg_image_layer(scene, layer);
            }
            GenericImage::LightDark(_) => {
                #[cfg(feature = "tracing")]
                warn!("Implement image layer drawing for ImageLightDark")
            }
            GenericImage::PaintWorklet(_) => {
                #[cfg(feature = "tracing")]
                warn!("Implement image layer drawing for Image::PaintWorklet")
            }
            GenericImage::CrossFade(_) => {
                #[cfg(feature = "tracing")]
                warn!("Implement image layer drawing for Image::CrossFade")
            }
            GenericImage::Image(_) => {
                #[cfg(feature = "tracing")]
                warn!("Implement image layer drawing for Image::Image")
            }
            GenericImage::ImageSet(_) => {
                #[cfg(feature = "tracing")]
                warn!("Implement image layer drawing for Image::ImageSet")
            }
        }
    }

    /// Paint the backgrounds of the table's column groups, columns, row groups and rows
    /// (in that order, bottom to top: CSS 2.1 §17.5.1).
    ///
    /// These backgrounds are only painted behind the table's cells (not in the
    /// `border-spacing` between them, nor where a row has no cell), so they are
    /// painted cell by cell.
    pub(super) fn draw_table_row_backgrounds(&self, scene: &mut impl PaintScene) {
        let SpecialElementData::TableRoot(table) = &self.element.special_data else {
            return;
        };

        let background_color = |node_id: NodeId| -> Option<Color> {
            let node = self.context.dom.get_node(node_id)?;
            let style = node.primary_styles()?;
            let current_color = style.clone_color();
            let color = style
                .get_background()
                .background_color
                .resolve_to_absolute(&current_color)
                .as_srgb_color();
            (color != Color::TRANSPARENT).then_some(color)
        };

        // Resolve the (group, track) background colors once per track rather than once per cell
        let column_colors: Vec<[Option<Color>; 2]> = table
            .columns
            .iter()
            .map(|col| {
                [
                    col.group.and_then(background_color),
                    background_color(col.node_id),
                ]
            })
            .collect();
        let row_colors: Vec<[Option<Color>; 2]> = table
            .rows
            .iter()
            .map(|row| {
                [
                    row.group.and_then(background_color),
                    background_color(row.node_id),
                ]
            })
            .collect();
        let has_color =
            |colors: &[[Option<Color>; 2]]| colors.iter().flatten().any(Option::is_some);
        if !has_color(&column_colors) && !has_color(&row_colors) {
            return;
        }

        let cell_rect = |cell_id: NodeId| -> Option<Rect> {
            let layout = self.context.dom.get_node(cell_id)?.final_layout();
            Some(Rect::new(
                layout.location.x as f64,
                layout.location.y as f64,
                (layout.location.x + layout.size.width) as f64,
                (layout.location.y + layout.size.height) as f64,
            ))
        };

        // In the collapsed borders model the borders are laid out as gutters between the
        // cells, but a cell's border box (and so its background) extends to the middle of
        // the borders around it. The gutter is split on a whole pixel so that the
        // backgrounds of adjacent cells tile exactly.
        let collapsed = table.border_collapse == BorderCollapse::Collapse;
        let (gutter, bounds) = if collapsed {
            let gap = table.style.gap.map(|gap| gap.into_raw().value() as f64);
            let bounds = table
                .cells
                .iter()
                .filter_map(|cell| cell_rect(cell.node_id))
                .reduce(|a, b| a.union(b));
            (Size::new(gap.width, gap.height), bounds)
        } else {
            (Size::ZERO, None)
        };
        let before = Size::new((gutter.width / 2.0).ceil(), (gutter.height / 2.0).ceil());
        let after = gutter - before;

        for cell in &table.cells {
            let no_colors = [None, None];
            let column = column_colors
                .get(cell.column as usize)
                .unwrap_or(&no_colors);
            let row = row_colors.get(cell.row as usize).unwrap_or(&no_colors);
            let mut colors = column.iter().chain(row).flatten().peekable();
            if colors.peek().is_none() {
                continue;
            }
            let Some(mut rect) = cell_rect(cell.node_id) else {
                continue;
            };
            if let Some(bounds) = bounds {
                rect = Rect::new(
                    rect.x0 - before.width,
                    rect.y0 - before.height,
                    rect.x1 + after.width,
                    rect.y1 + after.height,
                )
                .intersect(bounds);
            }

            let shape = rect.scale_from_origin(self.scale);
            for color in colors {
                scene.fill(Fill::NonZero, self.transform, *color, None, &shape);
            }
        }
    }

    /// (raea) its clip's path made only when there's a colour to fill (ZeroHeap: one was made for every element a frame)
    fn draw_solid_bg(&self, scene: &mut impl PaintScene, clip: BoxModelBox) {
        let current_color = self.style.clone_color();
        let background_color = &self.style.get_background().background_color;
        let bg_color = background_color
            .resolve_to_absolute(&current_color)
            .as_srgb_color();

        if bg_color != Color::TRANSPARENT {
            // Fill the color
            let bg_color = if self.fold_alpha < 1.0 { bg_color.multiply_alpha(self.fold_alpha) } else { bg_color };
            // on the kept path: a fill reads its shape only during the call
            crate::kurbo_css::with_scratch(|shape| {
                self.background_box_path_into(clip, shape);
                scene.fill(Fill::NonZero, self.transform, bg_color, None, &*shape);
            });
        }
    }

    /// Whether the layer is positioned against the viewport
    /// (`background-attachment: fixed`) rather than the element's origin box.
    /// `fixed` behaves as `scroll` on elements affected by a CSS transform
    /// (the transformed element acts as the layer's containing block).
    fn layer_is_fixed(&self, layer: &ImageLayerStyles) -> bool {
        layer.attachment == StyloBackgroundAttachment::Fixed && !self.is_transformed()
    }

    /// The background positioning area and the transform from its coordinate
    /// space to the scene for a fixed layer: the viewport, unaffected by any
    /// scrolling.
    fn fixed_positioning_area(&self) -> (Rect, Affine) {
        let viewport_rect = Rect::new(
            0.0,
            0.0,
            self.context.width as f64,
            self.context.height as f64,
        );
        let transform = Affine::translate((self.context.initial_x, self.context.initial_y));
        (viewport_rect, transform)
    }

    /// Whether this element or any of its ancestors has a CSS transform
    ///
    /// TODO: this misses transformed elements whose resolved transform is the
    /// identity (e.g. `transform: translate(0)`), and elements with
    /// `will-change: transform`, both of which should also degrade `fixed`
    /// to `scroll` (see WPT css/css-transforms/transform-fixed-bg-005/008)
    fn is_transformed(&self) -> bool {
        let mut current = Some(self.node.id);
        while let Some(node) = current.and_then(|id| self.context.dom.get_node(id)) {
            if node.transform().is_some() {
                return true;
            }
            current = node.parent;
        }
        false
    }

    #[cfg(feature = "svg")]
    fn draw_svg_image_layer(&self, scene: &mut impl PaintScene, layer: &ImageLayerStyles) {
        use kurbo::Affine;

        let Some(bg_image) = layer.image_data else {
            return;
        };
        let ImageData::Svg(svg) = &bg_image.image else {
            return;
        };

        // A zero-sized `viewBox` disables rendering of the SVG
        if svg.intrinsic_dimensions.degenerate_view_box {
            return;
        }

        let (origin_rect, base_transform) = if self.layer_is_fixed(layer) {
            self.fixed_positioning_area()
        } else {
            (self.box_rect(layer.origin), self.transform)
        };

        let frame_w = (origin_rect.width() / self.scale) as f32;
        let frame_h = (origin_rect.height() / self.scale) as f32;

        let svg_size = svg.tree.size();

        // Size the SVG per the CSS default sizing algorithm
        // (https://drafts.csswg.org/css-images/#default-sizing). An SVG image
        // may lack an intrinsic width, height, and/or aspect ratio, so each is
        // passed separately (usvg's resolved `Tree::size` is only used as the
        // source coordinate space of the rendered tree).
        let intrinsic_width = svg.intrinsic_width().filter(|w| w.is_finite() && *w > 0.0);
        let intrinsic_height = svg.intrinsic_height().filter(|h| h.is_finite() && *h > 0.0);
        let aspect_ratio = match (intrinsic_width, intrinsic_height) {
            (Some(w), Some(h)) => Some(w / h),
            _ => svg.viewbox_aspect_ratio(),
        }
        .filter(|r| r.is_finite() && *r > 0.0);

        let bg_size = compute_layer_size(
            layer,
            frame_w,
            frame_h,
            BackgroundSizeComputeMode::Intrinsic {
                width: intrinsic_width,
                height: intrinsic_height,
                ratio: aspect_ratio,
            },
        );

        if bg_size.width <= 0.0 || bg_size.height <= 0.0 {
            return;
        }

        let x_ratio = (bg_size.width / svg_size.width() as f64) * self.scale;
        let y_ratio = (bg_size.height / svg_size.height() as f64) * self.scale;

        let bg_pos = compute_layer_position(
            layer,
            frame_w - bg_size.width as f32,
            frame_h - bg_size.height as f32,
        );

        let transform = base_transform
            * kurbo::Affine::translate((
                origin_rect.x0 + bg_pos.x * self.scale,
                origin_rect.y0 + bg_pos.y * self.scale,
            ))
            * Affine::scale_non_uniform(x_ratio, y_ratio);

        // raea: Chrome draws an SVG background as an image of the tile's size, repeated per
        // background-repeat, with any filters inside it applied. Rasterize one tile at device size with
        // the SVG rasterizer (Skia under the vello backend), else the frame's own offscreen rasterizer
        // (the Skia backend draws inline SVG directly, and drew a repeating background's first tile only),
        // and tile it like a raster image.
        let rasterizer = super::three_d::SVG_RASTERIZER
            .with(|r| r.get())
            .or_else(|| super::three_d::RASTERIZER.with(|r| r.get()));
        if let Some(rasterize) = rasterizer {
            let tw = (bg_size.width * self.scale).round().max(1.0) as u32;
            let th = (bg_size.height * self.scale).round().max(1.0) as u32;
            if tw <= 4096 && th <= 4096 {
                let key = (std::sync::Arc::as_ptr(&svg.tree) as usize, tw, th);
                let cached = SVG_TILES.with(|c| c.borrow().get(&key).cloned());
                let data = cached.unwrap_or_else(|| {
                    let mut tile = anyrender::Scene::new();
                    let to_tile = Affine::scale_non_uniform(
                        tw as f64 / svg_size.width() as f64,
                        th as f64 / svg_size.height() as f64,
                    );
                    anyrender_svg::render_svg_tree(&mut tile, &svg.tree, to_tile);
                    let blob = peniko::Blob::new(std::sync::Arc::new(rasterize(tile, tw, th)));
                    SVG_TILES.with(|c| {
                        let mut c = c.borrow_mut();
                        if c.len() > 64 {
                            c.clear();
                        }
                        c.insert(key, blob.clone());
                    });
                    blob
                });
                let (iw, ih) = (tw as f64, th as f64);
                let (px, py) = (bg_pos.x as f64 * self.scale, bg_pos.y as f64 * self.scale);
                let (sw, sh) = (bg_size.width * self.scale, bg_size.height * self.scale);
                let (rx, ry) = (sw / iw, sh / ih);
                let BackgroundRepeat(repeat_x, repeat_y) = layer.repeat;
                let x = raster_axis_tiling(*repeat_x, origin_rect.x0, origin_rect.width(), px, sw, iw, rx);
                let y = raster_axis_tiling(*repeat_y, origin_rect.y0, origin_rect.height(), py, sh, ih, ry);
                let transform = base_transform
                    .pre_scale_non_uniform(rx, ry)
                    .then_translate(Vec2 { x: x.translate, y: y.translate });
                let tile_rect = Rect::new(0.0, 0.0, x.rect_len, y.rect_len);
                let brush = peniko::ImageBrush {
                    image: peniko::ImageData {
                        data,
                        format: peniko::ImageFormat::Rgba8,
                        width: tw,
                        height: th,
                        alpha_type: peniko::ImageAlphaType::AlphaPremultiplied,
                    },
                    sampler: peniko::ImageSampler {
                        x_extend: peniko::Extend::Repeat,
                        y_extend: peniko::Extend::Repeat,
                        quality: to_image_quality(self.style.clone_image_rendering()),
                        alpha: self.fold_alpha,
                    },
                };
                for hc in 0..y.count {
                    for wc in 0..x.count {
                        let transform = transform.then_translate(Vec2 { x: wc as f64 * x.stride, y: hc as f64 * y.stride });
                        scene.fill(Fill::NonZero, transform, brush.as_ref(), None, &tile_rect);
                    }
                }
                return;
            }
        }
        anyrender_svg::render_svg_tree(scene, &svg.tree, transform);
    }

    fn draw_raster_image_layer(&self, scene: &mut impl PaintScene, layer: &ImageLayerStyles) {
        let Some(bg_image) = layer.image_data else {
            return;
        };
        let ImageData::Raster(image_data) = &bg_image.image else {
            return;
        };

        let image_rendering = self.style.clone_image_rendering();
        let quality = to_image_quality(image_rendering);

        let (origin_rect, base_transform) = if self.layer_is_fixed(layer) {
            self.fixed_positioning_area()
        } else {
            (self.box_rect(layer.origin), self.transform)
        };

        let image_width = image_data.width as f64;
        let image_height = image_data.height as f64;

        let (bg_pos, bg_size) = compute_layer_position_and_size(
            layer,
            origin_rect.width() / self.scale,
            origin_rect.height() / self.scale,
            BackgroundSizeComputeMode::Size(image_width as f32, image_height as f32),
        );

        let bg_pos = (bg_pos.to_vec2() * self.scale).to_point();
        let bg_size = bg_size * self.scale;

        let x_ratio = bg_size.width / image_width;
        let y_ratio = bg_size.height / image_height;

        let BackgroundRepeat(repeat_x, repeat_y) = layer.repeat;

        let x = raster_axis_tiling(
            *repeat_x,
            origin_rect.x0,
            origin_rect.width(),
            bg_pos.x,
            bg_size.width,
            image_width,
            x_ratio,
        );
        let y = raster_axis_tiling(
            *repeat_y,
            origin_rect.y0,
            origin_rect.height(),
            bg_pos.y,
            bg_size.height,
            image_height,
            y_ratio,
        );

        let transform = base_transform
            .pre_scale_non_uniform(x_ratio, y_ratio)
            .then_translate(Vec2 {
                x: x.translate,
                y: y.translate,
            });
        let tile_rect = Rect::new(0.0, 0.0, x.rect_len, y.rect_len);

        for hc in 0..y.count {
            for wc in 0..x.count {
                let transform = transform.then_translate(Vec2 {
                    x: wc as f64 * x.stride,
                    y: hc as f64 * y.stride,
                });

                let mut brush = to_peniko_image(image_data, quality);
                brush.sampler.alpha = self.fold_alpha;
                scene.fill(peniko::Fill::NonZero, transform, brush.as_ref(), None, &tile_rect);
            }
        }
    }

    fn draw_gradient_layer(
        &self,
        scene: &mut impl PaintScene,
        gradient: &StyloGradient,
        layer: &ImageLayerStyles,
    ) {
        // For a fixed layer the positioning area (the viewport) already covers
        // everything visible, so it also serves as the clip rect (no extension
        // towards the clip box is needed).
        let (origin_rect, base_transform, clip_rect) = if self.layer_is_fixed(layer) {
            let (viewport_rect, transform) = self.fixed_positioning_area();
            (viewport_rect, transform, viewport_rect)
        } else {
            (
                self.box_rect(layer.origin),
                self.transform,
                self.box_rect(layer.clip),
            )
        };

        let (bg_pos, bg_size) = compute_layer_position_and_size(
            layer,
            origin_rect.width() / self.scale,
            origin_rect.height() / self.scale,
            BackgroundSizeComputeMode::Auto,
        );

        let bg_pos = (bg_pos.to_vec2() * self.scale).to_point();
        let bg_size = bg_size * self.scale;

        let BackgroundRepeat(repeat_x, repeat_y) = layer.repeat;

        let x = gradient_axis_tiling(
            *repeat_x,
            origin_rect.x0,
            origin_rect.width(),
            clip_rect.x0,
            clip_rect.width(),
            bg_pos.x,
            bg_size.width,
        );
        let y = gradient_axis_tiling(
            *repeat_y,
            origin_rect.y0,
            origin_rect.height(),
            clip_rect.y0,
            clip_rect.height(),
            bg_pos.y,
            bg_size.height,
        );

        // FIXME: https://wpt.live/css/css-backgrounds/background-size/background-size-near-zero-gradient.html
        // (local change: upstream skipped anything over 500 tiles, which drops ordinary dot grids
        // such as `radial-gradient(...) 0 0 / 20px 20px` over a 1080p frame; only degenerate tiles
        // are skipped here)
        if x.stride.abs() < 1.0 && x.count > 1
            || y.stride.abs() < 1.0 && y.count > 1
            || x.count as u64 * y.count as u64 > 50_000
        {
            return;
        }

        let tile_rect = Rect::new(0.0, 0.0, x.rect_len, y.rect_len);
        let bounding_box = self.frame.border_box.bounding_box();
        let current_color = self.style.clone_color();

        let (gradient, gradient_transform) = to_peniko_gradient(
            gradient,
            tile_rect,
            bounding_box,
            self.scale,
            &current_color,
        );
        let brush = anyrender::Paint::Gradient(&gradient);

        let transform = base_transform.then_translate(Vec2 {
            x: x.translate,
            y: y.translate,
        });

        for hc in 0..y.count {
            for wc in 0..x.count {
                let transform = transform.then_translate(Vec2 {
                    x: wc as f64 * x.stride,
                    y: hc as f64 * y.stride,
                });

                scene.fill(
                    peniko::Fill::NonZero,
                    transform,
                    brush.clone(),
                    gradient_transform,
                    &tile_rect,
                );
            }
        }
    }
}

fn compute_layer_position_and_size(
    layer: &ImageLayerStyles,
    container_w: f64,
    container_h: f64,
    size_mode: BackgroundSizeComputeMode,
) -> (Point, Size) {
    use BackgroundRepeatKeyword::*;

    let bg_size = compute_layer_size(layer, container_w as f32, container_h as f32, size_mode);

    let bg_pos = compute_layer_position(
        layer,
        (container_w - bg_size.width) as f32,
        (container_h - bg_size.height) as f32,
    );

    let BackgroundRepeat(repeat_x, repeat_y) = layer.repeat;

    let bg_size = if matches!(repeat_x, Round) && matches!(repeat_y, Round) {
        let count = (container_w / bg_size.width).round();
        let width = container_w / count;

        let count = (container_h / bg_size.height).round();
        let height = container_h / count;

        Size::new(width, height)
    } else if matches!(repeat_x, Round) {
        let count = (container_w / bg_size.width).round();
        let width = container_w / count;
        Size::new(width, bg_size.height)
    } else if matches!(repeat_y, Round) {
        let count = (container_h / bg_size.height).round();
        let height = container_h / count;
        Size::new(bg_size.width, height)
    } else {
        bg_size
    };

    (bg_pos, bg_size)
}

#[inline]
fn compute_layer_position(layer: &ImageLayerStyles, width: f32, height: f32) -> Point {
    use style::values::computed::Length;

    let bg_pos_x = layer.position_x.resolve(Length::new(width)).px() as f64;
    let bg_pos_y = layer.position_y.resolve(Length::new(height)).px() as f64;

    Point::new(bg_pos_x, bg_pos_y)
}

fn compute_layer_size(
    layer: &ImageLayerStyles,
    container_w: f32,
    container_h: f32,
    mode: BackgroundSizeComputeMode,
) -> kurbo::Size {
    use style::values::computed::Length;
    use style::values::generics::length::GenericLengthPercentageOrAuto as Lpa;

    let (width, height): (f32, f32) = match layer.size {
        BackgroundSize::ExplicitSize { width, height } => {
            let width = width.map(|w| w.0.resolve(Length::new(container_w)));
            let height = height.map(|h| h.0.resolve(Length::new(container_h)));

            match (width, height) {
                (Lpa::LengthPercentage(width), Lpa::LengthPercentage(height)) => {
                    let width = width.px();
                    let height = height.px();
                    match mode {
                        BackgroundSizeComputeMode::Auto => (width, height),
                        BackgroundSizeComputeMode::Size(_, _) => (width, height),
                        BackgroundSizeComputeMode::Intrinsic { .. } => (width, height),
                    }
                }
                (Lpa::LengthPercentage(width), Lpa::Auto) => {
                    let width = width.px();
                    let height = match mode {
                        BackgroundSizeComputeMode::Auto => container_h,
                        BackgroundSizeComputeMode::Size(bg_w, bg_h) => bg_h / bg_w * width,
                        BackgroundSizeComputeMode::Intrinsic { height, ratio, .. } => ratio
                            .map(|ratio| width / ratio)
                            .or(height)
                            .unwrap_or(container_h),
                    };
                    (width, height)
                }
                (Lpa::Auto, Lpa::LengthPercentage(height)) => {
                    let height = height.px();
                    let width = match mode {
                        BackgroundSizeComputeMode::Auto => container_w,
                        BackgroundSizeComputeMode::Size(bg_w, bg_h) => bg_w / bg_h * height,
                        BackgroundSizeComputeMode::Intrinsic { width, ratio, .. } => ratio
                            .map(|ratio| height * ratio)
                            .or(width)
                            .unwrap_or(container_w),
                    };
                    (width, height)
                }
                (Lpa::Auto, Lpa::Auto) => match mode {
                    BackgroundSizeComputeMode::Auto => (container_w, container_h),
                    BackgroundSizeComputeMode::Size(bg_w, bg_h) => (bg_w, bg_h),
                    BackgroundSizeComputeMode::Intrinsic {
                        width,
                        height,
                        ratio,
                    } => default_sizing(width, height, ratio, container_w, container_h),
                },
            }
        }
        BackgroundSize::Cover => match mode {
            BackgroundSizeComputeMode::Auto => (container_w, container_h),
            BackgroundSizeComputeMode::Size(bg_w, bg_h) => {
                // Scale to the smallest size that covers both axes
                let ratio = (container_w / bg_w).max(container_h / bg_h);
                (bg_w * ratio, bg_h * ratio)
            }
            BackgroundSizeComputeMode::Intrinsic { ratio, .. } => match ratio {
                // Scale the aspect ratio to the smallest size that covers both axes
                Some(ratio) => {
                    let scale = (container_w / ratio).max(container_h);
                    (scale * ratio, scale)
                }
                // No intrinsic aspect ratio: fill the positioning area
                None => (container_w, container_h),
            },
        },
        BackgroundSize::Contain => match mode {
            BackgroundSizeComputeMode::Auto => (container_w, container_h),
            BackgroundSizeComputeMode::Size(bg_w, bg_h) => {
                // Scale to the largest size contained by both axes
                let ratio = (container_w / bg_w).min(container_h / bg_h);
                (bg_w * ratio, bg_h * ratio)
            }
            BackgroundSizeComputeMode::Intrinsic { ratio, .. } => match ratio {
                // Scale the aspect ratio to the largest size contained by both axes
                Some(ratio) => {
                    let scale = (container_w / ratio).min(container_h);
                    (scale * ratio, scale)
                }
                // No intrinsic aspect ratio: fill the positioning area
                None => (container_w, container_h),
            },
        },
    };

    kurbo::Size {
        width: width as f64,
        height: height as f64,
    }
}

enum BackgroundSizeComputeMode {
    Auto,
    Size(f32, f32),
    /// Intrinsic dimensions of an image which may lack an intrinsic width,
    /// height, and/or aspect ratio (e.g. SVG), sized per the CSS default
    /// sizing algorithm (https://drafts.csswg.org/css-images/#default-sizing)
    Intrinsic {
        width: Option<f32>,
        height: Option<f32>,
        ratio: Option<f32>,
    },
}

/// The CSS default sizing algorithm for the unconstrained (`auto auto`) case:
/// resolve the concrete object size from whichever intrinsic dimensions exist,
/// falling back to the default object size (the background positioning area).
fn default_sizing(
    width: Option<f32>,
    height: Option<f32>,
    ratio: Option<f32>,
    container_w: f32,
    container_h: f32,
) -> (f32, f32) {
    match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, ratio.map(|r| w / r).unwrap_or(container_h)),
        (None, Some(h)) => (ratio.map(|r| h * r).unwrap_or(container_w), h),
        (None, None) => match ratio {
            // Intrinsic aspect ratio only: size as if `contain` were specified
            Some(ratio) => {
                let scale = (container_w / ratio).min(container_h);
                (scale * ratio, scale)
            }
            None => (container_w, container_h),
        },
    }
}

/// The placement and tiling of a background layer along one axis: a
/// translation applied to the layer as a whole, the length of each filled
/// rect, and the number of explicit tiles with the stride between them.
struct AxisTiling {
    /// Translation (in device pixels) positioning the first tile
    translate: f64,
    /// Length of each filled rect, in the coordinate space of the fill's transform
    rect_len: f64,
    /// Number of explicitly drawn tiles
    count: u32,
    /// Stride (in device pixels) between the starts of consecutive tiles
    stride: f64,
}

/// Per-axis placement and tiling for a raster image layer. `Repeat`/`Round`
/// produce a single fill covering the whole positioning area (relying on the
/// image brush repeating), while `Space` produces `count` explicit tiles
/// spaced `stride` apart.
///
/// The fill rect is in image pixel coordinates (the drawing transform is
/// pre-scaled by `ratio`), while translations are in device pixels.
thread_local! {
    /// raea: rasterized SVG background tiles by (tree, device tile size)
    static SVG_TILES: std::cell::RefCell<std::collections::HashMap<(usize, u32, u32), peniko::Blob<u8>>> =
        Default::default();
}

fn raster_axis_tiling(
    repeat: BackgroundRepeatKeyword,
    origin_start: f64,
    origin_len: f64,
    bg_pos: f64,
    tile_len: f64,
    image_len: f64,
    ratio: f64,
) -> AxisTiling {
    use BackgroundRepeatKeyword::*;

    match repeat {
        Repeat | Round => {
            let extend_len = extend(bg_pos, tile_len);
            AxisTiling {
                translate: origin_start - extend_len,
                rect_len: (origin_len + extend_len) / ratio,
                count: 1,
                stride: 0.0,
            }
        }
        Space => {
            let (count, stride) = compute_space_count_and_stride(origin_len, tile_len);
            AxisTiling {
                translate: origin_start + if count == 1 { bg_pos } else { 0.0 },
                rect_len: image_len,
                count,
                stride,
            }
        }
        NoRepeat => AxisTiling {
            translate: origin_start + bg_pos,
            rect_len: image_len,
            count: 1,
            stride: 0.0,
        },
    }
}

/// Per-axis placement and tiling for a gradient layer. Unlike raster images,
/// gradients cannot rely on brush repetition, so `Repeat`/`Round` also produce
/// explicit tiles. When the clip box extends beyond the origin box, tiling
/// starts from the clip box edge so the pattern covers the whole clipped area.
fn gradient_axis_tiling(
    repeat: BackgroundRepeatKeyword,
    origin_start: f64,
    origin_len: f64,
    clip_start: f64,
    clip_len: f64,
    bg_pos: f64,
    tile_len: f64,
) -> AxisTiling {
    use BackgroundRepeatKeyword::*;

    match repeat {
        Repeat | Round => {
            // The clip and origin boxes are nested, so the clip box extends
            // beyond the origin box iff it does so at either end
            let clip_is_outer =
                clip_start < origin_start || clip_start + clip_len > origin_start + origin_len;
            let (area_start, area_len) = if clip_is_outer {
                (clip_start, clip_len)
            } else {
                (origin_start, origin_len)
            };
            let extend_len = extend((origin_start - area_start) + bg_pos, tile_len);
            // raea: tolerate float noise (a transformed box can measure 540.0000000000001 px,
            // which asked for a second tile whose first rows showed along the edge)
            let count = ((area_len + extend_len) / tile_len - 1e-6).ceil().max(1.0) as u32;
            AxisTiling {
                translate: area_start - extend_len,
                rect_len: tile_len,
                count,
                stride: tile_len,
            }
        }
        Space => {
            let (count, stride) = compute_space_count_and_stride(origin_len, tile_len);
            AxisTiling {
                translate: origin_start + if count == 1 { bg_pos } else { 0.0 },
                rect_len: tile_len,
                count,
                stride,
            }
        }
        NoRepeat => AxisTiling {
            translate: origin_start + bg_pos,
            rect_len: tile_len,
            count: 1,
            stride: 0.0,
        },
    }
}

fn compute_space_count_and_stride(bg_size: f64, size: f64) -> (u32, f64) {
    let modulo = bg_size % size;
    let count = (((bg_size - modulo) / size) as u32).max(1);
    let stride = if count > 1 {
        modulo / (count - 1) as f64
    } else {
        0.0
    } + size;

    (count, stride)
}

/// Whether drawing the layer is guaranteed to paint nothing, making its clip
/// layer unnecessary
fn layer_paints_nothing(layer: &ImageLayerStyles) -> bool {
    match layer.stylo_image {
        GenericImage::None => true,
        GenericImage::Url(_) => layer.image_data.is_none(),
        _ => false,
    }
}

#[inline]
pub(super) fn get_cyclic<T>(values: &[T], layer_index: usize) -> &T {
    &values[layer_index % values.len()]
}

fn extend(offset: f64, length: f64) -> f64 {
    let extend_length = offset % length;
    // raea: float noise around a whole tile is no offset
    if extend_length.abs() < 1e-6 || (length - extend_length.abs()).abs() < 1e-6 {
        return 0.0;
    }
    if extend_length > 0.0 {
        length - extend_length
    } else {
        -extend_length
    }
}
