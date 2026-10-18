#!/bin/zsh
# void.app: the release binary in a macOS app bundle with void's icon (brand/logo/AppIcon.icns, the founder's pixel cat),
# so the Dock, Finder and the app switcher show it. Run after `lane build -- cargo build --release`; open it with
# `open target/release/void.app`. The binary still finds raea's other tools by its build paths.
set -e
here=${0:A:h}/..
app=$here/target/release/void.app
rm -rf $app
mkdir -p $app/Contents/MacOS $app/Contents/Resources
cp $here/target/release/void $app/Contents/MacOS/void
cp $here/brand/logo/AppIcon.icns $app/Contents/Resources/AppIcon.icns
cat > $app/Contents/Info.plist <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>void</string>
  <key>CFBundleDisplayName</key><string>void</string>
  <key>CFBundleIdentifier</key><string>pro.ugc.void</string>
  <key>CFBundleExecutable</key><string>void</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>void listens when you talk to it.</string>
</dict></plist>
PLIST
touch $app
echo $app
