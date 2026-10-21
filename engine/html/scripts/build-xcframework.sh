#!/usr/bin/env bash
# Build raea.xcframework: libraea.a for iOS devices (arm64) and the iOS
# Simulator (arm64), plus include/raea.h and a module map (`import raea`).
#
#   scripts/build-xcframework.sh                 iOS feature set: no network, no skia, no ffmpeg
#   RAEA_FEATURES=skia,network scripts/build-xcframework.sh
#   RAEA_MACOS=1 scripts/build-xcframework.sh      also a macOS (arm64) slice, same features
#   RAEA_SIM_X86=1 ...                             also x86_64 in the simulator slice (Intel Macs)
#   IPHONEOS_DEPLOYMENT_TARGET=16.0 (default)    minimum iOS
#
# Output: target/xcframework/raea.xcframework (+ <target>.native-static-libs.txt: what an
# app links besides the .a; the module maps carry the same list for Swift autolinking)
set -euo pipefail
cd "$(dirname "$0")/.."

export PATH="$HOME/.cargo/bin:$PATH" # rustup's cargo/rustc (the iOS std lives there, not in Homebrew's)
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-16.0}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-13.0}"
FEATURES="${RAEA_FEATURES:-}"
OUT=target/xcframework
HEADERS="$OUT/headers"

targets=(aarch64-apple-ios aarch64-apple-ios-sim)
[[ -n "${RAEA_SIM_X86:-}" ]] && targets+=(x86_64-apple-ios)
[[ -n "${RAEA_MACOS:-}" ]] && targets+=(aarch64-apple-darwin)
rustup target add "${targets[@]}" >/dev/null

rm -rf "$OUT"
mkdir -p "$OUT"
for t in "${targets[@]}"; do
  echo "== $t: cargo rustc --lib --features '$FEATURES'" >&2
  # --print native-static-libs: the system libraries this slice needs (cargo replays the note
  # when the crate is already built); they go into the slice's module map for autolinking.
  # --crate-type staticlib alone: built together with the rlib, the archive also carries every
  # dependency's embedded LLVM bitcode (~230 MB of 320 MB; Xcode ignores it and Xcode 26's
  # bitcode_strip can't remove it from archives). Alone it is ~80 MB, same linked size.
  cargo rustc --release --lib --target "$t" --no-default-features ${FEATURES:+--features "$FEATURES"} \
    --crate-type staticlib -- --print native-static-libs 2> >(tee "$OUT/$t.log" >&2)
  libs=$(sed -n 's/.*native-static-libs: //p' "$OUT/$t.log" | tail -1)
  [[ -n "$libs" ]] || { echo "no native-static-libs note for $t (try: cargo clean -p raea --target $t)" >&2; exit 1; }
  mkdir -p "$HEADERS/$t"
  cp include/raea.h "$HEADERS/$t/"
  {
    echo "module raea {"
    echo "    header \"raea.h\""
    set -- $libs
    while (($#)); do
      case $1 in
        -framework) echo "    link framework \"$2\""; shift ;;
        -lSystem | -lc | -lm) ;; # part of libSystem
        -l*) echo "    link \"${1#-l}\"" ;;
      esac
      shift
    done | awk '!seen[$0]++' # rustc repeats some (e.g. Foundation)
    echo "    export *"
    echo "}"
  } > "$HEADERS/$t/module.modulemap"
  echo "$libs" > "$OUT/$t.native-static-libs.txt"
done

lib() { echo "target/$1/release/libraea.a"; }
args=(-library "$(lib aarch64-apple-ios)" -headers "$HEADERS/aarch64-apple-ios")
if [[ -n "${RAEA_SIM_X86:-}" ]]; then
  mkdir -p "$OUT/sim"
  lipo -create "$(lib aarch64-apple-ios-sim)" "$(lib x86_64-apple-ios)" -output "$OUT/sim/libraea.a"
  args+=(-library "$OUT/sim/libraea.a" -headers "$HEADERS/aarch64-apple-ios-sim")
else
  args+=(-library "$(lib aarch64-apple-ios-sim)" -headers "$HEADERS/aarch64-apple-ios-sim")
fi
[[ -n "${RAEA_MACOS:-}" ]] && args+=(-library "$(lib aarch64-apple-darwin)" -headers "$HEADERS/aarch64-apple-darwin")

xcodebuild -create-xcframework "${args[@]}" -output "$OUT/raea.xcframework" >&2
rm -rf "$HEADERS" "$OUT/sim" "$OUT"/*.log
echo "$OUT/raea.xcframework"
find "$OUT/raea.xcframework" -name '*.a' -exec ls -lh {} \; >&2
