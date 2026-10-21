//! IntersectionObserver from the page's own layout (jetway #4), checked on every rendered frame: the first
//! observation notifies, then each threshold or isIntersecting change, as the spec has it. A GSAP timeline slides a
//! box across the viewport's right edge; each frame's callbacks are logged with the frame's time.
use std::path::Path;

const PAGE: &str = r##"<!doctype html><html><head><meta charset="utf-8">
<script src="https://cdn.jsdelivr.net/npm/gsap@3.14.2/dist/gsap.min.js"></script></head>
<body style="margin:0"><div id="root" data-composition-id="main" data-start="0" data-duration="2" data-width="400" data-height="200"
  style="position:relative;width:400px;height:200px;overflow:hidden">
<div id="box" style="position:absolute;left:300px;top:50px;width:100px;height:100px;background:#39f"></div>
<div id="far" style="position:absolute;left:1000px;top:0;width:10px;height:10px"></div>
</div>
<script>
const log = [];
window.__log = log;
const io = new IntersectionObserver((entries) => {
  for (const e of entries) log.push([e.target.id, e.isIntersecting, Math.round(e.intersectionRatio * 100) / 100]);
}, { threshold: [0, 0.5, 1] });
io.observe(document.getElementById("box"));
io.observe(document.getElementById("far"));
const tl = gsap.timeline({ paused: true });
// x: 0 -> 200 over 2 s: the box (300..400) slides to 500..600, out past the right edge (400)
tl.to("#box", { x: 200, duration: 2, ease: "none" });
window.__timelines = { main: tl };
</script></body></html>"##;

#[test]
fn intersection_observer_follows_each_frame() {
    let dir = std::env::temp_dir().join(format!("raea-page-io-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("index.html");
    std::fs::write(&html, PAGE).unwrap();
    let mut p = raea::api::probe(&raea::api::RenderOptions::default(), &html, Path::new(&dir), (30, 1)).expect("the page loads");
    let mut seen = Vec::new();
    for t in [0.0, 0.2, 0.6, 1.0, 1.2, 1.6, 2.0] {
        p.comp.seek(t);
        let _ = p.comp.doc.take_messages();
        p.comp.doc.eval("__blitz_send_message(JSON.stringify(window.__log.splice(0)))");
        let got = p.comp.doc.take_messages().pop().expect("the log");
        seen.push(format!("{t}: {got}"));
    }
    let _ = std::fs::remove_dir_all(&dir);
    let seen = seen.join("\n");
    // load: both observed, the box fully in, the far one out (both notified: a first observation always is). The
    // box's ratio falls as x grows: 0.2 s -> x 20 (80% in: threshold index 3 -> 2), 0.6 s -> x 60 (40%: index 1),
    // 1.0 s -> x 100 (0%, touching the edge: still intersecting, index still 1: nothing), 1.2 s -> not intersecting
    let want = [
        "0: [[\"box\",true,1],[\"far\",false,0]]",
        "0.2: [[\"box\",true,0.8]]",
        "0.6: [[\"box\",true,0.4]]",
        "1: []",
        "1.2: [[\"box\",false,0]]",
        "1.6: []",
        "2: []",
    ];
    assert_eq!(seen, want.join("\n"), "callbacks frame by frame");
}
