//! Bevy plugins in raea's engine, each behind a cargo feature (sdk/motion/docs/bevy-plugins.md). `install()` registers the
//! enabled ones with rdb's plugin hooks (engine/rdb/src/engine/plugins.rs): every Engine made afterwards adds them, steps them
//! by the frame index, and applies TSX's component rows through the TypeRegistry. What each plugin needed by hand is
//! here; everything reflected is reached through the generated TypeScript (react's raea-bevy-sdk).

#[cfg(feature = "hanabi")]
pub mod hanabi;

/// Registers every plugin this build has (by feature). Call before the first Engine.
pub fn install() {
    #[cfg(feature = "avian")]
    rdb::engine::plugins::register(rdb::engine::plugins::Registered {
        name: "avian3d",
        // one physics step per frame: PhysicsPlugins runs in FixedPostUpdate, and with a plugin installed rdb's Time
        // advances one frame (Time<Fixed>'s step) on each update that draws the next frame, and zero otherwise
        install: |app| {
            app.add_plugins(avian3d::prelude::PhysicsPlugins::default());
        },
        group: None,
    });
    #[cfg(feature = "hanabi")]
    rdb::engine::plugins::register(rdb::engine::plugins::Registered { name: "bevy_hanabi", install: hanabi::install, group: Some(hanabi::group) });
    #[cfg(feature = "tweening")]
    rdb::engine::plugins::register(rdb::engine::plugins::Registered {
        name: "bevy_tweening",
        // its animator ticks in Update by Time's delta, which rdb makes one frame per new frame (else zero)
        install: |app| {
            app.add_plugins(bevy_tweening::TweeningPlugin);
        },
        group: None,
    });
}

/// The plugins this build has, by name.
pub fn compiled() -> Vec<&'static str> {
    let mut v = Vec::new();
    if cfg!(feature = "avian") {
        v.push("avian3d");
    }
    if cfg!(feature = "hanabi") {
        v.push("bevy_hanabi");
    }
    if cfg!(feature = "tweening") {
        v.push("bevy_tweening");
    }
    v
}

/// The type-path prefixes each compiled plugin's reflected types live under (what the generator keeps).
pub fn prefixes() -> Vec<&'static str> {
    let mut v = Vec::new();
    if cfg!(feature = "avian") {
        v.push("avian3d::");
    }
    if cfg!(feature = "hanabi") {
        v.extend(["bevy_hanabi::", "raea_bevy_plugins::hanabi::"]);
    }
    if cfg!(feature = "tweening") {
        v.push("bevy_tweening::");
    }
    v
}
