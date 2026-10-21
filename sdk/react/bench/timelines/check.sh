#!/bin/sh
# Render equals play, and the bake's time (the no-V8 goal's gate 2): each timeline baked and played from its bake (no
# JS while it plays), snapshots taken at frames across it, the same frames rendered (React, RGB readback), each pair
# compared (SSIM >= 0.999 passes); the bake's time comes from the playtest's open.
#   sdk/react/bench/timelines/check.sh [TIMELINE...]      (default: titles)
# Build first: lane build -- cargo build --release --bin raea-motion (in react/).
set -e
cd "$(dirname "$0")/../.."
OUT=${TMPDIR:-/tmp}/raea-timelines-check
mkdir -p "$OUT"
# the smaller of two SSIMs, as the worst so far is printed: the first unless the second is lower (an empty one is 0),
# trailing zeros dropped (ffmpeg's 0.990000 is 0.99, its 1.000000 is 1.0)
minf() { awk -v w="$1" -v v="${2:-0}" 'BEGIN { if (v + 0 < w + 0) { if (index(v, ".")) { sub(/0+$/, "", v); if (v ~ /\.$/) v = v "0" } print v } else print w }'; }
# PASS when an SSIM is at least 0.999 (and $2, when given, is true: 1)
passf() { awk -v w="$1" -v c="${2:-1}" 'BEGIN { print (w + 0 >= 0.999 && c == 1) ? "PASS" : "FAIL" }'; }
# a frame's time in seconds
at() { awk -v f="$1" -v fps="$fps" 'BEGIN { printf "%.17g", f / fps }'; }
status=0
for t in ${*:-titles}; do
  edit="bench/timelines/$t/edit.tsx"
  fps=$(grep -o 'fps={[A-Z0-9]*}' "$edit" | head -1 | grep -o '[0-9]*' || true)
  [ -n "$fps" ] || fps=$(grep -o 'const FPS = [0-9]*' "$edit" | grep -o '[0-9]*$')
  frames="0 45 120 300 450 599"
  snaps=""
  for f in $frames; do snaps="$snaps --snapshot $(at $f) $OUT/$t-play-$f.png"; done
  last=$(awk -v fps="$fps" 'BEGIN { printf "%.17g", 599 / fps + 0.2 }')
  RAEA_AUDIO_MUTE=1 target/release/raea-motion play "$edit" --baked --offscreen --seconds "$last" --json "$OUT/$t.json" $snaps > "$OUT/$t.log" 2>&1 || { echo "$t: the baked play failed (see $OUT/$t.log)"; status=1; continue; }
  worst=1
  line=""
  for f in $frames; do
    RAEA_READBACK=rgba target/release/raea-motion render "$edit" -o "$OUT/$t-render-$f.png" --frame "$f" > /dev/null 2>&1
    s=$(ffmpeg -v error -i "$OUT/$t-render-$f.png" -i "$OUT/$t-play-$f.png" -filter_complex "[0]format=rgb24[a];[1]format=rgb24[b];[a][b]ssim=stats_file=-" -f null - 2>&1 | grep -o 'All:[0-9.]*' | tail -1 | cut -d: -f2)
    line="$line $f:$s"
    worst=$(minf "$worst" "$s")
  done
  # seeks: the same frames reached by jumping (the keyframe before each, then the frames after it), last frame first so
  # every jump goes backwards or far, compared with the same renders
  seeks=""
  for f in $(echo $frames | tr ' ' '\n' | sort -rn); do seeks="$seeks --seek $f $OUT/$t-seek-$f.png"; done
  RAEA_AUDIO_MUTE=1 target/release/raea-motion play "$edit" --baked --offscreen $seeks > "$OUT/$t-seek.log" 2>&1 || { echo "$t: the seeks failed (see $OUT/$t-seek.log)"; status=1; continue; }
  sline=""
  for f in $frames; do
    s=$(ffmpeg -v error -i "$OUT/$t-render-$f.png" -i "$OUT/$t-seek-$f.png" -filter_complex "[0]format=rgb24[a];[1]format=rgb24[b];[a][b]ssim=stats_file=-" -f null - 2>&1 | grep -o 'All:[0-9.]*' | tail -1 | cut -d: -f2)
    sline="$sline $f:$s"
    worst=$(minf "$worst" "$s")
  done
  bake=$(jq -r '.open_ms // 0' "$OUT/$t.json" | awk '{ printf "%.0f", $1 }')
  ok=$(passf "$worst")
  [ "$ok" = PASS ] || status=1
  echo "$t: $ok  render vs baked play, SSIM by frame:$line; seeks:$sline (worst $worst); opened and baked in $bake ms"
  # an edit baked from the bake before it (bake_incremental): the fixture's last spring (its cards) retuned in a copy, the copy
  # baked from the original (only the frames where the changed component renders are stepped), played, and compared
  # with renders of the copy; a scrub of the incremental bake (seeks across its re-stepped runs) must play through
  first=$(grep -n "damping: [0-9]*" "$edit" | tail -1 | cut -d: -f1)
  if [ -n "$first" ]; then
    mkdir -p "$OUT/$t-edit"
    sed "${first}s/damping: [0-9]*/damping: 5/" "$edit" > "$OUT/$t-edit/edit.tsx"
    esnaps=""
    for f in $frames; do esnaps="$esnaps --snapshot $(at $f) $OUT/$t-eplay-$f.png"; done
    RAEA_BAKE_FROM="$PWD/$edit" RAEA_AUDIO_MUTE=1 target/release/raea-motion play "$OUT/$t-edit/edit.tsx" --baked --offscreen --seconds "$last" $esnaps > "$OUT/$t-edit.log" 2>&1 || { echo "$t: the incremental play failed (see $OUT/$t-edit.log)"; status=1; continue; }
    inc=$(grep -o "raea bake: incremental, [0-9]* of [0-9]* frames stepped, [0-9]* ms" "$OUT/$t-edit.log" | sed "s/raea bake: incremental, //")
    eworst=1
    eline=""
    for f in $frames; do
      RAEA_READBACK=rgba target/release/raea-motion render "$OUT/$t-edit/edit.tsx" -o "$OUT/$t-erender-$f.png" --frame "$f" > /dev/null 2>&1
      s=$(ffmpeg -v error -i "$OUT/$t-erender-$f.png" -i "$OUT/$t-eplay-$f.png" -filter_complex "[0]format=rgb24[a];[1]format=rgb24[b];[a][b]ssim=stats_file=-" -f null - 2>&1 | grep -o 'All:[0-9.]*' | tail -1 | cut -d: -f2)
      eline="$eline $f:$s"
      eworst=$(minf "$eworst" "$s")
    done
    RAEA_BAKE_FROM="$PWD/$edit" RAEA_AUDIO_MUTE=1 target/release/raea-motion play "$OUT/$t-edit/edit.tsx" --baked --offscreen --scrub 60 > "$OUT/$t-escrub.log" 2>&1 && scrub=ok || scrub=failed
    eok=$(passf "$eworst" "$([ "$scrub" = ok ] && [ -n "$inc" ] && echo 1)")
    [ "$eok" = PASS ] || status=1
    echo "$t, edited: $eok  baked from the bake before ($inc), render vs play:$eline (worst $eworst); scrub $scrub"
  fi
done
exit $status
