// Copyright 2024 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::{
    FallbackKey, FamilyId, FamilyInfo, FamilyNameMap, GenericFamily, GenericFamilyMap, ScriptExt,
    scan,
};
use alloc::format;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ptr::{null, null_mut};
use hashbrown::{HashMap, HashSet};
use objc2_core_foundation::{
    CFArray, CFDictionary, CFRange, CFRetained, CFString, CFType, CFURL, CFURLPathStyle,
};
use objc2_core_text::{
    CTFont, CTFontCollection, CTFontDescriptor, CTFontUIFontType, kCTFontFamilyNameAttribute,
    kCTFontURLAttribute,
};
use objc2_foundation::{
    NSSearchPathDirectory, NSSearchPathDomainMask, NSSearchPathForDirectoriesInDomains,
};
use parlance::Script;
use std::path::{Path, PathBuf};

// raea: Chrome 152's macOS defaults (chrome-headless-shell, CSS.getPlatformFontsForNode) first
const DEFAULT_GENERIC_FAMILIES: &[(GenericFamily, &[&str])] = &[
    (GenericFamily::Serif, &["Times New Roman", "Times"]),
    (GenericFamily::SansSerif, &["Arial", "Helvetica"]),
    (GenericFamily::Monospace, &["Menlo", "Courier", "Courier New"]),
    (GenericFamily::Cursive, &["Times New Roman", "Apple Chancery"]),
    (GenericFamily::Fantasy, &["Impact", "Papyrus"]),
    (GenericFamily::SystemUi, &["System Font", ".SF NS"]),
    (GenericFamily::Emoji, &["Apple Color Emoji"]),
    (GenericFamily::Math, &["STIX Two Math"]),
];

pub(crate) struct SystemFonts {
    pub(crate) name_map: Arc<FamilyNameMap>,
    pub(crate) generic_families: Arc<GenericFamilyMap>,
    family_map: HashMap<FamilyId, FamilyInfo>,
    /// families already given CoreText's weights and member order (local change)
    coretext_adjusted: HashSet<FamilyId>,
}

impl SystemFonts {
    pub(crate) fn new() -> Self {
        let scanned = scan_system_fonts().unwrap_or_default();
        let name_map = scanned.family_names;
        let mut generic_families = GenericFamilyMap::default();
        for (family, names) in DEFAULT_GENERIC_FAMILIES {
            generic_families.set(
                *family,
                names
                    .iter()
                    .filter_map(|name| name_map.get(name))
                    .map(|name| name.id()),
            );
        }
        Self {
            name_map: Arc::new(name_map),
            generic_families: Arc::new(generic_families),
            family_map: scanned.families,
            coretext_adjusted: HashSet::new(),
        }
    }

    pub(crate) fn family(&mut self, id: FamilyId) -> Option<FamilyInfo> {
        if self.coretext_adjusted.insert(id) {
            if let Some(adjusted) = self.family_map.get(&id).and_then(coretext_family) {
                self.family_map.insert(id, adjusted);
            }
        }
        self.family_map.get(&id).cloned()
    }

    /// raea: the system UI font and its CoreText cascade list, in order, as families: what CoreText
    /// walks (and Chrome, through CTFontCreateForString) for a character the requested fonts lack. The fallback
    /// for scripts without a sample (Common: symbols, arrows, dingbats), where the per-script lookup has none.
    pub(crate) fn fallback_cascade(&mut self) -> Vec<FamilyId> {
        let base = create_base_font(true);
        let mut out: Vec<FamilyId> = Vec::new();
        let mut push = |name: &str, out: &mut Vec<FamilyId>| {
            if let Some(family) = self.name_map.get(name) {
                if !out.contains(&family.id()) {
                    out.push(family.id());
                }
            }
        };
        push(&unsafe { base.family_name() }.to_string(), &mut out);
        // SAFETY: CoreText calls with a valid font; the list holds font descriptors.
        let Some(list) = (unsafe { base.default_cascade_list_for_languages(None) }) else {
            return out;
        };
        let list: CFRetained<CFArray<CTFontDescriptor>> = unsafe { CFRetained::cast_unchecked(list) };
        for index in 0..list.len() {
            let Some(descriptor) = list.get(index) else { continue };
            let Some(name) = (unsafe { descriptor.attribute(kCTFontFamilyNameAttribute) }) else {
                continue;
            };
            if let Ok(name) = name.downcast::<CFString>() {
                push(&name.to_string(), &mut out);
            }
        }
        out
    }

    pub(crate) fn fallback(&mut self, key: impl Into<FallbackKey>) -> Option<FamilyId> {
        const HANI: Script = Script::from_bytes(*b"Hani");
        const HANT: Script = Script::from_bytes(*b"Hant");
        const HANS: Script = Script::from_bytes(*b"Hans");
        let key = key.into();
        let script = key.script();
        let font = create_fallback_font_for_text(script.sample()?, key.locale_str(), false)?;
        let family_name = unsafe { font.family_name() };
        if let Some(family) = self.name_map.get(&family_name.to_string()) {
            return Some(family.id());
        }
        // HACK: if we don't have a usable PingFangUI due to our inability
        // to render hvgl outlines then try another font
        let name = match script {
            HANI | HANS => "Heiti SC",
            HANT => "Heiti TC",
            _ => return None,
        };
        self.name_map.get(name).map(|family| family.id())
    }
}

