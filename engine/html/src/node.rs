//! An html node: one HTML page rendered as a layer by an embedding engine (moonsplice's `html` node
//! kind; docs/node.feature). The C API is `raea_node_*` (ffi.rs, include/raea.h).
//!
//! The contract an embedder relies on:
//! - Everything the page needs (CSS, fonts, images, scripts) resolves in `open`; the network is then
//!   sealed, and a later fetch is an error listed in `info`, never a late load.
//! - `render(t)` is a pure function of `t`: frames are the same bytes in any order of `t`
//!   (bench/seek-determinism.py proves this on the parity corpus).
//! - Frames are premultiplied RGBA8, `width*scale` x `height*scale`, transparent where the page
//!   paints nothing.
//! - A node is used by one thread; nodes on different threads are independent.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::media::VideoFrames;
use crate::scene::{Backend, Composition, Options, Profile};

pub struct NodeOptions {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub profile: Profile,
    /// Run the page's classic scripts once at open (otherwise they are removed before parsing)
    pub bake: bool,
    /// Faces registered before layout: (family, file path)
    pub fonts: Vec<(String, PathBuf)>,
    /// http(s) loads at open (off by default: a node should name local files)
    pub network: bool,
}

impl NodeOptions {
    /// `{"width","height","scale","profile":"web"|"html","bake","fonts":[{"family","path"}],"network"}`
    pub fn from_json(text: Option<&str>) -> Result<NodeOptions, String> {
        let mut o = NodeOptions { width: 1920, height: 1080, scale: 1.0, profile: Profile::Web, bake: true, fonts: vec![], network: false };
        let Some(text) = text.filter(|t| !t.trim().is_empty()) else { return Ok(o) };
        let v: Value = serde_json::from_str(text).map_err(|e| format!("opts_json: {e}"))?;
        let obj = v.as_object().ok_or("opts_json must be an object")?;
        for (k, v) in obj {
            match k.as_str() {
                "width" => o.width = v.as_u64().filter(|w| *w > 0).ok_or("opts_json.width must be a positive integer")? as u32,
                "height" => o.height = v.as_u64().filter(|h| *h > 0).ok_or("opts_json.height must be a positive integer")? as u32,
                "scale" => o.scale = v.as_f64().filter(|s| *s > 0.0).ok_or("opts_json.scale must be a positive number")? as f32,
                "profile" => o.profile = Profile::parse(v.as_str().ok_or("opts_json.profile must be a string")?)?,
                "bake" => o.bake = v.as_bool().ok_or("opts_json.bake must be a boolean")?,
                "network" => o.network = v.as_bool().ok_or("opts_json.network must be a boolean")?,
                "fonts" => {
                    for f in v.as_array().ok_or("opts_json.fonts must be an array")? {
                        let family = f["family"].as_str().ok_or("opts_json.fonts[].family must be a string")?;
                        let path = f["path"].as_str().ok_or("opts_json.fonts[].path must be a string")?;
                        o.fonts.push((family.to_string(), PathBuf::from(path)));
                    }
                }
                other => return Err(format!("opts_json: unknown key {other:?} (width, height, scale, profile, bake, fonts, network)")),
            }
        }
        Ok(o)
    }
}

pub struct HtmlNode {
    /// the composition in Bevy's schedule (stage.rs): every frame is one run of it
    stage: crate::stage::Stage,
    width: u32,
    height: u32,
    scale: f32,
    info: Value,
}

/// Remove `<script>` elements (and their contents) from markup: an unbaked node is static.
fn strip_scripts(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while let Some(start) = lower[i..].find("<script").map(|s| s + i) {
        out.push_str(&html[i..start]);
        match lower[start..].find("</script>") {
            Some(end) => i = start + end + "</script>".len(),
            None => return out,
        }
    }
    out.push_str(&html[i..]);
    out
}

fn css_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The page with the given faces declared first, so they apply before layout.
fn with_fonts(html: &str, fonts: &[(String, PathBuf)]) -> String {
    if fonts.is_empty() {
        return html.to_string();
    }
    let rules: String = fonts
        .iter()
        .map(|(family, path)| {
            let url = url::Url::from_file_path(path).map(|u| u.to_string()).unwrap_or_else(|_| path.display().to_string());
            format!("@font-face{{font-family:{};src:url({});}}", css_string(family), css_string(&url))
        })
        .collect();
    let style = format!("<style data-raea-node-fonts>{rules}</style>");
    match html.to_ascii_lowercase().find("<head>") {
        Some(i) => format!("{}{}{}", &html[..i + 6], style, &html[i + 6..]),
        None => format!("{style}{html}"),
    }
}

