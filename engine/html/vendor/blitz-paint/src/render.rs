mod background;
mod border;
mod box_shadow;
mod clip_path;
mod form_controls;
mod mask;
pub(crate) mod three_d;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use super::kurbo_css::CssBox;
use crate::color::{Color, ToColorColor};
use crate::debug_overlay::render_debug_overlay;
use crate::filters::convert_filters;
use crate::kurbo_css::NonUniformRoundedRectRadii;
use crate::layers::LayerManager;
use crate::sizing::compute_object_fit;
use crate::text::DrawTextContext;
use crate::{CustomWidgetSceneMap, SELECTION_COLOR};
use anyrender::{PaintScene, Scene};
use blitz_dom::node::{
    ListItemLayout, ListItemLayoutPosition, Marker, NodeData, RasterImageData, TextInputData,
    TextNodeData,
};
use blitz_dom::{BaseDocument, ElementData, Node, NodeId, local_name};
use blitz_traits::devtools::DevtoolSettings;

use style::values::computed::{BorderCornerRadius, ColorOrAuto};
use style::{
    dom::TElement,
    properties::{
        ComputedValues, generated::longhands::visibility::computed_value::T as StyloVisibility,
        style_structs::Font,
    },
    values::{
        computed::{CSSPixelLength, Contain, Overflow},
        specified::box_::{DisplayInside, DisplayOutside},
        specified::image::ImageRendering,
    },
};

use kurbo::{self, Affine, Insets, Point, Rect, Shape, Size, Stroke, Vec2};
use peniko::{self, Fill, ImageData, ImageSampler};
use style::values::generics::color::GenericColor;
use taffy::Layout;

/// A view of the track positions reported by taffy in physical (left-to-right /
/// top-to-bottom) order. Taffy reports tracks in logical order, which for RTL grid
/// containers is physically reversed.
#[derive(Clone, Copy)]
pub(crate) struct PhysicalTracks<'a> {
    positions: &'a [taffy::Line<f32>],
    reversed: bool,
}

impl<'a> PhysicalTracks<'a> {
    pub(crate) fn from_tracks<S: taffy::CheapCloneStr>(
        tracks: &'a taffy::DetailedGridTracksInfo<S>,
    ) -> Self {
        let positions = tracks.positions.as_slice();
        let reversed = positions
            .first()
            .zip(positions.last())
            .is_some_and(|(first, last)| first.start > last.start);
        Self {
            positions,
            reversed,
        }
    }

    fn get(self, index: usize) -> taffy::Line<f32> {
        if self.reversed {
            self.positions[self.positions.len() - 1 - index]
        } else {
            self.positions[index]
        }
    }

    /// Iterate track positions in physical order
    pub(crate) fn iter(self) -> impl ExactSizeIterator<Item = taffy::Line<f32>> + 'a {
        (0..self.positions.len()).map(move |index| self.get(index))
    }

    /// The physical start position of the first track
    pub(crate) fn origin(self) -> f32 {
        if self.positions.is_empty() {
            0.0
        } else {
            self.get(0).start
        }
    }

    /// Total distance from the start of the first track to the end of the last track
    pub(crate) fn span(self) -> f32 {
        if self.positions.is_empty() {
            0.0
        } else {
            self.get(self.positions.len() - 1).end - self.get(0).start
        }
    }
}

/// A short-lived struct which holds a bunch of parameters for rendering a scene so
/// that we don't have to pass them down as parameters
pub struct BlitzDomPainter<'dom, 'a> {
    /// Input parameters (read only) for generating the Scene
    pub(crate) dom: &'dom BaseDocument,
    pub(crate) scale: f64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) initial_x: f64,
    pub(crate) initial_y: f64,
    /// The id of the document's root element (cached to avoid re-resolving it for every element)
    pub(crate) root_element_id: Option<NodeId>,
    /// Scrollbar hover/drag state, resolved once per scene like the root element
    #[cfg(feature = "scrollbars")]
    pub(crate) hovered_scrollbar: Option<blitz_dom::node::ScrollbarRef>,
    #[cfg(feature = "scrollbars")]
    pub(crate) scrollbar_drag_target: Option<blitz_dom::node::ScrollbarRef>,
    pub(crate) layer_manager: LayerManager,
    /// Reusable scratch allocations shared by all text layouts painted for this document.
    pub(crate) draw_text_context: RefCell<DrawTextContext>,
    /// background-clip:text (local change): while set, elements paint only their glyphs, in opaque
    /// black, to build the mask a `background-clip: text` background is composited through.
    pub(crate) text_mask: std::cell::Cell<bool>,
    /// 3D transforms (local change): the next `render_element` call paints this node with this
    /// transform, and its children only when the flag is set (see render/three_d.rs)
    pub(crate) forced: std::cell::Cell<Option<(NodeId, Affine, bool)>>,
    /// Pixel snapping of box geometry (local change, raea): the transform of the nearest
    /// box with a CSS transform at its (rounded) paint offset. Blink snaps paint offsets to whole
    /// pixels of that space; identity is the device grid.
    pub(crate) snap_space: std::cell::Cell<Affine>,
    /// Cached selection ranges for O(1) lookup: node_id -> (start_offset, end_offset)
    pub(crate) selection_ranges: HashMap<NodeId, (usize, usize)>,

    // Pre-computed `Scene`s for each CustomWidget
    pub(crate) custom_widget_scenes: &'a CustomWidgetSceneMap,
}

impl<'dom, 'a> BlitzDomPainter<'dom, 'a> {
    /// Create a new BlitzDomPainter for the given document
    pub fn new(
        dom: &'dom BaseDocument,
        scale: f64,
        width: u32,
        height: u32,
        initial_x: f64,
        initial_y: f64,
        custom_widget_scenes: &'a CustomWidgetSceneMap,
    ) -> Self {
        let selection_ranges: HashMap<NodeId, (usize, usize)> = dom
            .get_text_selection_ranges()
            .into_iter()
            .map(|(node_id, start, end)| (node_id, (start, end)))
            .collect();

        let layer_manager = LayerManager::default();
        let root_element_id = dom.try_root_element().map(|el| el.id);

        Self {
            dom,
            scale,
            width,
            height,
            initial_x,
            initial_y,
            root_element_id,
            #[cfg(feature = "scrollbars")]
            hovered_scrollbar: dom.hovered_scrollbar(),
            #[cfg(feature = "scrollbars")]
            scrollbar_drag_target: dom.scrollbar_drag_target(),
            layer_manager,
            draw_text_context: RefCell::new(DrawTextContext::default()),
            text_mask: std::cell::Cell::new(false),
            forced: std::cell::Cell::new(None),
            snap_space: std::cell::Cell::new(Affine::IDENTITY),
            selection_ranges,
            custom_widget_scenes,
        }
    }

    /// Draw the current tree to current render surface
    /// Eventually we'll want the surface itself to be passed into the render function, along with things like the viewport
    ///
    /// This assumes styles are resolved and layout is complete.
    /// Make sure you do those before trying to render
    pub fn paint_scene(&self, scene: &mut impl PaintScene) {
        if self.dom.has_pending_critical_resources() {
            return;
        }

        // Simply render the document (the root element (note that this is not the same as the root node)))
        // scene.reset();
        let viewport_scroll = self.dom.as_ref().viewport_scroll();

        // A document without a root element (e.g. an empty iframe sub-document) has
        // nothing to paint.
        let Some(root_element) = self.dom.as_ref().try_root_element() else {
            return;
        };
        let root_id = root_element.id;
        let bg_width = (self.width as f32).max(root_element.final_layout().size.width);
        let bg_height = (self.height as f32).max(root_element.final_layout().size.height);

        let background_color = {
            let html_color = root_element
                .primary_styles()
                .map(|s| s.clone_background_color())
                .unwrap_or(GenericColor::TRANSPARENT_BLACK);
            if html_color == GenericColor::TRANSPARENT_BLACK {
                root_element
                    .children
                    .iter()
                    .find_map(|id| {
                        self.dom
                            .as_ref()
                            .get_node(*id)
                            .filter(|node| node.data.is_element_with_tag_name(&local_name!("body")))
                    })
                    .and_then(|body| body.primary_styles())
                    .map(|style| {
                        let current_color = style.clone_color();
                        style
                            .clone_background_color()
                            .resolve_to_absolute(&current_color)
                    })
            } else {
                let current_color = root_element.primary_styles().unwrap().clone_color();
                Some(html_color.resolve_to_absolute(&current_color))
            }
        };

        if let Some(bg_color) = background_color {
            let bg_color = bg_color.as_srgb_color();
            let rect = Rect::from_origin_size(
                (self.initial_x, self.initial_y),
                (bg_width as f64, bg_height as f64),
            );
            scene.fill(Fill::NonZero, Affine::IDENTITY, bg_color, None, &rect);
        }

        // The root clip rectangle is the viewport (in screen coordinates, with the
        // initial offset already subtracted). Elements outside of this are culled, and
        // scrollports narrow this rectangle further for their descendants.
        let viewport_clip_rect = Rect::new(0.0, 0.0, self.width as f64, self.height as f64);

        self.render_element(
            scene,
            root_id,
            Affine::translate(Vec2 {
                x: self.initial_x - (viewport_scroll.x * self.scale),
                y: self.initial_y - (viewport_scroll.y * self.scale),
            }),
            viewport_clip_rect,
        );

        // Render debug overlay
        if self.dom.devtools().highlight_hover {
            if let Some(node_id) = self.dom.as_ref().get_hover_node_id() {
                render_debug_overlay(
                    scene,
                    self.dom,
                    node_id,
                    self.scale,
                    self.initial_x,
                    self.initial_y,
                );
            }
        }
        if let Some(node_id) = self.dom.devtools().highlight_node {
            // Only nodes with element data have a layout to visualise
            if self
                .dom
                .as_ref()
                .get_node(node_id)
                .is_some_and(|node| node.element_data().is_some())
            {
                render_debug_overlay(
                    scene,
                    self.dom,
                    node_id,
                    self.scale,
                    self.initial_x,
                    self.initial_y,
                );
            }
        }
    }

