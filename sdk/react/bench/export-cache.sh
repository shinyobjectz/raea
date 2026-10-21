#!/bin/bash
# The export cache and renders a composition plays (sdk/react/src/motion.rs, export_cache). A cut laid from a shot's
# render must miss the cache once the shot is rendered again: the cache leaves its own earlier exports out of the key,
# but not one the composition plays (10-21: anything/cut/intro.tsx came back unchanged after a shot was re-rendered).
#   bash sdk/react/bench/export-cache.sh [raea-motion]
# Prints PASS/FAIL lines; exits non-zero on a FAIL.
set -u
M=${1:-raea-motion}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
export RAEA_EXPORT_CACHE=$W/cache
mkdir -p "$W/edit/media"
cd "$W/edit" || exit 1

shot() {
  cat > shot.tsx <<EOF
import { AbsoluteFill, Composition, registerRoot } from "@raea/motion";
registerRoot(() => <Composition id="shot" component={() => <AbsoluteFill style={{ background: "$1" }} />} width={64} height={64} fps={30} durationInFrames={6} />);
EOF
  "$M" render shot.tsx -o media/shot.mp4 > /dev/null 2>&1
}
# the shot's path built from a template, as the intro cut builds its shots': the bundle holds only "shot"
cat > cut.tsx <<'EOF'
import { AbsoluteFill, Composition, registerRoot, staticFile, Video } from "@raea/motion";
const NAME = "shot";
registerRoot(() => <Composition id="cut" component={() => <AbsoluteFill><Video src={staticFile(`${NAME}.mp4`)} style={{ width: 64, height: 64 }} /></AbsoluteFill>} width={64} height={64} fps={30} durationInFrames={6} />);
EOF
red() { ffmpeg -v error -i cut.mp4 -frames:v 1 -vf "crop=8:8:28:28,scale=1:1" -f rawvideo -pix_fmt rgb24 - | od -An -tu1 | awk '{print $1}'; }

fails=0
check() { if [ "$2" = 1 ]; then echo "PASS $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi; }

shot "#ff0000"
first=$("$M" render cut.tsx -o cut.mp4 2>&1)
r1=$(red)
again=$("$M" render cut.tsx -o cut.mp4 2>&1)
check "an unchanged cut comes from the cache" "$(grep -q 'from the export cache' <<< "$again" && echo 1)"
shot "#0000ff"
after=$("$M" render cut.tsx -o cut.mp4 2>&1)
r2=$(red)
check "the cut misses once its shot is rendered again" "$(grep -q 'from the export cache' <<< "$after" || echo 1)"
check "the cut shows the new shot (red $r1 then $r2)" "$([ "${r1:-0}" -gt 200 ] && [ "${r2:-255}" -lt 60 ] && echo 1)"
grep -q 'APPLIED' <<< "$first" || { echo "FAIL the first cut didn't render: $first"; fails=$((fails + 1)); }
exit $fails
