#!/bin/bash
# The appscreen fixture's measurements (docs/blitz-bevy.feature H12: <Screen app>, a module on its own Blitz document):
# edit.tsx played live offscreen with bench/render-play/appscreen.json's pointer (over the button from frame 6, pressed
# at 10 and let go at 12, off the screen from 21), and the play's input log rendered. On the render's frames:
#   drawn    the app is on the screen (its dark ground, the button blue) and neither run says it isn't drawn
#   hover    the button rings yellow while the pointer is over it (frame 9), and not once it's gone (frame 27)
#   click    the button is blue while pressed (frame 11) and green from the release on (frames 13 and 27)
#   clock    the bar its timer grows is there at frame 27 and not at frame 0
#   bash sdk/raea/bench/bevy/appscreen/check.sh [RAEA_BIN]     exits 1 on any failure
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../../../.." && pwd)
bin=${1:-${RAEA_BIN:-$root/target/release/raea}}
out=$(mktemp -d "${TMPDIR:-/tmp}/raea-appscreen-check-XXXXXX")
trap 'rm -rf "$out"' EXIT
export RAEA_SIGN=0 RAEA_MARK=0 RAEA_AUDIO_MUTE=1
(cd "$here" && "$bin" play edit.tsx --offscreen --size 960x540 --seconds 1 --input-script "$root/bench/render-play/appscreen.json" --input-log "$out/input.json") > "$out/play.log" 2>&1 || { echo "appscreen: the play failed"; cat "$out/play.log"; exit 1; }
"$bin" render "$here/edit.tsx" --input "$out/input.json" -o "$out/" > "$out/render.log" 2>&1 || { echo "appscreen: the render failed"; cat "$out/render.log"; exit 1; }
status=0
pass() { echo "appscreen: PASS  $1"; }
fail() { echo "appscreen: FAIL  $1"; status=1; }
# a pixel of frame F at X,Y as "R G B"
px() { ffmpeg -v error -i "$out/frame-$(printf %05d "$1").png" -vf "crop=1:1:$2:$3" -f rawvideo -pix_fmt rgb24 - | od -An -tu1 | tr -s ' ' | sed 's/^ //'; }
body() { px "$1" 420 290; }   # the button's face, below its label
ring() { px "$1" 390 290; }   # its border's left edge
bar() { px "$1" 341 379; }    # the bar's first 40 px
green() { read -r r g b <<< "$1"; [ "$g" -gt 140 ] && [ "$g" -gt "$b" ] && [ "$g" -gt "$r" ]; }
blue() { read -r r g b <<< "$1"; [ "$b" -gt 200 ] && [ "$b" -gt "$g" ]; }
yellow() { read -r r g b <<< "$1"; [ "$r" -gt 200 ] && [ "$g" -gt 150 ] && [ "$b" -lt 120 ]; }
pink() { read -r r g b <<< "$1"; [ "$r" -gt 200 ] && [ "$g" -lt 140 ]; }
if grep -q "isn't drawn yet\|didn't open" "$out/play.log" "$out/render.log"; then fail "drawn (a run says: $(grep -h -m1 "isn't drawn yet\|didn't open" "$out/play.log" "$out/render.log"))"; else pass "neither run says the app screen isn't drawn"; fi
blue "$(body 3)" && pass "the button is on the screen, blue (frame 3: $(body 3))" || fail "the button isn't on the screen (frame 3: $(body 3))"
yellow "$(ring 9)" && pass "the button rings with the pointer over it (frame 9: $(ring 9))" || fail "no ring with the pointer over the button (frame 9: $(ring 9))"
blue "$(body 11)" && pass "pressed, not yet let go, it's still blue (frame 11: $(body 11))" || fail "the button changed before its release (frame 11: $(body 11))"
green "$(body 13)" && pass "the click reached it: green from the release (frame 13: $(body 13))" || fail "the click didn't reach the button (frame 13: $(body 13))"
green "$(body 27)" && pass "it stays clicked (frame 27: $(body 27))" || fail "the click didn't hold (frame 27: $(body 27))"
yellow "$(ring 27)" && fail "the ring stayed after the pointer left (frame 27: $(ring 27))" || pass "the ring went with the pointer (frame 27: $(ring 27))"
! pink "$(bar 0)" && pink "$(bar 27)" && pass "its timer's bar grew on the frame's clock (frame 0: $(bar 0), frame 27: $(bar 27))" || fail "the timer's bar (frame 0: $(bar 0), frame 27: $(bar 27))"
grep -q '"apps"' "$out/input.json" && pass "the play's log keeps what reached the app screen" || fail "the play's log has no apps entries"
exit $status