    /// Renders a node, but is guaranteed that the node is an element
    /// This is because the font_size is calculated from layout resolution and all text is rendered directly here, instead
    /// of a separate text stroking phase.
    ///
    /// In Blitz, text styling gets its attributes from its container element/resolved styles
    /// In other libraries, text gets its attributes from a `text` element - this is not how HTML works.
    ///
    /// Approaching rendering this way guarantees we have all the styles we need when rendering text with not having
    /// to traverse back to the parent for its styles, or needing to pass down styles
    #[allow(clippy::too_many_lines)]
    fn render_element(
        &self,
        scene: &mut impl PaintScene,
        node_id: NodeId,
        parent_style_transform: Affine,
        clip_rect: Rect,
    ) {
        let forced = self.forced.take().filter(|f| f.0 == node_id);
        let paint_children = forced.is_none_or(|f| f.2);
        let node = &self.dom.as_ref().tree()[node_id];

        // Early return if the element is hidden
        if matches!(node.taffy_display(), taffy::Display::None) {
            return;
        }

        // Only draw elements with a style
        let Some(styles) = node.primary_styles() else {
            return;
        };

        // Hide inputs with type=hidden
        // Implemented here rather than using the style engine for performance reasons
        if node.local_name() == "input" && node.attr(local_name!("type")) == Some("hidden") {
            return;
        }

        // Hide elements with a visibility style other than visible
        if styles.get_inherited_box().visibility != StyloVisibility::Visible {
            return;
        }

        let effects = styles.get_effects();
        let mut opacity = effects.opacity;
        // raea: Chrome composites 3D planes and will-change layers in its software compositor,
        // which takes their opacity as a byte truncated from opacity * 255 (SkPaint::setAlpha): 0.9
        // composites as 229/255, where rounding gives 230
        if opacity < 1.0 && {
            use style::values::specified::box_::WillChangeBits;
            forced.is_some()
                || styles
                    .get_box()
                    .will_change
                    .bits
                    .intersects(WillChangeBits::TRANSFORM | WillChangeBits::OPACITY)
        } {
            opacity = (opacity * 255.0).floor() / 255.0;
        }
        if opacity == 0.0 {
            return;
        }
        let has_opacity = opacity < 1.0;

        // TODO: account for overflow_x vs overflow_y
        let overflow_x = styles.get_box().overflow_x;
        let overflow_y = styles.get_box().overflow_y;
        // `contain: paint` (and stronger values like `strict`/`content`) clips the element's
        // contents to its padding box. Paint containment does not apply to non-atomic inlines
        // or internal table boxes other than table-cell.
        let contain_paint = styles.get_box().clone_contain().contains(Contain::PAINT) && {
            let display = styles.clone_display();
            let is_internal_table_box_other_than_cell = display.outside()
                == DisplayOutside::InternalTable
                && display.inside() != DisplayInside::TableCell;
            !display.is_inline_flow() && !is_internal_table_box_other_than_cell
        };
        let is_image = node
            .element_data()
            .and_then(|e| e.raster_image_data())
            .is_some();
        let is_sub_doc = node
            .element_data()
            .and_then(|el| el.sub_doc_data())
            .is_some();
        let is_text_input = node
            .element_data()
            .and_then(|el| el.text_input_data())
            .is_some();
        // The root element's overflow is propagated to the viewport (which is clipped by the
        // window/surface bounds), so the root element must not clip its own overflow.
        let is_root_element = self.root_element_id == Some(node_id);
        let should_clip = !is_root_element
            && (is_image
                || is_sub_doc
                || is_text_input
                || contain_paint
                || !matches!(overflow_x, Overflow::Visible)
                || !matches!(overflow_y, Overflow::Visible));

        // Apply padding/border offset to inline root
        let taffy::Layout {
            size,
            border,
            padding,
            location,
            ..
        } = *node.final_layout();
        let box_position = Vec2::new(location.x as f64, location.y as f64) * self.scale;
        let box_size = Size::new(size.width as f64, size.height as f64);
        let border_box = Rect::from_origin_size(box_position.to_point(), box_size);
        let scaled_pb = (padding + border).map(f64::from);
        let content_position = kurbo::Point {
            x: scaled_pb.left,
            y: scaled_pb.top,
        };
        let content_box_size = kurbo::Size {
            width: (size.width as f64 - scaled_pb.left - scaled_pb.right) * self.scale,
            height: (size.height as f64 - scaled_pb.top - scaled_pb.bottom) * self.scale,
        };

        // Don't render things that are out of view
        let overflow = *node.scrollable_overflow();
        let mut new_snap_space = None;
        let transform = match forced {
            Some((_, transform, _)) => transform,
            None => match self.placement(node_id, parent_style_transform, box_position) {
                three_d::Placement::Normal => {
                    let local = node.transform().as_deref().copied().unwrap_or_default();
                    let positioned = parent_style_transform * Affine::translate(box_position);
                    if local == Affine::IDENTITY {
                        positioned
                    } else {
                        // raea: Blink's PaintOffsetTranslation. A transformed box's layout
                        // offset is rounded to whole pixels of the enclosing snapping space; the
                        // fractional rest carries into its contents only through a 2D translation
                        // without will-change: transform (CanPropagateSubpixelAccumulation).
                        let space = self.snap_space.get();
                        let rel = space.inverse() * positioned;
                        let [a, b, c, d, e, f] = rel.as_coeffs();
                        let rounded = if b.abs() < 1e-9 && c.abs() < 1e-9 && a > 0.0 && (a - d).abs() < 1e-9 {
                            space * Affine::translate((e.round() - e, f.round() - f)) * rel
                        } else {
                            positioned
                        };
                        let [la, lb, lc, ld, _, _] = local.as_coeffs();
                        let translation_only = la == 1.0 && lb == 0.0 && lc == 0.0 && ld == 1.0;
                        let will_change_transform = styles
                            .get_box()
                            .will_change
                            .bits
                            .contains(style::values::specified::box_::WillChangeBits::TRANSFORM);
                        new_snap_space = Some(rounded * local);
                        if translation_only && !will_change_transform {
                            positioned * local
                        } else {
                            rounded * local
                        }
                    }
                }
                three_d::Placement::Flat(transform) => transform,
                three_d::Placement::ThreeD(matrix) => {
                    self.render_3d(scene, node_id, matrix, clip_rect);
                    return;
                }
            },
        };

        let screen_transform = Affine::translate(Vec2 {
            x: -self.initial_x,
            y: -self.initial_y,
        }) * transform;
        let screen_bbox = screen_transform.transform_rect_bbox(overflow.union(border_box));

        // Cull elements that fall entirely outside the current clip rectangle. In addition to
        // the viewport, `clip_rect` is narrowed by any ancestor scrollport (see below), so this
        // also culls elements scrolled out of view inside a clipping/scrolling container.
        if screen_bbox.x1 < clip_rect.x0
            || screen_bbox.x0 > clip_rect.x1
            || screen_bbox.y1 < clip_rect.y0
            || screen_bbox.y0 > clip_rect.y1
        {
            return;
        }

        // Cull elements whose transformed geometry is not representable in f32 (e.g.
        // `transform: scale(99e99)`): the renderer works in f32 and flattening such paths
        // can attempt to allocate an unbounded number of line segments.
        let f32_max = f64::from(f32::MAX);
        if !screen_bbox.is_finite()
            || [
                screen_bbox.x0,
                screen_bbox.y0,
                screen_bbox.x1,
                screen_bbox.y1,
            ]
            .iter()
            .any(|c| c.abs() > f32_max)
        {
            return;
        }

        // Optimise zero-area (/very small area) clips by not rendering at all
        let clip_area = content_box_size.width * content_box_size.height;
        let overflow_area =
            node.scrollable_overflow().width() * node.scrollable_overflow().height();
        if should_clip && clip_area < 0.01 && overflow_area < 0.01 {
            return;
        }

        #[cfg(feature = "custom-widget")]
        let custom_widget_scene = self.custom_widget_scenes.get(&(self.dom.id(), node_id));
        #[cfg(not(feature = "custom-widget"))]
        let custom_widget_scene = None;

        // Apply CSS transform property (where transforms are 2d)

        // A CSS transform (or a 3D plane's own matrix) starts a new snapping space at its local origin;
        // restored when this element is done.
        let has_css_transform = forced.is_some() || node.transform().as_deref().is_some_and(|t| *t != Affine::IDENTITY);
        let saved_snap_space = self.snap_space.get();
        let _restore = RestoreCell(&self.snap_space, saved_snap_space);
        if has_css_transform {
            self.snap_space.set(new_snap_space.unwrap_or(transform));
        }

        let mut cx = self.element_cx(node, *node.final_layout(), transform, custom_widget_scene);

        // If this element clips its overflow it establishes a scrollport: narrow the clip
        // rectangle passed to descendants to the visible (clipped) region so that content
        // scrolled out of view is culled rather than drawn and clipped away. The box used
        // here matches the clip applied to the content below.
        let child_clip_rect = if should_clip {
            let clip_box = if is_text_input {
                cx.frame.content_box_bbox()
            } else {
                cx.frame.padding_box_bbox()
            };
            clip_rect.intersect(screen_transform.transform_rect_bbox(clip_box))
        } else {
            clip_rect
        };

        // Compute clip-path (if any) and wrap all rendering in a clip layer
        let clip_path_shape = cx.clip_path_shape();
        let has_clip_path = clip_path_shape.is_some();
        // (raea) without a clip-path the layer isn't pushed and its shape isn't read: an empty path, not the border
        // box's (ZeroHeap: one was made for every element a frame)
        let mut clip_path_for_layer = clip_path_shape.unwrap_or_default();
        clip_path_for_layer.apply_affine(Affine::scale(self.scale));

        // CSS 2 `clip` property (absolutely positioned elements only). Applies to the whole of
        // the element's rendering (including outlines and shadows), so it is the outermost layer.
        let css_clip_rect = cx.css_clip_rect();
        if css_clip_rect.is_some_and(|rect| rect.is_zero_area()) {
            return;
        }
        // mix-blend-mode (local change): the element's whole painting, descendants included,
        // goes through one layer that is blended into what is already drawn. Unclipped: the layer
        // spans the current clip rectangle, in scene space.
        let blend = blend_mode(effects.mix_blend_mode);
        let blend_layer_pushed = blend.is_some();
        if let Some(blend) = blend {
            scene.push_layer(
                Fill::NonZero,
                blend,
                1.0,
                Affine::translate((self.initial_x, self.initial_y)),
                &clip_rect,
                None,
                None,
            );
        }

        // isolation: isolate (local change): the element's painting composites as one group on a transparent
        // backdrop, so its descendants' blend modes blend only with what the group drew. Unclipped like the
        // blend layer (a mix-blend-mode layer isolates already); only pushed when something inside blends, the
        // one case where the group changes pixels.
        let isolation_layer_pushed = blend.is_none()
            && styles.get_box().isolation == style::computed_values::isolation::T::Isolate
            && self.has_blending_descendant(node_id);
        if isolation_layer_pushed {
            scene.push_layer(
                Fill::NonZero,
                peniko::BlendMode::default(),
                1.0,
                Affine::translate((self.initial_x, self.initial_y)),
                &clip_rect,
                None,
                None,
            );
        }

        let css_clip_layer_pushed = self.layer_manager.maybe_push_layer(
            scene,
            css_clip_rect.is_some(),
            1.0,
            cx.transform,
            &css_clip_rect.unwrap_or(Rect::ZERO),
            None,
            None,
        );

        let text_mask = self.text_mask.get();
        // raea: outlines and outer shadows belong to the element's painting, so its opacity,
        // filter, clip-path and mask apply to them (Blink). They stay outside only for
        // backdrop-filter, whose layer is clipped to the border box.
        let has_backdrop_filter = !effects.backdrop_filter.0.is_empty();
        if !text_mask && has_backdrop_filter {
            cx.draw_outline(scene, true);
            cx.draw_outset_box_shadow(scene);
        }

        // clip-path clip ayer
        self.layer_manager.maybe_with_layer(
            scene,
            has_clip_path,
            1.0,
            cx.transform,
            &clip_path_for_layer,
            None,
            None,
            |scene| {
                // If the element has a CSS `mask`, then push an isolation layer for the
                // masked content. The mask is applied when the layer is popped below.
                let mask_layer_pushed = cx.maybe_push_css_mask_layer(scene);
                // `cx.transform` is mutated to apply scroll offsets while drawing content.
                // Save it so that the mask can be drawn untransformed by scroll offsets.
                let unscrolled_transform = cx.transform;

                // raea: `url(#id)` items resolve to the SVG <filter> they name, in the element's
                // user space (CSS px from the border box's top-left)
                let user_box = (cx.frame.border_box.width() / self.scale, cx.frame.border_box.height() / self.scale);
                let filter = crate::filters::convert_element_filters(self.dom, &effects.filter.0, user_box).map(Arc::new);
                let backdrop_filter = convert_filters(&effects.backdrop_filter.0).map(Arc::new);

                // Adjust effect layer clip by filter expansion area
                //
                // Returns a rectangle centered at the origin representing how much the filter
                // expands the processing region in each direction. The rect coordinates are:
                // - x0: negative left expansion
                // - y0: negative top expansion
                // - x1: positive right expansion
                // - y1: positive bottom expansion
                let filter_expansion_area = filter
                    .as_ref()
                    .map(|f| f.expansion_rect())
                    .unwrap_or(Rect::ZERO);

                let mut effect_layer_clip = cx.frame.border_box_bbox();
                effect_layer_clip.x0 += filter_expansion_area.x0;
                effect_layer_clip.y0 += filter_expansion_area.y0;
                effect_layer_clip.x1 += filter_expansion_area.x1;
                effect_layer_clip.y1 += filter_expansion_area.y1;
                // local change: opacity/filter layers don't clip in CSS (content that overflows the
                // border box, e.g. a slightly rotated child, stays visible). A margin proportional to the
                // box covers transform/glyph overhang without making every layer viewport-sized.
                if !node.children.is_empty() {
                    let margin = effect_layer_clip.width().max(effect_layer_clip.height()) * 0.125 + 16.0 * self.scale;
                    effect_layer_clip = effect_layer_clip.inflate(margin, margin);
                }
                if !has_backdrop_filter {
                    if let Some(outset) = cx.outset_paint_rect() {
                        effect_layer_clip = effect_layer_clip.union(outset);
                    }
                }

                // Opacity/Filter layer if box has opacity or a filter.
                // Clipped to border-box as it needs to include the background and borders.
                // backdrop-filter (local change): Chrome clips the filtered backdrop to the border box
                // with its radii, so the layer takes the rounded border-box path.
                // raea: Chrome's paint playback folds an opacity layer that holds a single
                // draw into that draw's alpha (cc PlaybackFoldingIterator). Compositing a layer
                // instead rounds differently (one level darker on a faint overlay).
                let fold_opacity = has_opacity
                    && filter.is_none()
                    && backdrop_filter.is_none()
                    && !text_mask
                    && !mask_layer_pushed
                    && cx.folds_opacity_into_background();
                if fold_opacity {
                    cx.fold_alpha = opacity;
                }
                let push_effect_layer = (has_opacity && !fold_opacity) || filter.is_some() || backdrop_filter.is_some();
                let effect_layer_shape = if !push_effect_layer {
                    // (raea) not pushed: its shape isn't read (ZeroHeap: a path an element a frame)
                    kurbo::BezPath::new()
                } else if backdrop_filter.is_some() && filter.is_none() {
                    cx.frame.border_box_path()
                } else {
                    effect_layer_clip.to_path(0.1)
                };
                // raea: an SVG filter runs in the element's user space (Skia scales its
                // parameters by the layer transform) and draws only inside its filter region (each
                // primitive is cropped to it), so the layer covers the region as well
                let (effect_transform, effect_layer_shape) = match filter.as_ref().and_then(|f| f.region()) {
                    Some(region) if push_effect_layer => {
                        let user = effect_layer_clip.scale_from_origin(1.0 / self.scale).union(region);
                        (cx.transform * Affine::scale(self.scale), user.to_path(0.1))
                    }
                    _ => (cx.transform, effect_layer_shape),
                };
                self.layer_manager.maybe_with_layer(
                    scene,
                    push_effect_layer,
                    opacity,
                    effect_transform,
                    &effect_layer_shape,
                    filter,
                    backdrop_filter,
                    |scene| {
                        if !text_mask && !has_backdrop_filter {
                            cx.draw_outline(scene, false);
                            cx.draw_outset_box_shadow(scene);
                        }
                        if !text_mask {
                            if cx.background_clips_to_text() {
                                cx.draw_background_clipped_to_text(scene, content_position, child_clip_rect);
                            } else {
                                cx.draw_background(scene);
                            }
                            cx.draw_inset_box_shadow(scene);
                            cx.draw_table_row_backgrounds(scene);
                            cx.draw_table_borders(scene);
                            cx.draw_border(scene);
                            cx.stroke_devtools(scene);
                        }

                        // TODO: allow layers with opacity to be unclipped (overflow: visible)
                        // (raea) its path made only for a clip that's pushed (ZeroHeap: one an element a frame)
                        let clip = if !should_clip {
                            &kurbo::BezPath::new()
                        } else if is_text_input {
                            &cx.frame.content_box_path()
                        } else {
                            &cx.frame.padding_box_path()
                        };

                        // Clip layer if box requires clipping. Opacity set to 1.0
                        self.layer_manager.maybe_with_layer(
                            scene,
                            should_clip,
                            1.0, // opacity
                            cx.transform,
                            clip,
                            None,
                            None,
                            |scene| {
                                // Now that background has been drawn, offset pos and cx in order to draw our contents scrolled
                                let content_position = Point {
                                    x: content_position.x - node.scroll_offset().x,
                                    y: content_position.y - node.scroll_offset().y,
                                };

                                cx.transform = cx.transform.then_translate(Vec2 {
                                    x: -node.scroll_offset().x * self.scale,
                                    y: -node.scroll_offset().y * self.scale,
                                });
                                if !text_mask {
                                    cx.draw_image(scene);
                                    #[cfg(feature = "svg")]
                                    cx.draw_svg(scene);
                                    #[cfg(feature = "custom-widget")]
                                    cx.draw_custom_widget(scene);
                                    cx.draw_sub_document(scene);
                                    cx.draw_input(scene);
                                    cx.draw_text_input_text(scene, content_position);
                                }
                                crate::text::TEXT_CLIP.with(|c| c.set(child_clip_rect));
                                cx.draw_inline_layout(scene, content_position);
                                if !text_mask {
                                    cx.draw_marker(scene, content_position);
                                }
                                if paint_children {
                                    cx.draw_children(scene, cx.transform, child_clip_rect);
                                }
                            },
                        );

                        // Overlay scrollbars, drawn unscrolled above the
                        // clipped content.
                        #[cfg(feature = "scrollbars")]
                        {
                            cx.transform = unscrolled_transform;
                            cx.draw_scrollbars(scene);
                        }
                        // raea: an `outline-style: auto` ring in its own phase, after the border and the content
                        if !text_mask && !has_backdrop_filter {
                            cx.transform = unscrolled_transform;
                            cx.draw_focus_ring_phase(scene);
                        }
                    },
                );

                // Apply the CSS `mask` (if any) to the content drawn above
                cx.transform = unscrolled_transform;
                cx.maybe_pop_css_mask_layer(scene, mask_layer_pushed);
            },
        );

        self.layer_manager
            .maybe_pop_layer(scene, css_clip_layer_pushed);
        if isolation_layer_pushed {
            scene.pop_layer();
        }
        if blend_layer_pushed {
            scene.pop_layer();
        }
    }

