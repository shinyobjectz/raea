#!/bin/bash
# Video measured inside void, headless (apps/void/bench/video.py says what and how): a test instance of void with no window on
# screen (RAEA_VOID_HEADLESS=1: its stage paints into its texture and plays), its own home, ports and workspace, started
# here and killed when done. Prints PASS or FAIL per edit and for the re-bake.
#   apps/void/bench/video.sh [--out OUT.json] [--no-rebake] EDIT_FOLDER [EDIT_FOLDER...]
# Each folder holds an edit.tsx (its footage stays where it is: the workspace links to it, never copies or commits it).
# The re-bake runs on a copy of react/bench/timelines/titles. Build first: void (lane build -- cargo build --release in
# void/, with the probe) and raea-motion (react/).
set -u
cd "$(dirname "$0")/../../.."
ROOT=$PWD
OUT=${TMPDIR:-/tmp}/raea-void-video
JSON=$OUT/video.json
REBAKE=1
EDITS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --out) JSON=$2; shift 2 ;;
    --no-rebake) REBAKE=0; shift ;;
    *) EDITS+=("$1"); shift ;;
  esac
done
MCP=${RAEA_VOID_MCP_PORT:-7477}
PROBE=${RAEA_VOID_PROBE_PORT:-7478}
if lsof -iTCP:$MCP -iTCP:$PROBE -sTCP:LISTEN >/dev/null 2>&1; then echo "ports $MCP/$PROBE are taken: set RAEA_VOID_MCP_PORT and RAEA_VOID_PROBE_PORT"; exit 2; fi
rm -rf "$OUT/proj" "$OUT/home" "$OUT/shots"
mkdir -p "$OUT/proj/objects" "$OUT/home" "$OUT/shots"
specs=()
# an object per edit: its code copied (void writes beside it), everything else linked
for src in ${EDITS[@]+"${EDITS[@]}"}; do
  src=$(cd "$src" && pwd)
  slug=$(basename "$src")
  d=$OUT/proj/objects/$slug
  mkdir -p "$d"
  for e in "$src"/*; do
    n=$(basename "$e")
    case "$n" in
      space.json|final.mp4|render.json|previews) ;;
      *.tsx|*.ts) cp "$e" "$d/" ;;
      *) ln -s "$e" "$d/$n" ;;
    esac
  done
  printf '{"name":"%s","slug":"%s","created":0,"opened":0,"hue":200,"kind":"video"}' "$slug" "$slug" > "$d/space.json"
  specs+=("video:$slug")
done
if [ $REBAKE = 1 ]; then
  d=$OUT/proj/objects/titles-rebake
  mkdir -p "$d"
  cp sdk/react/bench/timelines/titles/edit.tsx "$d/"
  printf '{"name":"titles-rebake","slug":"titles-rebake","created":0,"opened":0,"hue":200,"kind":"video"}' > "$d/space.json"
  specs+=("rebake:titles-rebake")
fi
RAEA_VOID_HOME=$OUT/home RAEA_VOID_MCP_PORT=$MCP RAEA_VOID_PROBE=1 RAEA_VOID_PROBE_PORT=$PROBE RAEA_VOID_HEADLESS=1 \
  RAEA_VOID_BACKGROUND=1 RAEA_AUDIO_MUTE=1 nice -n 15 "${RAEA_VOID_BIN:-void/target/release/void}" "$OUT/proj" > "$OUT/void.log" 2>&1 &
pid=$!
trap 'kill $pid 2>/dev/null; wait $pid 2>/dev/null' EXIT
for _ in $(seq 60); do curl -s "localhost:$PROBE/report" >/dev/null && break; sleep 1; done
python3 apps/void/bench/video.py --proj "$OUT/proj" --out "$JSON" --shots "$OUT/shots" --mcp-port $MCP --probe-port $PROBE "${specs[@]}"
status=$?
echo "results: $JSON (void's log: $OUT/void.log)"
exit $status