/// FNV-1a over the inputs that decide the pixels (markup, options, every resolved resource).
fn inputs_hash(parts: &[&[u8]]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for part in parts {
        for b in part.iter().chain(&[0xff]) {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}")
}

impl HtmlNode {
    /// Open a page. `base_url` (a `file://` URL of the page or its folder, or empty) anchors relative
    /// URLs; every resource loads here, and the network is sealed afterwards.
    pub fn open(html: &str, base_url: &str, o: NodeOptions) -> Result<HtmlNode, String> {
        let base = if base_url.trim().is_empty() {
            std::env::temp_dir().join("raea-node").join("page.html")
        } else {
            let url = url::Url::parse(base_url).map_err(|e| format!("base_url: {e}"))?;
            if url.scheme() != "file" {
                return Err(format!("base_url must be a file:// URL or empty (got {})", url.scheme()));
            }
            let path = url.to_file_path().map_err(|_| format!("base_url: not a local path: {base_url}"))?;
            if base_url.ends_with('/') || path.is_dir() { path.join("page.html") } else { path }
        };
        let dir = base.parent().map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let source = if o.bake { html.to_string() } else { strip_scripts(html) };
        let source = with_fonts(&source, &o.fonts);
        let comp = Composition::load(Options {
            html_path: base.clone(),
            project_dir: dir,
            fps: (30, 1),
            scale: o.scale,
            size: Some((o.width, o.height)),
            variables: None,
            allow_network: o.network,
            transparent: true,
            videos: Arc::new(VideoFrames::default()),
            backend: Backend::Vello,
            profile: o.profile,
            html_source: Some(source.clone()),
            clock: Default::default(),
        })?;
        comp.net.sealed.store(true, std::sync::atomic::Ordering::Relaxed);

        let fetched: Vec<(String, Result<usize, String>)> = comp.net.fetched.lock().unwrap().clone();
        let mut resources: Vec<Vec<u8>> = Vec::new();
        for (url, r) in &fetched {
            if let (Ok(_), Ok(u)) = (r, url::Url::parse(url)) {
                // re-read cached/local bytes for the hash (the load already succeeded, so this is offline)
                if let Ok(bytes) = std::fs::read(u.to_file_path().unwrap_or_default()) {
                    resources.push(bytes);
                } else {
                    resources.push(url.as_bytes().to_vec());
                }
            }
        }
        let opts_key = format!("{}x{}@{} {:?} bake={} fonts={:?}", o.width, o.height, o.scale, o.profile, o.bake, o.fonts);
        let mut parts: Vec<&[u8]> = vec![source.as_bytes(), opts_key.as_bytes()];
        parts.extend(resources.iter().map(|r| r.as_slice()));
        let info = json!({
            "width": o.width,
            "height": o.height,
            "scale": o.scale,
            "duration": comp.duration,
            "profile": match o.profile { Profile::Web => "web", Profile::Html => "html" },
            "fonts": o.fonts.iter().map(|(f, p)| json!({"family": f, "path": p.display().to_string()})).collect::<Vec<_>>(),
            "fetched": fetched.iter().filter(|(_, r)| r.is_ok()).map(|(u, r)| json!({"url": u, "bytes": r.as_ref().ok()})).collect::<Vec<_>>(),
            "failed": fetched.iter().filter_map(|(u, r)| r.as_ref().err().map(|e| json!({"url": u, "error": e}))).collect::<Vec<_>>(),
            "warnings": comp.warnings,
            "jsErrors": comp.js_errors,
            "inputsHash": inputs_hash(&parts),
        });
        Ok(HtmlNode { stage: crate::stage::Stage::new(comp), width: o.width, height: o.height, scale: o.scale, info })
    }

    /// Output size in device pixels (`width*scale` x `height*scale`).
    pub fn pixel_size(&self) -> (u32, u32) {
        ((self.width as f32 * self.scale).round() as u32, (self.height as f32 * self.scale).round() as u32)
    }

    /// The frame at `t` seconds, premultiplied RGBA8, tightly packed.
    pub fn render(&mut self, t: f64, out: &mut Vec<u8>) {
        self.stage.frame(t, out);
    }

    /// The frame at `t` written into `dst` with `stride` bytes per row.
    pub fn render_into(&mut self, t: f64, dst: &mut [u8], stride: usize) -> Result<(), String> {
        let (w, h) = self.pixel_size();
        let row = w as usize * 4;
        if stride < row || dst.len() < stride * (h as usize - 1) + row {
            return Err(format!("buffer too small for {w}x{h} at stride {stride}"));
        }
        let mut buf = Vec::new();
        self.render(t, &mut buf);
        for y in 0..h as usize {
            dst[y * stride..y * stride + row].copy_from_slice(&buf[y * row..(y + 1) * row]);
        }
        Ok(())
    }

    /// Text boxes at `t` (the same JSON `snapshot --boxes` writes).
    pub fn boxes(&mut self, t: f64) -> Value {
        let mut buf = Vec::new();
        self.stage.text_boxes(t, &mut buf, None)
    }

    /// Open-time facts, plus any fetch attempted after open (an error) and new script errors.
    pub fn info(&mut self) -> Value {
        let mut info = self.info.clone();
        let late: Vec<Value> = self
            .stage
            .comp()
            .net
            .fetched
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(u, r)| r.as_ref().err().filter(|e| e.starts_with("fetched after")).map(|_| json!(u)))
            .collect();
        info["lateFetches"] = json!(late);
        info["jsErrors"] = json!(self.stage.comp().js_errors);
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<!doctype html><html><head><style>
        body{margin:0} #box{position:absolute;left:20px;top:20px;width:60px;height:40px;background:#3ee0c6;
        animation:slide 2s linear both} @keyframes slide{to{left:220px;opacity:0.3}}
        p{position:absolute;left:20px;top:100px;margin:0;font:20px sans-serif;color:#222}
        </style></head><body><div id="box"></div><p>node text</p></body></html>"#;

    fn open(json: &str) -> HtmlNode {
        HtmlNode::open(PAGE, "", NodeOptions::from_json(Some(json)).unwrap()).unwrap()
    }

    #[test]
    fn frames_are_the_same_in_any_order() {
        let ts = [0.0, 0.5, 1.0, 1.5, 1.9];
        let mut a = open(r#"{"width":320,"height":160}"#);
        let mut b = open(r#"{"width":320,"height":160}"#);
        let forward: Vec<Vec<u8>> = ts.iter().map(|t| { let mut v = vec![]; a.render(*t, &mut v); v }).collect();
        let mut backward: Vec<Vec<u8>> = ts.iter().rev().map(|t| { let mut v = vec![]; b.render(*t, &mut v); v }).collect();
        backward.reverse();
        assert_eq!(forward, backward);
        assert_ne!(forward[0], forward[2], "the animation moves");
    }

    #[test]
    fn transparent_and_premultiplied() {
        let mut n = open(r#"{"width":320,"height":160}"#);
        let mut v = vec![];
        n.render(0.0, &mut v);
        assert_eq!(&v[..4], &[0, 0, 0, 0], "a pixel the page doesn't paint is transparent");
        assert!(v.chunks(4).all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]), "premultiplied: no channel above alpha");
    }

    #[test]
    fn scale_strides_boxes_and_info() {
        let mut n = open(r#"{"width":320,"height":160,"scale":2}"#);
        assert_eq!(n.pixel_size(), (640, 320));
        let stride = 640 * 4 + 64;
        let mut dst = vec![0u8; stride * 320];
        n.render_into(0.0, &mut dst, stride).unwrap();
        assert!(n.render_into(0.0, &mut dst[..100], stride).is_err());
        let boxes = n.boxes(0.5).to_string();
        assert!(boxes.contains("node text"), "{boxes}");
        let info = n.info();
        assert_eq!(info["profile"], "web");
        assert!(info["inputsHash"].as_str().unwrap().len() == 16);
    }

    #[test]
    fn unbaked_nodes_drop_scripts_and_the_network_is_sealed() {
        let page = r#"<html><body><p id="p">static</p><script>document.getElementById("p").textContent="scripted"</script></body></html>"#;
        let mut baked = HtmlNode::open(page, "", NodeOptions::from_json(Some(r#"{"width":200,"height":60}"#)).unwrap()).unwrap();
        let mut unbaked = HtmlNode::open(page, "", NodeOptions::from_json(Some(r#"{"width":200,"height":60,"bake":false}"#)).unwrap()).unwrap();
        assert!(baked.boxes(0.0).to_string().contains("scripted"));
        assert!(unbaked.boxes(0.0).to_string().contains("static"));
        assert!(baked.stage.comp().net.sealed.load(std::sync::atomic::Ordering::Relaxed));
        assert!(NodeOptions::from_json(Some(r#"{"nope":1}"#)).is_err());
    }
}
