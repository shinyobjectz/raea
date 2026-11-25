//! The page's accessibility tree published to the OS (raea's window, the `window` feature): VoiceOver and macOS
//! accessibility clients (an AX test driving raea.app, Accessibility Inspector) read every element of the page as
//! AccessKit has it, by raea-sys-a11y's computation (the role, name and states Chrome gives the same markup), with its
//! box. Bevy's winit adapter carries it: Bevy's own tree (its UI entities' AccessibilityNode) is turned off
//! (ManageAccessibilityUpdates), and this sends the page's instead, while a client is listening (AccessibilityRequested)
//! and at most every 250 ms. An action a client asks of an element comes back as the input a person would give:
//! Click and SetValue press the element's middle (a pointer down and up, as the MCP input tool queues them), and
//! SetValue then types its text over what the field held.
//!
//! The tree is the Blitz document's own, walked from its root: an element's attributes are its props, and React's
//! props over them where the element came from React (the wire's ids). A page's markup set as HTML (innerHTML: raea
//! cloud's dashboard builds its rail so) has no wire ids, and the BRP `a11y` method's elements (lib.rs `records`) leave it out.
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use accesskit::{Action, ActionData, NodeId};
use bevy::a11y::{AccessibilityRequested, ActionRequest, ManageAccessibilityUpdates};
use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::accessibility::ACCESS_KIT_ADAPTERS;
use blitz_dom::{BaseDocument, NodeId as Id};
use raea_sys_a11y::El;
use raea_sys_page::ops::Nodes;
use serde_json::{Map, Value, json};

use crate::{Facts, svg_title, tag_of, wire_test_id, with_doc};

/// A page node's AccessKit id: its Blitz id (slot and version) with the top bit set, clear of the window entity's
/// (Entity::to_bits: its generation never reaches it, nor does a Blitz slot's version)
const PAGE: u64 = 1 << 63;
const EVERY: Duration = Duration::from_millis(250);
/// what a page never draws, and an assistive client has no use for
const UNDRAWN: &[&str] = &["head", "script", "style", "template", "noscript", "link", "meta", "title"];

/// The page host's side: the page shown, the frame it's on, and an input event queued for a frame or later as a person's
/// is (the frame it lands before).
#[derive(Resource, Clone, Copy)]
pub struct Host {
    pub page: fn(&World) -> Option<Entity>,
    pub frame: fn(&World) -> u64,
    pub queue: fn(&mut World, Value, u64) -> u64,
}

impl Host {
    fn doc<R>(self, world: &World, f: impl FnOnce(&BaseDocument, &Nodes) -> R) -> Option<R> {
        with_doc(world, (self.page)(world)?, f)
    }
}

pub struct AxPlugin(pub Host);

#[derive(Resource, Default)]
struct Ax {
    last: Option<Instant>,
    /// the frame last sent: a window at rest draws none, and has nothing new to say
    frame: u64,
}

impl Plugin for AxPlugin {
    fn build(&self, app: &mut App) {
        let mut manage = ManageAccessibilityUpdates::default();
        manage.set(false);
        app.insert_resource(manage).insert_resource(self.0).init_resource::<Ax>().add_systems(PostUpdate, (act, publish).chain());
    }
}

/// The page's tree sent to the window's adapter, while a client listens.
fn publish(world: &mut World) {
    if !world.get_resource::<AccessibilityRequested>().is_some_and(|r| r.get()) {
        return;
    }
    let host = *world.resource::<Host>();
    let (now, frame) = (Instant::now(), (host.frame)(world));
    let ax = world.resource::<Ax>();
    if ax.frame == frame || ax.last.is_some_and(|t| now - t < EVERY) {
        return;
    }
    let Ok((win, w)) = world.query_filtered::<(Entity, &Window), With<PrimaryWindow>>().single(world) else { return };
    let (scale, title) = (w.scale_factor() as f64, w.title.clone());
    let root = NodeId(win.to_bits());
    let Some(mut update) = host.doc(world, |doc, nodes| {
        let (els, tops) = elements(doc, nodes);
        let node = |k: Id| doc.get_node(k).filter(|n| n.is_element());
        let test_id = |k: Id| {
            let wire = nodes.id_of(k).and_then(|id| wire_test_id(nodes, id));
            wire.or_else(|| node(k)?.element_data()?.attrs().iter().find(|a| &*a.name.local == "data-testid").map(|a| a.value.to_string()))
        };
        let facts = Facts { node: &node, test_id: &test_id, drawn: true };
        let focus = doc.get_focussed_node_id().filter(|f| els.contains_key(f));
        let memo = RefCell::new(blitz_dom::RectMemo::default());
        // in the window's physical pixels, as Bevy's own nodes are
        let bounds = |k: Id| {
            let b = doc.get_client_bounding_rect_in(element(doc, k)?, &mut memo.borrow_mut())?;
            Some([b.x * scale, b.y * scale, (b.x + b.width) * scale, (b.y + b.height) * scale])
        };
        raea_sys_a11y::tree(els, tops, &facts, focus, |k| k.as_u64() | PAGE, root, Some(&title), bounds).0
    }) else {
        return;
    };
    raea_sys_a11y::unhide(&mut update, root);
    ACCESS_KIT_ADAPTERS.with_borrow_mut(|adapters| {
        if let Some(a) = adapters.get_mut(&win) {
            a.update_if_active(|| update);
        }
    });
    let mut ax = world.resource_mut::<Ax>();
    (ax.last, ax.frame) = (Some(now), frame);
}

