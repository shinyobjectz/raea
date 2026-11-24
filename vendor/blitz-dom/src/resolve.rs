//! Resolve style and layout

use blitz_traits::node_id::NodeId;
use std::cell::RefCell;

use debug_timer::debug_timer;
use kurbo::{Affine, Rect};
use parley::LayoutContext;
use selectors::Element as _;
use style::dom::TDocument;

#[cfg(feature = "parallel-construct")]
use rayon::prelude::*;

// FIXME: static thread_local FontCtx isn't necessarily correct in multi-document context.
// Should use thread_local crate with ThreadLocal value store in the Document.
thread_local! {
    pub(crate) static LAYOUT_CTX: RefCell<Option<Box<LayoutContext<TextBrush>>>> = const { RefCell::new(None) };
}

use taffy::AvailableSpace;

use crate::{
    BaseDocument,
    layout::{
        LayoutPassState,
        construct::{
            ConstructionTask, ConstructionTaskData, ConstructionTaskResult,
            ConstructionTaskResultData, LayoutChildren, build_inline_layout_into,
            collect_layout_children,
        },
        damage::{ALL_DAMAGE, CONSTRUCT_BOX, CONSTRUCT_DESCENDENT, CONSTRUCT_FC},
    },
    node::TextBrush,
};

impl BaseDocument {
    /// raea: the CSS animations of node `node` (and of its ::before and ::after) begun `by` seconds later than the
    /// resolve that started them. A page that meets several elements at once, each of which began at its own time (a
    /// frame rendered alone: the composition's start, a <Sequence>'s), resolves them all at the earliest and moves each
    /// later one's animations to its own start.
    pub fn delay_animations(&mut self, node: NodeId, by: f64) {
        let mut sets = self.animations.sets.write();
        for (_, set) in sets.iter_mut().filter(|(key, _)| NodeId::from_u64(key.node.id() as u64) == node) {
            for animation in set.animations.iter_mut() {
                animation.started_at += by;
            }
            set.dirty = true;
        }
    }

