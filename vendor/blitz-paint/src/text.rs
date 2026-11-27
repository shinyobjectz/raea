use anyrender::PaintScene;
use blitz_dom::{BaseDocument, NodeId, node::TextBrush, util::ToColorColor};
use kurbo::{Affine, BezPath, Cap, Circle, Rect, Stroke};
use parley::{Affinity, Cursor, Layout, Line, PositionedLayoutItem, Selection};
use peniko::Fill;
use std::collections::HashMap;
use style::properties::generated::longhands::_moz_osx_font_smoothing::computed_value::T as FontSmoothing;
use style::properties::generated::longhands::visibility::computed_value::T as StyloVisibility;
use style::properties::generated::longhands::text_decoration_style::computed_value::T as TextDecorationStyle;
use style::values::computed::{
    Length, LengthPercentage, TextDecorationLength, TextDecorationLine, TextUnderlinePosition,
};
use style::values::generics::text::{GenericTextDecorationInset, GenericTextDecorationLength};

use crate::color::{Color, ToColorColor as _};
use crate::FONT_EMBOLDEN_ENABLED;

/// raea: how glyphs are drawn to match Chrome's text on macOS, as bevy-tsx's page fits it (its render/mod.rs
/// `chrome_glyph_raster`): each outline grows by `per_size` of the font size up to `max` (CSS px), light text by
/// `light_per_size` up to `light_max` more; glyphs sit at Skia's places, a quarter pixel across and whole pixels down.
/// RAEA_PAGE_GLYPH="per_size,max,light_per_size,light_max" and RAEA_PAGE_GLYPH_SNAP (any of "x", "y", "u") override it.
pub(crate) struct ChromeGlyphs {
    pub per_size: f64,
    pub max: f64,
    pub light_per_size: f64,
    pub light_max: f64,
    pub snap_x: bool,
    pub snap_y: bool,
    /// text drawn from its box's place as laid out, not as rounded ("u" in RAEA_PAGE_GLYPH_SNAP)
    pub text_unrounded: bool,
    /// glyph runs asked for hinted where they'd be emboldened (macOS, unless RAEA_PAGE_CG_YMAP=0): with raea-page's
    /// GlyphTops hook set by the same variable, each face is drawn through where CoreGraphics puts its o and H tops
    pub cg_y_map: bool,
    /// light text judged by linear luminance, as Skia judges a text colour (RAEA_PAGE_GLYPH_LINEAR=0: by its sRGB
    /// components)
    pub light_linear: bool,
    /// text under -webkit-font-smoothing: antialiased drawn ungrown, as Chrome's macOS text is there
    /// (RAEA_PAGE_FONT_SMOOTHING=0: grown as any other)
    pub font_smoothing: bool,
}

pub(crate) fn chrome_glyphs() -> &'static ChromeGlyphs {
    static G: std::sync::OnceLock<ChromeGlyphs> = std::sync::OnceLock::new();
    G.get_or_init(|| {
        let n: Vec<f64> = std::env::var("RAEA_PAGE_GLYPH").unwrap_or_default().split(',').filter_map(|x| x.trim().parse().ok()).collect();
        let [per_size, max, light_per_size, light_max] = match n[..] {
            [a, b, c, d] => [a, b, c, d],
            _ => [0.008, 0.5, 0.02, 0.4],
        };
        let snap = std::env::var("RAEA_PAGE_GLYPH_SNAP").unwrap_or_else(|_| "xyu".into());
        ChromeGlyphs {
            per_size,
            max,
            light_per_size,
            light_max,
            snap_x: snap.contains('x'),
            snap_y: snap.contains('y'),
            text_unrounded: snap.contains('u'),
            // (off macOS there's no GlyphTops hook: hinted runs would go to skrifa's hinter)
            cg_y_map: cfg!(target_vendor = "apple") && std::env::var("RAEA_PAGE_CG_YMAP").as_deref() != Ok("0"),
            light_linear: std::env::var("RAEA_PAGE_GLYPH_LINEAR").as_deref() != Ok("0"),
            font_smoothing: std::env::var("RAEA_PAGE_FONT_SMOOTHING").as_deref() != Ok("0"),
        }
    })
}

