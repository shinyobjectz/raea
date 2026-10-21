use anyrender::PaintScene;
use blitz_dom::{BaseDocument, NodeId, node::TextBrush, util::ToColorColor};
use kurbo::{Affine, BezPath, Cap, Circle, Rect, Stroke};
use parley::{Affinity, Cursor, Layout, Line, PositionedLayoutItem, Selection};
use peniko::Fill;
use std::collections::HashMap;
use style::properties::generated::longhands::text_decoration_style::computed_value::T as TextDecorationStyle;
use style::values::computed::{
    Length, LengthPercentage, TextDecorationLength, TextDecorationLine, TextUnderlinePosition,
};
use style::values::generics::text::{GenericTextDecorationInset, GenericTextDecorationLength};

use crate::color::{Color, ToColorColor as _};
use crate::{FONT_EMBOLDEN_ENABLED, SELECTION_COLOR};

/// Draw the backgrounds of inline elements (e.g. `<span style="background: ...">`).
///
/// Each glyph run carries the node id of the innermost inline element it belongs to
/// (via its brush). We look up that node's `background-color` and, if non-transparent,
/// fill a rectangle covering the run's advance and its font's ascent/descent so that the
/// background sits behind the text.
///
/// The inline root's own background is painted separately (as a normal block box), so
/// runs belonging to the root are skipped to avoid drawing it twice.
pub(crate) fn draw_inline_backgrounds<'a>(
    scene: &mut impl PaintScene,
    lines: impl Iterator<Item = Line<'a, TextBrush>>,
    doc: &BaseDocument,
    transform: Affine,
    inline_root_id: NodeId,
) {
    for line in lines {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };

            let node_id = glyph_run.style().brush.id;
            if node_id == inline_root_id {
                continue;
            }

            let Some(styles) = doc.get_node(node_id).and_then(|node| node.primary_styles()) else {
                continue;
            };

            let current_color = styles.clone_color();
            // inline opacity (local change)
            let mut chain_opacity = 1.0f32;
            let mut walk = Some(node_id);
            while let Some(id) = walk {
                if id == inline_root_id {
                    break;
                }
                let n = doc.get_node(id);
                chain_opacity *= n.and_then(|n| n.primary_styles()).map(|s| s.get_effects().opacity).unwrap_or(1.0);
                walk = n.and_then(|n| n.parent);
            }
            let bg_color = styles
                .get_background()
                .background_color
                .resolve_to_absolute(&current_color)
                .as_srgb_color();
            if bg_color == Color::TRANSPARENT {
                continue;
            }

            let metrics = glyph_run.run().font_metrics();
            let x = glyph_run.offset() as f64;
            let w = glyph_run.advance() as f64;
            let baseline = glyph_run.baseline() as f64;
            let y0 = baseline - metrics.ascent as f64;
            let y1 = baseline + metrics.descent as f64;
            let rect = Rect::new(x, y0, x + w, y1);

            let bg_color = if chain_opacity < 1.0 { bg_color.multiply_alpha(chain_opacity) } else { bg_color };
            scene.fill(Fill::NonZero, transform, bg_color, None, &rect);
        }
    }
}

/// Per-font-face cache of the OS/2 `usWinAscent / unitsPerEm` ratio, keyed by the font
/// blob's unique id and face index. `None` is cached for fonts without a usable OS/2 or
/// head table so they aren't re-parsed either.
type WinAscentCache = HashMap<(u64, u32), Option<f32>>;

/// The font's OS/2 `usWinAscent`, in the same (device) pixels as `font_size`.
///
/// This is the ascent browsers use as the top of the "em box" when positioning overlines. It
/// is typically taller than the hhea ascent Parley exposes via [`parley::layout::run::RunMetrics`].
/// Returns `None` when the font has no OS/2 table.
fn win_ascent(cache: &mut WinAscentCache, font: &parley::FontData, font_size: f32) -> Option<f32> {
    let ratio = *cache
        .entry((font.data.id(), font.index))
        .or_insert_with(|| win_ascent_ratio(font));
    Some(ratio? * font_size)
}

/// The unitless `usWinAscent / unitsPerEm` ratio for a font face, or `None` when the font
/// has no usable head/OS/2 table.
fn win_ascent_ratio(font: &parley::FontData) -> Option<f32> {
    use skrifa::raw::{FontRef, TableProvider as _};
    let font_ref = FontRef::from_index(font.data.as_ref(), font.index).ok()?;
    let units_per_em = font_ref.head().ok()?.units_per_em();
    let win_ascent = font_ref.os2().ok()?.us_win_ascent();
    Some(win_ascent as f32 / units_per_em as f32)
}

