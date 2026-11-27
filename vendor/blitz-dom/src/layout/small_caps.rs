//! raea: `font-variant: small-caps` as Chrome draws it (#249). A face with an `smcp` feature gets it
//! (stylo_to_parley::font_variant_caps); for a face without one (Georgia, Times, Helvetica: most system faces) Chrome
//! synthesises the small capitals, each lower-case letter drawn as its capital at 70% of the font size (Blink's
//! kSmallCapsFontSizeMultiplier). Blitz only asked for the feature, so such text drew in normal case, with no warning.

use std::cell::RefCell;
use std::collections::HashMap;

use blitz_traits::node_id::NodeId;
use parley::fontique::{QueryFont, QueryStatus};
use parley::{FontContext, StyleProperty, TreeBuilder};
use skrifa::MetadataProvider as _;
use skrifa::raw::TableProvider as _;
use style::properties::style_structs::Font as FontStyles;

use crate::font_metrics::query_for_font_styles;
use crate::node::TextBrush;
use crate::stylo_to_parley::stylo::FontVariantCaps;

/// The size of a synthesised small capital, as a fraction of the font size (Blink's)
pub(crate) const SCALE: f32 = 0.7;

thread_local! {
    /// whether a face (its blob's id and index in it) has an `smcp` feature, read once
    static SMCP: RefCell<HashMap<(u64, u32), bool>> = RefCell::new(HashMap::new());
}

/// The elements whose text under `root` (an inline layout's root) is drawn in synthesised small capitals: each text
/// node's parent (its DOM parent, which an anonymous block wrapping the text isn't) whose font-variant-caps is
/// small-caps, in a face whose lower-case letters have no `smcp` feature.
pub(crate) fn synthesized(nodes: &crate::NodeTree, root: NodeId, fcx: &mut FontContext) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let node = &nodes[id];
        if node.is_text_node() {
            let Some(parent) = node.parent.filter(|p| !out.contains(p)) else { continue };
            let Some(style) = nodes[parent].primary_styles() else { continue };
            let font = style.get_font();
            if font.font_variant_caps == FontVariantCaps::SmallCaps && !has_smcp(fcx, font) {
                out.push(parent);
            }
            continue;
        }
        stack.extend(node.children.iter().copied());
        stack.extend(node.before());
        stack.extend(node.after());
    }
    out
}

/// Whether the face `font` draws a lower-case `a` in has an `smcp` feature (true when no face has the letter: then
/// there is nothing to synthesise).
fn has_smcp(fcx: &mut FontContext, font: &FontStyles) -> bool {
    let mut query = query_for_font_styles(fcx, font);
    let mut face = None;
    query.matches_with(|q: &QueryFont| {
        let maps = skrifa::FontRef::from_index(q.blob.as_ref(), q.index).is_ok_and(|f| f.charmap().map('a').is_some());
        if !maps {
            return QueryStatus::Continue;
        }
        face = Some((q.blob.clone(), q.index));
        QueryStatus::Stop
    });
    let Some((blob, index)) = face else { return true };
    SMCP.with(|seen| *seen.borrow_mut().entry((blob.id(), index)).or_insert_with(|| has_feature(blob.as_ref(), index, b"smcp")))
}

/// Whether the face at `index` in `data` lists the GSUB feature `tag`.
pub(crate) fn has_feature(data: &[u8], index: u32, tag: &[u8; 4]) -> bool {
    let tag = skrifa::Tag::new(tag);
    let Ok(font) = skrifa::FontRef::from_index(data, index) else { return false };
    let Ok(list) = font.gsub().and_then(|gsub| gsub.feature_list()) else { return false };
    list.feature_records().iter().any(|record| record.feature_tag() == tag)
}

/// A lower-case letter a synthesised small capital replaces: one with a capital of its own.
fn is_small(c: char) -> bool {
    c.is_lowercase() && !c.to_uppercase().eq(std::iter::once(c))
}

/// `text` pushed with each run of lower-case letters as their capitals at SCALE of `font_size` (px), the rest as it is.
pub(crate) fn push(builder: &mut TreeBuilder<TextBrush>, text: &str, font_size: f32) {
    let mut rest = text;
    while let Some(first) = rest.chars().next() {
        let small = is_small(first);
        let end = rest.char_indices().find(|&(_, c)| is_small(c) != small).map_or(rest.len(), |(i, _)| i);
        let (run, tail) = rest.split_at(end);
        if small {
            builder.push_style_modification_span(&[StyleProperty::FontSize(font_size * SCALE)]);
            builder.push_text(&run.to_uppercase());
            builder.pop_style_span();
        } else {
            builder.push_text(run);
        }
        rest = tail;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parley::{LayoutContext, TextStyle};

    /// Each glyph run's text and font size, for `text` pushed as small capitals at 40 px.
    fn runs(text: &str) -> Vec<(String, f32)> {
        let mut font_ctx = FontContext::new();
        let mut layout_ctx: LayoutContext<TextBrush> = LayoutContext::new();
        let style = TextStyle { font_size: 40.0, ..TextStyle::default() };
        let mut builder = layout_ctx.tree_builder(&mut font_ctx, 1.0, true, &style);
        push(&mut builder, text, 40.0);
        let mut layout = parley::Layout::new();
        let built = builder.build_into(&mut layout);
        layout.break_all_lines(None);
        let mut out = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                if let parley::PositionedLayoutItem::GlyphRun(run) = item {
                    out.push((built[run.run().text_range()].to_string(), run.run().font_size()));
                }
            }
        }
        out
    }

    #[test]
    fn lower_case_letters_are_capitals_at_70_percent() {
        let want = [("C", 40.0), ("OMPLIANCE", 28.0), (" A", 40.0), ("UDIT", 28.0)];
        let want: Vec<(String, f32)> = want.iter().map(|(t, s)| (t.to_string(), *s)).collect();
        assert_eq!(runs("Compliance Audit"), want);
    }

    #[test]
    fn capitals_digits_and_spaces_keep_their_size() {
        assert_eq!(runs("A1 B"), vec![("A1 B".to_string(), 40.0)]);
    }

    #[test]
    fn a_letter_without_a_capital_of_its_own_keeps_its_size() {
        // U+0138 kra is lower-case and has no capital
        assert!(!is_small('ĸ') && is_small('a') && !is_small('A') && !is_small('1'));
    }
}
