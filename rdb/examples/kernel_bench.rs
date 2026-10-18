//! Phase 3's measure: the hook's ring rules over N spheres, each frame through the compiled K kernels against the
//! interpreter (the reference), value for value; time per frame.
//!
//!     cargo run --release --example kernel_bench -- 1000 10000 100000
use std::time::Instant;

use rdb::eval::{self, Cols};
use rdb::kernel::Compiled;
use rdb::load_sql;

fn main() {
    for n in std::env::args().skip(1).map(|a| a.parse::<usize>().expect("n")) {
        let sql = format!(
            "INSERT INTO edit VALUES (30, 1080, 1920, 60);\n\
             INSERT INTO sphere (id, class, x, y, z, scale) SELECT 's' || i, 'ring', 0, 0.5, 0, 1 FROM range({n}) t(i);\n\
             UPDATE sphere SET\n\
               x = 2.4 * cos(2 * pi() * (row_number() OVER (ORDER BY id) / count(*) OVER () + frame * 0.004)),\n\
               z = 2.4 * sin(2 * pi() * (row_number() OVER (ORDER BY id) / count(*) OVER () + frame * 0.004)),\n\
               y = 0.6 + 0.5 * sin(2 * pi() * row_number() OVER (ORDER BY id) * 3 / count(*) OVER () + frame * 0.08)\n\
               WHERE class = 'ring';\n\
             UPDATE sphere SET scale = 0.55 + 0.6 * y / max(y) OVER () WHERE class = 'ring';\n"
        );
        let (prog, rows) = load_sql(&sql).expect("compiles");
        let base = Cols::from_tups(&rows);
        let k = Compiled::new(&prog);
        let frames = 10i64;
        // KB_TICKS=n: only n in-place ticks (for a profiler)
        if let Some(t) = std::env::var("KB_TICKS").ok().and_then(|v| v.parse::<i64>().ok()) {
            let mut tick = base.clone();
            for f in 0..t {
                for kn in k.kernels.iter().flatten() {
                    kn.run(&mut tick, f).unwrap().unwrap();
                }
            }
            continue;
        }
        // warm the kernels' invariant caches once (as a timeline does on its first frame)
        let _ = eval::raw_cols_k(&prog, &k, &base, 0).unwrap();
        let t = Instant::now();
        let kern: Vec<Cols> = (0..frames).map(|f| eval::raw_cols_k(&prog, &k, &base, f).unwrap()).collect();
        let kern_us = t.elapsed().as_secs_f64() * 1e6 / frames as f64;
        let t = Instant::now();
        let interp: Vec<Cols> = (0..frames).map(|f| eval::raw_cols(&prog, &base, f).unwrap()).collect();
        let interp_us = t.elapsed().as_secs_f64() * 1e6 / frames as f64;
        let same = kern.iter().zip(&interp).all(|(a, b)| a.to_tups() == b.to_tups());
        // the frame's own work, without copying the base (what a game tick that updates in place pays)
        let mut tick = base.clone();
        let t = Instant::now();
        for f in 0..frames {
            for kn in k.kernels.iter().flatten() {
                kn.run(&mut tick, f).unwrap().unwrap();
            }
        }
        let tick_us = t.elapsed().as_secs_f64() * 1e6 / frames as f64;
        println!("{}", serde_json::json!({"rows": n, "kernel_us_per_frame": kern_us, "kernel_tick_in_place_us": tick_us,  "interpreter_us_per_frame": interp_us,
            "speedup": interp_us / kern_us, "kernels_equal_interpreter": same, "k": k.k_text(&prog)}));
    }
}
