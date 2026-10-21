#!/bin/sh
# Render the kit's PNGs, .icns and .ico from the SVGs in src/. Needs rsvg-convert, ImageMagick (magick) and iconutil.
# Every size is rendered natively with the cat on whole pixels (crisp, no resampling of the cat).
set -e
L=$(cd "$(dirname "$0")/.." && pwd); SRC=$L/src
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
mkdir -p "$L/macos"
for n in 64 128 256 512 1024; do rsvg-convert -w $n -h $n "$SRC/appicon-macos.svg" -o "$L/macos/icon-$n.png"; done
# 32: the 15x16 grid at 1 px in the tile. 16: too small for the tile (the eyes go), so the 16 px favicon tile, the cat at 1 px filling it
rsvg-convert -w 32 -h 32 "$SRC/appicon-macos-small.svg" -o "$L/macos/icon-32.png"
rsvg-convert -w 16 -h 16 "$SRC/favicon-16.svg" -o "$L/macos/icon-16.png"
I=$T/AppIcon.iconset; mkdir -p "$I"
cp "$L/macos/icon-16.png" "$I/icon_16x16.png";      cp "$L/macos/icon-32.png" "$I/icon_16x16@2x.png"
cp "$L/macos/icon-32.png" "$I/icon_32x32.png";      cp "$L/macos/icon-64.png" "$I/icon_32x32@2x.png"
cp "$L/macos/icon-128.png" "$I/icon_128x128.png";   cp "$L/macos/icon-256.png" "$I/icon_128x128@2x.png"
cp "$L/macos/icon-256.png" "$I/icon_256x256.png";   cp "$L/macos/icon-512.png" "$I/icon_256x256@2x.png"
cp "$L/macos/icon-512.png" "$I/icon_512x512.png";   cp "$L/macos/icon-1024.png" "$I/icon_512x512@2x.png"
iconutil -c icns "$I" -o "$L/AppIcon.icns"
# iOS: 1024, opaque (no alpha channel)
rsvg-convert -w 1024 -h 1024 "$SRC/appicon-ios.svg" -o "$T/ios.png"
magick "$T/ios.png" -background '#0e0e10' -alpha remove -alpha off -define png:color-type=2 "$L/icon-1024.png"
# favicons
rsvg-convert -w 180 -h 180 "$SRC/apple-touch.svg" -o "$T/at.png"
magick "$T/at.png" -alpha remove -alpha off -define png:color-type=2 "$L/apple-touch-icon.png"
rsvg-convert -w 32 -h 32 "$SRC/favicon-32.svg" -o "$L/favicon-32.png"
rsvg-convert -w 16 -h 16 "$SRC/favicon-16.svg" -o "$T/f16.png"
rsvg-convert -w 48 -h 48 "$SRC/favicon-48.svg" -o "$T/f48.png"
magick "$T/f16.png" "$L/favicon-32.png" "$T/f48.png" "$L/favicon.ico"
