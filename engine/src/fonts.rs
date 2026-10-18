// Portions ported from HyperFrames (Apache-2.0, https://github.com/heygen-com/hyperframes); see NOTICE.
//! Deterministic fonts, ported from the composition format's compiler (packages/core/src/fonts/
//! deterministicFonts.ts + parsers/src/fontAliases.ts). The HTML composition format never lets Chrome pick host
//! fonts for named families: every requested family gets `@font-face` rules injected after the
//! page's stylesheets, so renders look the same on every machine. To match its output pixel for
//! pixel we inject the same faces:
//!
//! * aliased system families (Helvetica, Arial, SF Pro → Inter; Times, Georgia → EB Garamond;
//!   Courier, Menlo → JetBrains Mono ...) get the bundled Fontsource latin faces, plus Google
//!   Fonts faces of the canonical family for the weights/styles the bundle lacks;
//! * any other family is looked up on Google Fonts (all weights 100–900, italic 400/700);
//! * generic families (`sans-serif`, `system-ui` ...) and families the page declares itself are
//!   left alone.

use crate::compile::{all_elements, append_to, attr, find_tag, tag};
use crate::net::SyncNet;
use blitz_dom::{BaseDocument, LocalName, QualName, ns};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Lower-cased family → canonical bundled family (the composition format's FONT_ALIAS_MAP).
const ALIASES: &[(&str, &str)] = &[
    ("inter", "inter"), ("montserrat", "montserrat"), ("outfit", "outfit"), ("nunito", "nunito"),
    ("oswald", "oswald"), ("league gothic", "league-gothic"), ("archivo black", "archivo-black"),
    ("space mono", "space-mono"), ("ibm plex mono", "ibm-plex-mono"), ("jetbrains mono", "jetbrains-mono"),
    ("eb garamond", "eb-garamond"), ("playfair display", "playfair-display"), ("source code pro", "source-code-pro"),
    ("noto sans jp", "noto-sans-jp"), ("roboto", "roboto"), ("open sans", "open-sans"), ("lato", "lato"),
    ("poppins", "poppins"),
    ("helvetica neue", "inter"), ("helvetica", "inter"), ("arial", "inter"), ("helvetica bold", "inter"),
    ("futura", "montserrat"), ("din alternate", "montserrat"), ("arial black", "montserrat"),
    ("bebas neue", "league-gothic"), ("courier new", "jetbrains-mono"), ("courier", "jetbrains-mono"),
    ("garamond", "eb-garamond"), ("noto sans japanese", "noto-sans-jp"), ("segoe ui", "roboto"),
    ("sf pro", "inter"), ("sf pro display", "inter"), ("sf pro text", "inter"), ("sf pro rounded", "inter"),
    ("avenir", "inter"), ("avenir next", "inter"), ("lucida grande", "inter"), ("geneva", "inter"),
    ("optima", "inter"), ("verdana", "inter"), ("tahoma", "inter"), ("trebuchet ms", "inter"),
    ("calibri", "inter"), ("candara", "inter"), ("corbel", "inter"), ("lucida sans", "inter"),
    ("lucida sans unicode", "inter"), ("noto sans", "inter"), ("dejavu sans", "inter"),
    ("liberation sans", "inter"), ("sf mono", "jetbrains-mono"), ("menlo", "jetbrains-mono"),
    ("monaco", "jetbrains-mono"), ("consolas", "jetbrains-mono"), ("lucida console", "jetbrains-mono"),
    ("lucida sans typewriter", "jetbrains-mono"), ("andale mono", "jetbrains-mono"),
    ("dejavu sans mono", "jetbrains-mono"), ("liberation mono", "jetbrains-mono"),
    ("georgia", "eb-garamond"), ("palatino", "eb-garamond"), ("palatino linotype", "eb-garamond"),
    ("book antiqua", "eb-garamond"), ("cambria", "eb-garamond"), ("times", "eb-garamond"),
    ("times new roman", "eb-garamond"), ("dejavu serif", "eb-garamond"), ("liberation serif", "eb-garamond"),
];

/// `<canvas>` text: the bundled (WOFF2) face the composition format's deterministic fonts would give `family`
/// (aliases applied), nearest weight first, matching style preferred.
pub fn canvas_face(family: &str, weight: u16, italic: bool) -> Option<&'static [u8]> {
    let lower = family.trim().to_ascii_lowercase();
    let canonical = ALIASES.iter().find(|(k, _)| *k == lower).map(|(_, v)| *v)?;
    let style = if italic { "italic" } else { "normal" };
    BUNDLED
        .iter()
        .filter(|(f, _, _, _)| *f == canonical)
        .min_by_key(|(_, w, st, _)| {
            let w: i32 = w.parse().unwrap_or(400);
            ((*st != style) as i32) * 10_000 + (w - weight as i32).abs()
        })
        .map(|(_, _, _, bytes)| *bytes)
}

/// The bundled face for `family` as TrueType/OpenType bytes (WOFF2 decoded), for code that reads outlines itself
/// (rdb's text3d shapes). None when no bundled face matches.
pub fn face_ttf(family: &str, weight: u16, italic: bool) -> Option<Vec<u8>> {
    canvas_face(family, weight, italic).and_then(|w| wuff::decompress_woff2(w).ok())
}

/// Canonical family → display name on Google Fonts.
fn display_name(canonical: &str) -> &'static str {
    match canonical {
        "inter" => "Inter",
        "montserrat" => "Montserrat",
        "outfit" => "Outfit",
        "nunito" => "Nunito",
        "oswald" => "Oswald",
        "league-gothic" => "League Gothic",
        "archivo-black" => "Archivo Black",
        "space-mono" => "Space Mono",
        "ibm-plex-mono" => "IBM Plex Mono",
        "jetbrains-mono" => "JetBrains Mono",
        "eb-garamond" => "EB Garamond",
        "playfair-display" => "Playfair Display",
        "source-code-pro" => "Source Code Pro",
        "noto-sans-jp" => "Noto Sans JP",
        "roboto" => "Roboto",
        "open-sans" => "Open Sans",
        "lato" => "Lato",
        "poppins" => "Poppins",
        _ => "",
    }
}

const GENERIC: &[&str] = &[
    "sans-serif", "serif", "monospace", "cursive", "fantasy", "system-ui", "ui-sans-serif", "ui-serif",
    "ui-monospace", "ui-rounded", "emoji", "math", "fangsong", "-apple-system", "blinkmacsystemfont",
    "inherit", "initial", "unset", "revert", "revert-layer",
];

