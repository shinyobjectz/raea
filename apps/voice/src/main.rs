//! `raea-voice`: raea voice on its own (what `raea voice` runs), for a build that needs none of the engine.
fn main() {
    let args: Vec<String> = std::iter::once("voice".to_string()).chain(std::env::args().skip(1)).collect();
    match raea_app_voice::cli(&args) {
        Ok(s) if s.is_empty() => {}
        Ok(s) => println!("{s}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
