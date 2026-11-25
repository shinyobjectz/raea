//! A document read once into passages (VF8f): a PDF's pages and a still by raea-perceive read-doc (CoreGraphics draws
//! each page, Vision reads it, so a page that is a picture reads as one set in type), a Word document's paragraphs from
//! its XML, plain text and Markdown as they are.

use raea_sys_videoform_briefs::passages::{Line, Passage, from_pages, from_paragraphs, from_text};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// What raea videoform reads as a document, by its extension
pub const DOCS: &[&str] = &["pdf", "docx", "txt", "md", "markdown", "text"];
/// Stills read as a document when they're named (a brief sent as a screenshot); a folder's stills are left out
pub const STILLS: &[&str] = &["png", "jpg", "jpeg", "webp"];

/// Whether raea videoform reads a file as a document, by its extension: a still too when `stills` (a file named, not
/// a folder's)
pub fn is_doc(p: &Path, stills: bool) -> bool {
    let Some(e) = p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase) else { return false };
    DOCS.contains(&e.as_str()) || (stills && STILLS.contains(&e.as_str()))
}

/// The document's passages
pub fn passages(p: &Path) -> Result<Vec<Passage>, String> {
    let e = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let out = match e.as_str() {
        "docx" => from_paragraphs(&docx(p)?),
        "txt" | "md" | "markdown" | "text" => from_text(&std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?),
        _ => from_pages(&pages(p)?),
    };
    if out.is_empty() {
        return Err(format!("{}: no words read in it", p.display()));
    }
    Ok(out)
}

/// A PDF's or a still's lines, page by page, as raea-perceive read-doc reads them
fn pages(p: &Path) -> Result<Vec<Vec<Line>>, String> {
    let out = Command::new(raea_io_assets::perceive_bin()).arg("read-doc").arg(p).output().map_err(|e| format!("raea-perceive read-doc: {e}"))?;
    if !out.status.success() {
        return Err(format!("raea-perceive read-doc: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("raea-perceive read-doc: {e}"))?;
    let pages = v["pages"].as_array().cloned().unwrap_or_default();
    Ok(pages.iter().map(|pg| serde_json::from_value(pg["lines"].clone()).unwrap_or_default()).collect())
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// A Word document's paragraphs, each with whether it's styled a heading or title
fn docx(p: &Path) -> Result<Vec<(String, bool)>, String> {
    let f = std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("{}: not a Word document ({e})", p.display()))?;
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut z.by_name("word/document.xml").map_err(|e| format!("{}: {e}", p.display()))?, &mut xml).map_err(|e| e.to_string())?;
    let para = regex::Regex::new(r"(?s)<w:p[ >].*?</w:p>").unwrap();
    let run = regex::Regex::new(r"(?s)<w:t(?: [^>]*)?>(.*?)</w:t>|<w:tab/>").unwrap();
    let style = regex::Regex::new(r#"<w:pStyle w:val="(?:Heading|Title|Subtitle)"#).unwrap();
    Ok(para.find_iter(&xml).map(|m| {
        let text: String = run.captures_iter(m.as_str()).map(|c| c.get(1).map_or(" ".to_string(), |t| unescape(t.as_str()))).collect();
        (text.split_whitespace().collect::<Vec<_>>().join(" "), style.is_match(m.as_str()))
    }).filter(|(t, _)| !t.is_empty()).collect())
}