/// The Fontsource latin subset every bundled face covers.
const BUNDLED_RANGE: &str = "U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+2074, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD";

/// (canonical family, weight, style, woff2 bytes)
const BUNDLED: &[(&str, &str, &str, &[u8])] = &include!(concat!(env!("OUT_DIR"), "/bundled_fonts.rs"));

fn data_url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    format!("data:font/woff2;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn face_rule(family: &str, src: &str, weight: &str, style: &str, range: Option<&str>) -> String {
    let mut s = format!(
        "@font-face {{\n  font-family: \"{family}\";\n  src: url(\"{src}\") format(\"woff2\");\n  font-style: {style};\n  font-weight: {weight};\n  font-display: block;\n"
    );
    if let Some(r) = range {
        s.push_str(&format!("  unicode-range: {r};\n"));
    }
    s.push('}');
    s
}

fn face_rule_format(family: &str, src: &str, weight: &str, style: &str, format: &str) -> String {
    format!(
        "@font-face {{\n  font-family: \"{family}\";\n  src: url(\"{src}\") format(\"{format}\");\n  font-style: {style};\n  font-weight: {weight};\n  font-display: block;\n}}"
    )
}

fn sfnt_data_url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    format!("data:font/ttf;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// The composition format's system font capture (core/src/fonts/systemFontLocator.ts + deterministicFonts.ts
/// `systemFamilyFaceRules`): a family that's neither aliased nor on Google Fonts is embedded from the
/// installed file(s) in the user/system font directories, one @font-face per file with weight and style
/// guessed from the path, the first file winning per weight/style. A collection (.ttc) is one variant and
/// Chrome loads its first face, so every weight and style of e.g. Didot is synthesized from face 0.
fn system_capture(family: &str) -> Vec<(String, String, Vec<u8>)> {
    let want = family.trim().trim_matches(|c| c == '"' || c == '\'').trim().to_ascii_lowercase();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for path in system_font_index().get(&want).into_iter().flatten() {
        let (weight, style) = infer_weight_and_style(&path.to_string_lossy());
        if !seen.insert(format!("{weight}:{style}")) {
            continue;
        }
        let Ok(bytes) = std::fs::read(path) else { continue };
        let face = if bytes.starts_with(b"ttcf") { collection_face(&bytes, 0) } else { Some(bytes) };
        if let Some(face) = face {
            out.push((weight.to_string(), style.to_string(), face));
        }
    }
    out
}

/// The composition format's `inferWeightAndStyle`: tokens anywhere in the lowercased path, longest first.
fn infer_weight_and_style(path: &str) -> (&'static str, &'static str) {
    let lower = path.to_ascii_lowercase();
    let lower = lower.rsplit_once('.').map(|(a, _)| a.to_string()).unwrap_or(lower);
    let style = if lower.contains("italic") || lower.contains("oblique") { "italic" } else { "normal" };
    const TOKENS: &[(&str, &str)] = &[
        ("ultrablack", "950"), ("extralight", "200"), ("ultralight", "200"), ("extrabold", "800"), ("ultrabold", "800"),
        ("semibold", "600"), ("demibold", "600"), ("hairline", "100"), ("regular", "400"), ("normal", "400"),
        ("medium", "500"), ("light", "300"), ("black", "900"), ("heavy", "800"), ("roman", "400"), ("thin", "100"),
        ("book", "400"), ("bold", "700"),
    ];
    for (token, weight) in TOKENS {
        if lower.contains(token) {
            return (weight, style);
        }
    }
    ("400", style)
}

/// Installed font files by the composition format's file-name family (`toFamilyName`, lowercased), in its font
/// directories and directory order. system_profiler reports paths per file, not per typeface, so
/// The composition format's profiler index stays empty and this file-name scan is what decides (e.g. "Hoefler Text
/// Ornaments.ttf" is not "Hoefler Text"). Built once per process.
fn system_font_index() -> &'static HashMap<String, Vec<std::path::PathBuf>> {
    static INDEX: std::sync::OnceLock<HashMap<String, Vec<std::path::PathBuf>>> = std::sync::OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index: HashMap<String, Vec<std::path::PathBuf>> = HashMap::new();
        let mut dirs = Vec::new();
        if cfg!(target_os = "ios") {
            dirs.push(std::path::PathBuf::from("/System/Library/Fonts"));
        }
        if cfg!(target_os = "macos") {
            if let Some(home) = std::env::var_os("HOME") {
                dirs.push(std::path::PathBuf::from(home).join("Library/Fonts"));
            }
            dirs.extend(["/Library/Fonts", "/System/Library/Fonts", "/System/Library/Fonts/Supplemental"].map(std::path::PathBuf::from));
        }
        for dir in dirs {
            let mut files = Vec::new();
            collect_font_files(&dir, 0, &mut files);
            for path in files {
                let Some(family) = path.file_name().and_then(|f| f.to_str()).and_then(to_family_name) else { continue };
                index.entry(family.to_ascii_lowercase()).or_default().push(path);
            }
        }
        index
    })
}

/// The composition format's `toFamilyName`: file name -> family (camel case split, trailing style words dropped).
fn to_family_name(file_name: &str) -> Option<String> {
    let lower = file_name.to_ascii_lowercase();
    let ext = [".woff2", ".woff", ".otf", ".ttf", ".ttc"].iter().find(|e| lower.ends_with(*e))?;
    let stem = &file_name[..file_name.len() - ext.len()];
    if stem.is_empty() || stem.starts_with('.') {
        return None;
    }
    let chars: Vec<char> = stem.chars().map(|c| if c == '_' || c == '-' { ' ' } else { c }).collect();
    let mut spaced = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && c.is_ascii_uppercase() {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            // ([a-z])([A-Z]) and ([A-Z]+)([A-Z][a-z])
            if prev.is_ascii_lowercase() || (prev.is_ascii_uppercase() && next_lower) {
                spaced.push(' ');
            }
        }
        spaced.push(c);
    }
    const STYLE_SUFFIXES: &[&str] = &[
        "black", "bold", "book", "condensed", "demi", "demibold", "display", "extra", "extrabold", "hairline", "heavy",
        "italic", "light", "medium", "normal", "regular", "roman", "semibold", "thin", "ultra", "ultralight",
    ];
    let mut words: Vec<&str> = spaced.split_whitespace().collect();
    while words.len() > 1 && STYLE_SUFFIXES.contains(&words[words.len() - 1].to_ascii_lowercase().as_str()) {
        words.pop();
    }
    let family = words.join(" ");
    (family.len() >= 2).then_some(family)
}

