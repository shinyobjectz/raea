//! A bundle on a Blitz page (docs/blitz-bevy.feature B5c): mounted by raea-page's host, so its elements are an HTML
//! document's that Blitz styles, lays out and hit tests as a browser does, and Vello paints on Bevy's device into the
//! page's image. That image is drawn on the UI camera at the window's size; nothing else is in bevy_ui.
//!
//! The order in a frame (src/lib.rs): input injected (inject.rs) and turned into Blitz's UiEvents here, then an app's
//! own events for the page, `step` (the page's JS: its frame, timers and host events), its asks; in PostUpdate
//! raea-page restyles, lays out and paints; in Last the tree is read from the document (`tree`) for the `ui` method.

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowEvent};
use raea_sys_page::Docs;
use raea_runtime_dom::host::{self, Element, PageAsks, Roots, WindowInput};
use serde_json::{Value, json};

use crate::inject::{Frame, Pending};
use crate::tree::r1;

/// The page the host shows.
#[derive(Resource)]
pub struct HostPage(pub Entity);

/// The node on the UI camera that shows the page's image.
#[derive(Component)]
pub struct Shows;

/// What a page is mounted with.
pub struct Mount {
    /// the bundle's JS
    pub code: String,
    /// JS run before it (an app's globals)
    pub preamble: Vec<String>,
    /// the page's own CSS files, laid out as Chrome lays out its page (sdk/ui/parity/probe/page.html); None: bevy_ui's
    /// kinds as a bevy_ui Node is, under `styles`
    pub css: Option<Vec<String>>,
    /// the bundle's Tailwind build, when `css` is None
    pub styles: Option<String>,
    /// its pointers through raea-page's picking route and its keys through KeysPlugin (docs/blitz-bevy.feature B3 H6)
    pub picking: bool,
    /// the page's size at launch, in points
    pub size: UVec2,
}

/// The bundle mounted on a page at `m.size`, and its image on the UI camera.
pub fn mount(world: &mut World, m: Mount) -> Result<Entity, String> {
    let cam = world.resource::<crate::UiCam>().0;
    world.init_resource::<raea_runtime_js::HostCalls>();
    // bevy_text on the page's faces (B0d)
    raea_sys_fonts::share_with_text(world, std::path::Path::new(""));
    let page = host::mount(world, m.size.x, m.size.y, &m.code, &m.preamble, Some(system_fonts()))?;
    host::add_style(world, page, "html, body { height: 100%; }");
    match m.css {
        Some(files) => {
            for f in files {
                match std::fs::read_to_string(&f) {
                    Ok(c) => host::add_style(world, page, &c),
                    Err(e) => eprintln!("raea-page-host: {f}: {e}"),
                }
            }
        }
        None => {
            // presses as raea-runtime-js sent a bevy_ui root's; a page of its own CSS hears Chrome's (P11 right-press:
            // pointerdown, mousedown, contextmenu, pointerup, mouseup, auxclick)
            host::bevy_ui_presses(world, page);
            host::add_style(world, page, host::BEVY_UI_CSS);
            if let Some(s) = &m.styles {
                host::add_style(world, page, s);
            }
        }
    }
    world.insert_resource(HostPage(page));
    world.insert_resource(raea_page_stage::StagePage(page));
    let shows = world.spawn((Shows, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() }, UiTargetCamera(cam))).id();
    if m.picking {
        // the node itself isn't a hit (bevy_ui's backend would report it above the page's twins, and nothing would
        // reach the page): the page's elements are, through the view
        world.entity_mut(shows).insert((raea_runtime_input::pick::PageView { page, pass_through: false, keys_when_unfocused: true }, bevy::picking::Pickable::IGNORE));
    }
    Ok(page)
}

/// The page at the window's size in points, painted at the window's scale on screen: on a Retina display the page's
/// image is twice the points each way and the document lays out with hidpi_scale 2, so text and hairlines are drawn
/// at the display's pixels (v0.1.1 painted every window at 1x and stretched it: "UI renders at 1x on Retina"). The
/// scale is read every frame, so a window dragged to another display follows it. Headless, the image target's scale.
pub fn size(win: Query<&Window, With<PrimaryWindow>>, page: Res<HostPage>, target: Option<Res<crate::inject::Target>>, mut pages: Query<&mut raea_sys_page::Page>) {
    let Ok(w) = win.single() else { return };
    let want = UVec2::new(w.width().round() as u32, w.height().round() as u32);
    let scale = page_scale(target.as_deref().is_none_or(|t| t.windowed), w, target.as_deref().map_or(1.0, |t| t.scale));
    if let Ok(mut p) = pages.get_mut(page.0)
        && (p.size != want || p.scale != scale)
    {
        p.size = want;
        p.scale = scale;
    }
}

