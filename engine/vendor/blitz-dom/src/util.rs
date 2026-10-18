use crate::node::{Node, NodeData};
use color::{AlphaColor, Srgb};
use keyboard_types::Modifiers;
use std::borrow::Cow;
use style::color::AbsoluteColor;

#[cfg(target_os = "macos")]
pub(crate) const ACTION_MOD: Modifiers = Modifiers::SUPER;
#[cfg(not(target_os = "macos"))]
pub(crate) const ACTION_MOD: Modifiers = Modifiers::CONTROL;

pub type Color = AlphaColor<Srgb>;

/// Decode raw font bytes, decompressing WOFF/WOFF2 if the `woff` feature is enabled.
/// Returns the original slice unchanged for TTF/OTF input, and also on decompression
/// failure. With the `woff` feature disabled, all input passes through unchanged.
pub fn decode_font_bytes(bytes: &[u8]) -> Cow<'_, [u8]> {
    if bytes.len() < 4 {
        return Cow::Borrowed(bytes);
    }
    match &bytes[0..4] {
        #[cfg(feature = "woff")]
        b"wOFF" => wuff::decompress_woff1(bytes)
            .map(Cow::Owned)
            .unwrap_or_else(|_| {
                #[cfg(feature = "tracing")]
                tracing::warn!("Failed to decompress woff1 font");
                Cow::Borrowed(bytes)
            }),
        #[cfg(feature = "woff")]
        b"wOF2" => wuff::decompress_woff2(bytes)
            .map(Cow::Owned)
            .unwrap_or_else(|_| {
                #[cfg(feature = "tracing")]
                tracing::warn!("Failed to decompress woff2 font");
                Cow::Borrowed(bytes)
            }),
        _ => Cow::Borrowed(bytes),
    }
}

#[cfg(feature = "svg")]
use std::sync::{Arc, LazyLock};
#[cfg(feature = "svg")]
use usvg::fontdb;
#[cfg(feature = "svg")]
pub(crate) static FONT_DB: LazyLock<std::sync::RwLock<Arc<fontdb::Database>>> = LazyLock::new(|| {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    std::sync::RwLock::new(Arc::new(db))
});

/// Whether an sfnt (TrueType/OpenType, not a collection) has table `tag`.
#[cfg(feature = "svg")]
fn has_table(bytes: &[u8], tag: &[u8; 4]) -> bool {
    let n = bytes.get(4..6).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize).unwrap_or(0);
    (0..n).any(|i| bytes.get(12 + i * 16..16 + i * 16) == Some(&tag[..]))
}

/// Make an `@font-face` font available to SVG text under its CSS family name (local change: SVG
/// documents used to see only system fonts, so web fonts fell back to Times).
/// The tables Blink keeps in a web font: those OTS sanitizes, plus the ones Blink passes through
/// (web_font_decoder.cc `GetTableAction`). OTS drops every other table.
const WEB_FONT_TABLES: &[&[u8; 4]] = &[
    b"cmap", b"head", b"hhea", b"hmtx", b"maxp", b"name", b"OS/2", b"post", b"loca", b"glyf",
    b"CFF ", b"CFF2", b"cvt ", b"fpgm", b"prep", b"gasp", b"hdmx", b"LTSH", b"VDMX", b"VORG",
    b"vhea", b"vmtx", b"kern", b"MATH", b"BASE", b"GDEF", b"GPOS", b"GSUB", b"avar", b"cvar",
    b"fvar", b"gvar", b"HVAR", b"MVAR", b"VVAR", b"STAT", b"CBDT", b"CBLC", b"COLR", b"CPAL",
    b"sbix", b"Feat", b"Glat", b"Gloc", b"Sile", b"Silf", b"Sill",
];

