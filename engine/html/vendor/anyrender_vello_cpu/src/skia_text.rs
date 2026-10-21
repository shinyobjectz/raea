//! raea: Chrome draws axis-aligned text from Skia's glyph masks, which on macOS come from
//! CoreText. Those differ from outline coverage, most at small sizes (CoreText's 13-15 px masks
//! carry ~7% more ink). With Skia built in, a glyph run is rasterized through the Skia backend's
//! own `draw_glyphs` into an 8-bit mask, cached across frames, and composited here as an image.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use anyrender::{Glyph, NormalizedCoord};
use anyrender_skia::{GlyphMask, SkiaSceneCache};
use kurbo::{Affine, Vec2};
use peniko::FontData;

/// Masks kept per thread (rendered runs are usually static text, redrawn every frame).
const MAX_MASKS: usize = 1024;
const MAX_BYTES: usize = 16 << 20;

/// Fonts bigger than this stay on outlines: every render thread's Skia cache makes its own
/// typeface from the font data (a CJK system collection would be copied per thread).
const MAX_FONT_BYTES: usize = 32 << 20;

/// Whether a font's glyphs come out as plain coverage: color glyphs (sbix, COLR, CBDT, SVG) would
/// lose their color in a mask, and Apple Color Emoji alone is ~180 MB of bitmap strikes.
fn coverage_font(font: &FontData) -> bool {
    use skrifa::raw::types::Tag;
    let data = font.data.data();
    if data.len() > MAX_FONT_BYTES {
        return false;
    }
    let Ok(f) = skrifa::FontRef::from_index(data, font.index) else { return false };
    ![b"sbix", b"COLR", b"CBDT", b"SVG "].iter().any(|t| f.table_data(Tag::new(t)).is_some())
}

#[derive(Default)]
struct State {
    eligible: HashMap<(u64, u32), bool>,
    skia: SkiaSceneCache,
    masks: HashMap<u64, Arc<GlyphMask>>,
    order: VecDeque<u64>,
    bytes: usize,
    misses: u32,
    /// Path masks (fills) live in their own budget so large animated shapes don't evict glyph masks.
    path_masks: HashMap<u64, Arc<GlyphMask>>,
    path_order: VecDeque<u64>,
    path_bytes: usize,
}

/// Path mask budget per thread (smaller on iOS, like the SVG cache).
const MAX_PATH_BYTES: usize = if cfg!(target_os = "ios") { 8 << 20 } else { 32 << 20 };

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn glyph_run_mask(
    font: &FontData,
    font_size: f32,
    normalized_coords: &[NormalizedCoord],
    embolden: Vec2,
    transform: Affine,
    glyph_transform: Option<Affine>,
    glyphs: &[Glyph],
    clip: (i32, i32, i32, i32),
) -> Option<Arc<GlyphMask>> {
    static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *OFF.get_or_init(|| std::env::var_os("RAEA_OUTLINE_TEXT").is_some()) {
        return None;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (font.data.id(), font.index, font_size.to_bits(), normalized_coords).hash(&mut h);
    (embolden.x > 0.0 || embolden.y > 0.0).hash(&mut h);
    for c in transform.as_coeffs().into_iter().chain(glyph_transform.iter().flat_map(|g| g.as_coeffs())) {
        c.to_bits().hash(&mut h);
    }
    for g in glyphs {
        (g.id, g.x.to_bits(), g.y.to_bits()).hash(&mut h);
    }
    clip.hash(&mut h);
    let key = h.finish();
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let state = state.get_or_insert_with(State::default);
        if !*state.eligible.entry((font.data.id(), font.index)).or_insert_with(|| coverage_font(font)) {
            return None;
        }
        if let Some(mask) = state.masks.get(&key) {
            return Some(mask.clone());
        }
        let mask = Arc::new(state.skia.glyph_run_mask(
            font,
            font_size,
            normalized_coords,
            embolden,
            transform,
            glyph_transform,
            glyphs,
            clip,
        )?);
        state.misses += 1;
        if state.misses % 256 == 0 {
            // let Skia's font and typeface caches age out sizes no longer drawn
            state.skia.next_gen();
        }
        state.bytes += mask.coverage.len();
        state.masks.insert(key, mask.clone());
        state.order.push_back(key);
        while state.order.len() > MAX_MASKS || state.bytes > MAX_BYTES {
            let Some(old) = state.order.pop_front() else { break };
            if let Some(m) = state.masks.remove(&old) {
                state.bytes -= m.coverage.len();
            }
        }
        Some(mask)
    })
}

