// Portions ported from HyperFrames (Apache-2.0, https://github.com/heygen-com/hyperframes); see NOTICE.
//! The composition compile step, done in place on the Blitz document before any script runs:
//!
//! * sub-compositions (`data-composition-src`) are inlined into their host, recursively, with
//!   their relative asset URLs rebased onto the project root; their styles go to <head> and
//!   their scripts to the end of <body>, in discovery order (as the the HTML composition format compiler does);
//! * GSAP (CDN, inlined or vendored copies, plugins included) is dropped: the animato-backed
//!   shim stands in for it;
//! * `<video>` becomes an `<img>` with the same attributes whose pixels are the decoded frame
//!   for the current render time;
//! * the root composition's data-width/height size the root, <html> and <body>.

use blitz_dom::{BaseDocument, DocumentConfig, LocalName, NodeId, QualName, local_name, ns};
use blitz_html::DocumentHtmlParser;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn qual(name: &str) -> QualName {
    QualName::new(None, ns!(), LocalName::from(name))
}

pub fn attr(doc: &BaseDocument, id: NodeId, name: &str) -> Option<String> {
    doc.get_node(id)?.element_data()?.attr(LocalName::from(name)).map(str::to_string)
}

pub fn has_attr(doc: &BaseDocument, id: NodeId, name: &str) -> bool {
    doc.get_node(id).and_then(|n| n.element_data()).is_some_and(|e| e.has_attr(LocalName::from(name)))
}

pub fn tag(doc: &BaseDocument, id: NodeId) -> String {
    doc.get_node(id).and_then(|n| n.element_data()).map(|e| e.name.local.to_string()).unwrap_or_default()
}

/// Element ids under `root` in document order (root included when it is an element).
pub fn elements_under(doc: &BaseDocument, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let Some(node) = doc.get_node(id) else { continue };
        if node.is_element() {
            out.push(id);
        }
        if let Some(t) = node.element_data().and_then(|e| e.template_contents) {
            // template contents are inert; skip
            let _ = t;
        }
        stack.extend(node.children.iter().rev().copied());
    }
    out
}

pub fn all_elements(doc: &BaseDocument) -> Vec<NodeId> {
    elements_under(doc, doc.root_node().id)
}

pub fn find_tag(doc: &BaseDocument, tag_name: &str) -> Option<NodeId> {
    all_elements(doc).into_iter().find(|&id| tag(doc, id) == tag_name)
}

#[derive(Default, Debug)]
pub struct CompileInfo {
    /// authored composition id -> runtime (host) id, for `__timelines` keys
    pub aliases: HashMap<String, String>,
    pub warnings: Vec<String>,
    /// Video elements turned into frame images: (render id, element id)
    pub videos: Vec<String>,
}

fn is_relative_asset(url: &str) -> bool {
    let u = url.trim();
    !(u.is_empty()
        || u.starts_with('#')
        || u.starts_with('/')
        || u.starts_with("data:")
        || u.starts_with("blob:")
        || u.starts_with("about:")
        || u.starts_with("javascript:")
        || u.contains("://")
        || u.starts_with("//")
        || u.starts_with("mailto:"))
}

/// A sub-composition's relative URL as a project-root path, as the composition format's rewriteAssetPath decides it: a `../`
/// path resolves against the file's folder, a plain relative path only when that sibling exists in `root`
/// (otherwise it is already project-root relative: registry blocks installed under compositions/ reference
/// `assets/…` at the root).
fn rebase(root: &Path, src_file: &str, url: &str) -> String {
    if !is_relative_asset(url) {
        return url.to_string();
    }
    let sibling = sibling_path(src_file, url);
    let path = sibling.split(['?', '#']).next().unwrap_or("");
    if url.starts_with("../") || url == ".." || root.join(path).exists() { sibling } else { url.to_string() }
}

/// `dir/../x.png` style normalization of a URL relative to `src_file`'s folder.
fn sibling_path(src_file: &str, url: &str) -> String {
    let base = Path::new(src_file).parent().unwrap_or(Path::new(""));
    let mut parts: Vec<String> = base.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    let (path, suffix) = match url.find(['?', '#']) {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, ""),
    };
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s.to_string()),
        }
    }
    format!("{}{}", parts.join("/"), suffix)
}

