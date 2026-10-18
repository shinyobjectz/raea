#!/bin/sh
# Render equals play for 3D: each fixture's frames rendered (React, RGB readback) and the same frames taken from a
# baked play with its 3D (--3d), presented to a layer in no window as void's stage presents (RAEA_PLAY_LAYER), each
# pair compared (SSIM >= 0.999 passes). Exits 1 on any failure.
#   motion/bench/bevy/check.sh [EDIT.tsx ...]        (default: every motion/bench/bevy/*/edit.tsx)
#   FRAMES="0 30 59" to choose the frames (default: 0, the middle and the last)
# Build first: (cd react && lane build -- cargo build --release --bin raea-motion).
set -u
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
BIN=${RAEA_MOTION_BIN:-$ROOT/react/target/release/raea-motion}
OUT=${TMPDIR:-/tmp}/raea-bevy-check
mkdir -p "$OUT"
edits=${*:-$(ls "$ROOT"/motion/bench/bevy/*/edit.tsx)}
status=0
for edit in $edits; do
  name=$(basename "$(dirname "$edit")")
  fps=$(grep -o 'fps={[0-9]*}' "$edit" | head -1 | grep -o '[0-9]*' || true)
  [ -n "$fps" ] || fps=$(grep -o 'const FPS = [0-9]*' "$edit" | grep -o '[0-9]*$')
  total=$(grep -o 'durationInFrames={[0-9]*}' "$edit" | head -1 | grep -o '[0-9]*' || true)
  [ -n "$total" ] || total=$(grep -o 'const FRAMES = [0-9]*' "$edit" | grep -o '[0-9]*$')
  frames=${FRAMES:-"0 $((total / 2)) $((total - 1))"}
  line=""
  worst=1
  # the renders first: their size is the layer's
  for f in $frames; do
    if ! RAEA_READBACK=rgba nice -n 15 "$BIN" render "$edit" -o "$OUT/$name-render-$f.png" --frame "$f" > "$OUT/$name-render-$f.log" 2>&1; then
      echo "$name: the render of frame $f failed (see $OUT/$name-render-$f.log)"; status=1; continue 2
    fi
  done
  first=$(echo $frames | cut -d' ' -f1)
  size=$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=s=x:p=0 "$OUT/$name-render-$first.png")
  snaps=""
  for f in $frames; do snaps="$snaps --snapshot $(python3 -c "print($f / $fps)") $OUT/$name-play-$f.png"; done
  last=$(python3 -c "print(max([$(echo $frames | tr ' ' ',')]) / $fps + 0.3)")
  if ! RAEA_AUDIO_MUTE=1 RAEA_PLAY_LAYER=$size nice -n 15 "$BIN" play "$edit" --baked --offscreen --3d --seconds "$last" $snaps > "$OUT/$name-play.log" 2>&1; then
    echo "$name: the baked play failed (see $OUT/$name-play.log)"; status=1; continue
  fi
  for f in $frames; do
    s=$(ffmpeg -v error -i "$OUT/$name-render-$f.png" -i "$OUT/$name-play-$f.png" -filter_complex "[0]format=rgb24[a];[1]format=rgb24[b];[a][b]ssim=stats_file=-" -f null - 2>&1 | grep -o 'All:[0-9.]*' | tail -1 | cut -d: -f2)
    line="$line $f:${s:-none}"
    worst=$(python3 -c "print(min($worst, ${s:-0}))")
  done
  ok=$(python3 -c "print('PASS' if $worst >= 0.999 else 'FAIL')")
  [ "$ok" = PASS ] || status=1
  echo "$name: $ok  render vs baked play with 3D, SSIM by frame:$line (worst $worst)"
done
exit $status
