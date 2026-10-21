//! A CSS animation on an element a script makes after the page loaded starts when that element is first styled, as
//! a browser starts it when the element appears (jetway #8: an app's entrance animations, tw-animate's animate-in,
//! were seeked from the composition's start, as the page's own elements are). The page's own elements keep their
//! deterministic origin (the start of their clip), so parity and seek determinism don't move.
use std::path::Path;

// two boxes over white, each red at full opacity: the page's own at x 0-80, the late one at x 100-180
const PAGE: &str = r#"<!doctype html><html><head><style>
@keyframes fade { from { opacity: 0 } to { opacity: 1 } }
.a { animation: fade 1s linear forwards; position: absolute; top: 10px; width: 80px; height: 80px; background: rgb(255, 0, 0); }
</style></head><body style="margin:0">
<div id="root" data-composition-id="t" data-duration="3" data-width="200" data-height="100" style="position:relative;width:200px;height:100px;background:#fff">
<div id="own" class="a" style="left:0"></div></div>
</body></html>"#;

/// How opaque the red box at x is in the frame: its green channel over white is 255 * (1 - opacity).
fn opacity_at(px: &[u8], w: usize, x: usize) -> f64 {
    let g = px[(50 * w + x) * 4 + 1] as f64;
    1.0 - g / 255.0
}

#[test]
fn late_elements_start_their_animations_when_they_appear() {
    let dir = std::env::temp_dir().join(format!("raea-page-anim-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("index.html");
    std::fs::write(&html, PAGE).unwrap();
    let o = raea::api::RenderOptions { scale: 1.0, ..Default::default() };
    let mut p = raea::api::probe(&o, &html, Path::new(&dir), (30, 1)).expect("the page loads");
    // made by a script after the page loaded (an app mounting a screen); first styled at the first frame below
    p.comp.doc.eval("const d = document.createElement('div'); d.className = 'a'; d.style.left = '100px'; document.getElementById('root').appendChild(d);");
    let (w, _) = p.comp.output_size();
    let mut stage = raea::stage::Stage::new(p.comp);
    let mut px = Vec::new();
    // frames rendered in order, as a render does
    stage.frame(0.6, &mut px);
    let late_06 = opacity_at(&px, w as usize, 140);
    stage.frame(1.0, &mut px);
    let (late_10, own_10) = (opacity_at(&px, w as usize, 140), opacity_at(&px, w as usize, 40));
    stage.frame(1.6, &mut px);
    let late_16 = opacity_at(&px, w as usize, 140);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(late_06.abs() < 0.02, "the late element at its first frame: {late_06}");
    assert!((late_10 - 0.4).abs() < 0.02, "the late element 0.4 s after it appeared: {late_10}");
    assert!((late_16 - 1.0).abs() < 0.02, "the late element done: {late_16}");
    // the page's own element: from the composition's start, as before
    assert!((own_10 - 1.0).abs() < 0.02, "the page's own element at 1 s: {own_10}");
}