/// The scale a page is painted at: the window's on screen (a Retina display's 2, a window's override), else the
/// headless image's.
pub fn page_scale(windowed: bool, win: &Window, headless: f32) -> f32 {
    if windowed { (win.scale_factor() as f32).max(0.5) } else { headless }
}

/// The page's image on the node that shows it, once raea-page made it (and again when a resize remade it).
pub fn show(page: Res<HostPage>, images: Query<&raea_sys_page::PageImage>, mut shows: Query<(Entity, Option<&mut ImageNode>), With<Shows>>, mut commands: Commands) {
    let Ok(img) = images.get(page.0) else { return };
    for (e, node) in &mut shows {
        match node {
            Some(mut n) if n.image != img.0 => n.image = img.0.clone(),
            Some(_) => {}
            None => {
                commands.entity(e).insert(ImageNode::new(img.0.clone()));
            }
        }
    }
}

/// The window's events this frame, as Blitz's.
#[derive(Resource, Default)]
pub struct Events(Vec<WindowEvent>);

pub fn read_input(mut evs: MessageReader<WindowEvent>, mut out: ResMut<Events>) {
    out.0.extend(evs.read().cloned());
}

/// The frame's input dispatched on the page (each press with the suite's click count), the frame number set, a resize
/// told to the space.
pub fn input(world: &mut World, mut at: Local<WindowInput>) {
    let page = world.resource::<HostPage>().0;
    world.resource_mut::<raea_runtime_js::UiFrame>().0 = world.resource::<Frame>().0;
    let evs = std::mem::take(&mut world.resource_mut::<Events>().0);
    for ev in &evs {
        if let WindowEvent::WindowResized(r) = ev {
            host::post_to(world, page, "resize", json!({"w": r.width, "h": r.height}));
            continue;
        }
        let Some(ui) = at.event(ev) else { continue };
        if raea_io_trace::stat("RAEA_PAGE_EVENTS") {
            eprintln!("raea-page-host: frame {} {ui:?}", world.resource::<Frame>().0);
        }
        if matches!(ui, blitz_traits::events::UiEvent::PointerDown(_)) {
            let clicks = world.resource::<Pending>().clicks;
            host::set_clicks(world, page, clicks);
        }
        host::input(world, page, ui);
    }
    ime(world, page);
}

/// With picking: the frame number and a resize told to the space, and the injector's press count for the route
/// (PressCount, a double click's 2 as the injector counts it); the pointers and keys are the route's and KeysPlugin's.
pub fn frame_input(world: &mut World) {
    let page = world.resource::<HostPage>().0;
    world.resource_mut::<raea_runtime_js::UiFrame>().0 = world.resource::<Frame>().0;
    let evs = std::mem::take(&mut world.resource_mut::<Events>().0);
    for ev in &evs {
        if let WindowEvent::WindowResized(r) = ev {
            host::post_to(world, page, "resize", json!({"w": r.width, "h": r.height}));
        }
    }
    let clicks = world.resource::<Pending>().clicks;
    world.insert_resource(raea_runtime_input::pick::PressCount(clicks));
}

/// The window's IME on while a text field has the page's focus, as bevy_ui_widgets turns it on for bevy_ui's page:
/// typed text then comes as one commit (AppKit's insertText), not a key per character.
fn ime(world: &mut World, page: Entity) {
    let on = world.non_send::<Docs>().0.get(&page).is_some_and(|d| {
        d.get_focussed_node_id().and_then(|n| d.get_node(n)).and_then(|n| n.element_data()).is_some_and(|e| e.text_input_data().is_some())
    });
    let mut q = world.query_filtered::<&mut Window, With<bevy::window::PrimaryWindow>>();
    for mut w in q.iter_mut(world) {
        if w.ime_enabled != on {
            w.ime_enabled = on;
        }
    }
}

/// The page's frame, its asks left in PageAsks for the app (`step_dropping_asks`: the bin's, which has none to act on).
pub fn step(world: &mut World, picking: bool) {
    // with picking the page stepped in PageSystems::Step (HostStepPlugin)
    if !picking {
        host::step(world);
    }
}

pub fn step_dropping_asks(world: &mut World) {
    step(world, crate::picking());
    world.resource_mut::<PageAsks>().0.clear();
}

