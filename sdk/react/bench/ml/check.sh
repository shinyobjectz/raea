#!/bin/bash
# A model in a game, render equals play (@raea/motion/ml): the chaser (chaser/edit.tsx), a dot steered by a 487-byte
# policy net run on ONNX Runtime inside the game's step.
#   chaser   a live play pressed by a random hand on the arrow keys, its input logged, snapshotted at a few frames;
#            the same frames rendered from the log (RGB readback, as play reads back). The net's outputs feed the
#            state, so a single different float would move the chaser: each pair must match (SSIM >= 0.9999)
#   orbit    the video (orbit.tsx, one model run per frame), its frames played and rendered: the same test without
#            input
# Outputs are a few PNGs, gone when the check ends unless RAEA_ML_KEEP=1. Build first (cargo build --release;
# RAEA_MOTION names another binary).
#   sdk/react/bench/ml/check.sh
cd "$(dirname "$0")/../.."
BIN=${RAEA_MOTION:-target/release/raea-motion}
OUT=${TMPDIR:-/tmp}/raea-ml-check
rm -rf "$OUT"; mkdir -p "$OUT"
[ "${RAEA_ML_KEEP:-0}" = 1 ] || trap 'rm -rf "$OUT"' EXIT
E=bench/ml/chaser/edit.tsx
# play opens an edit's first composition: the video has an edit of its own
O=bench/ml/chaser/orbit.tsx
FPS=60
status=0
ssim() { ffmpeg -v error -i "$1" -i "$2" -filter_complex "[0]format=rgb24[a];[1]format=rgb24[b];[a][b]ssim=stats_file=-" -f null - 2>&1 | grep -o 'All:[0-9.]*' | tail -1 | cut -d: -f2; }
compare() { # name frames...
  local name=$1; shift
  local line="" worst=1 s
  for f in "$@"; do
    if [ ! -f "$OUT/$name-play-$f.png" ]; then line="$line $f:missing"; worst=0; continue; fi
    s=$(ssim "$OUT/$name-render-$f.png" "$OUT/$name-play-$f.png")
    line="$line $f:${s:-none}"
    worst=$(python3 -c "print(min($worst, ${s:-0}))")
  done
  local ok; ok=$(python3 -c "print('PASS' if $worst >= 0.9999 else 'FAIL')")
  [ "$ok" = PASS ] || status=1
  echo "$name: $ok  play vs render, SSIM by frame:$line"
}
snaps() { # name frames...
  local name=$1; shift
  for f in "$@"; do printf -- "--snapshot %s %s " "$(python3 -c "print($f / $FPS)")" "$OUT/$name-play-$f.png"; done
}
# the game: arrows pressed at random for 9 s, then each snapshotted frame rendered from the logged input
FRAMES="60 240 420 530"
RAEA_INPUT_KEYS='ArrowRight,ArrowRight+ArrowDown,ArrowUp,ArrowLeft,,ArrowLeft+ArrowUp' RAEA_AUDIO_MUTE=1 $BIN play $E --offscreen --seconds 9 --input --input-log $OUT/input.json $(snaps chaser $FRAMES) > $OUT/chaser-play.log 2>&1 || { echo "chaser: the play failed (see $OUT/chaser-play.log)"; exit 1; }
for f in $FRAMES; do
  RAEA_READBACK=rgba $BIN render $E --composition chaser --input $OUT/input.json --frame $f -o $OUT/chaser-render-$f.png > $OUT/chaser-render-$f.log 2>&1 || { echo "chaser: the render of frame $f failed (see $OUT/chaser-render-$f.log)"; status=1; }
done
compare chaser $FRAMES
# the video: no input, one model run a frame
FRAMES="30 180 359"
RAEA_AUDIO_MUTE=1 $BIN play $O --offscreen --seconds 6.2 $(snaps orbit $FRAMES) > $OUT/orbit-play.log 2>&1 || { echo "orbit: the play failed (see $OUT/orbit-play.log)"; exit 1; }
for f in $FRAMES; do
  RAEA_READBACK=rgba $BIN render $O --frame $f -o $OUT/orbit-render-$f.png > $OUT/orbit-render-$f.log 2>&1 || { echo "orbit: the render of frame $f failed (see $OUT/orbit-render-$f.log)"; status=1; }
done
compare orbit $FRAMES
exit $status
