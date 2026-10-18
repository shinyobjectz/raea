#!/bin/bash
# Two runners, one world (no-V8 goal, gate 7): a game played live offscreen with random input (the lite runtime,
# stepped by the clock) and the same frames rendered from the input the play logged (React, stepped by frame index)
# must be the same pixels. Prints SSIM per frame and PASS when every frame is >= 0.999.
#   react/bench/games/replay.sh [GAME...]     (default pong and platformer)
cd "$(dirname "$0")/../.."
GAMES=${*:-pong platformer}
OUT=${TMPDIR:-/tmp}/raea-games-replay
mkdir -p "$OUT"
SECS="0.25 0.5 0.75 1 1.25 1.5 1.75"
status=0
for g in $GAMES; do
  fps=$(grep -o 'fps={[0-9]*}' "bench/games/$g/edit.tsx" | head -1 | tr -dc 0-9)
  keys=""
  [ "$g" = platformer ] && keys="ArrowRight,ArrowRight+ArrowUp,ArrowRight,"
  [ "$g" = pong ] && keys="ArrowUp,ArrowDown,,ArrowUp"
  snaps=""
  for s in $SECS; do snaps="$snaps --snapshot $s $OUT/$g-play-$s.png"; done
  w=$(python3 -c "import re,sys;t=open('bench/games/$g/edit.tsx').read();cw=re.search(r'const (?:CW|VW) = (\d+)',t);print(cw.group(1) if cw else 1920)")
  h=$(python3 -c "import re,sys;t=open('bench/games/$g/edit.tsx').read();ch=re.search(r'const (?:CH|VH) = (\d+)',t);print(ch.group(1) if ch else 1080)")
  RAEA_INPUT_KEYS="$keys" RAEA_AUDIO_MUTE=1 target/release/raea-motion play "bench/games/$g/edit.tsx" --input --offscreen --size ${w}x${h} \
    --seconds 2 --input-log "$OUT/$g-input.json" $snaps > "$OUT/$g-play.log" 2>&1 || { echo "$g: the play failed (see $OUT/$g-play.log)"; status=1; continue; }
  line="$g:"
  worst=1
  for s in $SECS; do
    f=$(python3 -c "print(round($s * $fps))")
    RAEA_READBACK=rgba target/release/raea-motion render "bench/games/$g/edit.tsx" --frame "$f" --input "$OUT/$g-input.json" -o "$OUT/$g-render-$s.png" > "$OUT/$g-render.log" 2>&1 \
      || { echo "$g: the render of frame $f failed (see $OUT/$g-render.log)"; status=1; continue 2; }
    v=$(ffmpeg -i "$OUT/$g-play-$s.png" -i "$OUT/$g-render-$s.png" -lavfi ssim -f null - 2>&1 | grep -o 'All:[0-9.]*' | cut -d: -f2)
    line="$line $f:$v"
    worst=$(python3 -c "print(min($worst, ${v:-0}))")
  done
  keyed=$(python3 -c "import json;d=json.load(open('$OUT/$g-input.json'));print(sum(1 for v in d.values() if v['keys']), len(d))")
  ok=$(python3 -c "print('PASS' if $worst >= 0.999 else 'FAIL')")
  [ "$ok" = PASS ] || status=1
  echo "$ok $line (worst $worst; frames with keys down, of frames played: $keyed)"
done
exit $status
