#!/bin/bash
# The agent's sandbox (docs/media.feature M5), built from nothing: the WASI tools (rtools, the lean file and text set;
# jaq), shrunk with wasm-opt, compiled ahead of time for Pulley by wsxc, the iPhone library, and the probe app with its
# control (the same app without the sandbox, for the size it adds).
#   engine/sandbox/build.sh            tools, modules, the Mac's suite run
#   engine/sandbox/build.sh phone      also the iPhone library and the two probe apps (signed with the ASC API key)
set -eu
cd "$(dirname "$0")"
H=$PWD; T=$H/target; M=$H/modules; mkdir -p $M
RC=$(rustup which rustc)
wasm() { lane build -- env RUSTC=$RC CARGO_TARGET_DIR=$T ~/.cargo/bin/cargo "$@" --target wasm32-wasip1; }
wasm build --release --manifest-path $H/rtools/Cargo.toml
wasm install jaq --version 1.6.0 --locked --no-default-features --root $T/tools
lane build -- env CARGO_TARGET_DIR=$T cargo build --release --features compile --manifest-path $H/wsx/Cargo.toml
for w in $T/wasm32-wasip1/release/rtools.wasm $T/tools/bin/jaq.wasm; do
  n=$(basename $w .wasm)
  wasm-opt -Oz --strip-debug --strip-producers --enable-bulk-memory --enable-sign-ext --enable-nontrapping-float-to-int --enable-mutable-globals $w -o $T/$n.opt.wasm
  $T/release/wsxc $T/$n.opt.wasm $M/$n.cwasm
done
mkdir -p $T/work && $T/release/wsx-suite $M $T/work | python3 -c "import json,sys; d=json.load(sys.stdin); print('Mac:', d['engine'], d['as_they_must'])"
[ "${1:-}" = phone ] || exit 0
lane build -- env IPHONEOS_DEPLOYMENT_TARGET=17.0 RUSTC=$RC CARGO_TARGET_DIR=$T ~/.cargo/bin/cargo build --release --lib --target aarch64-apple-ios --manifest-path $H/wsx/Cargo.toml
rm -rf probe/Tools && mkdir -p probe/Tools && cp $M/*.cwasm probe/Tools/
(cd probe && xcodegen generate >/dev/null)
K=~/.appstoreconnect/private_keys/AuthKey_265V39N664.p8
for t in WsxProbe WsxControl; do
  lane build -- xcodebuild -project probe/WsxProbe.xcodeproj -target $t -configuration Release -sdk iphoneos -allowProvisioningUpdates \
    -authenticationKeyPath $K -authenticationKeyID 265V39N664 -authenticationKeyIssuerID ca81fc03-e9d2-4af2-9273-b9d1a3052169 SYMROOT=$H/build build | grep -E "BUILD (SUCCEEDED|FAILED)"
done
B=$H/build/Release-iphoneos
echo "added to the app: $(( $(du -sk $B/WsxProbe.app | cut -f1) - $(du -sk $B/WsxControl.app | cut -f1) )) KB (budget 15 MB)"
