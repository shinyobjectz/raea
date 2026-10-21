//! K on the GPU, checked: a corpus of K expressions with known answers, run on the CPU reference and on the GPU, and a
//! randomised comparison of the two on vectors from empty to over a million long.

use raea_kgpu::gpu::Gpu;
use raea_kgpu::k::{Backend, Cpu, K};

fn run(b: &dyn Backend, setup: &[(&str, Vec<i32>)], src: &str) -> Result<Vec<i32>, String> {
    let mut k = K::new(b);
    for (n, v) in setup {
        k.set(n, v);
    }
    let v = k.run(src)?;
    k.read(&v)
}

fn main() {
    let gpu = Gpu::new().expect("gpu");
    let cpu = Cpu;
    let corpus: &[(&str, &[i32])] = &[
        ("1+2", &[3]),
        ("2*3+4", &[14]),
        ("!5", &[0, 1, 2, 3, 4]),
        ("+/!10", &[45]),
        ("+\\1 2 3 4", &[1, 3, 6, 10]),
        ("|/3 9 2", &[9]),
        ("&/3 9 2", &[2]),
        ("*/1 2 3 4", &[24]),
        ("-':1 3 6 10", &[1, 2, 3, 4]),
        ("&0 1 0 1 1", &[1, 3, 4]),
        ("&2 0 1", &[0, 0, 2]),
        ("<3 1 2 1", &[1, 3, 2, 0]),
        (">3 1 2 1", &[0, 2, 1, 3]),
        ("x:5 6 7;x@2 0", &[7, 5]),
        ("x:5 6 7;x[1]", &[6]),
        ("2#5 6 7", &[5, 6]),
        ("5#1 2", &[1, 2, 1, 2, 1]),
        ("-2#5 6 7", &[6, 7]),
        ("1_5 6 7", &[6, 7]),
        ("-1_5 6 7", &[5, 6]),
        ("1 2,3 4 5", &[1, 2, 3, 4, 5]),
        ("|1 2 3", &[3, 2, 1]),
        ("#!7", &[7]),
        ("*9 8 7", &[9]),
        ("3!7 8 9", &[1, 2, 0]),
        ("7%2", &[3]),
        ("1 2 3=3 2 1", &[0, 1, 0]),
        ("~0 1 2", &[1, 0, 0]),
        ("1 2 3~1 2 3", &[1]),
        ("f:{x*x};f 1 2 3", &[1, 4, 9]),
        ("{x+y}[10;1 2]", &[11, 12]),
        ("{x*2}'1 2 3", &[2, 4, 6]),
        ("x:!10;+/x*x", &[285]),
        ("x:!8;x@<3!x", &[0, 3, 6, 1, 4, 7, 2, 5]),
        ("+/(!100)>50", &[49]),
        ("x:3 1 2;x@<x", &[1, 2, 3]),
        ("-5|1 2 3", &[1, 2, 3]),
        ("-(5|1 2 7)", &[-5, -5, -7]),
    ];
    let (mut pass, mut fail) = (0, 0);
    for (src, want) in corpus {
        for b in [&cpu as &dyn Backend, &gpu] {
            match run(b, &[], src) {
                Ok(v) if v == *want => pass += 1,
                Ok(v) => {
                    fail += 1;
                    println!("FAIL [{}] {src} = {v:?}, want {want:?}", b.name());
                }
                Err(e) => {
                    fail += 1;
                    println!("ERR  [{}] {src}: {e}", b.name());
                }
            }
        }
    }
    println!("corpus: {pass} pass, {fail} fail ({} expressions x 2 backends)", corpus.len());

    // CPU = GPU on random vectors, over programs that use every kernel
    let programs = [
        "+/x*y+3",
        "|/x-y",
        "&/x",
        "+\\x",
        "|\\x",
        "&x>500000",
        "<x",
        ">y",
        "x@<y",
        "(7!x)=7!y",
        "x,y",
        "-':x",
        "100#x",
        "3_y",
        "|x",
        "+/(x>y)*x-y",
        "x@&x>y",
        "{(x*x)!1000}'y",
    ];
    let mut s = 0x1234_5678_9abc_def0u64;
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let (mut same, mut diff) = (0, 0);
    for n in [0usize, 1, 2, 255, 256, 257, 1000, 65537, 1_048_577] {
        let x: Vec<i32> = (0..n).map(|_| (rnd() % 1_000_000) as i32).collect();
        let y: Vec<i32> = (0..n).map(|_| (rnd() % 1_000_000) as i32 - 500_000).collect();
        for p in programs {
            let setup = [("x", x.clone()), ("y", y.clone())];
            let (a, b) = (run(&cpu, &setup, p), run(&gpu, &setup, p));
            if a == b {
                same += 1;
            } else {
                diff += 1;
                let show = |r: &Result<Vec<i32>, String>| match r {
                    Ok(v) => format!("{:?}", &v[..v.len().min(6)]),
                    Err(e) => e.clone(),
                };
                println!("DIFF n={n} {p}: cpu {} gpu {}", show(&a), show(&b));
            }
        }
    }
    println!("cpu vs gpu: {same} identical, {diff} different ({} programs x 9 sizes up to 1,048,577)", programs.len());
}
