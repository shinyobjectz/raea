//! The page's elements read back (fields, values, styles), its plugins, and the laid-out boxes handed out.

use super::*;

/// The page's root element's inline style (the app's look under the space: its background, ink and type).
/// bevy_ui's element kinds (<node>, <image>, <text>) as a bevy_ui `Node` is by default: the user agent sheet for a
/// space written for raea-runtime-js's host, under the space's own styles (src/bevy_ui.css).
pub const BEVY_UI_CSS: &str = include_str!("../bevy_ui.css");
/// Page `page`'s video frames are placed by its caller (an HTML composition's seek: html.rs), each where the seek
/// says; its streams still tell the JS their events, but don't draw.
pub fn place_videos_itself(world: &mut World, page: Entity) {
    if let Some(r) = world.non_send_mut::<Roots>().get_mut(page) {
        r.placed_videos = true;
    }
}

/// The page's presses as raea-runtime-js's host sends a bevy_ui root's (the legacy app's space was written for them): a press keeps a
/// focus its handler moved (raea-runtime-js focuses before the handler runs: the note field's autoFocus on the press that
/// pins it), and a right press is its contextmenu, with no pointerdown or pointerup (a card's onPointerDown opens
/// it). A page left as it opened has Chrome's.
pub fn bevy_ui_presses(world: &mut World, page: Entity) {
    if let Some(r) = world.get_non_send_mut::<Roots>().and_then(|r| r.into_inner().get_mut(page)) {
        r.bevy_ui = true;
    }
}

/// A text field as its editor holds it.
pub struct Field {
    pub text: String,
    /// the selection as the DOM's selectionStart and selectionEnd (UTF-16 offsets)
    pub caret: [usize; 2],
    /// marked text is being composed (IME)
    pub composing: bool,
}

/// An element the ops made, as the page last laid it out: what an app's test tree (page.ui) reads.
pub struct Element {
    pub id: u64,
    pub kind: String,
    pub props: serde_json::Map<String, Value>,
    /// its border box in page px, [x, y, w, h]
    pub rect: [f32; 4],
    /// it has a box, isn't visibility: hidden, and some of it is inside every ancestor's clip and the page
    pub visible: bool,
    /// it or something in it is under the pointer
    pub hovered: bool,
    pub focused: bool,
    pub field: Option<Field>,
    /// a checkbox's or radio's checked state as the page holds it (a press toggles it)
    pub checked: Option<bool>,
    /// a scroller's offset and its most, [top, max], in page px
    pub scroll: Option<[f32; 2]>,
    /// a select's chosen option's value (the one shown: its value prop, else its text)
    pub chosen: Option<String>,
}

/// The elements the ops made whose props `pick` takes, in document order.
pub fn elements(roots: &Roots, docs: &Docs, page: Entity, pick: impl Fn(&serde_json::Map<String, Value>) -> bool) -> Vec<Element> {
    use style::computed_values::visibility::T as Visibility;
    use style::values::computed::Overflow;
    let Some(r) = roots.0.iter().find(|r| r.page == page) else { return vec![] };
    let Some(doc) = docs.0.get(&page) else { return vec![] };
    // the window in CSS px, as the boxes below are: window_size is in device px (twice these on a Retina display)
    let vp = doc.viewport().logical_size();
    let window = [0.0, 0.0, vp.0, vp.1];
    let bounds = |n: NodeId| doc.get_client_bounding_rect(n).map(|b| [b.x as f32, b.y as f32, b.width as f32, b.height as f32]);
    let meets = |a: [f32; 4], b: [f32; 4]| a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3];
    let focus = doc.get_focussed_node_id();
    // a captured pointer keeps its element hovered (and what holds it), as Chrome does while it's captured
    let captured: Vec<NodeId> = r.capture.and_then(|id| ops::node(&r.nodes, id)).map(|c| doc.node_chain(c)).or_else(|| r.hover_frozen.clone()).unwrap_or_default();
    let frozen = r.capture.is_none() && r.hover_frozen.is_some();
    let mut out = vec![];
    // document order: a walk from the root
    let mut stack = vec![r.nodes.root];
    while let Some(n) = stack.pop() {
        let Some(node) = doc.get_node(n) else { continue };
        stack.extend(node.children.iter().rev().copied());
        let Some(id) = r.nodes.id_of(n) else { continue };
        let Some(props) = r.nodes.props.get(&id).filter(|p| pick(p)) else { continue };
        let rect = bounds(n).unwrap_or([0.0; 4]);
        let style = node.primary_styles();
        let hidden = style.as_ref().is_some_and(|s| matches!(s.clone_visibility(), Visibility::Hidden | Visibility::Collapse));
        let mut visible = node.has_boxes() && !hidden && rect[2] > 0.0 && rect[3] > 0.0 && meets(rect, window);
        // clipped by an ancestor that doesn't let its overflow show
        let mut up = node.parent;
        while visible && let Some(p) = up {
            let Some(pn) = doc.get_node(p) else { break };
            let clips = pn.primary_styles().is_some_and(|s| s.clone_overflow_x() != Overflow::Visible || s.clone_overflow_y() != Overflow::Visible);
            if clips && pn.is_element() && let Some(b) = bounds(p) {
                visible = meets(rect, b);
            }
            if !pn.has_boxes() && pn.is_element() {
                visible = false;
            }
            up = pn.parent;
        }
        let el = node.element_data();
        let field = el.and_then(|e| e.text_input_data()).map(|t| {
            let text = t.editor.raw_text().to_string();
            let sel = t.editor.raw_selection();
            let at = |i: usize| text.get(..i.min(text.len())).map_or(0, |t| t.encode_utf16().count());
            let (a, f) = (at(sel.anchor().index()), at(sel.focus().index()));
            Field { caret: [a.min(f), a.max(f)], composing: t.editor.is_composing(), text }
        });
        let scroll = style.as_ref().filter(|s| matches!(s.clone_overflow_y(), Overflow::Scroll | Overflow::Auto)).map(|_| {
            let max = (node.scroll_height() - node.client_height()).max(0.0);
            [node.scroll_offset().y as f32, max as f32]
        });
        out.push(Element {
            id,
            kind: r.nodes.kind(id).unwrap_or("div").to_string(),
            props: props.clone(),
            rect,
            visible,
            hovered: (if captured.is_empty() && !frozen { node.is_hovered() } else { captured.contains(&n) }) && visible,
            focused: focus == Some(n),
            field,
            checked: el.and_then(|e| e.checkbox_input_checked()),
            scroll,
            chosen: (r.nodes.kind(id) == Some("select")).then(|| chosen_value(doc, &r.nodes, n)).flatten(),
        });
    }
    out
}