/// The document's drawn nodes, from its root: elements with their attributes (React's props over them) and text.
/// One document: every node shares one root for `id` lookups.
fn elements(doc: &BaseDocument, nodes: &Nodes) -> (HashMap<Id, El<Id>>, HashMap<Id, Id>) {
    let mut els = HashMap::new();
    // from the document's root, and from any React tree drawn apart from it (a layer whose top isn't the document:
    // raea.app's welcome over the dashboard), each top once
    let root = doc.root_node().id;
    let mut apart = std::collections::BTreeSet::new();
    for &n in nodes.by_id.values() {
        let mut top = n;
        while let Some(p) = doc.get_node(top).and_then(|t| t.parent) {
            top = p;
        }
        if top != root && doc.get_node(top).is_some() {
            apart.insert(top.as_u64());
        }
    }
    let mut stack: Vec<(Id, Option<Id>)> = apart.iter().rev().map(|t| (Id::from_u64(*t), None)).collect();
    stack.extend(doc.root_node().children.iter().rev().map(|c| (*c, None)));
    while let Some((k, parent)) = stack.pop() {
        let Some(n) = doc.get_node(k) else { continue };
        let el = if n.is_text_node() {
            El { tag: "text".into(), props: Map::new(), parent, kids: vec![], text: Some(n.text_content()), title: None }
        } else if let Some(e) = n.element_data() {
            let tag = e.name.local.to_string();
            if UNDRAWN.contains(&tag.as_str()) {
                continue;
            }
            let mut props: Map<String, Value> = e.attrs().iter().map(|a| (a.name.local.to_string(), Value::String(a.value.to_string()))).collect();
            // the computation reads React's spelling
            if let Some(t) = props.remove("tabindex") {
                props.insert("tabIndex".into(), t);
            }
            let wire = nodes.id_of(k);
            if let Some(p) = wire.and_then(|id| nodes.props.get(&id)) {
                props.extend(p.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
            let tag = wire.and_then(|id| nodes.kind(id)).filter(|k| *k == "editableText").map_or(tag, |k| tag_of(k, &props).to_string());
            let title = if tag == "svg" { svg_title(doc, n) } else { None };
            let kids: Vec<Id> = n.children.iter().copied().filter(|c| doc.get_node(*c).is_some_and(|c| c.is_text_node() || c.is_element())).collect();
            stack.extend(kids.iter().map(|c| (*c, Some(k))));
            El { tag, props, parent, kids, text: None, title }
        } else {
            continue;
        };
        els.insert(k, el);
    }
    // a kid left out (an undrawn element) is no kid
    let have: HashSet<Id> = els.keys().copied().collect();
    for el in els.values_mut() {
        el.kids.retain(|c| have.contains(c));
    }
    let tops = els.keys().map(|k| (*k, Id::default())).collect();
    (els, tops)
}

/// A Blitz node when it's an element: only an element has a box to ask for (Blitz panics asking a text node's
/// transform, which the rect's walk does: the first raea.app the AX test drove quit on its welcome, 11-23)
fn element(doc: &BaseDocument, k: Id) -> Option<Id> {
    doc.get_node(k).is_some_and(|n| n.is_element()).then_some(k)
}

/// Where an ask's element is, for a press.
enum At {
    /// its middle, and it's what a press there hits
    Press(f64, f64),
    /// scrolled out of a box it's in: the pointer's place over that box, and the scroll (wheel dx, dy) that shows it
    Scroll(f64, f64, f64, f64),
    /// gone, without a box, or something else is drawn over its middle
    Nothing,
}

/// A box a wheel scrolls.
fn by_wheel(o: style::values::computed::Overflow) -> bool {
    use style::values::computed::Overflow as O;
    o == O::Scroll || o == O::Auto
}

fn at(doc: &BaseDocument, k: Id) -> At {
    // a text node is pressed where its element is
    let Some(k) = element(doc, k).or_else(|| doc.get_node(k)?.parent).and_then(|k| element(doc, k)) else { return At::Nothing };
    let Some(b) = doc.get_client_bounding_rect(k).filter(|b| b.width > 0.0 && b.height > 0.0) else { return At::Nothing };
    let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    let mut up = doc.get_node(k).and_then(|n| n.parent);
    while let Some(a) = up {
        let Some(n) = doc.get_node(a) else { break };
        up = n.parent;
        let scrolls = n.primary_styles().map(|s| [s.clone_overflow_x(), s.clone_overflow_y()].map(by_wheel));
        let Some([sx, sy]) = scrolls else { continue };
        let Some(r) = element(doc, a).and_then(|a| doc.get_client_bounding_rect(a)) else { continue };
        let dx = if sx && (x < r.x || x > r.x + r.width) { r.x + r.width / 2.0 - x } else { 0.0 };
        let dy = if sy && (y < r.y || y > r.y + r.height) { r.y + r.height / 2.0 - y } else { 0.0 };
        if dx != 0.0 || dy != 0.0 {
            return At::Scroll(r.x + r.width / 2.0, r.y + r.height / 2.0, dx, dy);
        }
    }
    // what a press there lands on is the element, or in it: never something else drawn over it
    let mut hit = doc.hit(x as f32, y as f32).map(|h| h.node_id);
    while let Some(h) = hit {
        if h == k {
            return At::Press(x, y);
        }
        hit = doc.get_node(h).and_then(|n| n.parent);
    }
    At::Nothing
}

/// An ask whose element is being scrolled to: its next look, and the looks left.
struct Later(accesskit::ActionRequest, u64, u8);

/// What clients asked of the page's elements, as input: a press at the element's middle (and SetValue's text typed),
/// scrolled to first when a box it's in has it out of view, as Chrome's is. An element something else covers isn't
/// pressed: what covers it would be (a press on an option below a list's fold closed the list, 11-23).
fn act(world: &mut World, reader: &mut SystemState<MessageReader<ActionRequest>>, mut later: Local<Vec<Later>>) {
    let Ok(mut r) = reader.get_mut(world) else { return };
    // Focus is no press: macOS asks it beside a press (AXPress), and a second press where the first one's new page
    // has something else clicked that (a press on Projects opened Publishing, 11-23)
    let mut asks: Vec<Later> = r.read().filter(|r| matches!(r.0.action, Action::Click | Action::SetValue)).map(|r| Later(r.0.clone(), 0, 8)).collect();
    let host = *world.resource::<Host>();
    let frame = (host.frame)(world);
    for l in std::mem::take(&mut *later) {
        if l.1 > frame { later.push(l) } else { asks.push(l) }
    }
    for Later(ask, _, left) in asks {
        let Some(k) = (ask.target_node.0 & PAGE != 0).then(|| Id::from_u64(ask.target_node.0 & !PAGE)) else { continue };
        let Some(place) = host.doc(world, |doc, _| at(doc, k)) else { continue };
        let next = frame + 1;
        let (x, y) = match place {
            At::Press(x, y) => (x, y),
            At::Scroll(x, y, dx, dy) if left > 0 => {
                let f = (host.queue)(world, json!({"type": "wheel", "x": x, "y": y, "dx": dx, "dy": dy}), next);
                // laid out where it scrolled to by the frame after the wheel's
                later.push(Later(ask, f + 2, left - 1));
                continue;
            }
            _ => {
                warn!("raea-page: an assistive client's {:?} on an element nothing can press (gone, covered or out of reach)", ask.action);
                continue;
            }
        };
        (host.queue)(world, json!({"type": "down", "x": x, "y": y}), next);
        (host.queue)(world, json!({"type": "up", "x": x, "y": y}), next);
        if let (Action::SetValue, Some(ActionData::Value(v))) = (ask.action, &ask.data) {
            // what the field held selected, then the new text over it
            (host.queue)(world, json!({"type": "key", "key": "a", "mods": [if cfg!(target_os = "macos") { "cmd" } else { "ctrl" }]}), next);
            (host.queue)(world, json!({"type": "text", "text": v.to_string()}), next);
        }
    }
}
