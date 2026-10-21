//! `raea-game`: an exported game's player (sdk/react/docs/game-export.md). It plays the game packaged beside it (an app's
//! Resources/game, or a folder's game/) in a window of its own, or offscreen with `--selftest 5s`.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(raea_react::game::player_main(&args));
}
