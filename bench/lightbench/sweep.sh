#!/usr/bin/env bash
# LightBench's sweeps (docs/lightbench.feature LB4): light counts by mode, then detail by distance and field of view,
# at a fixed resolution, each a headless run of the release build. Prints one line a run; frames go to OUT.
# usage: bench/lightbench/sweep.sh OUT_DIR   (build first: cargo build --release -p raea-lightbench)
set -euo pipefail
out="${1:?usage: sweep.sh OUT_DIR}"
mkdir -p "$out"
root="$(cd "$(dirname "$0")/../.." && pwd)"
lb="$root/target/release/lightbench"
[ -x "$lb" ] || { echo "no $lb: cargo build --release -p raea-lightbench" >&2; exit 1; }
run() { "$lb" --t 0 --size 1280x720 "$@" 2>/dev/null | grep -E '^(FRAME|LOD|RAYS|GPU)' || { echo "FAILED: $*" >&2; return 1; }; }

echo "# frame time (the GPU waited for each frame) by light count and mode"
for n in 16 64 256; do
  run --lights "$n" --bench 30 --shot "$out/all-$n.png"
  for m in uniform weighted; do run --lights "$n" --mode "$m" --k 4 --bench 30 --raw --shot "$out/$m-$n.png"; done
done

echo "# bounces (LB5): rays a pixel and the lighting pass's time, 0 to 2 bounces"
for b in 0 1 2; do run --lights 64 --mode weighted --k 4 --bounce "$b" --bench 30 --shot "$out/bounce-$b.png"; done

echo "# detail: full against adaptive, then the field of view"
run --scene field --detail full --view lod --shot "$out/field-full.png"
for fov in 45 20 8; do run --scene field --fov "$fov" --view lod --shot "$out/field-fov$fov.png"; done
echo "# frame time of the field, full against adaptive"
for d in full adaptive; do run --scene field --detail "$d" --mode weighted --bench 30 --shot "$out/field-time-$d.png"; done
