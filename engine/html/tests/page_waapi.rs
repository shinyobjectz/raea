//! Web Animations on the composition's timeline (jetway #9): element.animate's values on every rendered frame, as
//! the spec times them (delay, fill, iterations, direction, keyframe offsets, the finished promise), and motion's own
//! animate() (motion 12, which uses element.animate and a requestAnimationFrame loop) moving with the frames under the
//! frames clock (C0).
use std::path::Path;

const PAGE: &str = r##"<!doctype html><html><head><meta charset="utf-8"><meta name="raea-clock" content="frames">
<script src="https://cdn.jsdelivr.net/npm/motion@12.23.12/dist/motion.js"></script></head>
<body style="margin:0"><div id="root" data-composition-id="main" data-start="0" data-duration="3" data-width="200" data-height="100"
  style="position:relative;width:200px;height:100px">
<div id="a"></div><div id="b" style="transform:none"></div><div id="c"></div><div id="d"></div><div id="m" style="opacity:0"></div>
</div>
<script>
const $ = (id) => document.getElementById(id);
const log = [];
$("a").animate([{ opacity: 0 }, { opacity: 1 }], { duration: 1000, fill: "forwards" }).finished.then(() => log.push("a finished"));
$("b").animate({ transform: ["translateX(0px)", "translateX(100px)"] }, { duration: 1000, iterations: 2, direction: "alternate" });
$("c").animate([{ opacity: 0.2 }, { opacity: 1 }], { duration: 500, delay: 500, fill: "backwards" });
$("d").animate({ opacity: [0, 1, 0], offset: [0, 0.8, 1] }, 1000);
Motion.animate($("m"), { opacity: [0, 1] }, { duration: 2, ease: "linear" });
const cs = (id, p) => getComputedStyle($(id)).getPropertyValue(p);
window.__state = () => JSON.stringify({ a: cs("a", "opacity"), b: cs("b", "transform"), c: cs("c", "opacity"), d: cs("d", "opacity"), bInline: $("b").style.transform,
  log: log.splice(0) });
window.__motion = () => parseFloat(getComputedStyle($("m")).opacity);
</script></body></html>"##;