fn collect_font_files(dir: &std::path::Path, depth: u32, out: &mut Vec<std::path::PathBuf>) {
    if depth > 2 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_font_files(&p, depth + 1, out);
        } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| ["ttf", "otf", "ttc", "woff", "woff2"].contains(&x.to_ascii_lowercase().as_str())) {
            out.push(p);
        }
    }
}

/// Face `index` of a TrueType/OpenType collection as a standalone font (tables copied, 4-byte aligned).
fn collection_face(ttc: &[u8], index: u32) -> Option<Vec<u8>> {
    let be32 = |o: usize| ttc.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let be16 = |o: usize| ttc.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let off = be32(12 + 4 * index as usize)? as usize;
    let num_tables = be16(off + 4)? as usize;
    let header_len = 12 + 16 * num_tables;
    let mut out = Vec::with_capacity(header_len);
    out.extend_from_slice(ttc.get(off..off + 12)?);
    let mut tables = Vec::new();
    for t in 0..num_tables {
        let rec = off + 12 + 16 * t;
        tables.push((ttc.get(rec..rec + 4)?.to_vec(), be32(rec + 4)?, be32(rec + 8)? as usize, be32(rec + 12)? as usize));
    }
    out.resize(header_len, 0);
    for (t, (tag, checksum, toff, len)) in tables.into_iter().enumerate() {
        let data = ttc.get(toff..toff + len)?;
        let new_off = out.len() as u32;
        out.extend_from_slice(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        let rec = 12 + 16 * t;
        out[rec..rec + 4].copy_from_slice(&tag);
        out[rec + 4..rec + 8].copy_from_slice(&checksum.to_be_bytes());
        out[rec + 8..rec + 12].copy_from_slice(&new_off.to_be_bytes());
        out[rec + 12..rec + 16].copy_from_slice(&(len as u32).to_be_bytes());
    }
    Some(out)
}

/// Families named by `font-family` / `font` declarations (and font custom properties), each with
/// whether a `font-family` declaration names it (the HTML composition format reads only those; see `inject`).
fn requested_families(css_texts: &[String]) -> Vec<(String, bool)> {
    let mut vars: BTreeMap<String, String> = BTreeMap::new();
    let mut decls: Vec<(String, bool)> = Vec::new();
    for css in css_texts {
        for (name, value) in declarations(css) {
            let n = name.trim().to_ascii_lowercase();
            if n.starts_with("--") {
                vars.insert(n, value.clone());
            } else if n == "font-family" {
                decls.push((value, true));
            } else if n == "font" {
                if let Some(f) = shorthand_families(&value) {
                    decls.push((f, false));
                }
            }
        }
    }
    let mut out: Vec<(String, bool)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (d, longhand) in decls {
        for fam in split_families(&expand_vars(&d, &vars)) {
            let lower = fam.to_ascii_lowercase();
            if fam.is_empty() || GENERIC.contains(&lower.as_str()) || lower.starts_with("var(") {
                continue;
            }
            match index.get(&lower) {
                Some(&i) => out[i].1 |= longhand,
                None => {
                    index.insert(lower, out.len());
                    out.push((fam, longhand));
                }
            }
        }
    }
    out
}

fn expand_vars(value: &str, vars: &BTreeMap<String, String>) -> String {
    let mut v = value.to_string();
    for _ in 0..8 {
        let Some(i) = v.find("var(") else { break };
        let Some(j) = v[i..].find(')').map(|j| i + j) else { break };
        let inner = &v[i + 4..j];
        let (name, fallback) = match inner.split_once(',') {
            Some((n, f)) => (n.trim().to_ascii_lowercase(), Some(f.trim().to_string())),
            None => (inner.trim().to_ascii_lowercase(), None),
        };
        let rep = vars.get(&name).cloned().or(fallback).unwrap_or_default();
        v.replace_range(i..=j, &rep);
    }
    v
}

/// `name: value` pairs from CSS text (rules, inline styles); nested blocks are flattened.
fn declarations(css: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let cleaned = strip_comments(css);
    for chunk in cleaned.split(['{', '}', ';']) {
        if let Some((k, v)) = chunk.split_once(':') {
            let k = k.trim();
            if k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') && !k.is_empty() {
                out.push((k.to_string(), v.trim().trim_end_matches("!important").trim().to_string()));
            }
        }
    }
    out
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        match rest[i + 2..].find("*/") {
            Some(j) => rest = &rest[i + 2 + j + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The family list of a `font` shorthand: everything after the size (and `/line-height`).
fn shorthand_families(value: &str) -> Option<String> {
    let v = value.trim();
    let tokens: Vec<&str> = v.split_whitespace().collect();
    for (i, t) in tokens.iter().enumerate() {
        let size = t.split('/').next().unwrap_or("");
        let is_size = size.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '.')
            && !size.chars().all(|c| c.is_ascii_digit())
            || ["xx-small", "x-small", "small", "medium", "large", "x-large", "xx-large", "smaller", "larger"].contains(&size);
        if is_size {
            let mut rest = tokens[i + 1..].join(" ");
            if t.contains('/') {
                // "16px/1.4 Inter"
            } else if rest.starts_with('/') {
                rest = rest.split_once(' ').map(|(_, r)| r.to_string()).unwrap_or_default();
            }
            return Some(rest);
        }
    }
    None
}

fn split_families(list: &str) -> Vec<String> {
    list.split(',')
        .map(|f| f.trim().trim_matches(|c| c == '"' || c == '\'').trim().to_string())
        .filter(|f| !f.is_empty())
        .collect()
}

/// Families the page defines itself with `@font-face`.
fn declared_families(css_texts: &[String]) -> HashSet<String> {
    let mut out = HashSet::new();
    for css in css_texts {
        let lower = css.to_ascii_lowercase();
        let mut rest = lower.as_str();
        while let Some(i) = rest.find("@font-face") {
            rest = &rest[i + 10..];
            let end = rest.find('}').unwrap_or(rest.len());
            for (k, v) in declarations(&rest[..end]) {
                if k == "font-family" {
                    for f in split_families(&v) {
                        out.insert(f.to_ascii_lowercase());
                    }
                }
            }
        }
    }
    out
}

struct GoogleFace {
    weight: String,
    style: String,
    url: String,
    range: Option<String>,
}

/// JavaScript's `encodeURIComponent`.
fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The composition format's `withPageText` (core/src/fonts/authoredGoogleFonts.ts): adds `text=` to a Google
/// Fonts URL that has none.
fn with_page_text(url: &str, text: Option<&str>) -> String {
    let Some(text) = text else { return url.to_string() };
    let Ok(parsed) = url::Url::parse(url) else { return url.to_string() };
    if parsed.query_pairs().any(|(k, _)| k == "text") {
        return url.to_string();
    }
    let (base, hash) = match url.split_once('#') {
        Some((b, h)) => (b, format!("#{h}")),
        None => (url, String::new()),
    };
    let joiner = if parsed.query().is_some_and(|q| !q.is_empty()) { "&" } else { "?" };
    format!("{base}{joiner}text={}{hash}", encode_uri_component(text))
}

/// The faces of one Google Fonts css2 response for `family`, in response order, as the composition format's
/// `fetchGoogleFontStylesheet` reads them: woff2 sources only, `font-weight` kept whole (a variable
/// face is `400 900`), each face's `unicode-range`.
fn google_stylesheet_faces(family: &str, url: &str, net: &SyncNet) -> Vec<GoogleFace> {
    let Ok(u) = url::Url::parse(url) else { return vec![] };
    let Ok(bytes) = net.load_with_agent(&u, crate::net::CHROME_USER_AGENT) else { return vec![] };
    let css = String::from_utf8_lossy(&bytes);
    let mut faces = Vec::new();
    let mut rest = css.as_ref();
    while let Some(i) = rest.find("@font-face") {
        rest = &rest[i + 10..];
        let end = rest.find('}').unwrap_or(rest.len());
        let block = &rest[..end];
        let mut face = GoogleFace { weight: "400".into(), style: "normal".into(), url: String::new(), range: None };
        let mut woff2 = false;
        let mut declared = None;
        for (k, v) in declarations(block) {
            match k.as_str() {
                "font-family" => declared = Some(v.trim_matches(|c| c == '"' || c == '\'').to_string()),
                "font-weight" => face.weight = v.split_whitespace().collect::<Vec<_>>().join(" "),
                "font-style" => face.style = v,
                "unicode-range" => face.range = Some(v.trim().to_string()),
                "src" => {
                    woff2 = v.contains("format('woff2')") || v.contains("format(\"woff2\")");
                    if let Some(a) = v.find("url(") {
                        if let Some(b) = v[a..].find(')') {
                            face.url = v[a + 4..a + b].trim_matches(|c| c == '"' || c == '\'').to_string();
                        }
                    }
                }
                _ => {}
            }
        }
        let same_family = declared.as_deref().is_none_or(|d| d.eq_ignore_ascii_case(family));
        if woff2 && same_family && !face.url.is_empty() {
            faces.push(face);
        }
        rest = &rest[end..];
    }
    faces
}

/// The composition format's `fetchGoogleFont`: the page's own Google Fonts stylesheets for the family when it
/// links any (and the family isn't bundled), else every weight 100-900 plus italic 400/700, each
/// narrowed to the page's characters with `text=`.
fn google_faces(family: &str, net: &SyncNet, text: Option<&str>, authored: Option<&Vec<String>>) -> Vec<GoogleFace> {
    let text_param = text.map(|t| format!("&text={}", encode_uri_component(t))).unwrap_or_default();
    let default_url = format!(
        "https://fonts.googleapis.com/css2?family={}:ital,wght@0,100;0,200;0,300;0,400;0,500;0,600;0,700;0,800;0,900;1,400;1,700{text_param}",
        encode_uri_component(family)
    );
    if let Some(urls) = authored.filter(|u| !u.is_empty()) {
        let faces: Vec<GoogleFace> = urls.iter().flat_map(|u| google_stylesheet_faces(family, &with_page_text(u, text), net)).collect();
        if !faces.is_empty() {
            return faces;
        }
    }
    google_stylesheet_faces(family, &default_url, net)
}

/// The composition format's `extractGoogleFontsText`: every character of the page's HTML and body text with
/// its case variants (the `text=` the HTML composition format sends to Google Fonts), or `None` past 1700 encoded
/// bytes.
fn google_fonts_text(doc: &BaseDocument, inlined_css: &[String]) -> Option<String> {
    let mut html = find_tag(doc, "html").map(|h| crate::compile::outer_html(doc, h)).unwrap_or_default();
    for css in inlined_css {
        html.push_str(css);
    }
    let body = find_tag(doc, "body").and_then(|b| doc.get_node(b)).map(|n| n.text_content()).unwrap_or_default();
    let mut locales: Vec<String> = Vec::new();
    for id in all_elements(doc) {
        if let Some(l) = attr(doc, id, "lang") {
            locales.push(l.to_ascii_lowercase());
        }
    }
    let turkic = locales.iter().any(|l| l.starts_with("tr") || l.starts_with("az"));
    let mut seen = HashSet::new();
    let mut out = String::new();
    let mut add = |c: char, out: &mut String| {
        if seen.insert(c) {
            out.push(c);
        }
    };
    // the compiler's own additions to the page it reads the text from
    let injected = "<style data-raea-text-rendering=\"true\">html,body,*{text-rendering:geometricPrecision}</style>Inter, ";
    let mut sources = HashSet::new();
    for c in html.chars().chain(body.chars()).chain(injected.chars()) {
        if !sources.insert(c) {
            continue;
        }
        add(c, &mut out);
        for v in c.to_uppercase().chain(c.to_lowercase()) {
            add(v, &mut out);
        }
        if turkic {
            match c {
                'i' => add('İ', &mut out),
                'I' => add('ı', &mut out),
                _ => {}
            }
        }
    }
    let lower = html.to_ascii_lowercase();
    if lower.contains("text-transform") && lower.contains("full-width") {
        for c in out.clone().chars() {
            if ('\u{21}'..='\u{7e}').contains(&c) {
                add(char::from_u32(c as u32 + 0xfee0).unwrap(), &mut out);
            }
        }
    }
    (encode_uri_component(&out).len() <= 1700).then_some(out)
}

/// Google Fonts stylesheets a CSS text `@import`s (screen/all media), by lower-cased family.
fn google_imports(css: &str) -> Vec<(String, Vec<String>)> {
    let screen = |media: &str| {
        media.trim().is_empty() || media.split(',').any(|q| matches!(q.trim().to_ascii_lowercase().as_str(), "all" | "screen"))
    };
    let mut urls = Vec::new();
    for stmt in strip_comments(css).split(';') {
        let stmt = stmt.trim();
        let Some(params) = stmt.strip_prefix("@import") else { continue };
        let params = params.trim();
        let (url, media) = if let Some(r) = params.strip_prefix("url(") {
            let Some(close) = r.find(')') else { continue };
            (r[..close].trim().trim_matches(|c| c == '"' || c == '\''), &r[close + 1..])
        } else if let Some(q) = params.chars().next().filter(|c| *c == '"' || *c == '\'') {
            let Some(close) = params[1..].find(q) else { continue };
            (&params[1..1 + close], &params[close + 2..])
        } else {
            continue;
        };
        if screen(media) {
            urls.push(url.to_string());
        }
    }
    google_url_families(urls)
}

/// Groups Google Fonts css/css2 URLs by the lower-cased families they request.
fn google_url_families(urls: Vec<String>) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for u in urls {
        let Ok(parsed) = url::Url::parse(&u) else { continue };
        if parsed.scheme() != "https" || parsed.host_str() != Some("fonts.googleapis.com") {
            continue;
        }
        for (k, v) in parsed.query_pairs() {
            if k != "family" {
                continue;
            }
            for fam in v.split('|') {
                let name = fam.split(':').next().unwrap_or("").trim().to_ascii_lowercase();
                if name.is_empty() {
                    continue;
                }
                match out.iter_mut().find(|(f, _)| *f == name) {
                    Some((_, list)) => {
                        list.retain(|x| x != &u);
                        list.push(u.clone());
                    }
                    None => out.push((name, vec![u.clone()])),
                }
            }
        }
    }
    out
}

/// The composition format's `authoredGoogleFontStylesheets`: Google Fonts stylesheet URLs the page links or
/// `@import`s (screen/all media only), by lower-cased family.
fn authored_google_stylesheets(doc: &BaseDocument) -> HashMap<String, Vec<String>> {
    let screen = |media: Option<String>| {
        media.is_none_or(|m| m.trim().is_empty() || m.split(',').any(|q| matches!(q.trim().to_ascii_lowercase().as_str(), "all" | "screen")))
    };
    let mut by_family: HashMap<String, Vec<String>> = HashMap::new();
    for id in all_elements(doc) {
        let t = tag(doc, id);
        if t != "link" && t != "style" {
            continue;
        }
        if crate::compile::has_attr(doc, id, "disabled") || !screen(attr(doc, id, "media")) {
            continue;
        }
        if attr(doc, id, "type").is_some_and(|ty| !ty.trim().is_empty() && !ty.trim().eq_ignore_ascii_case("text/css")) {
            continue;
        }
        let mut urls = Vec::new();
        if t == "link" {
            let rel = attr(doc, id, "rel").unwrap_or_default().to_ascii_lowercase();
            let rels: Vec<&str> = rel.split_whitespace().collect();
            if rels.contains(&"stylesheet") && !rels.contains(&"alternate") {
                urls.extend(attr(doc, id, "href"));
            }
        } else {
            let css = doc.get_node(id).map(|n| n.text_content()).unwrap_or_default();
            for stmt in strip_comments(&css).split(';') {
                let stmt = stmt.trim();
                let Some(params) = stmt.strip_prefix("@import") else { continue };
                let params = params.trim();
                let (url, media) = if let Some(r) = params.strip_prefix("url(") {
                    let Some(close) = r.find(')') else { continue };
                    (r[..close].trim().trim_matches(|c| c == '"' || c == '\''), &r[close + 1..])
                } else if let Some(q) = params.chars().next().filter(|c| *c == '"' || *c == '\'') {
                    let Some(close) = params[1..].find(q) else { continue };
                    (&params[1..1 + close], &params[close + 2..])
                } else {
                    continue;
                };
                if screen(Some(media.to_string())) {
                    urls.push(url.to_string());
                }
            }
        }
        for u in urls {
            let Ok(parsed) = url::Url::parse(&u) else { continue };
            if parsed.scheme() != "https" || parsed.host_str() != Some("fonts.googleapis.com") {
                continue;
            }
            for (k, v) in parsed.query_pairs() {
                if k != "family" {
                    continue;
                }
                for fam in v.split('|') {
                    let name = fam.split(':').next().unwrap_or("").trim().to_ascii_lowercase();
                    if name.is_empty() {
                        continue;
                    }
                    let list = by_family.entry(name).or_default();
                    list.retain(|x| x != &u);
                    list.push(u.clone());
                }
            }
        }
    }
    by_family
}

/// The composition format's `bundledFamilyFaceRules` supplement: Google faces the bundle doesn't already serve
/// (other weights, or ranges other than the bundled latin subset), collapsed into weight runs of
/// one source that don't span a bundled weight, in response order.
fn supplement_rules(family: &str, faces: Vec<GoogleFace>, covered: &HashSet<String>) -> Vec<String> {
    let norm_weight = |w: &str| w.trim().parse::<f64>().map(|n| n.to_string()).unwrap_or_else(|_| w.trim().to_ascii_lowercase());
    let key = |f: &GoogleFace| format!("{}:{}", norm_weight(&f.weight), f.style);
    let bundled_range = |r: &Option<String>| r.as_deref().is_some_and(|r| r.replace(' ', "").eq_ignore_ascii_case(&BUNDLED_RANGE.replace(' ', "")));
    let supplementary: Vec<GoogleFace> = faces.into_iter().filter(|f| !covered.contains(&key(f)) || !bundled_range(&f.range)).collect();
    // group by source, keeping the order groups first appear in
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut group_of: HashMap<(String, String, String), usize> = HashMap::new();
    for (i, f) in supplementary.iter().enumerate() {
        let k = (f.url.clone(), f.style.clone(), f.range.clone().unwrap_or_default());
        let g = *group_of.entry(k).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[g].push(i);
    }
    let spans_covered = |a: &GoogleFace, b: &GoogleFace| {
        let (Ok(x), Ok(y)) = (a.weight.trim().parse::<f64>(), b.weight.trim().parse::<f64>()) else { return true };
        let (lo, hi) = (x.min(y), x.max(y));
        covered.iter().any(|c| {
            let (w, st) = c.split_once(':').unwrap_or((c, ""));
            st == a.style && w.parse::<f64>().is_ok_and(|v| v > lo && v < hi)
        })
    };
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for g in groups {
        let (mut sortable, unsortable): (Vec<usize>, Vec<usize>) =
            g.into_iter().partition(|&i| supplementary[i].weight.trim().parse::<f64>().is_ok());
        sortable.sort_by(|&a, &b| {
            let wa: f64 = supplementary[a].weight.trim().parse().unwrap();
            let wb: f64 = supplementary[b].weight.trim().parse().unwrap();
            wa.partial_cmp(&wb).unwrap()
        });
        let mut current: Vec<usize> = Vec::new();
        for i in sortable {
            if let Some(&prev) = current.last() {
                if spans_covered(&supplementary[prev], &supplementary[i]) {
                    runs.push(std::mem::take(&mut current));
                }
            }
            current.push(i);
        }
        if !current.is_empty() {
            runs.push(current);
        }
        runs.extend(unsortable.into_iter().map(|i| vec![i]));
    }
    runs.sort_by_key(|r| r.iter().copied().min().unwrap_or(usize::MAX));
    runs.into_iter()
        .map(|run| {
            let first = &supplementary[run[0]];
            let last = &supplementary[*run.last().unwrap()];
            let weight = if run.len() > 1 { format!("{} {}", first.weight, last.weight) } else { first.weight.clone() };
            face_rule(family, &first.url, &weight, &first.style, first.range.as_deref())
        })
        .collect()
}

/// The composition format's `GENERIC_FAMILIES` (core/src/fonts/deterministicFonts.ts): families that resolve to
/// a host font, which its compiler won't leave as a primary family.
const HF_GENERIC_FAMILIES: &[&str] = &[
    "sans-serif", "serif", "monospace", "cursive", "fantasy", "system-ui", "ui-sans-serif", "ui-serif",
    "ui-monospace", "emoji", "math", "fangsong", "-apple-system", "blinkmacsystemfont",
];

/// `parseFontFamilyValue`: split at top-level commas (not inside quotes or parentheses), trim and
/// unquote.
fn parse_family_list(value: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let (mut start, mut depth, mut quote) = (0, 0usize, None::<char>);
    let mut chars = value.char_indices();
    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            chars.next();
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                pieces.push(&value[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    pieces.push(&value[start..]);
    pieces
        .into_iter()
        .map(|p| {
            let p = p.trim();
            let p = p.strip_prefix(['\'', '"']).unwrap_or(p);
            let p = p.strip_suffix(['\'', '"']).unwrap_or(p);
            p.trim().to_string()
        })
        .filter(|p| !p.is_empty())
        .collect()
}

/// `primaryCssVariable`: a value starting with `var(--name[, fallback])`, as (name, fallback, rest).
fn primary_css_variable(value: &str) -> Option<(String, Option<String>, String)> {
    let t = value.trim();
    if !t.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("var(")) {
        return None;
    }
    let mut depth = 0i32;
    for (i, c) in t.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth != 0 {
                    continue;
                }
                let inner = t[4..i].trim();
                let (name, fallback) = match inner.find(',') {
                    Some(k) => (inner[..k].trim(), Some(inner[k + 1..].to_string())),
                    None => (inner, None),
                };
                let valid = name.len() > 2
                    && name.starts_with("--")
                    && name[2..].chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
                return valid.then(|| (name.to_string(), fallback, t[i + 1..].to_string()));
            }
            _ => {}
        }
    }
    None
}

