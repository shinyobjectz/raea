//! A Blitz page's accessibility:
//! the document's elements as raea-sys-a11y computes their roles, names and states from, read two ways. `records` is the
//! parity harness's, by the wire's element ids (what React made); publish.rs is the OS's, the whole document by its own
//! nodes (raea's window, the `window` feature).
#[cfg(feature = "window")]
mod publish;
#[cfg(feature = "window")]
pub use publish::{AxPlugin, Host};

use std::collections::HashMap;

use bevy::prelude::*;
use blitz_dom::{BaseDocument, Node};
use raea_runtime_dom::host::Roots;
use raea_sys_a11y::El;
use raea_sys_page::Docs;
use raea_sys_page::ops::Nodes;
use serde_json::{Map, Value, json};

/// A page's Blitz document and its wire ids, for `f`. None before the page is up.
fn with_doc<R>(world: &World, page: Entity, f: impl FnOnce(&BaseDocument, &Nodes) -> R) -> Option<R> {
    let roots = world.get_non_send::<Roots>()?;
    let docs = world.get_non_send::<Docs>()?;
    let (r, doc) = (roots.0.iter().find(|r| r.page == page)?, docs.0.get(&page)?);
    Some(f(doc, &r.nodes))
}

/// What raea-sys-a11y asks of the document, an element's computed style and its control's state, by either reader's ids.
struct Facts<'a, K> {
    /// the element with an id
    node: &'a dyn Fn(K) -> Option<&'a Node>,
    test_id: &'a dyn Fn(K) -> Option<String>,
    /// the whole document's: an element with no style isn't drawn (it or an ancestor is display: none), and a name
    /// spaces what's laid out as blocks
    drawn: bool,
}

impl<K: Copy> raea_sys_a11y::Facts<K> for Facts<'_, K> {
    fn display_none(&self, k: K) -> bool {
        let Some(n) = (self.node)(k) else { return false };
        n.primary_styles().map_or(self.drawn, |s| s.get_box().display.is_none())
    }
    fn invisible(&self, k: K) -> bool {
        use style::computed_values::visibility::T as V;
        (self.node)(k).and_then(|n| n.primary_styles()).is_some_and(|s| matches!(s.clone_visibility(), V::Hidden | V::Collapse))
    }
    fn natively_disabled(&self, _: K) -> bool {
        false
    }
    fn editor_value(&self, k: K) -> Option<String> {
        (self.node)(k).and_then(|n| n.element_data()).and_then(|e| e.text_input_data()).map(|t| t.editor.raw_text().to_string())
    }
    fn tabbable(&self, k: K) -> bool {
        (self.node)(k).is_some_and(|n| n.is_focussable())
    }
    fn checked(&self, k: K) -> bool {
        (self.node)(k).and_then(|n| n.element_data()).and_then(|e| e.checkbox_input_checked()).unwrap_or(false)
    }
    fn block(&self, k: K) -> Option<bool> {
        use style::values::specified::box_::DisplayOutside;
        let d = (self.node)(k).filter(|_| self.drawn)?.primary_styles()?.get_box().display;
        Some(d.outside() == DisplayOutside::Block)
    }
    fn test_id(&self, k: K) -> Option<String> {
        (self.test_id)(k)
    }
}

/// An svg's `<title>`.
fn svg_title(doc: &BaseDocument, n: &Node) -> Option<String> {
    let title = n.children.iter().filter_map(|c| doc.get_node(*c)).find(|c| c.element_data().is_some_and(|e| &*e.name.local == "title"))?;
    Some(title.text_content())
}

/// The tag raea-sys-a11y knows an element of kind `kind` by: an editable text is a field.
fn tag_of<'a>(kind: &'a str, props: &Map<String, Value>) -> &'a str {
    match kind {
        "editableText" if props.get("multiline") == Some(&Value::Bool(true)) => "textarea",
        "editableText" => "input",
        k => k,
    }
}

fn wire_test_id(nodes: &Nodes, id: u64) -> Option<String> {
    nodes.props.get(&id)?.get("data-testid")?.as_str().map(str::to_string)
}

/// The `a11y` method on a page: each element with a data-testid as AccessKit has it, by the computation bevy_ui's page
/// uses (raea-runtime-js's a11y.rs), over the document's elements and the props React set on them.
pub fn records(world: &World, page: Entity) -> Value {
    with_doc(world, page, |doc, n| {
        let mut els = HashMap::new();
        for (&id, &node) in &n.by_id {
            let Some(dn) = doc.get_node(node) else { continue };
            let kind = n.kind(id).unwrap_or("div");
            let props = n.props.get(&id).cloned().unwrap_or_default();
            let tag = tag_of(kind, &props).to_string();
            let text = (kind == "text").then(|| dn.text_content());
            let title = if kind == "svg" { svg_title(doc, dn) } else { None };
            let parent = dn.parent.and_then(|p| n.id_of(p)).filter(|p| n.by_id.contains_key(p));
            let kids = dn.children.iter().filter_map(|c| n.id_of(*c)).filter(|c| n.by_id.contains_key(c)).collect();
            els.insert(id, El { tag, props, parent, kids, text, title });
        }
        let tops = els.keys().map(|k| (*k, 0)).collect();
        let node = |id: u64| n.by_id.get(&id).and_then(|k| doc.get_node(*k));
        let test_id = |id: u64| wire_test_id(n, id);
        let facts = Facts { node: &node, test_id: &test_id, drawn: false };
        let focus = doc.get_focussed_node_id().and_then(|f| n.id_of(f));
        raea_sys_a11y::records(els, tops, &facts, focus, |k| k)
    })
    .unwrap_or_else(|| json!({"nodes": {}}))
}
