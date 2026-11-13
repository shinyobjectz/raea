mod background;
mod bevy_debug;
mod border;
mod border_image;
mod bevy_shadow;
mod box_shadow;
mod clip_path;
mod form_controls;
mod mask;

use std::cell::{Cell, RefCell};
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
    ListItemLayout, ListItemLayoutPosition, NodeData, RasterImageData, TextInputData,
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
    /// raea: the transform the document is drawn under, and its inverse for culling in the document's own space: an
    /// <iframe>'s document takes its element's whole transform (a scale, a rotation, H5's 3D), not only its offset
    pub(crate) base: Affine,
    pub(crate) base_inv: Affine,
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
    /// Cached selection ranges for O(1) lookup: node_id -> (start_offset, end_offset)
    pub(crate) selection_ranges: HashMap<NodeId, (usize, usize)>,

    // Pre-computed `Scene`s for each CustomWidget
    pub(crate) custom_widget_scenes: &'a CustomWidgetSceneMap,
    /// raea: the 3D rendering context's root's parent's transform, which its elements are placed from (B3 H5): a
    /// homography (row-major 3x3), as inside a warped root it is the root's inverse on the way
    pub(crate) context_base: Cell<[f64; 9]>,
    /// raea: the element being drawn again inside its own warp layer, in its own space
    pub(crate) warping: Cell<Option<NodeId>>,
    /// raea: the 3D rendering context's root being drawn without its planes, which it then draws sorted
    pub(crate) rooting: Cell<Option<NodeId>>,
    /// raea: the plane of a 3D rendering context being drawn by its root's sorted pass
    pub(crate) plane: Cell<Option<NodeId>>,
    /// raea: the in-flow block being drawn in one of its parent's paint phases, not whole (RAEA_PAGE_PAINT_PHASES)
    pub(crate) split: Cell<Option<(NodeId, Phase)>>,
    /// raea: bevy_ui debug outlines recorded as their elements are drawn, drawn over the document (bevy_debug.rs)
    pub(crate) bevy_debug: RefCell<Vec<bevy_debug::DebugOutline>>,
}

/// raea: the steps of CSS 2.1 Appendix E an element paints its descendants in: the in-flow blocks' backgrounds and
/// borders, then the floats, then the in-flow content (inline content and inline-level boxes), then the positioned
/// ones. An in-flow block that clips nothing and is no stacking context of its own is drawn a phase at a time, so a
/// later block's background goes under an earlier block's text and inline-blocks; anything else is drawn whole, in its
/// phase. On unless RAEA_PAGE_PAINT_PHASES=0, where each element draws its children whole in tree order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Phase {
    Blocks,
    Floats,
    Inlines,
    Positioned,
}

pub(crate) fn paint_phases() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("RAEA_PAGE_PAINT_PHASES").as_deref() != Ok("0"))
}

/// raea: a cut plane's pieces drawn one after another are drawn as one clip (bsp::groups), and a piece reaches 1
/// device px past each cut it shares with a later piece of its plane (bsp::bled): no seam where two antialiased cuts
/// meet (preserve3d-and-filter-with-perspective). On by default; RAEA_PAGE_CUT_BLEED=0 turns it off.
pub(crate) fn cut_bleed() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("RAEA_PAGE_CUT_BLEED").as_deref() != Ok("0"))
}

/// raea: puts back the enclosing 3D rendering context's base when an element that starts one is done.
struct RestoreBase<'c>(&'c Cell<[f64; 9]>, [f64; 9]);

/// raea: 3x3 homographies (row-major, on x, y, w) for B3 H5's 3D transforms.
mod h3 {
    use kurbo::Affine;

    pub const IDENTITY: [f64; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

    pub fn of(a: Affine) -> [f64; 9] {
        let [a, b, c, d, e, f] = a.as_coeffs();
        [a, c, e, b, d, f, 0.0, 0.0, 1.0]
    }

    pub fn mul(a: &[f64; 9], b: &[f64; 9]) -> [f64; 9] {
        let mut m = [0.0; 9];
        for i in 0..3 {
            for j in 0..3 {
                m[i * 3 + j] = (0..3).map(|k| a[i * 3 + k] * b[k * 3 + j]).sum();
            }
        }
        m
    }

    pub fn inverse(m: &[f64; 9]) -> Option<[f64; 9]> {
        let [a, b, c, d, e, f, g, h, i] = *m;
        let (x, y, z) = (e * i - f * h, f * g - d * i, d * h - e * g);
        let det = a * x + b * y + c * z;
        if det.abs() < 1e-12 {
            return None;
        }
        let r = 1.0 / det;
        Some([x * r, (c * h - b * i) * r, (b * f - c * e) * r, y * r, (a * i - c * g) * r, (c * d - a * f) * r, z * r, (b * g - a * h) * r, (a * e - b * d) * r])
    }

    /// `m` as an affine, where it is one
    pub fn affine(m: &[f64; 9]) -> Option<Affine> {
        let w = m[8];
        (m[6].abs() < 1e-9 && m[7].abs() < 1e-9 && w > 0.0).then(|| Affine::new([m[0] / w, m[3] / w, m[1] / w, m[4] / w, m[2] / w, m[5] / w]))
    }
}

/// raea: a 3D rendering context's planes split where they cross and ordered back to front (css-transforms-2 §6's BSP
/// tree, as Chrome's), from their corners after the perspective divide (x, y in CSS px, z over w: the larger nearer,
/// the viewer along +z). Each piece is its plane's index and its polygon, `whole` when the plane wasn't cut.
mod bsp {
    pub type P = [f64; 3];

    pub struct Piece {
        pub plane: usize,
        pub poly: Vec<P>,
        /// the piece's clip: its plane's quad grown outward by a few px and cut where the piece is, so a cut piece's
        /// clip ends only at its cuts and the plane's own edges keep their antialiasing
        pub clip: Vec<P>,
        pub whole: bool,
    }

    struct Node {
        n: P,
        d: f64,
        on: Vec<Piece>,
        front: Option<Box<Node>>,
        back: Option<Box<Node>>,
    }

    /// how far from a plane a point still counts as on it: a share of how far out the polygons reach (z over w grows
    /// fast near the viewer, and with it the rounding of planes that are one plane by different matrices)
    const EPS_SHARE: f64 = 1e-6;

    /// the polygon's unit normal and its distance from the origin along it (Newell's), None when it has no area
    fn plane(poly: &[P]) -> Option<(P, f64)> {
        let mut n = [0.0; 3];
        let mut c = [0.0; 3];
        for (i, a) in poly.iter().enumerate() {
            let b = &poly[(i + 1) % poly.len()];
            n[0] += (a[1] - b[1]) * (a[2] + b[2]);
            n[1] += (a[2] - b[2]) * (a[0] + b[0]);
            n[2] += (a[0] - b[0]) * (a[1] + b[1]);
            for k in 0..3 {
                c[k] += a[k] / poly.len() as f64;
            }
        }
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        (len > 1e-9).then(|| {
            let n = [n[0] / len, n[1] / len, n[2] / len];
            (n, n[0] * c[0] + n[1] * c[1] + n[2] * c[2])
        })
    }

    fn build(mut pieces: Vec<Piece>) -> Option<Box<Node>> {
        let at = pieces.iter().position(|p| plane(&p.poly).is_some());
        let Some(at) = at else {
            // nothing with an area (planes edge-on): in paint order, on no plane
            return (!pieces.is_empty()).then(|| Box::new(Node { n: [0.0, 0.0, 1.0], d: f64::MAX, on: pieces, front: None, back: None }));
        };
        let first = pieces.remove(at);
        let (n, d) = plane(&first.poly).unwrap();
        let reach = pieces.iter().chain([&first]).flat_map(|p| p.poly.iter()).flat_map(|q| q.iter()).fold(1.0f64, |m, v| m.max(v.abs()));
        let eps = EPS_SHARE * reach * 1e3;
        let (mut on, mut front, mut back) = (vec![first], vec![], vec![]);
        for p in pieces {
            let s: Vec<f64> = p.poly.iter().map(|q| n[0] * q[0] + n[1] * q[1] + n[2] * q[2] - d).collect();
            if s.iter().all(|s| s.abs() <= eps) {
                on.push(p);
            } else if s.iter().all(|s| *s >= -eps) {
                front.push(p);
            } else if s.iter().all(|s| *s <= eps) {
                back.push(p);
            } else {
                // cut along the splitter's plane, the piece and its clip alike
                let (f, b) = cut(&p.poly, n, d, eps);
                let (cf, cb) = cut(&p.clip, n, d, eps);
                if f.len() >= 3 {
                    front.push(Piece { plane: p.plane, poly: f, clip: cf, whole: false });
                }
                if b.len() >= 3 {
                    back.push(Piece { plane: p.plane, poly: b, clip: cb, whole: false });
                }
            }
        }
        Some(Box::new(Node { n, d, on, front: build(front), back: build(back) }))
    }

    /// `poly` on each side of the plane (n, d) (Sutherland-Hodgman)
    fn cut(poly: &[P], n: P, d: f64, eps: f64) -> (Vec<P>, Vec<P>) {
        let s: Vec<f64> = poly.iter().map(|q| n[0] * q[0] + n[1] * q[1] + n[2] * q[2] - d).collect();
        let (mut f, mut b) = (vec![], vec![]);
        for i in 0..poly.len() {
            let j = (i + 1) % poly.len();
            let (qa, qb, sa, sb) = (poly[i], poly[j], s[i], s[j]);
            if sa >= -eps {
                f.push(qa);
            }
            if sa <= eps {
                b.push(qa);
            }
            if (sa > eps && sb < -eps) || (sa < -eps && sb > eps) {
                let t = sa / (sa - sb);
                let q = [qa[0] + (qb[0] - qa[0]) * t, qa[1] + (qb[1] - qa[1]) * t, qa[2] + (qb[2] - qa[2]) * t];
                f.push(q);
                b.push(q);
            }
        }
        (f, b)
    }

    /// the polygon grown by `by` px on screen: each corner moved away from the centre along the line to it, which
    /// keeps it in the polygon's plane
    fn grow(q: &[P], by: f64) -> Vec<P> {
        let c = [0, 1, 2].map(|k| q.iter().map(|v| v[k]).sum::<f64>() / q.len() as f64);
        q.iter()
            .map(|v| {
                let r = ((v[0] - c[0]).powi(2) + (v[1] - c[1]).powi(2)).sqrt();
                let t = if r > 1e-9 { 1.0 + by / r } else { 1.0 };
                [0, 1, 2].map(|k| c[k] + (v[k] - c[k]) * t)
            })
            .collect()
    }

