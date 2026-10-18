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
}

impl Dom {
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
        Dom { doc, body, width, height, net, head: false }
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
        crate::fonts::inject_with(&mut self.doc, &self.net, true)
    }

    /// Every element under the body after style and layout, parents first: its tag and attributes, its border box
    /// in page px, its border and padding widths, its direct children in order (text as Err(text), elements as
    /// Ok(index into the list)), and the computed value of each named CSS property, serialized as CSS (stylo's own
    /// serializer). What raea-react lower turns into rows (bevy.feature B5f, level 1b).
    pub fn laid_out(&mut self, props: &[&str]) -> Vec<Laid> {
        if self.style(0.0) {
            self.layout(0.0);
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

    pub fn set_attribute(&mut self, node: NodeId, name: &str, value: &str) {
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
    }

    /// Resolve (style, layout, text) and paint into `out` as RGBA8 (premultiplied), on a white page like a browser's.
    pub fn paint(&mut self, out: &mut Vec<u8>) {
        if self.style(0.0) {
            self.layout(0.0);
        }
        self.raster(out);
    }

    /// The page resolved and painted with what isn't painted left transparent (premultiplied RGBA8): a picture to
    /// put on a surface in 3D.
    pub fn paint_transparent(&mut self, out: &mut Vec<u8>) {
        if self.style(0.0) {
            self.layout(0.0);
        }
        paint_document(&mut self.doc, self.width, self.height, 1.0, false, out);
    }

    /// `paint_transparent`, started: the page is recorded now and painted on the GPU while the caller goes on;
    /// `Painting::finish` hands over the pixels.
    pub fn paint_transparent_submit(&mut self) -> crate::row_render::Painting {
        if self.style(0.0) {
            self.layout(0.0);
        }
        crate::row_render::submit_document(&mut self.doc, self.width, self.height, 1.0, false)
    }

    /// The page resolved and presented on a live stage (a window), on the GPU, scaled to fit: no pixels come back.
    pub fn present(&mut self, stage: &crate::quads::Stage, clear: bool) -> Result<crate::quads::FrameStats, String> {
        if self.style(0.0) {
            self.layout(0.0);
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
        if self.style(0.0) {
            self.layout(0.0);
        }
        crate::quads::paint_texture(&mut self.doc, self.width, self.height, 1.0, clear, id, under)
    }

    /// The page painted with what isn't painted left transparent into `dst` (quads::page_buffer), on the GPU with
    /// nothing read back; a frame the compositor can't do is painted on the CPU and written there.
    pub fn paint_transparent_buffer(&mut self, dst: &wgpu::Buffer) -> Result<(), String> {
        self.paint_buffer(false, None, dst)
    }

    /// The page painted into `dst` over texture `under` (a 3D scene, `quads::set_texture`), as `present_under` shows
    /// it in play; over black when `clear`.
    pub fn paint_buffer(&mut self, clear: bool, under: Option<u64>, dst: &wgpu::Buffer) -> Result<(), String> {
        if self.style(0.0) {
            self.layout(0.0);
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
        if self.style(0.0) {
            self.layout(0.0);
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
