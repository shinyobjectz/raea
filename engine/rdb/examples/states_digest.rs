//! The substrate gate's evaluator check: a digest of every evaluated value of every frame of each edit, printed as
//! "<edit> <frames> <digest>". Recorded before the substrate work (bench/ref/states.txt) and compared after each step:
//! any change to what a frame evaluates to shows up here, exactly, without the GPU.
//!
//!     cargo run --release --example states_digest -- examples/hook/hook.sql examples/ugc/*.sql
use rdb::{open, timeline::Timeline};

fn main() {
    for path in std::env::args().skip(1) {
        let (prog, rows) = match open(&path) {
            Ok(x) => x,
            Err(e) => {
                println!("{path} error {e}");
                continue;
            }
        };
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        let mut eat = |s: &str| {
            for b in s.bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100_0000_01b3);
            }
        };
        // every frame through the shared timeline (phase 2); eval::frame_state is the per-frame reference it replaced
        let tl = Timeline::new(&prog, &rows).unwrap_or_else(|e| panic!("{path}: {e}"));
        for f in 0..prog.frames {
            for t in tl.tups(f) {
                eat(&t.kind);
                eat(&t.id);
                for (c, v) in &t.vals {
                    eat(c);
                    eat(&match v {
                        rdb::ir::V::N(x) => format!("{:x}", x.to_bits()),
                        rdb::ir::V::S(s) => s.clone(),
                    });
                }
            }
            eat("|");
        }
        println!("{path} {} {h:016x}", prog.frames);
    }
}
