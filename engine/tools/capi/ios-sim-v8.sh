#!/usr/bin/env bash
# Run raea_bench on the iOS Simulator with the V8 build, JIT on and jitless (what an iPhone runs), on the
# React pages, and print time and frame hash for each. The two modes must hash the same.
#
#   lane sim-<udid> -- tools/capi/ios-sim-v8.sh <simulator-udid> <page.html>...
#
# On the shared Mac, use a Simulator from the host app's UI-test pool (~/the host app/tests/ui/sims.sh) through its lane,
# and leave it warm: this script boots it if needed and never shuts it down.
#
# Needs target/aarch64-apple-ios-sim/release/libraea.a built with --features v8:
#   cargo rustc --release --lib --target aarch64-apple-ios-sim --no-default-features --features v8 --crate-type staticlib
set -euo pipefail
cd "$(dirname "$0")/../.."
udid=$1; shift
# Link first (on the shared Mac, in `lane quick -- tools/capi/build.sh --ios-sim LIB target/raea_bench-ios-sim-v8`);
# linked here only if missing
bin=target/raea_bench-ios-sim-v8
[[ -x $bin ]] || bin=$(tools/capi/build.sh --ios-sim target/aarch64-apple-ios-sim/release/libraea.a $bin)
state=$(xcrun simctl list devices | grep "$udid" | grep -o "(Booted)\|(Shutdown)" || true)
[[ $state == "(Shutdown)" ]] && xcrun simctl boot "$udid"   # left booted (warm) afterwards
for page in "$@"; do
  for mode in jit jitless; do
    env=()
    [[ $mode == jitless ]] && env=(SIMCTL_CHILD_RAEA_JITLESS=1)
    out=$(env ${env[@]+"${env[@]}"} xcrun simctl spawn "$udid" "$PWD/$bin" snapshot "$page" 0 2>&1 | grep -E "snapshot|error" | tail -1)
    echo "$mode  $(basename "$page")  $out"
  done
done