/// Mirrors Blink's `SelectBestDashGap`: choose the gap length (as close as possible to
/// `gap_length`) that fits a whole number of `dash_length` dashes across `stroke_length`,
/// so dashes are evenly distributed and a dash lands at each end of the line.
fn select_best_dash_gap(stroke_length: f64, dash_length: f64, gap_length: f64) -> f64 {
    let available_length = stroke_length + gap_length;
    let min_num_dashes = (available_length / (dash_length + gap_length)).floor();
    let max_num_dashes = min_num_dashes + 1.0;
    let min_num_gaps = min_num_dashes - 1.0;
    let max_num_gaps = max_num_dashes - 1.0;
    if min_num_gaps < 1.0 {
        return gap_length;
    }
    let min_gap = (stroke_length - min_num_dashes * dash_length) / min_num_gaps;
    let max_gap = (stroke_length - max_num_dashes * dash_length) / max_num_gaps;
    if max_gap <= 0.0 || (min_gap - gap_length).abs() < (max_gap - gap_length).abs() {
        min_gap
    } else {
        max_gap
    }
}

/// A text decoration resolved from a single "decorating box" (an element that
/// sets `text-decoration-line`), ready to be drawn over a glyph run. Because the
/// `text-decoration-*` properties do not inherit, decorations are propagated from
/// ancestors rather than inherited, so a run may be covered by several of these.
///
/// Only the run-independent (style-derived) values are stored here so an instance
/// can be cached on the ancestor stack and reused across every run within the same
/// decorating box. The `text-decoration-inset` and `text-underline-offset` values
/// depend on the run's advance/font-size and are resolved lazily when drawing.
#[derive(Clone)]
struct ResolvedDecoration {
    line: TextDecorationLine,
    style: TextDecorationStyle,
    color: Color,
    thickness: TextDecorationLength,
    /// `text-underline-offset`; `None` for `auto`. Resolved per run.
    underline_offset: Option<LengthPercentage>,
    /// Whether `text-underline-position: under` is in effect.
    underline_under: bool,
    /// Whether `text-underline-position: from-font` is in effect.
    underline_from_font: bool,
    /// `text-decoration-inset`. Resolved per run.
    inset: GenericTextDecorationInset<LengthPercentage>,
}

/// An element on the current ancestor path (inline root -> run node), caching the
/// values resolved from its computed style so descending into the same node across
/// consecutive runs doesn't re-resolve them.
struct DecorationStackEntry {
    node_id: NodeId,
    /// This node's (inherited) text colour, used for the glyphs of runs whose
    /// innermost node is this one.
    text_color: Color,
    /// The decoration this node introduces as a decorating box, if any.
    decoration: Option<ResolvedDecoration>,
    /// inline opacity (local change): this node's own `opacity`
    opacity: f32,
}

/// Resolve the cached style values for a single node into a [`DecorationStackEntry`].
fn resolve_decoration_entry(doc: &BaseDocument, node_id: NodeId) -> DecorationStackEntry {
    let Some(styles) = doc.get_node(node_id).and_then(|node| node.primary_styles()) else {
        return DecorationStackEntry {
            node_id,
            text_color: Color::BLACK,
            decoration: None,
            opacity: 1.0,
        };
    };

    let itext = styles.get_inherited_text();
    let text = styles.get_text();
    let text_color = itext.color.as_color_color();

    let drawn_lines = TextDecorationLine::UNDERLINE
        | TextDecorationLine::OVERLINE
        | TextDecorationLine::LINE_THROUGH;
    let line = text.text_decoration_line;
    // Decorations propagate through the box tree, and a `display: contents` element
    // generates no box, so its decorations have no effect on descendants.
    let is_contents = styles.clone_display().is_contents();
    let decoration = (!is_contents && line.intersects(drawn_lines)).then(|| {
        // `text-decoration-color: currentColor` (the initial value) resolves against
        // the decorating box's own colour, not the descendant run's.
        let color = text
            .text_decoration_color
            .as_absolute()
            .map(ToColorColor::as_color_color)
            .unwrap_or(text_color);

        ResolvedDecoration {
            line,
            style: text.text_decoration_style,
            color,
            thickness: text.text_decoration_thickness.clone(),
            underline_offset: itext.text_underline_offset.non_auto(),
            underline_under: itext
                .text_underline_position
                .contains(TextUnderlinePosition::UNDER),
            underline_from_font: itext
                .text_underline_position
                .contains(TextUnderlinePosition::FROM_FONT),
            inset: text.text_decoration_inset.clone(),
        }
    });

    DecorationStackEntry {
        node_id,
        text_color,
        decoration,
        opacity: styles.get_effects().opacity,
    }
}

/// The run-dependent geometry used to position and size a decoration, captured from a
/// single glyph run. Firefox draws a decoration once for the whole decorating box using
/// the box's own font, so we prefer the values from a run whose innermost node *is* the
/// decorating box (its own text), falling back to the first run it covers.
#[derive(Clone)]
struct DecorationRunGeometry {
    /// The baseline the decoration is positioned from (the run's own, `vertical-align`
    /// shifted, baseline).
    baseline: f32,
    ascent: f32,
    descent: f32,
    underline_offset: f32,
    underline_size: f32,
    strikethrough_size: f32,
    font: parley::FontData,
    font_size: f32,
    css_font_size: f64,
}

