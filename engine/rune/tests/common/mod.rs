//! What the tests share: an app (tests/apps) or an example of @raea/rune (sdk/rune/examples), bundled as raea bundles
//! it (runtime/bundle/lib app.rs: raea-bundle's library beside the test binary, or RAEA_BUNDLE_LIB) and mounted at
//! 800x600.
use std::path::{Path, PathBuf};

use raea_rune_host::js::App;

/// raea's checkout, which the bundle resolves React, @raea/ui and @raea/rune from
fn home() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `file`'s script
pub fn script(file: &Path) -> String {
    raea_runtime_bundle_lib::app::bundle(file, &home(), raea_runtime_bundle_lib::app::WINDOW).unwrap().0
}

/// tests/apps/`name`
#[allow(dead_code)]
pub fn app(name: &str) -> App {
    App::new(&script(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/apps").join(name)), (800.0, 600.0)).unwrap()
}

/// sdk/rune/examples/`name`
#[allow(dead_code)]
pub fn example(name: &str) -> App {
    App::new(&script(&home().join("sdk/rune/examples").join(name)), (800.0, 600.0)).unwrap()
}
