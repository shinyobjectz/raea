//! The render manifest's videos, text and pictures (render.json, `raea render --manifest`), as the page laid them out.

use super::*;

impl BlitzPage {
    /// The render manifest's videos and text, as laid out (raea check's clipped, safe-area and reading rules read it):
    /// each shown element with a `data-raea-src` (a video: its source, time and box), and each element's own words
    /// (its text children's, inline elements' words going to the nearest element up with a box) with that box, its
    /// opacity, colour, size, weight and alignment as computed. In document order, as raea-runtime-js's page gave them.
    /// And the page's pictures (picture()): what fills the frame besides its words and videos.
    pub fn manifest(&mut self) -> (Vec<serde_json::Value>, Vec<serde_json::Value>, Vec<serde_json::Value>) {
        use serde_json::json;
        use style::values::specified::text::TextAlignKeyword;
        let docs = self.app.world().non_send::<Docs>();
        let Some(doc) = docs.0.get(&self.page) else { return (vec![], vec![], vec![]) };
        let least = PICTURE_LEAST * self.w as f64 * self.h as f64;
        let r = |v: f64| (v * 10.0).round() / 10.0;
        let rect = |n: blitz_dom::NodeId| doc.get_client_bounding_rect(n).filter(|b| b.width > 0.0 && b.height > 0.0);
        let alpha = |n: blitz_dom::NodeId| alpha(doc, n);
        let stamps = |n: blitz_dom::NodeId, entry: &mut serde_json::Value| stamps(doc, n, entry);
        let (mut videos, mut text, mut pictures) = (Vec::new(), Vec::new(), Vec::new());
        let mut owned: Vec<(blitz_dom::NodeId, Vec<String>)> = Vec::new();
        // the whole document: a composition's sequences and screens are pages of their own under the root's
        let mut stack = vec![doc.root_node().id];
        while let Some(n) = stack.pop() {
            let Some(node) = doc.get_node(n) else { continue };
            if node.is_element() && matches!(node.element_data().map(|e| &*e.name.local), Some("style" | "script")) {
                continue;
            }
            // an inline <svg> is drawn as one picture (usvg's): its <text>s are runs of their own, where usvg set them
            if let Some(tree) = node.element_data().and_then(|e| e.svg_data()) {
                if let (Some(b), a) = (rect(n), alpha(n)) {
                    if a > 0.01 && b.width * b.height >= least {
                        pictures.push(json!({"kind": "svg", "box": [r(b.x), r(b.y), r(b.width), r(b.height)], "opacity": (a * 100.0).round() / 100.0}));
                    }
                    let (sx, sy) = (b.width / tree.size().width().max(1e-6) as f64, b.height / tree.size().height().max(1e-6) as f64);
                    for (words, t, o) in svg_runs(tree.root(), a) {
                        let bb = [r(b.x + t.x() as f64 * sx), r(b.y + t.y() as f64 * sy), r(t.width() as f64 * sx), r(t.height() as f64 * sy)];
                        let mut v = json!({"text": words, "box": bb, "ink": bb, "opacity": (o * 100.0).round() / 100.0, "svg": true});
                        stamps(n, &mut v);
                        text.push(v);
                    }
                }
                continue;
            }
            stack.extend(node.children.iter().rev().copied());
            if let Some(src) = node.attr(LocalName::from("data-raea-src")) {
                let a = alpha(n);
                if let (Some(b), true) = (rect(n), a > 0.01) {
                    let at = node.attr(LocalName::from("data-raea-at")).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                    let mut v = json!({"src": src, "at": at, "box": [r(b.x), r(b.y), r(b.width), r(b.height)], "opacity": (a * 100.0).round() / 100.0});
                    // what of the box shows when a box it's in clips it (an `overflow: hidden` parent cropping it):
                    // the exports crop the clip to it; one clipped away entirely isn't shown
                    if let Some([x0, y0, x1, y1]) = clip(doc, n) {
                        let c = [x0.max(b.x), y0.max(b.y), x1.min(b.x + b.width), y1.min(b.y + b.height)];
                        if c[2] - c[0] < 0.5 || c[3] - c[1] < 0.5 {
                            continue;
                        }
                        if (c[0] - b.x).abs() > 0.05 || (c[1] - b.y).abs() > 0.05 || (c[2] - b.x - b.width).abs() > 0.05 || (c[3] - b.y - b.height).abs() > 0.05 {
                            v["clip"] = json!([r(c[0]), r(c[1]), r(c[2] - c[0]), r(c[3] - c[1])]);
                        }
                    }
                    // how the picture fills its box (the CapCut export places a clip by it, docs/capcut.feature CC2a)
                    if let Some(s) = node.primary_styles() {
                        use style::computed_values::object_fit::T as Fit;
                        v["fit"] = json!(match s.clone_object_fit() {
                            Fit::Contain => "contain",
                            Fit::Cover => "cover",
                            Fit::Fill => "fill",
                            Fit::None => "none",
                            Fit::ScaleDown => "scale-down",
                        });
                        // where the picture sits in what its box leaves free (object-position), when not centred: each
                        // axis as px plus a share of the free space ([x px, x share, y px, y share]), keywords and calc()
                        // alike, as blitz-paint places it
                        use style::values::computed::Length;
                        let p = s.clone_object_position();
                        let lin = |lp: &style::values::computed::LengthPercentage| {
                            let (a, b) = (lp.resolve(Length::new(0.0)).px() as f64, lp.resolve(Length::new(1000.0)).px() as f64);
                            (a, (b - a) / 1000.0)
                        };
                        let ((xp, xs), (yp, ys)) = (lin(&p.horizontal), lin(&p.vertical));
                        if xp.abs() > 0.01 || yp.abs() > 0.01 || (xs - 0.5).abs() > 0.001 || (ys - 0.5).abs() > 0.001 {
                            // a share to a millionth: of a 4K picture's free space, well under a pixel
                            let share = |v: f64| (v * 1e6).round() / 1e6;
                            v["position"] = json!([r(xp), share(xs), r(yp), share(ys)]);
                        }
                    }
                    stamps(n, &mut v);
                    videos.push(v);
                }
                continue;
            }
            if let Some(kind) = picture(node) {
                let a = alpha(n);
                if let (Some(b), true) = (rect(n), a > 0.01) {
                    if b.width * b.height >= least {
                        pictures.push(json!({"kind": kind, "box": [r(b.x), r(b.y), r(b.width), r(b.height)], "opacity": (a * 100.0).round() / 100.0}));
                    }
                }
            }
            if !node.is_text_node() {
                continue;
            }
            // kept untrimmed: a space in a span of its own is where a word ends in letters set one a span
            let words = node.text_content();
            if words.is_empty() {
                continue;
            }
            // an inline element (a <span> around a caption word) has no box of its own: its words belong to the
            // nearest element up that has one
            let mut up = node.parent;
            while let Some(p) = up {
                let inline = doc.get_node(p).and_then(|x| x.primary_styles()).is_some_and(|s| s.clone_display().outside() == style::values::specified::box_::DisplayOutside::Inline);
                if !inline && rect(p).is_some() {
                    break;
                }
                up = doc.get_node(p).and_then(|x| x.parent);
            }
            let Some(owner) = up else { continue };
            if node.parent.is_none_or(|p| alpha(p) <= 0.01) {
                continue;
            }
            match owned.iter_mut().find(|(o, _)| *o == owner) {
                Some((_, ws)) => ws.push(words.to_string()),
                None => owned.push((owner, vec![words.to_string()])),
            }
        }
        for (owner, words) in owned {
            let Some(words) = run_text(&words) else { continue };
            let (Some(b), Some(node)) = (rect(owner), doc.get_node(owner)) else { continue };
            let Some(s) = node.primary_styles() else { continue };
            let c = s.clone_color().to_color_space(style::color::ColorSpace::Srgb);
            let [cr, cg, cb] = [c.components.0, c.components.1, c.components.2].map(|v| (v.clamp(0.0, 1.0) * 255.0).round());
            let ca = c.alpha;
            let color = if ca >= 1.0 { format!("rgb({cr}, {cg}, {cb})") } else { format!("rgba({cr}, {cg}, {cb}, {})", (ca * 1000.0).round() / 1000.0) };
            let font = s.get_font();
            let align = match s.clone_text_align() {
                TextAlignKeyword::Center | TextAlignKeyword::MozCenter => "center",
                TextAlignKeyword::Right | TextAlignKeyword::End | TextAlignKeyword::MozRight => "right",
                TextAlignKeyword::Justify => "justify",
                _ => "left",
            };
            // words set in an element shorter or narrower than they are (a fixed height, overflow visible) are drawn
            // past it: the text's box is where they are, so it grows to cover its lines (DR4's caption cheat)
            let (mut w, mut h) = (b.width, b.height);
            if let Some(l) = node.element_data().and_then(|e| e.inline_layout_data.as_ref()) {
                let (fl, scale) = (node.final_layout(), doc.viewport().scale() as f64);
                // the element's own size to its bounding rect's (a transform scales one into the other)
                let (fx, fy) = (b.width / (fl.size.width as f64).max(1e-6), b.height / (fl.size.height as f64).max(1e-6));
                w = w.max(((fl.border.left + fl.padding.left) as f64 + l.layout.width() as f64 / scale) * fx);
                h = h.max(((fl.border.top + fl.padding.top) as f64 + l.layout.height() as f64 / scale) * fy);
            }
            let mut t = json!({"text": words, "box": [r(b.x), r(b.y), r(w), r(h)], "opacity": (alpha(owner) * 100.0).round() / 100.0,
                "color": color, "size": font.font_size.computed_size().px() as f64, "weight": font.font_weight.value() as f64, "align": align});
            // where the letters are across the lines (each line's offset is its alignment's), as a box of its own: a
            // centred line in a full-width box is tested by this, not the box (raea check's safe-area rule, #89)
            if let Some(l) = node.element_data().and_then(|e| e.inline_layout_data.as_ref()) {
                let (fl, scale) = (node.final_layout(), doc.viewport().scale() as f64);
                let fx = b.width / (fl.size.width as f64).max(1e-6);
                let (mut x0, mut x1) = (f64::MAX, f64::MIN);
                // the glyph runs' and inline-blocks' span on each line, inside the line's advance (its trailing spaces
                // hang past it): an inline element's padding at the line's ends (its edge spacers) is its box's, not
                // ink (a caption's padded word pill)
                for line in l.layout.lines() {
                    let m = line.metrics();
                    let (end, mut seen) = ((m.offset + m.advance) as f64, false);
                    for item in line.items() {
                        let (a, b) = match item {
                            parley::PositionedLayoutItem::GlyphRun(g) => (g.offset(), g.offset() + g.advance()),
                            parley::PositionedLayoutItem::InlineBox(b)
                                if b.kind == parley::InlineBoxKind::InFlow && b.id & parley::InlineBox::EDGE == 0 => (b.x, b.x + b.width),
                            _ => continue,
                        };
                        seen = true;
                        x0 = x0.min(a as f64);
                        x1 = x1.max((b as f64).min(end));
                    }
                    if !seen {
                        x0 = x0.min(m.offset as f64);
                        x1 = x1.max(end);
                    }
                }
                if x1 > x0 {
                    let lx = (fl.border.left + fl.padding.left) as f64 + x0 / scale;
                    // through the element's transforms as its box is (a card turned under a perspective draws its line
                    // narrower at the far end: a share of the box's width would put it where it isn't)
                    let local_h = (fl.size.height as f64).max((fl.border.top + fl.padding.top) as f64 + l.layout.height() as f64 / scale);
                    t["ink"] = match doc.transformed_rect(owner, [lx, 0.0, (x1 - x0) / scale, local_h]) {
                        Some(q) => json!([r(q.x), r(q.y), r(q.width), r(q.height)]),
                        None => json!([r(b.x + lx * fx), r(b.y), r((x1 - x0) / scale * fx), r(h)]),
                    };
                }
            }
            // its first named face and how many lines it was set on (docs/direction.feature DR1's Text)
            if let Some(f) = font.font_family.families.iter().find_map(|f| match f { style::values::computed::font::SingleFontFamily::FamilyName(n) => Some(n.name.to_string()), _ => None }) {
                t["family"] = json!(f);
            }
            if let Some(l) = node.element_data().and_then(|e| e.inline_layout_data.as_ref()) {
                t["lines"] = json!(l.layout.len());
            }
            // words a caption says (docs/layers.feature LA1a, LA2b): an export in layers renders captions as a pass of
            // their own only when a frame has some, and writes them as SubRip
            if in_caption(doc, owner) {
                t["caption"] = json!(true);
            }
            // words meant to sit on a 3D part (raea-sys-stage's type-over-part leaves them be, SG11)
            if up(doc, owner, |node| node.attr(LocalName::from("data-allow-overlap")).is_some()) {
                t["allowOverlap"] = json!(true);
            }
            // the world point a Label's words name ("x y z"), which makes the part there the shot's subject (SG14)
            if let Some(p) = up_attr(doc, owner, "data-points").and_then(|v| v.split_whitespace().map(|n| n.parse::<f64>().ok()).collect::<Option<Vec<f64>>>()).filter(|p| p.len() == 3 && p.iter().all(|n| n.is_finite())) {
                t["points"] = json!(p);
            }
            stamps(owner, &mut t);
            text.push(t);
        }
        (videos, text, pictures)
    }
}

