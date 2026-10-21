//! Phase 2's measure: the whole timeline of an edit, against frame_state per frame (what each caller did before).
//!
//!     cargo run --release --example timeline_bench -- examples/hook/hook.sql examples/ugc/ugc.sql
use std::time::Instant;

use rdb::{eval, open, timeline::Timeline};

fn main() {
    for path in std::env::args().skip(1) {
        let (prog, rows) = open(&path).expect("edit");
        let _ = Timeline::new(&prog, &rows).unwrap();
        let mut best = f64::MAX;
        for _ in 0..5 {
            let t = Instant::now();
            std::hint::black_box(Timeline::new(&prog, &rows).unwrap());
            best = best.min(t.elapsed().as_secs_f64() * 1e3);
        }
        let t = Instant::now();
        for f in 0..prog.frames {
            std::hint::black_box(eval::frame_state(&prog, &rows, f).unwrap());
        }
        let per_frame = t.elapsed().as_secs_f64() * 1e3;
        println!("{}", serde_json::json!({"edit": path, "frames": prog.frames, "timeline_ms": best, "frame_by_frame_ms": per_frame, "speedup": per_frame / best}));
    }
}