    /// Whether anything under `node_id` has a mix-blend-mode other than normal (local change, raea).
    fn has_blending_descendant(&self, node_id: NodeId) -> bool {
        let tree = self.dom.as_ref().tree();
        let mut stack: Vec<NodeId> = tree[node_id].children.iter().copied().collect();
        while let Some(id) = stack.pop() {
            let Some(n) = tree.get(id) else { continue };
            if n.primary_styles().is_some_and(|st| {
                st.get_effects().mix_blend_mode != style::computed_values::mix_blend_mode::T::Normal
            }) {
                return true;
            }
            stack.extend(n.children.iter().copied());
        }
        false
    }

    fn render_node(
        &self,
        scene: &mut impl PaintScene,
        node_id: NodeId,
        parent_style_transform: Affine,
        clip_rect: Rect,
    ) {
        let node = &self.dom.as_ref().tree()[node_id];

        match &node.data {
            NodeData::Element(_) | NodeData::AnonymousBlock(_) => {
                self.render_element(scene, node_id, parent_style_transform, clip_rect)
            }
            NodeData::Text(TextNodeData { .. }) => {
                // Text nodes should never be rendered directly
                // (they should always be rendered as part of an inline layout)
                // unreachable!()
            }
            NodeData::Document(_) => {}
            // NodeData::Doctype => {}
            NodeData::Comment { .. } => {} // NodeData::ProcessingInstruction { .. } => {}
        }
    }

