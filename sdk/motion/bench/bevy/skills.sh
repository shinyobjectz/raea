#!/bin/sh
# Every example in the Bevy skills library renders: each ```tsx block holding a registerRoot in
# apps/void/prompts/craft/bevy.md and void/prompts/craft/bevy/*.md is written to a folder of its own (with media/model.glb,
# media/poster.png, media/models/robot-expressive/RobotExpressive.glb and the environment fixture's .hdr panoramas), rendered at its first, middle and last frame, and fails on a render error or
# a blank frame (luma range under 8). Exits 1 on any failure.
#   sdk/motion/bench/bevy/skills.sh
# Build first: (cd react && lane build -- cargo build --release --bin raea-motion).
set -u
ROOT=$(cd "$(dirname "$0")/../../../.." && pwd)
BIN=${RAEA_MOTION_BIN:-$ROOT/sdk/react/target/release/raea-motion}
OUT=${TMPDIR:-/tmp}/raea-bevy-skills
rm -rf "$OUT" && mkdir -p "$OUT"
python3 -I - "$ROOT" "$OUT" <<'PY'
import pathlib, re, sys
root, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
files = [root / "apps/void/prompts/craft/bevy.md"] + sorted((root / "apps/void/prompts/craft/bevy").glob("*.md"))
for f in files:
    for i, block in enumerate(re.findall(r"```tsx\n(.*?)```", f.read_text(), re.S)):
        if "registerRoot" not in block:
            continue
        d = out / f"{f.stem}-{i}"
        (d / "media").mkdir(parents=True)
        (d / "edit.tsx").write_text(block)
        (d / "media/model.glb").write_bytes((root / "sdk/motion/bench/bevy/model/media/spinner.glb").read_bytes())
        (d / "media/poster.png").write_bytes((root / "sdk/motion/bench/bevy/planes/media/poster.png").read_bytes())
        (d / "media/models/robot-expressive").mkdir(parents=True)
        (d / "media/models/robot-expressive/RobotExpressive.glb").write_bytes((root / "sdk/motion/bench/bevy/character/media/RobotExpressive.glb").read_bytes())
        for hdr in (root / "sdk/motion/bench/bevy/environment/media").glob("*.hdr"):
            (d / "media" / hdr.name).write_bytes(hdr.read_bytes())
PY
status=0
n=0
for d in "$OUT"/*/; do
  name=$(basename "$d")
  total=$(grep -o 'durationInFrames={[0-9A-Z_]*}' "$d/edit.tsx" | head -1 | grep -o '[0-9]*' || true)
  [ -n "$total" ] || total=$(grep -o 'const FRAMES = [0-9]*' "$d/edit.tsx" | grep -o '[0-9]*$')
  line=""
  for f in 0 $((total / 2)) $((total - 1)); do
    if ! nice -n 15 "$BIN" render "$d/edit.tsx" --frame "$f" -o "$d/f$f.png" > "$d/f$f.log" 2>&1; then
      echo "$name: FAIL at frame $f: $(grep -v '^\s*$' "$d/f$f.log" | grep -iv 'warn\|logger' | tail -1)"; status=1; continue 2
    fi
    range=$(ffmpeg -v error -i "$d/f$f.png" -vf signalstats,metadata=print:key=lavfi.signalstats.YMAX:file=- -f null - 2>/dev/null | grep -o 'YMAX=[0-9]*' | cut -d= -f2)
    low=$(ffmpeg -v error -i "$d/f$f.png" -vf signalstats,metadata=print:key=lavfi.signalstats.YMIN:file=- -f null - 2>/dev/null | grep -o 'YMIN=[0-9]*' | cut -d= -f2)
    if [ $(( ${range:-0} - ${low:-0} )) -lt 8 ]; then echo "$name: FAIL at frame $f: a blank frame"; status=1; continue 2; fi
    line="$line $f"
  done
  n=$((n + 1))
  echo "$name: ok (frames$line)"
done
echo "$n examples rendered; frames in $OUT"
exit $status
