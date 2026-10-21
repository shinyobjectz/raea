#!/usr/bin/env bash
# Build tools/capi/raea_bench against libraea.a, dead-stripped, the way an app links the
# static library.
#   tools/capi/build.sh [LIB] [OUT]            macOS arm64 (default LIB target/release/libraea.a)
#   tools/capi/build.sh --ios-sim [LIB] [OUT]  iOS Simulator arm64 executable (run with simctl spawn)
#   tools/capi/build.sh --ios [LIB] [OUT]      iOS device arm64 executable (for size; not signed)
set -euo pipefail
cd "$(dirname "$0")/../.."
platform=macos
case "${1:-}" in --ios-sim) platform=ios-sim; shift ;; --ios) platform=ios; shift ;; esac
case $platform in
  macos)   target=arm64-apple-macos; sdk=macosx; lib=target/release/libraea.a ;;
  ios-sim) target=arm64-apple-ios16.0-simulator; sdk=iphonesimulator; lib=target/aarch64-apple-ios-sim/release/libraea.a ;;
  ios)     target=arm64-apple-ios16.0; sdk=iphoneos; lib=target/aarch64-apple-ios/release/libraea.a ;;
esac
LIB="${1:-$lib}"
OUT="${2:-target/raea_bench${platform/macos/}}"
[[ $platform != macos ]] && OUT="${2:-target/raea_bench-$platform}"
# Metal/MetalKit only matter for builds with the `skia` feature; unused frameworks cost nothing.
xcrun --sdk $sdk clang -O2 -target $target -Iinclude tools/capi/raea_bench.c "$LIB" \
  -framework CoreText -framework CoreGraphics -framework CoreFoundation -framework Foundation \
  -framework ImageIO -framework Metal -framework MetalKit -framework Security \
  -lc++ -lobjc -liconv -Wl,-dead_strip -o "$OUT" 2> >(grep -v "was built for newer" >&2)
echo "$OUT"