    /// Restyle the tree and then relayout it
    pub fn resolve(&mut self, current_time_for_animations: f64) {
        if TDocument::as_node(&self.root_node())
            .first_element_child()
            .is_none()
        {
            #[cfg(feature = "tracing")]
            tracing::warn!("No DOM - not resolving");
            return;
        }

        // Process messages that have been sent to our message channel (e.g. loaded resource)
        self.handle_messages();

        // While render-blocking resources (e.g. stylesheets linked from the `<head>`) are
        // still loading, don't resolve styles or layout (matching how browsers block
        // rendering). Resolving styles before the document's stylesheets have loaded would
        // give elements computed styles based on an incomplete cascade, and a later restyle
        // (once the stylesheet loads) would treat those as genuine "before-change styles",
        // spuriously starting CSS transitions from unstyled values. See issue #689.
        //
        // `handle_messages` above must still run so that loaded resources are ingested and
        // this state can clear.
        if self.has_pending_critical_resources() {
            return;
        }

        self.resolve_scroll_animation();

        // Drop scrollbar-activity entries whose fade-out has finished (also
        // sheds entries for removed nodes).
        {
            use crate::node::scrollbar::{FADE_DELAY, FADE_DURATION};
            self.scrollbar_activity
                .retain(|_, last| last.elapsed() < FADE_DELAY + FADE_DURATION);
        }

        let root_node_id = self.root_element().id;
        debug_timer!(timer, feature = "log-phase-times");

        // Apply any device changes (viewport resize, zoom, color-scheme, etc)
        // accumulated since the last resolve as a single device rebuild.
        self.flush_pending_device_changes();

        // we need to resolve stylist first since it will need to drive our layout bits
        self.resolve_stylist(current_time_for_animations);
        timer.record_time("style");

        // Non-incremental mode is "every node is damaged every frame": mark the
        // whole tree and let the same damage pipeline reconstruct and relayout
        // everything.
        if !self.incremental_layout {
            self.mark_all_damaged();
            timer.record_time("mark_all");
        }

        // Propagate damage flags (from mutation and restyles) up and down the tree
        self.propagate_damage_flags(root_node_id);
        timer.record_time("damage");

        // Fix up tree for layout (insert anonymous blocks as necessary, etc)
        self.resolve_layout_children();
        #[cfg(debug_assertions)]
        self.assert_layout_parents_consistent();
        timer.record_time("construct");

        self.resolve_deferred_tasks();
        // Flush background/mask images from style to dedicated storage on the
        // nodes whose style changed (queued by the style traversal and by
        // pseudo-element box construction), fetching any not-yet-loaded images.
        self.flush_pending_style_images();
        timer.record_time("pconstruct");

        // Next we resolve layout with the data resolved by stlist
        self.resolve_layout();
        timer.record_time("layout");
        self.nodes.bump_geometry_generation();

        // raea: query containers (css-contain-3): a size container whose content box changed in this layout has its
        // descendants restyled against the new size (cq units, @container) and laid out again, until no container's
        // size moves (a few passes at most: a container nested in another settles after it)
        for _ in 0..4 {
            if !self.update_container_sizes() {
                break;
            }
            self.resolve_stylist(current_time_for_animations);
            if !self.incremental_layout {
                self.mark_all_damaged();
            }
            self.propagate_damage_flags(root_node_id);
            self.resolve_layout_children();
            self.resolve_deferred_tasks();
            self.flush_pending_style_images();
            self.resolve_layout();
            self.nodes.bump_geometry_generation();
            timer.record_time("containers");
        }

        // raea: scrollers asked to their end, now that layout has their content
        for id in std::mem::take(&mut self.scroll_to_end) {
            if let Some(node) = self.nodes.get_mut(id) {
                let end = (node.scroll_height() - node.client_height()).max(0.0) as f64;
                node.scroll_offset_mut().y = end;
            }
        }
        // raea: scroll offsets the page set kept in range and bevy_ui Scrollbar thumbs placed, now that layout has the
        // content (bevy_scroll.rs)
        self.resolve_bevy_scroll();
        // raea: bevy_ui_widgets Popovers placed beside their parents (bevy_popover.rs)
        self.resolve_bevy_popovers();

        // Resolve transforms
        self.resolve_transforms(root_node_id, None, None);
        timer.record_time("transform");

        // Build paint children / stacking contexts for damaged subtrees. Runs
        // after layout and transforms so that stacking-context roots created by
        // this frame's transforms are picked up, and before damage is cleared.
        self.build_paint_tree(root_node_id);
        timer.record_time("paint_tree");

        // Clear all damage and dirty flags, walking only subtrees which are
        // marked as (potentially) containing damage.
        let doc_node_id = self.root_node().id;
        self.clear_damage_and_dirty_flags(doc_node_id);
        timer.record_time("c_damage");

        // Re-resolve the hover node from the pointer position against the fresh
        // layout. This must run *after* the damage/dirty flags are cleared
        // above, so that the restyle hint and ancestor `dirty_descendants`
        // flags set by any resulting hover change survive into the next resolve
        // pass (the clearing loop would otherwise wipe them). Any resulting
        // restyle is picked up on the next resolve pass; a redraw is requested
        // if the hovered node actually changes.
        self.refresh_hover();

        let mut subdoc_is_animating = false;
        for &node_id in &self.sub_document_nodes {
            let node = &mut self.nodes[node_id];
            let size = node.final_layout().size;
            if let Some(mut sub_doc) = node.subdoc_mut().map(|doc| doc.inner_mut()) {
                // Set viewport
                // viewport_mut handles change detection. So we just unconditionally set the values;
                let mut sub_viewport = sub_doc.viewport_mut();
                sub_viewport.hidpi_scale = self.viewport.hidpi_scale;
                sub_viewport.zoom = self.viewport.zoom;
                sub_viewport.color_scheme = self.viewport.color_scheme;

                let viewport_scale = self.viewport.scale();
                sub_viewport.window_size = (
                    (size.width * viewport_scale) as u32,
                    (size.height * viewport_scale) as u32,
                );
                drop(sub_viewport);

                sub_doc.resolve(current_time_for_animations);

                subdoc_is_animating |= sub_doc.is_animating();
            }
        }
        self.subdoc_is_animating = subdoc_is_animating;
        timer.record_time("subdocs");

        timer.print_times(&format!("Resolve({}): ", self.id()));
    }

