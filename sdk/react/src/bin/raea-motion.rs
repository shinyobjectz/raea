//! `raea-motion render EDIT.tsx -o OUT.mp4|OUT.png|DIR|OUT.sql [--frame N | --at A,B]`: an @raea/motion composition
//! rendered by the engine (raea_react::motion).
/// ZeroHeap Z1: every allocation counted per thread while RAEA_ZEROHEAP=1 (one relaxed load more otherwise)
#[global_allocator]
static ALLOC: raea_react::zeroheap::Counting = raea_react::zeroheap::Counting;

fn main() {
    raea_react::zeroheap::init();
    raea_react::zeroheap::init_sites();
    raea_react::startup_mark("main");
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `ci ...`: the CI gates' sound measurements (src/ci.rs), before anything else starts
    if args.first().map(String::as_str) == Some("ci") {
        std::process::exit(raea_react::ci::cli(&args[1..]));
    }
    // Bevy plugins this build has (react's avian, hanabi, tweening features): every Engine adds them
    #[cfg(feature = "bevy-plugins")]
    raea_bevy_plugins::install();
    raea_react::startup_mark("bevy plugins installed");
    std::process::exit(raea_react::motion::cli(&args));
}
