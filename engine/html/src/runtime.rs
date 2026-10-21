// Portions ported from HyperFrames (Apache-2.0, https://github.com/heygen-com/hyperframes); see NOTICE.
//! The parts of the the HTML composition format runtime a render depends on, ported from
//! packages/core/src/runtime (startResolver.ts, timeline.ts, clipWindow.ts, init.ts):
//! `data-start` expressions (absolute, `id`, `id + 2`, `id - 0.5`) offset by the host
//! composition, durations from data-duration / data-end / media / sub-timelines, frame-snapped
//! clip windows, the ancestor chain rule, and display:none for hidden in-flow leaf clips.

use crate::compile::{all_elements, attr, has_attr};
use blitz_dom::{BaseDocument, NodeId};
use std::collections::HashMap;

const FRAME_BOUNDARY_EPSILON: f64 = 1e-3;
const TERMINAL_EPSILON_SECONDS: f64 = 1e-6;

fn instant_tolerance(t: f64) -> f64 {
    4.0 * f64::EPSILON * t.abs().max(1.0)
}

fn same_instant(a: f64, b: f64) -> bool {
    if !a.is_finite() || !b.is_finite() {
        return a == b;
    }
    (a - b).abs() <= instant_tolerance(a.abs().max(b.abs()))
}

pub fn snap_to_frame(t: f64, fps: f64) -> f64 {
    let grid = if fps.is_finite() && fps > 0.0 { fps } else { 30.0 };
    let pos = t * grid;
    let nearest = pos.round();
    if (pos - nearest).abs() <= FRAME_BOUNDARY_EPSILON { nearest / grid } else { t }
}

fn clip_visible_at(t: f64, start: f64, end: f64, comp_duration: f64) -> bool {
    let started = t >= start || same_instant(t, start);
    let in_window = started && t < end && !same_instant(t, end);
    in_window || (started && comp_duration > 0.0 && end >= comp_duration - TERMINAL_EPSILON_SECONDS)
}

enum StartExpr {
    Absolute(f64),
    Reference(String, f64),
}

fn valid_ref_id(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
}

fn parse_number(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn parse_start(raw: Option<&str>) -> Option<StartExpr> {
    let s = raw?.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(v) = parse_number(s) {
        return Some(StartExpr::Absolute(v));
    }
    if valid_ref_id(s) {
        return Some(StartExpr::Reference(s.to_string(), 0.0));
    }
    // "<ref> + 2" / "<ref> - 0.5"
    let bytes = s.as_bytes();
    let mut i = bytes.len();
    while i > 0 && (bytes[i - 1].is_ascii_digit() || bytes[i - 1] == b'.') {
        i -= 1;
    }
    let magnitude: f64 = s[i..].parse().ok()?;
    let head = s[..i].trim_end();
    let (refid, op) = (head[..head.len().saturating_sub(1)].trim(), head.chars().last()?);
    if !valid_ref_id(refid) || !(op == '+' || op == '-') {
        return None;
    }
    Some(StartExpr::Reference(refid.to_string(), if op == '-' { -magnitude } else { magnitude }))
}

/// One timed element with its resolved window.
#[derive(Clone, Debug)]
pub struct Timed {
    pub node: NodeId,
    pub start: f64,
    pub end: f64,
    /// unsnapped start: the origin of CSS animations inside this clip (the composition format's css adapter)
    pub origin: f64,
    /// indices of timed ancestors below the root composition
    pub ancestors: Vec<usize>,
    pub in_flow_leaf: bool,
    pub hidden_attr: bool,
}

pub struct Timing {
    pub timed: Vec<Timed>,
    pub comp_duration: f64,
    /// last applied visibility per timed element (None = untouched)
    applied: Vec<Option<bool>>,
}

pub struct Resolver<'a> {
    doc: &'a BaseDocument,
    pub timeline_durations: &'a HashMap<String, f64>,
    pub media_windows: &'a HashMap<String, (f64, f64)>,
    start_cache: HashMap<NodeId, Option<f64>>,
    duration_cache: HashMap<NodeId, Option<f64>>,
    visiting: Vec<NodeId>,
}

impl<'a> Resolver<'a> {
    pub fn new(
        doc: &'a BaseDocument,
        timeline_durations: &'a HashMap<String, f64>,
        media_windows: &'a HashMap<String, (f64, f64)>,
    ) -> Self {
        Resolver {
            doc,
            timeline_durations,
            media_windows,
            start_cache: HashMap::new(),
            duration_cache: HashMap::new(),
            visiting: Vec::new(),
        }
    }

    fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.doc.get_node(id)?.parent
    }

    fn closest_comp(&self, from: Option<NodeId>) -> Option<NodeId> {
        let mut cur = from;
        while let Some(id) = cur {
            if has_attr(self.doc, id, "data-composition-id") {
                return Some(id);
            }
            cur = self.parent(id);
        }
        None
    }

    fn find_ref(&self, refid: &str) -> Option<NodeId> {
        let els = all_elements(self.doc);
        els.iter().copied().find(|&id| attr(self.doc, id, "id").as_deref() == Some(refid)).or_else(|| {
            els.iter().copied().find(|&id| attr(self.doc, id, "data-composition-id").as_deref() == Some(refid))
        })
    }

    pub fn duration(&mut self, el: NodeId) -> Option<f64> {
        if let Some(c) = self.duration_cache.get(&el) {
            return *c;
        }
        let mut resolved = None;
        for a in ["data-duration", "data-hf-authored-duration"] {
            if let Some(d) = attr(self.doc, el, a).as_deref().and_then(parse_number) {
                if d > 0.0 {
                    resolved = Some(d);
                    break;
                }
            }
        }
        if resolved.is_none() {
            let start = self.start(el, 0.0);
            for a in ["data-end", "data-hf-authored-end"] {
                if let Some(end) = attr(self.doc, el, a).as_deref().and_then(parse_number) {
                    if end > start {
                        resolved = Some(end - start);
                        break;
                    }
                }
            }
        }
        if resolved.is_none() {
            if let Some(vid) = attr(self.doc, el, "data-uv-video") {
                if let Some((s, e)) = self.media_windows.get(&vid) {
                    if e > s {
                        resolved = Some(e - s);
                    }
                }
            }
        }
        if resolved.is_none() {
            if let Some(cid) = attr(self.doc, el, "data-composition-id") {
                if let Some(d) = self.timeline_durations.get(&cid) {
                    if d.is_finite() && *d > 0.0 {
                        resolved = Some(*d);
                    }
                }
            }
        }
        self.duration_cache.insert(el, resolved);
        resolved
    }

    fn host_offset(&mut self, el: NodeId, fallback: f64) -> f64 {
        let comp = if has_attr(self.doc, el, "data-composition-id") {
            let p = self.parent(el);
            self.closest_comp(p)
        } else {
            self.closest_comp(Some(el))
        };
        match comp {
            Some(c) => self.start(c, fallback),
            None => 0.0,
        }
    }

    pub fn start(&mut self, el: NodeId, fallback: f64) -> f64 {
        if let Some(c) = self.start_cache.get(&el) {
            return c.unwrap_or(fallback);
        }
        if self.visiting.contains(&el) {
            return fallback;
        }
        self.visiting.push(el);
        let raw = attr(self.doc, el, "data-start");
        let result = match parse_start(raw.as_deref()) {
            None => {
                let mut r = None;
                if has_attr(self.doc, el, "data-composition-id") {
                    if let Some(p) = self.parent(el) {
                        if has_attr(self.doc, p, "data-composition-src")
                            || has_attr(self.doc, p, "data-composition-id")
                            || has_attr(self.doc, p, "data-composition-file")
                        {
                            r = Some(self.start(p, fallback));
                        }
                    }
                }
                self.start_cache.insert(el, r);
                r.unwrap_or(fallback)
            }
            Some(StartExpr::Absolute(v)) => {
                let host = self.host_offset(el, fallback);
                // data-hf-media-start-basis="global": a nested media element's literal start is
                // already root time (the composition format's resolveMediaStartSeconds)
                let media = has_attr(self.doc, el, "data-uv-video") || crate::compile::tag(self.doc, el) == "audio";
                let global = media
                    && host > 0.0
                    && !has_attr(self.doc, el, "data-hf-auto-start")
                    && attr(self.doc, el, "data-hf-media-start-basis").is_some_and(|b| b.trim().eq_ignore_ascii_case("global"));
                let resolved = if global { v.max(0.0) } else { (host + v.max(0.0)).max(0.0) };
                self.start_cache.insert(el, Some(resolved));
                resolved
            }
            Some(StartExpr::Reference(refid, offset)) => match self.find_ref(&refid) {
                None => {
                    self.start_cache.insert(el, None);
                    fallback
                }
                Some(target) => {
                    let ts = self.start(target, 0.0);
                    let resolved = match self.duration(target) {
                        Some(d) if d > 0.0 => (ts + d + offset).max(0.0),
                        _ => (ts + offset).max(0.0),
                    };
                    self.start_cache.insert(el, Some(resolved));
                    resolved
                }
            },
        };
        self.visiting.retain(|v| *v != el);
        result
    }
}