pub fn rebase_css_urls(root: &Path, css: &str, src_file: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = rest.find("url(") {
        out.push_str(&rest[..i + 4]);
        rest = &rest[i + 4..];
        let Some(end) = rest.find(')') else { break };
        let inner = rest[..end].trim();
        let (q, raw) = if inner.starts_with('"') || inner.starts_with('\'') {
            (&inner[..1], inner.trim_matches(|c| c == '"' || c == '\''))
        } else {
            ("", inner)
        };
        out.push_str(q);
        out.push_str(&rebase(root, src_file, raw));
        out.push_str(q);
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn strip_css_comments(css: &str) -> String {
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

/// Split on top-level commas (not inside parentheses or brackets).
fn split_selectors(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur);
    out
}

/// `#id` tokens for the authored root's id in a selector as `[data-hf-authored-id="id"]`.
fn authored_id_selector(sel: &str, dom_id: &str) -> String {
    let needle = format!("#{dom_id}");
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_' || !c.is_ascii();
    let mut out = String::with_capacity(sel.len() + 32);
    let mut rest = sel;
    while let Some(i) = rest.find(&needle) {
        let after = &rest[i + needle.len()..];
        out.push_str(&rest[..i]);
        if after.chars().next().is_some_and(ident) {
            out.push_str(&needle);
        } else {
            out.push_str(&format!("[data-hf-authored-id=\"{dom_id}\"]"));
        }
        rest = after;
    }
    out.push_str(rest);
    out.trim().to_string()
}

fn scope_selector(sel: &str, scope: &str, ids: &[&str]) -> String {
    let t = sel.trim();
    if t.is_empty() || t == "*" {
        return sel.to_string();
    }
    if t.eq_ignore_ascii_case("html") || t.eq_ignore_ascii_case("body") || t == ":root" {
        return scope.to_string();
    }
    for id in ids {
        for q in ['"', '\''] {
            let pat = format!("[data-composition-id={q}{id}{q}]");
            if t == pat {
                // the composition's own box lives on the preserved inner-root wrapper
                return format!("{scope} > [data-hf-inner-root]");
            }
            if t.contains(&pat) {
                return t.replace(&pat, scope);
            }
        }
    }
    format!("{scope} {t}")
}

/// The composition format's scopeCssToComposition: every top-level rule of a sub-composition's styles is
/// prefixed with the composition scope; @keyframes / @font-face stay global, @media and
/// @supports blocks are scoped inside, nested rules inherit their parent's scope.
/// Stylo (servo build) drops `background-clip: text`, so every `[-webkit-]background-clip: text`
/// declaration gets a sibling `--uv-bgclip: text` that the painter reads (vendor/blitz-paint,
/// "background-clip:text"); the UA sheet resets the marker on every element so it doesn't inherit.
pub fn mark_background_clip_text(src: &str) -> String {
    const NEEDLE: &str = "background-clip";
    let lower = src.to_ascii_lowercase();
    let mut out = String::with_capacity(src.len());
    let mut last = 0;
    let mut from = 0;
    while let Some(i) = lower[from..].find(NEEDLE).map(|i| i + from) {
        from = i + NEEDLE.len();
        let rest = &lower[from..];
        let after_ws = rest.trim_start();
        let Some(value) = after_ws.strip_prefix(':') else { continue };
        let value = value.trim_start();
        if !value.starts_with("text") || value[4..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            continue;
        }
        let end = lower.len() - value.len() + 4;
        out.push_str(&src[last..end]);
        out.push_str(";--uv-bgclip:text");
        last = end;
        from = end;
    }
    out.push_str(&src[last..]);
    out
}

/// Query containers marked for the runtime: `container-type: size | inline-size` declarations in `<style>`
/// blocks and `style=""` attributes (scripts are left alone) get a `--uv-ct` marker, which the GSAP shim
/// reads to turn container units in tween values into px (cqToPx). The units themselves are stylo's,
/// resolved against the container's layout (blitz-dom's query_container_size).
pub fn mark_query_containers(src: &str) -> String {
    let lower = src.to_ascii_lowercase();
    if !lower.contains("container-type") {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len() + 256);
    let mut last = 0;
    let mut i = 0;
    while i < src.len() {
        if lower[i..].starts_with("<script") {
            // skip the script element whole
            let end = lower[i..].find("</script").map(|e| i + e).unwrap_or(src.len());
            out.push_str(&src[last..end]);
            last = end;
            i = end + 1;
            continue;
        }
        if lower[i..].starts_with("<style") {
            let Some(open) = lower[i..].find('>').map(|e| i + e + 1) else { break };
            let close = lower[open..].find("</style").map(|e| open + e).unwrap_or(src.len());
            out.push_str(&src[last..open]);
            out.push_str(&container_css(&src[open..close]));
            last = close;
            i = close + 1;
            continue;
        }
        if lower[i..].starts_with("style=") && i > 0 && lower.as_bytes()[i - 1].is_ascii_whitespace() {
            let q = src.as_bytes().get(i + 6).copied();
            if let Some(q @ (b'"' | b'\'')) = q {
                let vstart = i + 7;
                if let Some(vend) = src[vstart..].find(q as char).map(|e| vstart + e) {
                    out.push_str(&src[last..vstart]);
                    out.push_str(&container_css(&src[vstart..vend]));
                    last = vend;
                    i = vend + 1;
                    continue;
                }
            }
        }
        i += 1;
        while i < src.len() && !src.is_char_boundary(i) {
            i += 1;
        }
    }
    out.push_str(&src[last..]);
    out
}

/// One CSS text with its container-type declarations marked.
pub fn container_css(css: &str) -> String {
    let b = css.as_bytes();
    let lower = css.to_ascii_lowercase();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'-' || c == b'_';
    let mut out = String::with_capacity(css.len() + 64);
    let (mut last, mut i) = (0, 0);
    while i < b.len() {
        if lower.as_bytes()[i..].starts_with(b"container-type") && (i == 0 || !ident(b[i - 1])) {
            let after = i + "container-type".len();
            let rest = &lower[after..];
            let trimmed = rest.trim_start();
            if let Some(v) = trimmed.strip_prefix(':') {
                let vstart = lower.len() - v.len();
                let vend = lower[vstart..].find([';', '}', '"', '\'']).map(|e| vstart + e).unwrap_or(lower.len());
                let value = lower[vstart..vend].replace("!important", "");
                let value = value.trim();
                if value == "size" || value == "inline-size" {
                    out.push_str(&css[last..vend]);
                    out.push_str(";--uv-ct:");
                    out.push_str(value);
                    last = vend;
                    i = vend;
                    continue;
                }
            }
        }
        i += 1;
    }
    out.push_str(&css[last..]);
    out
}

pub fn scope_css(css: &str, scope: &str, ids: &[&str]) -> String {
    let css = strip_css_comments(css);
    let mut out = String::with_capacity(css.len() + 256);
    scope_block(&css, scope, ids, &mut out);
    out
}

fn scope_block(css: &str, scope: &str, ids: &[&str], out: &mut String) {
    let bytes = css.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // prelude up to '{' or ';'
        let start = i;
        let mut depth = 0i32;
        let mut quote: Option<u8> = None;
        while i < bytes.len() {
            let c = bytes[i];
            if let Some(q) = quote {
                if c == q {
                    quote = None;
                }
            } else if c == b'"' || c == b'\'' {
                quote = Some(c);
            } else if c == b'(' || c == b'[' {
                depth += 1;
            } else if c == b')' || c == b']' {
                depth -= 1;
            } else if depth == 0 && (c == b'{' || c == b';') {
                break;
            }
            i += 1;
        }
        let prelude = &css[start..i.min(css.len())];
        if i >= bytes.len() {
            out.push_str(prelude);
            break;
        }
        if bytes[i] == b';' {
            out.push_str(prelude);
            out.push(';');
            i += 1;
            continue;
        }
        // find matching '}'
        let body_start = i + 1;
        let mut d = 1;
        let mut j = body_start;
        let mut q: Option<u8> = None;
        while j < bytes.len() && d > 0 {
            let c = bytes[j];
            if let Some(qq) = q {
                if c == qq {
                    q = None;
                }
            } else if c == b'"' || c == b'\'' {
                q = Some(c);
            } else if c == b'{' {
                d += 1;
            } else if c == b'}' {
                d -= 1;
            }
            j += 1;
        }
        let body = &css[body_start..j.saturating_sub(1).max(body_start)];
        let p = prelude.trim();
        if let Some(at) = p.strip_prefix('@') {
            let name = at.split(|c: char| c.is_whitespace() || c == '(' || c == '{').next().unwrap_or("").to_ascii_lowercase();
            out.push_str(prelude);
            out.push('{');
            if matches!(name.as_str(), "media" | "supports" | "container" | "layer" | "document" | "scope") {
                scope_block(body, scope, ids, out);
            } else {
                out.push_str(body);
            }
            out.push('}');
        } else {
            let scoped: Vec<String> = split_selectors(p).iter().map(|s| scope_selector(s, scope, ids)).collect();
            out.push_str(&scoped.join(", "));
            out.push('{');
            out.push_str(body);
            out.push('}');
        }
        i = j;
    }
}

/// Run a sub-composition script with `document` and `gsap` scoped to its composition root,
/// like the composition format's wrapScopedCompositionScript.
pub fn wrap_scoped_script(code: &str, comp_id: &str) -> String {
    let id = serde_json::to_string(comp_id).unwrap_or_else(|_| "\"\"".into());
    format!(
        "(function(){{var __hfs=__hfb.scope({id});var __hv=window.__raea,__gv=__hv&&__hv.getVariables,__vb=window.__hfVariablesByComp&&window.__hfVariablesByComp[{id}];if(__hv&&__vb)__hv.getVariables=function(){{return Object.assign({{}},__vb);}};try{{(function(document,gsap){{\n{code}\n}}).call(window,__hfs.document,__hfs.gsap);}}finally{{if(__hv&&__vb)__hv.getVariables=__gv;}}}})();"
    )
}

const URL_ATTRS: &[&str] = &["src", "href", "poster", "xlink:href", "data-composition-src", "data-src"];

pub fn is_gsap_script(src: Option<&str>, text: &str) -> bool {
    let probe = |s: &str| {
        let s = s.to_ascii_lowercase();
        (s.contains("/gsap@") || s.contains("/gsap/") || s.contains("gsap.min.js") || s.contains("gsap.js")
            || s.contains("tweenmax") || s.contains("tweenlite") || s.contains("timelinemax"))
            || ((s.contains("gsap") || s.contains("greensock"))
                && ["splittext", "customease", "scrolltrigger", "motionpath", "drawsvg", "morphsvg", "textplugin", "easepack", "flip", "observer", "scrolltoplugin", "physics", "inertia", "scrambletext", "custombounce", "customwiggle"].iter().any(|p| s.contains(p)))
    };
    if let Some(src) = src {
        let file = src.rsplit('/').next().unwrap_or(src).to_ascii_lowercase();
        if probe(src) || (file.starts_with("gsap") && file.ends_with(".js")) {
            return true;
        }
    }
    let head: String = text.trim_start().chars().take(400).collect();
    if let Some(rest) = head.strip_prefix("/* inlined: ") {
        let url = rest.split_whitespace().next().unwrap_or("");
        return probe(url);
    }
    head.contains("GSAP 3") || head.contains("gsap.com/standard-license") || head.contains("GreenSock")
}

fn new_element(doc: &mut BaseDocument, name: &str, attrs: &[(&str, &str)], text: Option<&str>) -> NodeId {
    let mut m = doc.mutate();
    let attrs = attrs.iter().map(|(k, v)| blitz_dom::Attribute { name: qual(k), value: v.to_string() }).collect();
    let el = m.create_element(QualName::new(None, ns!(html), LocalName::from(name)), attrs);
    if let Some(t) = text {
        let tn = m.create_text_node(t);
        m.append_children(el, &[tn]);
    }
    el
}

pub fn append_to(doc: &mut BaseDocument, parent: NodeId, child: NodeId) {
    let mut m = doc.mutate();
    m.append_children(parent, &[child]);
}

struct SubComposition {
    fragment: String,
    styles: Vec<String>,
    links: Vec<String>,
    scripts: Vec<(Option<String>, String)>,
    authored_id: Option<String>,
    inner_width: Option<String>,
    inner_height: Option<String>,
    /// the file's `<html data-composition-variables>` declaration (JSON), whose defaults the host's
    /// data-variable-values override per instance
    variables: Option<String>,
    /// the authored root's DOM id (demoted to data-hf-authored-id on the inner-root wrapper)
    authored_dom_id: Option<String>,
}

fn load_sub(root: &Path, path: &Path, src: &str, host_id: Option<&str>) -> Result<SubComposition, String> {
    let host_has_id = host_id.is_some();
    let html = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let html = mark_query_containers(&mark_background_clip_text(&html));
    let mut sub = BaseDocument::new(DocumentConfig::default());
    {
        let mut m = sub.mutate();
        DocumentHtmlParser::parse_into_mutator(&mut m, &html);
    }
    // content root: <template> contents, else <body>
    let all = all_elements(&sub);
    let template = all.iter().copied().find(|&id| tag(&sub, id) == "template");
    let content = match template {
        Some(t) => sub.get_node(t).and_then(|n| n.element_data()).and_then(|e| e.template_contents).unwrap_or(t),
        None => find_tag(&sub, "body").unwrap_or(sub.root_node().id),
    };
    let scope: Vec<NodeId> = elements_under(&sub, content);
    // The composition format's planCompositionAssembly: a host with a composition id takes over the root carrying that
    // same id; when none does, the content goes in as authored (its root keeps its own id and identity)
    let roots: Vec<NodeId> =
        scope.iter().copied().filter(|&id| id != content && has_attr(&sub, id, "data-composition-id")).collect();
    let inner_root = match host_id {
        Some(h) => roots.iter().copied().find(|&r| attr(&sub, r, "data-composition-id").as_deref() == Some(h)),
        None => roots.first().copied(),
    };
    let declared_id = inner_root.or(roots.first().copied()).and_then(|r| attr(&sub, r, "data-composition-id"));

    // styles / links / scripts from the whole file (head + content), in order
    let mut styles = Vec::new();
    let mut links = Vec::new();
    let mut scripts = Vec::new();
    let mut extracted = Vec::new();
    let mut everywhere = all.clone();
    for id in &scope {
        if !everywhere.contains(id) {
            everywhere.push(*id);
        }
    }
    for id in everywhere {
        match tag(&sub, id).as_str() {
            "style" => {
                let css = sub.get_node(id).map(|n| n.text_content()).unwrap_or_default();
                styles.push(rebase_css_urls(root, &css, src));
                extracted.push(id);
            }
            "link" => {
                let rel = attr(&sub, id, "rel").unwrap_or_default().to_ascii_lowercase();
                if rel.contains("stylesheet") {
                    if let Some(href) = attr(&sub, id, "href") {
                        links.push(rebase(root, src, &href));
                    }
                }
                extracted.push(id);
            }
            "script" => {
                let ty = attr(&sub, id, "type").unwrap_or_default().to_ascii_lowercase();
                if matches!(ty.as_str(), "" | "text/javascript" | "application/javascript" | "module") {
                    let text = sub.get_node(id).map(|n| n.text_content()).unwrap_or_default();
                    let s = attr(&sub, id, "src").map(|s| rebase(root, src, &s));
                    scripts.push((s, text));
                }
                extracted.push(id);
            }
            "meta" | "title" => extracted.push(id),
            _ => {}
        }
    }
    {
        let mut m = sub.mutate();
        for id in extracted {
            m.remove_node(id);
        }
    }
    // rebase URL attributes and inline-style url() in the content
    let scope: Vec<NodeId> = elements_under(&sub, content);
    for id in &scope {
        let Some(el) = sub.get_node(*id).and_then(|n| n.element_data()) else { continue };
        let mut updates = Vec::new();
        for a in el.attrs() {
            let name = a.name.local.to_string();
            let full = match &a.name.prefix {
                Some(p) => format!("{p}:{name}"),
                None => name.clone(),
            };
            if URL_ATTRS.contains(&full.as_str()) || URL_ATTRS.contains(&name.as_str()) {
                let v = rebase(root, src, &a.value);
                if v != a.value {
                    updates.push((a.name.clone(), v));
                }
            } else if name == "style" && a.value.contains("url(") {
                updates.push((a.name.clone(), rebase_css_urls(root, &a.value, src)));
            } else if name == "srcset" {
                let v = a
                    .value
                    .split(',')
                    .map(|c| {
                        let c = c.trim();
                        match c.split_once(' ') {
                            Some((u, d)) => format!("{} {d}", rebase(root, src, u)),
                            None => rebase(root, src, c),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                updates.push((a.name.clone(), v));
            }
        }
        let mut m = sub.mutate();
        for (n, v) in updates {
            m.set_attribute(*id, n, &v);
        }
    }

    let authored_id = declared_id;
    let inner_width = inner_root.and_then(|r| attr(&sub, r, "data-width"));
    let inner_height = inner_root.and_then(|r| attr(&sub, r, "data-height"));
    let variables = find_tag(&sub, "html").and_then(|h| attr(&sub, h, "data-composition-variables"));
    let authored_dom_id = inner_root.and_then(|r| attr(&sub, r, "id"));
    // The composition format's render compiler keeps the authored root as a wrapper inside the host
    // (prepareFlattenedInnerRoot): timing/identity attributes stripped, id demoted to
    // data-hf-authored-id, sized from data-width/height, marked data-hf-inner-root.
    if let Some(r) = inner_root {
        let authored_dom_id = attr(&sub, r, "id");
        let style = attr(&sub, r, "style").unwrap_or_default();
        let w = inner_width.as_deref().map(|w| format!("{}px", w.trim().trim_end_matches("px"))).unwrap_or_else(|| "100%".into());
        let h = inner_height.as_deref().map(|h| format!("{}px", h.trim().trim_end_matches("px"))).unwrap_or_else(|| "100%".into());
        let fill = format!("width:{w};height:{h}");
        let mut m = sub.mutate();
        for a in ["data-composition-id", "data-composition-file", "data-start", "data-duration", "data-end", "data-track-index", "data-track", "data-composition-src", "data-hf-authored-duration", "data-hf-authored-end"] {
            m.clear_attribute(r, qual(a));
        }
        if let Some(id) = authored_dom_id {
            m.clear_attribute(r, qual("id"));
            m.set_attribute(r, qual("data-hf-authored-id"), &id);
        }
        m.set_attribute(r, qual("data-hf-inner-root"), "true");
        let style = if style.trim().is_empty() { fill } else { format!("{};{fill}", style.trim().trim_end_matches(';')) };
        m.set_attribute(r, qual("style"), &style);
        if !host_has_id {
            if let Some(id) = &authored_id {
                m.set_attribute(r, qual("data-composition-id"), id);
            }
        }
    }
    let fragment = match inner_root {
        Some(r) => outer_html(&sub, r),
        None => children_html(&sub, content),
    };
    Ok(SubComposition { fragment, styles, links, scripts, authored_id, inner_width, inner_height, variables, authored_dom_id })
}

fn children_html(doc: &BaseDocument, id: NodeId) -> String {
    let Some(node) = doc.get_node(id) else { return String::new() };
    let mut s = String::new();
    for c in node.children.iter() {
        write_html(doc, *c, &mut s);
    }
    s
}

pub fn outer_html(doc: &BaseDocument, id: NodeId) -> String {
    let mut s = String::new();
    write_html(doc, id, &mut s);
    s
}

const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"];
const RAW_TEXT: &[&str] = &["script", "style", "xmp", "iframe", "noembed", "noframes", "plaintext"];

fn escape_into(s: &str, attr: bool, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' if !attr => out.push_str("&lt;"),
            '>' if !attr => out.push_str("&gt;"),
            '"' if attr => out.push_str("&quot;"),
            '\u{a0}' => out.push_str("&nbsp;"),
            c => out.push(c),
        }
    }
}

/// HTML serialization that round-trips through the HTML parser: explicit end tags for every
/// non-void element (Blitz's own serializer writes `<div />`, which HTML parses as an open tag).
fn write_html(doc: &BaseDocument, id: NodeId, out: &mut String) {
    let Some(node) = doc.get_node(id) else { return };
    if let Some(el) = node.element_data() {
        let name = el.name.local.to_string();
        out.push('<');
        out.push_str(&name);
        for a in el.attrs() {
            out.push(' ');
            if let Some(p) = &a.name.prefix {
                out.push_str(p);
                out.push(':');
            }
            out.push_str(&a.name.local);
            out.push_str("=\"");
            escape_into(&a.value, true, out);
            out.push('"');
        }
        out.push('>');
        if VOID.contains(&name.as_str()) && el.name.ns == blitz_dom::ns!(html) {
            return;
        }
        let raw = RAW_TEXT.contains(&name.as_str());
        let children: Vec<NodeId> = match el.template_contents {
            Some(t) => doc.get_node(t).map(|n| n.children.to_vec()).unwrap_or_default(),
            None => node.children.to_vec(),
        };
        for c in children {
            if raw {
                if let Some(n) = doc.get_node(c) {
                    if let Some(t) = n.text_data() {
                        out.push_str(&t.content);
                        continue;
                    }
                }
            }
            write_html(doc, c, out);
        }
        out.push_str("</");
        out.push_str(&name);
        out.push('>');
    } else if let Some(t) = node.text_data() {
        escape_into(&t.content, false, out);
    } else {
        for c in node.children.iter() {
            write_html(doc, *c, out);
        }
    }
}

/// Inline every `data-composition-src` host (nested ones too).
pub fn inline_sub_compositions(doc: &mut BaseDocument, project_dir: &Path, info: &mut CompileInfo) {
    let head = find_tag(doc, "head");
    let body = find_tag(doc, "body");
    // (src, code, origin "file#n" and line offset for error messages)
    let mut pending_scripts: Vec<(Option<String>, String, Option<(String, usize)>)> = Vec::new();
    let mut seen_srcs: Vec<String> = Vec::new();
    for _round in 0..16 {
        let hosts: Vec<NodeId> =
            all_elements(doc).into_iter().filter(|&id| has_attr(doc, id, "data-composition-src")).collect();
        if hosts.is_empty() {
            break;
        }
        for host in hosts {
            let src = attr(doc, host, "data-composition-src").unwrap_or_default();
            let host_id = attr(doc, host, "data-composition-id");
            let path: PathBuf = project_dir.join(src.split(['?', '#']).next().unwrap_or(&src));
            {
                let mut m = doc.mutate();
                m.clear_attribute(host, qual("data-composition-src"));
                m.set_attribute(host, qual("data-composition-file"), &src);
            }
            if seen_srcs.iter().filter(|s| **s == src).count() > 0 {
                info.warnings.push(format!(
                    "{src} is mounted more than once; its scripts run unscoped, so selectors may reach every instance"
                ));
            }
            seen_srcs.push(src.clone());
            let sub = match load_sub(project_dir, &path, &src, host_id.as_deref()) {
                Ok(s) => s,
                Err(e) => {
                    info.warnings.push(format!("sub-composition {src} not loaded: {e}"));
                    continue;
                }
            };
            let host_has_w = has_attr(doc, host, "data-width");
            let host_has_h = has_attr(doc, host, "data-height");
            {
                let mut m = doc.mutate();
                m.set_inner_html(host, &sub.fragment);
                if let (Some(w), false) = (&sub.inner_width, host_has_w) {
                    m.set_attribute(host, qual("data-width"), w);
                }
                if let (Some(h), false) = (&sub.inner_height, host_has_h) {
                    m.set_attribute(host, qual("data-height"), h);
                }
            }
            if let (Some(a), Some(h)) = (&sub.authored_id, &host_id) {
                if a != h {
                    info.aliases.insert(a.clone(), h.clone());
                }
            }
            let scope_id = host_id.clone().or_else(|| sub.authored_id.clone());
            if let Some(head) = head {
                for href in &sub.links {
                    let link = new_element(doc, "link", &[("rel", "stylesheet"), ("href", href)], None);
                    append_to(doc, head, link);
                }
                for css in &sub.styles {
                    let css = match &scope_id {
                        Some(id) => {
                            let scope = format!("[data-composition-id=\"{id}\"]");
                            let mut ids: Vec<&str> = vec![id.as_str()];
                            if let Some(a) = &sub.authored_id {
                                ids.push(a.as_str());
                            }
                            // the authored root's #id names the inner-root wrapper (its id was demoted)
                            let css = match &sub.authored_dom_id {
                                Some(dom_id) => {
                                    let mut out = String::with_capacity(css.len());
                                    map_selectors(&strip_css_comments(css), &|sel: &str| authored_id_selector(sel, dom_id), &mut out);
                                    out
                                }
                                None => css.clone(),
                            };
                            scope_css(&css, &scope, &ids)
                        }
                        None => css.clone(),
                    };
                    let style = new_element(doc, "style", &[], Some(&css));
                    append_to(doc, head, style);
                }
            }
            let file = src.split(['?', '#']).next().unwrap_or(&src).to_string();
            // this instance's variables for the sub's scoped getVariables(): declared defaults, then the
            // host's data-variable-values
            let instance = attr(doc, host, "data-variable-values");
            if let (Some(id), true) = (&scope_id, sub.variables.is_some() || instance.is_some()) {
                let mut merged = serde_json::Map::new();
                if let Some(serde_json::Value::Array(decl)) = sub.variables.as_deref().and_then(|v| serde_json::from_str(v).ok()) {
                    for e in decl {
                        if let (Some(k), Some(d)) = (e.get("id").and_then(|k| k.as_str()), e.get("default")) {
                            merged.insert(k.to_string(), d.clone());
                        }
                    }
                }
                if let Some(serde_json::Value::Object(vals)) = instance.as_deref().and_then(|v| serde_json::from_str(v).ok()) {
                    merged.extend(vals);
                }
                let key = serde_json::to_string(id).unwrap_or_default();
                pending_scripts.push((
                    None,
                    format!(
                        "window.__hfVariablesByComp=window.__hfVariablesByComp||{{}};window.__hfVariablesByComp[{key}]={};",
                        serde_json::Value::Object(merged)
                    ),
                    None,
                ));
            }
            let mut inline_index = 0;
            for (script_src, text) in sub.scripts {
                let origin = script_src.is_none().then(|| {
                    inline_index += 1;
                    format!("{file}#{inline_index}")
                });
                match (&script_src, &scope_id) {
                    (None, Some(id)) => pending_scripts.push((None, wrap_scoped_script(&text, id), origin.map(|o| (o, 1)))),
                    _ => pending_scripts.push((script_src, text, origin.map(|o| (o, 0)))),
                }
            }
        }
    }
    if let Some(body) = body {
        for (src, text, origin) in pending_scripts {
            let el = match (&src, &origin) {
                (Some(s), _) => new_element(doc, "script", &[("src", s)], None),
                (None, Some((o, offset))) => new_element(doc, "script", &[("data-uv-origin", o), ("data-uv-line-offset", &offset.to_string())], Some(&text)),
                (None, None) => new_element(doc, "script", &[], Some(&text)),
            };
            append_to(doc, body, el);
        }
    }
}

/// Drop GSAP scripts (the shim replaces them); returns how many were removed.
pub fn remove_gsap_scripts(doc: &mut BaseDocument) -> usize {
    let scripts: Vec<NodeId> = all_elements(doc).into_iter().filter(|&id| tag(doc, id) == "script").collect();
    let mut removed = 0;
    for id in scripts {
        let src = attr(doc, id, "src");
        let text = doc.get_node(id).map(|n| n.text_content()).unwrap_or_default();
        if is_gsap_script(src.as_deref(), &text) {
            doc.mutate().remove_node(id);
            removed += 1;
        }
    }
    removed
}

/// `<video>` -> `<img data-uv-video="id">` carrying the same attributes.
pub fn replace_videos(doc: &mut BaseDocument, info: &mut CompileInfo) {
    let videos: Vec<NodeId> = all_elements(doc).into_iter().filter(|&id| tag(doc, id) == "video").collect();
    let mut auto = 0;
    for v in videos {
        let Some(el) = doc.get_node(v).and_then(|n| n.element_data()) else { continue };
        let mut attrs: Vec<(String, String)> = Vec::new();
        for a in el.attrs() {
            let n = a.name.local.to_string();
            if matches!(n.as_str(), "src" | "autoplay" | "controls" | "preload" | "playsinline" | "muted" | "loop" | "crossorigin") {
                continue;
            }
            attrs.push((n, a.value.clone()));
        }
        let src = el.attr(local_name!("src")).map(str::to_string).or_else(|| {
            let node = doc.get_node(v)?;
            node.children.iter().find(|c| tag(doc, **c) == "source").and_then(|c| attr(doc, *c, "src"))
        });
        let Some(src) = src else { continue };
        let looping = el.has_attr(local_name!("loop"));
        let id = el
            .attr(LocalName::from("data-hf-render-id"))
            .or(el.attr(local_name!("id")))
            .map(str::to_string)
            .unwrap_or_else(|| {
                let id = format!("hf-video-{auto}");
                auto += 1;
                id
            });
        if !attrs.iter().any(|(k, _)| k == "id") {
            attrs.push(("id".into(), id.clone()));
        }
        attrs.push(("data-uv-video".into(), id.clone()));
        attrs.push(("data-uv-src".into(), src));
        if looping {
            attrs.push(("data-uv-loop".into(), String::new()));
        }
        let refs: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let img = new_element(doc, "img", &refs, None);
        let mut m = doc.mutate();
        m.insert_nodes_before(v, &[img]);
        m.remove_node(v);
        drop(m);
        info.videos.push(id);
    }
    if !info.videos.is_empty() {
        retarget_video_css(doc);
    }
}

/// Stylesheets that select `video` (`video { object-fit: cover }`) keep applying to the
/// `<img data-uv-video>` elements that replace the videos: each `<style>` that mentions `video` is
/// replaced by one whose `video` type selectors read `img[data-uv-video]`.
fn retarget_video_css(doc: &mut BaseDocument) {
    let styles: Vec<NodeId> = all_elements(doc).into_iter().filter(|&id| tag(doc, id) == "style").collect();
    for id in styles {
        let css = doc.get_node(id).map(|n| n.text_content()).unwrap_or_default();
        if !css.to_ascii_lowercase().contains("video") {
            continue;
        }
        let mut out = String::with_capacity(css.len() + 64);
        map_selectors(&strip_css_comments(&css), &video_selector, &mut out);
        if out == css {
            continue;
        }
        let attrs: Vec<(String, String)> = doc
            .get_node(id)
            .and_then(|n| n.element_data())
            .map(|e| e.attrs().iter().map(|a| (a.name.local.to_string(), a.value.to_string())).collect())
            .unwrap_or_default();
        let refs: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let style = new_element(doc, "style", &refs, Some(&out));
        let mut m = doc.mutate();
        m.insert_nodes_before(id, &[style]);
        m.remove_node(id);
    }
}

/// Rewrite every rule's selector list with `f` (recursing into conditional group rules), keeping
/// declaration blocks and other at-rules as they are.
fn map_selectors(css: &str, f: &dyn Fn(&str) -> String, out: &mut String) {
    let bytes = css.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let (mut depth, mut quote) = (0i32, None::<u8>);
        while i < bytes.len() {
            let c = bytes[i];
            if let Some(q) = quote {
                if c == q {
                    quote = None;
                }
            } else if c == b'"' || c == b'\'' {
                quote = Some(c);
            } else if c == b'(' || c == b'[' {
                depth += 1;
            } else if c == b')' || c == b']' {
                depth -= 1;
            } else if depth == 0 && (c == b'{' || c == b';') {
                break;
            }
            i += 1;
        }
        let prelude = &css[start..i.min(css.len())];
        if i >= bytes.len() {
            out.push_str(prelude);
            break;
        }
        if bytes[i] == b';' {
            out.push_str(prelude);
            out.push(';');
            i += 1;
            continue;
        }
        let body_start = i + 1;
        let (mut d, mut j, mut q) = (1, body_start, None::<u8>);
        while j < bytes.len() && d > 0 {
            let c = bytes[j];
            if let Some(qq) = q {
                if c == qq {
                    q = None;
                }
            } else if c == b'"' || c == b'\'' {
                q = Some(c);
            } else if c == b'{' {
                d += 1;
            } else if c == b'}' {
                d -= 1;
            }
            j += 1;
        }
        let body = &css[body_start..j.saturating_sub(1).max(body_start)];
        let p = prelude.trim();
        if let Some(at) = p.strip_prefix('@') {
            let name = at.split(|c: char| c.is_whitespace() || c == '(' || c == '{').next().unwrap_or("").to_ascii_lowercase();
            out.push_str(prelude);
            out.push('{');
            if matches!(name.as_str(), "media" | "supports" | "container" | "layer" | "document" | "scope") {
                map_selectors(body, f, out);
            } else {
                out.push_str(body);
            }
            out.push('}');
        } else {
            out.push_str(&split_selectors(p).iter().map(|s| f(s.trim())).collect::<Vec<_>>().join(", "));
            out.push('{');
            out.push_str(body);
            out.push('}');
        }
        i = j;
    }
}

/// `video` type selectors (not `.video-card`, `#video`, `[data-video]` or `video-player`) as
/// `img[data-uv-video]`.
fn video_selector(sel: &str) -> String {
    let mut out = String::with_capacity(sel.len() + 16);
    let (mut bracket, mut quote) = (0i32, None::<char>);
    let mut prev: Option<char> = None;
    let mut it = sel.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if let Some(q) = quote {
            out.push(c);
            if c == q {
                quote = None;
            }
            prev = Some(c);
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '[' => bracket += 1,
            ']' => bracket -= 1,
            _ => {}
        }
        let rest = &sel[i..];
        let starts = prev.is_none_or(|p| p.is_whitespace() || matches!(p, '>' | '+' | '~' | '(' | ','));
        if bracket == 0 && starts && rest.len() >= 5 && rest[..5].eq_ignore_ascii_case("video") {
            let next = rest[5..].chars().next();
            if next.is_none_or(|n| n.is_whitespace() || matches!(n, '.' | '#' | ':' | '[' | '>' | '+' | '~' | ')' | ',')) {
                out.push_str("img[data-uv-video]");
                for _ in 0..4 {
                    it.next();
                }
                prev = Some('o');
                continue;
            }
        }
        out.push(c);
        prev = Some(c);
    }
    out
}

