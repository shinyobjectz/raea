//! A scripted <iframe> on a host page (docs/blitz-bevy.feature B5c, iframes 3 and 6): the
//! frame's script runs in a JS context of its own, its message reaches the parent's window with an opaque origin and
//! the frame's contentWindow as its source, and a sandboxed frame's host call is refused (SecurityError), as nothing of
//! the app's is reachable from it. Its own binary: the env var and blitz-dom's FRAME_NET are the process's.
//!
//! The pages speak the wire raw (no @raea/ui), with just enough of a window for frames.js and frame.js: an Event, a
//! dispatchEvent that reports what it got, and a location.

use bevy::prelude::*;
use raea_runtime_dom::host::{self, HostStepPlugin, PageAsks};
use raea_sys_page::{Docs, PageClock, PageSystems};

/// a window's Event, dispatchEvent and location for the raw pages
const WINDOW: &str = r#"
  globalThis.Event = class { constructor(type) { this.type = type; } };
  globalThis.location = { href: "http://localhost/" };
"#;

#[test]
fn a_sandboxed_frame_posts_to_its_parent_and_reaches_nothing_else() {
    let root = std::env::temp_dir().join(format!("raea-frames-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("frame.html"), r#"<!doctype html><html><body><script src="/frame.js"></script></body></html>"#).unwrap();
    // the frame: its window set as @raea/ui's DOM would set it, its first render, then a host call (refused, so it
    // posts the error) and a message with its origin
    std::fs::write(
        root.join("frame.js"),
        format!(
            r#"{WINDOW}
            globalThis.__ui_deliver = (json) => {{
              const m = JSON.parse(json);
              if (m.t === "answer") parent.postMessage({{ refused: m.ok ? null : m.error }}, "*");
              if (m.t === "event" && m.event === "pointerdown") parent.postMessage({{ pressedAt: [m.payload.clientX, m.payload.clientY] }}, "*");
            }};
            __raea_frame.installed();
            __ui_flush(JSON.stringify({{ v: 1, ops: [{{ op: "create", id: 1, kind: "div", props: {{ id: "in-frame", style: {{ height: "80px" }} }} }}, {{ op: "append", parent: 0, child: 1 }}, {{ op: "listen", id: 1, events: ["pointerdown"] }}] }}));
            __ui_call(7, "world.read", JSON.stringify({{}}));
            __ui_emit("send", JSON.stringify("an ask the app must not get"));
            parent.postMessage({{ origin: globalThis.origin, url: location.href }}, "*");
            "#
        ),
    )
    .unwrap();
    // SAFETY: this binary's one test, set before anything reads them
    unsafe {
        std::env::set_var("RAEA_PAGE_IFRAMES", "1");
        std::env::set_var("RAEA_PAGE_ROOT", &root);
    }

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, HostStepPlugin))
        .init_non_send::<Docs>()
        .init_resource::<PageClock>()
        .add_systems(PostUpdate, raea_sys_page::resolve.in_set(PageSystems::Restyle));
    // the parent: an iframe sandboxed to scripts only, and each message its window gets reported as an ask
    let parent = format!(
        r#"{WINDOW}
        globalThis.dispatchEvent = (e) => __ui_emit("got", JSON.stringify({{ data: e.data, origin: e.origin, fromTheFrame: e.source === __raea_frames.window({{ __wid: 1 }}) }}));
        __ui_flush(JSON.stringify({{ v: 1, ops: [{{ op: "create", id: 1, kind: "iframe", props: {{ src: "/frame.html", sandbox: "allow-scripts", style: {{ width: "200px", height: "100px" }} }} }}, {{ op: "append", parent: 0, child: 1 }}, {{ op: "listen", id: 1, events: ["pointerdown"] }}, {{ op: "create", id: 2, kind: "div", props: {{ style: {{ width: "400px", height: "100px" }} }} }}, {{ op: "append", parent: 0, child: 2 }}, {{ op: "listen", id: 2, events: ["pointerdown"] }}] }}));
        // a press below the frame closes it (an artifact's review closed); any other event the parent hears is reported
        globalThis.__ui_deliver = (json) => {{ const m = JSON.parse(json); if (m.t !== "event") return; if (m.payload && m.payload.clientY > 150) __ui_flush(JSON.stringify({{ v: 1, ops: [{{ op: "remove", id: 1 }}] }})); else __ui_emit("parentHeard", JSON.stringify(m.event)); }};
        "#
    );
    let page = host::mount(app.world_mut(), 400, 300, &parent, &[], None).unwrap();
    for _ in 0..8 {
        app.update();
    }

    // a press on the frame, 50, 40 on the page: the frame's element hears it at the frame's 48, 38 (inside the iframe's
    // 2px border), and the parent's listener on the iframe doesn't
    let at = blitz_traits::events::PointerCoords { page_x: 50.0, page_y: 40.0, screen_x: 50.0, screen_y: 40.0, client_x: 50.0, client_y: 40.0 };
    let press = blitz_traits::events::BlitzPointerEvent {
        id: blitz_traits::events::BlitzPointerId::Mouse,
        is_primary: true,
        coords: at,
        button: blitz_traits::events::MouseEventButton::Main,
        buttons: blitz_traits::events::MouseEventButtons::Primary,
        mods: Default::default(),
        details: Default::default(),
        element: Default::default(),
        active_pointers: Default::default(),
    };
    host::input(app.world_mut(), page, blitz_traits::events::UiEvent::PointerDown(press));
    for _ in 0..3 {
        app.update();
    }

    let asks: Vec<(String, serde_json::Value)> = app.world().resource::<PageAsks>().0.iter().filter(|(p, _, _)| *p == page).map(|(_, n, v)| (n.clone(), v.clone())).collect();
    let got: Vec<&serde_json::Value> = asks.iter().filter(|(n, _)| n == "got").map(|(_, v)| v).collect();
    let hello = got.iter().find(|v| v["data"]["origin"].is_string()).unwrap_or_else(|| panic!("no message from the frame: {asks:?}"));
    assert_eq!(hello["data"]["origin"], "null", "a sandboxed frame's origin is opaque");
    assert_eq!(hello["origin"], "null", "its message's origin is opaque");
    assert_eq!(hello["fromTheFrame"], true, "its source is the iframe's contentWindow");
    assert_eq!(hello["data"]["url"], "http://localhost/frame.html");
    let refused = got.iter().find(|v| v["data"]["refused"].is_string()).unwrap_or_else(|| panic!("the frame's host call wasn't answered: {asks:?}"));
    assert!(refused["data"]["refused"].as_str().unwrap().starts_with("SecurityError"), "{refused}");
    let pressed = got.iter().find(|v| v["data"]["pressedAt"].is_array()).unwrap_or_else(|| panic!("the frame didn't hear the press: {asks:?}"));
    assert_eq!(pressed["data"]["pressedAt"], serde_json::json!([48, 38]));
    // the frame's own asks reach no one, and the parent heard nothing of the press
    let all = &app.world().resource::<PageAsks>().0;
    assert!(all.iter().all(|(_, n, _)| n == "got"), "a frame's ask, or the press, reached the parent's app: {all:?}");


    // the iframe removed (a press below it): the frame's root goes, its document and its entity with it, so a page
    // that opens and closes artifacts doesn't keep each one's JS running
    let frame_pages: Vec<Entity> = app.world().non_send::<Docs>().0.keys().copied().filter(|e| *e != page).collect();
    assert_eq!(frame_pages.len(), 1, "the frame's own document, beside the parent's");
    let below = blitz_traits::events::PointerCoords { page_x: 50.0, page_y: 160.0, screen_x: 50.0, screen_y: 160.0, client_x: 50.0, client_y: 160.0 };
    host::input(app.world_mut(), page, blitz_traits::events::UiEvent::PointerDown(blitz_traits::events::BlitzPointerEvent {
        id: blitz_traits::events::BlitzPointerId::Mouse,
        is_primary: true,
        coords: below,
        button: blitz_traits::events::MouseEventButton::Main,
        buttons: blitz_traits::events::MouseEventButtons::Primary,
        mods: Default::default(),
        details: Default::default(),
        element: Default::default(),
        active_pointers: Default::default(),
    }));
    for _ in 0..4 {
        app.update();
    }
    let docs: Vec<Entity> = app.world().non_send::<Docs>().0.keys().copied().collect();
    assert_eq!(docs, vec![page], "the frame's document outlived its iframe");
    assert!(app.world().get_entity(frame_pages[0]).is_err(), "the frame's entity outlived its iframe");

    std::fs::remove_dir_all(&root).unwrap();
}