    /// raea: each size container's content box from the layout just done, stored where its descendants' styles
    /// read it (query_container_size) and those descendants marked for restyle when it changed. Whether any did.
    fn update_container_sizes(&mut self) -> bool {
        let mut changed: Vec<(NodeId, (f32, f32))> = Vec::new();
        for (id, node) in self.nodes.iter() {
            let Some(el) = node.element_data() else { continue };
            let Some(styles) = node.primary_styles() else { continue };
            if !styles.get_box().clone_container_type().is_size_container_type() {
                continue;
            }
            let Some(l) = el.layout_data.as_deref().map(|d| &d.unrounded_layout) else { continue };
            let w = (l.size.width - l.padding.left - l.padding.right - l.border.left - l.border.right).max(0.0);
            let h = (l.size.height - l.padding.top - l.padding.bottom - l.border.top - l.border.bottom).max(0.0);
            if el.container_size != Some((w, h)) {
                changed.push((id, (w, h)));
            }
        }
        for &(id, size) in &changed {
            let node = &mut self.nodes[id];
            if let Some(el) = node.element_data_mut() {
                el.container_size = Some(size);
            }
            node.set_restyle_hint(style::invalidation::element::restyle_hints::RestyleHint::RESTYLE_DESCENDANTS);
            node.set_dirty_descendants();
        }
        !changed.is_empty()
    }