#[test]
fn web_animations_follow_the_frames() {
    let dir = std::env::temp_dir().join(format!("raea-page-waapi-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("index.html");
    std::fs::write(&html, PAGE).unwrap();
    let mut p = raea::api::probe(&raea::api::RenderOptions::default(), &html, Path::new(&dir), (30, 1)).expect("the page loads");
    let mut got = Vec::new();
    // every frame in order at 30 fps, as a render steps them (the frames clock runs motion's frame loop once a
    // frame), sampled at these
    let samples = [0.2, 0.8, 1.2, 2.5];
    for f in 0..=75 {
        let t = f as f64 / 30.0;
        p.comp.seek(t);
        let Some(&t) = samples.iter().find(|s: &&f64| ((**s * 30.0).round() as i64) == f) else { continue };
        let t: f64 = t;
        let _ = p.comp.doc.take_messages();
        p.comp.doc.eval("__blitz_send_message(window.__state()); __blitz_send_message(String(window.__motion()))");
        let mut msgs = p.comp.doc.take_messages();
        let m: f64 = msgs.pop().expect("motion's opacity").parse().unwrap();
        got.push(format!("{t}: {}", msgs.pop().expect("the state")));
        // motion's 0 -> 1 over 2 s from its first frame (1/30 s), its last value kept
        let want_m = ((t - 1.0 / 30.0) / 2.0).clamp(0.0, 1.0);
        assert!((m - want_m).abs() <= 0.02, "motion's opacity at {t}: {m}, not ~{want_m}");
    }
    let _ = std::fs::remove_dir_all(&dir);
    let got = got.join("\n");
    let want = [
        // a: 0 -> 1 over 1 s, held after; b: 0 -> 100px and back (computed as a matrix); c: held at 0.2 before its
        // delay (fill backwards), then 0.2 -> 1; d: 0 -> 1 by 80%, back to 0. bInline: the page's own inline transform,
        // untouched (animations sit above it)
        r#"0.2: {"a":"0.2","b":"matrix(1, 0, 0, 1, 20, 0)","c":"0.2","d":"0.25","bInline":"none","log":[]}"#,
        r#"0.8: {"a":"0.8","b":"matrix(1, 0, 0, 1, 80, 0)","c":"0.68","d":"1","bInline":"none","log":[]}"#,
        // a finished (its promise resolved); b's second, reversed iteration; c and d over (no fill): back to 1
        r#"1.2: {"a":"1","b":"matrix(1, 0, 0, 1, 80, 0)","c":"1","d":"1","bInline":"none","log":["a finished"]}"#,
        // b over: the page's own transform (none)
        r#"2.5: {"a":"1","b":"none","c":"1","d":"1","bInline":"none","log":[]}"#,
    ];
    assert_eq!(got, want.join("\n"));
}

/// The default (settled) clock: `linear()` easings (motion's springs) follow their points, and an animation that
/// finishes keeps its last value. motion writes that value from its requestAnimationFrame loop after the animation's
/// finish event and cancels the animation; a settled page has no frame loop between frames, so the engine gives it one
/// turn a frame, after the finish events, with performance.now() at the frame's time. Without it `m` snapped back.
const SETTLED: &str = r##"<!doctype html><html><head><meta charset="utf-8">
<script src="https://cdn.jsdelivr.net/npm/motion@12.23.12/dist/motion.js"></script></head>
<body style="margin:0"><div id="root" data-composition-id="main" data-start="0" data-duration="2" data-width="200" data-height="100"
  style="position:relative;width:200px;height:100px">
<div id="s"></div><div id="h"></div><div id="m" style="opacity:0"></div>
</div>
<script>
const $ = (id) => document.getElementById(id);
$("s").animate({ opacity: [0, 1] }, { duration: 1000, easing: "linear(0, 0.25 25%, 1 50%, 0.5)", fill: "forwards" });
// a motion-like finish: on finish, the last value written inline from the next frame, the animation cancelled
const a = $("h").animate({ opacity: [0, 1] }, { duration: 500 });
a.onfinish = () => requestAnimationFrame(() => { $("h").style.opacity = "0.75"; a.cancel(); });
// motion's element.animate path (opacity); its JavaScript loop (x, springs it can't express) runs in real time while a
// settled page loads, done before the first frame, as any timer
Motion.animate($("m"), { opacity: [0, 1] }, { duration: 0.5, ease: "linear" });
const cs = (id, p) => getComputedStyle($(id)).getPropertyValue(p);
window.__state = () => JSON.stringify({ s: cs("s", "opacity"), h: cs("h", "opacity"), m: ((o) => o > 0.1 && o < 0.3 ? "early" : String(o))(parseFloat(cs("m", "opacity"))) });
</script></body></html>"##;

#[test]
fn settled_animations_keep_their_last_values() {
    let dir = std::env::temp_dir().join(format!("raea-page-waapi-settled-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("index.html");
    std::fs::write(&html, SETTLED).unwrap();
    let mut p = raea::api::probe(&raea::api::RenderOptions::default(), &html, Path::new(&dir), (30, 1)).expect("the page loads");
    let mut got = Vec::new();
    for t in [0.125, 0.75, 1.0, 1.5] {
        p.comp.seek(t);
        let _ = p.comp.doc.take_messages();
        p.comp.doc.eval("__blitz_send_message(window.__state())");
        got.push(format!("{t}: {}", p.comp.doc.take_messages().pop().expect("the state")));
    }
    let _ = std::fs::remove_dir_all(&dir);
    let got = got.join("\n");
    let want = [
        // s: 0.125 of the way to 25% is half of 0.25; at 75%, halfway from 1 (50%) to 0.5 (100%). h: still animating,
        // then the page's 0.75 once it finished. m: 0 -> 1 over 0.5 s (from its first frame, early on), then 1, not back to 0
        r#"0.125: {"s":"0.125","h":"0.25","m":"early"}"#,
        r#"0.75: {"s":"0.75","h":"0.75","m":"1"}"#,
        r#"1: {"s":"0.5","h":"0.75","m":"1"}"#,
        r#"1.5: {"s":"0.5","h":"0.75","m":"1"}"#,
    ]
    .join("\n");
    assert_eq!(got, want);
}