    fn emit(node: Option<Box<Node>>, out: &mut Vec<Piece>) {
        let Some(node) = node else { return };
        let Node { n, on, front, back, .. } = *node;
        // the side the viewer is on last
        let (first, last) = if n[2] >= 0.0 { (back, front) } else { (front, back) };
        emit(first, out);
        out.extend(on);
        emit(last, out);
    }

    /// `quads`, in paint order, as the pieces to draw back to front
    pub fn order(quads: &[[P; 4]]) -> Vec<Piece> {
        let pieces = quads.iter().enumerate().map(|(i, q)| Piece { plane: i, poly: q.to_vec(), clip: grow(q, 4.0), whole: true }).collect();
        let mut out = vec![];
        emit(build(pieces), &mut out);
        out
    }

    /// how much of the screen the polygon covers, in px²
    fn screen_area(q: &[P]) -> f64 {
        (0..q.len()).map(|k| q[k][0] * q[(k + 1) % q.len()][1] - q[(k + 1) % q.len()][0] * q[k][1]).sum::<f64>().abs() / 2.0
    }

    /// The pieces as draw groups, in order. A cut plane's pieces drawn one after another, with only pieces that cover
    /// no pixel between them (edge-on planes, which draw nothing), are one group, drawn once clipped to their union,
    /// so the cuts between them are inside the clip and never antialiased; the pieces skipped over go first.
    pub fn groups(pieces: &[Piece]) -> Vec<Vec<usize>> {
        let mut used = vec![false; pieces.len()];
        let mut out = vec![];
        for i in 0..pieces.len() {
            if used[i] {
                continue;
            }
            used[i] = true;
            let mut group = vec![i];
            if !pieces[i].whole {
                for j in i + 1..pieces.len() {
                    if used[j] {
                        continue;
                    }
                    if pieces[j].plane == pieces[i].plane {
                        group.push(j);
                    } else if screen_area(&pieces[j].poly) < 0.25 {
                        out.push(vec![j]);
                    } else {
                        break;
                    }
                    used[j] = true;
                }
            }
            out.push(group);
        }
        out
    }

    /// `pieces[i]`'s clip reaching `by` px on screen past each cut edge it shares with a piece of the same plane in
    /// `later` (drawn after its group), None when it shares none. That piece's antialiased edge then lands on this
    /// one's reach, under everything drawn between them, so no seam lets the background through, as Chrome's
    /// unantialiased cuts leave none. An edge shared with a piece drawn before it stays put: a reach there would draw
    /// over the planes between.
    pub fn bled(pieces: &[Piece], i: usize, later: &[usize], by: f64) -> Option<Vec<P>> {
        let q = &pieces[i].clip;
        let edge = |q: &[P], k: usize| (q[k], q[(k + 1) % q.len()]);
        // how far a point is from the segment a-b on screen
        let off = |p: [f64; 2], a: P, b: P| {
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy).max(1e-18)).clamp(0.0, 1.0);
            ((p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2)).sqrt()
        };
        let later: Vec<&Piece> = later.iter().map(|&k| &pieces[k]).filter(|p| p.plane == pieces[i].plane).collect();
        let shift: Vec<f64> = (0..q.len())
            .map(|k| {
                let (a, b) = edge(q, k);
                let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
                let shared = later.iter().any(|p| (0..p.clip.len()).any(|j| {
                    let (c, d) = edge(&p.clip, j);
                    off(mid, c, d) < 1e-3
                }));
                if shared { by } else { 0.0 }
            })
            .collect();
        if shift.iter().all(|s| *s == 0.0) {
            return None;
        }
        let (n, d) = plane(q).filter(|(n, _)| n[2].abs() > 1e-9)?;
        let area: f64 = (0..q.len()).map(|k| q[k][0] * q[(k + 1) % q.len()][1] - q[(k + 1) % q.len()][0] * q[k][1]).sum();
        let side = if area >= 0.0 { 1.0 } else { -1.0 };
        // each edge's outward unit normal on screen
        let out: Vec<[f64; 2]> = (0..q.len())
            .map(|k| {
                let (a, b) = edge(q, k);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let len = (dx * dx + dy * dy).sqrt().max(1e-12);
                [side * dy / len, -side * dx / len]
            })
            .collect();
        // each corner where its two edges' lines meet once moved (m.x = sm, k.x = sk)
        Some(
            (0..q.len())
                .map(|k| {
                    let h = (k + q.len() - 1) % q.len();
                    let (m, kk, sm, sk) = (out[h], out[k], shift[h], shift[k]);
                    let det = m[0] * kk[1] - m[1] * kk[0];
                    let (ox, oy) = match det.abs() > 1e-6 {
                        true => ((sm * kk[1] - sk * m[1]) / det, (m[0] * sk - kk[0] * sm) / det),
                        false => (m[0] * sm.max(sk), m[1] * sm.max(sk)),
                    };
                    let (x, y) = (q[k][0] + ox, q[k][1] + oy);
                    [x, y, (d - n[0] * x - n[1] * y) / n[2]]
                })
                .collect(),
        )
    }
}