/// A picture smaller than this share of the frame is left out of the manifest: an icon or a rule fills none of it
/// that a check counts (raea-sys-stage's coverage is a grid of 3% cells), and a page can hold hundreds.
const PICTURE_LEAST: f64 = 0.0025;

/// What an element paints over the frame besides words: "img" (an <img>), "background" (a CSS background image), or
/// "fill" (a background colour at least half opaque). None for one that paints nothing of its own.
fn picture(node: &blitz_dom::Node) -> Option<&'static str> {
    if !node.is_element() {
        return None;
    }
    if node.element_data().is_some_and(|e| &*e.name.local == "img") {
        return Some("img");
    }
    let s = node.primary_styles()?;
    let bg = s.get_background();
    if bg.background_image.0.iter().any(|i| !matches!(i, style::values::computed::image::Image::None)) {
        return Some("background");
    }
    (bg.background_color.resolve_to_absolute(&s.clone_color()).alpha >= 0.5).then_some("fill")
}

impl BlitzPage {
    /// The first element shown on the page that paints something not black: a background colour, or its own words'
    /// colour, said with where it's written ("a rgb(34, 34, 34) background (handover.tsx:31:3)"). A frame that came out
    /// all black while this is on it is one the GPU lost (#96). A video's holder is skipped: it's see-through.
    pub fn visible_paint(&mut self) -> Option<String> {
        let docs = self.app.world().non_send::<Docs>();
        let doc = docs.0.get(&self.page)?;
        let (w, h) = (self.w as f64, self.h as f64);
        let rgb = |c: style::color::AbsoluteColor| {
            let c = c.to_color_space(style::color::ColorSpace::Srgb);
            let [r, g, b] = [c.components.0, c.components.1, c.components.2].map(|v| (v.clamp(0.0, 1.0) * 255.0).round());
            (r.max(g).max(b) > 8.0 && c.alpha > 0.05).then(|| format!("rgb({r}, {g}, {b})"))
        };
        let mut stack = vec![doc.root_node().id];
        while let Some(n) = stack.pop() {
            let Some(node) = doc.get_node(n) else { continue };
            stack.extend(node.children.iter().rev().copied());
            // an element's box only: the document and text nodes have none to ask for (blitz-dom panics on their transform)
            if !node.is_element() || node.attr(LocalName::from("data-raea-src")).is_some() {
                continue;
            }
            let on = doc.get_client_bounding_rect(n).is_some_and(|b| b.width > 0.0 && b.height > 0.0 && b.x < w && b.y < h && b.x + b.width > 0.0 && b.y + b.height > 0.0);
            if !on || alpha(doc, n) <= 0.01 {
                continue;
            }
            let Some(s) = node.primary_styles() else { continue };
            let words = node.children.iter().any(|c| doc.get_node(*c).is_some_and(|c| c.is_text_node() && !c.text_content().trim().is_empty()));
            let paint = rgb(s.clone_background_color().resolve_to_absolute(&s.clone_color())).map(|c| format!("a {c} background")).or_else(|| words.then(|| rgb(s.clone_color())).flatten().map(|c| format!("{c} text")));
            if let Some(p) = paint {
                let mut at = serde_json::json!({});
                stamps(doc, n, &mut at);
                return Some(match at["loc"].as_str() { Some(l) => format!("{p} ({l})"), None => p });
            }
        }
        None
    }
}

