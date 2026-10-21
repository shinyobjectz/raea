//! The frame loop before and after Bevy (docs/bevy.feature B2/B7), on one composition in one process: frames made by
//! the composition's own calls (seek_frame then paint, what Composition::render did) against frames made as runs of
//! Bevy's schedule (stage.rs). The same composition moves between the two in alternating blocks (best of 3), on a
//! 64 MB-stack thread as the engine's render threads are.
//!   cargo run --release --example stage_bench -- PAGE.html [frames]
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::thread::Builder::new().stack_size(64 << 20).spawn(move || run(args)).unwrap().join().unwrap();
}

fn run(args: Vec<String>) {
    let path = std::path::PathBuf::from(args.first().expect("PAGE.html"));
    let n: usize = args.get(1).and_then(|x| x.parse().ok()).unwrap_or(60);
    let o = raea::api::RenderOptions::from_json(None).unwrap();
    let (html, dir) = raea::api::resolve_input(&path).unwrap();
    let fps = raea::api::default_fps(&html);
    let dt = fps.1 as f64 / fps.0 as f64;
    let mut comp = raea::api::probe(&o, &html, &dir, fps).unwrap().comp;
    let mut buf = Vec::new();
    for i in 0..5 {
        comp.seek_frame(i as f64 * dt);
        comp.paint(&mut buf, 0.0, Instant::now());
    }
    let (mut best_d, mut best_s) = (f64::MAX, f64::MAX);
    for _ in 0..3 {
        let t0 = Instant::now();
        for i in 0..n {
            comp.seek_frame(i as f64 * dt);
            comp.paint(&mut buf, 0.0, Instant::now());
        }
        best_d = best_d.min(t0.elapsed().as_secs_f64() * 1e3 / n as f64);
        let mut stage = raea::stage::Stage::new(comp);
        let t0 = Instant::now();
        for i in 0..n {
            stage.frame(i as f64 * dt, &mut buf);
        }
        best_s = best_s.min(t0.elapsed().as_secs_f64() * 1e3 / n as f64);
        comp = stage.into_comp();
    }
    println!("{}: {n} frames, ms per frame: composition's own loop {best_d:.3}, Bevy's schedule {best_s:.3} ({:+.3} ms, {:+.1}%)",
        path.display(), best_s - best_d, (best_s / best_d - 1.0) * 100.0);
    // where a frame's time goes: the seek (timelines and scripts in V8) against the paint (style, layout, raster)
    let (mut seek, mut paint) = (0.0, 0.0);
    for i in 0..n {
        let t0 = Instant::now();
        comp.seek_frame(i as f64 * dt);
        let t1 = Instant::now();
        comp.paint(&mut buf, 0.0, t1);
        seek += (t1 - t0).as_secs_f64();
        paint += t1.elapsed().as_secs_f64();
    }
    println!("    split: seek (JS) {:.3} ms, paint {:.3} ms ({:.0}% JS)", seek * 1e3 / n as f64, paint * 1e3 / n as f64, seek / (seek + paint) * 100.0);
}