/// A control's value as Chrome reads it: a checkbox's or radio's checked, a field's text, a
/// select's or another input's value, else data-value.
fn value_of(e: &Element) -> Value {
    let p = &e.props;
    let ty = p.get("type").and_then(Value::as_str).unwrap_or("");
    if e.kind == "input" && matches!(ty, "checkbox" | "radio") {
        return json!(e.checked.unwrap_or(false));
    }
    if let Some(f) = &e.field {
        return json!(f.text);
    }
    let own = || p.get("value").or(p.get("defaultValue")).map(|v| json!(v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())));
    match (e.kind.as_str(), ty) {
        ("select", _) => own().or_else(|| e.chosen.clone().map(Value::String)).unwrap_or(Value::Null),
        ("input", "range") => {
            let num = |k: &str, d: f64| p.get(k).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(d);
            let (lo, hi) = (num("min", 0.0), num("max", 100.0));
            let hi = hi.max(lo);
            let v = own().and_then(|s| s.as_str().and_then(|s| s.parse::<f64>().ok())).unwrap_or(lo + (hi - lo) / 2.0).clamp(lo, hi);
            json!(if v.fract() == 0.0 { format!("{}", v as i64) } else { v.to_string() })
        }
        ("input", "submit") => own().unwrap_or(json!("Submit")),
        ("input", "reset") => own().unwrap_or(json!("Reset")),
        ("input", _) => own().or_else(|| p.get("data-value").cloned()).unwrap_or(Value::Null),
        _ => p.get("data-value").cloned().unwrap_or(Value::Null),
    }
}

/// The `ui` method's tree from the document: every element with a data-testid (`substrate` says which page).
pub fn tree(page: Res<HostPage>, roots: NonSend<Roots>, docs: NonSend<Docs>, win: Query<&Window, With<PrimaryWindow>>, frame: Res<Frame>, target: Res<crate::inject::Target>, mut out: ResMut<crate::tree::Tree>) {
    let size = win.single().map(|w| Vec2::new(w.width(), w.height())).unwrap_or(Vec2::ZERO);
    let els = host::elements(&roots, &docs, page.0, |p| p.get("data-testid").is_some_and(Value::is_string));
    let mut nodes: Vec<Value> = els
        .iter()
        .map(|e| {
            let p = &e.props;
            let s = |k: &str| p.get(k).and_then(Value::as_str);
            let pressed = p.get("aria-pressed") == Some(&Value::Bool(true)) || s("aria-pressed") == Some("true");
            let selected = p.get("aria-selected") == Some(&Value::Bool(true)) || s("aria-selected") == Some("true");
            // aria-current: the active row of a list (a page, a step), true or its token; "false" or absent is none
            let current = match p.get("aria-current") { Some(Value::Bool(b)) => *b, Some(Value::String(v)) => v != "false" && !v.is_empty(), _ => false };
            let [x, y, w, h] = e.rect;
            let mut n = json!({
                "id": s("data-testid"),
                "role": s("role").unwrap_or("group"),
                "name": s("aria-label").unwrap_or(""),
                "rect": {"x": r1(x), "y": r1(y), "w": r1(w), "h": r1(h)},
                "visible": e.visible,
                "hovered": e.hovered,
                "focused": e.focused,
                "pressed": pressed,
                "selected": selected,
                "current": current,
                "value": value_of(e),
            });
            if let Some(f) = &e.field {
                n["caret"] = json!(f.caret);
                if f.composing {
                    n["marked"] = json!(true);
                }
            }
            let mut state: Vec<&str> = vec![];
            for (on, tok) in [(e.hovered, "hovered"), (e.focused, "focused"), (pressed, "pressed"), (selected, "selected"), (current, "current")] {
                if on {
                    state.push(tok);
                }
            }
            state.extend(s("data-state").map(|v| v.split_whitespace().collect::<Vec<_>>()).unwrap_or_default());
            // a boolean data-* flag (data-selected on the feed's card) as its own token
            for (k, v) in p.iter().filter(|(k, _)| k.starts_with("data-") && !matches!(k.as_str(), "data-testid" | "data-state" | "data-text" | "data-value")) {
                if *v == Value::Bool(true) || v.as_str().is_some_and(|v| v.is_empty() || v == "true") {
                    state.push(k.as_str());
                }
            }
            n["state"] = json!(state);
            if let Some(t) = s("data-text") {
                n["text"] = json!(t);
            } else if p.get("contentEditable").or(p.get("contenteditable")).is_some_and(|v| *v != json!(false) && *v != "false") {
                // a contenteditable's text, as Chrome's textContent has it (an editor's document isn't in its props)
                let node = roots.0.iter().find(|r| r.page == page.0).and_then(|r| r.nodes.by_id.get(&e.id)).and_then(|&n| docs.0.get(&page.0)?.get_node(n));
                n["text"] = json!(node.map(|d| d.text_content()).unwrap_or_default());
            }
            if let Some([top, max]) = e.scroll {
                n["scroll"] = json!({"top": r1(top), "max": r1(max)});
            }
            n
        })
        .collect();
    crate::tree::sort(&mut nodes);
    let panels = crate::tree::panels(&nodes, target.scale);
    out.0 = Some(json!({"frame": frame.0, "window": {"w": r1(size.x), "h": r1(size.y)}, "scale": target.scale, "nodes": nodes, "panels": panels, "composite": false, "substrate": "blitz"}));
}