/// raea: SVG text's widening (anyrender_svg's TextLook), as this page grows its own runs' outlines: by the size, more
/// for a light colour, and synthetic bold's both edges where it's asked; the size and the widening in device px, the clamps taken as CSS px (a scale of 1, as SVG text
/// is compared against Chrome)
pub(crate) fn svg_text_look(size: f64, light: bool, fake_bold: bool) -> f64 {
    let mut d = 0.0;
    if FONT_EMBOLDEN_ENABLED {
        let g = chrome_glyphs();
        d += (g.per_size * size).min(g.max);
        if light {
            d += (g.light_per_size * size).min(g.light_max);
        }
    }
    if fake_bold && fake_bold_on() {
        d += 2.0 * fake_bold_edge(size);
    }
    d
}

/// Whether glyph runs ask Vello for hinting: off where the faces are emboldened as CoreGraphics draws them, unless
/// RAEA_PAGE_CG_YMAP takes CoreGraphics' y placement too. Not for a run drawn through a glyph transform (synthetic
/// italic's skew): CoreGraphics leaves a skewed font's glyphs where their outlines are (p13-text/inline-wrap's nested
/// bold italic: its x-height's top row half-covered in Chrome, filled through the y map)
fn hint_glyphs(skewed: bool) -> bool {
    !FONT_EMBOLDEN_ENABLED || (chrome_glyphs().cg_y_map && !skewed)
}

/// A glyph's place moved so that, through `at` (scale and translation only; anything else is left alone), it lands
/// where Skia puts it on the page: x on a quarter pixel, y on a whole one.
fn snap_glyph(x: f32, y: f32, at: Affine) -> (f32, f32) {
    let g = chrome_glyphs();
    let [a, b, c, d, _, _] = at.as_coeffs();
    if !FONT_EMBOLDEN_ENABLED || b != 0.0 || c != 0.0 || a == 0.0 || d == 0.0 || !(g.snap_x || g.snap_y) {
        return (x, y);
    }
    let p = at * kurbo::Point::new(x as f64, y as f64);
    let px = if g.snap_x { (p.x * 4.0).round() / 4.0 } else { p.x };
    let py = if g.snap_y { p.y.round() } else { p.y };
    (x + ((px - p.x) / a) as f32, y + ((py - p.y) / d) as f32)
}

