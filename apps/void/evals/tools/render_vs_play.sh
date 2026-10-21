#!/bin/bash
# Render equals play on an agent's edit, as sdk/react/bench/games/replay.sh (games) and sdk/react/bench/timelines/check.sh
# (timelines) do for the bench's own pieces, but for any edit.tsx:
#   apps/void/evals/tools/render_vs_play.sh game EDIT.tsx [KEYS] [SECONDS]
#       played live offscreen with input (KEYS: RAEA_INPUT_KEYS's comma list of chords, e.g. "ArrowRight,ArrowRight+ ,"),
#       snapshots across the play, then the same frames rendered from the input the play logged
#   apps/void/evals/tools/render_vs_play.sh edit EDIT.tsx [FRAMES]
#       baked play offscreen with snapshots, then seeks backwards to the same frames, each against a render
# Prints SSIM per frame and PASS when every pair is >= 0.999. Offscreen only: no window ever opens. Renders at utility QoS.
cd "$(dirname "$0")/../../../.." || exit 2
BIN=$PWD/sdk/react/target/release/raea-motion
mode=$1
edit=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
OUT=${TMPDIR:-/tmp}/raea-rvp-$(echo "$edit" | shasum | cut -c1-8)
rm -rf "$OUT"; mkdir -p "$OUT"
fps=$(grep -o 'fps={[0-9]*}' "$edit" | head -1 | tr -dc 0-9)
if [ -z "$fps" ]; then
  name=$(grep -o 'fps={[A-Za-z_]*}' "$edit" | head -1 | sed 's/fps={\(.*\)}/\1/')
  # in the edit, or a module beside it that it imports the constant from (an agent's sim.ts)
  [ -n "$name" ] && fps=$(cat "$edit" "$(dirname "$edit")"/*.ts "$(dirname "$edit")"/*.tsx 2>/dev/null | grep -oE "const $name *(: *number *)?= *[0-9]+" | head -1 | grep -oE '[0-9]+$')
fi
fps=${fps:-30}
size=$(grep -oE 'width=\{[0-9]+\} height=\{[0-9]+\}' "$edit" | head -1 | grep -oE '[0-9]+' | paste -sd x -)
size=${size:-1920x1080}
ssim() { ffmpeg -v error -i "$1" -i "$2" -filter_complex "[0]format=rgb24[a];[1]format=rgb24[b];[a][b]ssim=stats_file=-" -f null - 2>&1 | grep -o 'All:[0-9.]*' | tail -1 | cut -d: -f2; }
R() { RAEA_READBACK=rgba nice -n 15 taskpolicy -c utility "$BIN" render "$@"; }
worst=1; line=""
if [ "$mode" = game ]; then
  keys=${3:-"ArrowRight,ArrowRight+ ,ArrowLeft,,ArrowUp,ArrowRight+ArrowUp, ,ArrowDown"}
  secs=${4:-3}
  # snapshot times on whole frames: a time between two frames (0.375 s at 60 fps is frame 22.5) compares a frame the
  # play may round the other way, which read as a mismatch on both agent games of newstack-3
  pts=$(python3 -c "print(' '.join(repr(round($secs * k / 8 * $fps) / $fps) for k in range(1, 8)))")
  snaps=""; for s in $pts; do snaps="$snaps --snapshot $s $OUT/play-$s.png"; done
  RAEA_INPUT_KEYS="$keys" RAEA_AUDIO_MUTE=1 nice -n 15 "$BIN" play "$edit" --input --offscreen --size "$size" --seconds "$secs" \
    --input-log "$OUT/input.json" $snaps > "$OUT/play.log" 2>&1 || { echo "FAIL the play failed: $(tail -3 "$OUT/play.log" | tr '\n' ' ')"; exit 1; }
  for s in $pts; do
    f=$(python3 -c "print(round($s * $fps))")
    [ -f "$OUT/play-$s.png" ] || { line="$line $f:unreached"; worst=0; continue; }
    R "$edit" --frame "$f" --input "$OUT/input.json" -o "$OUT/render-$f.png" > "$OUT/render.log" 2>&1 || { echo "FAIL the render of frame $f failed: $(grep -v bevy_log "$OUT/render.log" | tail -2 | tr '\n' ' ')"; exit 1; }
    v=$(ssim "$OUT/play-$s.png" "$OUT/render-$f.png"); line="$line $f:$v"
    worst=$(python3 -c "print(min($worst, ${v:-0}))")
  done
  keyed=$(python3 -c "import json;d=json.load(open('$OUT/input.json'));print(sum(1 for v in d.values() if v['keys']), 'of', len(d))")
  ok=$(python3 -c "print('PASS' if $worst >= 0.999 else 'FAIL')")
  echo "$ok render vs live play with input, SSIM by frame:$line (worst $worst; frames with keys down: $keyed; fps $fps, $size)"
else
  frames=${3:-"0 30 60 90 120 150"}
  snaps=""; for f in $frames; do snaps="$snaps --snapshot $(python3 -c "print($f / $fps)") $OUT/play-$f.png"; done
  last=$(python3 -c "print(max(int(x) for x in '$frames'.split()) / $fps + 0.2)")
  RAEA_AUDIO_MUTE=1 nice -n 15 "$BIN" play "$edit" --baked --offscreen --seconds "$last" $snaps > "$OUT/play.log" 2>&1 || { echo "FAIL the baked play failed: $(tail -3 "$OUT/play.log" | tr '\n' ' ')"; exit 1; }
  seeks=""; for f in $(echo $frames | tr ' ' '\n' | sort -rn); do seeks="$seeks --seek $f $OUT/seek-$f.png"; done
  RAEA_AUDIO_MUTE=1 nice -n 15 "$BIN" play "$edit" --baked --offscreen $seeks > "$OUT/seek.log" 2>&1 || { echo "FAIL the seeks failed: $(tail -3 "$OUT/seek.log" | tr '\n' ' ')"; exit 1; }
  sline=""
  for f in $frames; do
    R "$edit" --frame "$f" -o "$OUT/render-$f.png" > "$OUT/render.log" 2>&1 || { echo "FAIL the render of frame $f failed"; exit 1; }
    [ -f "$OUT/play-$f.png" ] && { v=$(ssim "$OUT/render-$f.png" "$OUT/play-$f.png"); line="$line $f:$v"; worst=$(python3 -c "print(min($worst, ${v:-0}))"); } || { line="$line $f:unreached"; worst=0; }
    [ -f "$OUT/seek-$f.png" ] && { v=$(ssim "$OUT/render-$f.png" "$OUT/seek-$f.png"); sline="$sline $f:$v"; worst=$(python3 -c "print(min($worst, ${v:-0}))"); } || { sline="$sline $f:unreached"; worst=0; }
  done
  ok=$(python3 -c "print('PASS' if $worst >= 0.999 else 'FAIL')")
  echo "$ok render vs baked play, SSIM by frame:$line; seeks:$sline (worst $worst; fps $fps)"
fi
echo "  (files in $OUT)"
[ "$ok" = PASS ]
