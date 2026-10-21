//! (raea) Compositor-only motion: an absolutely positioned box that moves by its `left`/`top` alone is moved in its
//! laid-out tree where it stands, without a restyle or a relayout. Its new position is the one Taffy's out-of-flow
//! layout gives it (`imcb.start + margin.start + area_offset`, the inset an absolute length, the other inset auto, its
//! size and margins absolute lengths, so nothing else about it or around it changes), and its subtree is rounded as
//! the layout pass rounds it. raea's Dom moves such boxes after each layout pass; `move_out_of_flow` returning false
//! means the box isn't one this moves exactly, and the caller writes its style instead.

use taffy::{CoreStyle as _, MaybeResolve as _};

use blitz_traits::node_id::NodeId;

use crate::BaseDocument;
use crate::layout::{LayoutPassState, round_layout_subtree, step_for_rounding};

/// A CSS length's px as Stylo computes it from `text` (`12.5px`, `0`): the same tokenizer, so the same f32. None for
/// anything else.
pub fn css_px(text: &str) -> Option<f32> {
    let mut p = cssparser::Parser::new(text);
    let v = match p.next().ok()? {
        cssparser::Token::Dimension { value, unit, .. } if unit.eq_ignore_ascii_case("px") => *value,
        cssparser::Token::Number { value, .. } if *value == 0.0 => 0.0,
        _ => return None,
    };
    p.is_exhausted().then_some(v)
}

/// What the caller keeps for a moved box between layout passes: where this last put it, and its containing block's
/// offset then (a layout pass that left it there keeps it; one that laid it out again from its stale style didn't)
#[derive(Clone, Copy, Debug, Default)]
pub struct MoveState {
    set: Option<taffy::Point<f32>>,
    area: taffy::Point<f32>,
}

impl BaseDocument {
    fn mark_all_overflow(&mut self) {
        let ov = style::selector_parser::RestyleDamage::RECALCULATE_OVERFLOW;
        for (_, node) in self.nodes.iter_mut() {
            node.insert_damage(ov);
        }
    }

    /// The out-of-flow box `id` placed at `left`/`top` px (None: that axis as it is). False, with nothing changed,
    /// when it isn't a box this moves exactly.
    pub fn move_out_of_flow(&mut self, id: NodeId, left: Option<f32>, top: Option<f32>, state: &mut MoveState) -> bool {
        let Some(node) = self.nodes.get(id) else { return false };
        if !node.is_out_of_flow() || node.primary_styles().is_none() {
            return false;
        }
        let Some(cb) = node.containing_block() else { return false };
        // a scroll container's extents follow its boxes' positions (Taffy's overflow rect): laid out, not moved
        if self.nodes.get(cb).and_then(|c| c.primary_styles()).is_none_or(|s| {
            let b = s.get_box();
            b.overflow_x != style::values::computed::Overflow::Visible || b.overflow_y != style::values::computed::Overflow::Visible
        }) {
            return false;
        }
        let style = node.layout_style();
        let (inset, size, margin) = (style.inset(), style.size(), style.margin());
        let calc = |_: *const (), _: f32| f32::NAN;
        let u = *node.unrounded_layout();
        let fresh = state.set != Some(u.location);
        let mut loc = u.location;
        let mut area = state.area;
        // an axis: the inset an absolute length (the one Taffy used, from the style as written last), the other inset
        // auto, size and margins absolute lengths
        let axis = |new: Option<f32>, start: taffy::LengthPercentageAuto, end: taffy::LengthPercentageAuto, size: taffy::Dimension,
                    m0: taffy::LengthPercentageAuto, m1: taffy::LengthPercentageAuto, m: f32, at: f32, area: &mut f32| -> Option<f32> {
            let Some(new) = new else { return Some(at) };
            let old = start.maybe_resolve(None, calc)?;
            if !end.is_auto() || size.maybe_resolve(None, calc).is_none() || m0.maybe_resolve(None, calc).is_none() || m1.maybe_resolve(None, calc).is_none() {
                return None;
            }
            if fresh {
                // laid out from the written style: its containing block's offset is what's left of the position
                let a = at - (old + m);
                if (old + m) + a != at {
                    return None;
                }
                *area = a;
            }
            Some((new + m) + *area)
        };
        let Some(x) = axis(left, inset.left, inset.right, size.width, margin.left, margin.right, u.margin.left, u.location.x, &mut area.x) else { return false };
        let Some(y) = axis(top, inset.top, inset.bottom, size.height, margin.top, margin.bottom, u.margin.top, u.location.y, &mut area.y) else { return false };
        loc.x = x;
        loc.y = y;
        // its rounding parent's cumulative position (the chain the layout pass rounds along: an out-of-flow box under
        // its containing block, any other under its layout parent), summed from the root down as the pass sums it
        let root = self.root_element().id;
        let mut chain = [root; 64];
        let mut n = 0usize;
        let mut at = self.nodes[id].containing_block();
        while let Some(p) = at {
            if n == chain.len() {
                return false;
            }
            chain[n] = p;
            n += 1;
            if p == root {
                break;
            }
            let pn = &self.nodes[p];
            at = if pn.is_out_of_flow() { pn.containing_block() } else { pn.layout_parent.get() };
        }
        if n == 0 || chain[n - 1] != root {
            return false;
        }
        let (mut cx, mut cy) = (0.0f32, 0.0f32);
        for &p in chain[..n].iter().rev() {
            let l = self.nodes[p].unrounded_layout().location;
            cx += l.x;
            cy += l.y;
        }
        drop(style);
        self.nodes[id].unrounded_layout_mut().location = loc;
        state.set = Some(loc);
        state.area = area;
        let mut pass = LayoutPassState::new(self);
        round_layout_subtree(&mut pass, crate::taffy_node_id(id), cx, cy, step_for_rounding());
        // the paint overflow (resolve_transforms) recomputed for the box, its subtree (rounding at its new position can
        // change their sizes) and its containing blocks
        let ov = style::selector_parser::RestyleDamage::RECALCULATE_OVERFLOW;
        for &p in &chain[..n] {
            self.nodes[p].insert_damage(ov);
        }
        let mut stack = [id; 64];
        let mut top = 1usize;
        while top > 0 {
            top -= 1;
            let x = stack[top];
            self.nodes[x].insert_damage(ov);
            let node = &self.nodes[x];
            let kids = node.layout_children.borrow();
            let hoisted = node.hoisted_children.borrow();
            for &c in kids.iter().flat_map(|k| k.iter()).chain(hoisted.iter()) {
                if top == stack.len() {
                    // deeper than this walks: every node's overflow, as a layout pass would
                    drop(kids);
                    drop(hoisted);
                    self.mark_all_overflow();
                    top = 0;
                    break;
                }
                stack[top] = c;
                top += 1;
            }
        }
        self.resolve_transforms(root);
        self.nodes.bump_geometry_generation();
        true
    }
}
