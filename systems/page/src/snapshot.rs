//! The page's layout as script reads it: the one snapshot every page's JS gets its boxes from. An app page's (runtime/dom:
//! `__ui_layout` after each frame's layout, `layout.now` when script reads a box it changed) and a composition's
//! (`page.layout`, docs/layout-reads.feature LR1) are this same list, read the same way by sdk/ui's dom and sdk/raea's
//! page-layout.

use blitz_dom::BaseDocument;

use crate::ops::Nodes;

/// Numbers per element in the snapshot's `n` (sdk/ui's dom and sdk/raea's page-layout read the same).
pub const STRIDE: usize = 18;

/// Numbers per element in the snapshot's `o`, its offsets (sdk/raea's page-layout).
pub const OFFSETS: usize = 7;

/// An offsetParent that's the page's body (or the initial containing block): `offsetParent` is the body.
pub const OFFSET_BODY: i64 = -1;

/// No offsetParent: the element or an ancestor isn't displayed (Chrome's null).
pub const OFFSET_NONE: i64 = -2;

/// A number as the snapshot writes it: to 0.01 px, an integer without its point.
fn num(n: &mut String, v: f64) {
    let x = (v * 100.0).round() / 100.0;
    if x == x.trunc() {
        n.push_str(&format!("{},", x as i64));
    } else {
        n.push_str(&format!("{x},"));
    }
}

/// Per element, in page px: its id, then x, y, w, h (its bounding client rect: transformed, an inline element's line
/// fragments joined), scroll offsets and sizes, padding l t r b, border l t r b, and z (STRIDE numbers), as JSON with
/// the viewport's size. `id` gives the id script knows an element by, from its page id (None leaves it out).
/// With `offsets`, `o` has each element's id, offsetLeft, offsetTop, offsetWidth, offsetHeight (untransformed, from
/// its offsetParent's padding edge) its offsetParent's id (OFFSET_BODY, OFFSET_NONE), and 1 when it's a non-atomic inline (its client* and scroll* are 0).
pub fn snapshot_by(doc: &BaseDocument, nodes: &Nodes, id: impl Fn(u64) -> Option<u64>, offsets: bool) -> String {
    // the viewport in CSS px and the device pixels to one of them: what innerWidth, matchMedia and devicePixelRatio
    // answer. window_size is in device px, so on a Retina display it is twice the width the page is laid out at.
    let (vp, dpr) = (doc.viewport().logical_size(), doc.viewport().scale());
    let mut n = String::new();
    let mut o = String::new();
    for (page_id, node) in &nodes.by_id {
        let Some(sid) = id(*page_id) else { continue };
        // (text nodes have no layout data of their own to ask)
        let Some(el) = doc.get_node(*node).filter(|n| n.is_element()) else { continue };
        let Some(b) = doc.get_client_bounding_rect(*node) else { continue };
        let l = el.final_layout();
        let s = el.scroll_offset();
        num(&mut n, sid as f64);
        for v in [b.x, b.y, b.width, b.height, s.x, s.y, el.scroll_width() as f64, el.scroll_height() as f64] {
            num(&mut n, v);
        }
        for v in [l.padding.left, l.padding.top, l.padding.right, l.padding.bottom, l.border.left, l.border.top, l.border.right, l.border.bottom, 0.0] {
            num(&mut n, v as f64);
        }
        if !offsets {
            continue;
        }
        num(&mut o, sid as f64);
        if undisplayed(doc, *node) {
            for v in [0.0, 0.0, 0.0, 0.0, OFFSET_NONE as f64, 0.0] {
                num(&mut o, v);
            }
            continue;
        }
        let r = doc.offset_rect(*node).unwrap_or(blitz_dom::BoundingRect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 });
        // a non-atomic inline's offsetParent is its inline root's (the root itself when it's positioned), as offset_rect
        // measures it
        let parent = match el.inline_root_ancestor().filter(|_| el.inline_fragment_boxes().is_some()) {
            Some(root) if root.id != el.id && root.is_offset_parent() => Some(root),
            Some(root) => root.offset_parent(),
            None => el.offset_parent(),
        };
        let parent = match parent {
            None => OFFSET_BODY,
            Some(p) if p.data.is_element_with_tag_name(&blitz_dom::local_name!("body")) => OFFSET_BODY,
            Some(p) => nodes.id_of(p.id).and_then(&id).map_or(OFFSET_BODY, |i| i as i64),
        };
        let inline = if el.inline_fragment_boxes().is_some() { 1.0 } else { 0.0 };
        for v in [r.x, r.y, r.width, r.height, parent as f64, inline] {
            num(&mut o, v);
        }
    }
    n.pop();
    o.pop();
    if offsets {
        format!("{{\"vw\":{},\"vh\":{},\"dpr\":{dpr},\"n\":[{n}],\"o\":[{o}]}}", vp.0, vp.1)
    } else {
        format!("{{\"vw\":{},\"vh\":{},\"dpr\":{dpr},\"n\":[{n}]}}", vp.0, vp.1)
    }
}

/// [`snapshot_by`] with each element under its own page id, no offsets (an app page's).
pub fn snapshot(doc: &BaseDocument, nodes: &Nodes) -> String {
    snapshot_by(doc, nodes, Some, false)
}

/// Whether `node` or an ancestor is `display: none` (it has no box: offsetParent null, offsets 0, as in Chrome).
fn undisplayed(doc: &BaseDocument, node: blitz_dom::NodeId) -> bool {
    let mut at = Some(node);
    while let Some(id) = at {
        let Some(n) = doc.get_node(id) else { return false };
        if n.primary_styles().is_some_and(|s| s.get_box().display.is_none()) {
            return true;
        }
        at = n.parent;
    }
    false
}

