//! raea-bundle ENTRY OUT (or --z3 FILE): `entry` bundled (raea_bundle::bundle) into OUT (and its CSS beside it, OUT.css), for
//! comparing with bun's
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    // raea-bundle --z3 FILE: FILE's Z3 compile (raea_bundle::z3), for reading what a source compiles to
    if let [flag, file] = &a[..] {
        if flag == "--z3" {
            let src = std::fs::read_to_string(file).unwrap_or_else(|e| panic!("{file}: {e}"));
            match raea_bundle::z3::compile(&src, file.ends_with(".jsx")) {
                Ok(js) => print!("{js}"),
                Err(e) => {
                    eprintln!("{file}: {e}");
                    std::process::exit(1);
                }
            }
            return;
        }
    }
    let [entry, out] = &a[..] else {
        eprintln!("raea-bundle ENTRY OUT");
        std::process::exit(2);
    };
    let t0 = std::time::Instant::now();
    match raea_bundle::bundle(std::path::Path::new(entry)) {
        Ok(b) => {
            std::fs::write(out, b.js).unwrap_or_else(|e| panic!("{out}: {e}"));
            if !b.css.is_empty() {
                let css = std::path::Path::new(out).with_extension("css");
                std::fs::write(&css, b.css).unwrap_or_else(|e| panic!("{}: {e}", css.display()));
            }
            eprintln!("bundled in {:.1} ms", t0.elapsed().as_secs_f64() * 1e3);
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
