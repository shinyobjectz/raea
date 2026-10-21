#!/bin/zsh
# Builds libraea.dylib: the C API (include/raea.h) as a dynamic library, for engines that
# embed raea's html nodes (moonsplice links it this way, from its own target directory).
#
#   scripts/build-dylib.sh                  default features (network, skia, ffmpeg, intl, webgl)
#   FEATURES=v8 scripts/build-dylib.sh      plus scripts on V8 (Mac only)
#   OUT=dir scripts/build-dylib.sh          copy the dylib and header to dir (default target/dylib)
#
# The crate stays an rlib in Cargo.toml; `cargo rustc --crate-type cdylib` builds the dylib next to it
# (as scripts/build-xcframework.sh does for the staticlib). Its install name is @rpath/libraea.dylib.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
feat=()
[[ -n ${FEATURES:-} ]] && feat=(--features $FEATURES)
cargo rustc --release --lib $feat --crate-type cdylib
out=${OUT:-target/dylib}
mkdir -p $out
cp target/release/libraea.dylib include/raea.h $out/
install_name_tool -id @rpath/libraea.dylib $out/libraea.dylib
echo "$out/libraea.dylib"
