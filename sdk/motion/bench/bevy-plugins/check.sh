#!/bin/sh
# Render equals play for Bevy plugins (sdk/motion/docs/bevy-plugins.md): each plugin fixture through sdk/motion/bench/bevy/check.sh
# with the plugin build (render vs baked play with its 3D), then backward seeks in play against the same renders (a
# seek behind the plugin clock replays from frame 0 in a new App). SSIM >= 0.999 passes; exits 1 on any failure.
#   sdk/motion/bench/bevy-plugins/check.sh [avian|hanabi ...]          (default: both)
# Build first: (cd react && lane build -- cargo build --release --features avian,hanabi,tweening --bin raea-bevy-sdk)
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
export RAEA_MOTION_BIN="$HERE/motion.sh"
OUT=${TMPDIR:-/tmp}/raea-bevy-check
mkdir -p "$OUT"
status=0
ssim() {
  ffmpeg -v error -i "$1" -i "$2" -filter_complex "[0]format=rgb24[a];[1]format=rgb24[b];[a][b]ssim=stats_file=-" -f null - 2>&1 | grep -o 'All:[0-9.]*' | tail -1 | cut -d: -f2
}
for name in ${*:-avian hanabi}; do
  edit="$HERE/$name/edit.tsx"
  total=$(grep -o 'const FRAMES = [0-9]*' "$edit" | grep -o '[0-9]*$')
  frames="0 30 $((total / 2)) $((total - 1))"
  FRAMES="$frames" sh "$ROOT/sdk/motion/bench/bevy/check.sh" "$edit" || status=1
  seeks=""
  for f in $(echo $frames | tr ' ' '\n' | sort -rn); do seeks="$seeks --seek $f $OUT/$name-seek-$f.png"; done
  if ! RAEA_AUDIO_MUTE=1 RAEA_PLAY_LAYER=1280x720 nice -n 15 "$RAEA_MOTION_BIN" play "$edit" --baked --offscreen --3d $seeks > "$OUT/$name-seek.log" 2>&1; then
    echo "$name: the seeks failed (see $OUT/$name-seek.log)"; status=1; continue
  fi
  line=""; worst=1
  for f in $frames; do
    s=$(ssim "$OUT/$name-render-$f.png" "$OUT/$name-seek-$f.png")
    line="$line $f:${s:-none}"
    worst=$(python3 -c "print(min($worst, ${s:-0}))")
  done
  ok=$(python3 -c "print('PASS' if $worst >= 0.999 else 'FAIL')")
  [ "$ok" = PASS ] || status=1
  echo "$name: $ok  render vs backward seeks in play, SSIM by frame:$line (worst $worst)"
done
exit $status
