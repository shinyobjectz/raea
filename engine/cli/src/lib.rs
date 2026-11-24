//! raea's command line, which the `raea` binary (main.rs) and a plugin build (bin/raea-bevy-sdk.rs) both run: the
//! dispatch and the help (cli.rs, usage.rs), the guides and a project's context (guide.rs), the small verbs (verbs.rs)
//! and the verbs of raea's app crates.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use raea_runtime_react::scene::{self as motion, *};
use raea_runtime_export::*;
use raea_app_tools as tools;
use serde_json::json;
use raea_neural as neural;
use raea_light as light;

mod cli;
mod guide;
mod refused;
mod skill;
mod spec;
mod tests;
mod usage;
mod verbs;
pub use cli::cli;
use guide::*;
use usage::*;
use verbs::*;

/// The `raea` process after its allocator (main.rs, and a build that installs a signer first): its exit code.
pub fn main() -> i32 {
    raea_core_alloc::init();
    raea_core_alloc::init_sites();
    raea_runtime_react::startup_mark("main");
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        // `check VIDEO.mp4 …`: the floor's measurable lines on an export, ranked P0-P3
        // (with --taste or --quick, through the engine, which evaluates the taste.ts files or renders stills: `check` here)
        Some("check") if !args.iter().any(|a| a == "--taste" || a == "--quick") && raea_app_direction::check::of_video(&args[1..]) => raea_app_check::cli_with(&args[1..], Vec::new()),
        // `ci …`: the gates' sound measurements and lightspeed's process measurements
        Some("ci") => raea_app_ci::cli(&args[1..]),
        _ => {
            raea_runtime_react::CLI.get_or_init(|| cli);
            raea_runtime_react::start();
            cli(&args)
        }
    }
}
