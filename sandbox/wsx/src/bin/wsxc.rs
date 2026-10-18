//! wsxc IN.wasm OUT.cwasm: a WASI module compiled ahead of time for Pulley, the interpreter the iPhone runs.
fn main() -> wasmtime::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let mut cfg = wsx::config()?;
    if std::env::var_os("WSX_SIZE").is_some() {
        cfg.cranelift_opt_level(wasmtime::OptLevel::SpeedAndSize);
    }
    let engine = wasmtime::Engine::new(&cfg)?;
    let out = engine.precompile_module(&std::fs::read(&a[1])?)?;
    std::fs::write(&a[2], &out)?;
    println!("{} -> {} ({} bytes)", a[1], a[2], out.len());
    Ok(())
}
