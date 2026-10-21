//! position: fixed as Chrome lays it out (jetway #7): the viewport is the containing block (not a positioned
//! ancestor), unless an ancestor has a transform, a filter or will-change: transform, which then is. Static position for auto offsets,
//! percentages of the viewport, inset: 0 filling it. The expected rects are chrome-headless-shell 152's for this page.
use std::path::Path;

const PAGE: &str = r##"<!doctype html><html><head><style>
body { margin: 0 }
.f { position: fixed; background: #f60 }
</style></head><body>
<div id="root" data-composition-id="t" data-duration="1" data-width="800" data-height="600" style="position:relative;width:800px;height:600px">
<div style="position:relative;left:50px;top:40px;width:300px;height:200px;padding:10px">
  <div class="f" id="inrel" style="left:5px;top:6px;width:20px;height:10px"></div>
  <div class="f" id="auto" style="width:20px;height:10px"></div>
  <div class="f" id="pct" style="right:0;bottom:0;width:10%;height:10%"></div>
</div>
<div style="position:absolute;left:400px;top:300px;width:200px;height:100px">
  <div class="f" id="inset" style="inset:0;background:rgba(0,0,255,.15)"></div>
</div>
<div style="position:absolute;left:100px;top:350px;width:200px;height:100px;transform:translate(10px,10px)">
  <div class="f" id="intx" style="left:5px;top:5px;width:30px;height:30px"></div>
  <div class="f" id="intxinset" style="inset:0;opacity:.5"></div>
</div>
<div style="position:absolute;left:450px;top:450px;width:120px;height:80px;will-change:transform">
  <div class="f" id="inwill" style="left:10px;bottom:0;width:40px;height:20px;background:#0a0"></div>
</div>
<div style="position:absolute;left:600px;top:50px;width:100px;height:100px;filter:blur(0)">
  <div class="f" id="infilter" style="right:0;top:0;width:10px;height:10px"></div>
</div>
</div>
<script>
const r = {};
for (const id of ["inrel", "auto", "pct", "inset", "intx", "intxinset", "infilter", "inwill"]) {
  const b = document.getElementById(id).getBoundingClientRect();
  r[id] = [b.x, b.y, b.width, b.height].map((v) => Math.round(v * 100) / 100);
}
window.__rects = r;
</script></body></html>
"##;

/// id -> [x, y, width, height] in Chrome 152
const CHROME: &str = r#"{"inrel": [5, 6, 20, 10], "auto": [60, 50, 20, 10], "pct": [720, 540, 80, 60], "inset": [0, 0, 800, 600], "intx": [115, 365, 30, 30], "intxinset": [110, 360, 200, 100], "infilter": [690, 50, 10, 10], "inwill": [460, 510, 40, 20]}"#;

#[test]
fn fixed_boxes_as_in_chrome() {
    let dir = std::env::temp_dir().join(format!("raea-page-fixed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("index.html");
    std::fs::write(&html, PAGE).unwrap();
    let mut p = raea::api::probe(&raea::api::RenderOptions::default(), &html, Path::new(&dir), (30, 1)).expect("the page loads");
    let _ = p.comp.doc.take_messages();
    p.comp.doc.eval("__blitz_send_message(JSON.stringify(window.__rects))");
    let msgs = p.comp.doc.take_messages();
    let _ = std::fs::remove_dir_all(&dir);
    let got: serde_json::Value = serde_json::from_str(msgs.last().expect("the page sent its rects")).unwrap();
    let want: serde_json::Value = serde_json::from_str(CHROME).unwrap();
    let mut bad = Vec::new();
    for (id, w) in want.as_object().unwrap() {
        let g = &got[id];
        let close = (0..4).all(|i| (g[i].as_f64().unwrap_or(f64::NAN) - w[i].as_f64().unwrap()).abs() <= 0.02);
        if !close {
            bad.push(format!("{id}: raea {g}, Chrome {w}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("; "));
}