#[cfg(test)]
mod video_css_tests {
    #[test]
    fn video_type_selectors_match_the_replacement() {
        let mut out = String::new();
        super::map_selectors(
            "video{object-fit:cover} .video-card video, #video{x:1} @media (min-width:1px){ .a > video:first-child{y:2} } video-player{z:3} [data-k=\"video\"]{w:4}",
            &super::video_selector,
            &mut out,
        );
        assert!(out.contains("img[data-uv-video]{object-fit:cover}"), "{out}");
        assert!(out.contains(".video-card img[data-uv-video], #video{x:1}"), "{out}");
        assert!(out.contains(".a > img[data-uv-video]:first-child{y:2}"), "{out}");
        assert!(out.contains("video-player{z:3}"), "{out}");
        assert!(out.contains("[data-k=\"video\"]{w:4}"), "{out}");
    }
}

/// The runtime's applyCompositionSizing: data-width/height on the root drive root/html/body.
pub fn apply_root_sizing(doc: &mut BaseDocument, root: NodeId) {
    let w = attr(doc, root, "data-width").and_then(|v| v.trim().parse::<f32>().ok());
    let h = attr(doc, root, "data-height").and_then(|v| v.trim().parse::<f32>().ok());
    let html = find_tag(doc, "html");
    let body = find_tag(doc, "body");
    let mut m = doc.mutate();
    if let Some(w) = w {
        let v = format!("{w}px");
        m.set_style_property(root, "width", &v);
        m.set_style_property(root, "--comp-width", &v);
        for id in [html, body].into_iter().flatten() {
            m.set_style_property(id, "width", &v);
        }
    }
    if let Some(h) = h {
        let v = format!("{h}px");
        m.set_style_property(root, "height", &v);
        m.set_style_property(root, "--comp-height", &v);
        for id in [html, body].into_iter().flatten() {
            m.set_style_property(id, "height", &v);
        }
    }
}