    fn element_cx(
        &'dom self,
        node: &'dom Node,
        layout: Layout,
        transform: Affine,
        custom_widget_scene: Option<&'a Scene>,
    ) -> ElementCx<'dom, 'a> {
        let style = node
            .stylo_element_data()
            .primary_styles()
            .as_ref()
            .map(|styles| (*styles).clone())
            .unwrap_or_else(|| {
                ComputedValues::initial_values_with_font_override(Font::initial_values()).to_arc()
            });

        let scale = self.scale;

        // todo: maybe cache this so we don't need to constantly be figuring it out
        // It is quite a bit of math to calculate during render/traverse
        // Also! we can cache the bezpaths themselves, saving us a bunch of work
        let mut frame = create_css_rect(&style, &layout, scale);
        // raea: layout keeps LayoutUnit (1/64 px) precision; box painting (backgrounds,
        // borders, shadows, clips, replaced content) is pixel-snapped like Blink's PixelSnappedIntRect
        // while text keeps its fractional position (uniformly scaled, unrotated boxes only).
        {
            // Blink snaps the paint offset in the space of the nearest transformed ancestor (whole
            // pixels there, including rotated spaces), then applies the transform
            let space = self.snap_space.get();
            let rel = space.inverse() * transform;
            let [a, b, c, d, e, f] = rel.as_coeffs();
            if space.determinant().abs() > 1e-12 && b.abs() < 1e-9 && c.abs() < 1e-9 && a > 0.0 && (a - d).abs() < 1e-9 {
                let snap = |v: f64, k: f64, t: f64| ((k * v + t).round() - t) / k;
                let bb = frame.border_box;
                let snapped = Rect::new(snap(bb.x0, a, e), snap(bb.y0, d, f), snap(bb.x1, a, e), snap(bb.y1, d, f));
                if snapped != bb {
                    frame = create_css_rect_at(&style, &layout, scale, snapped);
                }
            }
        }

        let element = node.element_data().unwrap();

        ElementCx {
            context: self,
            frame,
            scale,
            style,
            node,
            element,
            transform,
            #[cfg(feature = "svg")]
            svg: element.svg_data(),
            text_input: element.text_input_data(),
            list_item: element.list_item_data.as_deref(),
            devtools: self.dom.devtools(),
            custom_widget_scene,
            fold_alpha: 1.0,
        }
    }
}

fn to_image_quality(image_rendering: ImageRendering) -> peniko::ImageQuality {
    match image_rendering {
        ImageRendering::Auto => peniko::ImageQuality::Medium,
        ImageRendering::CrispEdges => peniko::ImageQuality::Low,
        ImageRendering::Pixelated => peniko::ImageQuality::Low,
    }
}

