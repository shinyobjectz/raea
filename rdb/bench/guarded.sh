#!/bin/bash
# Run a render with a memory ceiling (the orchestrator's rules, 2026-10-13): refuse below 25% free memory, run through
# `lane eval`, and kill it if VideoToolbox's decoder service passes a quarter of the Mac's memory or the render itself
# passes 3/8 (4 and 6 GB on the 16 GB Mac this was written on; 16 and 24 GB on the 64 GB M5 Pro since 2026-10-16).
#   bench/guarded.sh <command...>
set -u
free=$(memory_pressure 2>/dev/null | awk -F': ' '/free percentage/ {gsub("%","",$2); print $2}')
mem_mb=$(( $(sysctl -n hw.memsize) / 1048576 )); vt_cap=$(( mem_mb / 4 )); render_cap=$(( mem_mb * 3 / 8 ))
if [ "${free:-0}" -lt 25 ]; then echo "guarded: only ${free}% memory free (need 25%)" >&2; exit 3; fi
mb() { # resident memory of a pid in MB, from ps (instant; top took a second a sample and read 0 for the render)
  ps -o rss= -p "$1" 2>/dev/null | awk '{print int($1/1024)}'
}
lane eval -- "$@" &
job=$!
peak_vt=0; peak_r=0
# exit is noticed within 0.2 s and memory sampled every second (polling every 2 s through top left a finished 3 s
# render waiting up to 3 s more)
tick=0
while kill -0 $job 2>/dev/null; do
  sleep 0.2
  tick=$((tick + 1))
  [ $((tick % 5)) -eq 0 ] || continue
  kill -0 $job 2>/dev/null || break
  for p in $(pgrep -f VTDecoderXPCService); do v=$(mb $p); [ "${v:-0}" -gt $peak_vt ] && peak_vt=$v; if [ "${v:-0}" -gt $vt_cap ]; then echo "guarded: VTDecoderXPCService at ${v} MB: killing" >&2; pkill -f "rdb render"; kill $p; fi; done
  for p in $(pgrep -f "rdb render"); do v=$(mb $p); [ "${v:-0}" -gt $peak_r ] && peak_r=$v; if [ "${v:-0}" -gt $render_cap ]; then echo "guarded: render at ${v} MB: killing" >&2; kill $p; fi; done
done
wait $job; rc=$?
echo "guarded: exit $rc, peak decoder ${peak_vt} MB, peak render ${peak_r} MB" >&2
exit $rc
