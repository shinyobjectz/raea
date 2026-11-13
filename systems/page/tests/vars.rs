//! Custom properties set in an element's style attribute reach its children's var() (p12-css/var-inherit).
mod common;
use blitz_dom::{BaseDocument, DocumentConfig, LocalName, QualName, ns};
use common::{attr, q};

#[test]
fn inline_custom_properties_inherit() {
    let mut doc = BaseDocument::new(DocumentConfig::default());
    let (style, inner, outside) = {
        let mut m = doc.mutate();
        let root = m.doc.root_node().id;
        let html = m.create_element(q("html"), vec![]);
        m.append_children(root, &[html]);
        let head = m.create_element(q("head"), vec![]);
        let body = m.create_element(q("body"), vec![]);
        m.append_children(html, &[head, body]);
        let style = m.create_element(q("style"), vec![]);
        let css = m.create_text_node(":root { --box: 40px; } .box { width: var(--box); height: calc(var(--box) * 2); background: red; }");
        m.append_children(style, &[css]);
        let kit = m.create_element(q("div"), vec![attr("style", "--box:60px;")]);
        let inner = m.create_element(q("div"), vec![attr("class", "box")]);
        m.append_children(kit, &[inner]);
        let outside = m.create_element(q("div"), vec![attr("class", "box")]);
        m.append_children(body, &[style, kit, outside]);
        (style, inner, outside)
    };
    doc.process_style_element(style);
    doc.resolve(0.0);
    let w = |id| doc.get_client_bounding_rect(id).map(|r| (r.width, r.height));
    assert_eq!(w(outside), Some((40.0, 80.0)));
    assert_eq!(w(inner), Some((60.0, 120.0)));
}

/// An @keyframes animation with fill-mode forwards holds its last keyframe once it has run (p12-css/animations).
#[test]
fn animation_forwards_holds() {
    let mut doc = BaseDocument::new(DocumentConfig::default());
    let (style, kept) = {
        let mut m = doc.mutate();
        let root = m.doc.root_node().id;
        let html = m.create_element(q("html"), vec![]);
        m.append_children(root, &[html]);
        let head = m.create_element(q("head"), vec![]);
        let body = m.create_element(q("body"), vec![attr("style", "margin:0")]);
        m.append_children(html, &[head, body]);
        let style = m.create_element(q("style"), vec![]);
        let css = m.create_text_node("@keyframes grow { from { transform: none } to { transform: translateX(120px) } } .kept { width: 48px; height: 48px; animation: grow 60ms linear forwards }");
        m.append_children(style, &[css]);
        let kept = m.create_element(q("div"), vec![attr("class", "kept")]);
        m.append_children(body, &[style, kept]);
        (style, kept)
    };
    // the page is styled once before its sheet is in, as a live page is when React's <style> lands a commit later
    doc.resolve(0.0);
    doc.process_style_element(style);
    let mut xs = vec![];
    for t in [0.016, 0.03, 0.1, 0.2] {
        doc.resolve(t);
        xs.push(doc.get_client_bounding_rect(kept).map(|r| r.x));
    }
    assert_eq!(xs.last().copied().flatten(), Some(120.0), "x over time: {xs:?}");
    // a re-render that sets the same class again (React's commit after a click elsewhere) keeps the ended animation
    {
        let mut m = doc.mutate();
        m.set_attribute(kept, QualName { prefix: None, ns: ns!(), local: LocalName::from("class") }, "kept");
    }
    for t in [0.3, 0.4] {
        doc.resolve(t);
        xs.push(doc.get_client_bounding_rect(kept).map(|r| r.x));
    }
    assert_eq!(xs.last().copied().flatten(), Some(120.0), "x over time: {xs:?}");
}

/// A closed <select> is a box its author sizes, showing its chosen option (p14-forms/select-keys).
#[test]
fn select_is_sized() {
    let mut doc = BaseDocument::new(DocumentConfig::default());
    let (style, sel) = {
        let mut m = doc.mutate();
        let root = m.doc.root_node().id;
        let html = m.create_element(q("html"), vec![]);
        m.append_children(root, &[html]);
        let head = m.create_element(q("head"), vec![]);
        let body = m.create_element(q("body"), vec![attr("style", "margin:0")]);
        m.append_children(html, &[head, body]);
        let style = m.create_element(q("style"), vec![]);
        let css = m.create_text_node(".s { width: 160px; height: 36px; border: 1px solid gray; font-size: 14px }");
        m.append_children(style, &[css]);
        let sel = m.create_element(q("select"), vec![attr("class", "s")]);
        for t in ["red", "green", "blue"] {
            let o = m.create_element(q("option"), vec![attr("value", t)]);
            let tx = m.create_text_node(t);
            m.append_children(o, &[tx]);
            m.append_children(sel, &[o]);
        }
        m.append_children(body, &[style, sel]);
        (style, sel)
    };
    doc.process_style_element(style);
    doc.resolve(0.0);
    let r = doc.get_client_bounding_rect(sel).map(|r| (r.width, r.height));
    assert_eq!(r, Some((160.0, 36.0)));
}

