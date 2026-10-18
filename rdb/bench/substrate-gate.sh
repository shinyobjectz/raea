#!/bin/bash
# The substrate gate (docs/substrate.feature): every phase must leave all of this unchanged against today's references
# (bench/ref/, recorded 2026-10-14 before the substrate work, and examples/out/ref/).
#   bench/substrate-gate.sh [--record]     --record writes the references instead of comparing
# Builds through the lane (release), renders guarded. Prints one PASS/FAIL line per check and exits non-zero on a FAIL.
set -u
cd "$(dirname "$0")/.."
REF=bench/ref
mkdir -p "$REF"
RECORD=0; [ "${1:-}" = "--record" ] && RECORD=1
B=$PWD/target/release/rdb
W=$(mktemp -d)
fails=0
# the byte-exact checks paint pages with vello_cpu: the GPU compositor (engine/src/quads, 515c8cf) matches it to within a
# few levels, not to the byte, so with it every UGC reference failed though nothing else changed (with RAEA_PAINT=cpu,
# HEAD gives G8's references exactly, soft and Apple decode). The GPU paint is checked against the CPU's at the end.
export RAEA_PAINT=cpu
# phase 4: nothing may start a duckdb process (a stand-in on PATH fails loudly and leaves a mark)
mkdir -p $W/nodb && printf '#!/bin/sh\necho "duckdb was called: $*" >&2\ntouch %s/duckdb-called\nexit 99\n' "$W" > $W/nodb/duckdb && chmod +x $W/nodb/duckdb
export PATH=$W/nodb:$PATH
lane --batch build "substrate-gate-$(date +%s)" -- cargo build --release --bin rdb --example states_digest 2>&1 | grep -E "^error" -A8
gate() { # name, file produced in $W
  local name=$1 got=$W/$2
  if [ $RECORD = 1 ]; then cp "$got" "$REF/$2"; echo "RECORD $name"; return; fi
  if cmp -s "$got" "$REF/$2"; then echo "PASS $name"; else echo "FAIL $name"; diff "$REF/$2" "$got" | head -5; fails=$((fails+1)); fi
}

# 1. every evaluated value of every frame of every example edit (no GPU)
EDITS="examples/hook/hook.sql examples/ugc/ugc.sql examples/ugc/v-empty.sql examples/ugc/v-noclips.sql examples/ugc/v-noclip2d.sql"
./target/release/examples/states_digest $EDITS > $W/states.txt 2>&1; gate "frame states, every frame of every example edit" states.txt

# 2. the live edit: the hook built from its snippets, its moves, save/load, every frame, then undo all the way
E=$W/live.rdb
$B new $E > /dev/null
python3 - "$B" "$E" > $W/live.txt <<'EOF'
import subprocess, sys
B, E = sys.argv[1], sys.argv[2]
blocks = [b.strip() for b in open("examples/hook/snippets.sql").read().split("\n--\n")]
for b in blocks:
    if b and not all(l.strip().startswith("--") for l in b.splitlines()):
        r = subprocess.run([B, "apply", E, b], capture_output=True, text=True, cwd="examples/hook")
        print("apply", r.returncode, r.stdout.strip().splitlines()[-1:] if r.stdout.strip() else "", r.stderr.strip()[:120])
EOF
$B moves $E >> $W/live.txt 2>&1
./target/release/examples/states_digest $E | sed "s|$E|live.rdb|" >> $W/live.txt
for i in $(seq 1 40); do $B undo $E > /dev/null 2>&1 || break; done
$B moves $E >> $W/live.txt 2>&1
gate "live edit: snippets applied, moves, every frame from the saved file, undo to empty" live.txt

# 2b. the agent's queries on our columns, against DuckDB's recorded answers
rm -f $W/q.rdb; $B new $W/q.rdb > /dev/null
python3 bench/queries.py $B $W/q.rdb > $W/queries.txt 2>&1
grep -q "^26/26 " $W/queries.txt && echo "PASS queries: 26/26 answer as DuckDB did" || { echo "FAIL queries"; tail -5 $W/queries.txt; fails=$((fails+1)); }

# 3. G1's pass test (G2 and the brief checks below it were rrl's, deleted with rrl on 2026-10-17)
(cd examples/hook && $B g1 . 2>&1 | grep -v "WARN" | grep -v "ERROR" | sed -E "s/ in [0-9.]+s,/ in Ns,/" > $W/g1.txt; echo "exit ${PIPESTATUS[0]}" >> $W/g1.txt); gate "G1 (live edit, frames identical, undo)" g1.txt