/// `[data-composition-id][data-root="true"]`, else the first composition in document order.
pub fn root_composition(doc: &BaseDocument) -> Option<NodeId> {
    let comps: Vec<NodeId> =
        all_elements(doc).into_iter().filter(|&id| has_attr(doc, id, "data-composition-id")).collect();
    comps
        .iter()
        .copied()
        .find(|&id| attr(doc, id, "data-root").as_deref() == Some("true"))
        .or_else(|| comps.first().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_containers_are_marked() {
        assert_eq!(container_css(".c{container-type: size}"), ".c{container-type: size;--uv-ct:size}");
        assert_eq!(container_css(".c{container-type:inline-size;width:50cqw}"), ".c{container-type:inline-size;--uv-ct:inline-size;width:50cqw}");
        assert_eq!(container_css(".c{container-type: normal}"), ".c{container-type: normal}");
        let html = r#"<style>.a{container-type:size}</style><div style="container-type: inline-size"></div><script>x="container-type:size"</script>"#;
        assert_eq!(
            mark_query_containers(html),
            r#"<style>.a{container-type:size;--uv-ct:size}</style><div style="container-type: inline-size;--uv-ct:inline-size"></div><script>x="container-type:size"</script>"#
        );
    }

    #[test]
    fn container_css_walks_multibyte_text() {
        let css = "/* ── layout ── */ .c{container-type:size} .é{color:red}";
        assert_eq!(container_css(css), "/* ── layout ── */ .c{container-type:size;--uv-ct:size} .é{color:red}");
    }

    #[test]
    fn rebases_relative_urls() {
        let dir = std::env::temp_dir().join(format!("uv-rebase-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("compositions/img")).unwrap();
        std::fs::write(dir.join("compositions/img/y.png"), b"").unwrap();
        assert_eq!(rebase(&dir, "compositions/a.html", "../assets/x.svg"), "assets/x.svg");
        // a sibling that exists is the file's own; anything else is already project-root relative
        assert_eq!(rebase(&dir, "compositions/a.html", "img/y.png?v=2"), "compositions/img/y.png?v=2");
        assert_eq!(rebase(&dir, "compositions/a.html", "assets/bg.jpeg"), "assets/bg.jpeg");
        assert_eq!(rebase(&dir, "compositions/a.html", "https://x.com/a.png"), "https://x.com/a.png");
        assert_eq!(
            rebase_css_urls(&dir, "background:url('../a.png') no-repeat", "c/s.html"),
            "background:url('a.png') no-repeat"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn marks_background_clip_text() {
        assert_eq!(
            mark_background_clip_text("a{-webkit-background-clip:text;Background-Clip : TEXT !important}"),
            "a{-webkit-background-clip:text;--uv-bgclip:text;Background-Clip : TEXT;--uv-bgclip:text !important}"
        );
        assert_eq!(mark_background_clip_text("b{background-clip:text-x;background-clip:padding-box}"), "b{background-clip:text-x;background-clip:padding-box}");
    }

    #[test]
    fn scopes_css() {
        let css = ".a, .b > c { color: red } @media (max-width: 9px) { body { x: 1 } } @keyframes k { from { opacity: 0 } } [data-composition-id=\"s1\"] .x { y: 2 }";
        let out = scope_css(css, "[data-composition-id=\"s\"]", &["s", "s1"]);
        assert!(out.contains("[data-composition-id=\"s\"] .a, [data-composition-id=\"s\"] .b > c"), "{out}");
        assert!(out.contains("@media (max-width: 9px) {[data-composition-id=\"s\"]{"), "{out}");
        assert!(out.contains("@keyframes k { from { opacity: 0 } }"), "{out}");
        assert!(out.contains("[data-composition-id=\"s\"] .x"), "{out}");
        let root = scope_css("[data-composition-id=\"s1\"] { width: 10px }", "[data-composition-id=\"s\"]", &["s", "s1"]);
        assert!(root.contains("[data-composition-id=\"s\"] > [data-hf-inner-root]"), "{root}");
    }

    #[test]
    fn detects_gsap() {
        assert!(is_gsap_script(Some("https://cdn.jsdelivr.net/npm/gsap@3.14.2/dist/gsap.min.js"), ""));
        assert!(is_gsap_script(Some("assets/gsap-3.14.2.min.js"), ""));
        assert!(is_gsap_script(None, "/* inlined: https://cdn.jsdelivr.net/npm/gsap@3.14.2/dist/CustomEase.min.js */\n..."));
        assert!(!is_gsap_script(Some("https://cdn.jsdelivr.net/npm/d3@7/dist/d3.min.js"), ""));
    }
}
