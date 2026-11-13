//! raea-runtime-js's wire ops (src/ops.rs there: what @raea/ui's core and React send) applied to a Blitz document through its
//! mutation API, the one Dioxus Native drives (docs/blitz-bevy.feature B1b). An element's props become what a browser
//! would hold: `className` the class attribute, `style` (React's object) the style attribute's CSS, the rest
//! attributes as React writes them. A `text` element is a text node.

use std::collections::HashMap;

use raea_core_wire::{Listening, Op};
use blitz_dom::{Attribute, BaseDocument, LocalName, Namespace, NodeId, QualName, ns};
use serde_json::{Map, Value};

mod attrs;
pub use attrs::*;

/// The ids the ops name, as the document's nodes, under the element that holds them (the page's root).
pub struct Nodes {
    pub root: NodeId,
    pub by_id: HashMap<u64, NodeId>,
    /// each node's id, so a node dropped with its parent's subtree leaves `by_id` (Blitz reuses its slot)
    ids: HashMap<NodeId, u64>,
    /// each id's kind, for SVG's attribute names and `text`
    kinds: HashMap<u64, String>,
    /// the events each element's listeners hear (the bridge's: host.rs)
    pub listening: HashMap<u64, Listening>,
    /// each element's props as React last set them (a test id's data-value can be a number, which its attribute isn't)
    pub props: HashMap<u64, Map<String, Value>>,
    /// each canvas's bitmap as its display list (sdk/ui's `__canvas2d`) drew it, raea-runtime-js's replay
    canvases: HashMap<u64, raea_sys_canvas::Replay>,
    /// scrollers whose scrollTop or scrollLeft the ops set, for the host to tell their scroll events
    pub scrolled: Vec<u64>,
    /// the document's own <html> and <body>, standing for the HTML runtime's (sdk/ui dom.ts adoptDocument), not
    /// elements mounted in its body, so what CSS gives only the root (the canvas background its transform leaves in
    /// place, `:root`, `html` and `body` selectors) reaches them as in a browser
    adopted: Vec<NodeId>,
}


impl Nodes {
    pub fn new(root: NodeId) -> Nodes {
        Nodes { root, by_id: HashMap::new(), ids: HashMap::new(), kinds: HashMap::new(), listening: HashMap::new(), props: HashMap::new(), canvases: HashMap::new(), scrolled: Vec::new(), adopted: Vec::new() }
    }

    /// The op id of a document node, when the ops made it.
    pub fn id_of(&self, node: NodeId) -> Option<u64> {
        // (the root is the ops' 0: a root container's listeners, React's among them, are on it)
        // (an adopted <body> is the root and the runtime's body: the runtime's id)
        if node == self.root && !self.adopted.contains(&node) { Some(0) } else { self.ids.get(&node).copied() }
    }

    /// An element's kind as the ops created it.
    pub fn kind(&self, id: u64) -> Option<&str> {
        self.kinds.get(&id).map(String::as_str)
    }

    /// Whether any element is of `kind`.
    pub fn has_kind(&self, kind: &str) -> bool {
        self.kinds.values().any(|k| k == kind)
    }

    /// The video and audio elements' ids, in the order they were made (their decoders open, and tell, in it).
    pub fn media_ids(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = self.kinds.iter().filter(|(_, k)| matches!(k.as_str(), "video" | "audio")).map(|(id, _)| *id).collect();
        ids.sort_unstable();
        ids
    }

    /// Whether element `id` hears `e`.
    pub fn hears(&self, id: u64, e: raea_core_wire::Ev) -> bool {
        self.listening.get(&id).is_some_and(|l| l.has(e))
    }
}

