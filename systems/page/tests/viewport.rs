//! A page painted at a scale other than 1 (raea's window on a Retina display) is laid out at its size in CSS px:
//! Blitz is given the viewport in device px, which it divides by the scale. Given the size in CSS px, a page at
//! scale 2 was laid out in a quarter of its image (2026-11-12, issue 119).
use bevy::math::UVec2;
use blitz_dom::{BaseDocument, LocalName, QualName, ns};
use raea_sys_page::{Page, config};

fn q(local: &str) -> QualName {
    QualName { prefix: None, ns: ns!(html), local: LocalName::from(local) }
}

fn attr(local: &str, v: &str) -> blitz_dom::Attribute {
    blitz_dom::Attribute { name: QualName { prefix: None, ns: ns!(), local: LocalName::from(local) }, value: v.into() }
}

/// The box of a `width: 100vw; height: 100vh` element in a page: the CSS viewport as layout sees it.
fn full(at: Page) -> (f64, f64) {
    let mut doc = BaseDocument::new(config(at));
    let div = {
        let mut m = doc.mutate();
        let root = m.doc.root_node().id;
        let html = m.create_element(q("html"), vec![]);
        m.append_children(root, &[html]);
        let body = m.create_element(q("body"), vec![attr("style", "margin:0")]);
        m.append_children(html, &[body]);
        let div = m.create_element(q("div"), vec![attr("style", "width:100vw;height:100vh")]);
        m.append_children(body, &[div]);
        div
    };
    doc.resolve(0.0);
    let b = doc.get_client_bounding_rect(div).expect("the element has a box");
    (b.width, b.height)
}

#[test]
fn a_page_at_scale_2_is_laid_out_at_its_size_in_css_px() {
    let at = Page { size: UVec2::new(400, 300), scale: 2.0 };
    assert_eq!(at.px(), UVec2::new(800, 600));
    let viewport = config(at).viewport.expect("a page's config has a viewport");
    assert_eq!(viewport.window_size, (800, 600));
    assert_eq!(viewport.logical_size(), (400.0, 300.0));
    assert_eq!(full(at), (400.0, 300.0));
}

#[test]
fn a_page_at_scale_1_is_as_it_was() {
    let at = Page { size: UVec2::new(400, 300), scale: 1.0 };
    assert_eq!(at.px(), UVec2::new(400, 300));
    assert_eq!(config(at).viewport.expect("a page's config has a viewport").window_size, (400, 300));
    assert_eq!(full(at), (400.0, 300.0));
}

#[test]
fn at_a_fractional_scale_the_image_is_rounded_up() {
    assert_eq!(Page { size: UVec2::new(1001, 601), scale: 1.25 }.px(), UVec2::new(1252, 752));
    assert_eq!(Page { size: UVec2::new(0, 0), scale: 2.0 }.px(), UVec2::ONE);
}