/// The part of the page an element can show in, as the boxes that clip it leave it: the padding boxes, on the axes they
/// clip, of each `overflow` other than visible along its containing blocks (the clips blitz-paint draws it under; the
/// root element's overflow is the viewport's and clips nothing), as [left, top, right, bottom]. None when none clips it.
fn clip(doc: &blitz_dom::BaseDocument, n: blitz_dom::NodeId) -> Option<[f64; 4]> {
    use style::values::computed::Overflow;
    let mut out: Option<[f64; 4]> = None;
    let mut up = doc.get_node(n).and_then(|x| x.containing_block());
    while let Some(id) = up {
        let Some(node) = doc.get_node(id) else { break };
        up = node.containing_block();
        let is_root_element = node.parent.and_then(|p| doc.get_node(p)).is_some_and(|p| p.parent.is_none());
        if !node.is_element() || node.parent.is_none() || is_root_element {
            continue;
        }
        let Some(s) = node.primary_styles() else { continue };
        let (cx, cy) = (!matches!(s.clone_overflow_x(), Overflow::Visible), !matches!(s.clone_overflow_y(), Overflow::Visible));
        if !(cx || cy) {
            continue;
        }
        let Some(b) = doc.get_client_bounding_rect(id) else { continue };
        let bd = node.final_layout().border;
        let o = out.get_or_insert([f64::NEG_INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::INFINITY]);
        if cx {
            o[0] = o[0].max(b.x + bd.left as f64);
            o[2] = o[2].min(b.x + b.width - bd.right as f64);
        }
        if cy {
            o[1] = o[1].max(b.y + bd.top as f64);
            o[3] = o[3].min(b.y + b.height - bd.bottom as f64);
        }
    }
    out
}