/// A decoration accumulated across the runs of a single line for one decorating box.
///
/// The `text-decoration-*` properties describe the whole box, so we gather the horizontal
/// extent it covers on the line (`min_x`/`max_x`) and the font geometry to draw with, then
/// paint a single line spanning the box rather than one segment per run.
struct LineDecoration {
    node_id: NodeId,
    deco: ResolvedDecoration,
    min_x: f64,
    max_x: f64,
    /// Geometry from a run whose innermost node is the decorating box itself.
    own: Option<DecorationRunGeometry>,
    /// Geometry from the first run the box covers (fallback when it has no own text on
    /// this line, e.g. it only wraps differently-sized descendants).
    first: Option<DecorationRunGeometry>,
}

/// Reusable scratch storage for drawing text across all inline formatting contexts in a
/// document. The vectors are cleared between uses without releasing their allocations.
#[derive(Default)]
pub(crate) struct DrawTextContext {
    stack: Vec<DecorationStackEntry>,
    path_scratch: Vec<NodeId>,
    deco_boxes: Vec<LineDecoration>,
    win_ascent_ratios: WinAscentCache,
    /// background-clip:text (local change): glyphs only, opaque black, no shadows or decorations
    pub(crate) mask: bool,
}

/// Resolve the CSS `text-decoration-thickness` to a device-pixel size.
///
/// - Percentages are resolved against the font size,
/// - `from-font` uses the metrics from Parley,
/// - `auto` uses a thickness of `font-size / 10` (minimum: 1px).
///
/// The result is floored to a whole number of device pixels (minimum 1), matching
/// browsers, which snap decoration thickness to whole pixels.
fn decoration_size(
    thickness: &TextDecorationLength,
    metric_size: f32,
    css_font_size: f64,
    scale: f64,
) -> f32 {
    (match thickness {
        GenericTextDecorationLength::LengthPercentage(lp) => {
            lp.resolve(Length::new(css_font_size as f32)).px() as f64 * scale
        }
        GenericTextDecorationLength::FromFont => metric_size as f64,
        GenericTextDecorationLength::Auto => (css_font_size / 10.0).max(1.0) * scale,
    })
    .floor()
    .max(1.0) as f32
}

/// Draws a single decoration line of the given `text-decoration-style` spanning
/// `[x0, x0 + width]`. `baseline` is the line's baseline and `offset` positions the line
/// relative to it (`y = baseline - offset + size / 2`, growing downward). Geometry is built
/// explicitly (rather than relying on backend dash support) so it renders consistently
/// across renderers.
///
/// `double_dir` points away from the text (`+1` = down, `-1` = up) and is used to place the
/// second line of a `double` decoration.
#[allow(clippy::too_many_arguments)]
fn draw_decoration_line(
    scene: &mut impl PaintScene,
    transform: Affine,
    scale: f64,
    x0: f64,
    width: f64,
    baseline: f32,
    offset: f32,
    size: f32,
    brush: &anyrender::Paint,
    double_dir: f64,
    deco_style: TextDecorationStyle,
    inset_start: f64,
    inset_end: f64,
) {
    // Inset the line from the box's start/end edges (assumes LTR: start is the left edge).
    // Negative insets extend the line past the text.
    let x = x0 + inset_start;
    let w = width - inset_start - inset_end;
    if w <= 0.0 {
        return;
    }
    let y = (baseline - offset + size / 2.0) as f64;
    let size = size as f64;
    let butt_stroke = Stroke::new(size).with_caps(Cap::Butt);

    match deco_style {
        TextDecorationStyle::MozNone => {
            // no line. Equivalent to `text-decoration-line: none`
        }
        // `solid`
        TextDecorationStyle::Solid => {
            let line = kurbo::Line::new((x, y), (x + w, y));
            scene.stroke(&butt_stroke, transform, brush, None, &line);
        }
        // Two lines, each `size` thick, separated by a 1px (CSS) gap. This
        // matches Chrome, where the second line is offset by `thickness + 1px`
        // and sits further from the text (below for underline, above for
        // overline).
        TextDecorationStyle::Double => {
            let one_css_px = scale;
            for cy in [y, y + double_dir * (size + one_css_px)] {
                let line = kurbo::Line::new((x, cy), (x + w, cy));
                scene.stroke(&butt_stroke, transform, brush, None, &line);
            }
        }
        // Round dots (diameter = line thickness) spaced one dot apart.
        TextDecorationStyle::Dotted => {
            let radius = size / 2.0;
            let step = 2.0 * size;
            let mut cx = x + radius;
            while cx <= x + w - radius {
                let dot = Circle::new((cx, y), radius);
                scene.fill(Fill::NonZero, transform, brush, None, &dot);
                cx += step;
            }
        }
        // Dashes as filled rectangles. Dash/gap lengths are relative to the
        // thickness (matching Blink): thinner lines use proportionally longer
        // dashes and gaps so they don't read as dotted or solid.
        TextDecorationStyle::Dashed => {
            let (dash_ratio, gap_ratio) = if size >= 3.0 { (2.0, 1.0) } else { (3.0, 2.0) };
            let dash = size * dash_ratio;
            let nominal_gap = size * gap_ratio;
            // Nudge the gap so a whole number of dashes fits the run evenly.
            let gap = if w > 2.0 * dash {
                select_best_dash_gap(w, dash, nominal_gap)
            } else {
                nominal_gap
            };
            let mut dx = x;
            while dx < x + w {
                let end = (dx + dash).min(x + w);
                let rect = Rect::new(dx, y - size / 2.0, end, y + size / 2.0);
                scene.fill(Fill::NonZero, transform, brush, None, &rect);
                dx += dash + gap;
            }
        }
        // A squiggle built from alternating quadratic Béziers.
        TextDecorationStyle::Wavy => {
            let amplitude = size;
            let half_wave = (2.0 * size).max(1.0);
            let mut path = BezPath::new();
            path.move_to((x, y));
            let mut px = x;
            let mut up = true;
            while px < x + w {
                let nx = (px + half_wave).min(x + w);
                // A quad Bézier reaches half the control-point offset at its
                // midpoint, so use `2 * amplitude` to hit the target peak.
                let ctrl_y = if up {
                    y - 2.0 * amplitude
                } else {
                    y + 2.0 * amplitude
                };
                path.quad_to(((px + nx) / 2.0, ctrl_y), (nx, y));
                px = nx;
                up = !up;
            }
            let stroke = Stroke::new(size).with_caps(Cap::Round);
            scene.stroke(&stroke, transform, brush, None, &path);
        }
    }
}

