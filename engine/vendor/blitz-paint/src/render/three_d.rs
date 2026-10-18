//! 3D transforms (local change, raea): CSS `transform` with 3D functions, `perspective` /
//! `perspective-origin` on the parent, `transform-style: preserve-3d` and `backface-visibility`.
//!
//! Upstream Blitz drops non-2D transforms. Here an element whose painting matrix is not an affine
//! map of its plane is painted the way Chrome's compositor does it: its subtree is rasterized flat
//! into an offscreen layer, which is then drawn through the plane's homography (bilinear, so the
//! layer's transparent border antialiases the edges). A preserve-3d element's children are planes
//! in one 3D context, ordered like Chrome's compositor (cc::BspTree): the planes' projected quads go
//! into a BSP tree in paint order, planes that cross another are split along it, and the fragments
//! are drawn back to front, each as its plane's image clipped to the fragment. Planes facing away
//! with `backface-visibility: hidden` are culled.
//!
//! Offscreen rasterization goes through a hook the embedder installs (`set_offscreen_rasterizer`)
//! so layers use the same rasterizer as the frame.

use std::cell::Cell;
use std::sync::Arc;

use anyrender::PaintScene;
use blitz_dom::NodeId;
use kurbo::{Affine, Rect};
use peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageSampler};
use style::properties::longhands::backface_visibility::computed_value::T as BackfaceVisibility;
use style::values::computed::{Perspective, TransformStyle};

use super::BlitzDomPainter;

/// Rasterizes a recorded scene into `width` x `height` premultiplied RGBA8 pixels.
pub type OffscreenRasterizer = fn(anyrender::Scene, u32, u32) -> Vec<u8>;

thread_local! {
    pub(crate) static RASTERIZER: Cell<Option<OffscreenRasterizer>> = const { Cell::new(None) };
}

/// Install the rasterizer for 3D layers on this thread (raea sets its frame backend).
pub fn set_offscreen_rasterizer(f: Option<OffscreenRasterizer>) {
    RASTERIZER.with(|r| r.set(f));
}

/// Draws a flat layer (premultiplied RGBA8, `sw` x `sh`, its top-left at local (`sx0`, `sy0`))
/// through a plane's 4x4 matrix (`m[row][col]`, column vectors) into a `dw` x `dh` premultiplied
/// RGBA8 image whose top-left is device (`dx0`, `dy0`). The embedder installs one (Skia's
/// concat_44 path); without it the layer is warped on the CPU.
pub type LayerWarper = fn(&[u8], u32, u32, f64, f64, [[f64; 4]; 4], f64, f64, u32, u32) -> Option<Vec<u8>>;

thread_local! {
    static WARPER: Cell<Option<LayerWarper>> = const { Cell::new(None) };
}

pub fn set_layer_warper(f: Option<LayerWarper>) {
    WARPER.with(|w| w.set(f));
}

thread_local! {
    pub(crate) static SVG_RASTERIZER: Cell<Option<OffscreenRasterizer>> = const { Cell::new(None) };
}

/// Inline SVG through a separate rasterizer (local change, raea): when set, each `<svg>` is
/// rasterized over its device-space bounds with it and composited as an image (the vello backend
/// installs Skia's, the rasterizer Chrome draws SVG with); `None` draws SVG into the frame directly.
pub fn set_svg_rasterizer(f: Option<OffscreenRasterizer>) {
    SVG_RASTERIZER.with(|r| r.set(f));
}