/// Removes the tables Chrome's font sanitizer (OTS) drops from a web font, so shaping sees what
/// Blink's HarfBuzz sees: no AAT `morx`/`kerx`/`trak`/`feat` (Didot keeps its old-style figures,
/// American Typewriter its plain 7) and no Apple (version 1) `kern`. Only the table directory is
/// rewritten; the table data stays where it is.
pub(crate) fn drop_unsanitized_tables(data: &mut [u8]) {
    fn be16(d: &[u8], o: usize) -> Option<u16> {
        Some(u16::from_be_bytes(d.get(o..o + 2)?.try_into().ok()?))
    }
    fn be32(d: &[u8], o: usize) -> Option<u32> {
        Some(u32::from_be_bytes(d.get(o..o + 4)?.try_into().ok()?))
    }
    fn filter(data: &mut [u8], dir: usize) -> Option<()> {
        let count = be16(data, dir + 4)? as usize;
        let records = dir + 12;
        data.get(records..records + count * 16)?;
        let mut kept: Vec<[u8; 16]> = Vec::with_capacity(count);
        for i in 0..count {
            let rec: [u8; 16] = data[records + i * 16..records + i * 16 + 16].try_into().ok()?;
            let tag: &[u8; 4] = rec[..4].try_into().ok()?;
            let offset = be32(&rec, 8)? as usize;
            let apple_kern = tag == b"kern" && be16(data, offset).is_some_and(|v| v != 0);
            if WEB_FONT_TABLES.contains(&tag) && !apple_kern {
                kept.push(rec);
            }
        }
        if kept.len() == count {
            return Some(());
        }
        for (i, rec) in kept.iter().enumerate() {
            data[records + i * 16..records + i * 16 + 16].copy_from_slice(rec);
        }
        data[records + kept.len() * 16..records + count * 16].fill(0);
        let n = kept.len() as u16;
        let entry_selector = if n == 0 { 0 } else { 15 - n.leading_zeros() as u16 };
        let search_range = (1u16 << entry_selector) * 16;
        data[dir + 4..dir + 6].copy_from_slice(&n.to_be_bytes());
        data[dir + 6..dir + 8].copy_from_slice(&search_range.to_be_bytes());
        data[dir + 8..dir + 10].copy_from_slice(&entry_selector.to_be_bytes());
        data[dir + 10..dir + 12].copy_from_slice(&(n * 16 - search_range.min(n * 16)).to_be_bytes());
        Some(())
    }
    let dirs: Vec<usize> = if data.get(..4) == Some(b"ttcf") {
        let n = be32(data, 8).unwrap_or(0) as usize;
        (0..n).filter_map(|i| be32(data, 12 + i * 4).map(|o| o as usize)).collect()
    } else {
        vec![0]
    };
    for dir in dirs {
        let _ = filter(data, dir);
    }
}

#[cfg(feature = "svg")]
pub(crate) fn register_svg_font(bytes: &[u8], family: Option<&str>, weight: Option<f32>, italic: Option<bool>) {
    // the same face arrives once per @font-face rule and per document; register it once
    static SEEN: LazyLock<std::sync::Mutex<std::collections::HashSet<u64>>> = LazyLock::new(Default::default);
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut h);
        family.hash(&mut h);
        weight.map(f32::to_bits).hash(&mut h);
        italic.hash(&mut h);
        h.finish()
    };
    if !SEEN.lock().unwrap_or_else(|p| p.into_inner()).insert(key) {
        return;
    }
    let mut guard = FONT_DB.write().unwrap_or_else(|p| p.into_inner());
    let db = Arc::make_mut(&mut guard);
    let ids = db.load_font_source(fontdb::Source::Binary(Arc::new(bytes.to_vec())));
    for id in ids {
        let Some(mut info) = db.face(id).cloned() else { continue };
        db.remove_face(id);
        if let Some(f) = family {
            info.families = vec![(f.to_string(), fontdb::Language::English_UnitedStates)];
        }
        // a variable font's @font-face weight is a range (only its lower bound reaches here): keep the
        // font's default instance so usvg picks the face and maps font-weight onto its wght axis
        if let Some(w) = weight.filter(|_| !has_table(bytes, b"fvar")) {
            info.weight = fontdb::Weight(w.round().clamp(1.0, 1000.0) as u16);
        }
        if let Some(i) = italic {
            info.style = if i { fontdb::Style::Italic } else { fontdb::Style::Normal };
        }
        db.push_face_info(info);
    }
}

/// Which kind of CSS image layer list (`background-image` or `mask-image`) to
/// flush from style to dedicated storage on the node.
#[derive(Clone, Copy, Debug)]
pub enum ImageLayerKind {
    Background,
    Mask,
}

