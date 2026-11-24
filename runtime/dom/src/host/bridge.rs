//! Blitz's event dispatch speaking to the JS (Bridge), and a window's input fed to the page (WindowInput).

use super::*;

/// The dispatch's side of the bridge: each DOM event handed to the nearest element up from its target that listens.
pub(crate) struct Bridge<'a> {
    /// lent to the JS's context calls, with the document the dispatch holds (PageSync)
    pub(crate) world: &'a mut World,
    pub(crate) page: Entity,
    pub(crate) js: &'a mut dyn JsEngine,
    pub(crate) out: &'a Shared,
    pub(crate) nodes: &'a mut Nodes,
    pub(crate) clicks: u32,
    pub(crate) captured: Option<NodeId>,
    pub(crate) bevy_ui: bool,
    /// the page's scripted <iframe>s, each its frame root's entity
    pub(crate) frames: &'a HashMap<NodeId, Entity>,
    /// events that landed on one, for its root once this dispatch is done
    pub(crate) forwards: &'a mut Vec<(Entity, NodeId, UiEvent)>,
}

/// A pointer event's payload for the element at `rect` (raea-runtime-js's: where in the element as fractions of its size, the
/// client point, the button, the press count, the modifiers).
pub(crate) fn pointer_json(e: &BlitzPointerEvent, rect: Option<blitz_dom::BoundingRect>, detail: u32) -> Value {
    let (x, y) = (e.coords.client_x as f64, e.coords.client_y as f64);
    let (fx, fy) = rect.map_or((0.0, 0.0), |r| ((x - r.x) / r.width.max(1.0), (y - r.y) / r.height.max(1.0)));
    let button = match e.button {
        MouseEventButton::Secondary => 2,
        MouseEventButton::Auxiliary => 1,
        _ => 0,
    };
    json!({"x": fx, "y": fy, "clientX": x, "clientY": y, "button": button, "detail": detail,
        "shiftKey": e.mods.shift(), "ctrlKey": e.mods.ctrl(), "altKey": e.mods.alt(), "metaKey": e.mods.meta()})
}

/// A text field's value (an <input>'s or a <textarea>'s), for a key's or a blur's payload.
pub(crate) fn field_value(doc: &BaseDocument, node: NodeId) -> Option<String> {
    doc.get_node(node)?.element_data()?.text_input_data().map(|t| t.editor.raw_text().to_string())
}

/// The nearest element up from `n` (itself included) with attribute `attr`.
pub(crate) fn up_with(d: &BaseDocument, n: NodeId, attr: &str) -> Option<NodeId> {
    let mut at = Some(n);
    while let Some(i) = at {
        let node = d.get_node(i)?;
        if node.element_data().is_some_and(|el| el.attrs().iter().any(|a| &*a.name.local == attr)) {
            return Some(i);
        }
        at = node.parent;
    }
    None
}