/// The ops applied in order. Ids the ops name that the document doesn't have are skipped, as raea-runtime-js's host does.
pub fn apply(doc: &mut BaseDocument, nodes: &mut Nodes, ops: Vec<Op>) {
    let mut styles = Vec::new();
    // canvases whose list or size changed
    let mut canvases = Vec::new();
    // a created element with autofocus, focused once it's in (as raea-runtime-js's host does, and Chrome on insertion)
    let mut autofocus = None;
    // a created menu, its first item focused once it's in (raea-runtime-js's MenuFocusState::Opening(First))
    let mut menus = vec![];
    // scrollTop and scrollLeft set (sdk/ui's scrollTo, clamped there): the scroller's offset, not an attribute
    let mut scrolls: Vec<(u64, &'static str, f64)> = Vec::new();
    let scroll_props = |id: u64, props: &Map<String, Value>, out: &mut Vec<(u64, &'static str, f64)>| {
        for k in ["scrollTop", "scrollLeft"] {
            if let Some(v) = props.get(k).and_then(Value::as_f64) {
                out.push((id, k, v));
            }
        }
        // scrollEnd set or changed (raea-runtime-js's: to the end whenever it changes): the document scrolls it to its end
        // once layout has its content (blitz-dom's scroll_to_end)
        if props.get("scrollEnd").is_some_and(|v| !v.is_null()) {
            out.push((id, "scrollEnd", 0.0));
        }
        // a bevy_ui Scrollbar track (sdk/render's Scrollbar), whose thumbs blitz-dom places after layout
        if props.contains_key("data-raea-bevy-scrollbar") {
            out.push((id, "scrollbar", 0.0));
        }
    };
    {
        let mut m = doc.mutate();
        for op in ops {
            match op {
                Op::Clear => {
                    m.remove_and_drop_all_children(nodes.root);
                    nodes.by_id.clear();
                    nodes.ids.clear();
                    nodes.kinds.clear();
                    // (the root stays, and what it hears: sdk/ui tells the root's listeners once)
                    nodes.listening.retain(|id, _| *id == 0);
                    nodes.props.clear();
                    nodes.canvases.clear();
                }
                Op::Create { id, kind, props, text } => {
                    if let Some(old) = nodes.by_id.get(&id).copied().filter(|n| !nodes.adopted.contains(n)) {
                        drop_node(&mut m, nodes, old);
                    }
                    let own = match props.get("$root").and_then(Value::as_str).unwrap_or("") {
                        "html" => m.parent_id(nodes.root),
                        "body" => Some(nodes.root),
                        _ => None,
                    };
                    let html = m.parent_id(nodes.root).filter(|h| m.element_name(*h).is_some_and(|q| &*q.local == "html"));
                    if html.is_some() && m.element_name(nodes.root).is_some_and(|q| &*q.local == "body")
                        && let Some(node) = own
                    {
                        // its attributes the runtime's (the body's margin:0 the host gives a page goes)
                        let had: Vec<QualName> = m.doc.get_node(node).and_then(|n| n.element_data()).map(|e| e.attrs().iter().map(|a| a.name.clone()).collect()).unwrap_or_default();
                        for q in had {
                            m.clear_attribute(node, q);
                        }
                        for a in attrs(&props, false) {
                            m.set_attribute(node, a.name, &a.value);
                        }
                        nodes.props.insert(id, props);
                        if !nodes.adopted.contains(&node) {
                            nodes.adopted.push(node);
                        }
                        nodes.by_id.insert(id, node);
                        nodes.ids.insert(node, id);
                        nodes.kinds.insert(id, kind);
                        continue;
                    }
                    // (a text node has no props; an SVG <text> element, the same kind, has its attributes, and its
                    // string child as `text`: React's inline text)
                    let node = if kind == "text" && props.is_empty() {
                        m.create_text_node(text.as_deref().unwrap_or(""))
                    } else {
                        // (text, image and title are SVG's only where sdk/ui says so: inside an <svg>)
                        let svg = in_svg(&kind) || props.get("$ns").and_then(Value::as_str) == Some("svg");
                        // sdk/ui's DOM layer sends a text input or a textarea as the host's text field: an <input> or a
                        // <textarea> here, whose `value` attribute Blitz's mutator keeps its editor at
                        // (multiline="shift" too, a field Shift+Enter breaks the line of: its plain Enter is the
                        // handler's, host.rs keeps it from breaking one)
                        let shift = props.get("multiline").and_then(Value::as_str) == Some("shift");
                        let multiline = shift || props.get("multiline") == Some(&Value::Bool(true));
                        let tag = match kind.as_str() {
                            "editableText" if multiline => "textarea",
                            "editableText" => "input",
                            k => k,
                        };
                        let mut at = attrs(&props, svg);
                        if shift {
                            at.push(Attribute { name: qual(ENTER_IS_KEY), value: "".into() });
                        }
                        // a menu's item takes the focus (raea-runtime-js's host gives it a TabIndex(0))
                        if props.get("menuItem").is_some_and(|v| v != &Value::Bool(false) && !v.is_null()) && !props.contains_key("tabIndex") {
                            at.push(Attribute { name: qual("tabindex"), value: "0".into() });
                        }
                        let el = m.create_element(name(tag, svg), at);
                        if let Some(t) = text.as_deref().filter(|_| kind != "editableText") {
                            let t = m.create_text_node(t);
                            m.append_children(el, &[t]);
                        }
                        // (a textarea's first text is its content)
                        if multiline && let Some(v) = props.get("value").and_then(Value::as_str) {
                            let t = m.create_text_node(v);
                            m.append_children(el, &[t]);
                        }
                        if kind == "canvas" {
                            canvases.push(id);
                        }
                        scroll_props(id, &props, &mut scrolls);
                        let on = |k: &str| props.get(k).is_some_and(|v| v == &Value::Bool(true) || v.as_str().is_some());
                        if (on("autoFocus") || on("autofocus")) && !on("disabled") {
                            autofocus = Some(el);
                        }
                        if on("menu") {
                            menus.push(el);
                        }
                        nodes.props.insert(id, props);
                        el
                    };
                    nodes.by_id.insert(id, node);
                    nodes.ids.insert(node, id);
                    nodes.kinds.insert(id, kind);
                }
                Op::Append { parent, child } => {
                    let (Some(p), Some(c)) = (parent_node(nodes, parent), nodes.by_id.get(&child).copied()) else { continue };
                    // (the adopted <html> and <body> are where they are already)
                    if nodes.adopted.contains(&c) {
                        continue;
                    }
                    m.append_children(p, &[c]);
                }
                Op::Insert { parent, child, before } => {
                    let (Some(_), Some(c), Some(b)) = (parent_node(nodes, parent), nodes.by_id.get(&child).copied(), nodes.by_id.get(&before).copied()) else { continue };
                    m.insert_nodes_before(b, &[c]);
                }
                Op::Remove { id } => {
                    // (already gone when an ancestor's removal took it)
                    if let Some(n) = nodes.by_id.get(&id).copied().filter(|n| !nodes.adopted.contains(n)) {
                        drop_node(&mut m, nodes, n);
                    }
                }
                Op::Update { id, set, unset, text } => {
                    let Some(n) = nodes.by_id.get(&id).copied() else { continue };
                    let kind = nodes.kinds.get(&id).map(String::as_str).unwrap_or("div");
                    if let Some(t) = text {
                        // (an element's inline text is its first child's)
                        let n = match m.doc.get_node(n).map(|x| (x.is_element(), x.children.first().copied())) {
                            Some((true, Some(c))) => c,
                            Some((true, None)) => {
                                let c = m.create_text_node("");
                                m.append_children(n, &[c]);
                                c
                            }
                            _ => n,
                        };
                        m.set_node_text(n, &t);
                        // a <style>'s text changed: its sheet is parsed again
                        if let Some(p) = m.parent_id(n).filter(|p| m.element_name(*p).is_some_and(|q| &*q.local == "style")) {
                            styles.push(p);
                        }
                    }
                    // (a text, image or title is SVG's when it was made in SVG's namespace: `$ns` at its create)
                    let svg = in_svg(kind) || m.element_name(n).is_some_and(|q| q.ns == ns!(svg));
                    // a textarea's value is its text too: Blitz makes its editor from its text (construct.rs)
                    let area = (kind == "editableText" && m.element_name(n).is_some_and(|q| &*q.local == "textarea")).then(|| set.as_ref()?.get("value")?.as_str().map(str::to_string)).flatten();
                    let kept = nodes.props.entry(id).or_default();
                    for key in unset.iter().flatten() { kept.remove(key); }
                    kept.extend(set.iter().flatten().map(|(k, v)| (k.clone(), v.clone())));
                    if kind == "canvas" && set.iter().flatten().any(|(k, _)| matches!(k.as_str(), "__canvas2d" | "width" | "height")) {
                        canvases.push(id);
                    }
                    if let Some(set) = &set {
                        scroll_props(id, set, &mut scrolls);
                    }
                    for key in unset.into_iter().flatten() {
                        if let Some(attr) = attr_name(&key, svg) {
                            m.clear_attribute(n, qual(&attr));
                        }
                    }
                    for (key, value) in set.into_iter().flatten() {
                        let Some(attr) = attr_name(&key, svg) else { continue };
                        match attr_value(&key, &value) {
                            Some(v) => m.set_attribute(n, qual(&attr), &v),
                            None => m.clear_attribute(n, qual(&attr)),
                        }
                    }
                    if let Some(t) = area.map(|v| m.create_text_node(&v)) { m.remove_and_drop_all_children(n); m.append_children(n, &[t]); }
                }
                // events go through the bridge (B3, H3: host.rs), not the document
                Op::Listen { id, events } => {
                    let l = nodes.listening.entry(id).or_default();
                    events.into_iter().for_each(|e| l.add(e));
                }
                Op::Unlisten { id, events } => {
                    if let Some(l) = nodes.listening.get_mut(&id) {
                        events.into_iter().for_each(|e| l.remove(e));
                    }
                }
            }
        }
    }
    // (a <style> whose text changed and that was removed later in the same batch is gone: #24)
    styles.retain(|s| doc.get_node(*s).is_some());
    for s in styles {
        doc.process_style_element(s);
    }
    mark_chosen(doc, nodes);
    for (id, k, v) in scrolls {
        if k == "scrollEnd" {
            if let Some(n) = nodes.by_id.get(&id).copied() {
                doc.scroll_to_end.push(n);
            }
            continue;
        }
        if k == "scrollbar" {
            if let Some(&n) = nodes.by_id.get(&id) && !doc.scrollbars.contains(&n) {
                doc.scrollbars.push(n);
            }
            continue;
        }
        let Some(n) = nodes.by_id.get(&id).copied() else { continue };
        // (kept in its range once layout has its content: blitz-dom's scroll_set)
        if !doc.scroll_set.contains(&n) {
            doc.scroll_set.push(n);
        }
        let Some(node) = doc.get_node_mut(n) else { continue };
        let at = node.scroll_offset_mut();
        if k == "scrollTop" { at.y = v } else { at.x = v }
        node.insert_damage(ALL_DAMAGE);
        node.mark_ancestors_dirty();
        if !nodes.scrolled.contains(&id) {
            nodes.scrolled.push(id);
        }
    }
    if let Some(n) = autofocus.filter(|n| doc.get_node(*n).is_some()) {
        doc.set_focus_to(n);
    }
    for m in menus {
        if let Some(first) = menu_items(doc, m).first().copied() {
            doc.set_focus_to(first);
        }
    }
    let fonts = (!canvases.is_empty()).then(|| raea_sys_canvas::page_fonts(&blitz_dom::util::svg_fonts()));
    for id in canvases {
        let (Some(props), Some(n)) = (nodes.props.get(&id), nodes.by_id.get(&id).copied()) else { continue };
        let r = raea_sys_canvas::Replay::update(nodes.canvases.remove(&id), props, fonts.as_ref().unwrap());
        let (w, h, px) = r.rgba();
        nodes.canvases.insert(id, r);
        let Some(node) = doc.get_node_mut(n) else { continue };
        let Some(el) = node.element_data_mut() else { continue };
        let data = vello::peniko::Blob::new(std::sync::Arc::new(px));
        el.special_data = blitz_dom::node::SpecialElementData::Image(Box::new(blitz_dom::node::ImageData::Raster(blitz_dom::node::RasterImageData { width: w, height: h, data })));
        node.clear_layout_cache();
        node.insert_damage(ALL_DAMAGE);
        node.mark_ancestors_dirty();
    }
}

/// Each closed select's chosen option marked `raea-chosen`, the one Blitz's UA sheet shows (Chrome draws a closed select
/// as its chosen option's text): its value prop's option, else one marked selected, else the first.
fn mark_chosen(doc: &mut BaseDocument, nodes: &Nodes) {
    let mut marks = Vec::new();
    for (&id, kind) in &nodes.kinds {
        if kind != "select" {
            continue;
        }
        let Some(&sel) = nodes.by_id.get(&id) else { continue };
        let props = nodes.props.get(&id);
        let mut options = Vec::new();
        let mut stack = vec![sel];
        while let Some(n) = stack.pop() {
            let Some(node) = doc.get_node(n) else { continue };
            if n != sel && node.element_data().is_some_and(|e| &*e.name.local == "option") {
                options.push(n);
                continue;
            }
            stack.extend(node.children.iter().rev().copied());
        }
        let prop = |n: NodeId, k: &str| nodes.id_of(n).and_then(|i| nodes.props.get(&i)).and_then(|p| p.get(k)).cloned();
        let value_of = |n: NodeId| match prop(n, "value") {
            Some(Value::String(s)) => s,
            Some(v) => v.to_string(),
            None => doc.get_node(n).map(|o| o.text_content().trim().to_string()).unwrap_or_default(),
        };
        let want = props.and_then(|p| p.get("value").or(p.get("defaultValue"))).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()));
        let chosen = want
            .and_then(|w| options.iter().copied().find(|o| value_of(*o) == w))
            .or_else(|| options.iter().copied().find(|o| prop(*o, "selected") == Some(Value::Bool(true))))
            .or(options.first().copied());
        for o in options {
            let has = doc.get_node(o).and_then(|n| n.element_data()).is_some_and(|e| e.attrs().iter().any(|a| &*a.name.local == "raea-chosen"));
            if has != (Some(o) == chosen) {
                marks.push((o, !has));
            }
        }
    }
    if marks.is_empty() {
        return;
    }
    let mut m = doc.mutate();
    for (o, on) in marks {
        if on {
            m.set_attribute(o, qual("raea-chosen"), "");
        } else {
            m.clear_attribute(o, qual("raea-chosen"));
        }
    }
}

