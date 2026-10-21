#!/bin/sh
# Legibility lint for a render's words: every line WCAG AA readable against what's actually behind it, none on the
# picture's focus, none in a letterbox bar, no box past the frame's edge and no centred line drawn off its centre
# (ai/perception/src/legible.rs says how each is measured). Frames are sampled every EVERY seconds
# (default 2) at 960x540, each painted twice: as rendered, and with its glyphs hidden (RAEA_MOTION_TEXT=hidden); the
# manifest says where each line is and its size. Prints one line per fault with its time, and exits 1 on any.
#   sdk/motion/bench/legible/lint.sh EDIT.tsx FRAMES        (FRAMES: the composition's length in frames)
#   EVERY=1 W=1920 H=1080 to sample more or larger; KEEP=DIR keeps the frames and legible.jsonl there
# Build first: cargo build --release --bin raea-motion (react) and --bin raea-perceive (perception).
set -eu
ROOT=$(cd "$(dirname "$0")/../../../.." && pwd)
BIN=${RAEA_MOTION_BIN:-$ROOT/sdk/react/target/release/raea-motion}
PERCEIVE=${RAEA_PERCEIVE:-$ROOT/ai/perception/target/release/raea-perceive}
export RAEA_BUNDLE_LIB=${RAEA_BUNDLE_LIB:-$ROOT/sdk/react/target/release/libraea_bundle.dylib}
edit=${1:?lint.sh EDIT.tsx FRAMES}
total=${2:?lint.sh EDIT.tsx FRAMES (the composition length in frames)}
fps=$(grep -o 'fps={[0-9]*}' "$edit" | head -1 | grep -o '[0-9]*' || true)
[ -n "$fps" ] || fps=$(grep -o 'const FPS = [0-9]*' "$edit" | head -1 | grep -o '[0-9]*$' || true)
fps=${fps:-30}
step=$(awk "BEGIN { print int(${EVERY:-2} * $fps) }")
at=$(awk "BEGIN { s = \"\"; for (f = int($step / 2); f < $total; f += $step) s = s (s == \"\" ? \"\" : \",\") f; print s }")
out=${KEEP:-$(mktemp -d "${TMPDIR:-/tmp}/raea-legible.XXXXXX")}
mkdir -p "$out"
rm -rf "$out/with" "$out/without"
"$BIN" render "$edit" -o "$out/with" --at "$at" --w "${W:-960}" --h "${H:-540}" --manifest "$out/render.json" > "$out/with.log" 2>&1 \
  || { echo "the render failed (see $out/with.log)"; exit 2; }
RAEA_MOTION_TEXT=hidden "$BIN" render "$edit" -o "$out/without" --at "$at" --w "${W:-960}" --h "${H:-540}" > "$out/without.log" 2>&1 \
  || { echo "the render without text failed (see $out/without.log)"; exit 2; }
"$PERCEIVE" legible "$out/render.json" "$out/with" "$out/without" > "$out/legible.jsonl"
status=0
python3 - "$out/legible.jsonl" "$fps" <<'EOF' || status=1
import json, sys
fps = float(sys.argv[2])
bad = 0
for l in open(sys.argv[1]):
    d = json.loads(l)
    t = d["frame"] / fps
    for x in d["lines"]:
        for f in x["faults"]:
            bad += 1
            print(f'{int(t // 60)}:{t % 60:04.1f}  "{x["text"][:48]}": {f}')
print(f"{bad} fault(s)" if bad else "every line legible and clear of the focus")
sys.exit(1 if bad else 0)
EOF
[ -n "${KEEP:-}" ] || rm -rf "$out"
exit $status