impl Drop for RestoreBase<'_> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
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
            base: Affine::IDENTITY,
            base_inv: Affine::IDENTITY,
            root_element_id,
            #[cfg(feature = "scrollbars")]
            hovered_scrollbar: dom.hovered_scrollbar(),
            #[cfg(feature = "scrollbars")]
            scrollbar_drag_target: dom.scrollbar_drag_target(),
            layer_manager,
            draw_text_context: RefCell::new(DrawTextContext::default()),
            selection_ranges,
            custom_widget_scenes,
            context_base: Cell::new(h3::IDENTITY),
            warping: Cell::new(None),
            rooting: Cell::new(None),
            plane: Cell::new(None),
            split: Cell::new(None),
            bevy_debug: RefCell::new(Vec::new()),
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
            scene.fill(Fill::NonZero, self.base, bg_color, None, &rect);
        }

        // The root clip rectangle is the viewport (in screen coordinates, with the
        // initial offset already subtracted). Elements outside of this are culled, and
        // scrollports narrow this rectangle further for their descendants.
        let viewport_clip_rect = Rect::new(0.0, 0.0, self.width as f64, self.height as f64);

        self.render_element(
            scene,
            root_id,
            self.base
                * Affine::translate(Vec2 {
                    x: self.initial_x - (viewport_scroll.x * self.scale),
                    y: self.initial_y - (viewport_scroll.y * self.scale),
                }),
            viewport_clip_rect,
        );
        bevy_debug::draw(scene, &self.bevy_debug.take());

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
    /// raea: the planes of the 3D rendering context `root` starts, under `node`: each element in it, through the ones
    /// that also preserve 3D, in paint order (B3 H5's sorting).
    fn planes(&self, node: NodeId, out: &mut Vec<(f64, NodeId)>) {
        let tree = self.dom.as_ref().tree();
        let Some(n) = tree.get(node) else { return };
        let mut kids: Vec<NodeId> = vec![];
        if let Some(sc) = &n.stacking_context {
            kids.extend(sc.neg_z_hoisted_children().map(|c| c.node_id));
        }
        if let Some(children) = &*n.paint_children.borrow() {
            kids.extend(children.iter().copied());
        }
        if let Some(sc) = &n.stacking_context {
            kids.extend(sc.pos_z_hoisted_children().map(|c| c.node_id));
        }
        for k in kids {
            let Some(p) = self.dom.as_ref().projections.get(&k).filter(|p| p.in_context) else { continue };
            out.push((p.depth, k));
            if tree[k].primary_styles().is_some_and(|s| blitz_dom::preserves_3d(&s)) {
                self.planes(k, out);
            }
        }
    }

    /// raea: a 3D rendering context's root drawn itself, with what's flat in it (its planes are drawn by the context):
    /// in its warp layer's space when it is warped, and as often as the BSP cut it
    fn render_root(&self, scene: &mut impl PaintScene, node_id: NodeId, parent_style_transform: Affine, clip_rect: Rect, warping: bool, warp: Option<[f64; 9]>) {
        if warping {
            self.warping.set(Some(node_id));
        }
        self.rooting.set(Some(node_id));
        let Some(g) = warp else {
            self.render_element(scene, node_id, parent_style_transform, clip_rect);
            return;
        };
        // a projective root: drawn in its own space into a layer warped into place, as any projective element is
        let node = &self.dom.as_ref().tree()[node_id];
        let size = node.final_layout().size;
        let (w, h) = (size.width as f64 * self.scale, size.height as f64 * self.scale);
        scene.push_warp_layer(g, node.scrollable_overflow().union(Rect::new(0.0, 0.0, w, h)));
        self.warping.set(Some(node_id));
        IN_WARP.with(|w| w.set(w.get() + 1));
        self.render_element(scene, node_id, parent_style_transform, Rect::new(-1e7, -1e7, 1e7, 1e7));
        IN_WARP.with(|w| w.set(w.get() - 1));
        scene.pop_layer();
    }

    /// raea: whether a plane draws anything of its own (a background, border, shadow or outline, or content that
    /// isn't a plane of the context): one that doesn't is left out of the BSP, as Chrome's has only layers that draw
    fn paints_itself(&self, node: NodeId) -> bool {
        let tree = self.dom.as_ref().tree();
        let n = &tree[node];
        let Some(s) = n.primary_styles() else { return false };
        let bg = s.get_background();
        let border = n.final_layout().border;
        bg.background_color.resolve_to_absolute(&s.clone_color()).as_srgb_color().components[3] > 0.0
            || bg.background_image.0.iter().any(|i| !matches!(i, style::values::computed::image::Image::None))
            || border.top + border.right + border.bottom + border.left > 0.0
            || !s.get_effects().box_shadow.0.is_empty()
            || s.get_outline().outline_width.0.to_f64_px() > 0.0
            || n.children.iter().any(|c| {
                let c = &tree[*c];
                match &c.data {
                    NodeData::Text(t) => !t.content.trim().is_empty(),
                    NodeData::Element(_) => !self.dom.as_ref().projections.get(&c.id).is_some_and(|p| p.in_context),
                    _ => false,
                }
            })
    }

    /// raea: the paint phase a child of an element belongs to (Appendix E, see [`Phase`]), and whether it's an in-flow
    /// block drawn a phase at a time: one in a block formatting context that clips nothing, isn't replaced or a form
    /// control and isn't a stacking context (which effects, clip-path and mask make) or part of a 3D rendering context
    fn phase_of(&self, child: &Node, in_flex_or_grid: bool) -> (Phase, bool) {
        let Some(style) = child.primary_styles() else {
            return (Phase::Inlines, false);
        };
        if style.clone_position() != style::computed_values::position::T::Static
            || child.is_stacking_context_root(in_flex_or_grid)
        {
            return (Phase::Positioned, false);
        }
        if style.clone_float() != style::values::computed::Float::None {
            return (Phase::Floats, false);
        }
        let display = style.clone_display();
        let box_style = style.get_box();
        let plain = !in_flex_or_grid
            && Some(child.id) != self.root_element_id
            && display.outside() == DisplayOutside::Block
            && matches!(display.inside(), DisplayInside::Flow | DisplayInside::FlowRoot)
            && matches!(box_style.overflow_x, Overflow::Visible)
            && matches!(box_style.overflow_y, Overflow::Visible)
            && !box_style.clone_contain().contains(Contain::PAINT)
            && !self.dom.as_ref().projections.contains_key(&child.id)
            && child
                .element_data()
                .is_none_or(|e| e.image_data().is_none() && e.sub_doc_data().is_none() && e.text_input_data().is_none())
            && !["input", "select", "button", "textarea"].contains(&&**child.local_name());
        (if plain { Phase::Blocks } else { Phase::Inlines }, plain)
    }

    /// raea: the boxes `node_id` and its descendants draw, in its own space: `scrollable_overflow`'s walk (blitz-dom's
    /// resolve_transforms), cut on each axis an element clips (both, under `contain: paint`) to that element's box
    /// grown by its overflow-clip-margin, as its clip cuts what is drawn (the root element clips nothing: `should_clip`).
    /// The overflow itself reaches through every clip, so a layer made from it is as large as content nobody sees: a
    /// filtered card around a small clipping box with a page of content in it would be drawn into a texture the size
    /// of the page. With `own_filter`, grown by the reach of the element's own filter, which draws past its boxes (a
    /// descendant's drop shadow).
    fn drawn_bounds(&self, node_id: NodeId, own_filter: bool) -> Rect {
        let dom = self.dom.as_ref();
        let Some(node) = dom.get_node(node_id) else { return Rect::ZERO };
        let size = node.final_layout().size;
        let own = Rect::new(0.0, 0.0, size.width as f64 * self.scale, size.height as f64 * self.scale);
        let styles = node.primary_styles();
        let (clips_x, clips_y, grow) = styles.as_ref().filter(|_| self.root_element_id != Some(node_id)).map_or((false, false, 0.0), |s| {
            let (b, paint) = (s.get_box(), contains_paint(s));
            (paint || !matches!(b.overflow_x, Overflow::Visible), paint || !matches!(b.overflow_y, Overflow::Visible), s.get_margin().overflow_clip_margin.offset.0.px() as f64 * self.scale.max(1.0))
        });
        let filtered = |r: Rect| match styles.as_ref().filter(|_| own_filter).and_then(|s| convert_filters(&s.get_effects().filter.0)) {
            Some(f) => {
                let reach = f.expansion_rect();
                Rect::new(r.x0 + reach.x0, r.y0 + reach.y0, r.x1 + reach.x1, r.y1 + reach.y1)
            }
            None => r,
        };
        let clip = own.inflate(grow.max(0.0), grow.max(0.0));
        if clips_x && clips_y {
            return filtered(clip);
        }
        let mut all = own;
        let in_flow = node.layout_children.borrow();
        let in_flow = in_flow.iter().flatten().copied().filter(|&c| dom.get_node(c).is_some_and(|c| !c.is_out_of_flow()));
        let pseudos = [node.before(), node.after()].into_iter().flatten().filter(|&c| dom.get_node(c).is_some_and(|c| !c.is_out_of_flow()));
        for child_id in in_flow.chain(node.hoisted_children.borrow().iter().copied()).chain(pseudos) {
            let Some(child) = dom.get_node(child_id) else { continue };
            let at = child.final_layout().location;
            let mut to_own = Affine::translate((at.x as f64 * self.scale, at.y as f64 * self.scale));
            if let Some(t) = child.transform().as_deref() {
                to_own *= *t;
            }
            all = all.union(to_own.transform_rect_bbox(self.drawn_bounds(child_id, true)));
        }
        if clips_x {
            (all.x0, all.x1) = (all.x0.max(clip.x0), all.x1.min(clip.x1));
        }
        if clips_y {
            (all.y0, all.y1) = (all.y0.max(clip.y0), all.y1.min(clip.y1));
        }
        filtered(all)
    }

    fn render_element(
        &self,
        scene: &mut impl PaintScene,
        node_id: NodeId,
        parent_style_transform: Affine,
        clip_rect: Rect,
    ) {
        let node = &self.dom.as_ref().tree()[node_id];
        let _painting = crate::Painting::enter(node_id);

        // raea: a plane of a 3D rendering context is drawn by its root, sorted by depth, not in paint order (and a
        // projective one again, inside its warp layer)
        if self.dom.as_ref().projections.get(&node_id).is_some_and(|p| p.in_context) && self.warping.get() != Some(node_id) {
            if self.plane.get() != Some(node_id) {
                return;
            }
            self.plane.set(None);
        }

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

        // raea: an element turned away from the viewer with `backface-visibility: hidden` isn't drawn (B3 H5): flat,
        // with what's flattened into it; in a 3D rendering context its children are planes of their own and still are
        let hidden_face = styles.get_box().backface_visibility == style::computed_values::backface_visibility::T::Hidden
            && self.dom.as_ref().projections.get(&node_id).is_some_and(|p| p.back_facing);

        let effects = styles.get_effects();
        let opacity = effects.opacity;
        if opacity == 0.0 {
            return;
        }
        let has_opacity = opacity < 1.0;
        // raea: mix-blend-mode and isolation: the element's group composited onto what's under it
        let blend = crate::blend::element_blend(&styles);

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
        let mut overflow = *node.scrollable_overflow();
        // raea: and what a stacking context paints of the out-of-flow boxes inside it whose containing blocks are
        // above it (#39), which its own overflow leaves to those containing blocks: not culled, and in its opacity layer
        if let Some(sc) = node.stacking_context.as_ref().filter(|sc| !sc.children.is_empty()) {
            let b = sc.hoisted_content_bbox(node.tree(), node_id, self.scale);
            let s = self.scale;
            overflow = overflow.union(Rect::new(b.left as f64 * s, b.top as f64 * s, b.right as f64 * s, b.bottom as f64 * s));
        }
        // raea: inside a 3D rendering context an element is placed from the context root's parent, not its own
        // parent (which may flatten to nothing), and an element that starts one keeps that space for its children
        // A projective one (a perspective's) is drawn in its own space into a layer the page warps into place.
        let projection = self.dom.as_ref().projections.get(&node_id);
        let placed = match projection {
            Some(p) if p.in_context => p.context.map(h3::of).or(p.h).map(|m| h3::mul(&self.context_base.get(), &m)),
            Some(p) => p.h.map(|h| h3::mul(&h3::of(parent_style_transform * Affine::translate(box_position)), &h)),
            None => None,
        };
        let warping = self.warping.get() == Some(node_id);
        // an element starting a 3D context: its elements are placed from its parent's space
        let starts = blitz_dom::preserves_3d(&styles) && !projection.is_some_and(|p| p.in_context);
        let rooting = starts && self.rooting.get() != Some(node_id);
        // a projective root's warp is its own drawing's only (render_root), not its context's: its planes are placed
        // from the page, not through the inverse of a warp that may be nearly singular (an element almost edge-on:
        // preserve3d-and-filter-with-perspective)
        let mut root_warp = None;
        let transform = match placed {
            _ if warping => {
                self.warping.set(None);
                Affine::IDENTITY
            }
            Some(g) => match h3::affine(&g) {
                Some(t) => t,
                None if rooting => {
                    root_warp = Some(g);
                    Affine::IDENTITY
                }
                None => {
                    let (w, h) = (size.width as f64 * self.scale, size.height as f64 * self.scale);
                    scene.push_warp_layer(g, overflow.union(Rect::new(0.0, 0.0, w, h)));
                    self.warping.set(Some(node_id));
                    // in its own space: nothing of it is culled by the page's clip
                    IN_WARP.with(|w| w.set(w.get() + 1));
                    self.render_element(scene, node_id, parent_style_transform, Rect::new(-1e7, -1e7, 1e7, 1e7));
                    IN_WARP.with(|w| w.set(w.get() - 1));
                    scene.pop_layer();
                    return;
                }
            },
            None => {
                parent_style_transform
                    * Affine::translate(box_position)
                    * node.transform().as_deref().copied().unwrap_or_default()
            }
        };
        if rooting {
            let parent = h3::of(parent_style_transform);
            let base = match placed.filter(|_| warping).and_then(|g| h3::inverse(&g)) {
                Some(inv) => h3::mul(&inv, &parent),
                None => parent,
            };
            let _base = RestoreBase(&self.context_base, self.context_base.replace(base));
            // itself and what's flat in it, then its planes back to front (the nearest last), each drawn from the
            // context's base: split where they cross (the BSP), or sorted by their centres' depth when one of them
            // reaches behind the viewer
            // the root is a plane of its context too when it draws (blitz-dom's root quad: css-transforms-2 §6), first in
            // paint order; else drawn first, under them
            let projections = &self.dom.as_ref().projections;
            let mut planes = vec![];
            match projections.get(&node_id).and_then(|p| p.quad.map(|_| p.depth)).filter(|_| self.paints_itself(node_id)) {
                Some(depth) => planes.push((depth, node_id)),
                None => self.render_root(scene, node_id, parent_style_transform, clip_rect, warping, root_warp),
            }
            self.planes(node_id, &mut planes);
            planes.retain(|(_, n)| self.paints_itself(*n));
            let quads: Option<Vec<[bsp::P; 4]>> = planes.iter().map(|(_, n)| projections.get(n).and_then(|p| p.quad)).collect();
            match quads {
                Some(quads) => {
                    let pieces = bsp::order(&quads);
                    let groups: Vec<Vec<usize>> = match cut_bleed() {
                        true => bsp::groups(&pieces),
                        false => (0..pieces.len()).map(|i| vec![i]).collect(),
                    };
                    for (g, group) in groups.iter().enumerate() {
                        let piece = &pieces[group[0]];
                        let n = planes[piece.plane].1;
                        let draw = |scene: &mut _| match n == node_id {
                            true => self.render_root(scene, node_id, parent_style_transform, clip_rect, warping, root_warp),
                            false => {
                                self.plane.set(Some(n));
                                self.render_node(scene, n, Affine::IDENTITY, clip_rect);
                            }
                        };
                        if piece.whole {
                            draw(scene);
                            continue;
                        }
                        // a piece of a cut plane (or its group's): the plane drawn within it, mapped as the plane is
                        let base = self.context_base.get();
                        let later: Vec<usize> = groups[g + 1..].iter().flatten().copied().collect();
                        let mut path = kurbo::BezPath::new();
                        for &k in group {
                            let bled = cut_bleed().then(|| bsp::bled(&pieces, k, &later, 1.0 / self.scale)).flatten();
                            let clip = bled.as_ref().unwrap_or(&pieces[k].clip);
                            for (j, q) in clip.iter().enumerate() {
                                let (x, y) = (q[0] * self.scale, q[1] * self.scale);
                                let w = base[6] * x + base[7] * y + base[8];
                                let p = Point::new((base[0] * x + base[1] * y + base[2]) / w, (base[3] * x + base[4] * y + base[5]) / w);
                                if j == 0 { path.move_to(p) } else { path.line_to(p) }
                            }
                            path.close_path();
                        }
                        scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &path);
                        draw(scene);
                        scene.pop_layer();
                    }
                }
                None => {
                    planes.sort_by(|a, b| a.0.total_cmp(&b.0));
                    for (_, n) in planes {
                        if n == node_id {
                            self.render_root(scene, node_id, parent_style_transform, clip_rect, warping, root_warp);
                            continue;
                        }
                        self.plane.set(Some(n));
                        self.render_node(scene, n, Affine::IDENTITY, clip_rect);
                    }
                }
            }
            return;
        }
        if starts {
            self.rooting.set(None);
        }

        let screen_transform = Affine::translate(Vec2 {
            x: -self.initial_x,
            y: -self.initial_y,
        }) * self.base_inv
            * transform;
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

        let mut cx = self.element_cx(node, *node.final_layout(), transform, custom_widget_scene);
        if let Some(o) = node.attr(blitz_dom::LocalName::from("data-raea-bevy-debug")).and_then(|a| bevy_debug::outline(a, &cx.frame, cx.transform, clip_rect, self.scale)) {
            self.bevy_debug.borrow_mut().push(o);
        }
        if hidden_face {
            if blitz_dom::preserves_3d(&styles) {
                cx.draw_children(scene, cx.transform, clip_rect);
            }
            return;
        }

        // raea: an in-flow block drawn in one of its parent's paint phases: its part of that phase, then its
        // children's (it clips nothing and has no effects, so no layers)
        if let Some((id, phase)) = self.split.get()
            && id == node_id
        {
            self.split.set(None);
            if phase == Phase::Blocks {
                cx.draw_outset_box_shadow(scene);
                cx.draw_background(scene);
                cx.draw_inset_box_shadow(scene);
                if !cx.draw_border_image(scene) {
                    cx.draw_border(scene);
                }
                cx.stroke_devtools(scene);
            }
            if phase == Phase::Inlines {
                cx.draw_inline_layout(scene, content_position);
                cx.draw_marker(scene, content_position);
            }
            cx.draw_phase(scene, cx.transform, clip_rect, phase);
            if phase == Phase::Inlines {
                cx.draw_outline(scene);
            }
            return;
        }

        // If this element clips its overflow it establishes a scrollport: narrow the clip
        // rectangle passed to descendants to the visible (clipped) region so that content
        // scrolled out of view is culled rather than drawn and clipped away. The box used
        // here matches the clip applied to the content below.
        // raea: overflow clips each axis on its own: `overflow-x: clip` beside a visible y clips only x (as Chrome
        // does, and bevy_ui's Overflow::clip_x), so the box of a lone clipping axis reaches far along the other
        let (clips_x, clips_y) = (!matches!(overflow_x, Overflow::Visible), !matches!(overflow_y, Overflow::Visible));
        let clip_margin = styles.get_margin().overflow_clip_margin.clone();
        let one_axis = clips_x != clips_y && !is_text_input && !is_image && !is_sub_doc && !contain_paint;
        let clip_shape = |b: crate::kurbo_css::BoxShape| match one_axis {
            true => {
                let (r, far) = (b.bounding_box(), 1e4);
                crate::kurbo_css::BoxShape::Rect(Rect::new(
                    if clips_x { r.x0 } else { -far },
                    if clips_y { r.y0 } else { -far },
                    if clips_x { r.x1 } else { far },
                    if clips_y { r.y1 } else { far },
                ))
            }
            false => b,
        };
        let child_clip_rect = if should_clip {
            let clip_box = if is_text_input {
                cx.frame.content_box_path()
            } else {
                clip_shape(overflow_clip_box(&cx.frame, &clip_margin))
            };
            clip_rect.intersect(screen_transform.transform_rect_bbox(clip_box.bounding_box()))
        } else {
            clip_rect
        };

        // Compute clip-path (if any) and wrap all rendering in a clip layer
        let clip_path_shape = cx.clip_path_shape();
        let has_clip_path = clip_path_shape.is_some();
        let clip_fill = clip_path_shape.as_ref().map_or(peniko::Fill::NonZero, |s| s.1);
        let default_clip = cx.frame.border_box_path();
        let mut clip_path_for_layer = clip_path_shape.map(|s| crate::kurbo_css::BoxShape::Path(s.0)).unwrap_or(default_clip);
        clip_path_for_layer.apply_affine(Affine::scale(self.scale));

        // CSS 2 `clip` property (absolutely positioned elements only). Applies to the whole of
        // the element's rendering (including outlines and shadows), so it is the outermost layer.
        let css_clip_rect = cx.css_clip_rect();
        if css_clip_rect.is_some_and(|rect| rect.is_zero_area()) {
            return;
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

        cx.draw_outset_box_shadow(scene);

        // clip-path clip ayer
        self.layer_manager.maybe_with_layer_filled(
            scene,
            has_clip_path,
            clip_fill,
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

                // raea: a list with url()s goes through usvg as an SVG's filters do, its region the filter element's
                // (in CSS px from the border box's corner); a url() to no <filter> leaves the element unfiltered
                let has_url = effects.filter.0.iter().any(|f| matches!(f, crate::filters::StyloFilter::Url(_)));
                #[cfg(not(feature = "svg"))]
                let url_filter: Option<(Arc<anyrender::filters::Filter>, Rect)> = None;
                #[cfg(feature = "svg")]
                let url_filter = has_url
                    .then(|| {
                        let b = cx.frame.border_box;
                        let (tree, ids) = blitz_dom::node::css_filter_tree(self.dom.as_ref(), &effects.filter.0, (b.width() / cx.scale) as f32, (b.height() / cx.scale) as f32)?;
                        let (f, r) = anyrender_svg::filter_graph(&tree, &ids)?;
                        Some((Arc::new(f), Rect::new(b.x0 + r.x0 * cx.scale, b.y0 + r.y0 * cx.scale, b.x0 + r.x1 * cx.scale, b.y0 + r.y1 * cx.scale)))
                    })
                    .flatten();
                let filter = match &url_filter {
                    Some((f, _)) => Some(f.clone()),
                    None if has_url => None,
                    None => convert_filters(&effects.filter.0).map(Arc::new),
                };
                // raea: a filter of opacity() functions alone is the group's alpha, as the `opacity` property is (Filter
                // Effects 1 §10.9 scales the result's alpha), so it's drawn as the layer's alpha, not as a filter: a
                // filtered element is an image the size of its box in Vello's atlas (8192 px a side), and twelve
                // full-frame `filter: opacity()` layers at 2160x3840 (a motion blur of <Freeze> samples) outgrew it,
                // so the frame was drawn without them (#96)
                let only_opacity = !has_url && !effects.filter.0.is_empty() && effects.filter.0.iter().all(|f| matches!(f, crate::filters::StyloFilter::Opacity(_)));
                let (filter, opacity) = if only_opacity {
                    (None, opacity * effects.filter.0.iter().map(|f| if let crate::filters::StyloFilter::Opacity(a) = f { a.0 } else { 1.0 }).product::<f32>())
                } else {
                    (filter, opacity)
                };
                let has_opacity = has_opacity || opacity < 1.0;
                // raea: backdrop-filter's url()s the same way, the backdrop clipped to the border box as its other
                // filters are; a url() to no <filter> leaves the backdrop unfiltered
                let has_backdrop_url = effects.backdrop_filter.0.iter().any(|f| matches!(f, crate::filters::StyloFilter::Url(_)));
                #[cfg(not(feature = "svg"))]
                let backdrop_url: Option<Arc<anyrender::filters::Filter>> = None;
                #[cfg(feature = "svg")]
                let backdrop_url = has_backdrop_url
                    .then(|| {
                        let b = cx.frame.border_box;
                        let (tree, ids) = blitz_dom::node::css_filter_tree(self.dom.as_ref(), &effects.backdrop_filter.0, (b.width() / cx.scale) as f32, (b.height() / cx.scale) as f32)?;
                        anyrender_svg::filter_graph(&tree, &ids).map(|(f, _)| Arc::new(f))
                    })
                    .flatten();
                let backdrop_filter = match backdrop_url {
                    Some(f) => Some(f),
                    None if has_backdrop_url => None,
                    None => convert_filters(&effects.backdrop_filter.0).map(Arc::new),
                };

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

                let mut effect_layer_clip = cx.frame.border_box_path().bounding_box();
                // raea: a filter's input is all the element draws, what its descendants draw outside its box
                // included, as Chrome filters it (a 0x0 wrapper's absolutely positioned children were cut off at its
                // box, a blur's reach of them left): its boxes up to the clips in their way (`drawn_bounds`), and the
                // same of each out-of-flow box its stacking context paints past its clip, from a containing block
                // above it (#39's `from_above`: the walk doesn't meet those), where hoisted_content_bbox places it.
                // The stacking context's other boxes are met by the walk, under the clips they're painted under
                if filter.is_some() {
                    effect_layer_clip = effect_layer_clip.union(self.drawn_bounds(node_id, false));
                    for hoisted in node.stacking_context.iter().flat_map(|sc| sc.children.iter()).filter(|c| cx.from_above(c.node_id)) {
                        let Some(child) = self.dom.as_ref().get_node(hoisted.node_id) else { continue };
                        let (p, at) = (hoisted.position(node.tree(), node_id), child.final_layout().location);
                        let mut to_own = Affine::translate(((p.x + at.x) as f64 * self.scale, (p.y + at.y) as f64 * self.scale));
                        if let Some(t) = child.transform().as_deref() {
                            to_own *= *t;
                        }
                        effect_layer_clip = effect_layer_clip.union(to_own.transform_rect_bbox(self.drawn_bounds(hoisted.node_id, true)));
                    }
                }
                effect_layer_clip.x0 += filter_expansion_area.x0;
                effect_layer_clip.y0 += filter_expansion_area.y0;
                effect_layer_clip.x1 += filter_expansion_area.x1;
                effect_layer_clip.y1 += filter_expansion_area.y1;
                if let Some((_, region)) = url_filter {
                    effect_layer_clip = region;
                }
                // raea: opacity alone clips nothing (its group is the element and all it draws, overflow and a child's
                // perspective included: transform3d-rotatex-perspective-003); only a filter's region is bounded. Its
                // layer is the element's overflow (its box and its descendants' transformed boxes) cut to what's in view
                // (`clip_rect`, in screen_transform's space, back in the box's): a layer the size of the view for each
                // element filled Vello's bump buffers on a page of many (see/people.tsx's ~100), which then drew nothing.
                // The overflow has a projected descendant's flattened box only, so with one inside it's all in view.
                if filter.is_none() && backdrop_filter.is_none() {
                    let view = match screen_transform.determinant().abs() > 1e-12 {
                        true => screen_transform.inverse().transform_rect_bbox(clip_rect),
                        false => clip_rect,
                    };
                    let dom = self.dom.as_ref();
                    let projected_inside = dom.projections.keys().any(|&p| {
                        let mut n = dom.get_node(p).and_then(|n| n.parent);
                        while let Some(id) = n {
                            if id == node_id {
                                return true;
                            }
                            n = dom.get_node(id).and_then(|n| n.parent);
                        }
                        false
                    });
                    effect_layer_clip = match projected_inside {
                        true => view,
                        false => overflow.union(effect_layer_clip).intersect(view),
                    };
                }

                // raea: the backdrop, filtered, in a layer of its own clipped to the border box's shape (its rounded
                // corners, as Chrome clips it), before the effect layer, whose clip is the box's bounds grown by the
                // filter's reach
                if backdrop_filter.is_some() {
                    self.layer_manager.maybe_with_layer(scene, true, 1.0, cx.transform, &cx.frame.border_box_path(), None, backdrop_filter.clone(), |_| {});
                }

                // Opacity/Filter layer if box has opacity or a filter.
                // Clipped to border-box as it needs to include the background and borders.
                self.layer_manager.maybe_with_blend_layer(
                    scene,
                    has_opacity || filter.is_some() || backdrop_filter.is_some() || blend.is_some(),
                    opacity,
                    blend,
                    cx.transform,
                    &effect_layer_clip,
                    // raea: a backdrop-filtered element is a backdrop root (Filter Effects 2): an empty filter has it
                    // painted apart, so a backdrop-filter inside sees only what it paints
                    filter.or_else(|| backdrop_filter.as_ref().map(|_| Arc::new(anyrender::filters::Filter::empty()))),
                    None,
                    |scene| {
                        // raea: the root's background is the canvas's, which the root's own transform doesn't move
                        // (Chrome; WPT transform-translate-background-002)
                        let own = node.transform().as_deref().copied().filter(|t| is_root_element && t.determinant() != 0.0);
                        match own {
                            Some(own) => self.element_cx(node, *node.final_layout(), cx.transform * own.inverse(), custom_widget_scene).draw_background(scene),
                            None => cx.draw_background(scene),
                        }
                        cx.draw_inset_box_shadow(scene);
                        cx.draw_table_row_backgrounds(scene);
                        cx.draw_table_borders(scene);
                        cx.draw_table_row_borders(scene);
                        if !cx.draw_border_image(scene) {
                            cx.draw_border(scene);
                        }
                        cx.stroke_devtools(scene);

                        // TODO: allow layers with opacity to be unclipped (overflow: visible)
                        let clip = if is_text_input {
                            &cx.frame.content_box_path()
                        } else {
                            &clip_shape(overflow_clip_box(&cx.frame, &clip_margin))
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
                                // raea: now that the background has been drawn, the contents scrolled, in the element's
                                // axes (#18), by the transform alone: content_position stays put, since inline text is
                                // placed by both and was scrolled twice (#46)
                                cx.transform = cx.transform.pre_translate(Vec2 {
                                    x: -node.scroll_offset().x * self.scale,
                                    y: -node.scroll_offset().y * self.scale,
                                });
                                cx.draw_image(scene);
                                #[cfg(feature = "svg")]
                                cx.draw_svg(scene);
                                #[cfg(feature = "custom-widget")]
                                cx.draw_custom_widget(scene);
                                cx.draw_sub_document(scene);
                                cx.draw_input(scene);
                                cx.draw_text_input_text(scene, content_position);
                                cx.draw_inline_layout(scene, content_position);
                                cx.draw_marker(scene, content_position);
                                cx.draw_children(scene, cx.transform, child_clip_rect);
                                cx.draw_select_chevron(scene);
                            },
                        );
                        // raea: the out-of-flow boxes it paints from containing blocks above it (#39), in its group
                        // but outside its overflow clip, which doesn't hold them (a clip layer is a layer of its own:
                        // a blended fixed box inside it blended with nothing)
                        let scrolled = unscrolled_transform.pre_translate(Vec2 {
                            x: -node.scroll_offset().x * self.scale,
                            y: -node.scroll_offset().y * self.scale,
                        });
                        cx.draw_from_above(scene, scrolled, clip_rect);

                        // Overlay scrollbars, drawn unscrolled above the
                        // clipped content.
                        #[cfg(feature = "scrollbars")]
                        {
                            cx.transform = unscrolled_transform;
                            cx.draw_scrollbars(scene);
                        }
                    },
                );

                // Apply the CSS `mask` (if any) to the content drawn above
                cx.transform = unscrolled_transform;
                cx.maybe_pop_css_mask_layer(scene, mask_layer_pushed);
            },
        );

        // raea: the outline over the element's border and content, as CSS paints it (a focused field's ring covers
        // its border; drawn first, the border hid the ring's outer half)
        cx.draw_outline(scene);

        self.layer_manager
            .maybe_pop_layer(scene, css_clip_layer_pushed);
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
        let frame = create_css_rect(&style, &layout, scale);

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
            svg: match element.image_data() {
                Some(blitz_dom::node::ImageData::Svg(svg)) => Some(svg),
                _ => None,
            },
            text_input: element.text_input_data(),
            list_item: element.list_item_data.as_deref(),
            devtools: self.dom.devtools(),
            custom_widget_scene,
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

