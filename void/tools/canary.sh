#!/bin/zsh
# void Canary: one app in ~/Applications, for the Dock, that is always the latest release build (the founder, 2026-10-17:
# "a canary version that I can have in my dock so I can always open it, whatever the latest build is").
#
#   tools/canary.sh            make or refresh ~/Applications/void Canary.app from target/release/void
#   tools/canary.sh --install  the same, then a LaunchAgent (local.void.canary) that refreshes it whenever any session's
#                              `cargo build --release` writes a new target/release/void
#
# Its own bundle id (pro.ugc.void.canary), name and yellow icon (brand/logo/AppIconCanary.icns, made by
# tools/canary-icon.swift), so the Dock keeps it apart from older void.app copies. The binary is copied whole and moved
# into place in one rename, so a canary that's running keeps its old binary until it's quit and opened again. A build
# still being written is waited out (the file has to hold still for 3 s).
set -e
here=${0:A:h}/..
bin=$here/target/release/void
app=$HOME/Applications/void\ Canary.app
[[ -f $bin ]] || { echo "no $bin: cargo build --release first" >&2; exit 1; }
# a build mid-write: wait until the binary stops changing
last=""
for i in {1..60}; do
  now=$(stat -f '%m %z' $bin)
  [[ $now == $last ]] && break
  last=$now
  sleep 3
done
mkdir -p $app/Contents/MacOS $app/Contents/Resources
cp $bin $app/Contents/MacOS/.void.part
mv -f $app/Contents/MacOS/.void.part $app/Contents/MacOS/void
cp $here/brand/logo/AppIconCanary.icns $app/Contents/Resources/AppIcon.icns
cat > $app/Contents/Info.plist <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>void Canary</string>
  <key>CFBundleDisplayName</key><string>void Canary</string>
  <key>CFBundleIdentifier</key><string>pro.ugc.void.canary</string>
  <key>CFBundleExecutable</key><string>void</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>canary</string>
  <key>CFBundleVersion</key><string>$(date +%Y%m%d.%H%M%S)</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>void listens when you talk to it.</string>
</dict></plist>
PLIST
touch $app
echo "$app ($(stat -f '%Sm' -t '%Y-%m-%d %H:%M' $bin) build)"

if [[ $1 == --install ]]; then
  plist=$HOME/Library/LaunchAgents/local.void.canary.plist
  cat > $plist <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>local.void.canary</string>
  <key>ProgramArguments</key><array><string>/bin/zsh</string><string>${here:A}/tools/canary.sh</string></array>
  <key>WatchPaths</key><array><string>${bin:A}</string></array>
  <key>StandardOutPath</key><string>$HOME/Library/Logs/void-canary.log</string>
  <key>StandardErrorPath</key><string>$HOME/Library/Logs/void-canary.log</string>
</dict></plist>
PLIST
  launchctl bootout gui/$UID/local.void.canary 2>/dev/null || true
  launchctl bootstrap gui/$UID $plist
  echo "refreshes on every release build (launchctl print gui/$UID/local.void.canary; log ~/Library/Logs/void-canary.log)"
fi