/// What raea-runtime-js's accessibility computation asks of the Blitz page: the document's computed style and its controls'
/// state, by the wire's element ids.
struct Facts<'a> {
    doc: &'a blitz_dom::BaseDocument,
    nodes: &'a raea_sys_page::ops::Nodes,
}

impl Facts<'_> {
    fn node(&self, id: u64) -> Option<&blitz_dom::Node> {
        self.nodes.by_id.get(&id).and_then(|n| self.doc.get_node(*n))
    }
}

impl raea_sys_a11y::Facts<u64> for Facts<'_> {
    fn display_none(&self, id: u64) -> bool {
        self.node(id).and_then(|n| n.primary_styles()).is_some_and(|s| s.get_box().display.is_none())
    }
    fn invisible(&self, id: u64) -> bool {
        use style::computed_values::visibility::T as V;
        self.node(id).and_then(|n| n.primary_styles()).is_some_and(|s| matches!(s.clone_visibility(), V::Hidden | V::Collapse))
    }
    fn natively_disabled(&self, _id: u64) -> bool {
        false
    }
    fn editor_value(&self, id: u64) -> Option<String> {
        self.node(id).and_then(|n| n.element_data()).and_then(|e| e.text_input_data()).map(|t| t.editor.raw_text().to_string())
    }
    fn tabbable(&self, id: u64) -> bool {
        self.node(id).is_some_and(|n| n.is_focussable())
    }
    fn checked(&self, id: u64) -> bool {
        self.node(id).and_then(|n| n.element_data()).and_then(|e| e.checkbox_input_checked()).unwrap_or(false)
    }
    fn test_id(&self, id: u64) -> Option<String> {
        self.nodes.props.get(&id).and_then(|p| p.get("data-testid")).and_then(Value::as_str).map(str::to_string)
    }
}

/// The `a11y` method on the page: each element with a data-testid as AccessKit has it, by the computation bevy_ui's page
/// uses (raea-runtime-js's a11y.rs), over the document's elements and the props React set on them.
pub fn a11y(world: &mut World) -> Value {
    if !world.contains_resource::<HostPage>() {
        return json!({"nodes": {}});
    }
    let page = world.resource::<HostPage>().0;
    let roots = world.non_send::<Roots>();
    let docs = world.non_send::<Docs>();
    let (Some(r), Some(doc)) = (roots.0.iter().find(|r| r.page == page), docs.0.get(&page)) else { return json!({"nodes": {}}) };
    let n = &r.nodes;
    let mut els = std::collections::HashMap::new();
    let mut tops = std::collections::HashMap::new();
    for (&id, &node) in &n.by_id {
        let Some(dn) = doc.get_node(node) else { continue };
        let kind = n.kind(id).unwrap_or("div");
        let props = n.props.get(&id).cloned().unwrap_or_default();
        let tag = match kind {
            "editableText" if props.get("multiline") == Some(&Value::Bool(true)) => "textarea",
            "editableText" => "input",
            k => k,
        };
        let text = (kind == "text").then(|| dn.text_content());
        let title = (kind == "svg")
            .then(|| dn.children.iter().filter_map(|c| doc.get_node(*c)).find(|c| c.element_data().is_some_and(|e| &*e.name.local == "title")).map(|t| t.text_content()))
            .flatten();
        let parent = dn.parent.and_then(|p| n.id_of(p)).filter(|p| n.by_id.contains_key(p));
        let kids = dn.children.iter().filter_map(|c| n.id_of(*c)).filter(|c| n.by_id.contains_key(c)).collect();
        els.insert(id, raea_sys_a11y::El { tag: tag.to_string(), props, parent, kids, text, title });
        tops.insert(id, 0u64);
    }
    let facts = Facts { doc, nodes: n };
    let focus = doc.get_focussed_node_id().and_then(|f| n.id_of(f));
    raea_sys_a11y::records(els, tops, &facts, focus, |k| k)
}

/// The system's faces as Chrome on macOS has them (Times, Helvetica and Courier boosted, Menlo for monospace, CoreText's
/// symbol fallbacks), without raea's bundled ones: the oracle is a Chrome that has none of them.
fn system_fonts() -> blitz_dom::FontContext {
    raea_sys_fonts::context(std::path::Path::new(""))
}
