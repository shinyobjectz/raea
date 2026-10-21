//! A Blitz document driven by plain operations (docs/bevy.feature B3): create an element or a text node, append,
//! insert before, remove, set an attribute or a style property, set text, then resolve (Stylo cascade, Taffy
//! layout, Parley text) and paint (Vello). Bevy systems apply React's entities through this; Blitz stays the engine's
//! internals behind it. Node ids are Blitz's own (`NodeId`, re-exported here).

use std::path::Path;
use std::sync::Arc;

use blitz_dom::{BaseDocument, DocumentConfig};
pub use blitz_dom::NodeId;
use blitz_traits::shell::{ColorScheme, Viewport};

use crate::net::SyncNet;
use crate::row_render::{paint_document, qn};

/// One laid-out element (Dom::laid_out).
#[derive(Debug, Clone)]
pub struct Laid {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub parent: Option<usize>,
    /// border box, page px
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// top, right, bottom, left
    pub border: [f32; 4],
    pub padding: [f32; 4],
    /// direct children in order: Ok(element index) or Err(text)
    pub kids: Vec<Result<usize, String>>,
    pub css: std::collections::BTreeMap<String, String>,
}

pub struct Dom {
    doc: BaseDocument,
    body: NodeId,
    pub width: u32,
    pub height: u32,
    net: Arc<SyncNet>,
    head: bool,
    /// the page's stylesheets as the last font injection left them (fonts::css_key), and what it said
    fonts_key: Option<u64>,
    fonts_notes: Vec<String>,
    /// the composition time the page is styled at (s): CSS animations and transitions run on it, an element's from
    /// when it was first styled, as in a browser. The host sets it each frame (set_time); it was 0 for every frame, so
    /// a composition's @keyframes held their first keyframe and a transition never moved
    time: f64,
    /// boxes moved without a restyle (`move_box`), moved again after each layout pass: each one's left/top px, the
    /// style its attribute is written with when it can't be moved, and what blitz keeps for it between passes
    moved: std::collections::HashMap<NodeId, Moved>,
}

/// A box moved by its left/top alone (`Dom::move_box`)
struct Moved {
    left: Option<f32>,
    top: Option<f32>,
    style: String,
    state: blitz_dom::MoveState,
}

impl Dom {
    /// The composition time the page is styled at from now on (s): the frame's, before its style and layout
    pub fn set_time(&mut self, t: f64) {
        self.time = t;
    }

    pub fn time(&self) -> f64 {
        self.time
    }

    /// Whether the page has something running on its own clock (a CSS animation or transition): it changes from frame
    /// to frame with nothing else touching it, so it's styled and painted again each frame
    pub fn is_animating(&self) -> bool {
        self.doc.is_animating()
    }

    /// A composition's timed clips (each <Sequence>'s box and its start, s): its CSS animations run from its nearest
    /// clip's start (blitz-dom set_animation_clips), so a frame is the same rendered alone or in order
    pub fn set_animation_clips(&mut self, clips: std::collections::HashMap<NodeId, f64>) {
        self.doc.set_animation_clips(clips);
    }

    /// An empty page (html > body) of `width` x `height` CSS px; relative URLs and fonts resolve against `dir`.
    pub fn new(width: u32, height: u32, dir: &Path) -> Dom {
        let net = Arc::new(SyncNet::new(dir, false));
        let config = DocumentConfig {
            viewport: Some(Viewport::new(width, height, 1.0, ColorScheme::Light)),
            ua_stylesheets: Some(vec![blitz_dom::DEFAULT_CSS.to_string()]),
            net_provider: Some(net.clone() as _),
            base_url: url::Url::from_directory_path(dir).ok().map(|u| u.to_string()),
            ..Default::default()
        };
        let mut doc = BaseDocument::new(config);
        let body = {
            let mut m = doc.mutate();
            let html = m.create_element(qn("html"), vec![]);
            let root = m.doc.root_node().id;
            m.append_children(root, &[html]);
            let body = m.create_element(qn("body"), vec![]);
            m.append_children(html, &[body]);
            body
        };
        Dom { doc, body, width, height, net, head: false, fonts_key: None, fonts_notes: Vec::new(), time: 0.0, moved: Default::default() }
    }

