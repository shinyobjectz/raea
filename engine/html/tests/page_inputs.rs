//! Form controls' value property apart from their value attribute, as in Chrome (jetway #10): setting .value
//! changes what the control shows and reads back, not the attribute or defaultValue; a control whose value was set
//! keeps it when its default changes (the dirty value flag). React's controlled inputs rely on both.
use blitz_dom::Document as _;
use std::path::Path;

const PAGE: &str = r##"<!doctype html><html><head><style>body{margin:0;font:20px sans-serif} input,textarea{display:block;width:180px;height:30px;font:20px sans-serif;margin:4px;border:1px solid #000;padding:2px}</style></head><body>
<div id="root" data-composition-id="t" data-duration="1" data-width="300" data-height="200" style="width:300px;height:200px">
<input id="a" value="attr-a">
<input id="b">
<textarea id="c">text-c</textarea>
<input id="d" value="dflt">
</div>
<script>
const $ = (id) => document.getElementById(id);
$("a").value = "prop-a";
$("b").value = "prop-b";
$("c").value = "prop-c";
$("d").value = "dirty";
$("d").defaultValue = "dflt2";
const r = {};
for (const id of ["a", "b", "c", "d"]) r[id] = [$(id).value, $(id).getAttribute("value"), $(id).defaultValue];
window.__r = r;
</script></body></html>
"##;

#[test]
fn value_property_is_not_the_attribute() {
    let dir = std::env::temp_dir().join(format!("raea-page-input-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("index.html");
    std::fs::write(&html, PAGE).unwrap();
    let mut p = raea::api::probe(&raea::api::RenderOptions::default(), &html, Path::new(&dir), (30, 1)).expect("the page loads");
    let _ = p.comp.doc.take_messages();
    p.comp.doc.eval("__blitz_send_message(JSON.stringify(window.__r))");
    let got = p.comp.doc.take_messages().pop().expect("the results");
    let _ = std::fs::remove_dir_all(&dir);
    // [value, getAttribute("value"), defaultValue] for each, as chrome-headless-shell 152 gives them
    let chrome = r#"{"a":["prop-a","attr-a","attr-a"],"b":["prop-b",null,""],"c":["prop-c",null,"text-c"],"d":["dirty","dflt2","dflt2"]}"#;
    assert_eq!(got, chrome);
}

/// A native input event (what typing into a field makes) reaches listeners on the root container in both phases, as
/// React's root listener needs (it listens for input and change on its container, capture and bubble).
#[test]
fn native_input_events_reach_root_listeners() {
    const EVENTS: &str = r##"<!doctype html><html><body>
<div id="root" data-composition-id="t" data-duration="1" data-width="200" data-height="100" style="width:200px;height:100px">
<form id="f"><input id="in" value="x"></form></div>
<script>
const log = [];
window.__log = log;
const root = document.getElementById("root");
root.addEventListener("input", (e) => log.push("root-capture:" + e.target.id + ":" + e.eventPhase), true);
root.addEventListener("input", (e) => log.push("root-bubble:" + e.target.id + ":" + e.eventPhase + ":" + (e instanceof Event)));
document.getElementById("in").addEventListener("input", (e) => log.push("target:" + e.type));
</script></body></html>"##;
    let dir = std::env::temp_dir().join(format!("raea-page-input-events-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("index.html");
    std::fs::write(&html, EVENTS).unwrap();
    let mut p = raea::api::probe(&raea::api::RenderOptions::default(), &html, Path::new(&dir), (30, 1)).expect("the page loads");
    let input = p.comp.doc.inner().query_selector("#in").ok().flatten().expect("the input");
    p.comp.doc.dispatch_dom_event(blitz_traits::events::DomEvent::new(
        input,
        blitz_traits::events::DomEventData::Input(blitz_traits::events::BlitzInputEvent { value: "typed".into() }),
    ));
    let _ = p.comp.doc.take_messages();
    p.comp.doc.eval("__blitz_send_message(JSON.stringify(window.__log))");
    let got = p.comp.doc.take_messages().pop().expect("the log");
    let _ = std::fs::remove_dir_all(&dir);
    // capture at the root (phase 1), the target (2), bubbling at the root (3), as in a browser
    assert_eq!(got, r#"["root-capture:in:1","target:input","root-bubble:in:3:true"]"#);
}
