#!/bin/bash
# The audio gate (no-V8 goal, gate 6): loudness, true peak and clicks, live against rendered.
#   - pong played live with no input (the game its master gain was taken from) and rendered: the live sound must sit
#     within 0.5 LU of the render's loudness and under the -1.5 dBTP ceiling
#   - the tones fixture (sines started, cut, overlapped, ramped, stopped) played live: no clicks (raea-motion ci clicks)
#   - each render's bounce (before the encoder) held against the
#     play's tap of the same frames (raea-motion ci bounce-vs-live), and the music fixture (@raea/motion/music) rendered and played
#   sdk/react/bench/games/audio.sh
cd "$(dirname "$0")/../.."
OUT=${TMPDIR:-/tmp}/raea-audio-gate
mkdir -p "$OUT"
T=bench/games/tones
M=target/release/raea-motion
# a bounce-vs-live report's numbers for one line (tab-separated), its live play's report as Python printed a dict
SAME='def pyrepr: if type == "object" then "{" + ([to_entries[] | (.key | pyrepr) + ": " + (.value | pyrepr)] | join(", ")) + "}"
    elif type == "array" then "[" + (map(pyrepr) | join(", ")) + "]"
    elif type == "string" then (if contains("\u0027") and (contains("\"") | not) then "\"" else "\u0027" end) as $q
      | $q + (gsub("\\\\"; "\\\\") | gsub($q; "\\" + $q) | gsub("\n"; "\\n") | gsub("\r"; "\\r") | gsub("\t"; "\\t")) + $q
    elif type == "boolean" then (if . then "True" else "False" end) elif type == "null" then "None"
    elif tostring | test("[.eE]") | not then tostring
    else tostring as $s | (. * 1) as $v | if $v == 0 then (if $s | startswith("-") then "-0.0" else "0.0" end)
      else $v | tostring | if test("[.e]") then . else . + ".0" end end end;
  [.max_abs_diff, (.max_abs_diff_db // "-inf"), .compared_s, ."samples_over_1e-6", (.best_lag_samples | pyrepr), (.live | pyrepr)]
  | map(tostring) | @tsv'
[ -f $T/tone-440.wav ] || ffmpeg -v error -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=4" -ac 2 -af volume=0.5 $T/tone-440.wav
[ -f $T/tone-660.wav ] || ffmpeg -v error -f lavfi -i "sine=frequency=660:sample_rate=48000:duration=4" -ac 2 -af volume=0.5 $T/tone-660.wav
status=0
RAEA_PLAY_WAIT_MASTER=60 RAEA_AUDIO_MUTE=1 RAEA_AUDIO_TAP=$OUT/pong-live.wav target/release/raea-motion play bench/games/pong/edit.tsx --offscreen --size 1920x1080 --seconds 39 > $OUT/pong-play.log 2>&1 || { echo "pong: the play failed"; status=1; }
RAEA_AUDIO_BOUNCE=$OUT/pong-bounce.wav target/release/raea-motion render bench/games/pong/edit.tsx -o $OUT/pong-render.mp4 > $OUT/pong-render.log 2>&1 || { echo "pong: the render failed"; status=1; }
RAEA_PLAY_WAIT_MASTER=60 RAEA_AUDIO_MUTE=1 RAEA_AUDIO_TAP=$OUT/tones-live.wav target/release/raea-motion play $T/edit.tsx --offscreen --seconds 10.5 > $OUT/tones-play.log 2>&1 || { echo "tones: the play failed"; status=1; }
RAEA_AUDIO_BOUNCE=$OUT/tones-bounce.wav target/release/raea-motion render $T/edit.tsx -o $OUT/tones-render.mp4 > $OUT/tones-render.log 2>&1 || { echo "tones: the render failed"; status=1; }
RAEA_PLAY_WAIT_MASTER=60 RAEA_AUDIO_MUTE=1 RAEA_AUDIO_TAP=$OUT/music-live.wav target/release/raea-motion play bench/games/music/edit.tsx --offscreen --seconds 8.5 > $OUT/music-play.log 2>&1 || { echo "music: the play failed"; status=1; }
RAEA_AUDIO_BOUNCE=$OUT/music-bounce.wav target/release/raea-motion render bench/games/music/edit.tsx -o $OUT/music-render.mp4 > $OUT/music-render.log 2>&1 || { echo "music: the render failed"; status=1; }
# pong on bevy_seedling (@raea/motion/seedling: its sounds are seedling's graph stepped beside the game, a chunk a
# frame). A chunk is a new voice every frame, so a frame stepped later than the head loses its start (the loop's own
# first frames come 25-50 ms late, and under load a few more): the head here (70 ms) is past any frame's lateness,
# so what's held is the seedling path itself
RAEA_AUDIO_HEAD_MS=70 RAEA_PLAY_WAIT_MASTER=60 RAEA_AUDIO_MUTE=1 RAEA_AUDIO_TAP=$OUT/seedling-live.wav target/release/raea-motion play bench/games/seedling/edit.tsx --offscreen --size 1920x1080 --seconds 39 > $OUT/seedling-play.log 2>&1 || { echo "seedling: the play failed"; status=1; }
RAEA_AUDIO_BOUNCE=$OUT/seedling-bounce.wav target/release/raea-motion render bench/games/seedling/edit.tsx -o $OUT/seedling-render.mp4 > $OUT/seedling-render.log 2>&1 || { echo "seedling: the render failed"; status=1; }
for n in pong tones music seedling; do
  $M ci bounce-vs-live $OUT/$n-bounce.wav $OUT/$n-live.wav --json $OUT/$n-same.json > /dev/null || { echo "$n: bounce against live failed"; status=1; }
done
# the judgement: loudness, true peak and clicks by raea-motion ci clicks (ffmpeg's ebur128 meter), a line a file
# ("PATH: I LUFS, true peak TP dBTP, clicks N ..."), and each bounce against its live tap
meter() { sed -E 's/^.*: (-?[0-9.]+) LUFS, true peak (-?[0-9.]+) dBTP, clicks ([0-9]+).*$/\1 \2 \3/'; }
judge() {
  local pong tones pl pp rl rp tc rc tline loud clean same=1 n
  pong=$($M ci clicks $OUT/pong-live.wav $OUT/pong-render.mp4) && tones=$($M ci clicks $OUT/tones-live.wav $OUT/tones-render.mp4) || return 1
  read -r pl pp _ <<< "$(echo "$pong" | sed -n 1p | meter)"; read -r rl rp _ <<< "$(echo "$pong" | sed -n 2p | meter)"
  loud=$(awk -v l="$pl" -v r="$rl" -v p="$pp" 'BEGIN { d = l - r; if (d < 0) d = -d; print (d <= 0.5 && p <= -1.5) ? "PASS" : "FAIL" }')
  echo "pong: $loud  live $pl LUFS, true peak $pp dBTP; render $rl LUFS, $rp dBTP"
  tline=$(echo "$tones" | sed -n 1p)
  read -r _ _ tc <<< "$(echo "$tline" | meter)"; read -r _ _ rc <<< "$(echo "$tones" | sed -n 2p | meter)"
  if [ "$tc" = 0 ]; then clean=PASS; else clean=FAIL; fi
  echo "tones: $clean  live clicks $tc (render $rc); ${tline#*: }"
  for n in pong tones music seedling; do
    [ -f $OUT/$n-same.json ] || continue
    jq -r "$SAME" $OUT/$n-same.json | awk -F'\t' -v n="$n" '{
      ok = $1 <= 1e-6
      db = $2 == "-inf" ? "-inf" : sprintf("%.1f", $2)
      printf "%s bounce vs live: %s  max diff %.3g (%s dB) over %.1f s, %s samples over 1e-6, best lag %s; live %s\n", n, ok ? "PASS" : "FAIL", $1, db, $3, $4, $5, $6
      exit !ok
    }' || same=0
  done
  [ $loud = PASS ] && [ $clean = PASS ] && [ $same = 1 ]
}
judge || status=1
exit $status
