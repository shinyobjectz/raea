#!/usr/bin/env bash
# Build tools/capi/raea_node_smoke against target/dylib/libraea.dylib (scripts/build-dylib.sh first).
set -euo pipefail
cd "$(dirname "$0")/../.."
xcrun clang -O2 -Itarget/dylib tools/capi/raea_node_smoke.c -Ltarget/dylib -lraea -Wl,-rpath,"$PWD/target/dylib" -o target/raea_node_smoke
echo target/raea_node_smoke