/// Ensure that the `resized_image` field has a correctly sized image
fn to_peniko_image(image: &RasterImageData, quality: peniko::ImageQuality) -> peniko::ImageBrush {
    peniko::ImageBrush {
        image: ImageData {
            data: image.data.clone(),
            format: peniko::ImageFormat::Rgba8,
            width: image.width,
            height: image.height,
            alpha_type: peniko::ImageAlphaType::Alpha,
        },
        sampler: ImageSampler {
            x_extend: peniko::Extend::Repeat,
            y_extend: peniko::Extend::Repeat,
            quality,
            alpha: 1.0,
        },
    }
}

/// Rasterized inline SVGs by element (local change, raea): the recording they were drawn from,
/// their size and pixels, evicted oldest first past a byte budget.
#[derive(Default)]
struct SvgCache {
    entries: std::collections::HashMap<NodeId, (anyrender::Scene, (u32, u32), peniko::Blob<u8>)>,
    order: std::collections::VecDeque<NodeId>,
    bytes: usize,
}

impl SvgCache {
    fn budget() -> usize {
        static BUDGET: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        *BUDGET.get_or_init(|| {
            let default_mb = if cfg!(target_os = "ios") { 16 } else { 48 };
            std::env::var("RAEA_SVG_CACHE_MB").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(default_mb) << 20
        })
    }

    fn get(&mut self, node: NodeId, scene: &anyrender::Scene, size: (u32, u32)) -> Option<peniko::Blob<u8>> {
        self.entries.get(&node).filter(|(s, sz, _)| *sz == size && s == scene).map(|(_, _, blob)| blob.clone())
    }

    fn insert(&mut self, node: NodeId, scene: anyrender::Scene, size: (u32, u32), blob: peniko::Blob<u8>) {
        let len = blob.data().len();
        if let Some((_, _, old)) = self.entries.remove(&node) {
            self.bytes -= old.data().len();
            self.order.retain(|n| *n != node);
        }
        if len > Self::budget() {
            return;
        }
        while self.bytes + len > Self::budget() {
            let Some(oldest) = self.order.pop_front() else { break };
            if let Some((_, _, old)) = self.entries.remove(&oldest) {
                self.bytes -= old.data().len();
            }
        }
        self.bytes += len;
        self.order.push_back(node);
        self.entries.insert(node, (scene, size, blob));
    }
}

thread_local! {
    static SVG_CACHE: std::cell::RefCell<SvgCache> = std::cell::RefCell::new(SvgCache::default());
}

/// Restores a cell's value when dropped (the snapping origin around an element's subtree).
struct RestoreCell<'c, T: Copy>(&'c std::cell::Cell<T>, T);

impl<T: Copy> Drop for RestoreCell<'_, T> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
}

/// A context of loaded and hot data to draw the element from
struct ElementCx<'dom, 'a> {
    context: &'dom BlitzDomPainter<'dom, 'a>,
    frame: CssBox,
    style: style::servo_arc::Arc<ComputedValues>,
    scale: f64,
    node: &'dom Node,
    element: &'dom ElementData,
    transform: Affine,
    #[cfg(feature = "svg")]
    svg: Option<&'dom usvg::Tree>,
    text_input: Option<&'dom TextInputData>,
    list_item: Option<&'dom ListItemLayout>,
    devtools: &'dom DevtoolSettings,
    #[cfg_attr(not(feature = "custom-widget"), expect(unused))]
    custom_widget_scene: Option<&'a Scene>,
    /// raea: element opacity folded into its single background draw (see
    /// `folds_opacity_into_background`); 1.0 otherwise.
    fold_alpha: f32,
}

/// Converts parley BoundingBox into peniko Rect
fn convert_rect(rect: &parley::BoundingBox) -> kurbo::Rect {
    peniko::kurbo::Rect::new(rect.x0, rect.y0, rect.x1, rect.y1)
}

impl ElementCx<'_, '_> {
    /// raea: whether the element's painting is exactly one draw: a background colour or a
    /// single raster background image, with no children, content, borders, shadows or outline.
    fn folds_opacity_into_background(&self) -> bool {
        if !self.node.children.is_empty()
            || !matches!(self.element.special_data, blitz_dom::node::SpecialElementData::None)
            || self.element.inline_layout_data.is_some()
            || self.list_item.is_some()
            || self.text_input.is_some()
            || self.background_clips_to_text()
        {
            return false;
        }
        #[cfg(feature = "svg")]
        if self.svg.is_some() {
            return false;
        }
        let style = &self.style;
        let b = style.get_border();
        use style::values::computed::BorderStyle;
        let side = |w: &style::values::computed::BorderSideWidth, st: BorderStyle| {
            w.0 .0 > 0 && !matches!(st, BorderStyle::None | BorderStyle::Hidden)
        };
        if side(&b.border_top_width, b.border_top_style)
            || side(&b.border_right_width, b.border_right_style)
            || side(&b.border_bottom_width, b.border_bottom_style)
            || side(&b.border_left_width, b.border_left_style)
            || !style.get_effects().box_shadow.0.is_empty()
        {
            return false;
        }
        let outline = style.get_outline();
        if outline.outline_width.0 .0 > 0
            && !matches!(
                outline.outline_style,
                style::values::computed::OutlineStyle::BorderStyle(BorderStyle::None)
            )
        {
            return false;
        }
        let bg = style.get_background();
        let has_color = bg.background_color.resolve_to_absolute(&style.clone_color()).alpha > 0.0;
        let mut images = bg
            .background_image
            .0
            .iter()
            .enumerate()
            .filter(|(_, i)| !matches!(i, style::values::generics::image::GenericImage::None));
        match (has_color, images.next(), images.next()) {
            (true, None, _) => true,
            (false, Some((idx, style::values::generics::image::GenericImage::Url(_))), None) => {
                matches!(
                    self.element.background_images.get(idx),
                    Some(Some(data)) if matches!(data.image, blitz_dom::node::ImageData::Raster(_))
                )
            }
            _ => false,
        }
    }

