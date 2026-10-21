#!/bin/bash
# Tone on the one mixer core (@raea/motion/tone): the song (a timeline cut on the bar) and the game (state-machine
# one-shots, a music bed ducked under a voice line), each rendered (the core's bounce, before the encoder) and played
# (the core's tap), the two held sample against sample (raea-motion ci bounce-vs-live), with each one's loudness, true peak and
# clicks; and the ducking measured: the music bus under the voice against the same music with ducking off.
# Plays measure timing: run it holding the measurement lock. Build first (lane build -- cargo build --release).
#   sdk/react/bench/tone/check.sh
cd "$(dirname "$0")/../.."
OUT=${TMPDIR:-/tmp}/raea-tone-check
mkdir -p "$OUT"
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
rm -f "$OUT"/*-same.json
G=bench/tone/game
[ -f $G/media/voice.wav ] || { say -v Samantha -o "$OUT/voice.aiff" "Welcome back. The ball keeps bouncing while the music steps aside for my voice, then comes back up." && ffmpeg -v error -y -i "$OUT/voice.aiff" -ar 48000 -ac 2 $G/media/voice.wav; }
# the gaps piece's impulse response: 1.2 s of seeded noise decaying 60 dB, a little wider on the right
# (uniform noise from ffmpeg's seeded generator, each channel its own seed, at the RMS of a 0.3 sigma gaussian)
[ -f bench/tone/gaps/media/ir.wav ] || ffmpeg -v error -f lavfi \
  -i "aevalsrc='if(eq(n,0),st(0,7));0.52*(2*random(0)-1)*pow(10,-3*n/57600)|if(eq(n,0),st(0,8));0.52*(2*random(0)-1)*pow(10,-3*n/57600)':s=48000:d=1.2" \
  -c:a pcm_s16le bench/tone/gaps/media/ir.wav
status=0
# the TokenSynth Sampler too when TokenSynth is installed (tools/tokensynth/setup.sh); its first render makes the roots
pieces="song game gaps"; [ -x "${RAEA_TOKENSYNTH:-$HOME/.cache/raea-tokensynth}/.venv/bin/python" ] && pieces="$pieces tokensynth"
for t in $pieces; do
  edit=bench/tone/$t/edit.tsx
  RAEA_AUDIO_BOUNCE=$OUT/$t-bounce.wav target/release/raea-motion render $edit -o $OUT/$t.mp4 > $OUT/$t-render.log 2>&1 || { echo "$t: the render failed"; status=1; continue; }
  baked=""; [ $t != game ] && baked=--baked
  RAEA_PLAY_WAIT_MASTER=60 RAEA_AUDIO_MUTE=1 RAEA_AUDIO_TAP=$OUT/$t-live.wav target/release/raea-motion play $edit $baked --offscreen --seconds end > $OUT/$t-play.log 2>&1 || { echo "$t: the play failed"; status=1; continue; }
  $M ci bounce-vs-live $OUT/$t-bounce.wav $OUT/$t-live.wav --json $OUT/$t-same.json > /dev/null || { echo "$t: bounce against live failed"; status=1; }
done
# ducking, on the game without its <Mix> (each variant's own loudness would give it its own gain) and 14 dB down at the
# Destination (the master's limiter must not act: it limits each variant differently), the sfx bus muted (the music bus
# alone is measured): as it is (A), with
# ducking off (C), and with ducking off and no voice (D). The music under the voice is A - C + D; its depth that against D
nomix='s#<Mix lufs={-14} ceiling={-1.5} />##;s#^Tone.getTransport().bpm.value = 100;#Tone.getTransport().bpm.value = 100; Tone.Destination.volume.value = -14; Tone.buses.sfx.mute = true;#'
sed -e "$nomix" $G/edit.tsx > $G/.duck.tsx
sed -e "$nomix" -e 's/amount: -12/amount: 0/' $G/edit.tsx > $G/.noduck.tsx
sed -e "$nomix" -e 's/amount: -12/amount: 0/' -e 's#<Audio src={staticFile("voice.wav")} bus="voice" />##' $G/edit.tsx > $G/.novoice.tsx
for v in duck noduck novoice; do
  RAEA_AUDIO_BOUNCE=$OUT/game-$v.wav target/release/raea-motion render $G/.$v.tsx -o $OUT/game-$v.mp4 > /dev/null 2>&1
done
rm -f $G/.duck.tsx $G/.noduck.tsx $G/.novoice.tsx
# the judgement: each piece's bounce against its live tap, with the loudness, true peak and clicks of its bounce and
# its render (raea-motion ci clicks: ffmpeg's ebur128 meter); then the ducking, from the three variants' bounces
judge() {
  local t ok=1 during before duck
  for t in song game gaps tokensynth; do
    [ -f $OUT/$t-same.json ] || continue
    jq -r "$SAME" $OUT/$t-same.json | awk -F'\t' -v t="$t" '{
      ok = $1 <= 1e-6
      printf "%s: %s  bounce vs live max diff %.3g over %.1f s, best lag %s; live %s\n", t, ok ? "PASS" : "FAIL", $1, $3, $5, $6
      exit !ok
    }' || ok=0
    $M ci clicks $OUT/$t-bounce.wav $OUT/$t.mp4 2> /dev/null | awk -F/ '{ print "    " $NF }'
  done
  # the music under the voice is A - C + D; its depth that against D, while the voice speaks (4-7.5 s) and before (1-2.8 s)
  during=$($M ci duck $OUT/game-duck.wav $OUT/game-noduck.wav $OUT/game-novoice.wav 4.0-7.5 1.0-2.8) || return 1
  read -r during before <<< $during
  duck=$(awk -v d="$during" -v b="$before" 'BEGIN { if (b < 0) b = -b; print (d < -8 && b < 0.5) ? "PASS" : "FAIL" }')
  printf 'ducking: %s  the music under the voice %.1f dB (asked -12), before the voice %.2f dB\n' $duck "$during" "$before"
  [ $ok = 1 ] && [ $duck = PASS ]
}
judge || status=1
exit $status
