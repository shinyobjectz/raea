//! rdb: the edit as live SQL tables, edited by snippets, run on Bevy's World (docs/relational-plan.feature). The
//! commands are `rdb::cli` (src/cli.rs), so raea's shell runs them in process too.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(rdb::cli::cli(&args));
}
