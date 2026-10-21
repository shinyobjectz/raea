//! The Streams API on a page (jetway #1): ReadableStream, WritableStream, TransformStream (web-streams-polyfill 4.3.0,
//! loaded the first time a page reaches for one), TextEncoderStream and TextDecoderStream on top, Blob.stream(), a
//! Response's body as a stream and a Response made from one.
//! Each behaviour is checked against chrome-headless-shell 152 on the same page.
use std::path::Path;

const PAGE: &str = r##"<!doctype html><html><body>
<div id="root" data-composition-id="t" data-duration="1" data-width="100" data-height="100" style="width:100px;height:100px"></div>
<script>
const s = {};
window.__s = s;
const all = [];
const t = (k, f) => all.push(Promise.resolve().then(f).then((v) => { s[k] = v; }, (e) => { s[k] = "threw: " + e.name; }));
const collect = async (rs) => { const r = rs.getReader(), out = []; for (;;) { const { done, value } = await r.read(); if (done) return out; out.push(value); } };
t("types", () => [typeof ReadableStream, typeof WritableStream, typeof TransformStream, typeof TextEncoderStream, typeof TextDecoderStream, typeof CountQueuingStrategy].join(","));
t("pull", async () => { let n = 0; const rs = new ReadableStream({ pull(c) { n < 3 ? c.enqueue(n++) : c.close(); } }); return (await collect(rs)).join(","); });
t("transform", async () => {
  const rs = new ReadableStream({ start(c) { c.enqueue("ab"); c.enqueue("cd"); c.close(); } });
  return (await collect(rs.pipeThrough(new TransformStream({ transform(ch, c) { c.enqueue(ch.toUpperCase()); } })))).join("|");
});
t("pipeTo", async () => {
  const got = [];
  const ws = new WritableStream({ write(ch) { got.push(ch * 2); } }, new CountQueuingStrategy({ highWaterMark: 1 }));
  await new ReadableStream({ start(c) { [1, 2, 3].forEach((x) => c.enqueue(x)); c.close(); } }).pipeTo(ws);
  return got.join(",");
});
t("backpressure", async () => {
  const ws = new WritableStream({ write() { return new Promise((r) => setTimeout(r, 0)); } }, new CountQueuingStrategy({ highWaterMark: 2 }));
  const w = ws.getWriter(); const sizes = [w.desiredSize]; w.write(1); sizes.push(w.desiredSize); w.write(2); sizes.push(w.desiredSize); w.write(3); sizes.push(w.desiredSize);
  await w.close(); return sizes.join(",");
});
t("tee", async () => { const [a, b] = new ReadableStream({ start(c) { c.enqueue(1); c.enqueue(2); c.close(); } }).tee(); return (await collect(a)).join(",") + "/" + (await collect(b)).join(","); });
t("text", async () => {
  const bytes = new TextEncoder().encode("héllo ☃");
  const rs = new ReadableStream({ start(c) { c.enqueue(bytes.slice(0, 2)); c.enqueue(bytes.slice(2, 7)); c.enqueue(bytes.slice(7)); c.close(); } });
  return (await collect(rs.pipeThrough(new TextDecoderStream()))).join("");
});
t("encode", async () => { const rs = new ReadableStream({ start(c) { c.enqueue("é"); c.close(); } }); return Array.from((await collect(rs.pipeThrough(new TextEncoderStream())))[0]).join(","); });
t("cancel", async () => { let why; const rs = new ReadableStream({ cancel(r) { why = r; } }); await rs.cancel("stop"); return why + "," + rs.locked; });
t("respstream", async () => Array.from(new Uint8Array(await new Response(new ReadableStream({ start(c) { c.enqueue(new Uint8Array([1, 2])); c.enqueue(new Uint8Array([3])); c.close(); } })).arrayBuffer())).join(","));
t("respbody", async () => Array.from((await collect(new Response("hi").body)).reduce((a, c) => a.concat(Array.from(c)), [])).join(","));
t("blob", async () => (await collect(new Blob(["ab", "c"]).stream())).map((c) => c.length).join(","));
Promise.all(all).then(() => { document.title = JSON.stringify(s); });
</script></body></html>
"##;

const CHROME: &str = r#"{"types":"function,function,function,function,function,function","cancel":"stop,false","respbody":"104,105","pull":"0,1,2","tee":"1,2/1,2","encode":"195,169","pipeTo":"2,4,6","transform":"AB|CD","text":"héllo ☃","respstream":"1,2,3","blob":"3","backpressure":"2,1,0,-1"}"#;

#[test]
fn streams_behave_as_in_chrome() {
    let dir = std::env::temp_dir().join(format!("raea-page-streams-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("index.html");
    std::fs::write(&html, PAGE).unwrap();
    let mut p = raea::api::probe(&raea::api::RenderOptions::default(), &html, Path::new(&dir), (30, 1)).expect("the page loads");
    let _ = p.comp.doc.take_messages();
    p.comp.doc.eval("__blitz_send_message(JSON.stringify(window.__s))");
    let got: serde_json::Value = serde_json::from_str(&p.comp.doc.take_messages().pop().expect("the results")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let want: serde_json::Value = serde_json::from_str(CHROME).unwrap();
    assert_eq!(got, want);
}