    /// raea: `perspective` is the parent's and `context` its matrix from its 3D rendering context's root's parent, when
    /// it is `transform-style: preserve-3d` (B3 H5's 3D transforms).
    fn resolve_transforms(
        &mut self,
        node_id: NodeId,
        perspective: Option<crate::stylo_to_kurbo::Mat4>,
        context: Option<crate::stylo_to_kurbo::Mat4>,
    ) -> Rect {
        if !self.nodes.contains_key(node_id) {
            return Rect::ZERO;
        }

        let scale = self.viewport.scale_f64();

        // raea: a child seen through its parent's perspective is placed again whenever the parent is, damaged or not:
        // the perspective moves with the parent's size and scroll (perspective-origin-scroll), not the child's. So is
        // one in a 3D rendering context, whose projection is its context's matrix times its own (a script setting the
        // preserve-3d parent's transform left its children at their old projection, #19), and one with a projection
        // now, which may have lost its context since
        // (a bevy_ui IgnoreScroll node is placed again too: its parent's scroll moves it)
        if perspective.is_none()
            && context.is_none()
            && !self.projections.contains_key(&node_id)
            && self.nodes[node_id].bevy_ignore_scroll().is_none()
            && !self.nodes[node_id]
                .damage()
                .map(|d| d.contains(style::selector_parser::RestyleDamage::RECALCULATE_OVERFLOW))
                .unwrap_or(false)
        {
            let node = &self.nodes[node_id];
            let location = node.final_layout().location.map(|v| v as f64 * scale);

            let mut transform = Affine::translate((location.x, location.y));
            if let Some(t) = node.transform().as_deref() {
                transform *= *t
            }

            let overflow = *node.scrollable_overflow();
            return transform.transform_rect_bbox(overflow);
        }

        let (mut transform, projection, context) =
            self.nodes[node_id].set_transform(scale as f32, perspective.as_ref(), context.as_ref());
        // raea: a bevy_ui IgnoreScroll node keeps its place on its axes while its parent scrolls (bevy_ui's layout leaves
        // the parent's scroll out of its position): moved back by that scroll, after its own transform
        if let Some((ix, iy)) = self.nodes[node_id].bevy_ignore_scroll()
            && let Some(parent) = self.nodes[node_id].parent
        {
            let s = *self.nodes[parent].scroll_offset();
            let (dx, dy) = (if ix { s.x } else { 0.0 }, if iy { s.y } else { 0.0 });
            if dx != 0.0 || dy != 0.0 {
                let t = Affine::translate((dx * scale, dy * scale)) * transform.unwrap_or(Affine::IDENTITY);
                *self.nodes[node_id].transform_mut() = Some(Box::new(t));
                transform = Some(t);
            }
        }
        match projection {
            Some(p) => {
                self.projections.insert(node_id, p);
            }
            None => {
                self.projections.remove(&node_id);
            }
        }
        // what this element's children are seen through
        let perspective = {
            let node = &self.nodes[node_id];
            let size = node.final_layout().size;
            let scroll = node.scroll_offset();
            node.primary_styles().and_then(|s| crate::stylo_to_kurbo::resolve_perspective(s.get_box(), (size.width, size.height), (scroll.x, scroll.y)))
        };

        let w = self.nodes[node_id].final_layout().size.width as f64 * scale;
        let h = self.nodes[node_id].final_layout().size.height as f64 * scale;
        let mut overflow = Rect::new(0.0, 0.0, w, h);

        let layout_children = std::mem::take(self.nodes[node_id].layout_children.get_mut());

        if let Some(ref children) = layout_children {
            for &child_id in children {
                // Out-of-flow children are laid out relative to their containing
                // block, not their DOM parent: they are visited (and their overflow
                // accounted for) via the containing block's hoisted list below.
                if self.nodes[child_id].is_out_of_flow() {
                    continue;
                }
                // a layout child that isn't a DOM child is in this element's context through the parents between that
                // make no box of their own and pass it on: `display: contents` (transform3d-preserve3d-014), and a
                // table's tbody that preserves 3D with no transform (transform-table-009); else it's outside it
                let passes = |n: &crate::Node| {
                    n.display_style().is_some_and(|d| d.inside() == style::values::specified::box_::DisplayInside::Contents)
                        || n.primary_styles().is_some_and(|s| crate::stylo_to_kurbo::preserves_3d(&s) && !crate::stylo_to_kurbo::has_transform(s.get_box()))
                };
                let mut parent = self.nodes[child_id].parent;
                while let Some(p) = parent.filter(|p| *p != node_id && passes(&self.nodes[*p])) {
                    parent = self.nodes[p].parent;
                }
                let ours = parent == Some(node_id);
                let child_rect_in_self = self.resolve_transforms(child_id, perspective, context.filter(|_| ours));
                overflow = overflow.union(child_rect_in_self);
            }
        }
        let hoisted_children =
            std::mem::take(&mut *self.nodes[node_id].hoisted_children.borrow_mut());
        for &child_id in &hoisted_children {
            if !self.nodes.contains_key(child_id) {
                continue;
            }
            // an out-of-flow box placed here is seen through this element's perspective and context only if it is
            // also its child (Chrome's perspective-children-only-abspos)
            let ours = self.nodes[child_id].parent == Some(node_id);
            let child_rect_in_self = self.resolve_transforms(child_id, perspective.filter(|_| ours), context.filter(|_| ours));
            overflow = overflow.union(child_rect_in_self);
        }
        *self.nodes[node_id].hoisted_children.borrow_mut() = hoisted_children;
        for pseudo in [self.nodes[node_id].before(), self.nodes[node_id].after()] {
            let Some(pseudo) = pseudo else { continue };
            if self.nodes[pseudo].is_out_of_flow() {
                continue;
            }
            let child_rect_in_self = self.resolve_transforms(pseudo, perspective, context);
            overflow = overflow.union(child_rect_in_self);
        }

        *self.nodes[node_id].scrollable_overflow_mut() = overflow;
        *self.nodes[node_id].layout_children.get_mut() = layout_children;

        let scaled_x = self.nodes[node_id].final_layout().location.x as f64 * scale;
        let scaled_y = self.nodes[node_id].final_layout().location.y as f64 * scale;

        let full = if let Some(t) = transform {
            Affine::translate((scaled_x, scaled_y)) * t
        } else {
            Affine::translate((scaled_x, scaled_y))
        };

        full.transform_rect_bbox(overflow)
    }

