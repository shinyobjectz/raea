//! Script-made events on a page behave as in Chrome: an AbortController aborts, a plain EventTarget dispatches,
//! a listener given a signal is removed by it, and a bubbling CustomEvent runs capture, target and bubble phases in
//! order. The page path's shim (gsap_shim.js) once replaced Event with its own while plain EventTargets kept the
//! bootstrap's dispatch, which rejected it: abort() and every EventTarget's dispatchEvent threw (jetway #5,
//! rendering Jetflow's React app: React's unmount cleanups abort).
use std::path::Path;

const PAGE: &str = r#"<!doctype html><html><body>
<div id="root" data-composition-id="t" data-duration="1" data-width="200" data-height="100" style="width:200px;height:100px">
<div id="a"><div id="b"><span id="c">x</span></div></div></div>
<script>
const r = {};
const t = (k, f) => { try { r[k] = String(f()); } catch (e) { r[k] = "threw: " + e.message; } };
t("abort", () => { const c = new AbortController(); let n = 0; c.signal.addEventListener("abort", () => n++); c.abort();
  return n + "/" + c.signal.aborted + "/" + c.signal.reason.name; });
t("plain", () => { const et = new EventTarget(); let n = 0; et.addEventListener("x", () => n++, { once: true });
  et.dispatchEvent(new Event("x")); et.dispatchEvent(new Event("x")); return n; });
t("signal", () => { const et = new EventTarget(), c = new AbortController(); let n = 0;
  et.addEventListener("y", () => n++, { signal: c.signal }); c.abort(); et.dispatchEvent(new Event("y")); return n; });
t("bubble", () => { const o = [], a = document.getElementById("a"), b = document.getElementById("b"), c = document.getElementById("c");
  a.addEventListener("ping", () => o.push("a-cap"), true); a.addEventListener("ping", () => o.push("a"));
  b.addEventListener("ping", (e) => o.push("b:" + e.detail)); c.addEventListener("ping", () => o.push("c"));
  c.dispatchEvent(new CustomEvent("ping", { bubbles: true, detail: 7 })); return o.join(","); });
t("stop", () => { const o = [], b = document.getElementById("b"), c = document.getElementById("c");
  b.addEventListener("pong", () => o.push("b")); c.addEventListener("pong", (e) => { o.push("c"); e.stopPropagation(); });
  c.dispatchEvent(new Event("pong", { bubbles: true })); return o.join(","); });
window.__eventResults = r;
</script></body></html>"#;

fn results() -> serde_json::Value {
    let dir = std::env::temp_dir().join(format!("raea-page-events-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("index.html");
    std::fs::write(&html, PAGE).unwrap();
    let o = raea::api::RenderOptions::default();
    let mut p = raea::api::probe(&o, &html, Path::new(&dir), (30, 1)).expect("the page loads");
    // (the loader drains the page's messages while it loads, so the results are asked for after)
    let _ = p.comp.doc.take_messages();
    p.comp.doc.eval("__blitz_send_message(JSON.stringify(window.__eventResults))");
    let msgs = p.comp.doc.take_messages();
    let _ = std::fs::remove_dir_all(&dir);
    serde_json::from_str(msgs.last().expect("the page sent its results")).unwrap()
}

#[test]
fn script_events_dispatch_as_in_chrome() {
    let r = results();
    // (each expectation is Chrome's)
    assert_eq!(r["abort"], "1/true/AbortError", "AbortController.abort");
    assert_eq!(r["plain"], "1", "a plain EventTarget, a once listener");
    assert_eq!(r["signal"], "0", "a listener removed by its signal");
    assert_eq!(r["bubble"], "a-cap,c,b:7,a", "capture at the ancestor, then the target, then bubbling");
    assert_eq!(r["stop"], "c", "stopPropagation at the target");
}

const HASH_PAGE: &str = r##"<!doctype html><html><body>
<div id="root" data-composition-id="t" data-duration="1" data-width="200" data-height="100" style="width:200px;height:100px"></div>
<script>
const r = { fired: [] };
const t = (k, f) => { try { r[k] = String(f()); } catch (e) { r[k] = "threw: " + e.message; } };
t("hce", () => { const e = new HashChangeEvent("hashchange", { oldURL: "a", newURL: "b" }); return e.type + "/" + e.oldURL + "/" + e.newURL + "/" + (e instanceof Event); });
t("pse", () => new PopStateEvent("popstate", { state: { n: 1 } }).state.n);
t("pkc", () => typeof PublicKeyCredential + "/" + typeof PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable);
t("pkcNew", () => { new PublicKeyCredential(); return "constructed"; });
PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable().then((v) => { r.uvpa = String(v); });
addEventListener("hashchange", (e) => r.fired.push(e.newURL.split("#")[1] + "<" + (e.oldURL.split("#")[1] || "")));
history.pushState(null, "", "#pushed");
location.hash = "step-2";
r.syncFired = r.fired.length;
r.hash = location.hash;
window.__hashResults = r;
</script></body></html>"##;

#[test]
fn hash_changes_and_credentials_as_in_chrome() {
    let dir = std::env::temp_dir().join(format!("raea-page-hash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("index.html");
    std::fs::write(&html, HASH_PAGE).unwrap();
    let o = raea::api::RenderOptions::default();
    let mut p = raea::api::probe(&o, &html, Path::new(&dir), (30, 1)).expect("the page loads");
    // a frame later: the hashchange task and the promise have run
    p.comp.seek(0.1);
    let _ = p.comp.doc.take_messages();
    p.comp.doc.eval("__blitz_send_message(JSON.stringify(window.__hashResults))");
    let msgs = p.comp.doc.take_messages();
    let _ = std::fs::remove_dir_all(&dir);
    let r: serde_json::Value = serde_json::from_str(msgs.last().expect("the page sent its results")).unwrap();
    assert_eq!(r["hce"], "hashchange/a/b/true", "HashChangeEvent");
    assert_eq!(r["pse"], "1", "PopStateEvent");
    assert_eq!(r["pkc"], "function/function", "PublicKeyCredential and its capability check");
    assert_eq!(r["pkcNew"], "threw: Illegal constructor", "PublicKeyCredential isn't constructible");
    assert_eq!(r["uvpa"], "false", "no platform authenticator");
    assert_eq!(r["hash"], "#step-2", "location.hash after setting it");
    assert_eq!(r["syncFired"], 0, "hashchange fires as a task, not inside the setter");
    // pushState fires nothing; the hash change fires once, with the URLs before and after it
    assert_eq!(r["fired"], serde_json::json!(["step-2<pushed"]), "hashchange events");
}