/// The transform from an SVG's rendered tree to a viewport of the given size.
#[cfg(feature = "svg")]
fn svg_viewport_affine(svg: &blitz_dom::node::SvgImageData, width: f32, height: f32) -> Affine {
    let t = svg.viewport_transform(width, height);
    Affine::new([t.sx, t.ky, t.kx, t.sy, t.tx, t.ty].map(f64::from))
}

// raea: each raster image's halvings (2x2 box means), built once on first use and kept while the image lives. An image
// drawn at under half its size is sampled from the smallest level still at least its drawn size, so bilinear reads
// stay within 2x, as Chrome's mipmapped downscale; sampling the full image instead aliases (a 1280x720 still shown
// 180 px wide sparkles, docs/blitz-bevy.feature B5c)
thread_local! {
    static MIPS: RefCell<HashMap<u64, (peniko::WeakBlob<u8>, Option<Vec<RasterImageData>>)>> = RefCell::new(HashMap::new());
}

fn halve(src: &RasterImageData) -> RasterImageData {
    let (w, h) = ((src.width / 2).max(1), (src.height / 2).max(1));
    let (sw, sh, d) = (src.width as usize, src.height as usize, src.data.data());
    let mut out = vec![0u8; w as usize * h as usize * 4];
    for y in 0..h as usize {
        let (y0, y1) = ((2 * y).min(sh - 1), (2 * y + 1).min(sh - 1));
        for x in 0..w as usize {
            let (x0, x1) = ((2 * x).min(sw - 1), (2 * x + 1).min(sw - 1));
            for c in 0..4 {
                let s = d[(y0 * sw + x0) * 4 + c] as u32 + d[(y0 * sw + x1) * 4 + c] as u32 + d[(y1 * sw + x0) * 4 + c] as u32 + d[(y1 * sw + x1) * 4 + c] as u32;
                out[(y * w as usize + x) * 4 + c] = ((s + 2) / 4) as u8;
            }
        }
    }
    RasterImageData::new(w, h, Arc::new(out))
}