/// Draw the backgrounds and borders of inline elements (e.g. `<span style="background: ...">`).
///
/// raea: each inline element on the root's lines (every element a glyph run or inline box is inside, and those that
/// only have a margin, border or padding spacer on a line) is painted as Chrome paints it: its background colour and
/// solid borders over its fragment on each line (`Node::inline_fragment_boxes`, the rects getBoundingClientRect
/// gives: its text's content area, its border and padding around it), the start edge on its first fragment and the
/// end edge on its last, outer elements first. (Blitz filled only a span's own glyph runs, without padding, borders,
/// or the text of elements inside it.)
///
/// The inline root's own background is painted separately (as a normal block box). `transform` is the root's
/// border box at device scale.
///
/// raea: its background images too (raea #248: a highlighter's `linear-gradient` on a phrase drew nothing, its colour
/// alone did), by `images` (the element, and the box they're laid out in, in the root's CSS px): each fragment clipped
/// to itself from one box as wide as all its fragments side by side, as Chrome's `box-decoration-break: slice` lays a
/// background out, so a `background-size` of 0% to 100% sweeps across the lines in turn.
pub(crate) fn draw_inline_backgrounds<'a, S: PaintScene>(
    scene: &mut S,
    lines: impl Iterator<Item = Line<'a, TextBrush>>,
    doc: &BaseDocument,
    transform: Affine,
    inline_root_id: NodeId,
    mut images: impl FnMut(&mut S, NodeId, Rect),
) {
    let mut seen: Vec<(usize, NodeId)> = Vec::new();
    let mut add = |mut id: NodeId| {
        while id != inline_root_id {
            if seen.iter().any(|&(_, s)| s == id) {
                return;
            }
            let Some(node) = doc.get_node(id) else { return };
            let depth = std::iter::successors(node.parent, |&p| doc.get_node(p).and_then(|n| n.parent)).count();
            seen.push((depth, id));
            match node.parent {
                Some(p) => id = p,
                None => return,
            }
        }
    };
    for line in lines {
        for item in line.items() {
            match item {
                PositionedLayoutItem::GlyphRun(run) => add(run.style().brush.id),
                PositionedLayoutItem::InlineBox(b) => add(blitz_dom::Spacer::node_of(b.id)),
            }
        }
    }
    seen.sort_by_key(|&(depth, _)| depth);

    for (_, id) in seen {
        let Some(node) = doc.get_node(id) else { continue };
        let Some(styles) = node.primary_styles() else { continue };
        let current_color = styles.clone_color();
        let bg = styles.get_background().background_color.resolve_to_absolute(&current_color).as_srgb_color();
        let border = styles.get_border();
        let side = |w: &style::values::computed::BorderSideWidth,
                    c: &style::values::computed::Color,
                    s: style::values::computed::BorderStyle| {
            if s != style::values::computed::BorderStyle::Solid {
                return None;
            }
            let w = w.0.to_f64_px();
            let c = c.resolve_to_absolute(&current_color).as_srgb_color();
            (w > 0.0 && c != Color::TRANSPARENT).then_some((w, c))
        };
        let [top, right, bottom, left] = [
            side(&border.border_top_width, &border.border_top_color, border.border_top_style),
            side(&border.border_right_width, &border.border_right_color, border.border_right_style),
            side(&border.border_bottom_width, &border.border_bottom_color, border.border_bottom_style),
            side(&border.border_left_width, &border.border_left_color, border.border_left_style),
        ];
        let has_images = styles.get_background().background_image.0.iter().any(|i| !matches!(i, style::values::generics::image::GenericImage::None));
        if bg == Color::TRANSPARENT && !has_images && [top, right, bottom, left].iter().all(Option::is_none) {
            continue;
        }
        let Some(frags) = node.inline_fragment_boxes() else { continue };
        let frags: Vec<_> = frags.collect();
        let n = frags.len();
        let slices = frags.iter().map(|f| (f.right - f.left) as f64).sum::<f64>();
        let mut before = 0.0;
        for (i, f) in frags.into_iter().enumerate() {
            let r = Rect::new(f.left as f64, f.top as f64, f.right as f64, f.bottom as f64);
            let (first, last) = (i == 0, i + 1 == n);
            // raea: border-radius, its start corners on the first fragment and its end corners on the last (Chrome's
            // box-decoration-break: slice), circular at the smaller of each corner's two radii
            let radius = |c: &style::values::computed::BorderCornerRadius, on: bool| {
                let w = c.0.width.0.resolve(style::values::computed::CSSPixelLength::new(r.width() as f32)).px() as f64;
                let h = c.0.height.0.resolve(style::values::computed::CSSPixelLength::new(r.height() as f32)).px() as f64;
                if on { w.min(h).max(0.0) } else { 0.0 }
            };
            let radii = kurbo::RoundedRectRadii::new(
                radius(&border.border_top_left_radius, first),
                radius(&border.border_top_right_radius, last),
                radius(&border.border_bottom_right_radius, last),
                radius(&border.border_bottom_left_radius, first),
            );
            let rounded = [radii.top_left, radii.top_right, radii.bottom_right, radii.bottom_left].iter().any(|&v| v > 0.0);
            let shape = kurbo::RoundedRect::from_rect(r, radii);
            if bg != Color::TRANSPARENT {
                scene.fill(Fill::NonZero, transform, bg, None, &shape);
            }
            if has_images {
                scene.push_clip_layer(Fill::NonZero, transform, &shape);
                images(scene, id, Rect::new(r.x0 - before, r.y0, r.x0 - before + slices, r.y1));
                scene.pop_layer();
            }
            before += r.width();
            if rounded {
                scene.push_clip_layer(Fill::NonZero, transform, &shape);
            }
            let mut edge = |s: Option<(f64, Color)>, on: bool, rect: Rect| {
                if let (Some((_, c)), true) = (s, on) {
                    scene.fill(Fill::NonZero, transform, c, None, &rect);
                }
            };
            if let Some((w, _)) = top {
                edge(top, true, Rect::new(r.x0, r.y0, r.x1, r.y0 + w));
            }
            if let Some((w, _)) = bottom {
                edge(bottom, true, Rect::new(r.x0, r.y1 - w, r.x1, r.y1));
            }
            if let Some((w, _)) = left {
                edge(left, first, Rect::new(r.x0, r.y0, r.x0 + w, r.y1));
            }
            if let Some((w, _)) = right {
                edge(right, last, Rect::new(r.x1 - w, r.y0, r.x1, r.y1));
            }
            if rounded {
                scene.pop_layer();
            }
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
    /// raea: the node's own `opacity`, which an inline box applies to the text and decorations under it
    opacity: f32,
    /// raea: its `text-shadow` (inherited), each (x, y, blur) in CSS px and its colour, first on top
    shadows: Vec<(f64, f64, f64, Color)>,
    /// The decoration this node introduces as a decorating box, if any.
    decoration: Option<ResolvedDecoration>,
}

/// Resolve the cached style values for a single node into a [`DecorationStackEntry`].
fn resolve_decoration_entry(doc: &BaseDocument, node_id: NodeId) -> DecorationStackEntry {
    let Some(styles) = doc.get_node(node_id).and_then(|node| node.primary_styles()) else {
        return DecorationStackEntry {
            node_id,
            text_color: Color::BLACK,
            opacity: 1.0,
            shadows: Vec::new(),
            decoration: None,
        };
    };

    let itext = styles.get_inherited_text();
    let text = styles.get_text();
    let text_color = itext.color.as_color_color();
    let opacity = styles.get_effects().opacity;
    let shadows = itext
        .text_shadow
        .0
        .iter()
        .map(|sh| {
            let color = sh.color.resolve_to_absolute(&itext.color).as_color_color();
            (sh.horizontal.px() as f64, sh.vertical.px() as f64, sh.blur.px() as f64, color)
        })
        .collect();

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
        opacity,
        shadows,
        decoration,
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
    /// raea: the font's strikethrough offset above the baseline, where bevy_text puts the line (a bevy_ui <Text>)
    strikethrough_offset: f32,
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
    /// raea: the opacity of the inline boxes down to the decorating box (see stroke_text)
    alpha: f32,
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
    /// raea: a colour every glyph is drawn in instead of its node's (a field's placeholder, its ::placeholder colour)
    pub(crate) color: Option<Color>,
    /// raea: text-overflow: ellipsis, where a clipping box's content ends (layout units): a line running past it is cut
    /// to fit an ellipsis before it
    pub(crate) ellipsis: Option<f32>,
    /// raea: -webkit-line-clamp, the index of the last line drawn and where the box's content ends (layout units): that
    /// line ends in an ellipsis, after its text or cutting what doesn't fit before it, as Chrome's clamp does
    pub(crate) clamp: Option<(usize, f32)>,
    /// raea: the glyphs as a mask (background-clip: text, #37): in `color`, without their text-shadow
    pub(crate) mask: bool,
}

/// raea: synthetic bold as Chrome draws it on macOS, in glyph space (it grows with the text): a weight of 600 or more
/// on a face whose own weight (OS/2) is under 600 and which has no weight axis, each edge pushed out by
/// fake_bold_edge. fontique's own embolden isn't the test: it rated a one-face family's 500 face 700, and thickened
/// only 900. `size` is the run's size in glyph space, `css_size` in CSS px. RAEA_PAGE_FAKE_BOLD=0 leaves it out
fn synthetic_bold(doc: &BaseDocument, id: NodeId, font: &peniko::FontData, size: f64, css_size: f64) -> kurbo::Vec2 {
    if !fake_bold_on() {
        return kurbo::Vec2::default();
    }
    use skrifa::MetadataProvider as _;
    let wanted = doc.get_node(id).and_then(|n| n.primary_styles()).map_or(400.0, |s| s.get_font().font_weight.value());
    if wanted < 600.0 {
        return kurbo::Vec2::default();
    }
    let Ok(face) = skrifa::FontRef::from_index(font.data.as_ref(), font.index) else { return kurbo::Vec2::default() };
    if face.attributes().weight.value() >= 600.0 || face.axes().iter().any(|a| a.tag() == skrifa::Tag::new(b"wght")) {
        return kurbo::Vec2::default();
    }
    let e = fake_bold_edge(css_size) * size / css_size.max(1e-6);
    kurbo::Vec2::new(e, e)
}

fn fake_bold_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("RAEA_PAGE_FAKE_BOLD").as_deref() != Ok("0"))
}