    /// Paint overlay scrollbar thumbs for scroll containers: `overflow:
    /// scroll`, or `auto` when the content overflows (never `hidden`/`clip`,
    /// which scroll only programmatically). Thumbs appear on scroll and fade
    /// out after a delay ([`BaseDocument::scrollbar_opacity`]); never-scrolled
    /// containers paint nothing, keeping thumbs out of static reftest
    /// screenshots.
    ///
    /// Geometry comes from [`Node::scrollbar_thumb`], shared with the
    /// thumb-drag hit testing in blitz-dom.
    #[cfg(feature = "scrollbars")]
    fn draw_scrollbars(&self, scene: &mut impl PaintScene) {
        // css-scrollbars-1 scrollbar-color: author thumb/track colors
        use blitz_dom::node::{ScrollbarColor, ScrollbarRef};
        use taffy::AbsoluteAxis;
        let (custom_thumb, custom_track) = match self.node.scrollbar_color() {
            ScrollbarColor::Auto => (None, None),
            ScrollbarColor::Colors { thumb, track } => {
                (Some(thumb.as_srgb_color()), Some(track.as_srgb_color()))
            }
        };

        let drag_target = self.context.scrollbar_drag_target;
        let hovered_thumb = self.context.hovered_scrollbar;

        // scrollbar-color doesn't affect overlay visibility: persistence is
        // UA policy, not author styling.
        let node_id = self.node.id;
        let opacity = self.context.dom.scrollbar_opacity(node_id);
        if opacity == 0.0 {
            return;
        }

        // Default thumb palette for the used color scheme; thumbs paint as
        // fill plus a thin contrast stroke so they read over same-colored
        // content.
        let dark_scheme =
            self.context.dom.viewport().color_scheme == blitz_traits::shell::ColorScheme::Dark;
        let (thumb_rest, thumb_hover, thumb_active, stroke_color) = if dark_scheme {
            (
                Color::from_rgba8(214, 214, 214, 178),
                Color::from_rgba8(190, 190, 190, 222),
                Color::from_rgba8(172, 172, 172, 255),
                Color::from_rgba8(0, 0, 0, 102),
            )
        } else {
            (
                Color::from_rgba8(128, 128, 128, 178),
                Color::from_rgba8(152, 152, 152, 222),
                Color::from_rgba8(170, 170, 170, 255),
                Color::from_rgba8(255, 255, 255, 102),
            )
        };

        // Chromium's hovered/pressed scrollbar contrast ratios.
        const HOVER_CONTRAST: f32 = 1.8;
        const ACTIVE_CONTRAST: f32 = 1.3;

        for axis in [AbsoluteAxis::Vertical, AbsoluteAxis::Horizontal] {
            if !self.node.wants_scrollbar(axis) {
                continue;
            }
            let Some(thumb) = self.node.scrollbar_thumb(axis) else {
                continue;
            };

            let rect = thumb.scale_from_origin(self.scale);

            // Track (only when the author specified a track color)
            if let Some(track_color) = custom_track {
                let padding_box = self.frame.padding_box;
                let track_rect = match axis {
                    AbsoluteAxis::Horizontal => {
                        Rect::new(padding_box.x0, rect.y0, padding_box.x1, rect.y1)
                    }
                    AbsoluteAxis::Vertical => {
                        Rect::new(rect.x0, padding_box.y0, rect.x1, padding_box.y1)
                    }
                };
                scene.fill(
                    Fill::NonZero,
                    self.transform,
                    track_color.multiply_alpha(opacity),
                    None,
                    &track_rect,
                );
            }

            let this = ScrollbarRef { node_id, axis };
            let is_active = drag_target == Some(this);
            let is_hovered = hovered_thumb == Some(this);
            let color = match custom_thumb {
                Some(base) if is_active => crate::color::blend_for_contrast(base, ACTIVE_CONTRAST),
                Some(base) if is_hovered => crate::color::blend_for_contrast(base, HOVER_CONTRAST),
                Some(base) => base,
                None if is_active => thumb_active,
                None if is_hovered => thumb_hover,
                None => thumb_rest,
            };
            let radius = match axis {
                AbsoluteAxis::Horizontal => rect.height() / 2.0,
                AbsoluteAxis::Vertical => rect.width() / 2.0,
            };
            scene.fill(
                Fill::NonZero,
                self.transform,
                color.multiply_alpha(opacity),
                None,
                &rect.to_rounded_rect(radius),
            );
            // Contrast stroke, default thumbs only: an author-specified
            // scrollbar-color is rendered exactly as given.
            if custom_thumb.is_none() {
                let stroke_width = self.scale;
                let stroke_rect = rect.inset(-stroke_width / 2.0);
                scene.stroke(
                    &Stroke::new(stroke_width),
                    self.transform,
                    stroke_color.multiply_alpha(opacity),
                    None,
                    &stroke_rect.to_rounded_rect(radius - stroke_width / 2.0),
                );
            }
        }
    }

    fn draw_inline_layout(&self, scene: &mut impl PaintScene, pos: Point) {
        if self.node.flags.is_inline_root() {
            let text_layout = self.element
                .inline_layout_data
                .as_ref()
                .unwrap_or_else(|| {
                    panic!("Tried to render node marked as inline root that does not have an inline layout: {:?}", self.node);
                });

            let pos = Point {
                x: pos.x,
                y: pos.y + text_layout.block_offset as f64,
            };
            let transform =
                self.transform * Affine::translate((pos.x * self.scale, pos.y * self.scale));

            let text_mask = self.context.text_mask.get();

            // Render inline element backgrounds (e.g. `<span style="background: ...">`)
            // behind the text and selection highlight.
            if !text_mask {
                crate::text::draw_inline_backgrounds(
                    scene,
                    text_layout.layout.lines(),
                    self.context.dom,
                    transform,
                    self.node.id,
                );
            }

            // Render text selection highlight (if any) using cached selection ranges
            if let Some(&(sel_start, sel_end)) = self.context.selection_ranges.get(&self.node.id).filter(|_| !text_mask) {
                crate::text::draw_text_selection(
                    scene,
                    &text_layout.layout,
                    transform,
                    sel_start,
                    sel_end,
                );
            }

            // Render text
            let mut draw_text_context = self.context.draw_text_context.borrow_mut();
            draw_text_context.mask = self.context.text_mask.get();
            crate::text::stroke_text(
                scene,
                text_layout.layout.lines(),
                self.context.dom,
                transform,
                self.scale,
                self.node.id,
                &mut draw_text_context,
                text_layout.ellipsis.as_deref(),
            );
        }
    }

    fn draw_text_input_text(&self, scene: &mut impl PaintScene, pos: Point) {
        // Render the text in text inputs
        if let Some(input_data) = self.text_input {
            // For single-line inputs, add an offset to vertically center the text input layout
            // within the content box of it's node.
            let y_offset = self.node.text_input_v_centering_offset(self.scale);
            let pos = Point {
                x: pos.x,
                y: pos.y + y_offset,
            };

            // Apply the scroll offset (stored in CSS pixels, scaled here to device pixels) so
            // that the caret stays visible. Single-line inputs scroll horizontally; multi-line
            // inputs scroll vertically.
            let scroll_offset = input_data.scroll_offset as f64 * self.scale;
            let (scroll_x, scroll_y) = if input_data.is_multiline {
                (0.0, scroll_offset)
            } else {
                (scroll_offset, 0.0)
            };

            let transform = self.transform
                * Affine::translate((pos.x * self.scale - scroll_x, pos.y * self.scale - scroll_y));

            if self.node.is_focussed() {
                // Render selection/caret
                for (rect, _line_idx) in input_data.editor.selection_geometry().iter() {
                    scene.fill(
                        Fill::NonZero,
                        transform,
                        SELECTION_COLOR,
                        None,
                        &convert_rect(rect),
                    );
                }
                if let Some(cursor) = input_data.editor.cursor_geometry(1.5) {
                    let color = self.style.get_inherited_text().color;
                    let caret_color = match &self.style.get_inherited_ui().caret_color.0 {
                        ColorOrAuto::Auto => color,
                        ColorOrAuto::Color(caret_color) => caret_color.resolve_to_absolute(&color),
                    };

                    scene.fill(
                        Fill::NonZero,
                        transform,
                        caret_color.as_srgb_color(),
                        None,
                        &convert_rect(&cursor),
                    );
                };
            }

            // Render text
            let mut draw_text_context = self.context.draw_text_context.borrow_mut();
            draw_text_context.mask = self.context.text_mask.get();
            crate::text::stroke_text(
                scene,
                input_data.editor.try_layout().unwrap().lines(),
                self.context.dom,
                transform,
                self.scale,
                self.node.id,
                &mut draw_text_context,
                None,
            );
        }
    }

    fn draw_marker(&self, scene: &mut impl PaintScene, pos: Point) {
        if let Some(ListItemLayout {
            marker,
            position: ListItemLayoutPosition::Outside(layout),
        }) = self.list_item
        {
            // Right align and pad the bullet when rendering outside
            let x_padding = match marker {
                Marker::Char(_) => 8.0,
                Marker::String(_) => 0.0,
            };
            // Outside markers are placed outside the list item's border box
            // (`pos` is the origin of its content box)
            let item_layout = self.node.final_layout();
            let x_offset = -(layout.full_width() / layout.scale()
                + x_padding
                + item_layout.padding.left
                + item_layout.border.left);

            // Align the marker with the baseline of the first line of text in the list item
            let y_offset = if let Some((text_layout, first_text_line)) = &self
                .element
                .inline_layout_data
                .as_ref()
                .and_then(|text_layout| Some((text_layout, text_layout.layout.lines().next()?)))
            {
                (first_text_line.metrics().baseline
                    - layout.lines().next().unwrap().metrics().baseline)
                    / layout.scale()
                    + text_layout.block_offset
            } else {
                0.0
            };

            let pos = Point {
                x: pos.x + x_offset as f64,
                y: pos.y + y_offset as f64,
            };

            let transform =
                self.transform * Affine::translate((pos.x * self.scale, pos.y * self.scale));

            let mut draw_text_context = self.context.draw_text_context.borrow_mut();
            draw_text_context.mask = self.context.text_mask.get();
            crate::text::stroke_text(
                scene,
                layout.lines(),
                self.context.dom,
                transform,
                self.scale,
                self.node.id,
                &mut draw_text_context,
                None,
            );
        }
    }

