#!/bin/zsh
# README point 4, video with sound: every approved reference in ~/raea/engine/media/golden (local: the footage is a partner's
# or a client's, so it stays out of git; the index is conformance/golden-av.md). For each case with
# reference/APPROVED and an edit.tsx, @raea/motion renders it through the engine (RAEA_MOTION_RENDER_AV, default
# raea-motion), checked against reference/approved.mp4 by golden_av.py (picture at every cut and part entrance,
# loudness, envelope, sync). A game's recorded input is its inputs.rrl (key and pointer steps; the file format outlived
# the rrl language). Exit 1 if any check fails. Usage: sdk/motion/tools/golden-av.sh [--sdk-only] [CASE ...]
set -u
here=${0:A:h}/..
root=~/raea/engine/media/golden
sdk=${RAEA_MOTION_RENDER_AV:-$here/../react/target/release/raea-motion render}
[[ ${1:-} = --sdk-only ]] && shift  # the only mode since rrl's deletion (2026-10-17)
cases=("$@"); (( ${#cases} )) || cases=($root/*(/N))
[ -d $root ] || { echo "golden-av: no local references ($root)"; exit 0; }
out=$(mktemp -d); fail=0
for c in $cases; do
  c=${c:A}; n=${c:t}
  [ -f $c/reference/APPROVED ] || { echo "== $n: not approved, skipped"; continue; }
  if [ -f $c/edit.tsx ]; then
    # what the engine hands the Clock: the game's input, and words or perception a case names in case.json
    extra=()
    [ -f $c/inputs.rrl ] && extra+=(--input inputs.rrl)
    for k in words perception; do
      f=$(python3 -I -c "import json,sys; print(json.load(open(sys.argv[1])).get(sys.argv[2], ''))" $c/case.json $k 2>/dev/null)
      [ -n "$f" ] && extra+=(--$k $f)
    done
    (cd $c && $=sdk edit.tsx -o $out/$n-sdk.mp4 $extra >/dev/null 2>&1)
    echo "== $n (@raea/motion)"
    if [ -f $out/$n-sdk.mp4 ]; then python3 -I $here/tools/golden_av.py $c $out/$n-sdk.mp4 | grep -v '^PASS  frame' || true; python3 -I $here/tools/golden_av.py $c $out/$n-sdk.mp4 >/dev/null || fail=1
    else echo "FAIL  the engine didn't render"; fail=1; fi
  fi
done
rm -rf $out
exit $fail
