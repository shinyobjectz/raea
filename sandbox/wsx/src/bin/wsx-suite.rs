//! wsx-suite TOOLS_DIR WORK_DIR: the sandbox suite, as the probe app runs it, printed as JSON.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    println!("{}", serde_json::to_string_pretty(&wsx::suite(std::path::Path::new(&a[1]), std::path::Path::new(&a[2]))).unwrap());
}