impl Bridge<'_> {
    /// `msg` to the JS, `d` and the World lent to the context calls it makes.
    pub(crate) fn deliver(&mut self, d: &mut BaseDocument, msg: &str) -> Option<Result<f64, String>> {
        let mut ctx = PageSync { world: &mut *self.world, page: self.page, doc: Some(d), nodes: &mut *self.nodes, out: self.out.clone(), queue: None, synced: None };
        raea_runtime_js::calls::with_sync_ctx(&mut ctx, || self.js.call("__ui_deliver", &[Arg::Str(msg)]))
    }

    /// `ev` to element `n` if it hears it, its commit applied; whether it did.
    pub(crate) fn send(&mut self, d: &mut BaseDocument, n: NodeId, ev: Ev) -> bool {
        let Some(id) = self.nodes.id_of(n).filter(|id| self.nodes.hears(*id, ev)) else { return false };
        if let Some(Err(e)) = self.deliver(d, &wire::event(id, ev, Value::Null).to_string()) {
            eprintln!("raea-page: js: an event threw: {e}");
        }
        let batches = std::mem::take(&mut self.out.borrow_mut().flushed);
        commit(self.page, d, self.nodes, batches);
        true
    }

    /// raea-runtime-js's menus (`menu` on a popup, `menuItem` on each item; its host.rs): the focus moves through the items
    /// with the arrows (a `menu="row"` with left and right), Home and End; Enter, Space or a click is an item's
    /// onSelect; Escape is the menu's onClose, and so is a press outside it. Whether the event was the menu's (its
    /// default and the page's handlers then skipped).
    pub(crate) fn menu(&mut self, d: &mut BaseDocument, event: &DomEvent) -> bool {
        use keyboard_types::{Key, NamedKey};
        let open = d.get_focussed_node_id().and_then(|f| up_with(d, f, "menu").map(|m| (f, m)));
        match &event.data {
            DomEventData::KeyDown(k) => {
                let Some((f, menu)) = open else { return false };
                let row = d.get_node(menu).and_then(|n| n.element_data()).and_then(|el| el.attr(blitz_dom::local_name!("menu"))) == Some("row");
                let step = match &k.key {
                    Key::Named(NamedKey::Escape) => {
                        self.send(d, menu, Ev::Close);
                        return true;
                    }
                    Key::Named(NamedKey::Enter) => return up_with(d, f, "menuitem").is_some_and(|item| self.send(d, item, Ev::Select)),
                    Key::Character(c) if c == " " => return up_with(d, f, "menuitem").is_some_and(|item| self.send(d, item, Ev::Select)),
                    Key::Named(NamedKey::ArrowDown) if !row => 1,
                    Key::Named(NamedKey::ArrowRight) if row => 1,
                    Key::Named(NamedKey::ArrowUp) if !row => -1,
                    Key::Named(NamedKey::ArrowLeft) if row => -1,
                    Key::Named(NamedKey::Home) => i32::MIN,
                    Key::Named(NamedKey::End) => i32::MAX,
                    _ => return false,
                };
                let items = ops::menu_items(d, menu);
                if items.is_empty() {
                    return true;
                }
                let last = items.len() as i32 - 1;
                let at = items.iter().position(|i| up_with(d, f, "menuitem") == Some(*i)).map(|i| i as i32);
                let to = match (step, at) {
                    (i32::MIN, _) => 0,
                    (i32::MAX, _) => last,
                    (s, Some(i)) => (i + s).rem_euclid(last + 1),
                    (1, None) => 0,
                    (_, None) => last,
                };
                d.set_focus_to(items[to as usize]);
                true
            }
            // (a primary click: a right press's release, over the menu it opened at the pointer, selects nothing)
            DomEventData::Click(c) if c.button == MouseEventButton::Main => up_with(d, event.target, "menuitem").is_some_and(|item| self.send(d, item, Ev::Select)),
            DomEventData::PointerDown(_) => {
                if let Some((_, menu)) = open
                    && up_with(d, event.target, "menu") != Some(menu)
                {
                    self.send(d, menu, Ev::Close);
                }
                false
            }
            _ => false,
        }
    }
}