# 5. rendered frames (hash-only, nothing on disk): the engine's GPU path, the step 0 path, the direct path
(cd examples/hook && RAEA_MEDIA=apple ../../bench/guarded.sh $B render hook.sql -o $W/hook-gpu.txt > /dev/null 2>&1); gate "render: hook, engine GPU path" hook-gpu.txt
(cd examples/hook && RAEA_READBACK=rgba RAEA_COMPOSITE=ui RAEA_MEDIA=apple ../../bench/guarded.sh $B render hook.sql -o $W/hook-rgba.txt > /dev/null 2>&1); gate "render: hook, step 0 path" hook-rgba.txt
(cd examples/ugc && RAEA_MEDIA=apple ../../bench/guarded.sh $B render ugc.sql -o $W/ugc-gpu.txt > /dev/null 2>&1); gate "render: UGC, engine GPU path" ugc-gpu.txt
(cd examples/ugc && RAEA_DIRECT=1 RAEA_MEDIA=apple ../../bench/guarded.sh $B render ugc.sql -o $W/ugc-direct.txt > /dev/null 2>&1); gate "render: UGC, direct" ugc-direct.txt
# the step 0 path decodes clips to BGRA in VideoToolbox, so its frames follow Apple's YUV->RGB conversion, which macOS 27
# changed (frames 0-592, the video ones, differ; the end card and every path that converts NV12 itself are unchanged;
# no decode code changed): each later macOS major has its own reference beside the original (ugc-rgba-macosNN.txt)
OSV=$(sw_vers -productVersion 2>/dev/null | cut -d. -f1)
RGBA=ugc-rgba.txt; [ -f "$REF/ugc-rgba-macos$OSV.txt" ] && RGBA=ugc-rgba-macos$OSV.txt
(cd examples/ugc && RAEA_READBACK=rgba RAEA_COMPOSITE=ui RAEA_MEDIA=apple ../../bench/guarded.sh $B render ugc.sql -o $W/$RGBA > /dev/null 2>&1); gate "render: UGC, step 0 path ($RGBA)" $RGBA
[ -e $W/duckdb-called ] && { echo "FAIL a duckdb process was started"; fails=$((fails+1)); } || echo "PASS no duckdb process"
if [ $RECORD = 0 ]; then
  cmp -s $W/ugc-gpu.txt examples/out/ref/gpu-apple-a.txt && echo "PASS UGC GPU path = G8's reference (examples/out/ref/gpu-apple-a.txt)" || { echo "FAIL UGC GPU path vs G8's reference"; fails=$((fails+1)); }
  G7=examples/out/ref/hash-apple-a.txt; [ -f "examples/out/ref/hash-apple-a-macos$OSV.txt" ] && G7=examples/out/ref/hash-apple-a-macos$OSV.txt
  cmp -s $W/$RGBA $G7 && echo "PASS UGC step 0 path = G7's reference ($G7)" || { echo "FAIL UGC step 0 path vs G7's reference ($G7)"; fails=$((fails+1)); }
  # the CPU render (docs/cpu.feature C1): no GPU, the same frames as the GPU's
  (cd examples/ugc && RAEA_CPU=1 RAEA_MEDIA=apple ../../bench/guarded.sh $B render ugc.sql -o $W/ugc-cpu.txt > /dev/null 2>&1)
  cmp -s $W/ugc-cpu.txt examples/out/ref/gpu-apple-a.txt && echo "PASS UGC CPU render = G8's GPU reference" || { echo "FAIL UGC CPU render vs G8's GPU reference"; fails=$((fails+1)); }
fi
# the GPU paint against the CPU's: UGC frames across the edit read back as RGB both ways (not through an encoder,
# whose noise on two near-identical inputs alone took mp4s to SSIM 0.985), every one SSIM >= 0.999
AT=10,49,200,400,700
for p in gpu cpu; do (cd examples/ugc && RAEA_PAINT=$p RAEA_READBACK=rgba RAEA_MEDIA=apple ../../bench/guarded.sh $B render ugc.sql -o $W/paint-$p --at $AT > /dev/null 2>&1); done
worst=1
for f in $(echo $AT | tr , ' '); do
  n=$(printf "%05d" $f)
  v=$(ffmpeg -i $W/paint-gpu/frame-$n.png -i $W/paint-cpu/frame-$n.png -lavfi ssim -f null - 2>&1 | grep -o 'All:[0-9.]*' | cut -d: -f2)
  worst=$(python3 -c "print(min($worst, ${v:-0}))")
done
python3 -c "import sys; sys.exit(0 if $worst >= 0.999 else 1)" && echo "PASS UGC GPU paint = CPU paint (frames $AT, worst SSIM $worst)" || { echo "FAIL UGC GPU paint vs CPU paint (frames $AT, worst SSIM $worst)"; fails=$((fails+1)); }
rm -rf "$W"
echo "gate: $fails failing"
exit $fails
