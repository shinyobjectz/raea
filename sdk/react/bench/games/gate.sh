#!/bin/sh
# The games gate (the no-V8 goal's gate 3), measured: each game played offscreen at its own fps (120) for SECONDS with
# key presses from a separate thread, then judged: no frame missed, frame work p99 <= 8.3 ms, input to present p99 <= 2
# frames, RSS <= 400 MB and growing <= 2 MB after the first minute. The growth is judged on RSS less its reusable pages:
# JavaScriptCore's allocator (libpas) gives free pages back with MADV_FREE_REUSABLE, and those stay resident, counted in
# RSS, until the kernel wants them. In a 4-minute pong all of RSS's 2-5 MB creep was such pages (vmmap's "WebKit Malloc"
# reclaimable), with RSS less them flat to +-0.5 MB. Raw RSS is still printed, and still capped at 400 MB.
#   sdk/react/bench/games/gate.sh [SECONDS] [GAME...]     (default 300 s, pong and platformer; arena3d and room3d are the 3D ones)
# Build first: lane build -- cargo build --release --bin raea-motion (in react/).
set -e
cd "$(dirname "$0")/../.."
SECONDS_RUN=${1:-300}
[ $# -gt 0 ] && shift
GAMES=${*:-pong platformer}
OUT=${TMPDIR:-/tmp}/raea-games-gate
mkdir -p "$OUT"
status=0
for g in $GAMES; do
  keys=""
  [ "$g" = platformer ] && keys="ArrowRight,ArrowRight+ArrowUp,ArrowRight,"
  [ "$g" = arena3d ] && keys="ArrowRight,ArrowUp,ArrowLeft+ArrowUp,ArrowDown,"
  [ "$g" = room3d ] && keys="ArrowRight,ArrowRight,ArrowUp,ArrowLeft,ArrowLeft,ArrowDown,"
  RAEA_INPUT_KEYS="$keys" RAEA_AUDIO_MUTE=1 target/release/raea-motion play "bench/games/$g/edit.tsx" --input --offscreen \
    --size 1280x720 --seconds "$SECONDS_RUN" --json "$OUT/$g.json" > "$OUT/$g.log" 2>&1 || { echo "$g: the run failed (see $OUT/$g.log)"; status=1; continue; }
  # the judgement, from the run's JSON: jq takes the numbers out (one tab-separated line), awk judges and prints
  jq -r '
    .rss_mb.each_second as $r | ($r | length) as $n
    | ((.reusable_mb.each_second // []) | if length == 0 then [$r[] | 0] else . end) as $u
    | [range([$n, ($u | length)] | min) as $i | $r[$i] - $u[$i]] as $live
    | (.footprint_mb.each_second // []) as $fp
    | .hitches_over_2ms as $h
    | [.missed_vs_display, .presented, .work_ms.p99, .input_to_present_ms.p99_frames,
       $r[0], $r[[60, $n - 1] | min], $r[-1], ($r | max), $live[[60, ($live | length) - 1] | min], $live[-1],
       ($fp | length), $fp[0], $fp[[60, ($fp | length) - 1] | min], $fp[-1], ($fp | max),
       (if $h == null then 0 else 1 end), (.work_ms.p999 // 0), $h.count, $h.max,
       ($h.first // [] | .[:4][] | .frame, .work, (.step // 0), (.present // 0))]
    | map(if . == null then "" else tostring end) | @tsv' "$OUT/$g.json" | awk -F'\t' -v g="$g" '
    function check(name, ok, text) { line = line (line == "" ? "" : "; ") name " " text (ok ? "" : " (over)"); all = all && ok }
    {
      all = 1; line = ""
      check("frames", $1 == 0, $2 " presented, " $1 " missed")
      check("work p99", $3 <= 8.3, sprintf("%.2f ms", $3))
      check("input p99", $4 <= 2, sprintf("%.2f frames", $4))
      check("rss", $8 <= 400 && $10 - $9 <= 2, sprintf("%s -> %s at 60 s -> %s MB (max %s); less reusable %.1f -> %.1f MB (%+.1f)", $5, $6, $7, $8, $9, $10, $10 - $9))
      if ($11 > 0) check("footprint", $15 <= 400 && $14 - $13 <= 2, sprintf("%.0f -> %.1f at 60 s -> %.1f MB", $12, $13, $14))
      print g ": " (all ? "PASS" : "FAIL") "  " line
      # lightspeed F2 and F6, printed and not judged (yet): the work p99.9 and first-use hitches (frames past the first
      # second over 2 ms)
      if ($16 == 1) {
        h = sprintf("%s: work p99.9 %.2f ms; hitches over 2 ms %s (max %.2f ms)", g, $17, $18, $19)
        for (i = 20; i + 3 <= NF; i += 4) h = h sprintf("; frame %s %.1f ms (step %.1f, present %.1f)", $i, $(i + 1), $(i + 2), $(i + 3))
        print h
      }
      exit !all
    }
    END { if (NR == 0) exit 1 }' || status=1
done
exit $status