impl EventHandler for Bridge<'_> {
    fn handle_event(&mut self, chain: &[NodeId], event: &mut DomEvent, doc: &mut dyn Document, state: &mut EventState) {
        let mut d = doc.inner_mut();
        // an event on a scripted frame is the frame's: none of this page's listeners hear it, and Blitz doesn't forward
        // it to the frame's document itself; its root takes it (frame_ui_event) once this dispatch is done
        if let Some(&frame) = self.frames.get(&event.target) {
            if let Some(ui) = blitz_dom::frame_ui_event(&d, event.target, event) {
                self.forwards.push((frame, event.target, ui));
            }
            state.prevent_default();
            return;
        }
        if self.menu(&mut d, event) {
            state.prevent_default();
            return;
        }
        // a click sdk/ui heard the release of is its to activate (it toggles a checkbox or radio, reverting it if the
        // click was prevented, and clicks a label's control): Blitz's would toggle it a second time. One it didn't hear
        // Blitz activates, and its input event is the element's change
        if let DomEventData::Click(_) = &event.data {
            let heard = chain.iter().any(|n| self.nodes.id_of(*n).is_some_and(|id| self.nodes.hears(id, Ev::PointerUp)));
            let owned = heard && chain.iter().any(|n| {
                d.get_node(*n).and_then(|n| n.element_data()).is_some_and(|el| match &*el.name.local {
                    "label" => true,
                    "input" => matches!(el.attr(blitz_dom::local_name!("type")), Some("checkbox" | "radio")),
                    _ => false,
                })
            });
            if owned {
                state.prevent_default();
            }
            return;
        }
        let ev = match &event.data {
            DomEventData::PointerDown(_) => Ev::PointerDown,
            DomEventData::PointerUp(_) => Ev::PointerUp,
            DomEventData::PointerMove(_) => Ev::PointerMove,
            DomEventData::PointerEnter(_) => Ev::PointerEnter,
            DomEventData::PointerLeave(_) => Ev::PointerLeave,
            // (a right press's contextmenu is sent after its pointerdown, below, as Chrome on macOS sends it; Blitz's
            // comes with the release)
            DomEventData::Wheel(_) => Ev::Wheel,
            DomEventData::Scroll(_) => Ev::Scroll,
            DomEventData::KeyDown(_) => Ev::KeyDown,
            DomEventData::KeyUp(_) => Ev::KeyUp,
            DomEventData::Input(_) => Ev::Change,
            DomEventData::Focus(_) => Ev::Focus,
            DomEventData::Blur(_) => Ev::Blur,
            // mouse and touch compatibility events, click, over and out: the wire has none (sdk/ui makes onClick from
            // the press and its release, as Chrome orders them)
            _ => return,
        };
        // a held press's moves and release go to what captured the pointer
        let captured = matches!(ev, Ev::PointerMove | Ev::PointerUp).then_some(self.captured).flatten();
        let mut path: Vec<NodeId> = captured.map(|c| d.node_chain(c)).unwrap_or_else(|| chain.to_vec());
        if path.is_empty() {
            path.push(event.target);
        }
        // enter and leave don't bubble: Blitz sends one to each element the pointer came into or left
        if matches!(ev, Ev::PointerEnter | Ev::PointerLeave) {
            path.truncate(1);
        }
        let hearer = path.iter().find_map(|n| self.nodes.id_of(*n).filter(|id| self.nodes.hears(*id, ev)).map(|id| (id, *n)));
        // RAEA_PAGE_EVENTS=1: each DOM event the bridge sees and who hears it
        if raea_io_trace::stat("RAEA_PAGE_EVENTS") {
            eprintln!("raea-page: event {ev:?} target {} hearer {:?}", event.target, hearer.map(|h| h.0));
        }
        let msg = match (&event.data, hearer) {
            (DomEventData::PointerDown(p) | DomEventData::PointerUp(p) | DomEventData::PointerMove(p), Some((id, n))) => {
                // a press's count on its release too: sdk/ui gives click and dblclick their detail from it
                let detail = if matches!(ev, Ev::PointerDown | Ev::PointerUp) { self.clicks } else { 0 };
                let mut payload = pointer_json(p, d.get_client_bounding_rect(n), detail);
                // the deepest element sdk/ui knows on Blitz's hit path: Stylo's pointer-events and z-order decided it, as
                // Chrome's would (sdk/ui's own walk sees only inline styles and Tailwind classes, so a stylesheet's
                // `pointer-events: none` layer over the page, Excalidraw's SVGLayer, took every release and no click came)
                if let Some(target) = path.iter().find_map(|n| self.nodes.id_of(*n)) {
                    payload["target"] = json!(target);
                }
                wire::event(id, ev, payload)
            }
            (DomEventData::PointerEnter(_) | DomEventData::PointerLeave(_), Some((id, _))) => wire::event(id, ev, Value::Null),
            (DomEventData::Wheel(w), Some((id, n))) => {
                let (dx, dy, mode) = match w.delta {
                    BlitzWheelDelta::Lines(x, y) => (x, y, "line"),
                    BlitzWheelDelta::Pixels(x, y) => (x, y, "pixel"),
                };
                let rect = d.get_client_bounding_rect(n);
                let (x, y) = (w.coords.client_x as f64, w.coords.client_y as f64);
                let (fx, fy) = rect.map_or((0.0, 0.0), |r| ((x - r.x) / r.width.max(1.0), (y - r.y) / r.height.max(1.0)));
                wire::event(id, ev, json!({"x": fx, "y": fy, "clientX": x, "clientY": y, "button": 0, "detail": 0, "deltaX": -dx, "deltaY": -dy, "deltaMode": mode,
                    "shiftKey": w.mods.shift(), "ctrlKey": w.mods.ctrl(), "altKey": w.mods.alt(), "metaKey": w.mods.meta()}))
            }
            (DomEventData::Scroll(s), Some((id, _))) => wire::event(
                id,
                ev,
                json!({"scrollTop": s.scroll_top, "scrollLeft": s.scroll_left, "scrollHeight": s.scroll_height, "scrollWidth": s.scroll_width, "clientHeight": s.client_height, "clientWidth": s.client_width}),
            ),
            (DomEventData::KeyDown(k) | DomEventData::KeyUp(k), hearer) => {
                let mut v = json!({"key": k.key.to_string(), "shiftKey": k.modifiers.shift(), "ctrlKey": k.modifiers.ctrl(), "altKey": k.modifiers.alt(), "metaKey": k.modifiers.meta(), "repeat": k.is_auto_repeating});
                match hearer {
                    Some((id, n)) => {
                        if let Some(t) = field_value(&d, n) {
                            v["value"] = json!(t);
                        }
                        wire::event(id, ev, v)
                    }
                    // a key nothing focused hears: the root's `keydown` or `keyup` host event, as raea-runtime-js's host
                    // sends them (sdk/ui's dom.ts hears both; a dropped keyup left a window listener's key held)
                    None => wire::named(if ev == Ev::KeyDown { "keydown" } else { "keyup" }, v),
                }
            }
            (DomEventData::Input(i), Some((id, _))) => wire::event(id, ev, json!(i.value)),
            (DomEventData::Focus(_), Some((id, _))) => wire::event(id, ev, Value::Null),
            // where the focus went (blur's relatedTarget): Blitz has moved it before it sends the blur
            (DomEventData::Blur(_), Some((id, n))) => {
                let related = d.get_focussed_node_id().filter(|f| *f != event.target).and_then(|f| self.nodes.id_of(f));
                wire::event(id, ev, json!({"value": field_value(&d, n).unwrap_or_default(), "related": related}))
            }
            _ => return,
        };
        let focus = d.get_focussed_node_id();
        // (a bevy_ui root hears a right press as its contextmenu alone, below)
        let right = matches!(&event.data, DomEventData::PointerDown(p) | DomEventData::PointerUp(p) if p.button == MouseEventButton::Secondary);
        if !(self.bevy_ui && right)
            && let Some(Err(e)) = self.deliver(&mut d, &msg.to_string())
        {
            eprintln!("raea-page: js: an event threw: {e}");
        }
        // its commit, applied before the default action runs; its `prevented`, Blitz's prevent_default
        let (batches, mut prevented) = {
            let mut o = self.out.borrow_mut();
            let prevented = o.emits.iter().any(|(n, _)| n == "prevented");
            o.emits.retain(|(n, _)| n != "prevented");
            (std::mem::take(&mut o.flushed), prevented)
        };
        commit(self.page, &mut d, self.nodes, batches);
        // a right press: its contextmenu, to the nearest element up that hears it (before the press's focus default: a
        // menu it opens has the focus)
        if let DomEventData::PointerDown(p) = &event.data
            && p.button == MouseEventButton::Secondary
            && let Some((id, n)) = path.iter().find_map(|n| self.nodes.id_of(*n).filter(|id| self.nodes.hears(*id, Ev::ContextMenu)).map(|id| (id, *n)))
        {
            let before = d.get_focussed_node_id();
            let msg = wire::event(id, Ev::ContextMenu, pointer_json(p, d.get_client_bounding_rect(n), 1));
            if let Some(Err(e)) = self.deliver(&mut d, &msg.to_string()) {
                eprintln!("raea-page: js: an event threw: {e}");
            }
            let batches = std::mem::take(&mut self.out.borrow_mut().flushed);
            commit(self.page, &mut d, self.nodes, batches);
            // (Chrome's contextmenu comes after the press's focus: a focus it moves stays, on any page)
            prevented |= d.get_focussed_node_id() != before;
        }
        // a press whose handler moved the focus: Blitz's press default (the target's focus, else none) would undo it
        prevented |= self.bevy_ui && ev == Ev::PointerDown && d.get_focussed_node_id() != focus;
        // Enter in a multiline="shift" field is its handler's key, not a line break (raea-runtime-js's host breaks it on Shift+Enter)
        if let DomEventData::KeyDown(k) = &event.data
            && k.key == keyboard_types::Key::Named(keyboard_types::NamedKey::Enter)
            && !k.modifiers.shift()
            && d.get_node(event.target).and_then(|n| n.element_data()).is_some_and(|el| el.attrs().iter().any(|a| &*a.name.local == ops::ENTER_IS_KEY))
        {
            prevented = true;
        }
        if prevented {
            if raea_io_trace::stat("RAEA_PAGE_EVENTS") {
                eprintln!("raea-page: event {ev:?} prevented");
            }
            state.prevent_default();
        }
    }
}