/// An element's own opacity times its parents', 0 when it or one of them isn't shown.
fn alpha(doc: &blitz_dom::BaseDocument, n: blitz_dom::NodeId) -> f64 {
    use style::computed_values::visibility::T as Visibility;
    let (mut a, mut at) = (1.0f64, Some(n));
    while let Some(x) = at {
        let Some(node) = doc.get_node(x) else { break };
        if node.is_element() {
            let Some(s) = node.primary_styles() else { return 0.0 };
            if !node.has_boxes() || (x == n && matches!(s.clone_visibility(), Visibility::Hidden | Visibility::Collapse)) {
                return 0.0;
            }
            a *= s.get_effects().opacity as f64;
        }
        at = node.parent;
    }
    a
}

/// The source stamps (engine/bundle stamp.rs) of the nearest element up from `n` that has them, into `entry`: where the
/// JSX is written ("promo.tsx:12:7"), and the uses of the edit's components it was drawn in, nearest first.
fn stamps(doc: &blitz_dom::BaseDocument, n: blitz_dom::NodeId, entry: &mut serde_json::Value) {
    use serde_json::json;
    let mut at = Some(n);
    while let Some(x) = at {
        let Some(node) = doc.get_node(x) else { break };
        if let Some(loc) = node.attr(LocalName::from("data-raea-loc")) {
            entry["loc"] = json!(loc);
            if let Some(uses) = node.attr(LocalName::from("data-raea-use")).filter(|u| !u.is_empty()) {
                entry["use"] = json!(uses.split('|').collect::<Vec<_>>());
            }
            // its place in a list (its .map()'s key), when it has one
            if let Some(idx) = node.attr(LocalName::from("data-raea-idx")) {
                entry["index"] = json!(idx);
            }
            return;
        }
        at = node.parent;
    }
}