    /// The bundled and Google faces for the families the page's styles name, as an HTML page gets them
    /// (fonts::inject, into a head made for it): call once the content is in, before painting. Without it a React
    /// part asking for Inter painted in Helvetica (raea-react lower). Native: an installed font is the font itself,
    /// not the composition format's stand-in (fonts::inject_with).
    pub fn load_fonts(&mut self) -> Vec<String> {
        if !self.head {
            let mut m = self.doc.mutate();
            let head = m.create_element(qn("head"), vec![]);
            m.insert_nodes_before(self.body, &[head]);
            self.head = true;
        }
        // the same stylesheets as the last injection left them: nothing new to inject (a seek re-creates the page's
        // <style>s word for word; the faces injected stay in <head>, outside what a seek replaces)
        let key = crate::fonts::css_key(&self.doc);
        if self.fonts_key == Some(key) {
            return self.fonts_notes.clone();
        }
        let notes = crate::fonts::inject_with(&mut self.doc, &self.net, true);
        // the injection's own <style> isn't part of the key, so the page as it was before is what the next look matches
        self.fonts_key = Some(key);
        self.fonts_notes = notes.clone();
        notes
    }

    /// The page's injected font faces with every remote face inlined (fonts::packaged_css): an exported game carries
    /// them, so its player fetches nothing.
    pub fn packaged_fonts_css(&mut self) -> Option<String> {
        self.load_fonts();
        crate::fonts::packaged_css(&self.doc, &self.net)
    }

    /// A packaged font stylesheet put in the page's <head> (fonts::add_packaged_css), before its fonts load.
    pub fn add_packaged_fonts_css(&mut self, css: &str) {
        if !self.head {
            let mut m = self.doc.mutate();
            let head = m.create_element(qn("head"), vec![]);
            m.insert_nodes_before(self.body, &[head]);
            self.head = true;
        }
        crate::fonts::add_packaged_css(&mut self.doc, css);
    }

    /// Every element under the body after style and layout, parents first: its tag and attributes, its border box
    /// in page px, its border and padding widths, its direct children in order (text as Err(text), elements as
    /// Ok(index into the list)), and the computed value of each named CSS property, serialized as CSS (stylo's own
    /// serializer). What raea-react lower turns into rows (bevy.feature B5f, level 1b).
    pub fn laid_out(&mut self, props: &[&str]) -> Vec<Laid> {
        if self.style(self.time) {
            self.layout(self.time);
        }
        let ids: Vec<(style::properties::LonghandId, String)> = props
            .iter()
            .filter_map(|p| style::properties::PropertyId::parse_enabled_for_all_content(p).ok().and_then(|id| id.longhand_id()).map(|l| (l, p.to_string())))
            .collect();
        let mut out: Vec<Laid> = Vec::new();
        fn walk(doc: &BaseDocument, id: NodeId, parent: Option<usize>, ids: &[(style::properties::LonghandId, String)], out: &mut Vec<Laid>) -> Option<usize> {
            let node = doc.get_node(id)?;
            let el = node.element_data()?;
            let lay = node.final_layout();
            let pos = node.absolute_position(0.0, 0.0);
            let mut css = std::collections::BTreeMap::new();
            if let Some(st) = node.primary_styles() {
                for (l, name) in ids {
                    css.insert(name.clone(), st.computed_value_to_string(style::properties::PropertyDeclarationId::Longhand(*l)));
                }
            }
            let i = out.len();
            out.push(Laid {
                tag: el.name.local.to_string(),
                attrs: el.attrs().iter().map(|a| (a.name.local.to_string(), a.value.to_string())).collect(),
                parent,
                x: pos.x,
                y: pos.y,
                w: lay.size.width,
                h: lay.size.height,
                border: [lay.border.top, lay.border.right, lay.border.bottom, lay.border.left],
                padding: [lay.padding.top, lay.padding.right, lay.padding.bottom, lay.padding.left],
                kids: Vec::new(),
                css,
            });
            let children: Vec<NodeId> = node.children.iter().copied().collect();
            for c in children {
                let Some(cn) = doc.get_node(c) else { continue };
                if let blitz_dom::NodeData::Text(t) = &cn.data {
                    out[i].kids.push(Err(t.content.to_string()));
                } else if let Some(k) = walk(doc, c, Some(i), ids, out) {
                    out[i].kids.push(Ok(k));
                }
            }
            Some(i)
        }
        let body: Vec<NodeId> = self.doc.get_node(self.body).map(|b| b.children.iter().copied().collect()).unwrap_or_default();
        for c in body {
            walk(&self.doc, c, None, &ids, &mut out);
        }
        out
    }