    /// Ensure that the layout_children field is populated for all nodes
    pub fn resolve_layout_children(&mut self) {
        resolve_layout_children_recursive(self, self.root_node().id);

        fn resolve_layout_children_recursive(doc: &mut BaseDocument, node_id: NodeId) {
            // Anonymous blocks and pseudo-elements can be removed from the slab
            // between render passes. Bail out rather than panicking on a stale key.
            if doc.nodes.get(node_id).is_none() {
                return;
            }

            let mut damage = doc.nodes[node_id].damage().unwrap_or(ALL_DAMAGE);
            let _flags = doc.nodes[node_id].flags;

            if damage.intersects(CONSTRUCT_FC | CONSTRUCT_BOX) {
                //} || flags.contains(NodeFlags::IS_INLINE_ROOT) {

                // Deallocate the anonymous blocks created for this node in the
                // previous construction round. They live only in the slab, so
                // reconstructing without freeing them would leak a slab entry per
                // anonymous block per reconstruction.
                let old_anonymous_blocks = std::mem::take(&mut doc.nodes[node_id].anonymous_blocks);
                for anon_id in old_anonymous_blocks {
                    doc.deallocate_anonymous_block(anon_id);
                }

                let mut collected = LayoutChildren::default();
                collect_layout_children(doc, node_id, &mut collected);
                let layout_children = collected.children;
                doc.nodes[node_id].anonymous_blocks = collected.anonymous_blocks;

                // Recurse into newly collected layout children
                for child_id in layout_children.iter().copied() {
                    resolve_layout_children_recursive(doc, child_id);
                    doc.nodes[child_id].layout_parent.set(Some(node_id));
                    if let Some(mut data) = doc.nodes[child_id]
                        .try_stylo_element_data_mut()
                        .and_then(|s| s.get_mut())
                    {
                        data.damage
                            .remove(CONSTRUCT_DESCENDENT | CONSTRUCT_FC | CONSTRUCT_BOX);
                    }
                }

                *doc.nodes[node_id].layout_children.borrow_mut() = Some(layout_children.clone());
                // *doc.nodes[node_id].paint_children.borrow_mut() = Some(layout_children);

                damage.remove(CONSTRUCT_DESCENDENT | CONSTRUCT_FC | CONSTRUCT_BOX);
                // damage.insert(RestyleDamage::RELAYOUT | RestyleDamage::REPAINT);
            } else {
                //if damage.contains(CONSTRUCT_DESCENDENT) {
                let layout_children = doc.nodes[node_id].layout_children.borrow_mut().take();
                if let Some(layout_children) = layout_children {
                    for child_id in layout_children.iter().copied() {
                        // Anonymous blocks and pseudo-elements can be removed from the
                        // slab between render passes; skip stale IDs.
                        if !doc.nodes.contains_key(child_id) {
                            continue;
                        }
                        resolve_layout_children_recursive(doc, child_id);
                        doc.nodes[child_id].layout_parent.set(Some(node_id));
                    }

                    *doc.nodes[node_id].layout_children.borrow_mut() = Some(layout_children);
                }

                // damage.remove(CONSTRUCT_DESCENDENT);
                // damage.insert(RestyleDamage::RELAYOUT | RestyleDamage::REPAINT);
            }

            doc.nodes[node_id].set_damage(damage);
        }
    }

    /// Every layout child must point back at its container via
    /// `layout_parent`, which `propagate_damage_flags` relies on to reach
    /// anonymous boxes.
    #[cfg(debug_assertions)]
    fn assert_layout_parents_consistent(&self) {
        use crate::node::NodeFlags;

        for (parent_id, node) in self.nodes.iter() {
            if !node.flags.contains(NodeFlags::IS_IN_DOCUMENT) {
                continue;
            }
            let Some(children) = node.layout_children.borrow().clone() else {
                continue;
            };
            for child_id in children {
                let Some(child) = self.nodes.get(child_id) else {
                    panic!("layout child {child_id:?} of {parent_id:?} is not in the slab");
                };
                debug_assert_eq!(
                    child.layout_parent.get(),
                    Some(parent_id),
                    "layout_parent of {child_id:?} does not point at its layout container {parent_id:?}"
                );
            }
        }
    }