    fn draw_children(
        &self,
        scene: &mut impl PaintScene,
        parent_style_transform: Affine,
        clip_rect: Rect,
    ) {
        // Negative z_index hoisted nodes

        if let Some(hoisted) = &self.node.stacking_context {
            for hoisted_child in hoisted.neg_z_hoisted_children() {
                let position = hoisted_child.position(self.node.tree(), self.node.id);
                let pos = kurbo::Vec2 {
                    x: position.x as f64 * self.scale,
                    y: position.y as f64 * self.scale,
                };
                self.render_node(
                    scene,
                    hoisted_child.node_id,
                    parent_style_transform.pre_translate(pos),
                    clip_rect,
                );
            }
        }

        // Regular children
        if let Some(children) = &*self.node.paint_children.borrow() {
            for child_id in children {
                // Fixed-position children of the root element are positioned against
                // the viewport and do not scroll with it, so cancel out the viewport
                // scroll (applied in `paint_scene`). A fixed box contained by a
                // transformed ancestor scrolls with it like any out-of-flow box.
                let child = &self.context.dom.as_ref().tree()[*child_id];
                let child_transform = if child.taffy_position() == taffy::Position::Fixed
                    && Some(self.node.id) == self.context.root_element_id
                {
                    let scroll = self.context.dom.as_ref().viewport_scroll();
                    parent_style_transform.pre_translate(kurbo::Vec2 {
                        x: scroll.x * self.scale,
                        y: scroll.y * self.scale,
                    })
                } else {
                    parent_style_transform
                };
                self.render_node(scene, *child_id, child_transform, clip_rect);
            }
        }

        // Positive z_index hoisted nodes
        if let Some(hoisted) = &self.node.stacking_context {
            for hoisted_child in hoisted.pos_z_hoisted_children() {
                let position = hoisted_child.position(self.node.tree(), self.node.id);
                let pos = kurbo::Vec2 {
                    x: position.x as f64 * self.scale,
                    y: position.y as f64 * self.scale,
                };
                self.render_node(
                    scene,
                    hoisted_child.node_id,
                    parent_style_transform.pre_translate(pos),
                    clip_rect,
                );
            }
        }
    }

    #[cfg(feature = "svg")]
    fn draw_svg(&self, scene: &mut impl PaintScene) {
        use style::properties::generated::longhands::object_fit::computed_value::T as ObjectFit;

        let Some(svg) = self.svg else {
            return;
        };

        let width = self.frame.content_box.width() as u32;
        let height = self.frame.content_box.height() as u32;
        let svg_size = svg.size();

        let x = self.frame.content_box.origin().x;
        let y = self.frame.content_box.origin().y;

        // let object_fit = self.style.clone_object_fit();
        let object_position = self.style.clone_object_position();

        // Apply object-fit algorithm
        let container_size = taffy::Size {
            width: width as f32,
            height: height as f32,
        };
        let object_size = taffy::Size {
            width: svg_size.width(),
            height: svg_size.height(),
        };
        // local change (raea): an inline <svg> maps its viewBox into its box by its own
        // preserveAspectRatio (upstream always used "xMidYMid meet"): none stretches, the alignment places
        // it, meet fits inside, slice covers
        let par = (self.node.local_name() == "svg")
            .then(|| self.node.attr(local_name!("preserveAspectRatio")))
            .flatten()
            .map(|v| v.trim().to_string());
        let (fit, align) = match par.as_deref() {
            Some(v) if v.starts_with("none") => (ObjectFit::Fill, None),
            Some(v) if !v.is_empty() => {
                let mut parts = v.split_whitespace();
                let a = parts.next().unwrap_or("xMidYMid");
                let at = |s: &str| if s.contains("Min") { 0.0 } else if s.contains("Max") { 1.0 } else { 0.5 };
                let (ax, ay) = a.split_at(a.len().min(4));
                let fit = if parts.next() == Some("slice") { ObjectFit::Cover } else { ObjectFit::Contain };
                (fit, Some((at(ax), at(ay))))
            }
            _ => (ObjectFit::Contain, None),
        };
        let paint_size = compute_object_fit(container_size, Some(object_size), fit);

        // Compute object-position
        let (x_offset, y_offset) = match align {
            Some((ax, ay)) => (
                ((container_size.width - paint_size.width) * ax) as f64,
                ((container_size.height - paint_size.height) * ay) as f64,
            ),
            None => (
                (object_position.horizontal.resolve(
                    CSSPixelLength::new(container_size.width - paint_size.width) / self.scale as f32,
                ) * self.scale as f32)
                    .px() as f64,
                (object_position.vertical.resolve(
                    CSSPixelLength::new(container_size.height - paint_size.height) / self.scale as f32,
                ) * self.scale as f32)
                    .px() as f64,
            ),
        };
        let x = x + x_offset;
        let y = y + y_offset;

        let x_scale = paint_size.width as f64 / object_size.width as f64;
        let y_scale = paint_size.height as f64 / object_size.height as f64;

        // local change: offset in the element's own space, like draw_image (upstream translated after the
        // element transform, so an ancestor scale() left the viewBox centring offset unscaled)
        let transform = self
            .transform
            .pre_translate(Vec2 { x, y })
            .pre_scale_non_uniform(x_scale, y_scale);

        // local change: rasterize the SVG with the installed SVG rasterizer (Skia under the vello
        // backend) over its device-space bounds, clipped to the frame, and composite it as an image
        if let Some(rasterize) = three_d::SVG_RASTERIZER.with(|r| r.get()) {
            let view = kurbo::Rect::new(0.0, 0.0, svg_size.width() as f64, svg_size.height() as f64);
            let frame = kurbo::Rect::new(0.0, 0.0, self.context.width as f64, self.context.height as f64);
            let origin = kurbo::Vec2::new(self.context.initial_x, self.context.initial_y);
            let device = (Affine::translate(-origin) * transform).transform_rect_bbox(view).inflate(2.0, 2.0).intersect(frame).expand();
            let (w, h) = (device.width() as u32, device.height() as u32);
            if w == 0 || h == 0 {
                return;
            }
            if w <= 8192 && h <= 8192 {
                let mut layer = anyrender::Scene::new();
                let to_layer = Affine::translate((-device.x0, -device.y0)) * Affine::translate(-origin) * transform;
                anyrender_svg::render_svg_tree(&mut layer, svg, to_layer);
                // an unchanged SVG (same commands, same place) reuses last frame's pixels, within a byte
                // budget per render thread (RAEA_SVG_CACHE_MB; 48 MB, 16 MB on iOS)
                let node_id = self.node.id;
                let cached = SVG_CACHE.with(|c| c.borrow_mut().get(node_id, &layer, (w, h)));
                let data = match cached {
                    Some(blob) => blob,
                    None => {
                        let blob = peniko::Blob::new(std::sync::Arc::new(rasterize(layer.clone(), w, h)));
                        SVG_CACHE.with(|c| c.borrow_mut().insert(node_id, layer, (w, h), blob.clone()));
                        blob
                    }
                };
                let brush = peniko::ImageBrush {
                    image: peniko::ImageData {
                        data,
                        format: peniko::ImageFormat::Rgba8,
                        width: w,
                        height: h,
                        alpha_type: peniko::ImageAlphaType::AlphaPremultiplied,
                    },
                    sampler: peniko::ImageSampler::default(),
                };
                scene.draw_image(brush.as_ref(), Affine::translate(origin + device.origin().to_vec2()));
                return;
            }
        }
        anyrender_svg::render_svg_tree(scene, svg, transform);
    }

    /// raea: an `<img>` showing a texture the renderer holds (a video frame on the GPU: `data-raea-texture` its id,
    /// `data-raea-w`/`-h` its size), placed by object-fit and object-position as a picture of that size would be, and
    /// drawn as an anyrender resource the renderer resolves. Nothing is decoded or uploaded here.
    fn draw_texture(&self, scene: &mut impl PaintScene) -> bool {
        let attr = |n: &str| self.element.attrs().iter().find(|a| &*a.name.local == n).map(|a| a.value.as_str());
        let Some(id) = attr("data-raea-texture").and_then(|v| v.parse::<u64>().ok()) else { return false };
        let (Some(tw), Some(th)) = (attr("data-raea-w").and_then(|v| v.parse::<f64>().ok()), attr("data-raea-h").and_then(|v| v.parse::<f64>().ok())) else { return false };
        if tw <= 0.0 || th <= 0.0 {
            return false;
        }
        let width = self.frame.content_box.width() as f32;
        let height = self.frame.content_box.height() as f32;
        let container_size = taffy::Size { width, height };
        let object_size = taffy::Size { width: tw as f32, height: th as f32 };
        let paint_size = compute_object_fit(container_size, Some(object_size), self.style.clone_object_fit());
        let object_position = self.style.clone_object_position();
        let x_offset = object_position.horizontal.resolve(CSSPixelLength::new(container_size.width - paint_size.width) / self.scale as f32) * self.scale as f32;
        let y_offset = object_position.vertical.resolve(CSSPixelLength::new(container_size.height - paint_size.height) / self.scale as f32) * self.scale as f32;
        let x = self.frame.content_box.origin().x + x_offset.px() as f64;
        let y = self.frame.content_box.origin().y + y_offset.px() as f64;
        let transform = self.transform.pre_translate(Vec2 { x, y }).pre_scale_non_uniform(paint_size.width as f64 / tw, paint_size.height as f64 / th);
        let quality = to_image_quality(self.style.clone_image_rendering());
        let brush = peniko::ImageBrush { image: anyrender::ResourceId::from_ffi(id), sampler: ImageSampler { quality, ..Default::default() } };
        // clipped to the box, as an image's paint is (object-fit: cover overflows it)
        let clip = Rect::new(self.frame.content_box.origin().x, self.frame.content_box.origin().y, self.frame.content_box.origin().x + width as f64, self.frame.content_box.origin().y + height as f64);
        scene.push_clip_layer(Fill::NonZero, self.transform, &clip);
        scene.fill(Fill::NonZero, transform, anyrender::Paint::Resource(brush), None, &Rect::new(0.0, 0.0, tw, th));
        scene.pop_layer();
        true
    }

