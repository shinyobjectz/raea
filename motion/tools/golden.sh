#!/bin/zsh
# README point 4: golden frames. Each case in conformance/golden/ is an edit.tsx (@raea/motion) rendered at case.json's
# frames, which must agree to its ssim_min with the case's reference frames (reference/fNNNN.png): the retired rrl
# stack's renders of the case's edit.rrl, frozen on 2026-10-17 when rrl was deleted (03-video-take's stay local, since
# its footage is), or renders the user approved (reference/APPROVED). The SDK's renderer is RAEA_MOTION_RENDER
# (an agent session, first slice 2026-10-22), called as: $RAEA_MOTION_RENDER EDIT.tsx --frame N -o OUT.png
# Real ffmpeg does the comparison, so the grader doesn't depend on what it grades.
# A case with reference/APPROVED compares against those frames, which the user approved; editing them needs a new approval.
# Usage: motion/tools/golden.sh [CASE_DIR ...]   (default: every case)
set -u
here=${0:A:h}/..
cases=("$@"); (( ${#cases} )) || cases=($here/conformance/golden/*(/))
if [ -z "${RAEA_MOTION_RENDER:-}" ]; then
  echo "golden: waiting for the SDK's renderer (set RAEA_MOTION_RENDER); ${#cases} cases ready"; exit 0
fi
fail=0
for c in $cases; do
  cfg=$(cat $c/case.json)
  frames=($(python3 -I -c "import json,sys; print(*json.loads(sys.argv[1])['frames'])" "$cfg"))
  floor=$(python3 -I -c "import json,sys; print(json.loads(sys.argv[1])['ssim_min'])" "$cfg")
  out=$(mktemp -d)
  for f in $frames; do
    want=$c/reference/f$(printf %04d $f).png
    [ -f $want ] || { echo "FAIL  ${c:t} frame $f: no reference frame ($want)"; fail=1; continue; }
    cp $want $out/want.png
    $=RAEA_MOTION_RENDER $c/edit.tsx --frame $f -o $out/got.png >/dev/null 2>&1 || { echo "FAIL  ${c:t} frame $f: the SDK didn't render"; fail=1; continue; }
    s=$(ffmpeg -hide_banner -i $out/got.png -i $out/want.png -lavfi ssim -f null - 2>&1 | grep -o 'All:[0-9.]*' | cut -d: -f2)
    if python3 -I -c "import sys; sys.exit(0 if float('${s:-0}') >= $floor else 1)"; then echo "PASS  ${c:t} frame $f  ssim $s"
    else echo "FAIL  ${c:t} frame $f  ssim ${s:-none} < $floor"; fail=1; fi
  done
  rm -rf $out
done
exit $fail
