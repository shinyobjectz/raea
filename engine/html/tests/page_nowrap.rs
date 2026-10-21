//! `white-space: nowrap` holds when the line starts with an inline-block (the animatic's chat chips): Parley started
//! every paragraph in Wrap mode until its first text, so it broke after the box and the text dropped to a second line.
//! Chrome 152 keeps the nowrap paragraph on one line (25 px tall here) and wraps the normal one onto two (49 px).
use std::path::Path;

const PAGE: &str = r##"<!doctype html><html><head><style>
body{margin:0} .txt{width:300px;font:20px/24px sans-serif;white-space:nowrap;overflow:hidden}
.chip{display:inline-block;width:120px;height:20px;background:#ccd}
</style></head><body>
<div id="root" data-composition-id="t" data-duration="1" data-width="400" data-height="200" style="width:400px;height:200px">
<div class="txt" id="a"><span class="chip" id="chip"></span><span id="after">can you peel the brief? please, it is long</span></div>
<div class="txt" id="b" style="white-space:normal"><span class="chip" id="chip2"></span><span id="after2">can you peel the brief? please, it is long</span></div>
</div>
<script>
const R = (id) => { const b = document.getElementById(id).getBoundingClientRect(); return [Math.round(b.top), Math.round(b.height)]; };
window.__r = { chip: R("chip"), after: R("after"), a: R("a"), chip2: R("chip2"), after2: R("after2"), b: R("b") };
</script></body></html>
"##;

#[test]
fn nowrap_after_an_inline_block_stays_on_one_line() {
    let dir = std::env::temp_dir().join(format!("raea-page-nowrap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("index.html");
    std::fs::write(&html, PAGE).unwrap();
    let mut p = raea::api::probe(&raea::api::RenderOptions::default(), &html, Path::new(&dir), (30, 1)).expect("the page loads");
    let _ = p.comp.doc.take_messages();
    p.comp.doc.eval("__blitz_send_message(JSON.stringify(window.__r))");
    let r: serde_json::Value = serde_json::from_str(&p.comp.doc.take_messages().pop().expect("the rects")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let at = |k: &str| (r[k][0].as_i64().unwrap(), r[k][1].as_i64().unwrap());
    // nowrap: one line, the text beside the chip
    assert!(at("a").1 < 32, "the nowrap paragraph is one line: {r}");
    assert!((at("after").0 - at("chip").0).abs() < 8, "the text beside the chip, not under it: {r}");
    // the same without nowrap wraps (as in Chrome), so the check above isn't vacuous
    assert!(at("b").1 > 40, "the normal paragraph wraps onto two lines: {r}");
}