/// A window's input as Blitz takes it: Bevy's WindowEvent mirror, which bevy_winit writes and the legacy app's injector writes
/// in its place, turned into UiEvents as blitz-shell turns winit's events. It keeps where the pointer is, the buttons
/// held and the modifiers between events; points are the page's (the window's top left is the page's).
#[derive(Default)]
pub struct WindowInput {
    pub(crate) at: Vec2,
    pub(crate) buttons: blitz_traits::events::MouseEventButtons,
    pub(crate) mods: keyboard_types::Modifiers,
}

impl WindowInput {
    pub(crate) fn pointer(&self, button: MouseEventButton) -> BlitzPointerEvent {
        BlitzPointerEvent {
            id: blitz_traits::events::BlitzPointerId::Mouse,
            is_primary: true,
            coords: self.coords(),
            button,
            buttons: self.buttons,
            mods: self.mods,
            details: blitz_traits::events::PointerDetails { pressure: if self.buttons.is_empty() { 0.0 } else { 0.5 }, ..Default::default() },
            element: Default::default(),
            active_pointers: Default::default(),
        }
    }

    pub(crate) fn coords(&self) -> blitz_traits::events::PointerCoords {
        let (x, y) = (self.at.x, self.at.y);
        blitz_traits::events::PointerCoords { page_x: x, page_y: y, screen_x: x, screen_y: y, client_x: x, client_y: y }
    }