impl Timing {
    pub fn build(
        doc: &BaseDocument,
        root: Option<NodeId>,
        fps: f64,
        comp_duration: f64,
        timeline_durations: &HashMap<String, f64>,
        media_windows: &HashMap<String, (f64, f64)>,
    ) -> Timing {
        let mut resolver = Resolver::new(doc, timeline_durations, media_windows);
        let nodes: Vec<NodeId> = all_elements(doc)
            .into_iter()
            .filter(|&id| {
                let t = crate::compile::tag(doc, id);
                // the root composition is the stage, not a clip: the HTML composition format never hides it, even past its
                // own data-duration when nested compositions run longer (warm-grain's last caption)
                Some(id) != root
                    && !matches!(t.as_str(), "script" | "style" | "link" | "meta")
                    && (has_attr(doc, id, "data-start") || has_attr(doc, id, "data-hidden"))
            })
            .collect();
        let mut timed = Vec::with_capacity(nodes.len());
        let index: HashMap<NodeId, usize> = nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
        for &node in &nodes {
            let start = resolver.start(node, 0.0);
            let mut duration = resolver.duration(node);
            if let Some(cid) = attr(doc, node, "data-composition-id") {
                let authored = ["data-duration", "data-end", "data-hf-authored-duration", "data-hf-authored-end"]
                    .iter()
                    .any(|a| has_attr(doc, node, a));
                if !authored && duration.is_none_or(|d| d <= 0.0) {
                    duration = timeline_durations.get(&cid).copied().filter(|d| d.is_finite() && *d > 0.0);
                }
            }
            let end = match duration {
                Some(d) if d > 0.0 => start + d,
                _ => f64::INFINITY,
            };
            let start_s = snap_to_frame(start, fps);
            let end_s = if end.is_finite() { snap_to_frame(end, fps) } else { end };
            // ancestors with data-start, stopping at the root composition (exclusive)
            let mut ancestors = Vec::new();
            let mut cur = doc.get_node(node).and_then(|n| n.parent);
            while let Some(p) = cur {
                if Some(p) == root {
                    break;
                }
                if let Some(&i) = index.get(&p) {
                    if has_attr(doc, p, "data-start") {
                        ancestors.push(i);
                    }
                }
                cur = doc.get_node(p).and_then(|n| n.parent);
            }
            let is_leaf = !crate::compile::elements_under(doc, node).iter().skip(1).any(|&d| has_attr(doc, d, "data-start"));
            let in_flow = doc
                .get_node(node)
                .and_then(|n| n.primary_styles())
                .map(|s| {
                    use style::computed_values::position::T as P;
                    matches!(s.get_box().position, P::Static | P::Relative | P::Sticky)
                })
                .unwrap_or(false);
            timed.push(Timed {
                node,
                start: start_s,
                end: end_s,
                origin: start,
                ancestors,
                in_flow_leaf: is_leaf && in_flow,
                hidden_attr: has_attr(doc, node, "data-hidden"),
            });
        }
        let n = timed.len();
        if crate::env_os("DEBUG").is_some() {
            for t in &timed {
                eprintln!(
                    "[debug]   clip <{}#{}> [{:.3}, {:.3}) origin {:.3}",
                    crate::compile::tag(doc, t.node),
                    attr(doc, t.node, "id").or_else(|| attr(doc, t.node, "data-composition-id")).unwrap_or_default(),
                    t.start,
                    t.end,
                    t.origin
                );
            }
        }
        Timing { timed, comp_duration, applied: vec![None; n] }
    }

    pub fn visible(&self, i: usize, t: f64) -> bool {
        let el = &self.timed[i];
        if el.hidden_attr {
            return false;
        }
        let own = clip_visible_at(t, el.start, el.end, self.comp_duration);
        own && el.ancestors.iter().all(|&a| {
            let p = &self.timed[a];
            !p.hidden_attr && clip_visible_at(t, p.start, p.end, self.comp_duration)
        })
    }

    /// Set visibility (and display for in-flow leaves) for time t. Returns true when anything changed.
    pub fn apply(&mut self, doc: &mut BaseDocument, t: f64) -> bool {
        let mut changed = false;
        let states: Vec<bool> = (0..self.timed.len()).map(|i| self.visible(i, t)).collect();
        let mut m = doc.mutate();
        for (i, visible) in states.into_iter().enumerate() {
            if self.applied[i] == Some(visible) {
                continue;
            }
            self.applied[i] = Some(visible);
            changed = true;
            let el = &self.timed[i];
            if el.hidden_attr {
                m.set_style_property(el.node, "display", "none");
                continue;
            }
            m.set_style_property(el.node, "visibility", if visible { "visible" } else { "hidden" });
            if el.in_flow_leaf {
                if visible {
                    m.remove_style_property(el.node, "display");
                } else {
                    m.set_style_property(el.node, "display", "none");
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_expressions() {
        assert!(matches!(parse_start(Some("12.5")), Some(StartExpr::Absolute(v)) if v == 12.5));
        assert!(matches!(parse_start(Some("intro")), Some(StartExpr::Reference(r, o)) if r == "intro" && o == 0.0));
        assert!(matches!(parse_start(Some("intro + 2")), Some(StartExpr::Reference(r, o)) if r == "intro" && o == 2.0));
        assert!(matches!(parse_start(Some("intro - 0.5")), Some(StartExpr::Reference(r, o)) if r == "intro" && o == -0.5));
        // like the HTML composition format, a hyphenated token is an id, not an offset
        assert!(matches!(parse_start(Some("intro-0.5")), Some(StartExpr::Reference(r, o)) if r == "intro-0.5" && o == 0.0));
    }

    #[test]
    fn visibility_windows() {
        assert!(clip_visible_at(1.0, 1.0, 2.0, 10.0));
        assert!(!clip_visible_at(2.0, 1.0, 2.0, 10.0));
        // a clip running to the end owns the terminal instant
        assert!(clip_visible_at(10.0, 5.0, 10.0, 10.0));
        assert_eq!(snap_to_frame(1.0004, 30.0), 1.0004);
        assert_eq!(snap_to_frame(0.0333334, 30.0), 1.0 / 30.0);
    }
}