/// A 4x4 matrix for column vectors, `m[row][col]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Mat4(pub [[f64; 4]; 4]);

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]);

    pub fn mul(&self, o: &Mat4) -> Mat4 {
        let mut r = [[0.0; 4]; 4];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..4).map(|k| self.0[i][k] * o.0[k][j]).sum();
            }
        }
        Mat4(r)
    }

    pub fn translate(x: f64, y: f64, z: f64) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        m.0[0][3] = x;
        m.0[1][3] = y;
        m.0[2][3] = z;
        m
    }

    pub fn from_affine(a: Affine) -> Mat4 {
        let [a, b, c, d, e, f] = a.as_coeffs();
        Mat4([[a, c, 0.0, e], [b, d, 0.0, f], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]])
    }

    /// From euclid's row-vector `Transform3D` (p' = p·M), i.e. the transpose.
    pub fn from_euclid(t: &euclid::default::Transform3D<f64>) -> Mat4 {
        Mat4([
            [t.m11, t.m21, t.m31, t.m41],
            [t.m12, t.m22, t.m32, t.m42],
            [t.m13, t.m23, t.m33, t.m43],
            [t.m14, t.m24, t.m34, t.m44],
        ])
    }

    /// `perspective(d)`
    pub fn perspective(d: f64) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        if d > 0.0 {
            m.0[3][2] = -1.0 / d;
        }
        m
    }

    /// Homogeneous image of the plane point (x, y, 0, 1).
    pub fn apply(&self, x: f64, y: f64) -> [f64; 4] {
        let m = &self.0;
        [
            m[0][0] * x + m[0][1] * y + m[0][3],
            m[1][0] * x + m[1][1] * y + m[1][3],
            m[2][0] * x + m[2][1] * y + m[2][3],
            m[3][0] * x + m[3][1] * y + m[3][3],
        ]
    }

    /// The affine map of the z = 0 plane, when the matrix has no perspective on it.
    pub fn flat(&self) -> Option<Affine> {
        let m = &self.0;
        let w = m[3][3];
        if m[3][0].abs() > 1e-12 || m[3][1].abs() > 1e-12 || w.abs() < 1e-12 {
            return None;
        }
        Some(Affine::new([m[0][0] / w, m[1][0] / w, m[0][1] / w, m[1][1] / w, m[0][3] / w, m[1][3] / w]))
    }

    /// Plane -> screen homography as rows of a 3x3 matrix.
    fn homography(&self) -> [[f64; 3]; 3] {
        let m = &self.0;
        [[m[0][0], m[0][1], m[0][3]], [m[1][0], m[1][1], m[1][3]], [m[3][0], m[3][1], m[3][3]]]
    }
}

