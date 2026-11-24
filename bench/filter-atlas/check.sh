#!/bin/bash
# #125: a 2160x3840 page with many filtered elements draws, as Chrome draws it.
# squares.tsx: twelve blurred full-frame wrappers, each around a 200 px square (the issue's repro, two rows). Each
#   filter's layer is what its element draws, not its box, so all twelve fit; each square's middle is white and the
#   page between them #333.
# frames.tsx: ten full-frame filtered layers whose boxes draw, so each is a frame-sized image: the atlas grows to the
#   device's largest texture (16384 on Apple silicon). The page's middle is 184 (nine layers would be 176).
# Either refused ("has no room for them") is a failure. Exits 1 on one.
#   bench/filter-atlas/check.sh      (RAEA_BIN: the raea to run, default target/release/raea)
cd "$(dirname "$0")/../.." || exit 2
BIN=${RAEA_BIN:-$PWD/target/release/raea}
export RAEA_SIGN=0 RAEA_MARK=0 RAEA_EXPORT_CACHE=0
OUT=$(mktemp -d "${TMPDIR:-/tmp}/raea-filter-atlas.XXXXXX")
trap 'rm -rf "${OUT:?}"' EXIT
status=0
for c in squares frames; do
  mkdir -p "$OUT/$c"
  if ! "$BIN" render bench/filter-atlas/$c.tsx --at 0 -o "$OUT/$c" > "$OUT/$c.log" 2>&1 || [ ! -f "$OUT/$c/frame-00000.png" ]; then
    echo "FAIL $c.tsx: $(grep -m1 -E 'REFUSED|error' "$OUT/$c.log" | cut -c1-220)"; status=1
  fi
done
grey() { # FRAME X,Y: the pixel's red channel, 0-255
  magick "$OUT/$1/frame-00000.png" -alpha off -format "%[fx:int(255*p{$2}.r+0.5)]" info:
}
near() { # GOT WANT WHAT
  (( $1 - $2 <= 3 && $2 - $1 <= 3 )) || { echo "FAIL $3: $1, wanted $2"; status=1; }
}
if [ -f "$OUT/squares/frame-00000.png" ]; then
  for i in $(seq 0 11); do
    x=$((200 + (i % 6) * 340)); y=$((1600 + i / 6 * 500))
    near "$(grey squares $x,$y)" 255 "squares.tsx: square $i's middle ($x,$y)"
  done
  near "$(grey squares 1080,3000)" 51 "squares.tsx: the page below the squares"
fi
[ -f "$OUT/frames/frame-00000.png" ] && near "$(grey frames 1080,1920)" 184 "frames.tsx: ten white layers at 10% over #333"
[ $status = 0 ] && echo "PASS a 2160x3840 page of twelve blurred wrappers and ten frame-sized filtered layers draws"
exit $status