/// Paint the decorations accumulated for one line, one line per decorating box.
fn flush_line_decorations(
    scene: &mut impl PaintScene,
    transform: Affine,
    scale: f64,
    deco_boxes: &[LineDecoration],
    win_ascent_ratios: &mut WinAscentCache,
    inline_root_id: NodeId,
    line_baseline: f32,
) {
    // Draw innermost boxes first so ancestors' decorations paint on top, matching the
    // per-run drawing order this replaced (`stack.iter().rev()`).
    for acc in deco_boxes.iter().rev() {
        let deco = &acc.deco;
        // Prefer the decorating box's own font; fall back to the first run it covers.
        let geom = match (&acc.own, &acc.first) {
            (Some(own), _) => own.clone(),
            (None, Some(first)) => {
                let mut geom = first.clone();
                // A descendant run's baseline may be shifted by `vertical-align`, but the
                // decoration is positioned from the decorating box's own baseline. For the
                // inline root that is the line's baseline.
                if acc.node_id == inline_root_id {
                    geom.baseline = line_baseline;
                }
                geom
            }
            (None, None) => continue,
        };
        let geom = &geom;
        let width = acc.max_x - acc.min_x;
        if width <= 0.0 {
            continue;
        }
        let brush = anyrender::Paint::from(deco.color);

        // `text-decoration-inset` shortens (or, when negative, extends) the line from the
        // inline-start/end edges. Percentages resolve against the decoration line length
        // (the box's total advance on this line); `auto` is no inset.
        let (inset_start, inset_end) = match &deco.inset {
            GenericTextDecorationInset::LengthPercentage { start, end } => {
                // The extent is in device pixels; convert to CSS pixels so the resolved
                // result can be scaled back.
                let line_length = Length::new((width / scale) as f32);
                (
                    start.resolve(line_length).px() as f64 * scale,
                    end.resolve(line_length).px() as f64 * scale,
                )
            }
            GenericTextDecorationInset::Auto => (0.0, 0.0),
        };

        if deco.line.contains(TextDecorationLine::UNDERLINE) {
            let size = decoration_size(
                &deco.thickness,
                geom.underline_size,
                geom.css_font_size,
                scale,
            );

            // `text-underline-offset` moves the underline away from the text. `auto` keeps
            // the font's suggested position; otherwise it resolves against the font size
            // (percentages relative to 1em).
            let extra_underline_offset = deco
                .underline_offset
                .as_ref()
                .map(|lp| lp.resolve(Length::new(geom.css_font_size as f32)).px() as f64 * scale)
                .unwrap_or(0.0);

            // `text-underline-position: under` places the underline below the glyph box
            // (below descenders) rather than at the font's suggested position near the
            // alphabetic baseline. We anchor the top of the line at the descent so it clears
            // descending glyphs like "gqy".
            // An explicit offset under `auto` is taken from the alphabetic
            // baseline, as CSS and Chrome have it, not from the auto gap.
            let base_offset = if deco.underline_under {
                -geom.descent
            } else if deco.underline_offset.is_some() && !deco.underline_from_font {
                0.0
            } else {
                geom.underline_offset
            };
            let offset = base_offset - extra_underline_offset as f32;

            // A `double` underline extends downward, away from the text.
            draw_decoration_line(
                scene,
                transform,
                scale,
                acc.min_x,
                width,
                geom.baseline,
                offset,
                size,
                &brush,
                1.0,
                deco.style,
                inset_start,
                inset_end,
            );
        }
        if deco.line.contains(TextDecorationLine::OVERLINE) {
            // Fonts don't provide a dedicated overline metric, so reuse the underline
            // thickness. The line sits at the top of the "em box": its lower edge rests on
            // the ascent so it clears the glyphs, and it extends upward from there.
            //
            // Browsers use the OS/2 `usWinAscent` for this edge, which is taller than the
            // hhea-based ascent Parley reports; using the smaller value would draw the
            // overline too low. Fall back to Parley's ascent for fonts without a usable
            // OS/2 table.
            let size = decoration_size(
                &deco.thickness,
                geom.underline_size,
                geom.css_font_size,
                scale,
            );
            let ascent =
                win_ascent(win_ascent_ratios, &geom.font, geom.font_size).unwrap_or(geom.ascent);
            let offset = ascent + size;

            // A `double` overline extends upward, away from the text.
            draw_decoration_line(
                scene,
                transform,
                scale,
                acc.min_x,
                width,
                geom.baseline,
                offset,
                size,
                &brush,
                -1.0,
                deco.style,
                inset_start,
                inset_end,
            );
        }
        if deco.line.contains(TextDecorationLine::LINE_THROUGH) {
            let size = decoration_size(
                &deco.thickness,
                geom.strikethrough_size,
                geom.css_font_size,
                scale,
            );

            // Centre the line-through a third of the ascent above the baseline, matching
            // Chrome (which places it `2/3 * ascent` below the text-top). Parley's
            // `strikethrough_offset` (the font's `yStrikeoutPosition`) sits lower and would
            // draw the line too close to the baseline.
            let ascent =
                win_ascent(win_ascent_ratios, &geom.font, geom.font_size).unwrap_or(geom.ascent);
            let offset = ascent / 3.0 + size / 2.0;

            draw_decoration_line(
                scene,
                transform,
                scale,
                acc.min_x,
                width,
                geom.baseline,
                offset,
                size,
                &brush,
                1.0,
                deco.style,
                inset_start,
                inset_end,
            );
        }
    }
}