/// raea: how far synthetic bold pushes each edge out, in CSS px, for a size in CSS px: measured against the parity
/// Chrome on macOS (a one-face 500 mono drawn at 600, its ink over 500's: none at 16 px, 6.1% at 24, 8.4% at 44, met
/// by a push of 0, 0.0030 and 0.0041 of the size), straight between them and held past them. Not Skia's
/// size/24..size/32 stroke, which drew 600 a third heavier than Chrome at 44 px
pub(crate) fn fake_bold_edge(size: f64) -> f64 {
    let share = if size <= 16.0 {
        0.0
    } else if size <= 24.0 {
        0.0030 * (size - 16.0) / 8.0
    } else {
        0.0030 + (0.0041 - 0.0030) * ((size - 24.0) / 20.0).min(1.0)
    };
    share * size
}

/// The ellipsis in `font` at `size`, its glyph, how many times it's drawn and its advance: U+2026 in the cut run's font,
/// or three full stops where the font has none, as Chrome draws it.
fn ellipsis_glyph(font: &peniko::FontData, size: f32) -> Option<(u32, u32, f32)> {
    use skrifa::MetadataProvider as _;
    let font_ref = skrifa::FontRef::from_index(font.data.as_ref(), font.index).ok()?;
    let charmap = font_ref.charmap();
    let (id, n) = charmap.map('\u{2026}').map(|id| (id, 1)).or_else(|| charmap.map('.').map(|id| (id, 3)))?;
    let advance = font_ref.glyph_metrics(skrifa::instance::Size::new(size), skrifa::instance::LocationRef::default()).advance_width(id)?;
    Some((id.to_u32(), n, advance))
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
    bevy: bool,
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
        let brush = anyrender::Paint::from(deco.color.multiply_alpha(acc.alpha));

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
            // raea: in a bevy_ui <Text> (bevy), the font's thickness as bevy_text draws it, not floored to a pixel
            let size = match bevy {
                true => geom.underline_size,
                false => decoration_size(&deco.thickness, geom.underline_size, geom.css_font_size, scale),
            };

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
            let size = match bevy {
                true => geom.strikethrough_size,
                false => decoration_size(&deco.thickness, geom.strikethrough_size, geom.css_font_size, scale),
            };

            // Centre the line-through a third of the ascent above the baseline, matching
            // Chrome (which places it `2/3 * ascent` below the text-top). Parley's
            // `strikethrough_offset` (the font's `yStrikeoutPosition`) sits lower and would
            // draw the line too close to the baseline.
            // raea: a bevy_ui <Text> puts it where bevy_text does, its top at that offset above the baseline
            // (pipeline.rs: strikethrough_y = baseline - strikethrough_offset)
            let offset = match bevy {
                true => geom.strikethrough_offset,
                false => win_ascent(win_ascent_ratios, &geom.font, geom.font_size).unwrap_or(geom.ascent) / 3.0 + size / 2.0,
            };

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

pub(crate) fn stroke_text<'a>(
    scene: &mut impl PaintScene,
    lines: impl Iterator<Item = Line<'a, TextBrush>>,
    doc: &BaseDocument,
    transform: Affine,
    scale: f64,
    inline_root_id: NodeId,
    context: &mut DrawTextContext,
) {
    let DrawTextContext {
        stack,
        path_scratch,
        deco_boxes,
        win_ascent_ratios,
        color: color_override,
        ellipsis,
        clamp,
        mask,
    } = context;
    let mask = *mask;
    let color_override = *color_override;
    let ellipsis = *ellipsis;
    let clamp = *clamp;
    stack.clear();
    path_scratch.clear();
    deco_boxes.clear();
    // raea: a bevy_ui <Text>'s block (blitz-dom's Node::is_bevy_text): its lines drawn as bevy_text draws them
    let bevy = doc.get_node(inline_root_id).is_some_and(|n| n.is_bevy_text());

    // Persistent stack mirroring the ancestor path (inline root -> current run's
    // node) as we walk the runs. The `text-decoration-*` properties are *not*
    // inherited; instead a decoration set on an ancestor is propagated to the
    // descendant text it wraps. Rather than re-resolving styles for the whole
    // ancestor chain on every run, we cache each node's resolved values here and,
    // for each run, only resolve styles for the nodes newly descended into (popping
    // as we ascend). `path_scratch` is a reusable buffer for the run's node path.
    for (index, line) in lines.enumerate() {
        // Decorations accumulated for this line, keyed by decorating box, so each box is
        // painted once (spanning all its runs) using its own font — matching Firefox, which
        // draws one decoration per box rather than one stepped segment per differently-sized
        // run. Clearing preserves the allocation for the next line and inline context.
        deco_boxes.clear();

        // raea: a line running past an ellipsis box's end keeps the glyphs that fit before the ellipsis; the ellipsis
        // is drawn after the last one, in the font of the run the cut falls in
        let runs = || line.items().filter_map(|i| match i {
            PositionedLayoutItem::GlyphRun(r) => Some(r),
            _ => None,
        });
        // raea: a clamp's last line: the ellipsis after its text (its trailing spaces left out), or where it fits
        let clamped = clamp.filter(|(last, _)| index == *last).and_then(|(_, end)| {
            let at = runs().last()?;
            let spaces: f32 = at.run().visual_clusters().collect::<Vec<_>>().into_iter().rev()
                .take_while(|c| c.is_space_or_nbsp() || c.is_hard_line_break()).map(|c| c.advance()).sum();
            let text_end = at.offset() + at.advance() - spaces;
            let (id, n, advance) = ellipsis_glyph(at.run().font(), at.run().font_size())?;
            Some((text_end.min(end - advance * n as f32), id, n, advance))
        });
        let cut = clamped.or_else(|| ellipsis.and_then(|end| {
            let runs = || line.items().filter_map(|i| match i {
                PositionedLayoutItem::GlyphRun(r) => Some(r),
                _ => None,
            });
            let over = runs().any(|r| r.offset() + r.advance() > end + 0.5);
            if !over {
                return None;
            }
            let at = runs().filter(|r| r.offset() < end).last()?;
            let (id, n, advance) = ellipsis_glyph(at.run().font(), at.run().font_size())?;
            Some((end - advance * n as f32, id, n, advance))
        }));
        // (the kept text's end, its baseline, font, size, colour, opacity, outline growth and coords)
        let mut kept: Option<(f32, f32, peniko::FontData, f32, Color, f32, kurbo::Vec2, Vec<i16>)> = None;

        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                if let Some((limit, ..)) = cut {
                    if glyph_run.offset() >= limit {
                        continue;
                    }
                }
                let fits = |g: &parley::Glyph| cut.is_none_or(|(limit, ..)| g.x + g.advance <= limit + 0.01);
                let run = glyph_run.run();
                let font = run.font();
                let font_size = run.font_size();
                let metrics = run.font_metrics();
                let style = glyph_run.style();
                // raea: a run whose element is visibility: hidden (or collapse) isn't drawn, though it keeps its place
                // in the line, as Chrome draws it: `visibility` inherits, so the run's own element says (a visible
                // child of a hidden span draws). Blitz hid only boxes, so a hidden inline word drew (#13, word-flip's
                // slot strut)
                if doc.get_node(style.brush.id).and_then(|n| n.primary_styles()).is_some_and(|s| s.get_inherited_box().visibility != StyloVisibility::Visible) {
                    continue;
                }
                let _painting = crate::Painting::enter(style.brush.id);
                let synthesis = run.synthesis();
                // raea: a bevy_ui <Text> draws its glyphs upright, as bevy_text does: it takes no synthetic italic's
                // skew from parley (an Italic or Oblique run of a face with no slant drawn as its upright face)
                let glyph_xform = synthesis
                    .skew()
                    .filter(|_| !bevy)
                    .map(|angle| Affine::skew(angle.to_radians().tan() as f64, 0.0));

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
                let text_color = color_override.or(stack.last().map(|e| e.text_color)).unwrap_or(Color::BLACK);
                // raea: the inline boxes' opacity between the inline root (whose own opacity is its layer's) and
                // the run, multiplied: Blitz drew a `<span style="opacity: 0.14">`'s text at full strength
                let alpha: f32 = stack.iter().skip(1).map(|e| e.opacity).product();

                // raea: Chrome's macOS text (CoreText's font smoothing through Skia) as bevy-tsx's page fits it
                // (engine/bevy-tsx/src/render/mod.rs): every outline grown by a share of its size, light text more
                // raea: none under -webkit-font-smoothing: antialiased (stylo's grayscale), where Chrome's macOS text
                // isn't smoothed (feed's app screens: Geist 500 at 12px drawn heavy); RAEA_PAGE_FONT_SMOOTHING=0
                // grows it anyway
                let grayscale = chrome_glyphs().font_smoothing
                    && doc.get_node(style.brush.id).and_then(|n| n.primary_styles()).is_some_and(|s| {
                        s.get_font().clone__moz_osx_font_smoothing() == FontSmoothing::Grayscale
                    });
                let embolden = if FONT_EMBOLDEN_ENABLED && !grayscale {
                    let g = chrome_glyphs();
                    let fs = font_size as f64 / scale;
                    let c = text_color.components;
                    // (light by linear luminance, as Skia judges a text colour: ui-parity app's grey ink-3 on dark is
                    // drawn thinner in Chrome than the light extra gives)
                    let c = if g.light_linear { text_color.convert::<::color::LinearSrgb>().components } else { c };
                    let light = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2] > 0.5;
                    let mut d = (g.per_size * fs).min(g.max);
                    if light {
                        d += (g.light_per_size * fs).min(g.light_max);
                    }
                    // (kurbo's expand moves each edge out by the amount; bevy_text grows a glyph by `d` in all). The
                    // outline is grown before the run's transform: a scaled element's text grows by `d` on screen, as
                    // CoreText's does, not by its scale times `d` (a 40x title's glyph a wide band)
                    let k = transform.determinant().abs().sqrt();
                    let e = if k > 1e-6 { 0.5 * d * scale / k } else { 0.0 };
                    kurbo::Vec2::new(e, e)
                } else {
                    kurbo::Vec2::default()
                };
                let embolden = embolden + synthetic_bold(doc, style.brush.id, font, font_size as f64, css_font_size);
                let place = |x: f32, y: f32, at: Affine| snap_glyph(x, y, at);

                let normalized_coords: &[i16] = bytemuck::cast_slice(run.normalized_coords());
                // raea: text-shadow, under the run's glyphs, the last shadow first so the first is on top; a blurred
                // one drawn in a layer the blur filter runs on (blitz-paint drew no text shadows)
                let shadows = stack.last().filter(|_| !mask).map(|e| e.shadows.as_slice()).unwrap_or(&[]);
                for &(dx, dy, blur, color) in shadows.iter().rev() {
                    if color.components[3] == 0.0 {
                        continue;
                    }
                    let at = transform.pre_translate(kurbo::Vec2::new(dx * scale, dy * scale)); // in the run's axes (#18)
                    let sigma = blur * scale / 2.0;
                    if sigma > 0.0 {
                        let reach = sigma * 3.0;
                        let x0 = glyph_run.offset() as f64;
                        let base = glyph_run.baseline() as f64;
                        let clip = Rect::new(
                            x0 - reach,
                            base - metrics.ascent as f64 - reach,
                            x0 + glyph_run.advance() as f64 + reach,
                            base + metrics.descent as f64 + reach,
                        );
                        let filter = anyrender::Filter::single(anyrender::filters::FilterEffect::blur(sigma as f32));
                        scene.push_layer(Fill::NonZero, peniko::BlendMode::default(), 1.0, at, &clip, Some(std::sync::Arc::new(filter)), None);
                    }
                    scene.draw_glyphs(
                        font,
                        font_size,
                        hint_glyphs(glyph_xform.is_some()),
                        normalized_coords,
                        embolden,
                        Fill::NonZero,
                        &anyrender::Paint::from(color),
                        alpha,
                        at,
                        glyph_xform,
                        glyph_run.positioned_glyphs().filter(|g| fits(g)).map(|glyph| {
                            let (x, y) = place(glyph.x, glyph.y, at);
                            anyrender::Glyph { id: glyph.id as _, x, y }
                        }),
                    );
                    if sigma > 0.0 {
                        scene.pop_layer();
                    }
                }
                scene.draw_glyphs(
                    font,
                    font_size,
                    hint_glyphs(glyph_xform.is_some()),
                    normalized_coords,
                    embolden,
                    Fill::NonZero,
                    &anyrender::Paint::from(text_color),
                    alpha,
                    transform,
                    glyph_xform,
                    glyph_run.positioned_glyphs().filter(|g| fits(g)).map(|glyph| {
                        let (x, y) = place(glyph.x, glyph.y, transform);
                        anyrender::Glyph { id: glyph.id as _, x, y }
                    }),
                );
                // raea: the hyphen a line ends with at a soft hyphen, in this run's font and colour (parley
                // GlyphRun::raea_hyphen)
                if let Some(hyphen) = glyph_run.raea_hyphen() {
                    // in the soft hyphen's own colour and font (a transparent span around it hides it; a larger one
                    // draws it larger)
                    let hyphen_color = match hyphen.brush.id == style.brush.id {
                        true => text_color,
                        false => color_override.unwrap_or_else(|| resolve_decoration_entry(doc, hyphen.brush.id).text_color),
                    };
                    let hyphen_coords: &[i16] = bytemuck::cast_slice(hyphen.normalized_coords);
                    scene.draw_glyphs(
                        hyphen.font,
                        hyphen.font_size,
                        hint_glyphs(glyph_xform.is_some()),
                        hyphen_coords,
                        embolden,
                        Fill::NonZero,
                        &anyrender::Paint::from(hyphen_color),
                        alpha,
                        transform,
                        glyph_xform,
                        hyphen.glyphs.iter().filter(|g| fits(g)).map(|glyph| {
                            let (x, y) = place(glyph.x, glyph.y, transform);
                            anyrender::Glyph { id: glyph.id as _, x, y }
                        }),
                    );
                }
                if cut.is_some() {
                    let end = glyph_run.positioned_glyphs().filter(|g| fits(g)).map(|g| g.x + g.advance).fold(f32::NEG_INFINITY, f32::max);
                    if end.is_finite() && kept.as_ref().is_none_or(|k| end >= k.0) {
                        kept = Some((end, glyph_run.baseline(), font.clone(), font_size, text_color, alpha, embolden, normalized_coords.to_vec()));
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
                    strikethrough_offset: metrics.strikethrough_offset,
                    font: font.clone(),
                    font_size,
                    css_font_size,
                };
                let run_node_id = style.brush.id;
                let run_x0 = glyph_run.offset() as f64;
                let run_x1 = run_x0 + glyph_run.advance() as f64;

                let mut box_alpha = 1.0f32;
                for (depth, entry) in stack.iter().enumerate() {
                    if depth > 0 {
                        box_alpha *= entry.opacity;
                    }
                    if entry.decoration.is_none() {
                        continue;
                    }
                    let idx = match deco_boxes.iter().position(|d| d.node_id == entry.node_id) {
                        Some(idx) => idx,
                        None => {
                            deco_boxes.push(LineDecoration {
                                node_id: entry.node_id,
                                deco: entry.decoration.clone().unwrap(),
                                alpha: box_alpha,
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

        if let (Some((_, id, n, advance)), Some((x, baseline, font, size, color, alpha, embolden, coords))) = (cut, kept.take()) {
            scene.draw_glyphs(
                &font,
                size,
                hint_glyphs(false),
                &coords,
                embolden,
                Fill::NonZero,
                &anyrender::Paint::from(color),
                alpha,
                transform,
                None,
                (0..n).map(|i| {
                    let (x, y) = snap_glyph(x + advance * i as f32, baseline, transform);
                    anyrender::Glyph { id, x, y }
                }),
            );
        }

        flush_line_decorations(
            scene,
            transform,
            scale,
            deco_boxes,
            win_ascent_ratios,
            inline_root_id,
            line.metrics().baseline,
            bevy,
        );
    }
}

/// Draw selection highlight rectangles for the given byte range in a layout.
/// Uses Parley's Selection type for accurate geometry calculation.
/// raea: in `color` (the ::selection background), returning the rects as one path (layout units) so the selected
/// glyphs can be drawn again inside them in the ::selection colour
pub(crate) fn draw_text_selection(
    scene: &mut impl PaintScene,
    layout: &Layout<TextBrush>,
    transform: Affine,
    selection_start: usize,
    selection_end: usize,
    color: Color,
) -> kurbo::BezPath {
    let anchor = Cursor::from_byte_index(layout, selection_start, Affinity::Downstream);
    let focus = Cursor::from_byte_index(layout, selection_end, Affinity::Downstream);
    let selection = Selection::new(anchor, focus);

    let mut path = kurbo::BezPath::new();
    selection.geometry_with(layout, |rect, _line_idx| {
        let rect = kurbo::Rect::new(rect.x0, rect.y0, rect.x1, rect.y1);
        scene.fill(Fill::NonZero, transform, color, None, &rect);
        kurbo::Shape::path_elements(&rect, 0.1).for_each(|e| path.push(e));
    });
    path
}
