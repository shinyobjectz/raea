#!/bin/sh
# The games gate (the no-V8 goal's gate 3), measured: each game played offscreen at its own fps (120) for SECONDS with
# key presses from a separate thread, then judged: no frame missed, frame work p99 <= 8.3 ms, input to present p99 <= 2
# frames, RSS <= 400 MB and growing <= 2 MB after the first minute. The growth is judged on RSS less its reusable pages:
# JavaScriptCore's allocator (libpas) gives free pages back with MADV_FREE_REUSABLE, and those stay resident, counted in
# RSS, until the kernel wants them. In a 4-minute pong all of RSS's 2-5 MB creep was such pages (vmmap's "WebKit Malloc"
# reclaimable), with RSS less them flat to +-0.5 MB. Raw RSS is still printed, and still capped at 400 MB.
#   react/bench/games/gate.sh [SECONDS] [GAME...]     (default 300 s, pong and platformer)
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
  RAEA_INPUT_KEYS="$keys" RAEA_AUDIO_MUTE=1 target/release/raea-motion play "bench/games/$g/edit.tsx" --input --offscreen \
    --size 1280x720 --seconds "$SECONDS_RUN" --json "$OUT/$g.json" > "$OUT/$g.log" 2>&1 || { echo "$g: the run failed (see $OUT/$g.log)"; status=1; continue; }
  python3 - "$g" "$OUT/$g.json" <<'PY' || status=1
import json, sys
g, path = sys.argv[1], sys.argv[2]
j = json.load(open(path))
r = j["rss_mb"]["each_second"]
at60 = r[min(60, len(r) - 1)]
u = j.get("reusable_mb", {}).get("each_second") or [0] * len(r)
live = [a - b for a, b in zip(r, u)]
l60 = live[min(60, len(live) - 1)]
checks = {
    "frames": (j["missed_vs_display"] == 0, f"{j['presented']} presented, {j['missed_vs_display']} missed"),
    "work p99": (j["work_ms"]["p99"] <= 8.3, f"{j['work_ms']['p99']:.2f} ms"),
    "input p99": (j["input_to_present_ms"]["p99_frames"] <= 2, f"{j['input_to_present_ms']['p99_frames']:.2f} frames"),
    "rss": (max(r) <= 400 and live[-1] - l60 <= 2, f"{r[0]} -> {at60} at 60 s -> {r[-1]} MB (max {max(r)}); less reusable {l60:.1f} -> {live[-1]:.1f} MB ({live[-1] - l60:+.1f})"),
}
fp = j.get("footprint_mb", {}).get("each_second") or []
if fp:
    f60 = fp[min(60, len(fp) - 1)]
    checks["footprint"] = (max(fp) <= 400 and fp[-1] - f60 <= 2, f"{fp[0]:.0f} -> {f60:.1f} at 60 s -> {fp[-1]:.1f} MB")
ok = all(c[0] for c in checks.values())
print(f"{g}: {'PASS' if ok else 'FAIL'}  " + "; ".join(f"{k} {v[1]}{'' if v[0] else ' (over)'}" for k, v in checks.items()))
sys.exit(0 if ok else 1)
PY
done
exit $status