/// Whether `n` is a caption or inside one: it or an element up from it carries data-raea-role="caption".
fn in_caption(doc: &blitz_dom::BaseDocument, n: blitz_dom::NodeId) -> bool {
    up(doc, n, |node| node.attr(LocalName::from("data-raea-role")) == Some("caption"))
}

/// Whether `n` or an element it's in passes `test`.
/// The value of an attribute on n or the nearest element above it that has one.
fn up_attr(doc: &blitz_dom::BaseDocument, n: blitz_dom::NodeId, name: &str) -> Option<String> {
    let mut at = Some(n);
    while let Some(x) = at {
        let node = doc.get_node(x)?;
        if let Some(v) = node.attr(LocalName::from(name)) {
            return Some(v.to_string());
        }
        at = node.parent;
    }
    None
}

fn up(doc: &blitz_dom::BaseDocument, n: blitz_dom::NodeId, test: impl Fn(&blitz_dom::Node) -> bool) -> bool {
    let mut at = Some(n);
    while let Some(x) = at {
        let Some(node) = doc.get_node(x) else { break };
        if test(node) {
            return true;
        }
        at = node.parent;
    }
    false
}

/// An svg's shown <text>s, in document order: each one's words (as [`run_text`] joins its chunks), its box in the
/// svg's canvas, and its opacity times its groups' (from `a`, the svg element's own). One under 0.01 isn't shown.
fn svg_runs(g: &usvg::Group, a: f64) -> Vec<(String, usvg::Rect, f64)> {
    let a = a * g.opacity().get() as f64;
    let mut out = Vec::new();
    if a <= 0.01 {
        return out;
    }
    for c in g.children() {
        match c {
            usvg::Node::Group(g) => out.extend(svg_runs(g, a)),
            usvg::Node::Text(t) => {
                let pieces: Vec<String> = t.chunks().iter().map(|c| c.text().to_string()).collect();
                if let Some(words) = run_text(&pieces) {
                    out.push((words, t.abs_bounding_box(), a));
                }
            }
            _ => {}
        }
    }
    out
}

/// A run's words from its text nodes, in order: joined by a space, as caption words set one a span are; or, when every
/// one is a single letter (a word revealed letter by letter, its spaces spans of their own), as the letters read
/// ("49 BC", not "4 9 B C"). None when the run is only white space.
fn run_text(pieces: &[String]) -> Option<String> {
    let words: Vec<&str> = pieces.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
    if words.is_empty() {
        return None;
    }
    if words.iter().all(|w| w.chars().count() == 1) {
        return Some(pieces.concat().split_whitespace().collect::<Vec<_>>().join(" "));
    }
    Some(words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::run_text;

    #[test]
    fn letters_set_one_a_span_read_as_their_words() {
        let s = |v: &[&str]| v.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        assert_eq!(run_text(&s(&["4", "9", " ", "B", "C"])).as_deref(), Some("49 BC"));
        assert_eq!(run_text(&s(&["won", "a", "civil war,"])).as_deref(), Some("won a civil war,"), "word spans keep a space between");
        assert_eq!(run_text(&s(&[" ", "\n"])), None);
    }
}