    pub fn resolve_deferred_tasks(&mut self) {
        let mut deferred_construction_nodes = std::mem::take(&mut self.deferred_construction_nodes);

        // Deduplicate deferred tasks by node_id to avoid redundant work
        deferred_construction_nodes.sort_unstable_by_key(|task| task.node_id);
        deferred_construction_nodes.dedup_by_key(|task| task.node_id);

        #[cfg(feature = "parallel-construct")]
        let iter = deferred_construction_nodes.into_par_iter();
        #[cfg(not(feature = "parallel-construct"))]
        let iter = deferred_construction_nodes.into_iter();

        let results: Vec<ConstructionTaskResult> = iter
            .map(|task: ConstructionTask| match task.data {
                ConstructionTaskData::InlineLayout(mut layout) => {
                    #[cfg(feature = "parallel-construct")]
                    let mut layout_ctx = LAYOUT_CTX
                        .take()
                        .unwrap_or_else(|| Box::new(LayoutContext::new()));
                    #[cfg(feature = "parallel-construct")]
                    let layout_ctx_mut = &mut layout_ctx;

                    #[cfg(feature = "parallel-construct")]
                    let mut font_ctx = self
                        .thread_font_contexts
                        .get_or(|| RefCell::new(Box::new(self.font_ctx.lock().unwrap().clone())))
                        .borrow_mut();
                    #[cfg(feature = "parallel-construct")]
                    let font_ctx_mut = &mut *font_ctx;

                    #[cfg(not(feature = "parallel-construct"))]
                    let layout_ctx_mut = &mut self.layout_ctx;
                    #[cfg(not(feature = "parallel-construct"))]
                    let font_ctx_mut = &mut *self.font_ctx.lock().unwrap();

                    layout.content_widths = None;
                    build_inline_layout_into(
                        &self.nodes,
                        layout_ctx_mut,
                        font_ctx_mut,
                        &mut layout,
                        self.viewport.scale(),
                        task.node_id,
                    );

                    #[cfg(feature = "parallel-construct")]
                    {
                        LAYOUT_CTX.set(Some(layout_ctx));
                    }

                    // If layout doesn't contain any inline boxes, then it is safe to populate the content_widths
                    // cache during this parallelized stage.
                    // if layout.layout.inline_boxes().is_empty() {
                    //     layout.content_widths();
                    // }

                    ConstructionTaskResult {
                        node_id: task.node_id,
                        data: ConstructionTaskResultData::InlineLayout(layout),
                    }
                }
            })
            .collect();

        for result in results {
            match result.data {
                ConstructionTaskResultData::InlineLayout(layout) => {
                    self.nodes[result.node_id].clear_layout_cache();
                    self.nodes[result.node_id]
                        .element_data_mut()
                        .unwrap()
                        .inline_layout_data = Some(layout);
                }
            }
        }

        self.deferred_construction_nodes.clear();
    }

