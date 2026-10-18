#!/bin/bash
# Conformance tests on the iPhone (and whatever compositions probe/Bundle holds, rendered in the same launch): the files bundled for JavaScriptCore (raea-motion test --bundle-only), the fixtures
# they read copied under RAEA_ROOT (the app's Bundle/), the probe app built, installed and launched, and its report read
# back: one PASS/FAIL line per case and the summary, exit 1 on any FAIL.
#   react/ios/test.sh motion/conformance/web.test.ts ...    (paths from ~/raea; the phone unlocked)
#   env: MOTION=raea-motion binary, CARGO_TARGET_DIR_IOS=target for the iOS lib, DEVICE=devicectl id
set -eu
cd "$(dirname "$0")"
H=$PWD; R=$H/../..
M=${MOTION:-$R/react/target/release/raea-motion}
T=${CARGO_TARGET_DIR_IOS:-$H/target}
D=${DEVICE:-7ED59008-F34D-57A1-B99C-180A62C891E5}
K=~/.appstoreconnect/private_keys/AuthKey_265V39N664.p8
files=(); for f in "$@"; do files+=("$R/$f"); done
# only motion.js and the fixtures are ours: other files in probe/Bundle (compositions NAME.comp.js and their media/,
# which the app renders too) are other sessions' and stay
mkdir -p probe/Bundle/media/fixtures
cp $R/media/fixtures/{take-h264.mp4,take-hevc.mov,music-bed.m4a} probe/Bundle/media/fixtures/
# the sandbox's WASI tools (compiled for Pulley on the Mac; the phone only interprets)
mkdir -p probe/Bundle/wsx && cp $R/sandbox/modules/*.cwasm probe/Bundle/wsx/
# the browser's fixture pages (the Blitz backend opens them on the phone)
mkdir -p probe/Bundle/browser/conformance && rm -rf probe/Bundle/browser/conformance/pages && cp -R $R/browser/conformance/pages probe/Bundle/browser/conformance/
$M test "${files[@]}" --bundle-only $H/probe/Bundle/motion.js
[ -s probe/Bundle/motion.js ] || { echo "REFUSED the tests didn't bundle"; exit 1; }
lane build -- env IPHONEOS_DEPLOYMENT_TARGET=17.0 RUSTC=$(rustup which rustc) CARGO_TARGET_DIR=$T ~/.cargo/bin/cargo build --release --target aarch64-apple-ios --manifest-path $H/Cargo.toml 2>&1 | grep -E "^error" -A8 || true
(cd probe && xcodegen generate >/dev/null)
lane build -- env RUN=$(date +%s) xcodebuild -project probe/MotionProbe.xcodeproj -target MotionProbe -configuration Release -sdk iphoneos -allowProvisioningUpdates \
  -authenticationKeyPath $K -authenticationKeyID 265V39N664 -authenticationKeyIssuerID ca81fc03-e9d2-4af2-9273-b9d1a3052169 \
  RUST_LIB_DIR=$T/aarch64-apple-ios/release SYMROOT=$H/probe-build build | grep -E "BUILD (SUCCEEDED|FAILED)"
lane phone -- xcrun devicectl device install app --device $D probe-build/Release-iphoneos/MotionProbe.app >/dev/null
lane phone -- xcrun devicectl device process launch --terminate-existing --device $D com.shinyobjectz.motionprobe | tail -1
out=$(mktemp -d); start=$(date +%s)
for i in $(seq 1 180); do
  xcrun devicectl device copy from --device $D --domain-type appDataContainer --domain-identifier com.shinyobjectz.motionprobe \
    --source Documents/report.json --destination $out/report.json >/dev/null 2>&1 || true
  if [ -s $out/report.json ] && [ $(stat -f %m $out/report.json) -ge $start ] && grep -q '"motion.js"' $out/report.json; then break; fi
  rm -f $out/report.json; sleep 10
done
python3 - $out/report.json <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))["motion.js"]
if "error" in r: print("REFUSED", r["error"]); sys.exit(1)
for l in r.get("printed", "").splitlines():
    if l.startswith(("PASS", "FAIL", "error")) or "pass," in l: print(l)
print(f"(on the iPhone: {r.get('ms', 0) / 1000:.1f} s)")
sys.exit(1 if r.get("code") else 0)
PY
