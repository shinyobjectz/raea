//! A PDF's pages drawn into pictures by CoreGraphics, to be read as any picture is: a brief or a brand's guide given
//! with a videoform's references (docs/videoform.feature VF8f) is read by Vision's text recognition page by page, so a
//! page that is itself a picture reads as well as one set in type.

use objc2_core_foundation::{CFURL, CGRect};
use objc2_core_graphics::{CGBitmapContextCreate, CGColorSpace, CGContext, CGImageAlphaInfo, CGPDFBox, CGPDFDocument, CGPDFPage, kCGColorSpaceSRGB};
use std::path::Path;

/// Each page of the PDF at `path`, `width` pixels across: RGBA, the top row first, on white
pub fn pages(path: &Path, width: usize) -> Result<Vec<(Vec<u8>, usize, usize)>, String> {
    let url = CFURL::from_file_path(path).ok_or_else(|| format!("{}: not a path", path.display()))?;
    let doc = CGPDFDocument::with_url(Some(&url)).ok_or_else(|| format!("{}: not a PDF CoreGraphics reads", path.display()))?;
    if CGPDFDocument::is_encrypted(Some(&doc)) && !CGPDFDocument::is_unlocked(Some(&doc)) {
        return Err(format!("{}: locked with a password", path.display()));
    }
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).ok_or("no sRGB")?;
    let mut out = Vec::new();
    for n in 1..=CGPDFDocument::number_of_pages(Some(&doc)) {
        let Some(page) = CGPDFDocument::page(Some(&doc), n) else { continue };
        let b = CGPDFPage::box_rect(Some(&page), CGPDFBox::CropBox);
        if b.size.width <= 0.0 || b.size.height <= 0.0 {
            continue;
        }
        let s = width as f64 / b.size.width;
        let h = (b.size.height * s).round().max(1.0) as usize;
        let mut px = vec![255u8; width * h * 4];
        {
            let c = unsafe { CGBitmapContextCreate(px.as_mut_ptr().cast(), width, h, 8, width * 4, Some(&space), CGImageAlphaInfo::PremultipliedLast.0) }
                .ok_or("CGBitmapContextCreate failed")?;
            let c = Some(&*c);
            CGContext::set_rgb_fill_color(c, 1.0, 1.0, 1.0, 1.0);
            CGContext::fill_rect(c, CGRect::new(objc2_core_foundation::CGPoint::new(0.0, 0.0), objc2_core_foundation::CGSize::new(width as f64, h as f64)));
            CGContext::scale_ctm(c, s, s);
            CGContext::translate_ctm(c, -b.origin.x, -b.origin.y);
            CGContext::draw_pdf_page(c, Some(&page));
        }
        out.push((px, width, h));
    }
    Ok(out)
}