    /// Walk the nodes now that they're properly styled and transfer their styles to the taffy style system
    ///
    /// TODO: update taffy to use an associated type instead of slab key
    /// TODO: update taffy to support traited styles so we don't even need to rely on taffy for storage
    pub fn resolve_layout(&mut self) {
        let size = self.stylist.device().au_viewport_size();

        let available_space = taffy::Size {
            width: AvailableSpace::Definite(size.width.to_f32_px()),
            height: AvailableSpace::Definite(size.height.to_f32_px()),
        };

        let root_element_id = crate::taffy_node_id(self.root_element().id);

        // println!("\n\nRESOLVE LAYOUT\n===========\n");

        let mut state = LayoutPassState::new(self);
        #[cfg(feature = "writing-mode")]
        let available_space = {
            state.layout_wm = state.layout_wm_of(crate::dom_node_id(root_element_id));
            if state.layout_wm.is_vertical() {
                available_space.transpose()
            } else {
                available_space
            }
        };
        taffy::compute_root_layout(&mut state, root_element_id, available_space);
        #[cfg(feature = "writing-mode")]
        state.physicalise_and_round_layout(root_element_id);
        #[cfg(not(feature = "writing-mode"))]
        taffy::round_layout(&mut state, root_element_id);

        // println!("\n\n");
        // taffy::print_tree(self, root_node_id)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Attribute, BaseDocument, DocumentConfig, qual_name};
    use blitz_traits::shell::{ColorScheme, Viewport};

    /// `#root` (perspective) > `#world` (preserve-3d, `world` its style) > `#card`: the document and the card
    fn rig(world: &str) -> (BaseDocument, crate::NodeId, crate::NodeId) {
        let mut doc = BaseDocument::new(DocumentConfig { viewport: Some(Viewport::new(1920, 1080, 1.0, ColorScheme::Light)), ..Default::default() });
        let style = |v: &str| vec![Attribute { name: qual_name!("style"), value: v.to_string() }];
        let root_id = doc.root_node().id;
        let mut m = doc.mutate();
        let root = m.create_element(qual_name!("div"), style("position:relative;width:1920px;height:1080px;perspective:1000px"));
        let w = m.create_element(qual_name!("div"), style(&format!("position:absolute;inset:0;transform-style:preserve-3d;{world}")));
        let card = m.create_element(qual_name!("div"), style("position:absolute;left:600px;top:1200px;width:700px;height:427px"));
        m.append_children(w, &[card]);
        m.append_children(root, &[w]);
        m.append_children(root_id, &[root]);
        drop(m);
        doc.resolve(0.0);
        (doc, w, card)
    }

    const MOVE: &str = "translate3d(0px,-1000px,100px) rotateX(11deg)";

    /// raea #19: a preserve-3d parent's transform set after the first resolve (as a script sets it) places its children
    /// as the same transform in its first style does; they kept their old projection, and the card was drawn off-frame
    #[test]
    fn a_child_follows_its_preserve_3d_parent_set_later() {
        let (styled, _, card) = rig(&format!("transform:{MOVE}"));
        let want = styled.projections.get(&card).copied();
        assert!(want.is_some(), "the card has a projection in its parent's 3D context");
        let (mut doc, world, card) = rig("");
        doc.mutate().set_style_property(world, "transform", MOVE);
        doc.resolve(0.0);
        assert_eq!(doc.projections.get(&card).copied(), want);
    }

    /// raea: getBoundingClientRect through a perspective is the box of the four corners as drawn (layout-reads LR10:
    /// snapcn's card-rail read every turned card at its flat size). Chrome 's numbers, for a 400x200 card at 300,400
    /// under a 1000px perspective, turned rotateY(60deg): flat, and in a preserve-3d world turned rotateX(10deg).
    #[test]
    fn a_rect_under_a_perspective_is_its_corners_as_drawn() {
        let rect = |world: Option<&str>| {
            let mut doc = BaseDocument::new(DocumentConfig { viewport: Some(Viewport::new(1000, 1000, 1.0, ColorScheme::Light)), ..Default::default() });
            let style = |v: &str| vec![Attribute { name: qual_name!("style"), value: v.to_string() }];
            let root_id = doc.root_node().id;
            let mut m = doc.mutate();
            let root = m.create_element(qual_name!("div"), style("position:relative;width:1000px;height:1000px;perspective:1000px"));
            let card = m.create_element(qual_name!("div"), style("position:absolute;left:300px;top:400px;width:400px;height:200px;transform:rotateY(60deg)"));
            if let Some(w) = world {
                let w = m.create_element(qual_name!("div"), style(&format!("position:absolute;inset:0;transform-style:preserve-3d;transform:{w}")));
                m.append_children(w, &[card]);
                m.append_children(root, &[w]);
            } else {
                m.append_children(root, &[card]);
            }
            m.append_children(root_id, &[root]);
            drop(m);
            doc.resolve(0.0);
            let r = doc.get_client_bounding_rect(card).unwrap();
            [r.x, r.y, r.width, r.height]
        };
        let near = |got: [f64; 4], want: [f64; 4]| assert!(got.iter().zip(want).all(|(g, w)| (g - w).abs() < 0.05), "{got:?}, Chrome {want:?}");
        near(rect(None), [379.05, 379.05, 206.19, 241.90]);
        near(rect(Some("rotateX(10deg)")), [376.86, 348.18, 209.86, 263.30]);
    }
}