/// Discover system fonts by combining CoreText enumeration with a directory scan of all
/// Library/Fonts paths, then index them through the shared scan pipeline.
fn scan_system_fonts() -> Option<scan::ScannedCollection> {
    // SAFETY: Calls into CoreText. If anything fails we return None and use the fallback scan.
    let collection = unsafe { CTFontCollection::from_available_fonts(None) };
    let descriptors = unsafe { collection.matching_font_descriptors()? };
    let descriptors: CFRetained<CFArray<CTFontDescriptor>> =
        unsafe { CFRetained::cast_unchecked(descriptors) };

    // Collect unique font file paths to avoid redundant scanning.
    let mut paths: HashSet<PathBuf> = HashSet::new();
    for index in 0..descriptors.len() {
        let Some(descriptor) = descriptors.get(index) else {
            continue;
        };

        let Some(url_cf): Option<CFRetained<CFType>> =
            (unsafe { descriptor.attribute(kCTFontURLAttribute) })
        else {
            continue;
        };

        // The attribute is typed as CFType; attempt to downcast to CFURL.
        let Ok(url_cf): Result<CFRetained<CFURL>, _> = url_cf.downcast::<CFURL>() else {
            continue;
        };

        // Convert the file URL into a POSIX path.
        let Some(path_cf): Option<CFRetained<CFString>> =
            url_cf.file_system_path(CFURLPathStyle::CFURLPOSIXPathStyle)
        else {
            continue;
        };

        let path = PathBuf::from(path_cf.to_string());
        if path.exists() {
            paths.insert(path);
        }
    }

    // Apple hides certain fonts from CTFontCollection (notably SFNS.ttf, the San Francisco
    // system UI font). Scanning Library/Fonts directories catches what CoreText omits.
    paths.extend(library_font_files());

    if paths.is_empty() {
        return None;
    }

    Some(scan::ScannedCollection::from_paths(paths.iter(), 0))
}

fn library_font_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in NSSearchPathForDirectoriesInDomains(
        NSSearchPathDirectory::LibraryDirectory,
        NSSearchPathDomainMask::AllDomainsMask,
        true,
    ) {
        let font_dir = PathBuf::from(format!("{dir}/Fonts"));
        if font_dir.is_dir() {
            collect_files(&font_dir, 8, 0, &mut files);
        }
    }
    files
}

fn collect_files(dir: &Path, max_depth: u32, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            if depth < max_depth {
                collect_files(&path, max_depth, depth + 1, out);
            }
        } else {
            out.push(path);
        }
    }
}

fn create_base_font(prefer_ui_font: bool) -> CFRetained<CTFont> {
    if prefer_ui_font
        && let Some(font) =
            unsafe { CTFont::new_ui_font_for_language(CTFontUIFontType::System, 0.0, None) }
    {
        return font;
    }
    unsafe {
        let attrs = CFDictionary::new(None, null_mut(), null_mut(), 0, null(), null());
        let desc = CTFontDescriptor::with_attributes(&attrs.unwrap());
        CTFont::with_font_descriptor(&desc, 0.0, null())
    }
}

fn create_fallback_font_for_text(
    text: &str,
    locale: Option<&str>,
    prefer_ui_font: bool,
) -> Option<CFRetained<CTFont>> {
    let text = CFString::from_str(text);
    let text_range = CFRange {
        location: 0,
        length: text.length(),
    };
    let locale = locale.map(CFString::from_str);
    let base_font = create_base_font(prefer_ui_font);
    let font = unsafe {
        if let Some(locale) = locale {
            CTFont::for_string_with_language(&base_font, &text, text_range, Some(&locale))
        } else {
            CTFont::for_string(&base_font, &text, text_range)
        }
    };
    Some(font)
}

// --- local change (raea): Chrome's view of installed Mac families -------------------------
//
// Chrome 152 on macOS matches an installed family with CSS font matching over CoreText's description of
// each face: the weight is CoreText's kCTFontWeightTrait mapped onto CSS weights through Apple's
// standard weights, and ties go to the face CoreText lists first. The fonts' own OS/2 weight classes
// often disagree (Didot Bold, Hoefler Text Black...). Verified per family/weight/style against
// chrome-headless-shell 152's CSS.getPlatformFontsForNode (tests/parity/fonts/mac-families).

#[cfg(target_os = "macos")]
/// CoreText weight (-1..1) -> CSS weight, through Apple's NSFontWeight constants.
fn css_weight_from_coretext(w: f64) -> f32 {
    const POINTS: [(f64, f64); 9] = [(-0.8, 100.0), (-0.6, 200.0), (-0.4, 300.0), (0.0, 400.0), (0.23, 500.0), (0.3, 600.0), (0.4, 700.0), (0.56, 800.0), (0.62, 900.0)];
    if w <= POINTS[0].0 {
        return 100.0;
    }
    for pair in POINTS.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        if w <= x1 {
            return (y0 + (w - x0) / (x1 - x0) * (y1 - y0)) as f32;
        }
    }
    900.0
}