/// A painted glyph run (local change, for raea's text-layout report): the node whose text it
/// is, the run's ascent-to-descent box mapped to device pixels (four corners), and the clip rectangle
/// (device pixels) in force where it was painted.
#[derive(Clone, Debug)]
pub struct TextRunBox {
    pub node: NodeId,
    pub corners: [kurbo::Point; 4],
    pub clip: kurbo::Rect,
}

thread_local! {
    static TEXT_RUNS: std::cell::RefCell<Option<Vec<TextRunBox>>> = const { std::cell::RefCell::new(None) };
    pub(crate) static TEXT_CLIP: std::cell::Cell<kurbo::Rect> = const { std::cell::Cell::new(kurbo::Rect::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::INFINITY)) };
}

/// Run `f` without recording glyph runs (offscreen 3D planes paint in their own coordinates).
pub(crate) fn without_text_runs<R>(f: impl FnOnce() -> R) -> R {
    let saved = TEXT_RUNS.with(|r| r.borrow_mut().take());
    let out = f();
    TEXT_RUNS.with(|r| *r.borrow_mut() = saved);
    out
}

/// Record every glyph run painted on this thread while `f` runs.
pub fn collect_text_runs<R>(f: impl FnOnce() -> R) -> (R, Vec<TextRunBox>) {
    TEXT_RUNS.with(|r| *r.borrow_mut() = Some(Vec::new()));
    let out = f();
    let runs = TEXT_RUNS.with(|r| r.borrow_mut().take()).unwrap_or_default();
    (out, runs)
}

/// (raea) Whether a script is moving this text: it or an ancestor has a transform or translate in its inline style (GSAP
/// and other animation libraries write those every frame; a stylesheet's static transform doesn't count). A property of
/// the document, so every frame decides it the same way.
fn moved_by_script(doc: &BaseDocument, id: NodeId) -> bool {
    let mut cur = Some(id);
    while let Some(n) = cur.and_then(|i| doc.get_node(i)) {
        if doc.inline_style_moves(n.id) {
            return true;
        }
        cur = n.parent;
    }
    false
}