    /// The page at a new size (a composition's settings arrive after its first commit): Blitz restyles and lays out
    /// against the new viewport on the next frame.
    pub fn resize(&mut self, width: u32, height: u32) {
        if (width, height) == (self.width, self.height) {
            return;
        }
        self.doc.set_viewport(Viewport::new(width, height, 1.0, ColorScheme::Light));
        self.width = width;
        self.height = height;
    }

    /// The body: where top-level content goes.
    pub fn body(&self) -> NodeId {
        self.body
    }

    pub fn create_element(&mut self, tag: &str) -> NodeId {
        self.doc.mutate().create_element(qn(tag), vec![])
    }

    pub fn create_text(&mut self, text: &str) -> NodeId {
        self.doc.mutate().create_text_node(text)
    }

    pub fn append(&mut self, parent: NodeId, child: NodeId) {
        self.doc.mutate().append_children(parent, &[child]);
    }

    pub fn insert_before(&mut self, before: NodeId, child: NodeId) {
        self.doc.mutate().insert_nodes_before(before, &[child]);
    }

    pub fn remove(&mut self, node: NodeId) {
        self.doc.mutate().remove_node(node);
    }

    /// Remove `node` for good: it and its subtree are freed (remove only detaches, and the arena kept every node
    /// a live page ever removed). The ids freed, so a mirror can forget them.
    pub fn drop(&mut self, node: NodeId) -> Vec<NodeId> {
        let mut gone = Vec::new();
        self.doc.mutate().remove_and_drop_node_with(node, &mut |id| gone.push(id));
        gone
    }

