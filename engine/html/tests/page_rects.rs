//! getBoundingClientRect with CSS transforms, as Chrome gives it (jetway #6): a transform on the element or on a
//! containing block moves and sizes the rect (the border box's corners through each transform, the rect their bounds).
//! Before, raea gave the untransformed layout box. The expected numbers are chrome-headless-shell 152's for this page.
use std::path::Path;

const PAGE: &str = r##"<!doctype html><html><head><style>
body { margin: 0 }
.b { position: absolute; width: 100px; height: 50px; background: #39f }
</style></head><body>
<div id="root" data-composition-id="t" data-duration="1" data-width="800" data-height="600" style="position:relative;width:800px;height:600px">
<div class="b" id="plain" style="left:10px;top:10px"></div>
<div class="b" id="tx" style="left:10px;top:100px;transform:translate(30px, 5px)"></div>
<div class="b" id="sc" style="left:200px;top:100px;transform:scale(2)"></div>
<div class="b" id="rot" style="left:400px;top:100px;transform:rotate(45deg)"></div>
<div class="b" id="origin" style="left:10px;top:250px;transform:scale(0.5);transform-origin:0 0"></div>
<div style="position:absolute;left:300px;top:300px;width:200px;height:200px;transform:translate(10px,20px) rotate(90deg)">
  <div class="b" id="kid" style="left:20px;top:30px;transform:scale(1.5)"></div>
</div>
<div style="position:absolute;left:600px;top:20px;transform:translateX(-50%)"><span id="inl" style="display:inline-block;width:40px;height:20px">x</span></div>
</div>
<script>
const r = {};
for (const id of ["plain", "tx", "sc", "rot", "origin", "kid", "inl"]) {
  const b = document.getElementById(id).getBoundingClientRect();
  r[id] = [b.x, b.y, b.width, b.height].map((v) => Math.round(v * 100) / 100);
}
window.__rects = r;
</script></body></html>
"##;

/// id -> [x, y, width, height] in Chrome 152
const CHROME: &str = r#"{"plain": [10, 10, 100, 50], "tx": [40, 105, 100, 50], "sc": [150, 75, 200, 100], "rot": [396.97, 71.97, 106.07, 106.07], "origin": [10, 250, 50, 25], "kid": [417.5, 315, 75, 150], "inl": [580, 20, 40, 20]}"#;

#[test]
fn bounding_rects_follow_transforms_as_in_chrome() {
    let dir = std::env::temp_dir().join(format!("raea-page-rects-{}", std::process::id()));
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