/// A node and everything in it dropped, each one's id forgotten.
fn drop_node(m: &mut blitz_dom::DocumentMutator, nodes: &mut Nodes, node: NodeId) {
    let mut gone = Vec::new();
    m.remove_and_drop_node_with(node, &mut |n| gone.push(n));
    for n in gone {
        if let Some(id) = nodes.ids.remove(&n) {
            nodes.by_id.remove(&id);
            nodes.kinds.remove(&id);
            nodes.listening.remove(&id);
            nodes.props.remove(&id);
        }
    }
}

/// The node an op's parent id names: 0 is the page's root.
fn parent_node(nodes: &Nodes, id: u64) -> Option<NodeId> {
    if id == 0 { Some(nodes.root) } else { nodes.by_id.get(&id).copied() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_read_as_react_writes_them() {
        let o = serde_json::json!({"width": 10, "opacity": 0.5, "WebkitLineClamp": 2, "backgroundColor": "red", "--x": 3, "margin": 0, "zIndex": 2});
        let css = style_css(o.as_object().unwrap());
        for d in ["--x:3;", "-webkit-line-clamp:2;", "background-color:red;", "margin:0;", "opacity:0.5;", "width:10px;", "z-index:2;"] {
            assert!(css.contains(d), "{d} in {css}");
        }
        assert_eq!(kebab("msTransform"), "-ms-transform");
        assert_eq!(attr_name("onClick", false), None);
        assert_eq!(attr_name("strokeWidth", true).as_deref(), Some("stroke-width"));
        assert_eq!(attr_name("viewBox", true).as_deref(), Some("viewBox"));
    }
}

/// A menu's items (`menuItem`: a `menuitem` attribute), in document order.
pub fn menu_items(d: &BaseDocument, menu: NodeId) -> Vec<NodeId> {
    let mut out = vec![];
    let mut stack = vec![menu];
    while let Some(n) = stack.pop() {
        let Some(node) = d.get_node(n) else { continue };
        if n != menu && node.element_data().is_some_and(|el| el.attrs().iter().any(|a| &*a.name.local == "menuitem")) {
            out.push(n);
        }
        stack.extend(node.children.iter().rev());
    }
    out
}

/// Everything about an element made again (blitz-dom's ALL_DAMAGE, crate-private there): what loading an image does.
pub const ALL_DAMAGE: style::selector_parser::RestyleDamage = style::selector_parser::RestyleDamage::from_bits_retain(0b_0000_0000_1110_1111);
/// Only what an element is painted with changed, its box as it was: a video's next frame, the same size as its last.
pub const REPAINT_DAMAGE: style::selector_parser::RestyleDamage = style::selector_parser::RestyleDamage::REPAINT;
