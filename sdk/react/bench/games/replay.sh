#!/bin/bash
# Two runners, one world (no-V8 goal, gate 7): a game played live offscreen with random input (the lite runtime,
# stepped by the clock) and the same frames rendered from the input the play logged (React, stepped by frame index)
# must be the same pixels. Prints SSIM per frame and PASS when every frame is >= 0.999.
#   sdk/react/bench/games/replay.sh [GAME...]     (default pong, platformer and arena3d, whose 3D is drawn live)
cd "$(dirname "$0")/../.."
GAMES=${*:-pong platformer arena3d}
OUT=${TMPDIR:-/tmp}/raea-games-replay
mkdir -p "$OUT"
# RAEA_REPLAY_SECS="5 10 15": other moments (the play runs a quarter second past the last)
SECS=${RAEA_REPLAY_SECS:-"0.25 0.5 0.75 1 1.25 1.5 1.75"}
PLAY_SECS=$(echo $SECS | awk '{ m = $1; for (i = 2; i <= NF; i++) if ($i + 0 > m + 0) m = $i; printf "%.17g", m + 0.25 }')
# the smaller of two SSIMs, as the worst so far is printed: the first unless the second is lower (an empty one is 0),
# trailing zeros dropped (ffmpeg's 0.990000 is 0.99, its 1.000000 is 1.0)
minf() { awk -v w="$1" -v v="${2:-0}" 'BEGIN { if (v + 0 < w + 0) { if (index(v, ".")) { sub(/0+$/, "", v); if (v ~ /\.$/) v = v "0" } print v } else print w }'; }
# PASS when an SSIM is at least 0.999 (and $2, when given, is true: 1)
passf() { awk -v w="$1" -v c="${2:-1}" 'BEGIN { print (w + 0 >= 0.999 && c == 1) ? "PASS" : "FAIL" }'; }
status=0
for g in $GAMES; do
  fps=$(grep -o 'fps={[0-9]*}' "bench/games/$g/edit.tsx" | head -1 | tr -dc 0-9)
  keys=""
  [ "$g" = platformer ] && keys="ArrowRight,ArrowRight+ArrowUp,ArrowRight,"
  [ "$g" = pong ] && keys="ArrowUp,ArrowDown,,ArrowUp"
  [ "$g" = seedling ] && keys="ArrowUp,ArrowDown,,ArrowUp"
  [ "$g" = arena3d ] && keys="ArrowRight,ArrowUp,ArrowLeft+ArrowUp,ArrowDown,"
  snaps=""
  for s in $SECS; do snaps="$snaps --snapshot $s $OUT/$g-play-$s.png"; done
  # last run's snapshots go first: a frame the play doesn't reach (it falls behind under load) must fail as unreached,
  # not be compared from the run before
  find "$OUT" -maxdepth 1 -name "$g-play-*.png" -delete
  # the page size: the edit's CW/VW and CH/VH constants (1920x1080 without them)
  w=$(grep -oE 'const (CW|VW) = [0-9]+' "bench/games/$g/edit.tsx" | head -1 | grep -oE '[0-9]+$'); w=${w:-1920}
  h=$(grep -oE 'const (CH|VH) = [0-9]+' "bench/games/$g/edit.tsx" | head -1 | grep -oE '[0-9]+$'); h=${h:-1080}
  RAEA_INPUT_KEYS="$keys" RAEA_AUDIO_MUTE=1 target/release/raea-motion play "bench/games/$g/edit.tsx" --input --offscreen --size ${w}x${h} \
    --seconds "$PLAY_SECS" --input-log "$OUT/$g-input.json" $snaps > "$OUT/$g-play.log" 2>&1 || { echo "$g: the play failed (see $OUT/$g-play.log)"; status=1; continue; }
  line="$g:"
  worst=1
  for s in $SECS; do
    f=$(awk -v s="$s" -v fps="$fps" 'BEGIN { printf "%.0f", s * fps }')
    if [ ! -f "$OUT/$g-play-$s.png" ]; then line="$line $f:unreached"; worst=0; continue; fi
    RAEA_READBACK=rgba target/release/raea-motion render "bench/games/$g/edit.tsx" --frame "$f" --input "$OUT/$g-input.json" -o "$OUT/$g-render-$s.png" > "$OUT/$g-render.log" 2>&1 \
      || { echo "$g: the render of frame $f failed (see $OUT/$g-render.log)"; status=1; continue 2; }
    v=$(ffmpeg -i "$OUT/$g-play-$s.png" -i "$OUT/$g-render-$s.png" -lavfi ssim -f null - 2>&1 | grep -o 'All:[0-9.]*' | cut -d: -f2)
    line="$line $f:$v"
    worst=$(minf "$worst" "$v")
  done
  keyed=$(jq -r '[([.[] | select(.keys | length > 0)] | length), length] | join(" ")' "$OUT/$g-input.json")
  ok=$(passf "$worst")
  [ "$ok" = PASS ] || status=1
  echo "$ok $line (worst $worst; frames with keys down, of frames played: $keyed)"
done
exit $status