/// raea: an image's halvings handed over ready made (each half the last, the first half the image): a video's frame,
/// halved on its decoder's thread. Kept while the image lives, as the ones made here are.
pub fn preload_mips(image: &RasterImageData, levels: Vec<RasterImageData>) {
    MIPS.with_borrow_mut(|mips| {
        mips.retain(|_, (weak, _)| weak.upgrade().is_some());
        mips.insert(image.data.id(), (image.data.downgrade(), Some(levels)));
    });
}

/// The level of `image` to sample when it is drawn at `scale` of its size: the image itself, or a halving of it.
/// `make`: halvings not there yet are made now (an img's, once); without it only the ones handed over are used (a
/// video's frame is a new picture each time: nothing is halved for it on the page's thread).
fn mip_level(image: &RasterImageData, scale: f64, make: bool) -> RasterImageData {
    let mut want = 0;
    while scale * f64::from(1u32 << (want + 1)) <= 1.0 && image.width >> (want + 1) >= 1 && image.height >> (want + 1) >= 1 {
        want += 1;
    }
    if want == 0 {
        return image.clone();
    }
    MIPS.with_borrow_mut(|mips| {
        if !make {
            return match mips.get(&image.data.id()) {
                Some((_, Some(levels))) if !levels.is_empty() => levels[want.min(levels.len()) - 1].clone(),
                _ => image.clone(),
            };
        }
        mips.retain(|_, (weak, _)| weak.upgrade().is_some());
        // an all-zero image is a placeholder whose blob keys a GPU texture's override (raea-page's views, filters):
        // drawn as is, or the override is lost
        let (_, levels) = mips
            .entry(image.data.id())
            .or_insert_with(|| (image.data.downgrade(), image.data.data().iter().any(|&b| b != 0).then(Vec::new)));
        let Some(levels) = levels else { return image.clone() };
        while levels.len() < want {
            let next = halve(levels.last().unwrap_or(image));
            levels.push(next);
        }
        levels[want - 1].clone()
    })
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

/// A context of loaded and hot data to draw the element from
// raea: how many warped layers (a perspective's, B3 H5) the element being drawn is inside
thread_local! {
    pub(crate) static IN_WARP: Cell<u32> = const { Cell::new(0) };
}

struct ElementCx<'dom, 'a> {
    context: &'dom BlitzDomPainter<'dom, 'a>,
    frame: CssBox,
    style: style::servo_arc::Arc<ComputedValues>,
    scale: f64,
    node: &'dom Node,
    element: &'dom ElementData,
    transform: Affine,
    #[cfg(feature = "svg")]
    svg: Option<&'dom blitz_dom::node::SvgImageData>,
    text_input: Option<&'dom TextInputData>,
    list_item: Option<&'dom ListItemLayout>,
    devtools: &'dom DevtoolSettings,
    #[cfg_attr(not(feature = "custom-widget"), expect(unused))]
    custom_widget_scene: Option<&'a Scene>,
}

/// Converts parley BoundingBox into peniko Rect
fn convert_rect(rect: &parley::BoundingBox) -> kurbo::Rect {
    peniko::kurbo::Rect::new(rect.x0, rect.y0, rect.x1, rect.y1)
}

impl ElementCx<'_, '_> {
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

    /// Where an inline root's text is drawn from `pos` (its content box's place): the place, its block offset added,
    /// and the transform its glyphs are drawn with
    fn inline_text_at(&self, pos: Point, block_offset: f32) -> (Point, Affine) {
        // raea: the text at its box's place as laid out, not as rounded to whole pixels for its background (each
        // box's place here is its rounded one): Chrome snaps a box's background and border but draws its text from
        // the fractional place (a button at x 78.4 has its label at 86.4, not 86)
        let pos = if crate::text::chrome_glyphs().text_unrounded {
            let (mut dx, mut dy) = (0.0f32, 0.0f32);
            let mut current = Some(self.node.id);
            while let Some(id) = current {
                let Some(n) = self.context.dom.get_node(id) else { break };
                dx += n.unrounded_layout().location.x - n.final_layout().location.x;
                dy += n.unrounded_layout().location.y - n.final_layout().location.y;
                current = n.containing_block();
            }
            Point::new(pos.x + dx as f64, pos.y + dy as f64)
        } else {
            pos
        };
        let pos = Point {
            x: pos.x,
            y: pos.y + block_offset as f64,
        };
        let transform =
            self.transform * Affine::translate((pos.x * self.scale, pos.y * self.scale));
        (pos, transform)
    }

    /// raea: the element's text as a mask (background-clip: text, #37): its glyphs where they're drawn, opaque and
    /// without their shadows, for the background group under them to keep only what they cover (DestIn). Its own
    /// inline layout and, as Chrome's, the text of every element inside it at its place, unscrolled: Chrome 148 masks
    /// a scroller's background with its text as at scrollTop 0 (bench/background-clip-text/check.sh's scrolled case).
    pub(super) fn draw_text_mask(&self, scene: &mut impl PaintScene) {
        if let Some(text_layout) = self.element.inline_layout_data.as_ref().filter(|_| self.node.flags.is_inline_root()) {
            let l = self.node.final_layout();
            let at = Point::new((l.padding.left + l.border.left) as f64, (l.padding.top + l.border.top) as f64);
            let (_, transform) = self.inline_text_at(at, text_layout.block_offset);
            let mut cx = self.context.draw_text_context.borrow_mut();
            (cx.color, cx.mask) = (Some(peniko::color::palette::css::BLACK), true);
            crate::text::stroke_text(scene, text_layout.layout.lines(), self.context.dom, transform, self.scale, self.node.id, &mut cx);
            (cx.color, cx.mask) = (None, false);
        }
        let Some(children) = self.node.paint_children.borrow().clone() else { return };
        for id in children {
            let child = &self.context.dom.as_ref().tree()[id];
            if child.element_data().is_none() || child.primary_styles().is_none() || matches!(child.taffy_display(), taffy::Display::None) {
                continue;
            }
            let at = child.final_layout().location;
            let transform = self.transform * Affine::translate(Vec2::new(at.x as f64, at.y as f64) * self.scale) * child.transform().as_deref().copied().unwrap_or_default();
            self.context.element_cx(child, *child.final_layout(), transform, None).draw_text_mask(scene);
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

            let (pos, transform) = self.inline_text_at(pos, text_layout.block_offset);

            // Render inline element backgrounds (e.g. `<span style="background: ...">`)
            // behind the text and selection highlight.
            // raea: the fragment rects are in CSS px from the root's border box
            let layout = self.node.final_layout();
            let border_box = self.transform
                * Affine::translate((
                    (pos.x - (layout.padding.left + layout.border.left) as f64) * self.scale,
                    (pos.y - text_layout.block_offset as f64 - (layout.padding.top + layout.border.top) as f64) * self.scale,
                ))
                * Affine::scale(self.scale);
            crate::text::draw_inline_backgrounds(
                scene,
                text_layout.layout.lines(),
                self.context.dom,
                border_box,
                self.node.id,
            );

            // Render text selection highlight (if any) using cached selection ranges
            // raea: in the inline root's ::selection background, and its glyphs in the ::selection colour when that
            // isn't the text's own (as Chrome keeps the text's colours under its default highlight)
            let (sel_fill, sel_ink) = self.selection_colors();
            let selected = self.context.selection_ranges.get(&self.node.id).map(|&(sel_start, sel_end)| {
                crate::text::draw_text_selection(scene, &text_layout.layout, transform, sel_start, sel_end, sel_fill)
            });

            // Render text
            let mut draw_text_context = self.context.draw_text_context.borrow_mut();
            // raea: text-overflow: ellipsis on a box that clips its overflow (Blitz drew the text cut off)
            draw_text_context.ellipsis = self.node.primary_styles().and_then(|s| {
                let clips = !matches!(s.get_box().clone_overflow_x(), Overflow::Visible);
                let end = s.get_text().text_overflow.second.clone();
                (clips && matches!(end, style::values::specified::text::TextOverflowSide::Ellipsis))
                    .then(|| layout.content_box_width() * text_layout.layout.scale())
            });
            // raea: -webkit-line-clamp: its lines only, the last ending in an ellipsis (blitz-dom ends the box there)
            let keep = self.node.primary_styles().and_then(|s| blitz_dom::util::clamped_lines(&s)).filter(|&n| text_layout.layout.len() > n);
            draw_text_context.clamp = keep.map(|n| (n - 1, layout.content_box_width() * text_layout.layout.scale()));
            crate::text::stroke_text(
                scene,
                text_layout.layout.lines().take(keep.unwrap_or(usize::MAX)),
                self.context.dom,
                transform,
                self.scale,
                self.node.id,
                &mut draw_text_context,
            );
            if let (Some(path), Some(ink)) = (selected.filter(|p| !p.elements().is_empty()), sel_ink) {
                scene.push_clip_layer(Fill::NonZero, transform, &path);
                draw_text_context.color = Some(ink);
                let lines = text_layout.layout.lines().take(keep.unwrap_or(usize::MAX));
                crate::text::stroke_text(scene, lines, self.context.dom, transform, self.scale, self.node.id, &mut draw_text_context);
                draw_text_context.color = None;
                scene.pop_layer();
            }
            draw_text_context.ellipsis = None;
            draw_text_context.clamp = None;
            drop(draw_text_context);

            // raea: a contenteditable's caret (BaseDocument::caret), 1px wide in the caret colour, as a text input's
            if let Some((root, byte)) = self.context.dom.caret
                && root == self.node.id
            {
                let layout = &text_layout.layout;
                let cursor = parley::Cursor::from_byte_index(layout, byte.min(text_layout.text.len()), parley::Affinity::Downstream);
                let rect = cursor.geometry(layout, self.scale as f32);
                let color = self.style.get_inherited_text().color;
                let caret_color = match &self.style.get_inherited_ui().caret_color.0 {
                    ColorOrAuto::Auto => color,
                    ColorOrAuto::Color(c) => c.resolve_to_absolute(&color),
                };
                scene.fill(Fill::NonZero, transform, caret_color.as_srgb_color(), None, &convert_rect(&rect));
            }
        }
    }

    /// raea: the element's ::selection background (Blitz's default highlight while it's transparent) and its colour,
    /// None when that's the element's own colour (its text keeps its colours then, as under Chrome's default)
    fn selection_colors(&self) -> (Color, Option<Color>) {
        let data = self.node.try_stylo_element_data().and_then(|d| d.get());
        let Some(sel) = data.as_ref().and_then(|d| d.styles.pseudos.get(&style::selector_parser::PseudoElement::Selection)) else {
            return (SELECTION_COLOR, None);
        };
        let own = self.style.get_inherited_text().color;
        let ink = sel.get_inherited_text().color;
        let fill = sel.get_background().background_color.resolve_to_absolute(&ink);
        let fill = if fill.alpha == 0.0 { SELECTION_COLOR } else { fill.as_srgb_color() };
        (fill, (ink != own).then(|| ink.as_srgb_color()))
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
            // raea: an empty field shows its placeholder, in its ::placeholder colour
            let placeholder = input_data
                .placeholder
                .as_ref()
                .filter(|_| input_data.editor.raw_text().is_empty())
                .and_then(|p| p.try_layout());
            if placeholder.is_some() {
                draw_text_context.color = self.context.dom.placeholder_color(self.node.id).map(|c| c.as_srgb_color());
            }
            // (a password's bullets in place of its text)
            let text = input_data.mask.as_ref().and_then(|m| m.try_layout()).or_else(|| input_data.editor.try_layout()).unwrap();
            crate::text::stroke_text(
                scene,
                placeholder.unwrap_or(text).lines(),
                self.context.dom,
                transform,
                self.scale,
                self.node.id,
                &mut draw_text_context,
            );
            draw_text_context.color = None;
            drop(draw_text_context);
            self.draw_spin_buttons(scene);
        }
    }

    /// raea: a number field's spin buttons while it's hovered or focused, as Chrome on macOS draws them (bevy-tsx's
    /// controls.rs): a 15x16 box at the content box's right, centred, an arrow up and one down in rows 1, 3, 5, 7
    /// pixels wide, unsmoothed
    fn draw_spin_buttons(&self, scene: &mut impl PaintScene) {
        let number = self.element.attr(local_name!("type")).is_some_and(|t| t.eq_ignore_ascii_case("number"));
        if !number || !(self.node.is_hovered() || self.node.is_focussed()) {
            return;
        }
        let s = self.scale;
        let c = self.frame.content_box;
        let (x0, y0) = (c.x1 - 15.0 * s, ((c.y0 + c.y1) / 2.0 - 8.0 * s).round());
        let bg = peniko::Color::from_rgb8(241, 241, 241);
        let ink = peniko::Color::from_rgb8(80, 80, 80);
        scene.fill(Fill::NonZero, self.transform, bg, None, &Rect::new(x0, y0, x0 + 15.0 * s, y0 + 16.0 * s));
        for r in 0..4 {
            let (l, w) = (x0 + (4.0 + 3.0 - r as f64) * s, (2 * r + 1) as f64 * s);
            for top in [3.0 + r as f64, 10.0 + (3 - r) as f64] {
                let y = y0 + top * s;
                scene.fill(Fill::NonZero, self.transform, ink, None, &Rect::new(l, y, l + w, y + s));
            }
        }
    }

    /// raea: a closed select's chevron, as Chrome on macOS draws it (bevy-tsx's controls.rs): a 10x6 vee stroked
    /// 1.8px in the select's text colour, 4px in from its right, centred
    fn draw_select_chevron(&self, scene: &mut impl PaintScene) {
        if self.element.name.local != local_name!("select") || self.element.attr(local_name!("multiple")).is_some() {
            return;
        }
        let s = self.scale;
        let p = self.frame.padding_box;
        let (x0, y0) = (p.x1 - 14.0 * s, (p.y0 + p.y1) / 2.0 - 3.0 * s);
        let at = |x: f64, y: f64| kurbo::Point::new(x0 + x * 10.0 * s, y0 + y * 6.0 * s);
        let mut path = kurbo::BezPath::new();
        path.move_to(at(0.1, 0.17));
        path.line_to(at(0.5, 0.83));
        path.line_to(at(0.9, 0.17));
        let ink = self.style.clone_color().as_srgb_color();
        let stroke = kurbo::Stroke::new(1.8 * s).with_caps(kurbo::Cap::Round).with_join(kurbo::Join::Round);
        scene.stroke(&stroke, self.transform, ink, None, &path);
    }

    fn draw_marker(&self, scene: &mut impl PaintScene, pos: Point) {
        if let Some(ListItemLayout {
            position: ListItemLayoutPosition::Outside(layout),
            ..
        }) = self.list_item
        {
            // Right align the bullet when rendering outside (raea: with no pad: a char marker carries its space, as a
            // string marker does, blitz-dom layout/list.rs)
            // Outside markers are placed outside the list item's border box
            // (`pos` is the origin of its content box)
            let item_layout = self.node.final_layout();
            let x_offset = -(layout.full_width() / layout.scale()
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
            // raea: in the ::marker colour (the item's text colour when no rule sets one)
            draw_text_context.color = self.context.dom.marker_style(self.node.id).map(|m| m.get_inherited_text().color.as_srgb_color());
            crate::text::stroke_text(
                scene,
                layout.lines(),
                self.context.dom,
                transform,
                self.scale,
                self.node.id,
                &mut draw_text_context,
            );
            draw_text_context.color = None;
        }
    }

    /// Fixed-position children of the root element are positioned against
    /// the viewport and do not scroll with it, so cancel out the viewport
    /// scroll (applied in `paint_scene`). A fixed box contained by a
    /// transformed ancestor scrolls with it like any out-of-flow box.
    fn child_transform(&self, child_id: NodeId, parent_style_transform: Affine) -> Affine {
        let child = &self.context.dom.as_ref().tree()[child_id];
        if child.taffy_position() == taffy::Position::Fixed
            && Some(self.node.id) == self.context.root_element_id
        {
            let scroll = self.context.dom.as_ref().viewport_scroll();
            parent_style_transform.pre_translate(kurbo::Vec2 {
                x: scroll.x * self.scale,
                y: scroll.y * self.scale,
            })
        } else {
            parent_style_transform
        }
    }

    /// raea: the children's part of one paint phase (Appendix E, see [`Phase`]): an in-flow block that can be split
    /// draws its part and its children's, anything else is drawn whole in the phase it belongs to
    fn draw_phase(&self, scene: &mut impl PaintScene, parent_style_transform: Affine, clip_rect: Rect, phase: Phase) {
        let Some(children) = &*self.node.paint_children.borrow() else {
            return;
        };
        let in_flex_or_grid = self
            .node
            .primary_styles()
            .is_some_and(|s| matches!(s.clone_display().inside(), DisplayInside::Flex | DisplayInside::Grid));
        for &child_id in children {
            let child = &self.context.dom.as_ref().tree()[child_id];
            let (own, split) = self.context.phase_of(child, in_flex_or_grid);
            if !split && own != phase {
                continue;
            }
            let child_transform = self.child_transform(child_id, parent_style_transform);
            if split {
                self.context.split.set(Some((child_id, phase)));
            }
            self.render_node(scene, child_id, child_transform, clip_rect);
            self.context.split.set(None);
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
            for hoisted_child in hoisted.neg_z_hoisted_children().filter(|c| !self.from_above(c.node_id)) {
                let position = hoisted_child.position(self.node.tree(), self.node.id);
                let pos = kurbo::Vec2 {
                    x: position.x as f64 * self.scale,
                    y: position.y as f64 * self.scale,
                };
                let clips = self.hoisted_clips(scene, hoisted_child.node_id, parent_style_transform);
                self.render_node(
                    scene,
                    hoisted_child.node_id,
                    parent_style_transform.pre_translate(pos),
                    clip_rect,
                );
                (0..clips).for_each(|_| scene.pop_layer());
            }
        }

        // Regular children
        if paint_phases() {
            for phase in [Phase::Blocks, Phase::Floats, Phase::Inlines, Phase::Positioned] {
                self.draw_phase(scene, parent_style_transform, clip_rect, phase);
            }
        } else if let Some(children) = &*self.node.paint_children.borrow() {
            for child_id in children {
                let child_transform = self.child_transform(*child_id, parent_style_transform);
                self.render_node(scene, *child_id, child_transform, clip_rect);
            }
        }

        // Positive z_index hoisted nodes
        if let Some(hoisted) = &self.node.stacking_context {
            for hoisted_child in hoisted.pos_z_hoisted_children().filter(|c| !self.from_above(c.node_id)) {
                let position = hoisted_child.position(self.node.tree(), self.node.id);
                let pos = kurbo::Vec2 {
                    x: position.x as f64 * self.scale,
                    y: position.y as f64 * self.scale,
                };
                let clips = self.hoisted_clips(scene, hoisted_child.node_id, parent_style_transform);
                self.render_node(
                    scene,
                    hoisted_child.node_id,
                    parent_style_transform.pre_translate(pos),
                    clip_rect,
                );
                (0..clips).for_each(|_| scene.pop_layer());
            }
        }
    }

    /// raea: whether a hoisted child is an out-of-flow box this stacking context paints though its containing block
    /// is above it (#39): the root isn't on its chain of containing blocks
    fn from_above(&self, child: NodeId) -> bool {
        let tree = self.node.tree();
        let mut chain = tree.get(child).and_then(|n| n.containing_block());
        while let Some(id) = chain.filter(|id| *id != self.node.id) {
            chain = tree.get(id).and_then(|n| n.containing_block());
        }
        chain.is_none()
    }

    /// raea: the out-of-flow boxes this stacking context paints from containing blocks above it (#39), by z, after
    /// its content: in its group, outside its overflow clip, and under none of the clips between (the boxes they're
    /// in are the root's too, whose clips are already pushed)
    fn draw_from_above(&self, scene: &mut impl PaintScene, transform: Affine, clip_rect: Rect) {
        let Some(sc) = &self.node.stacking_context else { return };
        for c in sc.neg_z_hoisted_children().chain(sc.pos_z_hoisted_children()).filter(|c| self.from_above(c.node_id)) {
            let p = c.position(self.node.tree(), self.node.id);
            let pos = Vec2 { x: p.x as f64 * self.scale, y: p.y as f64 * self.scale };
            self.render_node(scene, c.node_id, transform.pre_translate(pos), clip_rect);
        }
    }

    /// raea: a hoisted child (a z-index's, painted from its stacking-context root) under the overflow clips of the
    /// boxes between it and that root along its containing blocks, as Chrome clips it: each one's padding box, from
    /// the root, pushed as a clip layer. Returns how many were pushed. Drawn from the root alone it painted past them
    /// (Excalidraw's frame names, `z-index: 2` in its `overflow: hidden` editor, over the page around it).
    fn hoisted_clips(&self, scene: &mut impl PaintScene, child: NodeId, transform: Affine) -> usize {
        let tree = self.node.tree();
        let root = self.node.id;
        let mut pushed = 0;
        let mut current = tree[child].containing_block();
        while let Some(id) = current.filter(|id| *id != root) {
            let Some(node) = tree.get(id) else { break };
            current = node.containing_block();
            let Some(styles) = node.primary_styles() else { continue };
            let (x, y) = (styles.get_box().overflow_x, styles.get_box().overflow_y);
            let (cx, cy) = (!matches!(x, Overflow::Visible), !matches!(y, Overflow::Visible));
            if !cx && !cy {
                continue;
            }
            // its border box from the root: its place and those of the boxes it's in, less what they're scrolled by
            let layout = node.final_layout();
            let mut at = Vec2::new(layout.location.x as f64, layout.location.y as f64);
            let mut up = node.containing_block();
            while let Some(a) = up.filter(|a| *a != root) {
                let Some(an) = tree.get(a) else { break };
                let (l, sc) = (an.final_layout().location, *an.scroll_offset());
                at += Vec2::new(l.x as f64 - sc.x, l.y as f64 - sc.y);
                up = an.containing_block();
            }
            let b = layout.border;
            let far = 1e7;
            let pad = Rect::new(
                if cx { at.x + b.left as f64 } else { -far },
                if cy { at.y + b.top as f64 } else { -far },
                if cx { at.x + (layout.size.width - b.right) as f64 } else { far },
                if cy { at.y + (layout.size.height - b.bottom) as f64 } else { far },
            );
            scene.push_clip_layer(Fill::NonZero, transform, &pad.scale_from_origin(self.scale));
            pushed += 1;
        }
        pushed
    }

    #[cfg(feature = "svg")]
    fn draw_svg(&self, scene: &mut impl PaintScene) {
        use style::properties::generated::longhands::object_fit::computed_value::T as ObjectFit;

        let Some(svg) = self.svg else {
            return;
        };

        let width = self.frame.content_box.width() as f32;
        let height = self.frame.content_box.height() as f32;
        // raea: the tree parsed at its box's CSS size when the box is its viewport (SvgImageData::tree_at)
        let tree = svg.tree_at(width / self.scale as f32, height / self.scale as f32);
        let svg_size = tree.size();

        let x = self.frame.content_box.origin().x;
        let y = self.frame.content_box.origin().y;

        // An SVG with an aspect ratio but no natural size is sized by the default sizing
        // algorithm, which for `none` and `scale-down` contains it within the content box.
        let object_fit = match self.style.clone_object_fit() {
            ObjectFit::None | ObjectFit::ScaleDown
                if svg.intrinsic_width().is_none()
                    && svg.intrinsic_height().is_none()
                    && svg.viewbox_aspect_ratio().is_some() =>
            {
                ObjectFit::Contain
            }
            object_fit => object_fit,
        };
        let object_position = self.style.clone_object_position();

        // Apply object-fit algorithm
        let container_size = taffy::Size { width, height };
        let object_size = taffy::Size {
            width: svg_size.width(),
            height: svg_size.height(),
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

        // raea: without a viewBox there is no scaling: a user unit is a CSS px at any box size, as Chrome draws an
        // inline <svg> sized by CSS (blitz stretched its content to the box); `tree_at` has it at the box's size. An
        // SVG image (<img>) with a natural size (absolute width and height) is still scaled from it to its object-fit
        // rect, as Chrome draws one: a 1280 x 720 drawing in a 180 x 101 cover box is shrunk, not cropped to a corner
        let inline = &*self.element.name.local == "svg";
        let natural = svg.intrinsic_width().is_some() && svg.intrinsic_height().is_some();
        let viewport = match svg.intrinsic_dimensions.view_box {
            None if inline || !natural => Affine::scale(self.scale),
            _ => svg_viewport_affine(svg, paint_size.width, paint_size.height),
        };
        // raea: the content box's place in the element's axes (#18): before its transform, not after
        let transform = self.transform.pre_translate(Vec2 { x, y }) * viewport;
        // raea: the same, split where a non-scaling stroke is drawn (anyrender_svg render_svg_tree_in): the page's
        // place and device scale, then the viewBox's mapping in CSS px
        let host = self.transform.pre_translate(Vec2 { x, y }) * Affine::scale(self.scale);
        let in_css = Affine::scale(1.0 / self.scale) * viewport;
        // raea: its text grown as the page's text is (text.rs svg_text_look)
        anyrender_svg::set_text_look(crate::text::svg_text_look);

        // raea: a drawing that reaches outside its viewport (a viewBox's slice, shapes past the box) is clipped to
        // it, as Chrome's `svg:not(:root) { overflow: hidden }` does; blitz drew it whole
        let b = tree.root().abs_stroke_bounding_box();
        let (sw, sh) = (svg_size.width(), svg_size.height());
        let spills = b.left() < -0.01 || b.top() < -0.01 || b.right() > sw + 0.01 || b.bottom() > sh + 0.01;
        if spills {
            self.context.layer_manager.maybe_with_layer(scene, true, 1.0, self.transform, &self.frame.content_box_path(), None, None, |scene| {
                anyrender_svg::render_svg_tree_in(scene, &tree, transform, host, in_css);
            });
            return;
        }
        self.with_replaced_content_clip(
            scene,
            container_size,
            paint_size,
            x_offset.px(),
            y_offset.px(),
            |scene| {
                anyrender_svg::render_svg_tree_in(scene, &tree, transform, host, in_css);
            },
        );
    }

    /// Clip replaced content to the content box when its object-fit/object-position
    /// rect overflows it.
    fn with_replaced_content_clip<S: PaintScene>(
        &self,
        scene: &mut S,
        container_size: taffy::Size<f32>,
        paint_size: taffy::Size<f32>,
        x_offset: f32,
        y_offset: f32,
        paint: impl FnOnce(&mut S),
    ) {
        let overflows = x_offset < 0.0
            || y_offset < 0.0
            || x_offset + paint_size.width > container_size.width
            || y_offset + paint_size.height > container_size.height;
        self.context.layer_manager.maybe_with_layer(
            scene,
            overflows,
            1.0,
            self.transform,
            &self.frame.content_box_path(),
            None,
            None,
            paint,
        );
    }

    fn draw_image(&self, scene: &mut impl PaintScene) {
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
            let mut x = x + x_offset.px() as f64;
            let mut y = y + y_offset.px() as f64;

            let mut x_scale = paint_size.width as f64 / object_size.width as f64;
            let mut y_scale = paint_size.height as f64 / object_size.height as f64;
            // raea: a video's picture lands on whole device pixels, as Chrome composites a video's quad (its object-fit
            // rect pixel-snapped): a contain box's half-pixel offset otherwise shifts where each pixel samples the frame
            // and softens its edges (p15-media/img-downscale). Only under an axis-aligned transform; an img isn't snapped
            if self.element.name.local == local_name!("video") {
                let [a, b, c, d, e, f] = self.transform.as_coeffs();
                if b == 0.0 && c == 0.0 && a > 0.0 && d > 0.0 {
                    let snap = |p: f64, len: f64, s: f64, t: f64| {
                        let (p0, p1) = ((s * p + t).round(), (s * (p + len) + t).round());
                        ((p0 - t) / s, (p1 - p0) / s)
                    };
                    let (sx, sw) = snap(x, paint_size.width as f64, a, e);
                    let (sy, sh) = snap(y, paint_size.height as f64, d, f);
                    if sw > 0.0 && sh > 0.0 {
                        (x, y) = (sx, sy);
                        x_scale = sw / object_size.width as f64;
                        y_scale = sh / object_size.height as f64;
                    }
                }
            }
            // raea: a minified <img> is drawn from its halving nearest its drawn size (mip_level); a <video>'s frames
            // are each a new picture, so one is drawn from the halvings it came with (preload_mips), as it comes
            // when it came with none
            let outer = self.transform.determinant().abs().sqrt();
            let img = self.element.name.local == local_name!("img");
            let mip = quality != peniko::ImageQuality::Low && (img || self.element.name.local == local_name!("video"));
            let level = if mip { mip_level(image, x_scale.min(y_scale) * outer, img) } else { image.clone() };
            let (lx, ly) = (image.width as f64 / level.width as f64, image.height as f64 / level.height as f64);
            let image = &level;
            let transform = self
                .transform
                .pre_translate(Vec2 { x, y })
                .pre_scale_non_uniform(x_scale * lx, y_scale * ly);

            self.with_replaced_content_clip(
                scene,
                container_size,
                paint_size,
                x_offset.px(),
                y_offset.px(),
                |scene| {
                    scene.draw_image(to_peniko_image(image, quality).as_ref(), transform);
                },
            );
        }
    }

    #[cfg(feature = "custom-widget")]
    fn draw_custom_widget(&self, scene: &mut impl PaintScene) {
        if let Some(widget_scene) = self.custom_widget_scene {
            let x = self.frame.content_box.origin().x;
            let y = self.frame.content_box.origin().y;
            let transform = self.transform.pre_translate(Vec2 { x, y }); // raea: in the element's axes (#18)

            // TODO: eliminate clone
            scene.append_scene(widget_scene.clone(), transform);
        }
    }

    fn draw_sub_document(&self, scene: &mut impl PaintScene) {
        if let Some(sub_doc) = self.element.sub_doc_data().map(|doc| doc.inner()) {
            let scale = self.scale;
            let width = self.frame.content_box.width() as u32;
            let height = self.frame.content_box.height() as u32;

            // raea: the document under the element's whole transform, clipped to its content box (an <iframe>
            // scaled by its parent draws its document scaled; upstream took the offset only)
            let origin = self.frame.content_box.origin();
            let base = self.transform * Affine::translate(Vec2 { x: origin.x, y: origin.y });
            let mut painter = BlitzDomPainter::new(&sub_doc, scale, width, height, 0.0, 0.0, self.custom_widget_scenes);
            painter.base = base;
            painter.base_inv = base.inverse();
            scene.push_clip_layer(Fill::NonZero, base, &Rect::new(0.0, 0.0, width as f64, height as f64));
            painter.paint_scene(scene);
            scene.pop_layer();
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
    // Resolve and rescale
    // We have to scale since document pixels are not same same as rendered pixels
    let width: f64 = layout.size.width as f64;
    let height: f64 = layout.size.height as f64;
    let border_box = Rect::new(0.0, 0.0, width * scale, height * scale);
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

    // raea: outline-offset, which the outline is drawn out by
    let mut frame = CssBox::new(border_box, border, padding, outline_width, border_radii);
    frame.outline_offset = style.get_outline().outline_offset.to_f64_px() * scale;
    frame
}

/// raea: whether `contain: paint` clips an element's contents, as render_element's `contain_paint` has it (not a
/// non-atomic inline's, nor an internal table box's other than a cell)
fn contains_paint(styles: &style::properties::ComputedValues) -> bool {
    let display = styles.clone_display();
    let table_part = display.outside() == DisplayOutside::InternalTable && display.inside() != DisplayInside::TableCell;
    styles.get_box().clone_contain().contains(Contain::PAINT) && !display.is_inline_flow() && !table_part
}

/// raea: what an overflow clip clips to: overflow-clip-margin's visual box (the padding box by default), grown by its
/// offset (bevy_ui's OverflowClipMargin, ComputedNode::resolve_clip_rect). A grown one is a rect, as Bevy's clip is.
fn overflow_clip_box(frame: &CssBox, m: &style::values::computed::OverflowClipMargin) -> crate::kurbo_css::BoxShape {
    use style::values::generics::box_::OverflowClipMarginBox as B;
    let b = match m.visual_box {
        B::BorderBox => frame.border_box_path(),
        B::PaddingBox => frame.padding_box_path(),
        B::ContentBox => frame.content_box_path(),
    };
    let grow = m.offset.0.px() as f64;
    match grow > 0.0 {
        true => crate::kurbo_css::BoxShape::Rect(b.bounding_box().inflate(grow, grow)),
        false => b,
    }
}
