//! `<canvas>` 2D drawing (M6): the shim's CanvasRenderingContext2D records calls; they are
//! replayed here onto one Skia raster surface per canvas, the rasterizer Chrome's canvas uses, with
//! the HTML spec's semantics (path points transformed by the current matrix as they are added,
//! state stack, compositing, shadows that ignore the transform). The bitmap is then shown as the
//! canvas element's image.

// skia-safe 0.153 deprecates gradient_shader in favour of a newer module; the old API is what
// anyrender_skia uses too
#![allow(deprecated)]

use std::collections::HashMap;

use serde_json::Value;
use skia_safe::{
    self as sk, gradient_shader, image_filters, paint, BlendMode, ClipOp, Color4f, Data, Font, FontMgr, FontStyle,
    ImageInfo, Matrix, Paint, PathBuilder, Point, Rect, Shader, Surface, TileMode,
};

#[derive(Clone)]
enum Style {
    Color(Color4f),
    Shader(Value),
    /// `createPattern`: the source bitmap and its tile modes (x, y)
    Pattern(sk::Image, sk::TileMode, sk::TileMode),
}

#[derive(Clone)]
struct State {
    fill: Style,
    stroke: Style,
    line_width: f32,
    cap: paint::Cap,
    join: paint::Join,
    miter: f32,
    dash: Vec<f32>,
    dash_offset: f32,
    alpha: f32,
    blend: BlendMode,
    shadow_color: Color4f,
    shadow_blur: f32,
    shadow_x: f32,
    shadow_y: f32,
    font: String,
    align: String,
    baseline: String,
    smoothing: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            fill: Style::Color(Color4f::new(0.0, 0.0, 0.0, 1.0)),
            stroke: Style::Color(Color4f::new(0.0, 0.0, 0.0, 1.0)),
            line_width: 1.0,
            cap: paint::Cap::Butt,
            join: paint::Join::Miter,
            miter: 10.0,
            dash: Vec::new(),
            dash_offset: 0.0,
            alpha: 1.0,
            blend: BlendMode::SrcOver,
            shadow_color: Color4f::new(0.0, 0.0, 0.0, 0.0),
            shadow_blur: 0.0,
            shadow_x: 0.0,
            shadow_y: 0.0,
            font: "10px sans-serif".into(),
            align: "start".into(),
            baseline: "alphabetic".into(),
            smoothing: true,
        }
    }
}

/// One canvas: its surface, the 2D state and the current path (device space).
pub struct Canvas2D {
    surface: Surface,
    width: i32,
    height: i32,
    state: State,
    stack: Vec<State>,
    path: PathBuilder,
    has_point: bool,
    fonts: FontCache,
    /// @font-face families Chrome hasn't loaded yet when this replay happens (lowercased): text falls
    /// back past them, to the default serif when none is left (see Composition::canvas_font_pending)
    pub pending_fonts: Option<std::collections::HashSet<String>>,
}

/// Images `drawImage` can use: element ref -> (width, height, straight-alpha RGBA8).
pub type ImageLookup<'a> = &'a dyn Fn(u64) -> Option<(u32, u32, Vec<u8>)>;

/// The image source an op refers to (`{ref, src?}`): `drawImage`'s first argument, or a `createPattern`
/// fill/stroke style's source.
pub fn op_image_source(op: &Value) -> Option<&Value> {
    match op.get(0).and_then(Value::as_str)? {
        "drawImage" => op.get(1),
        "fillStyle" | "strokeStyle" => op.get(1)?.get("pattern")?.get("src"),
        _ => None,
    }
}

