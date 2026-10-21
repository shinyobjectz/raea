// raea-bundle's library (bundle/: Rolldown, lightningcss, the Z3 compile), built with raea-react and put beside the
// binaries (target/<profile>/libraea_bundle.dylib), where raea-motion and void load it at run time (src/motion.rs
// inproc). It's never a Cargo dependency: its crates' features would reach raea's own (f60e0b7). Only builds that bundle
// need it (the `modules` feature, on the Mac); the exported game player (no default features) and the iPhone go without.
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../engine/bundle/src");
    println!("cargo:rerun-if-changed=../../engine/bundle/Cargo.toml");
    println!("cargo:rerun-if-changed=../../engine/bundle/Cargo.lock");
    println!("cargo:rerun-if-env-changed=RAEA_BUNDLE_SKIP");
    if std::env::var_os("CARGO_FEATURE_MODULES").is_none() || std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") || std::env::var_os("RAEA_BUNDLE_SKIP").is_some() {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    // OUT_DIR is target/<profile>/build/raea-react-<hash>/out: the binaries are three folders up
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let bins = out.ancestors().nth(3).expect("target/<profile>").to_path_buf();
    // its own target folder inside the same target folder, so its build is kept between raea's
    let target = bins.parent().expect("target").join("raea-bundle");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .args(["build", "--release", "--lib", "--manifest-path"])
        .arg(manifest.join("../../engine/bundle/Cargo.toml"))
        .arg("--target-dir")
        .arg(&target)
        // the outer build's target folder and profile settings aren't the library's
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_BUILD_TARGET_DIR")
        .status()
        .expect("cargo, to build raea-bundle's library");
    assert!(status.success(), "raea-bundle's library didn't build (bundle/); RAEA_BUNDLE_SKIP=1 builds raea without it");
    let lib = target.join("release/libraea_bundle.dylib");
    // copied beside and renamed into place: a copy over the old file keeps its inode, whose code signature macOS has
    // cached, so the next process to load it was killed (SIGKILL, Code Signature Invalid; void's panel thread, 10-21)
    let tmp = bins.join("libraea_bundle.dylib.new");
    std::fs::copy(&lib, &tmp).and_then(|_| std::fs::rename(&tmp, bins.join("libraea_bundle.dylib"))).unwrap_or_else(|e| panic!("{} -> {}: {e}", lib.display(), bins.display()));
}
