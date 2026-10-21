#!/bin/sh
# Framing lint, measured on the pictures: the subject's top cut by the frame (a head out of the top, a bust aimed at
# its chest), a push ending lower on its subject than it began, the camera leaving its subject without a cut, and the
# camera moving with nothing to look at (ai/perception/src/framing.rs says how each is measured). The subject is Vision's
# attention blob and faces, filled out to the lit thing under it. Runs on a video, or renders a composition first
# without its words (RAEA_MOTION_TEXT=hidden, so captions don't draw the eye) to MP4. Samples at 960 wide every EVERY
# seconds (default 0.5); shots are split at the cuts raea-perceive finds and at dips through black. Prints one line per
# fault with its span and a fix, and exits 1 on any; long shots with no subject are notes and don't fail.
#   sdk/motion/bench/legible/framing.sh FILM.mp4
#   sdk/motion/bench/legible/framing.sh EDIT.tsx
#   EVERY=0.25 to sample more; NOCUTS=1 to skip the cut detector; KEEP=DIR keeps the frames and framing.jsonl there
# Build first: cargo build --release --bin raea-motion (react) and --bin raea-perceive (perception).
set -eu
ROOT=$(cd "$(dirname "$0")/../../../.." && pwd)
BIN=${RAEA_MOTION_BIN:-$ROOT/sdk/react/target/release/raea-motion}
PERCEIVE=${RAEA_PERCEIVE:-$ROOT/ai/perception/target/release/raea-perceive}
export RAEA_BUNDLE_LIB=${RAEA_BUNDLE_LIB:-$ROOT/sdk/react/target/release/libraea_bundle.dylib}
src=${1:?framing.sh FILM.mp4 | EDIT.tsx}
every=${EVERY:-0.5}
out=${KEEP:-$(mktemp -d "${TMPDIR:-/tmp}/raea-framing.XXXXXX")}
mkdir -p "$out"
rm -rf "$out/frames"
mkdir -p "$out/frames"
case "$src" in
*.tsx | *.ts | *.jsx | *.js)
  RAEA_MOTION_TEXT=hidden "$BIN" render "$src" -o "$out/film.mp4" > "$out/render.log" 2>&1 \
    || { echo "the render failed (see $out/render.log)"; exit 2; }
  video=$out/film.mp4 ;;
*) video=$src ;;
esac
ffmpeg -v error -i "$video" -vf "fps=1/$every,scale=960:-2" "$out/frames/%05d.png"
cuts=
if [ -z "${NOCUTS:-}" ]; then
  cuts=$("$PERCEIVE" cuts "$video" | python3 -c 'import json, sys; print(",".join(str(c) for c in json.load(sys.stdin)["cuts"]))')
fi
"$PERCEIVE" framing "$out/frames" --every "$every" ${cuts:+--cuts "$cuts"} > "$out/framing.jsonl"
status=0
python3 - "$out/framing.jsonl" <<'EOF' || status=1
import json, sys
bad = notes = 0
for l in open(sys.argv[1]):
    d = json.loads(l)
    if "fault" in d:
        f = d["fault"]
        if f["kind"] == "no-subject":
            notes += 1
        else:
            bad += 1
        print(f["said"])
print(f"{bad} fault(s), {notes} note(s)" if bad else f"framing holds its subjects ({notes} note(s))")
sys.exit(1 if bad else 0)
EOF
[ -n "${KEEP:-}" ] || rm -rf "$out"
exit $status