/// An outset box shadow's coverage from the Skia backend's own `draw_box_shadow` (Skia's blur mask
/// filter, Chrome's software raster), cached across frames like glyph masks.
/// raea: Skia's antialiased coverage for a filled path (Chrome rasterizes every path with
/// Skia's scan converter; vello's analytic area coverage differs at curves and slanted edges).
pub(crate) fn path_mask(transform: Affine, path: &kurbo::BezPath, fill: peniko::Fill, clip: (i32, i32, i32, i32)) -> Option<Arc<GlyphMask>> {
    static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *OFF.get_or_init(|| std::env::var_os("RAEA_VELLO_PATHS").is_some()) {
        return None;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    "path".hash(&mut h);
    matches!(fill, peniko::Fill::EvenOdd).hash(&mut h);
    for c in transform.as_coeffs() {
        c.to_bits().hash(&mut h);
    }
    for el in path.elements() {
        use kurbo::PathEl::*;
        let (tag, pts): (u8, [Option<kurbo::Point>; 3]) = match *el {
            MoveTo(p) => (0, [Some(p), None, None]),
            LineTo(p) => (1, [Some(p), None, None]),
            QuadTo(a, b) => (2, [Some(a), Some(b), None]),
            CurveTo(a, b, c) => (3, [Some(a), Some(b), Some(c)]),
            ClosePath => (4, [None, None, None]),
        };
        tag.hash(&mut h);
        for p in pts.into_iter().flatten() {
            p.x.to_bits().hash(&mut h);
            p.y.to_bits().hash(&mut h);
        }
    }
    clip.hash(&mut h);
    let key = h.finish();
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let state = state.get_or_insert_with(State::default);
        if let Some(mask) = state.path_masks.get(&key) {
            return Some(mask.clone());
        }
        let mask = Arc::new(state.skia.path_mask(transform, path, fill, clip)?);
        state.path_bytes += mask.coverage.len();
        state.path_masks.insert(key, mask.clone());
        state.path_order.push_back(key);
        while state.path_order.len() > MAX_MASKS || state.path_bytes > MAX_PATH_BYTES {
            let Some(old) = state.path_order.pop_front() else { break };
            if let Some(m) = state.path_masks.remove(&old) {
                state.path_bytes -= m.coverage.len();
            }
        }
        Some(mask)
    })
}

pub(crate) fn box_shadow_mask(transform: Affine, rect: kurbo::Rect, radius: f64, std_dev: f64, clip: (i32, i32, i32, i32)) -> Option<Arc<GlyphMask>> {
    static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *OFF.get_or_init(|| std::env::var_os("RAEA_OUTLINE_TEXT").is_some()) {
        return None;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    "shadow".hash(&mut h);
    for c in transform.as_coeffs().into_iter().chain([rect.x0, rect.y0, rect.x1, rect.y1, radius, std_dev]) {
        c.to_bits().hash(&mut h);
    }
    clip.hash(&mut h);
    let key = h.finish();
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let state = state.get_or_insert_with(State::default);
        if let Some(mask) = state.masks.get(&key) {
            return Some(mask.clone());
        }
        let mask = Arc::new(state.skia.box_shadow_mask(transform, rect, radius, std_dev, clip)?);
        state.bytes += mask.coverage.len();
        state.masks.insert(key, mask.clone());
        state.order.push_back(key);
        while state.order.len() > MAX_MASKS || state.bytes > MAX_BYTES {
            let Some(old) = state.order.pop_front() else { break };
            if let Some(m) = state.masks.remove(&old) {
                state.bytes -= m.coverage.len();
            }
        }
        Some(mask)
    })
}
