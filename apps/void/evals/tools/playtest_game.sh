#!/bin/bash
# The playtest stack on a game an agent built (sdk/react/docs/playtest.md): the object is copied to OUT/game (the agent's
# folder is never written), tools/game.playtest.ts goes beside its edit as edit.playtest.ts, then the bots
# (sdk/react/bench/playtest/run.ts: random, novelty, beam; machine coverage; softlocks; finishable), the views (rendered
# offscreen at utility QoS) and the judge (rules, plus clef on clean views; prints its calls and cost).
#   apps/void/evals/tools/playtest_game.sh OBJECT_DIR OUT [--quick]
# Needs the game's `export const playtest` (sdk/motion/src/games.ts); without one, run.ts says so and stops.
set -u
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
obj=$1; out=$2; shift 2
mkdir -p "$out"
rm -rf "$out/game"; cp -R "$obj" "$out/game"
cp "$here/game.playtest.ts" "$out/game/edit.playtest.ts"
cd "$repo"
echo "== bots"
bun sdk/react/bench/playtest/run.ts "$out/game/edit.tsx" "$out" "$@" || exit 1
echo "== views"
nice -n 15 bun sdk/react/bench/playtest/views.ts "$out/game/edit.tsx" "$out" || echo "views failed (the judge still runs its rules)"
echo "== judge"
bun sdk/react/bench/playtest/judge.ts "$out" --playtest "$out/game/edit.playtest.ts"
