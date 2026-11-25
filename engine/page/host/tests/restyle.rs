//! A stylesheet added while something is focused, hovered or pressed (a page that adds its styles as it draws: a sheet, a
//! dialog). raea.app's window crashed on it (11-24: Approve pressed): the pressed element's snapshot holds its state and no
//! attributes, and stylo's stylesheet invalidation asked it for the class and id it had.
use bevy::prelude::*;

#[test]
fn a_stylesheet_added_while_an_element_is_focused() {
    let app = App::new();
    let at = raea_sys_page::Page { size: UVec2::new(400, 300), scale: 1.0 };
    let config = raea_sys_page::providers::config_in(app.world(), at, None);
    let html = r#"<head><style>button:focus { color: rgb(1, 2, 3) }</style></head><body><button id="go" class="b">Go</button><div id="more"></div></body>"#;
    let mut doc = blitz_html::HtmlDocument::from_html(html, config).into_inner();
    doc.resolve(0.0);
    let go = doc.get_element_by_id("go").unwrap();
    doc.set_focus_to(go);
    let more = doc.get_element_by_id("more").unwrap();
    doc.mutate().set_inner_html(more, "<style>.b { width: 40px; display: block } #go { height: 20px }</style>");
    doc.resolve(0.0);
    assert_eq!(doc.resolved_style_value(go, "width"), "40px");
    assert_eq!(doc.resolved_style_value(go, "height"), "20px");
}
