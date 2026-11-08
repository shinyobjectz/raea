//! A stylesheet's `url()`s that name the composition's files, written as what the page reads them by: a path on the Mac
//! and Linux, a file URL on Windows (raea_io_home::src). Written raw, a Windows path breaks twice in CSS: its
//! backslashes are escapes (`\A` a newline, `\c` and the hex digits after it a character) and `C:` a URL scheme (#28).

use std::path::Path;

/// `css` with each `url(..)` naming a file (not a URL with a scheme, a data: URI or a `#fragment`) as that file, quoted:
/// `/media/...` (staticFile's) found from the edit's folder `media` (media_path), anything else from `rel`, the folder
/// the stylesheet's own relative URLs start from. `missing` hears of each file that isn't there.
pub fn css_file_urls(css: &str, media: &Path, rel: &Path, missing: impl FnMut(&Path)) -> String {
    urls_as(css, media, rel, missing, raea_io_home::src)
}

/// css_file_urls with each file written by `src` (a test writes Windows' form on any system)
fn urls_as(css: &str, media: &Path, rel: &Path, mut missing: impl FnMut(&Path), src: impl Fn(&Path) -> String) -> String {
    if !css.contains("url(") {
        return css.to_string();
    }
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(at) = rest.find("url(") {
        out.push_str(&rest[..at + 4]);
        rest = &rest[at + 4..];
        let Some(end) = rest.find(')') else { break };
        let inner = rest[..end].trim();
        let quote = inner.chars().next().filter(|c| *c == '"' || *c == '\'');
        let url = quote.map_or(inner, |q| inner.trim_matches(q));
        if url.is_empty() || url.contains("://") || url.starts_with("data:") || url.starts_with('#') {
            out.push_str(&rest[..end]);
        } else {
            let file = raea_core_scene::media_path(if url.starts_with("/media/") { media } else { rel }, url);
            if !file.exists() {
                missing(&file);
            }
            out.push_str(&format!("\"{}\"", src(&file)));
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// css_file_urls as each path's look says it's written: Windows' form for a C:\ path, so it's checked on any system
    fn css_file_urls(css: &str, media: &Path, rel: &Path, missing: impl FnMut(&Path)) -> String {
        let src = |p: &Path| raea_io_home::src_on(&p.display().to_string(), p.display().to_string().as_bytes().get(1) == Some(&b':'));
        super::urls_as(css, media, rel, missing, src)
    }

    #[test]
    fn a_windows_path_is_a_file_url_with_no_backslash() {
        let dir = Path::new(r"C:\Users\a\AppData\Local\Temp\claude\edit");
        let css = r#"@font-face { font-family: X; src: url("/media/fonts/x.woff2") format("woff2"); } .a { background: url(bg.png) }"#;
        let mut gone = Vec::new();
        let out = css_file_urls(css, dir, dir, |p| gone.push(p.to_path_buf()));
        assert!(!out.contains('\\'), "{out}");
        assert!(out.contains(r#"url("file:///C:/Users/a/AppData/Local/Temp/claude/edit/"#), "{out}");
        assert!(out.contains(r#"fonts/x.woff2") format("woff2")"#) && out.contains(r#"bg.png") }"#), "{out}");
        assert_eq!(gone.len(), 2);
    }

    #[test]
    fn urls_naming_no_file_stay() {
        let dir = std::env::temp_dir();
        let css = r#"a { b: url(https://x.io/f.woff2), url('data:font/woff2;base64,AA'), url(#clip), url() }"#;
        assert_eq!(css_file_urls(css, &dir, &dir, |_| panic!("no file named")), css);
        assert_eq!(css_file_urls("a { color: red }", &dir, &dir, |_| ()), "a { color: red }");
    }

    #[test]
    fn a_relative_url_starts_from_the_stylesheets_folder_and_media_from_the_edits() {
        let edit = std::env::temp_dir().join(format!("raea-urls-{}", std::process::id()));
        let brand = edit.join("brand");
        std::fs::create_dir_all(edit.join("media")).unwrap();
        std::fs::create_dir_all(&brand).unwrap();
        std::fs::write(edit.join("media/m.woff2"), b"").unwrap();
        std::fs::write(brand.join("b.woff2"), b"").unwrap();
        let out = css_file_urls("url(/media/m.woff2) url(b.woff2)", &edit, &brand, |p| panic!("{p:?} is there"));
        let _ = std::fs::remove_dir_all(&edit);
        assert_eq!(out, format!("url(\"{}\") url(\"{}\")", edit.join("media/m.woff2").display(), brand.join("b.woff2").display()));
    }
}
