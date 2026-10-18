//! `raea-motion render EDIT.tsx -o OUT.mp4|OUT.png|DIR|OUT.sql [--frame N | --at A,B]`: an @raea/motion composition
//! rendered by the engine (raea_react::motion).
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(raea_react::motion::cli(&args));
}
