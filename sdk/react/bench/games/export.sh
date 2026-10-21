#!/bin/bash
# Exported games, end to end (sdk/react/docs/game-export.md): each game exported as a macOS app, the app copied to a temp
# folder outside the repo, and its self-test run there with a scrubbed environment (PATH=/usr/bin:/bin, no RAEA_*
# variables, HOME an empty folder): offscreen, scripted keys and pointer, sound mixed silently, no window. Then the
# self-test's snapshots are held against the dev tree's render of the same frames from the input the self-test logged
# (`raea-motion render --input`), SSIM >= 0.999.
#
#   sdk/react/bench/games/export.sh [EDIT.tsx ...]     (default: pong, the platformer, arena3d, the golden 50-game, a void game)
#   KEEP=1 keeps the exports; SECS=5 the self-test's length
#
# Needs target/release/raea-motion and a player, best the game profile's (target/game: fat LTO, ~74 MB; the release
# build's ships ~128 MB, and export-game takes whichever is newer):
#   cargo build --release && cargo build --profile game --bin raea-game --no-default-features --features game
cd "$(dirname "$0")/../.." || exit 1
REACT=$(pwd)
ROOT=$(cd .. && pwd)
SECS=${SECS:-5}
# outside the repo, and away from TMPDIR's other users
WORK=$(mktemp -d "${TMPDIR:-/tmp}/raea-game-export.XXXXXX")
case "$WORK" in "$ROOT"*) echo "the work folder is inside the repo: $WORK"; exit 1 ;; esac
VOID_GAME="$HOME/.raea-void/evals/runs/20261017-project-glm3/project--an-ad-and-a-game-in-one-project-switching-between-them/spaces/pong/edit.tsx"
if [ $# -gt 0 ]; then GAMES=("$@"); else
  GAMES=("$REACT/bench/games/pong/edit.tsx" "$REACT/bench/games/platformer/edit.tsx" "$REACT/bench/games/arena3d/edit.tsx" "$ROOT/engine/media/golden/50-game/edit.tsx" "$VOID_GAME")
fi
for b in raea-motion raea-game; do
  [ -x "target/release/$b" ] || { echo "no target/release/$b (see the header)"; exit 1; }
done

# the key sets a game's self-test cycles through (sets split by commas, keys by +, an empty set for none)
keys_for() {
  case "$1" in
    */platformer/*) echo "ArrowRight,ArrowRight+ArrowUp,ArrowRight," ;;
    */bench/games/pong/*) echo "ArrowUp,ArrowDown,,ArrowUp" ;;
    *) echo "ArrowLeft,ArrowRight,ArrowUp,ArrowDown, ,ArrowRight+ArrowUp," ;;
  esac
}

# each game's export, self-test and renders run under the shared lock (/tmp/raea-measure.lock, mkdir; about a minute a
# game, well under a 10 minute turn), so they don't land inside another agent's timing run (a run of this script put a
# 19 ms step on lightspeed's platformer); waited for at most 10 minutes, then run without it (said in the row)
HAVE=0
lock() {
  for _ in $(seq 1 300); do mkdir /tmp/raea-measure.lock 2>/dev/null && { HAVE=1; return; }; sleep 2; done
  echo "(no measure lock after 10 minutes: measured without it)"
}
unlock() { [ $HAVE = 1 ] && rmdir /tmp/raea-measure.lock 2>/dev/null; HAVE=0; }
trap unlock EXIT

status=0
printf "%-14s %-6s %-8s %-8s %-10s %-7s %-7s %-12s %-5s %-36s %s\n" game export app_MB game_MB presented missed inputs sound_peak load "frame:SSIM" verdict
for edit in "${GAMES[@]}"; do
  if [ ! -f "$edit" ]; then echo "SKIP $edit (missing)"; continue; fi
  dir=$(dirname "$edit")
  g=$(basename "$dir")
  case "$edit" in */.raea-void/*) g="void-$(basename "$dir")" ;; esac
  out="$WORK/$g"
  mkdir -p "$out"
  lock
  t0=$(python3 -c 'import time;print(time.time())')
  nice -n 15 target/release/raea-motion export-game "$edit" -o "$out/$g.app" --keys "$(keys_for "$edit")" > "$out/export.log" 2>&1
  if ! grep -q '^EXPORTED' "$out/export.log"; then echo "FAIL $g: the export failed: $(grep -m1 REFUSED "$out/export.log")"; status=1; unlock; continue; fi
  te=$(python3 -c "import time;print(round(time.time()-$t0,1))")
  # the app alone, somewhere else, run with nothing of the dev tree's around it
  run="$WORK/run-$g"
  mkdir -p "$run/home" "$run/tmp"
  cp -R "$out/$g.app" "$run/"
  exe=$(ls "$run/$g.app/Contents/MacOS/" | head -1)
  fps=$(python3 -c "import json;print(json.load(open('$run/$g.app/Contents/Resources/game/game.json'))['fps'])")
  snaps=""
  for s in 1 2.5 4; do snaps="$snaps --snapshot $s $run/snap-$s.png"; done
  load=$(sysctl -n vm.loadavg | awk '{print $2}')
  (cd "$run" && env -i HOME="$run/home" PATH=/usr/bin:/bin TMPDIR="$run/tmp" nice -n 15 "$run/$g.app/Contents/MacOS/$exe" \
    --selftest "${SECS}s" $snaps --input-log "$run/input.json" --json "$run/selftest.json" > "$run/selftest.log" 2>&1)
  verdict=$(tail -1 "$run/selftest.log")
  case "$verdict" in PASS*) ;; *) status=1 ;; esac
  # the dev tree's render of the snapshotted frames from the self-test's logged input
  eq=""
  worst=1
  for s in 1 2.5 4; do
    f=$(python3 -c "print(round($s * $fps))")
    if [ ! -f "$run/snap-$s.png" ]; then eq="$eq $f:none"; worst=0; continue; fi
    RAEA_READBACK=rgba nice -n 15 target/release/raea-motion render "$edit" --frame "$f" --input "$run/input.json" -o "$run/render-$s.png" > "$run/render-$s.log" 2>&1
    v=$(ffmpeg -hide_banner -i "$run/snap-$s.png" -i "$run/render-$s.png" -lavfi ssim -f null - 2>&1 | grep -o 'All:[0-9.]*' | cut -d: -f2)
    eq="$eq $f:${v:-err}"
    worst=$(python3 -c "print(min($worst, float('${v:-0}')))")
  done
  unlock
  same=$(python3 -c "print('PASS' if $worst >= 0.999 else 'FAIL')")
  [ "$same" = PASS ] || status=1
  python3 - "$run/selftest.json" "$out/export.log" "$g" "$te" "$eq" "${verdict%% *}" "$same" "$load" <<'EOF'
import json, sys
st = json.load(open(sys.argv[1])) if sys.argv[1] and __import__("os").path.exists(sys.argv[1]) else {}
ex = json.loads(open(sys.argv[2]).read().split("EXPORTED", 1)[1])
g, te, eq, verdict, same, load = sys.argv[3:9]
fr = st.get("frames", {})
inp = st.get("input", {})
snd = st.get("sound", {})
peak = snd.get("peak_dbfs")
sound = ("%.1f dBFS" % peak) if snd.get("mixed") else ("none" if not snd.get("in_game") else "SILENT")
print("%-14s %-6s %-8s %-8s %-10s %-7s %-7s %-12s %-5s %-36s selftest %s, render %s" % (g, te, ex["total_mb"], ex["game_mb"], "%s/%s" % (fr.get("presented"), fr.get("expected")), fr.get("missed"), inp.get("changes_reached"), sound, load, eq.strip(), verdict, same))
EOF
  if [ -z "$KEEP" ]; then
    case "$out" in "$WORK"/*) rm -rf "$out" "$run" ;; esac
  fi
done
if [ -z "$KEEP" ]; then rmdir "$WORK" 2>/dev/null; else echo "kept in $WORK"; fi
exit $status
