#!/bin/bash
# ZeroHeap's guard (docs/lightspeed.feature, goal 6): every Rust thread's allocations a frame, on the three games played
# live, held against their budgets (bench/games/zeroheap_budget.json). A game over its budget fails the gate: a change put
# per-frame allocations back. The counts don't depend on load, so this runs under any load.
#   sdk/react/bench/games/zeroheap.sh [--seconds S] [--update]
# RAEA_MOTION_BIN: another raea-motion. --update writes the measured counts, plus the budget's margin, as the new budgets
# (after a change that lowers them).
cd "$(dirname "$0")/../.."
BUDGET=bench/games/zeroheap_budget.json
GAMES="pong platformer arena3d"
KEYS="ArrowRight,ArrowUp,ArrowLeft+ArrowUp,ArrowDown,"
MARGIN=1.15
MOTION=${RAEA_MOTION_BIN:-target/release/raea-motion}
SECS=20; UPDATE=0
while [ $# -gt 0 ]; do
  case $1 in --seconds) SECS=$2; shift ;; --update) UPDATE=1 ;; esac
  shift
done
OUT=$(mktemp -d "${TMPDIR:-/tmp}/raea-zeroheap.XXXXXX")
trap 'rm -rf "$OUT"' EXIT

# one game's Rust allocations a frame, from the report the play prints (its JSON, among the log lines around it)
measure() {
  RAEA_ZEROHEAP=1 RAEA_AUDIO_MUTE=1 RAEA_INPUT_KEYS="$KEYS" "$MOTION" play "bench/games/$1/edit.tsx" --input --offscreen \
    --size 1280x720 --seconds "$SECS" > "$OUT/$1.out" 2> "$OUT/$1.err"
  cat "$OUT/$1.out" "$OUT/$1.err" > "$OUT/$1.log"
  jq -Rs 'match("\\{[\\s\\S]*\\}").string | fromjson | (.report // .) | .rust_allocs.all_threads_per_frame' "$OUT/$1.log"
}

got="{}"
for g in $GAMES; do
  v=$(measure "$g") && [ -n "$v" ] || { echo "FAIL $g: no allocation count (see the play's output below)"; tail -20 "$OUT/$g.log"; exit 1; }
  got=$(jq -c --arg g "$g" --argjson v "$v" '.[$g] = $v' <<< "$got")
done
if [ $UPDATE = 1 ]; then
  jq --argjson m $MARGIN 'map_values(. * $m | ceil)' <<< "$got" > "$BUDGET"
  # (as Python printed the dict)
  echo "budgets written: {$(jq -r 'to_entries | map("@" + .key + "@: " + (.value | tostring)) | join(", ")' "$BUDGET" | tr @ "'")}"
  exit 0
fi
budget=$( [ -f "$BUDGET" ] && cat "$BUDGET" || echo "{}")
bad=0
for g in $GAMES; do
  v=$(jq -r --arg g "$g" '.[$g]' <<< "$got"); b=$(jq -r --arg g "$g" '.[$g] // "None"' <<< "$budget")
  if [ "$b" = None ] || awk -v v="$v" -v b="$b" 'BEGIN { exit !(v + 0 <= b + 0) }'; then ok=PASS; else ok=FAIL; bad=$((bad + 1)); fi
  printf '%s %s: %.0f Rust allocations a frame (budget %s)\n' "$ok" "$g" "$v" "$b"
done
[ $bad = 0 ]
