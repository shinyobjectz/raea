// Embeds the bundled deterministic fonts (fonts/index.json) as a const table for src/fonts.rs.
use std::{env, fs, path::Path};

fn main() {
    // Read at run time, not with env!(): a build script compiled for one checkout path can be reused by
    // cargo for another (worktrees sharing a target dir), and env!() would keep the first path.
    let dir = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("fonts");
    println!("cargo:rerun-if-changed={}", dir.join("index.json").display());
    let index = fs::read_to_string(dir.join("index.json")).expect("fonts/index.json");
    let mut out = String::from("[\n");
    // tiny dependency-free parse of [{"family":..,"weight":..,"style":..,"file":..}, ...]
    for obj in index.split('{').skip(1) {
        let field = |k: &str| -> String {
            let key = format!("\"{k}\":");
            let i = obj.find(&key).expect("field") + key.len();
            let rest = obj[i..].trim_start();
            rest[1..].split('"').next().unwrap().to_string()
        };
        let file = dir.join(field("file"));
        out.push_str(&format!(
            "    ({:?}, {:?}, {:?}, include_bytes!({:?}) as &[u8]),\n",
            field("family"),
            field("weight"),
            field("style"),
            file.display().to_string()
        ));
    }
    out.push(']');
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("bundled_fonts.rs"), out).unwrap();
}
