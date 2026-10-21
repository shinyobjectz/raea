//! A restyle that changes nothing about layout lays out the same as a fresh build: inline-block chips inside a
//! shrink-to-fit box, one chip's background changed (lightspeed Q1: a played timeline's caption box came out ~17 px
//! taller than the render's, which builds the frame fresh).
use raea::dom::Dom;

const BOX: &str = "background-color: #fbf7ef; border-radius: 32px; font-family: Helvetica; font-size: 64px; font-weight: 900; padding: 18px 28px";
const COL: &str = "position: absolute; inset: 0; display: flex; flex-direction: column; justify-content: flex-end; align-items: center; padding-bottom: 220px";

fn chip(a: f64) -> String {
    format!("background-color: rgba(255, 90, 122, {a}); border-radius: 999px; display: inline-block; margin: 0 2px; padding: 2px 12px")
}

/// The caption: a column holding the box, the box holding three chips. Returns the dom and the chips.
fn build(alphas: [f64; 3]) -> (Dom, Vec<raea::dom::NodeId>) {
    let mut d = Dom::new(1080, 1920, std::path::Path::new("."));
    let body = d.body();
    d.set_attribute(body, "style", "margin: 0");
    let col = d.create_element("div");
    d.set_attribute(col, "style", COL);
    let bx = d.create_element("div");
    d.set_attribute(bx, "style", BOX);
    let mut chips = Vec::new();
    for (w, a) in ["Steeped", "twenty", "hours,"].iter().zip(alphas) {
        let s = d.create_element("span");
        d.set_attribute(s, "style", &chip(a));
        let t = d.create_text(w);
        d.append(s, t);
        d.append(bx, s);
        chips.push(s);
    }
    d.append(col, bx);
    d.append(body, col);
    (d, chips)
}

fn boxes(d: &mut Dom) -> Vec<(String, f32, f32, f32, f32)> {
    d.laid_out(&[]).into_iter().map(|l| (l.tag, l.x, l.y, l.w, l.h)).collect()
}

#[test]
fn restyled_chip_lays_out_as_fresh() {
    let (mut fresh, _) = build([0.3, 0.6, 0.0]);
    let want = boxes(&mut fresh);
    let (mut d, chips) = build([0.6, 0.3, 0.0]);
    let _ = boxes(&mut d);
    d.set_attribute(chips[0], "style", &chip(0.3));
    d.set_attribute(chips[1], "style", &chip(0.6));
    let got = boxes(&mut d);
    assert_eq!(got, want, "after a restyle (left) against a fresh build (right)");
}
