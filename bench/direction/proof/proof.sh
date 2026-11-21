#!/usr/bin/env bash
# DR4's proof (docs/direction.feature): five checks written as systems over the world (checks/*.ts, none importing
# Bevy), each a line in the project's taste.ts run by `raea check` on a piece that holds it and on a cheat that must
# fail it, and a taste line run as the named check of its form beside them. Its media are made here with ffmpeg
# (nothing downloaded); the two pieces whose checks need a render are rendered first. Every case runs in a copy, so the
# bench folder stays clean.
#
#   RAEA_BIN=path/to/raea bash bench/direction/proof/proof.sh
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
raea=${RAEA_BIN:-raea}
work=$(mktemp -d "${TMPDIR:-/tmp}/raea-dr4-proof.XXXXXX")
# KEEP=1 keeps the copy (its renders, logs and record) and says where
if [ -n "${KEEP:-}" ]; then trap 'echo "proof: kept $work"' EXIT; else trap 'rm -rf "$work"' EXIT; fi
cp -R "$here/." "$work/"
mkdir -p "$work/.raea" "$work/media" "$work/out"
: >"$work/.raea/project"
cd "$work" || exit 2

ff() { ffmpeg -hide_banner -loglevel error -y "$@" || { echo "proof: ffmpeg failed: $*"; exit 2; }; }
# a 4 s take with a tone, and its words ("one" to 0.95 s, "two" 1.05 to 1.90, "three" 2.05 to 2.95, "four" from 3.05)
ff -f lavfi -i "testsrc2=size=540x960:rate=30:duration=4" -f lavfi -i "sine=frequency=220:duration=4" -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest media/take.mp4
printf '[{"text":"one","start":0.10,"end":0.95},{"text":"two","start":1.05,"end":1.90},{"text":"three","start":2.05,"end":2.95},{"text":"four","start":3.05,"end":3.90}]\n' >media/take.words.json
# a beep that starts at once, and the same beep after 150 ms of silence
ff -f lavfi -i "sine=frequency=880:duration=0.3" media/beep.wav
ff -f lavfi -i "anullsrc=r=44100:cl=mono:d=0.15" -f lavfi -i "sine=frequency=880:duration=0.3:sample_rate=44100" -filter_complex "[0][1]concat=n=2:v=0:a=1" media/late-beep.wav

for p in lyric lyric-cheat; do "$raea" render "$p.tsx" -o "out/$p.mp4" --layers >"out/$p.log" 2>&1 || { echo "proof: raea render $p.tsx failed:"; tail -5 "out/$p.log"; exit 2; }; done
for p in mix mix-cheat; do "$raea" render "$p.tsx" -o "out/$p.mp4" >"out/$p.log" 2>&1 || { echo "proof: raea render $p.tsx failed:"; tail -5 "out/$p.log"; exit 2; }; done

bad=0
# FILE ID WANT [FLAGS...]: `raea check FILE --only ID` finds nothing (WANT none) or something (WANT some), waits for
# nothing, and every finding it prints is shown
case_() {
  local file=$1 id=$2 want=$3
  shift 3
  local out
  out=$("$raea" check "$file" --only "$id" --json "$@" 2>"out/$file.$id.err")
  local code=$?
  if [ $code -eq 2 ] || ! jq -e .compositions >/dev/null 2>&1 <<<"$out"; then
    echo "FAIL $file --only $id: raea check couldn't check it (exit $code): $(tail -3 "out/$file.$id.err") $(head -c 300 <<<"$out")"
    bad=$((bad + 1))
    return
  fi
  local n waits
  n=$(jq '[.compositions[].findings[]] | length' <<<"$out")
  waits=$(jq -r '[.compositions[].waits[] | "\(.rule) waits for \(.for)"] | join("; ")' <<<"$out")
  local ok=1
  { [ "$want" = none ] && [ "$n" -ne 0 ]; } && ok=0
  { [ "$want" = some ] && [ "$n" -eq 0 ]; } && ok=0
  [ -n "$waits" ] && ok=0
  [ $ok -eq 1 ] || bad=$((bad + 1))
  printf '%s %s --only %s: %s finding(s)%s\n' "$([ $ok -eq 1 ] && echo "ok  " || echo FAIL)" "$file" "$id" "$n" "${waits:+, $waits}"
  jq -r '.compositions[].findings[] | "       \(.level) \(.check) \(.frames // "" | tostring): \(.says)"' <<<"$out"
}

case_ lyric.tsx lyric none
case_ lyric-cheat.tsx lyric some
case_ cut.tsx cut none
case_ cut-cheat.tsx cut some
case_ mix.tsx cue none
case_ mix-cheat.tsx cue some
case_ ad.tsx caption none
case_ ad-cheat.tsx caption some
case_ ride.tsx rider none
case_ ride-cheat.tsx rider some
# a taste line is a named check of its form, run beside the project's own
case_ ad.tsx text-above none --taste taste.ts
case_ ad-cheat.tsx text-above some --taste taste.ts

if [ $bad -eq 0 ]; then echo "proof: every case as expected"; else echo "proof: $bad case(s) wrong"; fi
exit $((bad > 0))
