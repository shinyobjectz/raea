//! A page's own clocks follow the frames with the frames clock (C0, `<meta name="raea-clock" content="frames">` or
//! `--clock frames`): Date and performance.now advance with the composition time, a timer fires at the frame it's
//! due, and requestAnimationFrame runs each frame, so an app's own frame loop (motion/react, a game loop) runs as it
//! would in a browser. The default (the composition format's settled clock) runs the timers while the page loads and holds still.
use std::path::Path;

const PAGE: &str = r#"<!doctype html><html><head>META</head><body>
<div id="root" data-composition-id="t" data-duration="3" data-width="200" data-height="100" style="width:200px;height:100px"></div>
<script>
const d0 = Date.now(), p0 = performance.now();
const s = { t500: 0, t1500: 0, raf: 0 };
setTimeout(() => s.t500 = 1, 500); setTimeout(() => s.t1500 = 1, 1500);
const loop = () => { s.raf++; requestAnimationFrame(loop); }; requestAnimationFrame(loop);
window.__clock = () => JSON.stringify({ date: Date.now() - d0, perf: Math.round(performance.now() - p0), ...s });
</script></body></html>"#;

fn states(meta: &str, opts: raea::api::RenderOptions, times: &[f64]) -> Vec<serde_json::Value> {
    // a folder per call: the tests run in parallel, two of them with the same page
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("raea-page-clock-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let html = dir.join("index.html");
    std::fs::write(&html, PAGE.replace("META", meta)).unwrap();
    let mut p = raea::api::probe(&opts, &html, Path::new(&dir), (30, 1)).expect("the page loads");
    let out = times
        .iter()
        .map(|&t| {
            p.comp.seek(t);
            let _ = p.comp.doc.take_messages();
            p.comp.doc.eval("__blitz_send_message(window.__clock())");
            serde_json::from_str(p.comp.doc.take_messages().last().expect("the page sent its clocks")).unwrap()
        })
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    out
}

#[test]
fn frames_clock_follows_the_frames() {
    let meta = r#"<meta name="raea-clock" content="frames">"#;
    let s = states(meta, Default::default(), &[0.0, 0.4, 1.0, 2.0]);
    let ms: Vec<i64> = s.iter().map(|v| v["date"].as_i64().unwrap()).collect();
    assert_eq!(ms, [0, 400, 1000, 2000], "Date.now follows the frames");
    let perf: Vec<i64> = s.iter().map(|v| v["perf"].as_i64().unwrap()).collect();
    assert_eq!(perf, [0, 400, 1000, 2000], "performance.now follows the frames");
    let timers: Vec<(i64, i64)> = s.iter().map(|v| (v["t500"].as_i64().unwrap(), v["t1500"].as_i64().unwrap())).collect();
    assert_eq!(timers, [(0, 0), (0, 0), (1, 0), (1, 1)], "each timer fires at the frame it's due");
    let raf: Vec<i64> = s.iter().map(|v| v["raf"].as_i64().unwrap()).collect();
    assert!(raf.windows(2).all(|w| w[1] > w[0]), "requestAnimationFrame runs as the frames advance: {raf:?}");
    assert!(raf[3] >= 100, "about 60 callbacks a second over 2 s: {raf:?}");

    // the option does the same as the page's meta tag
    let o = raea::api::RenderOptions { clock: raea::scene::Clock::Frames, ..Default::default() };
    let s2 = states("", o, &[0.0, 0.4, 1.0, 2.0]);
    assert_eq!(s.iter().map(|v| &v["date"]).collect::<Vec<_>>(), s2.iter().map(|v| &v["date"]).collect::<Vec<_>>());
}

#[test]
fn settled_clock_is_unchanged() {
    // The composition format's model: the timers ran while the page loaded, and the clocks hold still across frames
    let s = states("", Default::default(), &[0.0, 1.0, 2.0]);
    assert!(s.windows(2).all(|w| w[0] == w[1]), "the clocks hold still across frames: {s:?}");
    assert_eq!((s[0]["t500"].as_i64(), s[0]["t1500"].as_i64()), (Some(1), Some(1)), "the timers ran while loading");
}