    /// Compositor-only motion: `node`'s style changed only in `left`/`top` (CSS text, `12.5px`), so it's moved where it's
    /// laid out instead of restyled and laid out again (blitz's `move_out_of_flow`, after each layout pass); `style` is
    /// its whole style, written to its attribute if it can't be moved that way. False (nothing recorded: write the
    /// style) for text that isn't a px length. RAEA_MOTION_FAST=0: never.
    pub fn move_box(&mut self, node: NodeId, left: Option<&str>, top: Option<&str>, style: &str) -> bool {
        static OFF: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| std::env::var("RAEA_MOTION_FAST").as_deref() == Ok("0"));
        if *OFF {
            return false;
        }
        let px = |t: Option<&str>| t.map(blitz_dom::css_px);
        let (l, t) = (px(left), px(top));
        if l == Some(None) || t == Some(None) || (l.is_none() && t.is_none()) {
            return false;
        }
        if !self.doc.get_node(node).is_some_and(|n| n.is_out_of_flow()) {
            return false;
        }
        let m = self.moved.entry(node).or_insert_with(|| Moved { left: None, top: None, style: String::new(), state: Default::default() });
        // an axis this update leaves alone keeps what an earlier one set
        if let Some(l) = l {
            m.left = l;
        }
        if let Some(t) = t {
            m.top = t;
        }
        m.style.clear();
        m.style.push_str(style);
        true
    }

    /// The moved boxes moved again after a layout pass; one that can't be is written as styled, and the page resolved
    /// again. RAEA_MOTION_CHECK=1: each pass then also lays the page out from every moved box's style, and every
    /// node's layout must match (the fast path's equivalence check).
    fn place_moved(&mut self, t: f64) {
        if self.moved.is_empty() {
            return;
        }
        let mut failed = false;
        let doc = &mut self.doc;
        self.moved.retain(|&n, m| {
            if doc.get_node(n).is_none() {
                return false;
            }
            if doc.move_out_of_flow(n, m.left, m.top, &mut m.state) {
                return true;
            }
            doc.mutate().set_attribute(n, crate::row_render::qn("style"), &m.style);
            failed = true;
            false
        });
        if failed {
            self.doc.resolve(t);
            let doc = &mut self.doc;
            self.moved.retain(|&n, m| doc.get_node(n).is_some() && doc.move_out_of_flow(n, m.left, m.top, &mut m.state));
        }
        // RAEA_MOTION_CHECK=N: every Nth pass (boxes stay moved, on what blitz kept for them, in between)
        static EVERY: std::sync::LazyLock<u64> = std::sync::LazyLock::new(|| std::env::var("RAEA_MOTION_CHECK").ok().map(|v| v.parse().unwrap_or(1).max(1)).unwrap_or(0));
        static PASSES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let checking = *EVERY > 0 && PASSES.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % *EVERY == 0;
        if checking && !self.moved.is_empty() {
            let ids = crate::compile::all_elements(&self.doc);
            // Taffy's overflow rect aside (a moved box's containing block isn't a scroll container: only scrolling
            // reads it); Blitz's paint overflow compared instead
            let key = |x: &blitz_dom::node::Node| {
                let mut l = *x.final_layout();
                l.scrollable_overflow_rect = Default::default();
                (l, *x.scrollable_overflow())
            };
            let fast: Vec<_> = ids.iter().filter_map(|&n| self.doc.get_node(n).map(|x| (n, key(x)))).collect();
            for (n, m) in std::mem::take(&mut self.moved) {
                self.doc.mutate().set_attribute(n, crate::row_render::qn("style"), &m.style);
            }
            self.doc.resolve(t);
            for (n, l) in fast {
                if let Some(x) = self.doc.get_node(n) {
                    assert!(key(x) == l, "RAEA_MOTION_CHECK: node {n} laid out {:?} from its style, {:?} moved", key(x), l);
                }
            }
        }
    }

    pub fn set_attribute(&mut self, node: NodeId, name: &str, value: &str) {
        // its style written whole: no longer a moved box
        if name == "style" {
            self.moved.remove(&node);
        }
        self.doc.mutate().set_attribute(node, qn(name), value);
    }

    pub fn clear_attribute(&mut self, node: NodeId, name: &str) {
        self.doc.mutate().clear_attribute(node, qn(name));
    }

    pub fn set_text(&mut self, node: NodeId, text: &str) {
        self.doc.mutate().set_node_text(node, text);
    }

    /// Stylo over what changed since the last frame (the style system). False while there's nothing to lay out.
    pub fn style(&mut self, t: f64) -> bool {
        self.doc.resolve_style(t)
    }

    /// Box construction, Taffy layout and Parley text over the damaged subtrees, then the paint tree (the layout system).
    pub fn layout(&mut self, t: f64) {
        self.doc.resolve_boxes(t);
        self.place_moved(t);
    }

    /// Resolve (style, layout, text) and paint into `out` as RGBA8 (premultiplied), on a white page like a browser's.
    pub fn paint(&mut self, out: &mut Vec<u8>) {
        if self.style(self.time) {
            self.layout(self.time);
        }
        self.raster(out);
    }

    /// The page resolved and painted with what isn't painted left transparent (premultiplied RGBA8): a picture to
    /// put on a surface in 3D.
    pub fn paint_transparent(&mut self, out: &mut Vec<u8>) {
        if self.style(self.time) {
            self.layout(self.time);
        }
        paint_document(&mut self.doc, self.width, self.height, 1.0, false, out);
    }

    /// `paint_transparent`, started: the page is recorded now and painted on the GPU while the caller goes on;
    /// `Painting::finish` hands over the pixels.
    pub fn paint_transparent_submit(&mut self) -> crate::row_render::Painting {
        if self.style(self.time) {
            self.layout(self.time);
        }
        crate::row_render::submit_document(&mut self.doc, self.width, self.height, 1.0, false)
    }

    /// The page resolved and presented on a live stage (a window), on the GPU, scaled to fit: no pixels come back.
    pub fn present(&mut self, stage: &crate::quads::Stage, clear: bool) -> Result<crate::quads::FrameStats, String> {
        if self.style(self.time) {
            self.layout(self.time);
        }
        match crate::quads::present(&mut self.doc, stage, self.width, self.height, 1.0, clear) {
            // a frame the compositor can't do (a backdrop filter, a blend over the page): painted on the CPU, shown the same way
            Err(e) if e == crate::quads::NEEDS_CPU => {
                let mut px = Vec::new();
                paint_document(&mut self.doc, self.width, self.height, 1.0, clear, &mut px);
                crate::quads::present_rgba(stage, self.width, self.height, &px).map(|_| Default::default())
            }
            r => r,
        }
    }

    /// The page painted into texture `id` (quads::paint_texture) for another page to draw, over texture `under`.
    pub fn paint_texture(&mut self, clear: bool, id: u64, under: Option<u64>) -> Result<crate::quads::FrameStats, String> {
        if self.style(self.time) {
            self.layout(self.time);
        }
        crate::quads::paint_texture(&mut self.doc, self.width, self.height, 1.0, clear, id, under)
    }

    /// `paint_texture`, shrunk by area to `fit` when that's smaller than the page (quads::paint_texture_fit): a fixed-size
    /// page shown in a smaller box (void's stage), its layout untouched.
    pub fn paint_texture_fit(&mut self, clear: bool, id: u64, under: Option<u64>, fit: Option<(u32, u32)>) -> Result<crate::quads::FrameStats, String> {
        if self.style(self.time) {
            self.layout(self.time);
        }
        crate::quads::paint_texture_fit(&mut self.doc, self.width, self.height, 1.0, clear, id, under, fit)
    }

    /// The page painted with what isn't painted left transparent into `dst` (quads::page_buffer), on the GPU with
    /// nothing read back; a frame the compositor can't do is painted on the CPU and written there.
    pub fn paint_transparent_buffer(&mut self, dst: &wgpu::Buffer) -> Result<(), String> {
        self.paint_buffer(false, None, dst)
    }

    /// The page painted into `dst` over texture `under` (a 3D scene, `quads::set_texture`), as `present_under` shows
    /// it in play; over black when `clear`.
    pub fn paint_buffer(&mut self, clear: bool, under: Option<u64>, dst: &wgpu::Buffer) -> Result<(), String> {
        if self.style(self.time) {
            self.layout(self.time);
        }
        match crate::quads::buffer_paint(&mut self.doc, self.width, self.height, 1.0, clear, under, dst) {
            Ok(_) => Ok(()),
            // a texture under the page has no CPU paint: the frame can't be made here
            Err(e) if under.is_some() => Err(e),
            Err(_) => {
                let mut px = Vec::new();
                crate::row_render::paint_cpu(&mut self.doc, self.width, self.height, 1.0, clear, &mut px);
                crate::quads::write_buffer(&px, dst)
            }
        }
    }

    /// `present` over texture `under` (a live 3D scene, `quads::set_texture`), which fills the page.
    pub fn present_under(&mut self, stage: &crate::quads::Stage, clear: bool, under: u64) -> Result<crate::quads::FrameStats, String> {
        if self.style(self.time) {
            self.layout(self.time);
        }
        crate::quads::present_under(&mut self.doc, stage, self.width, self.height, 1.0, clear, under)
    }

    /// Vello over the resolved page (the paint system): RGBA8 over white. Call after `style` and `layout`.
    pub fn raster(&mut self, out: &mut Vec<u8>) {
        paint_document(&mut self.doc, self.width, self.height, 1.0, false, out);
        // premultiplied over white
        for px in out.chunks_exact_mut(4) {
            let k = 255 - px[3] as u16;
            for c in &mut px[..3] {
                *c = (*c as u16 + k).min(255) as u8;
            }
            px[3] = 255;
        }
    }
}
