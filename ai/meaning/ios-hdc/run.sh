#!/bin/zsh
# Time the HDC sandwich's layers on an iPhone: the same hdc_time.rs + k.rs the Mac's `raea-meaning hdc-time` runs.
#   ./run.sh DEVICE_ID [COLUMN]     COLUMN: a SigLIP column (.f32 + .json, 768 a row); default the first corpus clip
# The phone must be unlocked. Writes phone-run.json (the third of three runs).
set -euo pipefail
cd "$(dirname "$0")"
D=${1:?device id (xcrun devicectl list devices)}
C=${2:-$(ls ~/.cache/raea-corpus/features/siglip8/*.json | head -1 | sed 's/\.json$//')}
mkdir -p app/inputs && cp "$C.f32" app/inputs/col.f32 && echo "$C" > app/inputs/SOURCE.txt
PATH=$HOME/.cargo/bin:$PATH cargo build --release --target aarch64-apple-ios
(cd app && xcodegen generate >/dev/null && xcodebuild -project RaeaHdc.xcodeproj -scheme RaeaHdc -configuration Release \
  -destination generic/platform=iOS -allowProvisioningUpdates -derivedDataPath build -quiet)
xcrun devicectl device install app --device "$D" app/build/Build/Products/Release-iphoneos/RaeaHdc.app >/dev/null
xcrun devicectl device process launch --device "$D" --console --terminate-existing com.shinyobjectz.raeahdc 2>&1 \
  | tee phone-console.log | grep -m3 "HDC_JSON" | tail -1 | sed 's/^.*HDC_JSON run 3 //' > phone-run.json
cat phone-run.json