#[cfg(target_os = "macos")]
fn postscript_name(font: &super::FontInfo) -> Option<alloc::string::String> {
    let data = font.load(None)?;
    let font_ref = read_fonts::FontRef::from_index(data.as_ref(), font.index()).ok()?;
    use read_fonts::TableProvider as _;
    let names = font_ref.name().ok()?;
    let name = super::super::scan::english_or_first(&names, read_fonts::types::NameId::POSTSCRIPT_NAME)?;
    Some(name.chars().collect())
}

#[cfg(target_os = "macos")]
/// Whether CoreText calls the face italic (its symbolic traits): some faces' own tables don't say so (Avenir Next's
/// Bold Italic and UltraLight Italic scanned as upright, so a weight of 700 could pick the italic, 2026-10-14).
fn coretext_italic(postscript: &str) -> Option<bool> {
    // SAFETY: as coretext_weight
    unsafe {
        let name = CFString::from_str(postscript);
        let font = CTFont::with_name(&name, 12.0, null());
        if font.post_script_name().to_string() != postscript {
            return None;
        }
        Some(font.symbolic_traits().contains(objc2_core_text::CTFontSymbolicTraits::ItalicTrait))
    }
}

#[cfg(target_os = "macos")]
fn coretext_weight(postscript: &str) -> Option<f64> {
    // SAFETY: CoreText calls with valid CF objects; a missing font yields a fallback we reject below.
    unsafe {
        let name = CFString::from_str(postscript);
        let font = CTFont::with_name(&name, 12.0, null());
        if font.post_script_name().to_string() != postscript {
            return None;
        }
        let traits = font.traits();
        let value = traits.value(objc2_core_text::kCTFontWeightTrait as *const CFString as *const core::ffi::c_void);
        if value.is_null() {
            return None;
        }
        let number = &*(value as *const objc2_core_foundation::CFNumber);
        let mut w: f64 = 0.0;
        number
            .value(objc2_core_foundation::CFNumberType::Float64Type, &mut w as *mut f64 as *mut core::ffi::c_void)
            .then_some(w)
    }
}

#[cfg(target_os = "macos")]
/// PostScript names of the family's faces in CoreText's order.
fn coretext_member_order(family: &str) -> Vec<alloc::string::String> {
    // SAFETY: CoreText enumeration; failures give an empty order (fontique's own order stays).
    unsafe {
        let collection = CTFontCollection::from_available_fonts(None);
        let Some(descriptors) = collection.matching_font_descriptors_for_family(&CFString::from_str(family), None) else {
            return Vec::new();
        };
        let descriptors: CFRetained<CFArray<CTFontDescriptor>> = CFRetained::cast_unchecked(descriptors);
        (0..descriptors.len())
            .filter_map(|i| descriptors.get(i))
            .filter_map(|d| d.attribute(objc2_core_text::kCTFontNameAttribute))
            .filter_map(|v| v.downcast::<CFString>().ok())
            .map(|s| s.to_string())
            .collect()
    }
}

/// iOS: CTFontCollectionCreateMatchingFontDescriptorsForFamily is macOS-only, and there are no
/// installed families to mirror Chrome's matching of; fontique's own order and weights stay.
#[cfg(not(target_os = "macos"))]
fn coretext_family(_info: &FamilyInfo) -> Option<FamilyInfo> {
    None
}

#[cfg(target_os = "macos")]
fn coretext_family(info: &FamilyInfo) -> Option<FamilyInfo> {
    // only files on disk: registered web fonts and bundled faces keep CSS matching on their own data
    if info.fonts().iter().any(|f| !matches!(f.source().kind, crate::SourceKind::Path(_))) {
        return None;
    }
    let order = coretext_member_order(info.name());
    let mut fonts: Vec<(usize, super::FontInfo)> = Vec::with_capacity(info.fonts().len());
    for font in info.fonts() {
        let mut font = font.clone();
        let ps = postscript_name(&font);
        if let Some(ps) = &ps {
            // variable faces keep their wght axis handling
            if !font.has_weight_axis() {
                if let Some(w) = coretext_weight(ps) {
                    font.set_weight(crate::FontWeight::new(css_weight_from_coretext(w)));
                }
            }
            // an italic CoreText knows of that the face's own tables didn't flag
            if coretext_italic(ps) == Some(true) && font.style() == crate::FontStyle::Normal {
                font.set_style(crate::FontStyle::Italic);
            }
        }
        let rank = ps.and_then(|p| order.iter().position(|o| *o == p)).unwrap_or(usize::MAX);
        fonts.push((rank, font));
    }
    fonts.sort_by_key(|f| f.0);
    Some(info.with_fonts(fonts.into_iter().map(|f| f.1)))
}