/// `createPattern(source, repetition)` recorded as `{src: {ref, src?}, rep}`.
fn pattern_style(pat: &Value, images: ImageLookup<'_>) -> Option<Style> {
    let (w, h, px) = images(pat["src"]["ref"].as_u64()?)?;
    let info = ImageInfo::new((w as i32, h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
    let img = sk::images::raster_from_data(&info, sk::Data::new_copy(&px), w as usize * 4)?;
    let (r, d) = (sk::TileMode::Repeat, sk::TileMode::Decal);
    let (tx, ty) = match pat["rep"].as_str().unwrap_or("repeat") {
        "repeat-x" => (r, d),
        "repeat-y" => (d, r),
        "no-repeat" => (d, d),
        _ => (r, r),
    };
    Some(Style::Pattern(img, tx, ty))
}

fn color(v: &str) -> Option<Color4f> {
    let v = v.trim();
    if v.eq_ignore_ascii_case("transparent") {
        return Some(Color4f::new(0.0, 0.0, 0.0, 0.0));
    }
    let c = csscolorparser::parse(v).ok()?;
    Some(Color4f::new(c.r, c.g, c.b, c.a))
}

fn num(v: &Value, i: usize) -> f32 {
    v.get(i).and_then(Value::as_f64).unwrap_or(0.0) as f32
}

fn blend(op: &str) -> BlendMode {
    match op {
        "source-in" => BlendMode::SrcIn,
        "source-out" => BlendMode::SrcOut,
        "source-atop" => BlendMode::SrcATop,
        "destination-over" => BlendMode::DstOver,
        "destination-in" => BlendMode::DstIn,
        "destination-out" => BlendMode::DstOut,
        "destination-atop" => BlendMode::DstATop,
        "lighter" => BlendMode::Plus,
        "copy" => BlendMode::Src,
        "xor" => BlendMode::Xor,
        "multiply" => BlendMode::Multiply,
        "screen" => BlendMode::Screen,
        "overlay" => BlendMode::Overlay,
        "darken" => BlendMode::Darken,
        "lighten" => BlendMode::Lighten,
        "color-dodge" => BlendMode::ColorDodge,
        "color-burn" => BlendMode::ColorBurn,
        "hard-light" => BlendMode::HardLight,
        "soft-light" => BlendMode::SoftLight,
        "difference" => BlendMode::Difference,
        "exclusion" => BlendMode::Exclusion,
        "hue" => BlendMode::Hue,
        "saturation" => BlendMode::Saturation,
        "color" => BlendMode::Color,
        "luminosity" => BlendMode::Luminosity,
        _ => BlendMode::SrcOver,
    }
}

/// CSS `font` shorthand -> (families, weight, italic, size px).
fn parse_font(font: &str) -> (Vec<String>, u16, bool, f32) {
    let mut weight = 400u16;
    let mut italic = false;
    let mut size = 10.0f32;
    let mut rest = font.trim();
    loop {
        let (word, tail) = match rest.find(char::is_whitespace) {
            Some(i) => (&rest[..i], rest[i..].trim_start()),
            None => (rest, ""),
        };
        let w = word.to_ascii_lowercase();
        match w.as_str() {
            "italic" | "oblique" => italic = true,
            "normal" | "small-caps" => {}
            "bold" | "bolder" => weight = 700,
            "lighter" => weight = 300,
            _ if w.parse::<u16>().is_ok() => weight = w.parse().unwrap_or(400),
            _ => {
                // size[/line-height] then families
                let size_part = word.split('/').next().unwrap_or("");
                if let Some(px) = size_part.strip_suffix("px") {
                    size = px.parse().unwrap_or(10.0);
                } else if let Some(pt) = size_part.strip_suffix("pt") {
                    size = pt.parse::<f32>().unwrap_or(7.5) * 4.0 / 3.0;
                } else if let Some(em) = size_part.strip_suffix("em").or_else(|| size_part.strip_suffix("rem")) {
                    size = em.parse::<f32>().unwrap_or(0.625) * 16.0;
                }
                let families = tail.split(',').map(|f| f.trim().trim_matches(['"', '\'']).to_string()).filter(|f| !f.is_empty()).collect();
                return (families, weight, italic, size);
            }
        }
        if tail.is_empty() {
            return (vec!["sans-serif".into()], weight, italic, size);
        }
        rest = tail;
    }
}


/// A face for canvas text: Skia's typeface plus the font file when we have it (for shaping).
/// (typeface, its font file for shaping, the wght a variable face is drawn at)
type Face = (sk::Typeface, Option<std::sync::Arc<Vec<u8>>>, Option<f32>);
type FontCache = HashMap<(String, u16, bool), Option<Face>>;

/// Shape `text` like Chrome does (HarfBuzz rules: kerning, ligatures): glyphs, pen positions,
/// advance width, at `size` px.
fn shape(ttf: &[u8], text: &str, size: f32, wght: Option<f32>) -> Option<(Vec<sk::GlyphId>, Vec<Point>, f32)> {
    let font = harfrust::FontRef::new(ttf).ok()?;
    let data = harfrust::ShaperData::new(&font);
    let instance = wght.map(|v| {
        harfrust::ShaperInstance::from_variations(&font, [harfrust::Variation { tag: harfrust::Tag::new(b"wght"), value: v }])
    });
    let shaper = data.shaper(&font).instance(instance.as_ref()).build();
    let mut buffer = harfrust::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let out = shaper.shape(buffer, harfrust::ShapeOptions::new());
    let scale = size / shaper.units_per_em().max(1) as f32;
    let (mut glyphs, mut points, mut x) = (Vec::new(), Vec::new(), 0.0f32);
    for (info, pos) in out.glyph_infos().iter().zip(out.glyph_positions()) {
        glyphs.push(info.glyph_id as sk::GlyphId);
        points.push(Point::new(x + pos.x_offset as f32 * scale, -(pos.y_offset as f32) * scale));
        x += pos.x_advance as f32 * scale;
    }
    Some((glyphs, points, x))
}

/// The face canvas text uses: the composition format's bundled aliases first (as in Chrome via its injected
/// @font-face rules), then system fonts.
fn typeface_for(cache: &mut FontCache, families: &[String], weight: u16, italic: bool) -> Option<Face> {
    for fam in families {
        let key = (fam.to_ascii_lowercase(), weight, italic);
        if let Some(t) = cache.get(&key) {
            if t.is_some() {
                return t.clone();
            }
            continue;
        }
        // the document's own @font-face rules first, as Chrome matches them (local change)
        if let Some(face) = document_face(fam, weight, italic) {
            cache.insert(key, Some(face.clone()));
            return Some(face);
        }
        let bundled = crate::fonts::canvas_face(fam, weight, italic).and_then(|woff2| wuff::decompress_woff2(woff2).ok());
        let face = bundled
            .and_then(|ttf| {
                let tf = FontMgr::new().new_from_data(Data::new_copy(&ttf), None)?;
                Some((tf, Some(std::sync::Arc::new(ttf)), None))
            })
            .or_else(|| {
                let style = FontStyle::new(
                    sk::font_style::Weight::from(weight as i32),
                    sk::font_style::Width::NORMAL,
                    if italic { sk::font_style::Slant::Italic } else { sk::font_style::Slant::Upright },
                );
                let generic = match key.0.as_str() {
                    "sans-serif" | "system-ui" => "Helvetica",
                    "serif" => "Times",
                    "monospace" => "Courier",
                    other => other,
                };
                FontMgr::new().match_family_style(generic, style).map(|tf| (tf, None, None))
            });
        cache.insert(key, face.clone());
        if face.is_some() {
            return face;
        }
    }
    None
}

thread_local! {
    static MEASURE_FONTS: std::cell::RefCell<FontCache> = std::cell::RefCell::new(HashMap::new());
    static DOC_FONTS: std::cell::RefCell<Option<DocFonts>> = const { std::cell::RefCell::new(None) };
}

/// One `@font-face` rule of the document: family (lower case), source URL, weight range, italic, whether its
/// unicode-range covers Basic Latin.
#[derive(Clone, Debug)]
struct FaceRule {
    family: String,
    src: String,
    weight: (u16, u16),
    italic: bool,
    latin: bool,
}

struct DocFonts {
    rules: Vec<FaceRule>,
    net: std::sync::Arc<crate::net::SyncNet>,
    base: Option<url::Url>,
    loaded: HashMap<String, Option<std::sync::Arc<Vec<u8>>>>,
}

/// The document's `@font-face` rules for canvas text drawn on this thread (the scene sets them before the
/// page's scripts run): Chrome matches canvas fonts against the page's font faces like any text.
pub fn set_document_fonts(css: &[String], net: std::sync::Arc<crate::net::SyncNet>, base: Option<url::Url>) {
    let rules = css.iter().flat_map(|c| font_face_rules(c)).collect();
    DOC_FONTS.with(|d| *d.borrow_mut() = Some(DocFonts { rules, net, base, loaded: HashMap::new() }));
    MEASURE_FONTS.with(|c| c.borrow_mut().clear());
}

fn font_face_rules(css: &str) -> Vec<FaceRule> {
    let mut out = Vec::new();
    for block in css.split("@font-face").skip(1) {
        let Some(open) = block.find('{') else { continue };
        let body = &block[open + 1..];
        let body = &body[..body.find('}').unwrap_or(body.len())];
        // a descriptor's value (none but src holds a ';', and src is read separately)
        let desc = |name: &str| -> Option<String> {
            let mut from = 0;
            while let Some(i) = body[from..].find(name).map(|i| i + from) {
                let before = body[..i].chars().last();
                let after = body[i + name.len()..].trim_start();
                if !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-') && after.starts_with(':') {
                    let v = &after[1..];
                    return Some(v[..v.find(';').unwrap_or(v.len())].trim().to_string());
                }
                from = i + name.len();
            }
            None
        };
        let Some(family) = desc("font-family") else { continue };
        let family = family.trim().trim_matches(|c| c == '"' || c == '\'').trim().to_ascii_lowercase();
        let Some(si) = body.find("src") else { continue };
        let Some(ui) = body[si..].find("url(") else { continue };
        let u = &body[si + ui + 4..];
        let Some(end) = u.find(')') else { continue };
        let src = u[..end].trim().trim_matches(|c| c == '"' || c == '\'').to_string();
        let weight = desc("font-weight").map(|w| weight_range(&w)).unwrap_or((400, 400));
        let italic = desc("font-style").is_some_and(|s| s.contains("italic") || s.contains("oblique"));
        let latin = desc("unicode-range").is_none_or(|r| range_covers(&r, 0x41));
        out.push(FaceRule { family, src, weight, italic, latin });
    }
    out
}

fn weight_range(v: &str) -> (u16, u16) {
    let one = |s: &str| match s {
        "normal" => Some(400),
        "bold" => Some(700),
        s => s.parse::<f32>().ok().map(|n| n.round().clamp(1.0, 1000.0) as u16),
    };
    let parts: Vec<u16> = v.split_whitespace().filter_map(one).collect();
    match parts.as_slice() {
        [a] => (*a, *a),
        [a, b, ..] => (*a.min(b), *a.max(b)),
        _ => (400, 400),
    }
}

fn range_covers(ranges: &str, cp: u32) -> bool {
    ranges.split(',').any(|r| {
        let r = r.trim().trim_start_matches(['U', 'u']).trim_start_matches('+');
        let (lo, hi) = match r.split_once('-') {
            Some((a, b)) => (u32::from_str_radix(a, 16).ok(), u32::from_str_radix(b, 16).ok()),
            None if r.contains('?') => (u32::from_str_radix(&r.replace('?', "0"), 16).ok(), u32::from_str_radix(&r.replace('?', "F"), 16).ok()),
            None => (u32::from_str_radix(r, 16).ok(), u32::from_str_radix(r, 16).ok()),
        };
        matches!((lo, hi), (Some(lo), Some(hi)) if lo <= cp && cp <= hi)
    })
}

fn load_face_data(d: &mut DocFonts, src: &str) -> Option<std::sync::Arc<Vec<u8>>> {
    if let Some(v) = d.loaded.get(src) {
        return v.clone();
    }
    let raw = if let Some(rest) = src.strip_prefix("data:") {
        use base64::Engine as _;
        let (meta, payload) = rest.split_once(',')?;
        if meta.ends_with(";base64") {
            base64::engine::general_purpose::STANDARD.decode(payload.trim()).ok()
        } else {
            Some(payload.as_bytes().to_vec())
        }
    } else {
        let url = match &d.base {
            Some(b) => b.join(src).ok(),
            None => url::Url::parse(src).ok(),
        };
        url.and_then(|u| d.net.load(&u).ok())
    };
    let ttf = raw.and_then(|b| match b.get(..4) {
        Some(b"wOF2") => wuff::decompress_woff2(&b).ok(),
        Some(b"wOFF") => wuff::decompress_woff1(&b).ok(),
        _ => Some(b),
    });
    let v = ttf.map(std::sync::Arc::new);
    d.loaded.insert(src.to_string(), v.clone());
    v
}

/// The face for `families` while the web font families in `pending` still load: Chrome skips a loading family,
/// and draws in its default font, the system Times (not a CSS family, so the composition format's aliases don't apply), when
/// only loading ones are named.
fn typeface_pending(
    cache: &mut FontCache,
    families: &[String],
    weight: u16,
    italic: bool,
    pending: Option<&std::collections::HashSet<String>>,
) -> Option<Face> {
    if let Some(pending) = pending {
        let available: Vec<String> = families.iter().filter(|f| !pending.contains(&f.to_ascii_lowercase())).cloned().collect();
        if available.len() != families.len() {
            if let Some(face) = typeface_for(cache, &available, weight, italic) {
                return Some(face);
            }
            let style = FontStyle::new(
                sk::font_style::Weight::from(weight as i32),
                sk::font_style::Width::NORMAL,
                if italic { sk::font_style::Slant::Italic } else { sk::font_style::Slant::Upright },
            );
            return FontMgr::new().match_family_style("Times", style).map(|tf| (tf, None, None));
        }
    }
    typeface_for(cache, families, weight, italic)
}

/// The face the document's @font-face rules give `family` at `weight` (CSS Fonts 4 font matching).
fn document_face(family: &str, weight: u16, italic: bool) -> Option<Face> {
    DOC_FONTS.with(|cell| {
        let mut guard = cell.borrow_mut();
        let d = guard.as_mut()?;
        let fam = family.trim().to_ascii_lowercase();
        let faces: Vec<(usize, FaceRule)> =
            d.rules.iter().cloned().enumerate().filter(|(_, r)| r.family == fam && r.latin).collect();
        if faces.is_empty() {
            return None;
        }
        // the requested style when a face has it
        let styled = faces.iter().any(|(_, r)| r.italic == italic);
        let w = weight;
        // weight matching: a range holding the weight; else for 400-500 heavier up to 500, then lighter, then
        // heavier; below 400 lighter first; above 500 heavier first. Among equals the later rule wins.
        let rank = |r: &FaceRule| -> (u8, u16) {
            let (lo, hi) = r.weight;
            if lo <= w && w <= hi {
                return (0, 0);
            }
            let lighter = hi < w;
            let dist = if lighter { w - hi } else { lo - w };
            if (400..=500).contains(&w) {
                if !lighter && lo <= 500 { (1, dist) } else if lighter { (2, dist) } else { (3, dist) }
            } else if w < 400 {
                if lighter { (1, dist) } else { (2, dist) }
            } else if !lighter {
                (1, dist)
            } else {
                (2, dist)
            }
        };
        let (_, rule) = faces
            .into_iter()
            .filter(|(_, r)| !styled || r.italic == italic)
            .min_by_key(|(i, r)| (rank(r), std::cmp::Reverse(*i)))?;
        let bytes = load_face_data(d, &rule.src)?;
        let tf = FontMgr::new().new_from_data(Data::new_copy(&bytes), None)?;
        // a variable face draws at the requested weight within its rule's range (Blink sets wght from it)
        let tag = sk::FourByteTag::from_chars('w', 'g', 'h', 't');
        let variable = tf.variation_design_parameters().is_some_and(|axes| axes.iter().any(|a| a.tag == tag));
        if !variable {
            return Some((tf, Some(bytes), None));
        }
        let v = w.clamp(rule.weight.0, rule.weight.1) as f32;
        let coords = [sk::font_arguments::variation_position::Coordinate { axis: tag, value: v }];
        let pos = sk::font_arguments::VariationPosition { coordinates: &coords };
        let tf = tf.clone_with_arguments(&sk::FontArguments::new().set_variation_design_position(pos))?;
        Some((tf, Some(bytes), Some(v)))
    })
}

/// `measureText` for the shim (served through the script fetcher): (width, ascent, descent).
/// `pending`: web font families still loading (they measure in the fallback, as in Chrome).
pub fn measure_text(font: &str, text: &str, pending: Option<&std::collections::HashSet<String>>) -> (f32, f32, f32) {
    let (families, weight, italic, size) = parse_font(font);
    MEASURE_FONTS.with(|c| {
        let Some((tf, ttf, wght)) = typeface_pending(&mut c.borrow_mut(), &families, weight, italic, pending) else { return (0.0, 0.0, 0.0) };
        let mut f = Font::from_typeface(tf, size);
        f.set_subpixel(true);
        f.set_linear_metrics(true);
        f.set_hinting(sk::FontHinting::Normal);
        let w = ttf.as_deref().and_then(|t| shape(t, text, size, wght)).map(|s| s.2).unwrap_or_else(|| f.measure_str(text, None).0);
        let (_, m) = f.metrics();
        (w, -m.ascent, m.descent)
    })
}

impl Canvas2D {
    pub fn new(width: i32, height: i32) -> Option<Canvas2D> {
        // text blends glyph coverage linearly, like Chrome on macOS (see vendor/anyrender_skia NOTE.md)
        let props = sk::SurfaceProps::new_with_text_properties(Default::default(), sk::PixelGeometry::Unknown, 0.0, 1.0);
        let surface = sk::surfaces::raster(&sk::ImageInfo::new_n32_premul((width.max(1), height.max(1)), None), None, Some(&props))?;
        Some(Canvas2D {
            surface,
            width,
            height,
            state: State::default(),
            stack: Vec::new(),
            path: PathBuilder::new(),
            has_point: false,
            pending_fonts: None,
            fonts: HashMap::new(),
        })
    }

    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    fn ctm(&mut self) -> Matrix {
        self.surface.canvas().local_to_device_as_3x3()
    }

    fn shader(&mut self, v: &Value) -> Option<Shader> {
        let stops = v["stops"].as_array()?;
        let mut colors = Vec::new();
        let mut pos = Vec::new();
        for s in stops {
            colors.push(color(s[1].as_str()?)?);
            pos.push(s[0].as_f64()? as f32);
        }
        let c = &v["coords"];
        let colors4: Vec<Color4f> = colors;
        let grad = match v["kind"].as_str()? {
            "linear" => gradient_shader::linear(
                (Point::new(num(c, 0), num(c, 1)), Point::new(num(c, 2), num(c, 3))),
                gradient_shader::GradientShaderColors::ColorsInSpace(&colors4, None),
                Some(&pos[..]),
                TileMode::Clamp,
                None,
                None,
            ),
            "radial" => gradient_shader::two_point_conical(
                Point::new(num(c, 0), num(c, 1)),
                num(c, 2),
                Point::new(num(c, 3), num(c, 4)),
                num(c, 5),
                gradient_shader::GradientShaderColors::ColorsInSpace(&colors4, None),
                Some(&pos[..]),
                TileMode::Clamp,
                None,
                None,
            ),
            "conic" => {
                // CSS/canvas conic starts at 12 o'clock... Skia's sweep starts at 3 o'clock
                let start = num(c, 0).to_degrees();
                let m = Matrix::rotate_deg_pivot(start - 90.0, Point::new(num(c, 1), num(c, 2)));
                gradient_shader::sweep(
                    Point::new(num(c, 1), num(c, 2)),
                    gradient_shader::GradientShaderColors::ColorsInSpace(&colors4, None),
                    Some(&pos[..]),
                    TileMode::Clamp,
                    None,
                    None,
                    Some(&m),
                )
            }
            _ => None,
        }?;
        Some(grad)
    }

    /// Paint for fill or stroke; `local`: the matrix the shape is drawn with (gradients live in
    /// the user space of the time of drawing).
    fn paint(&mut self, stroke: bool, device_space: bool) -> Paint {
        let st = self.state.clone();
        let mut p = Paint::default();
        p.set_anti_alias(true);
        p.set_blend_mode(st.blend);
        match if stroke { &st.stroke } else { &st.fill } {
            Style::Color(c) => {
                let mut c = *c;
                c.a *= st.alpha;
                p.set_color4f(c, None);
            }
            Style::Shader(v) => {
                if let Some(mut s) = self.shader(v) {
                    if device_space {
                        let m = self.ctm();
                        s = s.with_local_matrix(&m);
                    }
                    p.set_shader(s);
                    p.set_alpha_f(st.alpha);
                }
            }
            Style::Pattern(img, tx, ty) => {
                // patterns live in the user space current when they're drawn (like gradients)
                let local = if device_space { Some(self.ctm()) } else { None };
                let sampling = sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::None);
                if let Some(s) = img.to_shader((*tx, *ty), sampling, local.as_ref()) {
                    p.set_shader(s);
                    p.set_alpha_f(st.alpha);
                }
            }
        }
        if stroke {
            p.set_style(paint::Style::Stroke);
            p.set_stroke_width(st.line_width);
            p.set_stroke_cap(st.cap);
            p.set_stroke_join(st.join);
            p.set_stroke_miter(st.miter);
            if !st.dash.is_empty() {
                let mut d = st.dash.clone();
                if d.len() % 2 == 1 {
                    d.extend(st.dash.clone());
                }
                p.set_path_effect(sk::PathEffect::dash(&d, st.dash_offset));
            }
        }
        if st.shadow_color.a > 0.0 && (st.shadow_blur > 0.0 || st.shadow_x != 0.0 || st.shadow_y != 0.0) {
            // Blink: sigma = blur / 2, offsets in device space (ignore the transform)
            let sigma = st.shadow_blur / 2.0;
            let (mut dx, mut dy) = (st.shadow_x, st.shadow_y);
            if !device_space {
                let inv = self.ctm().invert().unwrap_or_default();
                let v = inv.map_vector((dx, dy));
                dx = v.x;
                dy = v.y;
            }
            let mut sc = st.shadow_color;
            sc.a *= st.alpha;
            p.set_image_filter(image_filters::drop_shadow((dx, dy), (sigma, sigma), sc, None, None, None));
        }
        p
    }

    fn map(&mut self, x: f32, y: f32) -> Point {
        self.ctm().map_point((x, y))
    }

    /// Append an elliptical arc (canvas `arc` / `ellipse` semantics) in user space.
    #[allow(clippy::too_many_arguments)]
    fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, rot: f32, start: f32, end: f32, ccw: bool) {
        let tau = std::f32::consts::TAU;
        let mut sweep = end - start;
        if !ccw && sweep >= tau {
            sweep = tau;
        } else if ccw && -sweep >= tau {
            sweep = -tau;
        } else {
            sweep %= tau;
            if !ccw && sweep < 0.0 {
                sweep += tau;
            } else if ccw && sweep > 0.0 {
                sweep -= tau;
            }
        }
        // Blink: SkPath::arcTo on the unit-rotated ellipse (conics), split so no piece reaches 360°,
        // then mapped through translate(cx, cy) · rotate(rot) and the current matrix
        let local = Matrix::translate((cx, cy)) * Matrix::rotate_rad(rot);
        let m = self.ctm() * local;
        let oval = Rect::from_ltrb(-rx, -ry, rx, ry);
        let mut tmp = PathBuilder::new();
        let (start_deg, sweep_deg) = (start.to_degrees(), sweep.to_degrees());
        if sweep_deg.abs() >= 359.999 {
            tmp.arc_to(oval, start_deg, sweep_deg / 2.0, true);
            tmp.arc_to(oval, start_deg + sweep_deg / 2.0, sweep_deg / 2.0, false);
        } else if sweep_deg == 0.0 {
            let (sx, sy) = (rx * start.cos(), ry * start.sin());
            tmp.move_to((sx, sy));
            tmp.line_to((sx, sy));
        } else {
            tmp.arc_to(oval, start_deg, sweep_deg, true);
        }
        let arc = tmp.snapshot_and_transform(&m);
        let mode = if self.has_point { sk::path::AddPathMode::Extend } else { sk::path::AddPathMode::Append };
        self.path.add_path(&arc, mode);
        self.has_point = true;
    }

    fn typeface(&mut self, families: &[String], weight: u16, italic: bool) -> Option<Face> {
        typeface_pending(&mut self.fonts, families, weight, italic, self.pending_fonts.as_ref())
    }

    fn text(&mut self, text: &str, x: f32, y: f32, max_width: Option<f32>, stroke: bool) {
        let (families, weight, italic, size) = parse_font(&self.state.font);
        let Some((tf, ttf, wght)) = self.typeface(&families, weight, italic) else { return };
        let mut font = Font::from_typeface(tf, size);
        // Blink's FontPlatformData::SetupSkFont for canvas text: subpixel AA edging (A8 here), subpixel
        // positioning with linear metrics, and the default (normal) hinting, which only the page's
        // `text-rendering: geometricPrecision` turns off and canvas fonts don't inherit
        // Blink asks for subpixel AA; on canvas surfaces without LCD geometry Skia makes that A8 "from
        // LCD", which only changes path-drawn glyphs (strokeText, synthetic bold)
        font.set_edging(sk::font::Edging::SubpixelAntiAlias);
        font.set_subpixel(true);
        font.set_linear_metrics(true);
        // measured against Chrome: the system Times it falls back to while a web font loads draws
        // undilated (as with hinting off)
        font.set_hinting(if ttf.is_some() { sk::FontHinting::Normal } else { sk::FontHinting::None });
        font.set_embolden(weight >= 600 && !self.fonts_has_weight(weight));
        let shaped = ttf.as_deref().and_then(|t| shape(t, text, size, wght));
        let width = shaped.as_ref().map(|s| s.2).unwrap_or_else(|| font.measure_str(text, None).0);
        let (_, metrics) = font.metrics();
        let dx = match self.state.align.as_str() {
            "center" => -width / 2.0,
            "right" | "end" => -width,
            _ => 0.0,
        };
        let dy = match self.state.baseline.as_str() {
            "top" | "hanging" => -metrics.ascent,
            "middle" => -(metrics.ascent + metrics.descent) / 2.0,
            "bottom" | "ideographic" => -metrics.descent,
            _ => 0.0,
        };
        let paint = self.paint(stroke, false);
        let canvas = self.surface.canvas();
        canvas.save();
        let (ox, oy) = if let Some(mw) = max_width.filter(|&mw| mw > 0.0 && width > mw) {
            canvas.translate((x, y));
            canvas.scale((mw / width, 1.0));
            (dx, dy)
        } else {
            (x + dx, y + dy)
        };
        match &shaped {
            Some((glyphs, points, _)) => canvas.draw_glyphs_at(glyphs, points.as_slice(), (ox, oy), &font, &paint),
            None => {
                canvas.draw_str(text, (ox, oy), &font, &paint);
            }
        }
        canvas.restore();
    }

    fn fonts_has_weight(&self, _weight: u16) -> bool {
        // bundled faces carry the real weights; system fallbacks match by weight too
        true
    }

    fn draw_image(&mut self, img: (u32, u32, Vec<u8>), a: &Value) {
        let (w, h, rgba) = img;
        let info = ImageInfo::new((w as i32, h as i32), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
        let Some(image) = sk::images::raster_from_data(&info, Data::new_copy(&rgba), (w * 4) as usize) else { return };
        let args: Vec<f32> = (1..a.as_array().map_or(1, Vec::len)).map(|i| num(a, i)).collect();
        let (src, dst) = match args.len() {
            2 => (Rect::from_wh(w as f32, h as f32), Rect::from_xywh(args[0], args[1], w as f32, h as f32)),
            4 => (Rect::from_wh(w as f32, h as f32), Rect::from_xywh(args[0], args[1], args[2], args[3])),
            8 => (Rect::from_xywh(args[0], args[1], args[2], args[3]), Rect::from_xywh(args[4], args[5], args[6], args[7])),
            _ => return,
        };
        let mut paint = self.paint(false, false);
        paint.set_shader(None);
        let mut c = Color4f::new(0.0, 0.0, 0.0, self.state.alpha);
        c.a = self.state.alpha;
        paint.set_color4f(c, None);
        let sampling = if self.state.smoothing {
            sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::None)
        } else {
            sk::SamplingOptions::default()
        };
        self.surface.canvas().draw_image_rect_with_sampling_options(&image, Some((&src, sk::canvas::SrcRectConstraint::Strict)), dst, sampling, &paint);
    }

    /// Replay recorded calls: `[name, ...args]`.
    pub fn apply(&mut self, ops: &[Value], images: ImageLookup<'_>) {
        for op in ops {
            let Some(name) = op.get(0).and_then(Value::as_str) else { continue };
            let s = |i: usize| op.get(i).and_then(Value::as_str).unwrap_or("").to_string();
            match name {
                "save" => {
                    self.stack.push(self.state.clone());
                    self.surface.canvas().save();
                }
                "restore" => {
                    if let Some(st) = self.stack.pop() {
                        self.state = st;
                        self.surface.canvas().restore();
                    }
                }
                "reset" => {
                    while self.stack.pop().is_some() {
                        self.surface.canvas().restore();
                    }
                    self.state = State::default();
                    self.surface.canvas().reset_matrix();
                    self.surface.canvas().clear(Color4f::new(0.0, 0.0, 0.0, 0.0));
                    self.path = PathBuilder::new();
                    self.has_point = false;
                }
                "translate" => {
                    self.surface.canvas().translate((num(op, 1), num(op, 2)));
                }
                "rotate" => {
                    self.surface.canvas().rotate(num(op, 1).to_degrees(), None);
                }
                "scale" => {
                    self.surface.canvas().scale((num(op, 1), num(op, 2)));
                }
                "transform" | "setTransform" => {
                    let m = Matrix::new_all(num(op, 1), num(op, 3), num(op, 5), num(op, 2), num(op, 4), num(op, 6), 0.0, 0.0, 1.0);
                    if name == "setTransform" {
                        self.surface.canvas().set_matrix(&sk::M44::from(m));
                    } else {
                        self.surface.canvas().concat(&m);
                    }
                }
                "resetTransform" => {
                    self.surface.canvas().reset_matrix();
                }
                "fillStyle" | "strokeStyle" => {
                    let style = match op.get(1) {
                        Some(Value::String(c)) => color(c).map(Style::Color),
                        Some(v @ Value::Object(_)) if v.get("pattern").is_some() => pattern_style(&v["pattern"], images),
                        Some(v @ Value::Object(_)) => Some(Style::Shader(v.clone())),
                        _ => None,
                    };
                    if let Some(st) = style {
                        if name == "fillStyle" {
                            self.state.fill = st;
                        } else {
                            self.state.stroke = st;
                        }
                    }
                }
                "lineWidth" => {
                    let w = num(op, 1);
                    if w > 0.0 && w.is_finite() {
                        self.state.line_width = w;
                    }
                }
                "lineCap" => self.state.cap = match s(1).as_str() { "round" => paint::Cap::Round, "square" => paint::Cap::Square, _ => paint::Cap::Butt },
                "lineJoin" => self.state.join = match s(1).as_str() { "round" => paint::Join::Round, "bevel" => paint::Join::Bevel, _ => paint::Join::Miter },
                "miterLimit" => self.state.miter = num(op, 1),
                "setLineDash" => self.state.dash = op.get(1).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).map(|v| v as f32).collect()).unwrap_or_default(),
                "lineDashOffset" => self.state.dash_offset = num(op, 1),
                "globalAlpha" => {
                    let a = num(op, 1);
                    if (0.0..=1.0).contains(&a) {
                        self.state.alpha = a;
                    }
                }
                "globalCompositeOperation" => self.state.blend = blend(&s(1)),
                "shadowColor" => {
                    if let Some(c) = color(&s(1)) {
                        self.state.shadow_color = c;
                    }
                }
                "shadowBlur" => self.state.shadow_blur = num(op, 1).max(0.0),
                "shadowOffsetX" => self.state.shadow_x = num(op, 1),
                "shadowOffsetY" => self.state.shadow_y = num(op, 1),
                "font" => self.state.font = s(1),
                "textAlign" => self.state.align = s(1),
                "textBaseline" => self.state.baseline = s(1),
                "imageSmoothingEnabled" => self.state.smoothing = op.get(1).and_then(Value::as_bool).unwrap_or(true),
                "svg" => {
                    // Path2D("M ...")
                    let d = s(1);
                    for seg in crate::paths::parse(&d).segs {
                        let p = &seg.pts;
                        if p.len() < 2 {
                            continue;
                        }
                        let p0 = self.map(p[0] as f32, p[1] as f32);
                        self.path.move_to(p0);
                        self.has_point = true;
                        let mut i = 2;
                        while i + 5 < p.len() {
                            let (c1, c2, e) = (
                                self.map(p[i] as f32, p[i + 1] as f32),
                                self.map(p[i + 2] as f32, p[i + 3] as f32),
                                self.map(p[i + 4] as f32, p[i + 5] as f32),
                            );
                            self.path.cubic_to(c1, c2, e);
                            i += 6;
                        }
                        if seg.closed {
                            self.path.close();
                        }
                    }
                }
                "beginPath" => {
                    self.path = PathBuilder::new();
                    self.has_point = false;
                }
                "closePath" => {
                    self.path.close();
                }
                "moveTo" => {
                    let p = self.map(num(op, 1), num(op, 2));
                    self.path.move_to(p);
                    self.has_point = true;
                }
                "lineTo" => {
                    let p = self.map(num(op, 1), num(op, 2));
                    if self.has_point {
                        self.path.line_to(p);
                    } else {
                        self.path.move_to(p);
                        self.has_point = true;
                    }
                }
                "quadraticCurveTo" => {
                    let (c, p) = (self.map(num(op, 1), num(op, 2)), self.map(num(op, 3), num(op, 4)));
                    if !self.has_point {
                        self.path.move_to(c);
                        self.has_point = true;
                    }
                    self.path.quad_to(c, p);
                }
                "bezierCurveTo" => {
                    let (c1, c2, p) = (self.map(num(op, 1), num(op, 2)), self.map(num(op, 3), num(op, 4)), self.map(num(op, 5), num(op, 6)));
                    if !self.has_point {
                        self.path.move_to(c1);
                        self.has_point = true;
                    }
                    self.path.cubic_to(c1, c2, p);
                }
                "arc" => {
                    let ccw = op.get(6).and_then(Value::as_bool).unwrap_or(false);
                    self.ellipse(num(op, 1), num(op, 2), num(op, 3), num(op, 3), 0.0, num(op, 4), num(op, 5), ccw);
                }
                "ellipse" => {
                    let ccw = op.get(8).and_then(Value::as_bool).unwrap_or(false);
                    self.ellipse(num(op, 1), num(op, 2), num(op, 3), num(op, 4), num(op, 5), num(op, 6), num(op, 7), ccw);
                }
                "arcTo" => {
                    // tangent arc in user space, appended through the current matrix
                    let inv = self.ctm().invert().unwrap_or_default();
                    let last = self.path.get_last_pt().map(|p| inv.map_point(p));
                    let (x1, y1, x2, y2, r) = (num(op, 1), num(op, 2), num(op, 3), num(op, 4), num(op, 5));
                    let mut tmp = PathBuilder::new();
                    tmp.move_to(last.unwrap_or(Point::new(x1, y1)));
                    tmp.arc_to_tangent((x1, y1), (x2, y2), r);
                    let m = self.ctm();
                    let p = tmp.snapshot_and_transform(&m);
                    self.has_point = true;
                    self.path.add_path(&p, sk::path::AddPathMode::Extend);
                }
                "rect" | "roundRect" => {
                    let (x, y, w, h) = (num(op, 1), num(op, 2), num(op, 3), num(op, 4));
                    let m = self.ctm();
                    let mut tmp = PathBuilder::new();
                    if name == "roundRect" {
                        let r = op.get(5).and_then(|v| v.as_f64().or_else(|| v.get(0).and_then(Value::as_f64))).unwrap_or(0.0) as f32;
                        tmp.add_rrect(sk::RRect::new_rect_xy(Rect::from_xywh(x, y, w, h), r, r), None, None);
                    } else {
                        tmp.add_rect(Rect::from_xywh(x, y, w, h), None, None);
                    }
                    self.path.add_path(&tmp.snapshot_and_transform(&m), sk::path::AddPathMode::Append);
                    let p = self.map(x, y);
                    self.path.move_to(p);
                    self.has_point = true;
                }
                "fill" | "stroke" | "clip" => {
                    let rule = if s(1) == "evenodd" { sk::PathFillType::EvenOdd } else { sk::PathFillType::Winding };
                    let mut path = match op.get(2) {
                        Some(Value::Array(path_ops)) => self.path2d(path_ops),
                        _ => self.path.snapshot(),
                    };
                    path.set_fill_type(rule);
                    match name {
                        "fill" => {
                            let paint = self.paint(false, true);
                            let canvas = self.surface.canvas();
                            canvas.save();
                            canvas.reset_matrix();
                            canvas.draw_path(&path, &paint);
                            canvas.restore();
                        }
                        "stroke" => {
                            // stroke in user space so the line width follows the transform
                            let m = self.ctm();
                            let inv = m.invert().unwrap_or_default();
                            let user = path.with_transform(&inv);
                            let paint = self.paint(true, false);
                            self.surface.canvas().draw_path(&user, &paint);
                        }
                        _ => {
                            let canvas = self.surface.canvas();
                            let m = canvas.local_to_device_as_3x3();
                            canvas.reset_matrix();
                            canvas.clip_path(&path, ClipOp::Intersect, true);
                            canvas.set_matrix(&sk::M44::from(m));
                        }
                    }
                }
                "fillRect" | "strokeRect" | "clearRect" => {
                    let r = Rect::from_xywh(num(op, 1), num(op, 2), num(op, 3), num(op, 4));
                    if name == "clearRect" {
                        let mut p = Paint::default();
                        p.set_blend_mode(BlendMode::Clear);
                        self.surface.canvas().draw_rect(r, &p);
                    } else {
                        let paint = self.paint(name == "strokeRect", false);
                        self.surface.canvas().draw_rect(r, &paint);
                    }
                }
                "fillText" | "strokeText" => {
                    let mw = op.get(4).and_then(Value::as_f64).map(|v| v as f32);
                    self.text(&s(1), num(op, 2), num(op, 3), mw, name == "strokeText");
                }
                "drawImage" => {
                    if let Some(img) = op.get(1).and_then(|v| v["ref"].as_u64()).and_then(images) {
                        self.draw_image(img, op);
                    }
                }
                "putImageData" => {
                    let (w, h) = (op.get(2).and_then(Value::as_u64).unwrap_or(0) as i32, op.get(3).and_then(Value::as_u64).unwrap_or(0) as i32);
                    let data: Vec<u8> = op.get(1).and_then(Value::as_str).map(|b| {
                        use base64::Engine as _;
                        base64::engine::general_purpose::STANDARD.decode(b).unwrap_or_default()
                    }).unwrap_or_default();
                    if w > 0 && h > 0 && data.len() == (w * h * 4) as usize {
                        let info = ImageInfo::new((w, h), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
                        let _ = self.surface.canvas().write_pixels(&info, &data, (w * 4) as usize, (num(op, 4) as i32, num(op, 5) as i32));
                    }
                }
                _ => {}
            }
        }
    }

    /// A Path2D's recorded calls -> a path in device space under the current matrix.
    fn path2d(&mut self, ops: &[Value]) -> sk::Path {
        let saved = std::mem::replace(&mut self.path, PathBuilder::new());
        let saved_point = std::mem::replace(&mut self.has_point, false);
        let filtered: Vec<Value> = ops.iter().filter(|o| matches!(o.get(0).and_then(Value::as_str), Some("svg" | "moveTo" | "lineTo" | "quadraticCurveTo" | "bezierCurveTo" | "arc" | "ellipse" | "arcTo" | "rect" | "roundRect" | "closePath"))).cloned().collect();
        self.apply(&filtered, &|_| None);
        let path = self.path.snapshot();
        self.path = saved;
        self.has_point = saved_point;
        path
    }

    /// `getImageData`: a rectangle of the bitmap as straight-alpha RGBA8; outside the canvas is transparent.
    pub fn region(&mut self, x: i32, y: i32, w: i32, h: i32) -> Vec<u8> {
        let (w, h) = (w.max(0), h.max(0));
        let mut out = vec![0u8; (w * h * 4) as usize];
        if w == 0 || h == 0 {
            return out;
        }
        let info = ImageInfo::new((w, h), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
        // read_pixels clips to the surface and leaves the rest untouched (transparent)
        self.surface.read_pixels(&info, &mut out, (w * 4) as usize, (x, y));
        out
    }

    /// Seeds the bitmap with straight-alpha RGBA8 pixels (a canvas's last flushed frame).
    pub fn write_rgba(&mut self, w: i32, h: i32, data: &[u8]) {
        if w > 0 && h > 0 && data.len() == (w * h * 4) as usize {
            let info = ImageInfo::new((w, h), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
            let _ = self.surface.canvas().write_pixels(&info, data, (w * 4) as usize, (0, 0));
        }
    }

    /// The bitmap as straight-alpha RGBA8 (what an <img> holds).
    pub fn pixels(&mut self) -> Vec<u8> {
        let info = ImageInfo::new((self.width.max(1), self.height.max(1)), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
        let mut out = vec![0u8; (self.width.max(1) * self.height.max(1) * 4) as usize];
        self.surface.read_pixels(&info, &mut out, (self.width.max(1) * 4) as usize, (0, 0));
        out
    }
}
