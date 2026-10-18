//! raea as a library: the renderer modules (used by the `raea` CLI in main.rs),
//! a Rust session API (api.rs) and the C ABI over it (ffi.rs, header in include/raea.h)
//! that the iOS / macOS apps link as a static library.
//!
//! Cargo features (all on by default; iOS builds use `--no-default-features`, optionally `skia`):
//! `network` (http(s) assets through ureq/rustls), `skia` (the Skia raster backend),
//! `ffmpeg` (decode `<video>` by shelling out to ffmpeg).

#[cfg(feature = "skia")]
pub mod canvas;
pub mod compile;
pub mod ease;
pub mod effects;
pub mod fonts;
pub mod gsap;
pub mod breakdown;
pub mod measure;
pub mod media;
pub mod net;
pub mod node;
pub mod row;
pub mod row_render;
pub mod world;
pub mod paths;
pub mod previz;
pub mod quads;
pub mod runtime;
pub mod dom;
pub mod scene;
pub mod stage;
pub mod textfit;
pub mod timeline;
pub mod value;
#[cfg(feature = "webgl")]
pub mod webgl;

/// An environment setting: `RAEA_<key>`.
pub fn env_os(key: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(format!("RAEA_{key}"))
}

/// [`env_os`] as a `String`.
pub fn env_var(key: &str) -> Result<String, std::env::VarError> {
    std::env::var(format!("RAEA_{key}"))
}

/// Whether WebGL can run (ANGLE + SwiftShader found).
pub fn webgl_available() -> bool {
    #[cfg(feature = "webgl")]
    {
        webgl::available()
    }
    #[cfg(not(feature = "webgl"))]
    {
        false
    }
}

pub mod api;
pub mod ffi;

/// Moving text drawn from outlines at exact fractional positions (video renders; stills keep Chrome's glyph masks for
/// parity): text a script moves by an inline transform, and rows a host marks (world::mark_moving).
pub fn smooth_moving_text(on: bool) {
    anyrender::moving_text::enable(on);
}
