#!/bin/sh
# The paint look's demo: each shot rendered in 16:9 and 9:16, joined into demo-16x9.mp4 and demo-9x16.mp4.
#   ./render.sh            (RAEA_MOTION_BIN to use another build)
set -eu
cd "$(dirname "$0")"
BIN=${RAEA_MOTION_BIN:-$(cd ../../../../../.. && pwd)/sdk/react/target/release/raea-motion}
mkdir -p out
for aspect in 16x9 9x16; do
  list=out/list-$aspect.txt
  : > "$list"
  for shot in houses containers field; do
    "$BIN" render demo.tsx --composition "$shot-$aspect" -o "out/$shot-$aspect.mp4"
    echo "file '$shot-$aspect.mp4'" >> "$list"
  done
  ffmpeg -y -v error -f concat -safe 0 -i "$list" -c copy "demo-$aspect.mp4"
done