pub(crate) fn stroke_text<'a>(
    scene: &mut impl PaintScene,
    lines: impl Iterator<Item = Line<'a, TextBrush>>,
    doc: &BaseDocument,
    transform: Affine,
    scale: f64,
    inline_root_id: NodeId,
    context: &mut DrawTextContext,
    ellipsis: Option<&blitz_dom::node::Ellipsis>,
) {
    // (raea) text a script is moving draws from outlines at exact fractional positions (anyrender::moving_text)
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            anyrender::moving_text::set(self.0);
        }
    }
    let _restore = Restore(anyrender::moving_text::set(anyrender::moving_text::enabled() && moved_by_script(doc, inline_root_id)));
    // `text-overflow: ellipsis` (local change, raea): where each cut line's "…" goes
    let mut ellipses: Vec<(f64, f64)> = Vec::new();
    let DrawTextContext {
        stack,
        path_scratch,
        deco_boxes,
        win_ascent_ratios,
        mask,
    } = &mut *context;
    let mask = *mask;
    stack.clear();
    path_scratch.clear();
    deco_boxes.clear();

    // Persistent stack mirroring the ancestor path (inline root -> current run's
    // node) as we walk the runs. The `text-decoration-*` properties are *not*
    // inherited; instead a decoration set on an ancestor is propagated to the
    // descendant text it wraps. Rather than re-resolving styles for the whole
    // ancestor chain on every run, we cache each node's resolved values here and,
    // for each run, only resolve styles for the nodes newly descended into (popping
    // as we ascend). `path_scratch` is a reusable buffer for the run's node path.
    for line in lines {
        // Decorations accumulated for this line, keyed by decorating box, so each box is
        // painted once (spanning all its runs) using its own font — matching Firefox, which
        // draws one decoration per box rather than one stepped segment per differently-sized
        // run. Clearing preserves the allocation for the next line and inline context.
        deco_boxes.clear();

        // a line reaching past the content box keeps the glyphs that end before the "…" (Blink cuts at the
        // last character that fits whole) and the "…" follows them; lines that fit are drawn whole
        let cut = ellipsis.and_then(|e| {
            let end = line
                .items()
                .map(|item| match item {
                    PositionedLayoutItem::GlyphRun(g) => g.offset() + g.advance(),
                    PositionedLayoutItem::InlineBox(b) => b.x + b.width,
                })
                .fold(0.0f32, f32::max);
            (end > e.line_width + 0.01).then_some(e.line_width - e.width + 0.01)
        });
        let keeps = |g: &parley::Glyph| cut.is_none_or(|c| g.x + g.advance <= c);
        let mut kept_end: Option<f64> = None;

        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                let run = glyph_run.run();
                let font = run.font();
                let font_size = run.font_size();
                let metrics = run.font_metrics();
                let style = glyph_run.style();
                let synthesis = run.synthesis();
                // synthetic oblique leans forward: Chrome's SkFont::setSkewX(-1/4) (fontique's default 14
                // degrees is that angle). Upstream sheared +tan, a backward lean.
                let glyph_xform = synthesis.skew().map(|angle| {
                    let k = if (angle - 14.0).abs() < 1e-3 { 0.25 } else { angle.to_radians().tan() as f64 };
                    Affine::skew(-k, 0.0)
                });

                let css_font_size = font_size as f64 / scale;

                // Reconcile the stack with this run's ancestor path. Build the path
                // (inline root -> run node), keep the shared prefix already on the stack,
                // pop the rest, and resolve styles only for the newly-descended nodes.
                path_scratch.clear();
                let mut walk_id = Some(style.brush.id);
                while let Some(node_id) = walk_id {
                    path_scratch.push(node_id);
                    if node_id == inline_root_id {
                        break;
                    }
                    walk_id = doc.get_node(node_id).and_then(|node| node.parent);
                }
                path_scratch.reverse();

                let shared = stack
                    .iter()
                    .zip(path_scratch.iter())
                    .take_while(|(entry, node_id)| entry.node_id == **node_id)
                    .count();
                stack.truncate(shared);
                for &node_id in &path_scratch[shared..] {
                    stack.push(resolve_decoration_entry(doc, node_id));
                }

                // The glyph colour comes from the run's own node (the stack top): `color`
                // inherits, so the innermost inline element already carries the right value.
                let text_color = stack.last().map(|e| e.text_color).unwrap_or(Color::BLACK);
                let text_color = if mask { Color::BLACK } else { text_color };
                // inline opacity (local change): inline elements paint inside their inline root's
                // layout, so their (and their inline ancestors') opacity applies to the glyphs
                // here; the root's own opacity is its layer.
                let inline_opacity: f32 = if mask { 1.0 } else { stack.iter().skip(1).map(|e| e.opacity).product() };
                let text_color = if inline_opacity < 1.0 { text_color.multiply_alpha(inline_opacity) } else { text_color };

                // raea: the compiled-in emboldening only where the document asks for it (void's own window), never on
                // the pages the engine paints, which must match a render glyph for glyph
                let embolden = if FONT_EMBOLDEN_ENABLED && doc.font_embolden() {
                    let fs = font_size as f64 / scale;
                    kurbo::Vec2::new((0.015125 * fs).min(0.3), (0.0121 * fs).min(0.3))
                } else if synthesis.embolden() {
                    // CSS synthetic bold (requested weight the face doesn't have). Skia's fake bold,
                    // which Chrome uses: outline grows by size * (1/24 at 9px .. 1/32 at 36px).
                    let size = font_size as f64;
                    let k = ((size / scale).clamp(9.0, 36.0) - 9.0) / 27.0;
                    let extra = size * (1.0 / 24.0 + (1.0 / 32.0 - 1.0 / 24.0) * k);
                    kurbo::Vec2::new(extra / 2.0, extra / 2.0)
                } else {
                    kurbo::Vec2::default()
                };

                let normalized_coords: &[i16] = bytemuck::cast_slice(run.normalized_coords());

                // text-shadow: offset copies of the run under the text, last shadow
                // lowest, blurred through a Gaussian-blur layer (sigma = radius / 2).
                if let Some(styles) = doc.get_node(style.brush.id).and_then(|n| n.primary_styles()).filter(|_| !mask) {
                    let shadows = &styles.get_inherited_text().text_shadow.0;
                    if !shadows.is_empty() {
                        let current = styles.clone_color();
                        let x0 = glyph_run.offset() as f64;
                        let x1 = x0 + glyph_run.advance() as f64;
                        let baseline = glyph_run.baseline() as f64;
                        let top = baseline - metrics.ascent as f64;
                        let bottom = baseline + metrics.descent as f64;
                        for shadow in shadows.iter().rev() {
                            use blitz_dom::util::ToColorColor as _;
                            let color = shadow.color.resolve_to_absolute(&current).as_color_color();
                            let color = if inline_opacity < 1.0 { color.multiply_alpha(inline_opacity) } else { color };
                            let dx = shadow.horizontal.px() as f64 * scale;
                            let dy = shadow.vertical.px() as f64 * scale;
                            let blur = shadow.blur.px() as f64 * scale;
                            let shadow_transform = Affine::translate((dx, dy)) * transform;
                            let blurred = blur > 0.0;
                            if blurred {
                                let pad = blur * 1.5 + 2.0;
                                let clip = kurbo::Rect::new(x0 - pad, top - pad, x1 + pad, bottom + pad);
                                let filter = anyrender::filters::Filter::linear_list(std::iter::once(
                                    anyrender::filters::FilterEffect::blur((blur / 2.0) as f32),
                                ));
                                scene.push_layer(
                                    Fill::NonZero,
                                    peniko::Mix::Normal,
                                    1.0,
                                    shadow_transform,
                                    &clip,
                                    Some(std::sync::Arc::new(filter)),
                                    None,
                                );
                            }
                            scene.draw_glyphs(
                                font,
                                font_size,
                                false, // unhinted, like the glyphs (local change)
                                normalized_coords,
                                embolden,
                                Fill::NonZero,
                                &anyrender::Paint::from(color),
                                1.0,
                                shadow_transform,
                                glyph_xform,
                                glyph_run.positioned_glyphs().filter(keeps).map(|glyph| anyrender::Glyph {
                                    id: glyph.id as _,
                                    x: glyph.x,
                                    y: glyph.y,
                                }),
                            );
                            if blurred {
                                scene.pop_layer();
                            }
                        }
                    }
                }

                // -webkit-text-fill-color / -webkit-text-stroke (local change): the fill in the fill colour
                // (currentcolor unless set), the outline stroked centred on it, over the fill unless
                // paint-order names the stroke first (Chrome)
                let (fill_color, stroke, stroke_first) =
                    match doc.get_node(style.brush.id).and_then(|n| n.primary_styles()).filter(|_| !mask) {
                        Some(styles) => {
                            use blitz_dom::util::ToColorColor as _;
                            use style::values::specified::svg::PaintOrder;
                            let current = styles.clone_color();
                            let text = styles.get_inherited_text();
                            let fade = |c: Color| if inline_opacity < 1.0 { c.multiply_alpha(inline_opacity) } else { c };
                            let fill = fade(text._webkit_text_fill_color.resolve_to_absolute(&current).as_color_color());
                            let width = text._webkit_text_stroke_width.to_f64_px() * scale;
                            let stroke = (width > 0.0).then(|| {
                                (width, fade(text._webkit_text_stroke_color.resolve_to_absolute(&current).as_color_color()))
                            });
                            let order = styles.get_inherited_svg().paint_order;
                            let stroke_first = (0..3)
                                .map(|i| order.order_at(i))
                                .find(|o| !matches!(o, PaintOrder::Markers))
                                .is_some_and(|o| matches!(o, PaintOrder::Stroke));
                            (fill, stroke, stroke_first)
                        }
                        None => (text_color, None, false),
                    };
                for pass in 0..3 {
                    let (width, color) = match pass {
                        0 if stroke_first => match stroke { Some((w, c)) => (Some(w), c), None => continue },
                        1 if fill_color.components[3] > 0.0 => (None, fill_color),
                        2 if !stroke_first => match stroke { Some((w, c)) => (Some(w), c), None => continue },
                        _ => continue,
                    };
                    let glyphs = glyph_run.positioned_glyphs().filter(keeps).map(|glyph| anyrender::Glyph {
                        id: glyph.id as _,
                        x: glyph.x,
                        y: glyph.y,
                    });
                    match width {
                        Some(w) => scene.draw_glyphs(
                            font,
                            font_size,
                            false,
                            normalized_coords,
                            embolden,
                            &kurbo::Stroke::new(w),
                            &anyrender::Paint::from(color),
                            1.0,
                            transform,
                            glyph_xform,
                            glyphs,
                        ),
                        None => scene.draw_glyphs(
                            font,
                            font_size,
                            false, // hint: Chrome on macOS draws text unhinted (local change)
                            normalized_coords,
                            embolden,
                            Fill::NonZero,
                            &anyrender::Paint::from(color),
                            1.0, // alpha
                            transform,
                            glyph_xform,
                            glyphs,
                        ),
                    }
                }

                // Accumulate this run's contribution to each decorating box on its ancestor
                // path. The decoration is drawn once per box after the whole line has been
                // walked (see `flush_line_decorations`), so mixed font sizes within a box
                // produce a single straight line rather than one stepped segment per run.
                let geometry = DecorationRunGeometry {
                    baseline: glyph_run.baseline(),
                    ascent: metrics.ascent,
                    descent: metrics.descent,
                    underline_offset: metrics.underline_offset,
                    underline_size: metrics.underline_size,
                    strikethrough_size: metrics.strikethrough_size,
                    font: font.clone(),
                    font_size,
                    css_font_size,
                };
                let run_node_id = style.brush.id;
                let run_x0 = glyph_run.offset() as f64;
                let mut run_x1 = run_x0 + glyph_run.advance() as f64;
                if cut.is_some() {
                    // the run's decorations and text box end with its last kept glyph
                    let end = glyph_run.positioned_glyphs().filter(keeps).map(|g| (g.x + g.advance) as f64).fold(run_x0, f64::max);
                    run_x1 = run_x1.min(end);
                    kept_end = Some(kept_end.map_or(end, |k: f64| k.max(end)));
                }
                if !mask {
                    TEXT_RUNS.with(|r| {
                        if let Some(runs) = r.borrow_mut().as_mut() {
                            let (y0, y1) = ((glyph_run.baseline() - metrics.ascent) as f64, (glyph_run.baseline() + metrics.descent) as f64);
                            let corners = [(run_x0, y0), (run_x1, y0), (run_x1, y1), (run_x0, y1)].map(|p| transform * kurbo::Point::new(p.0, p.1));
                            runs.push(TextRunBox { node: run_node_id, corners, clip: TEXT_CLIP.with(|c| c.get()) });
                        }
                    });
                }

                for entry in stack.iter() {
                    if mask || entry.decoration.is_none() {
                        continue;
                    }
                    let idx = match deco_boxes.iter().position(|d| d.node_id == entry.node_id) {
                        Some(idx) => idx,
                        None => {
                            deco_boxes.push(LineDecoration {
                                node_id: entry.node_id,
                                deco: entry.decoration.clone().unwrap(),
                                min_x: f64::INFINITY,
                                max_x: f64::NEG_INFINITY,
                                own: None,
                                first: None,
                            });
                            deco_boxes.len() - 1
                        }
                    };
                    let acc = &mut deco_boxes[idx];
                    acc.min_x = acc.min_x.min(run_x0);
                    acc.max_x = acc.max_x.max(run_x1);
                    if acc.first.is_none() {
                        acc.first = Some(geometry.clone());
                    }
                    // A run whose innermost node is the box itself is the box's own text, so
                    // its font is the one Firefox positions and sizes the decoration with.
                    if entry.node_id == run_node_id {
                        acc.own = Some(geometry.clone());
                    }
                }
            }
        }

        flush_line_decorations(
            scene,
            transform,
            scale,
            deco_boxes,
            win_ascent_ratios,
            inline_root_id,
            line.metrics().baseline,
        );
        if cut.is_some() {
            let start = line.metrics().offset as f64;
            ellipses.push((kept_end.unwrap_or(start), line.metrics().baseline as f64));
        }
    }

    // the "…" in the container's style: its colour, shadow and decorations, as Blink paints it
    if let Some(e) = ellipsis.filter(|_| !ellipses.is_empty()) {
        let own_baseline = e.layout.lines().next().map(|l| l.metrics().baseline as f64).unwrap_or(0.0);
        for (x, baseline) in ellipses {
            let at = transform * Affine::translate((x, baseline - own_baseline));
            stroke_text(scene, e.layout.lines(), doc, at, scale, inline_root_id, context, None);
        }
    }
}

/// Draw selection highlight rectangles for the given byte range in a layout.
/// Uses Parley's Selection type for accurate geometry calculation.
pub(crate) fn draw_text_selection(
    scene: &mut impl PaintScene,
    layout: &Layout<TextBrush>,
    transform: Affine,
    selection_start: usize,
    selection_end: usize,
) {
    let anchor = Cursor::from_byte_index(layout, selection_start, Affinity::Downstream);
    let focus = Cursor::from_byte_index(layout, selection_end, Affinity::Downstream);
    let selection = Selection::new(anchor, focus);

    selection.geometry_with(layout, |rect, _line_idx| {
        let rect = kurbo::Rect::new(rect.x0, rect.y0, rect.x1, rect.y1);
        scene.fill(Fill::NonZero, transform, SELECTION_COLOR, None, &rect);
    });
}