    /// The UiEvent a window event is, if any (resizes and the like are the app's).
    pub fn event(&mut self, ev: &bevy::window::WindowEvent) -> Option<UiEvent> {
        use bevy::input::ButtonState;
        use bevy::input::keyboard::KeyCode;
        use bevy::input::mouse::{MouseButton, MouseScrollUnit};
        use bevy::window::{Ime, WindowEvent};
        use blitz_traits::events::{BlitzImeEvent, BlitzKeyEvent, BlitzWheelEvent, KeyState, MouseEventButtons};
        use keyboard_types::{Code, Key, Location, Modifiers};
        match ev {
            WindowEvent::CursorMoved(m) => {
                // a move to where the pointer is isn't one (an injected press writes its position first)
                if m.position == self.at {
                    return None;
                }
                self.at = m.position;
                Some(UiEvent::PointerMove(self.pointer(MouseEventButton::Main)))
            }
            WindowEvent::CursorLeft(_) => Some(UiEvent::PointerLeave(self.pointer(MouseEventButton::Main))),
            WindowEvent::MouseButtonInput(b) => {
                let (button, bit) = match b.button {
                    MouseButton::Left => (MouseEventButton::Main, MouseEventButtons::Primary),
                    MouseButton::Right => (MouseEventButton::Secondary, MouseEventButtons::Secondary),
                    MouseButton::Middle => (MouseEventButton::Auxiliary, MouseEventButtons::Auxiliary),
                    _ => return None,
                };
                if b.state == ButtonState::Pressed {
                    self.buttons.insert(bit);
                    Some(UiEvent::PointerDown(self.pointer(button)))
                } else {
                    self.buttons.remove(bit);
                    Some(UiEvent::PointerUp(self.pointer(button)))
                }
            }
            WindowEvent::MouseWheel(w) => Some(UiEvent::Wheel(BlitzWheelEvent {
                delta: match w.unit {
                    MouseScrollUnit::Line => BlitzWheelDelta::Lines(w.x as f64, w.y as f64),
                    MouseScrollUnit::Pixel => BlitzWheelDelta::Pixels(w.x as f64, w.y as f64),
                },
                coords: self.coords(),
                buttons: self.buttons,
                mods: self.mods,
                element: Default::default(),
            })),
            WindowEvent::KeyboardInput(k) => {
                let down = k.state == ButtonState::Pressed;
                let bit = match k.key_code {
                    KeyCode::ShiftLeft | KeyCode::ShiftRight => Some(Modifiers::SHIFT),
                    KeyCode::ControlLeft | KeyCode::ControlRight => Some(Modifiers::CONTROL),
                    KeyCode::AltLeft | KeyCode::AltRight => Some(Modifiers::ALT),
                    KeyCode::SuperLeft | KeyCode::SuperRight => Some(Modifiers::META),
                    _ => None,
                };
                if let Some(bit) = bit {
                    self.mods.set(bit, down);
                }
                // the key as the DOM names it (a character, or its name: Bevy's names are the DOM's)
                let name = match &k.logical_key {
                    bevy::input::keyboard::Key::Character(c) => c.to_string(),
                    bevy::input::keyboard::Key::Space => " ".into(),
                    other => format!("{other:?}"),
                };
                let key = name.parse::<Key>().unwrap_or(Key::Character(name));
                let code = format!("{:?}", k.key_code).parse::<Code>().unwrap_or(Code::Unidentified);
                let e = BlitzKeyEvent {
                    key,
                    code,
                    modifiers: self.mods,
                    location: Location::Standard,
                    is_auto_repeating: k.repeat,
                    is_composing: false,
                    state: if down { KeyState::Pressed } else { KeyState::Released },
                    text: k.text.as_ref().map(|t| blitz_traits::SmolStr::new(t.as_str())),
                };
                Some(if down { UiEvent::KeyDown(e) } else { UiEvent::KeyUp(e) })
            }
            WindowEvent::Ime(i) => Some(UiEvent::Ime(match i {
                Ime::Enabled { .. } => BlitzImeEvent::Enabled,
                Ime::Disabled { .. } => BlitzImeEvent::Disabled,
                Ime::Preedit { value, cursor, .. } => BlitzImeEvent::Preedit(value.clone(), *cursor),
                Ime::Commit { value, .. } => BlitzImeEvent::Commit(value.clone()),
            })),
            _ => None,
        }
    }
}

/// The press count the app's input says for the presses that come next (a double click's 2: pointerdown's detail).
pub fn set_clicks(world: &mut World, page: Entity, clicks: u32) {
    if let Some(r) = world.get_non_send_mut::<Roots>().as_mut().and_then(|r| r.get_mut(page)) {
        r.clicks = clicks;
    }
}
