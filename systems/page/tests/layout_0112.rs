//! Three of 0.1.11's layout misses, each laid out from its issue's own repro: `font-variant: small-caps` drew in normal
//! case (#249), a flex row whose items fit but for f32 rounding wrapped (#252), and a `display: table` element around
//! its text drew nothing at all (#254).
use bevy::math::UVec2;
use blitz_dom::BaseDocument;
use raea_sys_page::{Page, config};

fn laid_out(body: &str) -> BaseDocument {
    let html = format!("<!doctype html><html><body style=\"margin:0\">{body}</body></html>");
    let mut doc = blitz_html::HtmlDocument::from_html(&html, config(Page { size: UVec2::new(700, 400), scale: 1.0 })).into_inner();
    doc.resolve(0.0);
    doc
}

/// The border box of the element with `id`: x, y, width, height.
fn rect(doc: &BaseDocument, id: &str) -> (f64, f64, f64, f64) {
    let node = doc.query_selector(&format!("#{id}")).unwrap().unwrap_or_else(|| panic!("no #{id}"));
    let r = doc.get_client_bounding_rect(node).unwrap_or_else(|| panic!("#{id} has no box"));
    (r.x, r.y, r.width, r.height)
}

/// #249: small capitals in a face without an `smcp` feature (Georgia) are synthesised as Chrome does: each lower-case
/// letter drawn as its capital at 70% of the font size. So the line is as wide as those capitals written out at 28 px,
/// and narrower than the text in full capitals.
#[test]
fn small_caps_are_synthesised_capitals_at_70_percent() {
    let doc = laid_out(
        r#"<div style="font-size: 40px; font-family: Georgia, serif">
          <span id="caps" style="font-variant: small-caps">Compliance Audit</span><br>
          <span id="spelled">C<span style="font-size: 28px">OMPLIANCE</span> A<span style="font-size: 28px">UDIT</span></span><br>
          <span id="plain">Compliance Audit</span><br>
          <span id="upper">COMPLIANCE AUDIT</span>
        </div>"#,
    );
    let width = |id| rect(&doc, id).2;
    let (caps, spelled, plain, upper) = (width("caps"), width("spelled"), width("plain"), width("upper"));
    assert!((caps - spelled).abs() < 0.5, "small caps {caps} px wide, the capitals written out at 28 px {spelled}");
    assert!(caps < upper - 20.0, "small caps {caps} px wide, full capitals {upper}");
    assert!((caps - plain).abs() > 5.0, "small caps {caps} px wide, the text in normal case {plain}: drawn as it is");
}

/// #252: three 183.6 px items and two 54 px gaps add up to the 658.8 px container in exact arithmetic, a few ulps over
/// it in f32. Chrome lays out in 1/64 px units and keeps them on one line: rows of 3 and 2, not 2, 2 and 1.
#[test]
fn a_flex_row_that_fits_but_for_rounding_stays_one_row() {
    let (w, size, cols) = (1080.0_f64, 0.17, 3.0);
    let items: String = (0..5).map(|i| format!(r#"<div id="i{i}" style="width: {}px; height: 60px"></div>"#, size * w)).collect();
    let doc = laid_out(&format!(
        r#"<div style="display: flex; flex-wrap: wrap; justify-content: center; gap: {}px {}px; width: {}px">{items}</div>"#,
        0.04 * w,
        0.05 * w,
        (cols * size + (cols - 1.0) * 0.05) * w
    ));
    let y = |i: usize| rect(&doc, &format!("i{i}")).1;
    assert_eq!((y(1), y(2)), (y(0), y(0)), "the first three items share a row");
    assert!(y(3) > y(0) && y(4) == y(3), "the last two share the second row: {:?}", (0..5).map(y).collect::<Vec<_>>());
}

/// #254: a `display: table` element holding only text is laid out as Chrome lays it out, its text in an anonymous row
/// and cell: a block as wide as its text (shrink-to-fit), with the next block below it.
#[test]
fn a_table_around_its_text_is_as_wide_as_its_text() {
    let doc = laid_out(
        r#"<div style="font-size: 30px">
          <div id="table" style="display: table; background: #FFD43B">Line one as a table</div>
          <div id="next">Line two, block</div>
          <span id="text">Line one as a table</span>
        </div>"#,
    );
    let (tx, ty, tw, th) = rect(&doc, "table");
    let text = rect(&doc, "text");
    let next = rect(&doc, "next");
    assert!(th > 30.0, "the table is a line tall: {th}");
    assert!((tw - text.2).abs() < 0.5, "the table is {tw} px wide, its text {}", text.2);
    assert_eq!((tx, ty), (0.0, 0.0));
    assert!((next.1 - (ty + th)).abs() < 0.5, "the next block starts below the table: {} against {}", next.1, ty + th);
}

/// #254, the rest of css-tables-3's wrapping: a table's stray text beside a row of its own gets an anonymous row of its
/// own above it, and a table of rows alone builds no anonymous box (its cells as before).
#[test]
fn stray_text_beside_a_row_gets_a_row_of_its_own() {
    let doc = laid_out(
        r#"<div id="t" style="display: table; font-size: 20px">loose
          <div style="display: table-row"><div id="cell" style="display: table-cell">in a cell</div></div></div>
          <table id="plain"><tr><td id="td">a</td></tr></table>"#,
    );
    let cell = rect(&doc, "cell");
    assert!(cell.1 >= 20.0, "the cell is in the second row, below the loose text: {cell:?}");
    let td = rect(&doc, "td");
    assert!(td.2 > 0.0 && td.3 > 0.0, "a plain table's cell keeps its box: {td:?}");
}
