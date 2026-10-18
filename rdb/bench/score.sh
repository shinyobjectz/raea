#!/bin/bash
# raea's scoreboard: the same measures every time, appended to bench/score.jsonl with the commit, and the change since
# the last line printed. Every performance change is judged against it (the founder's standing goal, 2026-10-14).
#   bench/score.sh            build (lane), measure, record
#   bench/score.sh --no-build measure the binaries as they are
set -u
cd "$(dirname "$0")/.."
B=$PWD/target/release/rdb
[ "${1:-}" = "--no-build" ] || lane --batch rdb-release build -- cargo build --release --bin rdb --example kernel_bench --example timeline_bench 2>&1 | grep -E "^error" -A6
W=$(mktemp -d)
export RAEA_MEDIA=apple RUST_LOG=error
# wall seconds and peak MB of one command
timed() { /usr/bin/time -l "$@" > $W/out.txt 2> $W/time.txt; local s=$(awk '/ real /{print $1}' $W/time.txt); local m=$(awk '/maximum resident/{printf "%.0f", $1/1048576}' $W/time.txt); echo "$s $m"; }
read html_s html_mb <<< "$(cd examples && timed $B render html/ugc.w.html -o $W/h.mp4)"
html_frames=$(ffprobe -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of csv=p=0 $W/h.mp4)
read ugc_s ugc_mb <<< "$(cd examples/ugc && RAEA_DIRECT=1 timed $B render ugc.sql -o $W/u.mp4)"
read hook_s hook_mb <<< "$(cd examples/hook && timed $B render hook.sql -o $W/k.mp4)"
k10=$(./target/release/examples/kernel_bench 10000 | python3 -c "import sys,json; print(round(json.loads(sys.stdin.read())['kernel_us_per_frame'],1))")
tl=$(./target/release/examples/timeline_bench examples/ugc/ugc.sql | python3 -c "import sys,json; print(round(json.loads(sys.stdin.read())['timeline_ms'],2))")
line=$(python3 - "$html_s" "$html_mb" "$html_frames" "$ugc_s" "$ugc_mb" "$hook_s" "$hook_mb" "$k10" "$tl" <<'PY'
import json, sys, subprocess, time
a = sys.argv[1:]
sha = subprocess.run(["git", "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
dirty = bool(subprocess.run(["git", "status", "--porcelain", "--", "."], capture_output=True, text=True).stdout.strip())
print(json.dumps({"at": time.strftime("%Y-%m-%dT%H:%M"), "commit": sha + ("+" if dirty else ""),
    "html_gpu_s": float(a[0]), "html_gpu_mb": int(a[1]), "html_fps": round(int(a[2]) / float(a[0]), 1),
    "ugc_direct_s": float(a[3]), "ugc_direct_mb": int(a[4]), "hook_3d_s": float(a[5]), "hook_3d_mb": int(a[6]),
    "rules_10k_us": float(a[7]), "timeline_ugc_ms": float(a[8])}))
PY
)
prev=$(tail -1 bench/score.jsonl 2>/dev/null)
echo "$line" >> bench/score.jsonl
python3 - "$prev" "$line" <<'PY'
import json, sys
prev = json.loads(sys.argv[1]) if sys.argv[1] else {}
now = json.loads(sys.argv[2])
lower_better = lambda k: not k.endswith("fps")
for k, v in now.items():
    if k in ("at", "commit"):
        continue
    p = prev.get(k)
    if isinstance(p, (int, float)) and p:
        d = (v - p) / p * 100
        good = (d < -2) if lower_better(k) else (d > 2)
        bad = (d > 2) if lower_better(k) else (d < -2)
        mark = "better" if good else "WORSE" if bad else ""
        print(f"{k:16s} {v:>10}   was {p:<10} {d:+6.1f}% {mark}")
    else:
        print(f"{k:16s} {v:>10}")
print(f"commit {now['commit']} (bench/score.jsonl)")
PY
rm -rf "$W"
