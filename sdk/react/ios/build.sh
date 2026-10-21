#!/bin/bash
# @raea/motion on the iPhone: a TypeScript program (default: sdk/motion/examples/headless/solids-and-worlds.ts) bundled by
# raea-motion, raea-react with its native calls built for aarch64-apple-ios, and the probe app that runs the bundle at
# launch and writes Documents/report.json. Signed with the ASC API key (this Mac has no Xcode account).
#   sdk/react/ios/build.sh [SCRIPT.ts]       then: xcrun devicectl device install app --device ID probe-build/Release-iphoneos/MotionProbe.app
set -eu
cd "$(dirname "$0")"
H=$PWD; T=${CARGO_TARGET_DIR_IOS:-$H/target}
script=${1:-$H/../../motion/examples/headless/solids-and-worlds.ts}
mkdir -p probe/Bundle
lane build -- env CARGO_TARGET_DIR=$H/../target cargo build --release --bin raea-motion --manifest-path $H/../Cargo.toml
$H/../target/release/raea-motion bundle "$script" -o probe/Bundle/motion.js
lane build -- env IPHONEOS_DEPLOYMENT_TARGET=17.0 RUSTC=$(rustup which rustc) CARGO_TARGET_DIR=$T ~/.cargo/bin/cargo build --release --target aarch64-apple-ios --manifest-path $H/Cargo.toml
(cd probe && xcodegen generate >/dev/null)
K=~/.appstoreconnect/private_keys/AuthKey_265V39N664.p8
lane build -- xcodebuild -project probe/MotionProbe.xcodeproj -target MotionProbe -configuration Release -sdk iphoneos -allowProvisioningUpdates \
  -authenticationKeyPath $K -authenticationKeyID 265V39N664 -authenticationKeyIssuerID ca81fc03-e9d2-4af2-9273-b9d1a3052169 \
  RUST_LIB_DIR=$T/aarch64-apple-ios/release SYMROOT=$H/probe-build build | grep -E "BUILD (SUCCEEDED|FAILED)"
# gate 1 (no V8): the shipped binary carries no V8 (no v8:: C++ symbol, no rusty_v8) and links the system's JavaScriptCore
B=$H/probe-build/Release-iphoneos/MotionProbe.app/MotionProbe
if nm "$B" | grep -qE "_ZN2v8|_ZNK2v8|rusty_v8"; then echo "REFUSED V8 symbols in $B"; exit 1; fi
otool -L "$B" | grep -q JavaScriptCore.framework || { echo "REFUSED $B doesn't link JavaScriptCore"; exit 1; }
echo "no V8 in $B (JavaScriptCore linked)"