/// The value of the option a select shows (ops.rs marks it raea-chosen): its value prop, else its text.
pub(crate) fn chosen_value(doc: &BaseDocument, nodes: &Nodes, select: NodeId) -> Option<String> {
    let mut stack = vec![select];
    while let Some(n) = stack.pop() {
        let node = doc.get_node(n)?;
        if node.element_data().is_some_and(|e| e.attrs().iter().any(|a| &*a.name.local == "raea-chosen")) {
            let prop = nodes.id_of(n).and_then(|i| nodes.props.get(&i)).and_then(|p| p.get("value"));
            return Some(match prop {
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => node.text_content().trim().to_string(),
            });
        }
        stack.extend(node.children.iter().rev().copied());
    }
    None
}

/// A stylesheet in the page's head, after any added before it (an app's own CSS: a Tailwind build, a base sheet).
pub fn add_style(world: &mut World, page: Entity, css: &str) {
    let Some(root) = world.get_non_send::<Roots>().and_then(|r| r.0.iter().find(|r| r.page == page).map(|r| r.nodes.root)) else { return };
    let mut docs = world.non_send_mut::<Docs>();
    let Some(doc) = docs.0.get_mut(&page) else { return };
    // the root is body; html's first child is head
    let Some(head) = doc.get_node(root).and_then(|b| b.parent).and_then(|h| doc.get_node(h)).and_then(|h| h.children.first().copied()) else { return };
    let style = {
        let mut m = doc.mutate();
        let el = m.create_element(blitz_dom::QualName { prefix: None, ns: blitz_dom::ns!(html), local: blitz_dom::LocalName::from("style") }, vec![]);
        let text = m.create_text_node(css);
        m.append_children(el, &[text]);
        m.append_children(head, &[el]);
        el
    };
    doc.process_style_element(style);
}

/// The roots' per-frame work that has to follow the page's layout: added by an app that mounts roots.
pub struct HostPlugin;

/// Each frame's [`step`] in [`PageSystems::Step`](raea_sys_page::PageSystems::Step): after the pages' input events, before
/// the game systems (H8). An app that runs `step` in its own order leaves this out.
pub struct HostStepPlugin;

impl Plugin for HostStepPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, step.in_set(raea_sys_page::PageSystems::Step));
    }
}

impl Plugin for HostPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, layout_out.after(raea_sys_page::paint));
    }
}

/// The page's layout as its JS reads it (`__ui_layout`'s argument, and `layout.now`'s answer: computed.rs): raea-sys-page's
/// snapshot, the one a composition's `page.layout` answers too (docs/layout-reads.feature LR1).
pub use raea_sys_page::snapshot;

/// After the page's layout: each root's snapshot to its JS's `__ui_layout`, as raea-runtime-js's dom.rs sends bevy_ui's (the
/// DOM layer's geometry: getBoundingClientRect, client and scroll sizes, hit tests, the observers, rAF). Per element,
/// in page px: id, x, y, w, h, scrollLeft, scrollTop, scrollWidth, scrollHeight, padding l t r b, border l t r b, z.
/// "" when it's the same as last frame's. What the JS committed from it (rAF, observers) is applied now and painted
/// next frame, as raea-runtime-js's is.
pub fn layout_out(world: &mut World, mut last: Local<HashMap<Entity, String>>) {
    let Some(mut roots) = world.remove_non_send::<Roots>() else { return };
    for r in &mut roots.0 {
        // a frame's root reads its own document, swapped out of its iframe as for its step (step.rs): in Docs its
        // page holds the iframe's placeholder, where none of its nodes are, so its JS saw no geometry at all
        if let Some(f) = &r.frame
            && !super::frames::swap_frame(world, r.page, f)
        {
            continue;
        }
        let json = {
            let docs = world.non_send::<Docs>();
            let Some(doc) = docs.0.get(&r.page) else {
                drop(docs);
                if let Some(f) = &r.frame {
                    super::frames::swap_frame(world, r.page, f);
                }
                continue;
            };
            snapshot(doc, &r.nodes)
        };
        let arg = if last.get(&r.page) == Some(&json) { String::new() } else { json.clone() };
        match r.js(world, |js| js.call("__ui_layout", &[Arg::Str(&arg)])) {
            Some(Err(e)) => eprintln!("raea-page: js: __ui_layout threw: {e}"),
            Some(Ok(_)) => {
                last.insert(r.page, json);
            }
            None => {}
        }
        drain(world, r);
        if let Some(f) = &r.frame {
            super::frames::swap_frame(world, r.page, f);
        }
    }
    world.insert_non_send(roots);
}