impl ImageLayerKind {
    pub fn image_type(self, idx: usize) -> ImageType {
        match self {
            Self::Background => ImageType::Background(idx),
            Self::Mask => ImageType::Mask(idx),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ImageType {
    Image,
    Background(usize),
    Mask(usize),
}

/// A point
#[derive(Clone, Debug, Copy, Eq, PartialEq)]
pub struct Point<T> {
    /// The x coordinate
    pub x: T,
    /// The y coordinate
    pub y: T,
}

impl Point<f64> {
    pub const ZERO: Self = Point { x: 0.0, y: 0.0 };
}

// Debug print an RcDom
pub fn walk_tree(indent: usize, node: &Node) {
    // Skip all-whitespace text nodes entirely
    if let NodeData::Text(data) = &node.data {
        if data.content.chars().all(|c| c.is_ascii_whitespace()) {
            return;
        }
    }

    print!("{}", " ".repeat(indent));
    let id = node.id;
    match &node.data {
        NodeData::Document(_) => println!("#Document {id}"),

        NodeData::Text(data) => {
            if data.content.chars().all(|c| c.is_ascii_whitespace()) {
                println!("{id} #text: <whitespace>");
            } else {
                let content = data.content.trim();
                if content.len() > 10 {
                    println!(
                        "#text {id}: {}...",
                        content
                            .split_at(content.char_indices().take(10).last().unwrap().0)
                            .0
                            .escape_default()
                    )
                } else {
                    println!("#text {id}: {}", data.content.trim().escape_default())
                }
            }
        }

        NodeData::Comment { .. } => println!("<!-- COMMENT {id} -->"),

        NodeData::AnonymousBlock(_) => println!("{id} AnonymousBlock"),

        NodeData::Element(data) => {
            print!("<{} {id}", data.name.local);
            for attr in data.attrs.iter() {
                print!(" {}=\"{}\"", attr.name.local, attr.value);
            }
            if !node.children.is_empty() {
                println!(">");
            } else {
                println!("/>");
            }
        } // NodeData::Doctype {
          //     ref name,
          //     ref public_id,
          //     ref system_id,
          // } => println!("<!DOCTYPE {} \"{}\" \"{}\">", name, public_id, system_id),
          // NodeData::ProcessingInstruction { .. } => unreachable!(),
    }

    if !node.children.is_empty() {
        for child_id in node.children.iter() {
            walk_tree(indent + 2, node.with(*child_id));
        }

        if let NodeData::Element(data) = &node.data {
            println!("{}</{}>", " ".repeat(indent), data.name.local);
        }
    }
}

/// usvg's fallback takes the first face in the database with the character (often a system font). Web
/// fonts arrive as unicode-range subsets of one family, so try the base face's own family first
/// (local change: SVG text in Google Fonts drew in a fallback face and lost its weight).
#[cfg(feature = "svg")]
fn same_family_fallback() -> usvg::FallbackSelectionFn<'static> {
    let default = usvg::FontResolver::default_fallback_selector();
    Box::new(move |c, exclude, db| {
        let base = exclude.first().and_then(|id| db.face(*id)).map(|f| (f.families.first().map(|x| x.0.to_ascii_lowercase()), f.weight, f.style));
        if let Some((Some(family), weight, style)) = base {
            let mut same: Vec<&fontdb::FaceInfo> = db
                .faces()
                .filter(|f| !exclude.contains(&f.id) && f.families.iter().any(|x| x.0.to_ascii_lowercase() == family))
                .collect();
            same.sort_by_key(|f| ((f.weight.0 as i32 - weight.0 as i32).abs(), f.style != style));
            let has_char = |id: fontdb::ID| {
                db.with_face_data(id, |data, index| {
                    use skrifa::MetadataProvider as _;
                    skrifa::FontRef::from_index(data, index).ok().and_then(|f| f.charmap().map(c)).is_some()
                })
                .unwrap_or(false)
            };
            if let Some(f) = same.into_iter().find(|f| has_char(f.id)) {
                return Some(f.id);
            }
        }
        default(c, exclude, db)
    })
}

/// Parse an SVG image.
#[cfg(feature = "svg")]
pub(crate) fn parse_svg_image(source: &[u8]) -> Result<crate::node::SvgImageData, usvg::Error> {
    let options = usvg::Options {
        fontdb: Arc::clone(&FONT_DB.read().unwrap_or_else(|p| p.into_inner())),
        font_resolver: usvg::FontResolver {
            select_font: usvg::FontResolver::default_font_selector(),
            select_fallback: same_family_fallback(),
        },
        ..Default::default()
    };
    crate::node::SvgImageData::from_data(source, &options)
}

pub trait ToColorColor {
    /// Converts a color into the `AlphaColor<Srgb>` type from the `color` crate
    fn as_color_color(&self) -> Color;
}
impl ToColorColor for AbsoluteColor {
    fn as_color_color(&self) -> Color {
        Color::new(
            *self
                .to_color_space(style::color::ColorSpace::Srgb)
                .raw_components(),
        )
    }
}

#[cfg(all(test, feature = "svg"))]
mod svg_tests {
    use super::parse_svg_image;

    #[test]
    fn svg_without_xmlns_parses() {
        let src = br#"<svg viewBox="0 0 24 24"><path d="M0 0h24v24H0z"/></svg>"#;
        let svg = parse_svg_image(src).unwrap();
        assert!(!svg.tree.root().children().is_empty());
    }

    #[test]
    fn missing_height_is_computed_from_width_and_viewbox_ratio() {
        let src = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="200"><rect width="100%" height="100%" fill="green"/></svg>"#;
        let svg = parse_svg_image(src).unwrap();
        assert_eq!(svg.intrinsic_width(), Some(200.0));
        assert_eq!(svg.intrinsic_height(), None);
        assert_eq!(svg.viewbox_aspect_ratio(), Some(1.0));
        assert_eq!(svg.tree.size().width(), 200.0);
        assert_eq!(svg.intrinsic_size(), (200.0, 200.0));
    }

    #[test]
    fn viewbox_only_has_no_intrinsic_dimensions() {
        let src = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 485 58"></svg>"#;
        let svg = parse_svg_image(src).unwrap();
        assert_eq!(svg.intrinsic_width(), None);
        assert_eq!(svg.intrinsic_height(), None);
        // The aspect ratio is still available from the viewBox.
        assert!((svg.aspect_ratio() - (485.0 / 58.0)).abs() < 1e-3);
    }

    #[test]
    fn absolute_dimensions_are_intrinsic() {
        let src = br#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="16" viewBox="0 0 48 32"></svg>"#;
        let svg = parse_svg_image(src).unwrap();
        assert_eq!(svg.intrinsic_width(), Some(24.0));
        assert_eq!(svg.intrinsic_height(), Some(16.0));
    }

    #[test]
    fn percentage_dimensions_are_not_intrinsic() {
        let src = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="50%" viewBox="0 0 200 100"></svg>"#;
        let svg = parse_svg_image(src).unwrap();
        assert_eq!(svg.intrinsic_width(), None);
        assert_eq!(svg.intrinsic_height(), None);
    }

    #[test]
    fn unit_lengths_are_intrinsic() {
        let src = br#"<svg xmlns="http://www.w3.org/2000/svg" width="24px" height="1.5em" viewBox="0 0 48 32"></svg>"#;
        let svg = parse_svg_image(src).unwrap();
        assert!(svg.intrinsic_width().is_some());
        assert!(svg.intrinsic_height().is_some());
    }

    #[test]
    fn non_numeric_dimensions_are_not_intrinsic() {
        let src = br#"<svg xmlns="http://www.w3.org/2000/svg" width="auto" height="foo" viewBox="0 0 200 100"></svg>"#;
        let svg = parse_svg_image(src).unwrap();
        assert_eq!(svg.intrinsic_width(), None);
        assert_eq!(svg.intrinsic_height(), None);
    }
}

/// Creates an markup5ever::QualName.
/// Given a local name and an optional namespace
#[macro_export]
macro_rules! qual_name {
    ($local:tt $(, $ns:ident)?) => {
        $crate::QualName {
            prefix: None,
            ns: $crate::ns!($($ns)?),
            local: $crate::local_name!($local),
        }
    };
}