/// `systemPrimaryReplacement`: `Inter, <value>` when the value's first family is generic; through a
/// leading `var()` only when that property is never defined.
fn system_primary_replacement(value: &str, custom: &HashMap<String, String>) -> Option<String> {
    if let Some((name, fallback, rest)) = primary_css_variable(value) {
        if custom.get(&name).is_some_and(|v| !v.is_empty()) {
            return None;
        }
        let fallback = system_primary_replacement(&fallback?, custom)?;
        return Some(format!("var({name}, {fallback}){rest}"));
    }
    let families = parse_family_list(value);
    let first = families.first()?.to_ascii_lowercase();
    HF_GENERIC_FAMILIES.contains(&first.as_str()).then(|| format!("Inter, {}", value.trim()))
}

/// The declarations of a style sheet (or of an inline style when `inline`), as
/// (property, value range in `css`, inside `@font-face`).
fn css_declaration_spans(css: &str, inline: bool) -> Vec<(String, std::ops::Range<usize>, bool)> {
    let b = css.as_bytes();
    let mut out = Vec::new();
    // per open block: does it hold declarations, is it @font-face
    let mut blocks: Vec<(bool, bool)> = Vec::new();
    let mut seg = 0; // start of the current prelude or declaration
    let mut i = 0;
    let decl_block = |blocks: &Vec<(bool, bool)>| if inline { blocks.is_empty() || blocks.last().unwrap().0 } else { blocks.last().is_some_and(|b| b.0) };
    let push_decl = |seg: usize, end: usize, blocks: &Vec<(bool, bool)>, out: &mut Vec<_>| {
        if !decl_block(blocks) {
            return;
        }
        let text = &css[seg..end];
        let Some(colon) = text.find(':') else { return };
        let prop = text[..colon].trim();
        if prop.is_empty() || !prop.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return;
        }
        out.push((prop.to_string(), seg + colon + 1..end, blocks.last().is_some_and(|b| b.1)));
    };
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = css[i + 2..].find("*/").map_or(b.len(), |j| i + 2 + j + 2);
                continue;
            }
            q @ (b'"' | b'\'') => {
                i += 1;
                while i < b.len() && b[i] != q {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'(' => {
                let mut depth = 1;
                i += 1;
                while i < b.len() && depth > 0 {
                    match b[i] {
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        b'"' | b'\'' => {
                            let q = b[i];
                            i += 1;
                            while i < b.len() && b[i] != q {
                                i += if b[i] == b'\\' { 2 } else { 1 };
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                continue;
            }
            b'{' => {
                let prelude = css[seg..i].trim();
                let at = prelude.starts_with('@');
                let font_face = prelude.eq_ignore_ascii_case("@font-face");
                // a style rule or @font-face/@page-like at-rule holds declarations; @media etc. hold rules
                let holds_decls = !at || font_face || !matches!(
                    prelude.split_whitespace().next().unwrap_or("").to_ascii_lowercase().as_str(),
                    "@media" | "@supports" | "@layer" | "@container" | "@document" | "@scope" | "@starting-style"
                );
                blocks.push((holds_decls, font_face));
                seg = i + 1;
            }
            b';' => {
                push_decl(seg, i, &blocks, &mut out);
                seg = i + 1;
            }
            b'}' => {
                push_decl(seg, i, &blocks, &mut out);
                blocks.pop();
                seg = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    push_decl(seg, b.len().min(css.len()), &blocks, &mut out);
    out
}

/// Applies `system_primary_replacement` to every `font-family` and custom property declaration
/// (`normalizeCssDeclarations`), keeping `!important`. `None` when nothing changed.
fn rewrite_system_primaries(css: &str, inline: bool, custom: &HashMap<String, String>) -> Option<String> {
    let mut out = String::with_capacity(css.len() + 16);
    let mut last = 0;
    for (prop, range, in_font_face) in css_declaration_spans(css, inline) {
        let is_custom = prop.starts_with("--");
        if !is_custom && (!prop.eq_ignore_ascii_case("font-family") || in_font_face) {
            continue;
        }
        let raw = &css[range.clone()];
        let (value, important) = match raw.trim_end().to_ascii_lowercase().rfind("!important") {
            Some(k) if raw.trim_end().len() == k + 10 => (&raw[..k], true),
            _ => (raw, false),
        };
        let Some(new) = system_primary_replacement(value, custom) else { continue };
        out.push_str(&css[last..range.start]);
        out.push(' ');
        out.push_str(&new);
        if important {
            out.push_str(" !important");
        }
        last = range.end;
    }
    (last > 0).then(|| {
        out.push_str(&css[last..]);
        out
    })
}

/// The composition format's `normalizeSystemFontPrimaryFamilies` (run by its HTML compiler on every
/// composition): a font stack led by a generic or host-UI family (`-apple-system, Helvetica`,
/// `system-ui, sans-serif`, plain `serif`) gets the deterministic Inter promoted in front of it.
fn normalize_system_font_primaries(doc: &mut BaseDocument) {
    let mut custom: HashMap<String, String> = HashMap::new();
    let mut styles = Vec::new();
    let mut inline = Vec::new();
    let mut data_ff = Vec::new();
    for id in all_elements(doc) {
        if tag(doc, id) == "style" {
            let css = doc.get_node(id).map(|n| n.text_content()).unwrap_or_default();
            for (prop, range, _) in css_declaration_spans(&css, false) {
                if prop.starts_with("--") {
                    custom.insert(prop, css[range].trim().to_string());
                }
            }
            styles.push((id, css));
        }
        if let Some(s) = attr(doc, id, "style") {
            for (prop, range, _) in css_declaration_spans(&s, true) {
                if prop.starts_with("--") {
                    custom.insert(prop, s[range].trim().to_string());
                }
            }
            inline.push((id, s));
        }
        if let Some(f) = attr(doc, id, "data-font-family") {
            data_ff.push((id, f));
        }
    }
    let mut m = doc.mutate();
    for (id, css) in styles {
        let Some(new) = rewrite_system_primaries(&css, false, &custom) else { continue };
        let texts: Vec<_> = m.doc.get_node(id).map(|n| n.children.to_vec()).unwrap_or_default();
        if let [only] = texts[..] {
            m.set_node_text(only, &new);
        } else {
            m.remove_and_drop_all_children(id);
            let t = m.create_text_node(&new);
            m.append_children(id, &[t]);
        }
    }
    for (id, s) in inline {
        if let Some(new) = rewrite_system_primaries(&s, true, &custom) {
            m.set_attribute(id, crate::compile::qual("style"), &new);
        }
    }
    for (id, f) in data_ff {
        if let Some(new) = system_primary_replacement(&f, &custom) {
            m.set_attribute(id, crate::compile::qual("data-font-family"), &new);
        }
    }
}

/// Collect requested families from the document's CSS and inject the deterministic faces.
/// Returns a note per family for `lint`/warnings.
/// The page's local stylesheets (`<link rel=stylesheet>` to a relative URL), which the composition format's
/// bundler inlines into `<style>` before its font pass, so their families, Google Fonts `@import`s
/// and characters count as the page's own.
fn local_stylesheets(doc: &BaseDocument, net: &SyncNet) -> Vec<String> {
    let base = doc.url().clone();
    let mut out = Vec::new();
    for id in all_elements(doc) {
        if tag(doc, id) != "link" || crate::compile::has_attr(doc, id, "disabled") {
            continue;
        }
        let rel = attr(doc, id, "rel").unwrap_or_default().to_ascii_lowercase();
        if !rel.split_whitespace().any(|r| r == "stylesheet") {
            continue;
        }
        let Some(href) = attr(doc, id, "href") else { continue };
        let href = href.trim();
        if href.contains("://") || href.starts_with("//") || href.starts_with("data:") {
            continue;
        }
        let Ok(url) = base.join(href) else { continue };
        if let Ok(bytes) = net.load(&url) {
            out.push(String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    out
}

/// A system family the bundle only stands in for (Avenir Next -> Inter, Futura -> Montserrat): a row naming one keeps
/// the installed font, as it always has; the HTML path swaps it for the HTML composition format parity
pub fn is_stand_in(family: &str) -> bool {
    let lower = family.trim().trim_matches(['"', '\'']).to_ascii_lowercase();
    ALIASES.iter().any(|(a, canonical)| *a == lower && !display_name(canonical).eq_ignore_ascii_case(&lower))
}

pub fn inject(doc: &mut BaseDocument, net: &SyncNet) -> Vec<String> {
    inject_with(doc, net, false)
}

/// Whether `family` is installed on this machine (its user/system font directories), by file-name family.
pub fn installed(family: &str) -> bool {
    system_font_index().contains_key(&family.trim().trim_matches(['"', '\'']).trim().to_ascii_lowercase())
}

/// `inject`, or with `native` a page that isn't held to the HTML composition format: an installed family the bundle only stands in
/// for (Avenir Next, Futura) and an installed family Google doesn't serve get no @font-face, so the system lookup
/// picks the real face for each weight and style (Avenir Next Heavy at 800, Didot's own italic) where the HTML composition format
/// paints Inter, or synthesizes every style from a collection's first face. Elsewhere the stand-in still answers.
pub fn inject_with(doc: &mut BaseDocument, net: &SyncNet, native: bool) -> Vec<String> {
    if !native {
        normalize_system_font_primaries(doc);
    }
    let mut css_texts: Vec<String> = Vec::new();
    for id in all_elements(doc) {
        match tag(doc, id).as_str() {
            "style" => css_texts.push(doc.get_node(id).map(|n| n.text_content()).unwrap_or_default()),
            _ => {
                if let Some(s) = attr(doc, id, "style") {
                    css_texts.push(s);
                }
            }
        }
    }
    let local_css = local_stylesheets(doc, net);
    css_texts.extend(local_css.iter().cloned());
    let declared = declared_families(&css_texts);
    let text = google_fonts_text(doc, &local_css);
    let mut authored = authored_google_stylesheets(doc);
    for css in &local_css {
        for (family, urls) in google_imports(css) {
            let list = authored.entry(family).or_default();
            for u in urls {
                list.retain(|x| x != &u);
                list.push(u);
            }
        }
    }
    let mut rules: Vec<String> = Vec::new();
    let mut notes = Vec::new();
    for (family, longhand) in requested_families(&css_texts) {
        let lower = family.to_ascii_lowercase();
        if declared.contains(&lower) {
            continue;
        }
        // The HTML composition format injects faces only for the families of `font-family` declarations, so a system
        // family named only in a `font` shorthand reaches Chrome as authored: the installed Menlo or
        // Helvetica, not the bundled stand-in, and nothing for one Chrome can't name (SF Mono). Bundled
        // and Google Fonts families stay embedded from shorthands too (a template's
        // `font: 900 96px var(--head)` keeps its brand font where the HTML composition format would fall back).
        if native && is_stand_in(&family) && installed(&family) {
            continue;
        }
        let alias = ALIASES.iter().find(|(a, _)| *a == lower);
        if !longhand && alias.is_some_and(|(_, canonical)| !display_name(canonical).eq_ignore_ascii_case(&family)) {
            continue;
        }
        if let Some((_, canonical)) = alias {
            let mut covered = HashSet::new();
            let mut bundled = Vec::new();
            for (_, weight, style, bytes) in BUNDLED.iter().filter(|(f, ..)| f == canonical) {
                bundled.push(face_rule(&family, &data_url(bytes), weight, style, Some(BUNDLED_RANGE)));
                let w = weight.trim().parse::<f64>().map(|n| n.to_string()).unwrap_or_else(|_| weight.to_string());
                covered.insert(format!("{w}:{style}"));
            }
            // the canonical family's Google faces first, the bundle last: overlapping
            // unicode-ranges resolve last-defined-first, so Latin keeps the bundled source
            rules.extend(supplement_rules(&family, google_faces(display_name(canonical), net, text.as_deref(), None), &covered));
            rules.extend(bundled);
            continue;
        }
        let faces = google_faces(&family, net, text.as_deref(), authored.get(&lower));
        if faces.is_empty() {
            if !longhand || (native && installed(&family)) {
                continue;
            }
            // The HTML composition format embeds the installed font file instead (its system font capture)
            let captured = system_capture(&family);
            if captured.is_empty() {
                notes.push(format!("font \"{family}\" isn't bundled, on Google Fonts or installed; the fallback font is used"));
            }
            for (weight, style, bytes) in captured {
                rules.push(face_rule_format(&family, &sfnt_data_url(&bytes), &weight, &style, "truetype"));
            }
            continue;
        }
        for f in faces {
            rules.push(face_rule(&family, &f.url, &f.weight, &f.style, f.range.as_deref()));
        }
    }
    if rules.is_empty() {
        return notes;
    }
    if let Some(head) = find_tag(doc, "head") {
        let css = rules.join("\n\n");
        let mut m = doc.mutate();
        let style = m.create_element(
            QualName::new(None, ns!(html), LocalName::from("style")),
            vec![blitz_dom::Attribute { name: crate::compile::qual("data-raea-deterministic-fonts"), value: "true".into() }],
        );
        let text = m.create_text_node(&css);
        m.append_children(style, &[text]);
        drop(m);
        append_to(doc, head, style);
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_from_declarations() {
        let css = vec![
            ".a{font-family:\"Helvetica Neue\",Arial,sans-serif} .b{font:700 40px/1.2 'Inter', serif}".to_string(),
            ":root{--f: Georgia, serif} .c{font-family:var(--f)}".to_string(),
        ];
        let requested = requested_families(&css);
        let families: Vec<(&str, bool)> = requested.iter().map(|(f, l)| (f.as_str(), *l)).collect();
        assert_eq!(families, vec![("Helvetica Neue", true), ("Arial", true), ("Inter", false), ("Georgia", true)]);
        assert!(declared_families(&["@font-face{font-family:'Lob';src:url(x)}".to_string()]).contains("lob"));
    }

    #[test]
    fn system_primaries_get_inter() {
        let none = HashMap::new();
        let css = ".a{font-family:-apple-system,Helvetica,sans-serif}.b{font-family:Georgia,serif}\
                   @font-face{font-family:serif;src:url(x)}@media (min-width:1px){.c{font-family:serif !important}}";
        assert_eq!(
            rewrite_system_primaries(css, false, &none).unwrap(),
            ".a{font-family: Inter, -apple-system,Helvetica,sans-serif}.b{font-family:Georgia,serif}\
                   @font-face{font-family:serif;src:url(x)}@media (min-width:1px){.c{font-family: Inter, serif !important}}"
        );
        assert_eq!(rewrite_system_primaries("color:red; font-family: system-ui", true, &none).unwrap(), "color:red; font-family: Inter, system-ui");
        assert_eq!(rewrite_system_primaries(":root{--f: system-ui, sans-serif}", false, &none).unwrap(), ":root{--f: Inter, system-ui, sans-serif}");
        let defined: HashMap<String, String> = [("--a".to_string(), "Lobster".to_string())].into();
        assert_eq!(system_primary_replacement("var(--a, serif)", &defined), None);
        assert_eq!(system_primary_replacement("var(--b, serif)", &defined).as_deref(), Some("var(--b, Inter, serif)"));
        assert_eq!(system_primary_replacement("'Inter', sans-serif", &none), None);
    }
}