fn inverse3(h: [[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let [[a, b, c], [d, e, f], [g, h, i]] = h;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() < 1e-18 {
        return None;
    }
    let k = 1.0 / det;
    Some([
        [(e * i - f * h) * k, (c * h - b * i) * k, (b * f - c * e) * k],
        [(f * g - d * i) * k, (a * i - c * g) * k, (c * d - a * f) * k],
        [(d * h - e * g) * k, (b * g - a * h) * k, (a * e - b * d) * k],
    ])
}

/// How `render_element` should paint a node.
pub(crate) enum Placement {
    /// Upstream behaviour.
    Normal,
    /// 2D painting with this transform (a parent's perspective that happens to be affine).
    Flat(Affine),
    /// Through the 3D path with this plane -> target matrix.
    ThreeD(Mat4),
}

/// One plane of a 3D rendering context.
struct Plane {
    node_id: NodeId,
    matrix: Mat4,
    /// false for a preserve-3d element: its children are planes of their own
    paint_children: bool,
    depth: f64,
    order: usize,
}

impl BlitzDomPainter<'_, '_> {
    /// The element's own transform as a 4x4 matrix in device pixels (3D or the 2D one).
    fn own_matrix(&self, node_id: NodeId) -> Mat4 {
        let node = &self.dom.as_ref().tree()[node_id];
        let three = node.primary_styles().and_then(|s| {
            let size = node.final_layout().size;
            let reference_box = euclid::default::Rect::new(
                euclid::default::Point2D::new(style::values::computed::CSSPixelLength::new(0.0), style::values::computed::CSSPixelLength::new(0.0)),
                euclid::default::Size2D::new(
                    style::values::computed::CSSPixelLength::new(size.width),
                    style::values::computed::CSSPixelLength::new(size.height),
                ),
            );
            blitz_dom::resolve_3d_transform(s.get_box(), reference_box)
        });
        match three {
            Some(t) => {
                // CSS px -> device px: S · M · S^-1
                let s = self.scale;
                let m = Mat4::from_euclid(&t);
                let sm = Mat4([[s, 0.0, 0.0, 0.0], [0.0, s, 0.0, 0.0], [0.0, 0.0, s, 0.0], [0.0, 0.0, 0.0, 1.0]]);
                let si = Mat4([[1.0 / s, 0.0, 0.0, 0.0], [0.0, 1.0 / s, 0.0, 0.0], [0.0, 0.0, 1.0 / s, 0.0], [0.0, 0.0, 0.0, 1.0]]);
                sm.mul(&m).mul(&si)
            }
            None => Mat4::from_affine(node.transform().as_deref().copied().unwrap_or_default()),
        }
    }

    /// `perspective` of `node_id` applied to its children, in its local device coordinates.
    fn child_perspective(&self, node_id: NodeId) -> Option<Mat4> {
        let node = &self.dom.as_ref().tree()[node_id];
        let styles = node.primary_styles()?;
        let box_styles = styles.get_box();
        let Perspective::Length(d) = box_styles.perspective else { return None };
        let size = node.final_layout().size;
        let s = self.scale;
        let ox = box_styles.perspective_origin.horizontal.resolve(style::values::computed::CSSPixelLength::new(size.width)).px() as f64 * s;
        let oy = box_styles.perspective_origin.vertical.resolve(style::values::computed::CSSPixelLength::new(size.height)).px() as f64 * s;
        Some(Mat4::translate(ox, oy, 0.0).mul(&Mat4::perspective(d.px() as f64 * s)).mul(&Mat4::translate(-ox, -oy, 0.0)))
    }

    fn is_preserve_3d(&self, node_id: NodeId) -> bool {
        let node = &self.dom.as_ref().tree()[node_id];
        node.primary_styles().is_some_and(|s| s.get_box().transform_style == TransformStyle::Preserve3d)
    }

    /// Decide how to paint `node_id`, given the transform of its parent's coordinate space.
    pub(crate) fn placement(&self, node_id: NodeId, parent_style_transform: Affine, box_position: kurbo::Vec2) -> Placement {
        let node = &self.dom.as_ref().tree()[node_id];
        let Some(styles) = node.primary_styles() else { return Placement::Normal };
        let box_styles = styles.get_box();
        let has_3d = !box_styles.transform.0.is_empty() && self.own_matrix_is_3d(node_id);
        let preserve = box_styles.transform_style == TransformStyle::Preserve3d;
        let parent = node.layout_parent.get().or(node.parent);
        let persp = parent.and_then(|p| self.child_perspective(p));
        if !has_3d && !preserve && (persp.is_none() || node.transform().is_none()) {
            return Placement::Normal;
        }
        let full = Mat4::from_affine(parent_style_transform)
            .mul(&persp.unwrap_or(Mat4::IDENTITY))
            .mul(&Mat4::translate(box_position.x, box_position.y, 0.0))
            .mul(&self.own_matrix(node_id));
        if !preserve {
            if let Some(affine) = full.flat() {
                return Placement::Flat(affine);
            }
        }
        Placement::ThreeD(full)
    }

    fn own_matrix_is_3d(&self, node_id: NodeId) -> bool {
        let m = self.own_matrix(node_id);
        let m = &m.0;
        // anything beyond a 2D affine of the plane: z mixing or perspective
        m[3][0] != 0.0 || m[3][1] != 0.0 || m[3][2] != 0.0 || m[3][3] != 1.0
            || m[0][2] != 0.0 || m[1][2] != 0.0 || m[2][0] != 0.0 || m[2][1] != 0.0
            || m[2][3] != 0.0 || m[2][2] != 1.0
    }

    /// Paint `node_id` (and, if preserve-3d, its 3D context) through the 3D path.
    pub(crate) fn render_3d(&self, scene: &mut impl PaintScene, node_id: NodeId, full: Mat4, clip_rect: Rect) {
        let mut planes = Vec::new();
        self.collect_planes(node_id, full, &mut planes);
        planes.retain(|p| self.plane_draws_content(p));
        let quads: Option<Vec<Vec<[f64; 3]>>> = planes.iter().map(|p| self.projected_quad(p)).collect();
        let Some(quads) = quads.filter(|_| planes.len() > 1) else {
            // a single plane, or one crossing behind the viewer: depth order
            planes.sort_by(|a, b| a.depth.total_cmp(&b.depth).then(a.order.cmp(&b.order)));
            for plane in &planes {
                self.paint_plane(scene, plane, clip_rect);
            }
            return;
        };
        let ordered = bsp_back_to_front(&quads);
        let mut prepared: Vec<Option<Option<Prepared>>> = planes.iter().map(|_| None).collect();
        for frag in ordered {
            let plane = &planes[frag.plane];
            if frag.whole {
                self.paint_plane(scene, plane, clip_rect);
                continue;
            }
            let mut clip = kurbo::BezPath::new();
            for (i, p) in frag.pts.iter().enumerate() {
                let pt = kurbo::Point::new(p[0], p[1]);
                if i == 0 { clip.move_to(pt) } else { clip.line_to(pt) }
            }
            clip.close_path();
            // the plane's warped layer is made once and drawn once per fragment
            let image = prepared[frag.plane].get_or_insert_with(|| self.prepare_plane(plane, clip_rect));
            match image {
                // split polygons aren't antialiased (as in cc): each pixel goes to the fragment that
                // contains its centre, so fragments neither overlap nor leave seams
                Some(Prepared::Image(brush, at)) => {
                    let masked = mask_to_polygon(brush, *at, &frag.pts);
                    scene.draw_image(masked.as_ref(), Affine::translate(*at));
                }
                Some(Prepared::Flat) => {
                    scene.push_clip_layer(peniko::Fill::NonZero, Affine::IDENTITY, &clip);
                    self.paint_plane(scene, plane, clip_rect);
                    scene.pop_layer();
                }
                None => {}
            }
        }
    }

    /// Whether the plane's own box paints anything (cc only puts layers that draw content into its
    /// BSP tree: an empty preserve-3d container must not split its children).
    fn plane_draws_content(&self, plane: &Plane) -> bool {
        let node = &self.dom.as_ref().tree()[plane.node_id];
        if plane.paint_children {
            return true;
        }
        let Some(style) = node.primary_styles() else { return true };
        let current = style.clone_color();
        let bg = style.get_background();
        if bg.background_color.resolve_to_absolute(&current).alpha > 0.0
            || bg.background_image.0.iter().any(|i| !matches!(i, style::values::generics::image::GenericImage::None))
        {
            return true;
        }
        let b = style.get_border();
        use style::values::computed::BorderStyle;
        let side = |w: &style::values::computed::BorderSideWidth, st: BorderStyle| w.0 .0 > 0 && !matches!(st, BorderStyle::None | BorderStyle::Hidden);
        if side(&b.border_top_width, b.border_top_style)
            || side(&b.border_right_width, b.border_right_style)
            || side(&b.border_bottom_width, b.border_bottom_style)
            || side(&b.border_left_width, b.border_left_style)
            || !style.get_effects().box_shadow.0.is_empty()
        {
            return true;
        }
        let outline = style.get_outline();
        if outline.outline_width.0 .0 > 0 && !matches!(outline.outline_style, style::values::computed::OutlineStyle::BorderStyle(style::values::computed::BorderStyle::None)) {
            return true;
        }
        // its own text
        node.children.iter().any(|&c| self.dom.as_ref().tree()[c].text_data().is_some_and(|t| !t.content.trim().is_empty()))
    }

    /// The plane's border box in screen space with its depth (x/w, y/w, z/w), in Chrome's
    /// DrawPolygon winding; `None` when a corner is at or behind the viewer.
    fn projected_quad(&self, plane: &Plane) -> Option<Vec<[f64; 3]>> {
        let size = self.dom.as_ref().tree()[plane.node_id].final_layout().size;
        let (w, h) = (size.width as f64 * self.scale, size.height as f64 * self.scale);
        [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]
            .iter()
            .map(|&(x, y)| {
                let q = plane.matrix.apply(x, y);
                (q[3] > 1e-6).then(|| [q[0] / q[3], q[1] / q[3], q[2] / q[3]])
            })
            .collect()
    }

    fn collect_planes(&self, node_id: NodeId, matrix: Mat4, out: &mut Vec<Plane>) {
        let node = &self.dom.as_ref().tree()[node_id];
        let preserve = self.is_preserve_3d(node_id);
        let size = node.final_layout().size;
        let (w, h) = (size.width as f64 * self.scale, size.height as f64 * self.scale);
        let c = matrix.apply(w / 2.0, h / 2.0);
        let depth = if c[3].abs() > 1e-12 { c[2] / c[3] } else { c[2] };
        let order = out.len();
        out.push(Plane { node_id, matrix, paint_children: !preserve, depth, order });
        if !preserve {
            return;
        }
        let persp = self.child_perspective(node_id).unwrap_or(Mat4::IDENTITY);
        let base = matrix.mul(&persp);
        let children: Vec<NodeId> = node.paint_children.borrow().as_ref().map(|c| c.iter().copied().collect()).unwrap_or_default();
        for child_id in children {
            let child = &self.dom.as_ref().tree()[child_id];
            if !child.is_element() {
                continue;
            }
            let loc = child.final_layout().location;
            let m = base
                .mul(&Mat4::translate(loc.x as f64 * self.scale, loc.y as f64 * self.scale, 0.0))
                .mul(&self.own_matrix(child_id));
            self.collect_planes(child_id, m, out);
        }
    }

    fn backface_hidden(&self, node_id: NodeId, matrix: &Mat4) -> bool {
        let node = &self.dom.as_ref().tree()[node_id];
        let hidden = node
            .primary_styles()
            .is_some_and(|s| s.get_box().backface_visibility == BackfaceVisibility::Hidden);
        if !hidden {
            return false;
        }
        // The plane faces away when its projected orientation flips (Chrome: the transformed
        // normal points away from the viewer).
        let size = node.final_layout().size;
        let (w, h) = (size.width.max(1.0) as f64, size.height.max(1.0) as f64);
        let p = |x: f64, y: f64| {
            let q = matrix.apply(x, y);
            (q[0] / q[3], q[1] / q[3], q[3])
        };
        let (a, b, c) = (p(0.0, 0.0), p(w, 0.0), p(0.0, h));
        if a.2 <= 0.0 || b.2 <= 0.0 || c.2 <= 0.0 {
            return false;
        }
        let cross = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        cross < 0.0
    }

    fn paint_plane(&self, scene: &mut impl PaintScene, plane: &Plane, clip_rect: Rect) {
        if self.backface_hidden(plane.node_id, &plane.matrix) {
            return;
        }
        if let Some(affine) = plane.matrix.flat() {
            self.forced.set(Some((plane.node_id, affine, plane.paint_children)));
            self.render_element(scene, plane.node_id, Affine::IDENTITY, clip_rect);
            return;
        }
        if let Some(Prepared::Image(brush, at)) = self.prepare_plane(plane, clip_rect) {
            scene.draw_image(brush.as_ref(), Affine::translate(at));
        }
    }

    /// A non-flat plane rasterized flat and warped through its homography (`Flat` when the plane
    /// is an affine map and paints directly, `None` when it is culled or off screen).
    fn prepare_plane(&self, plane: &Plane, clip_rect: Rect) -> Option<Prepared> {
        if self.backface_hidden(plane.node_id, &plane.matrix) {
            return None;
        }
        if plane.matrix.flat().is_some() {
            return Some(Prepared::Flat);
        }
        let rasterize = RASTERIZER.with(|r| r.get())?;

        // Local bounds of the layer: the border box plus a margin for overflow, antialiased edges
        // and the transparent border bilinear sampling fades into.
        let node = &self.dom.as_ref().tree()[plane.node_id];
        let size = node.final_layout().size;
        let overflow = *node.scrollable_overflow();
        let local = Rect::new(0.0, 0.0, size.width as f64 * self.scale, size.height as f64 * self.scale)
            .union(Rect::new(overflow.x0 * self.scale, overflow.y0 * self.scale, overflow.x1 * self.scale, overflow.y1 * self.scale))
            .inflate(2.0, 2.0)
            .expand();

        // Where it lands; planes that cross behind the viewer are skipped (Chrome clips them).
        let corners = [(local.x0, local.y0), (local.x1, local.y0), (local.x1, local.y1), (local.x0, local.y1)];
        let mut screen = Rect::new(f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (x, y) in corners {
            let q = plane.matrix.apply(x, y);
            if q[3] <= 1e-6 {
                return None;
            }
            screen = screen.union_pt(kurbo::Point::new(q[0] / q[3], q[1] / q[3]));
        }
        let target = clip_rect + kurbo::Vec2::new(self.initial_x, self.initial_y);
        let dst = screen.intersect(target).expand();
        if dst.width() < 1.0 || dst.height() < 1.0 {
            return None;
        }
        let (lw, lh) = (local.width() as u32, local.height() as u32);
        if lw == 0 || lh == 0 || lw > 8192 || lh > 8192 {
            return None;
        }

        // Rasterize the plane flat, its border-box origin at -local.origin.
        let mut layer = anyrender::Scene::new();
        self.forced.set(Some((plane.node_id, Affine::translate((-local.x0, -local.y0)), plane.paint_children)));
        let layer_clip = Rect::new(0.0, 0.0, local.width(), local.height());
        crate::text::without_text_runs(|| self.render_element(&mut layer, plane.node_id, Affine::IDENTITY, layer_clip));
        let src = rasterize(layer, lw, lh);

        let (dw, dh) = (dst.width() as u32, dst.height() as u32);
        let warped = WARPER
            .with(|w| w.get())
            .and_then(|warper| warper(&src, lw, lh, local.x0, local.y0, plane.matrix.0, dst.x0, dst.y0, dw, dh));
        let out = match warped {
            Some(out) => out,
            None => {
                let inv = inverse3(plane.matrix.homography())?;
                let mut out = vec![0u8; (dw * dh * 4) as usize];
                warp(&src, lw, lh, local.x0, local.y0, &inv, dst.x0, dst.y0, dw, dh, &mut out);
                out
            }
        };

        let brush = ImageBrush {
            image: ImageData {
                data: Blob::new(Arc::new(out)),
                format: ImageFormat::Rgba8,
                width: dw,
                height: dh,
                alpha_type: ImageAlphaType::AlphaPremultiplied,
            },
            sampler: ImageSampler::default(),
        };
        Some(Prepared::Image(brush, kurbo::Vec2::new(dst.x0, dst.y0)))
    }
}

/// A copy of a device-space layer image (top-left at `at`) keeping only the pixels whose centres lie
/// in the convex polygon `pts` (device x, y).
fn mask_to_polygon(brush: &ImageBrush, at: kurbo::Vec2, pts: &[[f64; 3]]) -> ImageBrush {
    let (w, h) = (brush.image.width as usize, brush.image.height as usize);
    let mut data = brush.image.data.data().to_vec();
    // orientation of the polygon, so "inside" is the same side of every edge
    let area: f64 = (0..pts.len()).map(|i| {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        a[0] * b[1] - b[0] * a[1]
    }).sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    // cc draws split polygons without edge antialiasing, along their outline too
    let cuts: Vec<([f64; 3], [f64; 3])> = (0..pts.len()).map(|i| (pts[i], pts[(i + 1) % pts.len()])).collect();
    for y in 0..h {
        let py = at.y + y as f64 + 0.5;
        for x in 0..w {
            let px = at.x + x as f64 + 0.5;
            let inside = cuts.iter().all(|&(a, b)| sign * ((b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0])) >= 0.0);
            if !inside {
                let o = (y * w + x) * 4;
                data[o..o + 4].fill(0);
            }
        }
    }
    let mut out = brush.clone();
    out.image.data = Blob::new(Arc::new(data));
    out
}

enum Prepared {
    Image(ImageBrush, kurbo::Vec2),
    Flat,
}

/// A piece of a plane's projected quad (all of it while `whole`).
struct Frag {
    plane: usize,
    pts: Vec<[f64; 3]>,
    whole: bool,
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Unit normal of a polygon (Newell's method, robust to degenerate corners).
fn normal(pts: &[[f64; 3]]) -> [f64; 3] {
    let mut n = [0.0; 3];
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let l = dot(n, n).sqrt();
    if l > 0.0 { [n[0] / l, n[1] / l, n[2] / l] } else { [0.0, 0.0, 1.0] }
}

/// Splits `frag` by the plane (n, d): signed distances above `eps` are in front.
fn split(frag: &Frag, n: [f64; 3], d: f64, eps: f64) -> (Option<Frag>, Option<Frag>) {
    let dist: Vec<f64> = frag.pts.iter().map(|p| dot(n, *p) + d).collect();
    let (mut front, mut back) = (Vec::new(), Vec::new());
    for i in 0..frag.pts.len() {
        let j = (i + 1) % frag.pts.len();
        let (a, b, da, db) = (frag.pts[i], frag.pts[j], dist[i], dist[j]);
        if da >= -eps {
            front.push(a);
        }
        if da <= eps {
            back.push(a);
        }
        if (da > eps && db < -eps) || (da < -eps && db > eps) {
            let t = da / (da - db);
            let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
            front.push(p);
            back.push(p);
        }
    }
    let piece = |pts: Vec<[f64; 3]>| (pts.len() >= 3).then(|| Frag { plane: frag.plane, pts, whole: false });
    (piece(front), piece(back))
}

/// cc::BspTree over the planes' projected quads (screen x, y and depth, in paint order): the first
/// fragment splits the rest (front / back / coplanar) with cc's distance threshold, recursively, and the
/// result is drawn back to front by the IsFacingPositiveZ rule. (WebRender's plane-split crate was
/// tried: its tolerance splits cube faces that only share an edge into slivers, cube 0.997 -> 0.88.)
fn bsp_back_to_front(quads: &[Vec<[f64; 3]>]) -> Vec<Frag> {
    let frags = quads.iter().enumerate().map(|(plane, pts)| Frag { plane, pts: pts.clone(), whole: true }).collect();
    let mut out = Vec::new();
    bsp_node(frags, &mut out);
    out
}

fn bsp_node(mut frags: Vec<Frag>, out: &mut Vec<Frag>) {
    if frags.is_empty() {
        return;
    }
    let node = frags.remove(0);
    let n = normal(&node.pts);
    let d = -dot(n, node.pts[0]);
    let eps = 0.05;
    let (mut front, mut back, mut co_front, mut co_back) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for f in frags {
        let dist: Vec<f64> = f.pts.iter().map(|p| dot(n, *p) + d).collect();
        let above = dist.iter().any(|&x| x > eps);
        let below = dist.iter().any(|&x| x < -eps);
        match (above, below) {
            (true, false) => front.push(f),
            (false, true) => back.push(f),
            (false, false) => {
                if dot(normal(&f.pts), n) >= 0.0 { co_front.push(f) } else { co_back.push(f) }
            }
            (true, true) => {
                let (fr, bk) = split(&f, n, d, eps);
                front.extend(fr);
                back.extend(bk);
            }
        }
    }
    if n[2] > 0.0 {
        bsp_node(back, out);
        out.extend(co_back);
        out.push(node);
        out.extend(co_front);
        bsp_node(front, out);
    } else {
        bsp_node(front, out);
        out.extend(co_front);
        out.push(node);
        out.extend(co_back);
        bsp_node(back, out);
    }
}

/// Inverse-map every destination pixel centre through the homography and sample the
/// premultiplied layer bilinearly (transparent outside it).
#[allow(clippy::too_many_arguments)]
fn warp(src: &[u8], sw: u32, sh: u32, sx0: f64, sy0: f64, inv: &[[f64; 3]; 3], dx0: f64, dy0: f64, dw: u32, dh: u32, out: &mut [u8]) {
    let (sw, sh) = (sw as i64, sh as i64);
    let texel = |x: i64, y: i64| -> [f32; 4] {
        if x < 0 || y < 0 || x >= sw || y >= sh {
            return [0.0; 4];
        }
        let i = ((y * sw + x) * 4) as usize;
        [src[i] as f32, src[i + 1] as f32, src[i + 2] as f32, src[i + 3] as f32]
    };
    for j in 0..dh {
        let py = dy0 + j as f64 + 0.5;
        for i in 0..dw {
            let px = dx0 + i as f64 + 0.5;
            let w = inv[2][0] * px + inv[2][1] * py + inv[2][2];
            if w.abs() < 1e-12 {
                continue;
            }
            let u = (inv[0][0] * px + inv[0][1] * py + inv[0][2]) / w - sx0 - 0.5;
            let v = (inv[1][0] * px + inv[1][1] * py + inv[1][2]) / w - sy0 - 0.5;
            if u < -1.0 || v < -1.0 || u > sw as f64 || v > sh as f64 {
                continue;
            }
            let (x0, y0) = (u.floor(), v.floor());
            let (fx, fy) = ((u - x0) as f32, (v - y0) as f32);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let (a, b, c, d) = (texel(x0, y0), texel(x0 + 1, y0), texel(x0, y0 + 1), texel(x0 + 1, y0 + 1));
            let o = ((j * dw + i) * 4) as usize;
            for k in 0..4 {
                let top = a[k] + (b[k] - a[k]) * fx;
                let bottom = c[k] + (d[k] - c[k]) * fx;
                out[o + k] = (top + (bottom - top) * fy).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}