    fn draw_image(&self, scene: &mut impl PaintScene) {
        if self.draw_texture(scene) {
            return;
        }
        if let Some(image) = self.element.raster_image_data() {
            let width = self.frame.content_box.width() as u32;
            let height = self.frame.content_box.height() as u32;
            let x = self.frame.content_box.origin().x;
            let y = self.frame.content_box.origin().y;

            let object_fit = self.style.clone_object_fit();
            let object_position = self.style.clone_object_position();
            let image_rendering = self.style.clone_image_rendering();
            let quality = to_image_quality(image_rendering);

            // Apply object-fit algorithm
            let container_size = taffy::Size {
                width: width as f32,
                height: height as f32,
            };
            let object_size = taffy::Size {
                width: image.width as f32,
                height: image.height as f32,
            };
            let paint_size = compute_object_fit(container_size, Some(object_size), object_fit);

            // Compute object-position
            let x_offset = object_position.horizontal.resolve(
                CSSPixelLength::new(container_size.width - paint_size.width) / self.scale as f32,
            ) * self.scale as f32;
            let y_offset = object_position.vertical.resolve(
                CSSPixelLength::new(container_size.height - paint_size.height) / self.scale as f32,
            ) * self.scale as f32;
            let x = x + x_offset.px() as f64;
            let y = y + y_offset.px() as f64;

            let x_scale = paint_size.width as f64 / object_size.width as f64;
            let y_scale = paint_size.height as f64 / object_size.height as f64;
            let transform = self
                .transform
                .pre_translate(Vec2 { x, y })
                .pre_scale_non_uniform(x_scale, y_scale);

            scene.draw_image(to_peniko_image(image, quality).as_ref(), transform);
        }
    }

    #[cfg(feature = "custom-widget")]
    fn draw_custom_widget(&self, scene: &mut impl PaintScene) {
        if let Some(widget_scene) = self.custom_widget_scene {
            let x = self.frame.content_box.origin().x;
            let y = self.frame.content_box.origin().y;
            let transform = self.transform.then_translate(Vec2 { x, y });

            // TODO: eliminate clone
            scene.append_scene(widget_scene.clone(), transform);
        }
    }

    fn draw_sub_document(&self, scene: &mut impl PaintScene) {
        if let Some(sub_doc) = self.element.sub_doc_data().map(|doc| doc.inner()) {
            let scale = self.scale;
            let width = self.frame.content_box.width() as u32;
            let height = self.frame.content_box.height() as u32;

            // TODO: Support arbitrary transforms of subdocuments
            let translation = self.transform.translation();
            let initial_x = translation.x + self.frame.content_box.origin().x;
            let initial_y = translation.y + self.frame.content_box.origin().y;
            // let transform = self.transform.then_translate(Vec2 { x, y });

            let painter = BlitzDomPainter::new(
                &sub_doc,
                scale,
                width,
                height,
                initial_x,
                initial_y,
                self.custom_widget_scenes,
            );
            painter.paint_scene(scene);
        }
    }

    fn stroke_devtools(&self, scene: &mut impl PaintScene) {
        if self.devtools.show_layout {
            let shape = &self.frame.border_box;
            let stroke = Stroke::new(self.scale);

            let stroke_color = match self.node.taffy_display() {
                taffy::Display::Block | taffy::Display::FlowRoot => {
                    Color::new([1.0, 0.0, 0.0, 1.0])
                }
                taffy::Display::Flex => Color::new([0.0, 1.0, 0.0, 1.0]),
                taffy::Display::Grid => Color::new([0.0, 0.0, 1.0, 1.0]),
                taffy::Display::None => Color::new([0.0, 0.0, 1.0, 1.0]),
            };

            scene.stroke(&stroke, self.transform, stroke_color, None, &shape);
        }
    }
}
impl<'dom, 'a> std::ops::Deref for ElementCx<'dom, 'a> {
    type Target = BlitzDomPainter<'dom, 'a>;
    fn deref(&self) -> &Self::Target {
        self.context
    }
}

fn insets_from_taffy_rect(input: taffy::Rect<f64>) -> Insets {
    Insets {
        x0: input.left,
        y0: input.top,
        x1: input.right,
        y1: input.bottom,
    }
}

/// Convert Stylo and Taffy types into Kurbo types
fn create_css_rect(style: &ComputedValues, layout: &Layout, scale: f64) -> CssBox {
    let border_box = Rect::new(0.0, 0.0, layout.size.width as f64 * scale, layout.size.height as f64 * scale);
    create_css_rect_at(style, layout, scale, border_box)
}

fn create_css_rect_at(style: &ComputedValues, layout: &Layout, scale: f64, border_box: Rect) -> CssBox {
    // Resolve and rescale
    // We have to scale since document pixels are not same same as rendered pixels
    let width: f64 = layout.size.width as f64;
    let height: f64 = layout.size.height as f64;
    let border = insets_from_taffy_rect(layout.border.map(|p| p as f64 * scale));
    let padding = insets_from_taffy_rect(layout.padding.map(|p| p as f64 * scale));
    let outline_width = style.get_outline().outline_width.0.to_f64_px() * scale;

    // Resolve the radii to a length. need to downscale since the radii are in document pixels
    let resolve_w = CSSPixelLength::new(width as _);
    let resolve_h = CSSPixelLength::new(height as _);
    let resolve_radii = |radius: &BorderCornerRadius| -> Vec2 {
        Vec2 {
            x: scale * radius.0.width.0.resolve(resolve_w).px() as f64,
            y: scale * radius.0.height.0.resolve(resolve_h).px() as f64,
        }
    };
    let s_border = style.get_border();
    let border_radii = NonUniformRoundedRectRadii {
        top_left: resolve_radii(&s_border.border_top_left_radius),
        top_right: resolve_radii(&s_border.border_top_right_radius),
        bottom_right: resolve_radii(&s_border.border_bottom_right_radius),
        bottom_left: resolve_radii(&s_border.border_bottom_left_radius),
    };

    CssBox::new(border_box, border, padding, outline_width, border_radii)
}

/// mix-blend-mode (local change): the layer blend for a computed `mix-blend-mode`, `None` for normal.
fn blend_mode(mode: style::computed_values::mix_blend_mode::T) -> Option<peniko::BlendMode> {
    use peniko::{BlendMode, Compose, Mix};
    use style::computed_values::mix_blend_mode::T as M;
    let mix = match mode {
        M::Normal => return None,
        M::PlusLighter => return Some(BlendMode::new(Mix::Normal, Compose::PlusLighter)),
        M::Multiply => Mix::Multiply,
        M::Screen => Mix::Screen,
        M::Overlay => Mix::Overlay,
        M::Darken => Mix::Darken,
        M::Lighten => Mix::Lighten,
        M::ColorDodge => Mix::ColorDodge,
        M::ColorBurn => Mix::ColorBurn,
        M::HardLight => Mix::HardLight,
        M::SoftLight => Mix::SoftLight,
        M::Difference => Mix::Difference,
        M::Exclusion => Mix::Exclusion,
        M::Hue => Mix::Hue,
        M::Saturation => Mix::Saturation,
        M::Color => Mix::Color,
        M::Luminosity => Mix::Luminosity,
    };
    Some(BlendMode::new(mix, Compose::SrcOver))
}
