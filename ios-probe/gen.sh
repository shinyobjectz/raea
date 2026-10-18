#!/bin/bash
# Regenerate the probe crate (this folder) from ~/rowlio-sql: it links rowlio-sql's library, and lib.rs is rowlio-sql's
# own main.rs with `fn main()` renamed to `cli_main()` plus a C entry point, so the phone runs exactly the CLI's code.
# Cargo.toml is rowlio-sql's (same dependencies and [patch] lines, which only apply at a workspace root) plus rowlio-sql.
set -euo pipefail
cd "$(dirname "$0")"
rm -rf src && mkdir -p src
{
  sed -e "s/^fn main() {/pub fn cli_main() {/" ~/rowlio-sql/src/main.rs
  cat <<'RS'

/// The iPhone entry point: run the CLI in `dir`; its arguments are the app's launch arguments (std::env::args).
#[unsafe(no_mangle)]
pub extern "C" fn rowlio_probe_main(dir: *const std::ffi::c_char) {
    let d = unsafe { std::ffi::CStr::from_ptr(dir) }.to_string_lossy().into_owned();
    std::env::set_current_dir(&d).expect("probe dir");
    cli_main();
}
RS
} > src/lib.rs
grep -q '^pub fn cli_main' src/lib.rs
sed -e 's/^name = "rowlio-sql"/name = "rowlio-probe"/' \
    -e 's|^\[dependencies\]|[dependencies]\nrowlio-sql = { path = "../rdb" }|' ~/rowlio-sql/Cargo.toml \
  | awk '/^\[lib\]/{skip=1;next} /^\[\[bin\]\]/{skip=1;next} /^\[/{skip=0} !skip' > Cargo.toml
printf '\n[lib]\npath = "src/lib.rs"\ncrate-type = ["staticlib"]\n' >> Cargo.toml
cp ~/rowlio-sql/Cargo.lock Cargo.lock
