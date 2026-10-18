#!/bin/bash
# The corpus harness: real agent-written compositions (and a few synthetic ones) run through the no-V8 engine's main
# paths, one PASS/FAIL line per edit with the reason. findings.md says what broke and why.
#   react/bench/corpus/run.sh [LIST] [FILTER]
# LIST (default react/bench/corpus/list.txt) holds one edit per line: a path relative to the repo, or ~/... for void's
# eval spaces; '#' starts a comment. Edits that aren't on this Mac print SKIP. FILTER keeps only paths containing it.
# For each edit:
#   1. render:  raea-motion render EDIT -o OUT.mp4 (the GPU page path; the 3D path for 3D), its frames counted against
#      the edit's own length when it states one (score's `export const config`)
#   2. a timeline or a 3D scene: a baked play (--baked --offscreen, and --3d for 3D) snapshotted at four frames across it, the same
#      frames rendered with RAEA_READBACK=rgba, each pair compared by SSIM (>= 0.999 passes)
#   3. a game (useInput): a live play with random keys and its --input-log, then those frames rendered from the log,
#      as react/bench/games/replay.sh does (>= 0.999 passes)
# A panic (in stderr, or panic.txt / crash.txt in the flight recorder's session folder, which is named by the pid) and
# any frame the play painted on the CPU (the play report's cpu_paint) are reported. Times are wall times. The runs take
# turns on the shared measurement lock (/tmp/raea-measure.lock): held in batches of at most ~10 minutes, then released
# for 60 s so timing runs get theirs; CORPUS_TIMED=1 holds it for the whole run.
# Generation is off: paid models refused (RAEA_GEN_PAID=0) and no local SAO, so only sounds already made play.
# Outputs go to /tmp/raea-corpus (work/ is emptied after each edit; logs/ keeps each edit's logs for diagnosis).
# Build first: lane build -- cargo build --release --bin raea-motion (in react/).
cd "$(dirname "$0")/../.." || exit 2
REPO=$(cd .. && pwd)
LIST=${1:-bench/corpus/list.txt}
FILTER=${2:-}
BIN=$PWD/target/release/raea-motion
LOGS=$HOME/Library/Logs/raea/raea-motion
mkdir -p /tmp/raea-corpus/work /tmp/raea-corpus/logs
W=/tmp/raea-corpus/work
export RAEA_AUDIO_MUTE=1 RAEA_GEN_PAID=0 RAEA_SAO=/nonexistent/raea-sao
# turns on the shared measurement lock: the renders hold it in batches of at most ~10 minutes (no new command starts
# after 8), then let it go for 60 s so the timing runs get theirs; CORPUS_TIMED=1 holds it for the whole run
HELD=0
HELD_AT=0
take() {
  if [ "$HELD" = 1 ]; then
    [ "${CORPUS_TIMED:-0}" = 1 ] || [ $(($(date +%s) - HELD_AT)) -lt 480 ] && return
    give
    sleep 60
  fi
  until mkdir /tmp/raea-measure.lock 2>/dev/null; do sleep 5; done
  HELD=1
  HELD_AT=$(date +%s)
}
give() { if [ "$HELD" = 1 ]; then rmdir /tmp/raea-measure.lock; HELD=0; fi; }
trap give EXIT
trap 'exit 130' INT TERM

# run LOG LIMIT_S CMD...: CMD in the background with a watchdog; sets RC, SECS and CRASH (a panic or crash note)
run() {
  local log=$1 limit=$2
  shift 2
  take
  local t0
  t0=$(python3 -c 'import time; print(time.time())')
  "$@" > "$log" 2>&1 &
  local pid=$!
  # the watchdog polls each second, so it ends within a second of the command
  ( i=0; while kill -0 "$pid" 2>/dev/null; do sleep 1; i=$((i + 1)); [ "$i" -lt "$limit" ] || { kill "$pid" && echo "raea-corpus: killed after ${limit} s" >> "$log"; break; }; done ) &
  local dog=$!
  wait "$pid"
  RC=$?
  wait "$dog" 2>/dev/null
  SECS=$(python3 -c "import time; print(round(time.time() - $t0, 1))")
  CRASH=""
  local d
  d=$(ls -d "$LOGS"/*-"$pid" 2>/dev/null | tail -1)
  if [ -n "$d" ] && [ -s "$d/panic.txt" ]; then CRASH="panic: $(head -c 300 "$d/panic.txt" | tr '\n' ' ')"; fi
  if [ -z "$CRASH" ] && [ -n "$d" ] && [ -s "$d/crash.txt" ]; then CRASH="crash: $(head -c 300 "$d/crash.txt" | tr '\n' ' ')"; fi
  if [ -z "$CRASH" ] && grep -q "panicked at" "$log"; then CRASH="panic: $(grep -m1 -A1 'panicked at' "$log" | tr '\n' ' ' | head -c 300)"; fi
  if grep -q "raea-corpus: killed" "$log"; then CRASH="hung: killed after ${limit} s${CRASH:+; }$CRASH"; fi
}

# ssim A B: SSIM of two images as RGB ("" when ffmpeg couldn't compare them, e.g. different sizes)
ssim() {
  ffmpeg -v error -i "$1" -i "$2" -filter_complex "[0]format=rgb24[a];[1]format=rgb24[b];[a][b]ssim=stats_file=-" -f null - 2>&1 | grep -o 'All:[0-9.]*' | tail -1 | cut -d: -f2
}

# why LOG: the line of a failed run's log that says why
why() {
  grep -v '^\[\(INFO\|WARN\)' "$1" | grep -v 'bevy_log\|render statistics' | grep -m1 -iE 'refused|error|panicked|failed|cannot|no such|not found|wasn.t' | head -c 240
}

dims() { python3 -c "from PIL import Image; im=Image.open('$1'); print('%dx%d' % im.size)" 2>/dev/null || sips -g pixelWidth -g pixelHeight "$1" 2>/dev/null | awk '/pixelWidth/{w=$2}/pixelHeight/{h=$2}END{print w"x"h}'; }

status=0
pass=0
fail=0
skip=0
while IFS= read -r line || [ -n "$line" ]; do
  p=$(echo "${line%%#*}" | xargs)
  [ -n "$p" ] || continue
  [ -z "$FILTER" ] || [[ "$p" == *"$FILTER"* ]] || continue
  case "$p" in "~/"*) edit="$HOME/${p#\~/}" ;; /*) edit=$p ;; *) edit="$REPO/$p" ;; esac
  # a short name: the last two folders, or for a void space its run and scenario (and the space when not eval)
  name=$(echo "$p" | awk -F/ '{n=NF; s=$(n-1); if (n>2) s=$(n-2)"/"s; for (i=1;i<=n;i++) if ($i=="spaces") { s=$(i-2)"/"$(i-1); if ($(i+1)!="eval") s=s"/"$(i+1) } print s}' | sed 's|/project--|/|; s|/ugc--|/|; s|/app--|/|')
  if [ ! -f "$edit" ]; then echo "SKIP  $name: not on this Mac ($p)"; skip=$((skip + 1)); continue; fi
  KIND=timeline
  CONF=""
  eval "$(python3 bench/corpus/info.py "$edit")"
  slug=$(echo "$p" | tr '/~' '__' | tr -cd 'A-Za-z0-9_.-' | tail -c 120)
  L=/tmp/raea-corpus/logs/$slug
  mkdir -p "$L"
  rm -f /tmp/raea-corpus/work/*
  notes=""
  bad=""

  # 1. the render
  # shellcheck disable=SC2086
  run "$L/render.log" 900 "$BIN" render "$edit" -o "$W/out.mp4" $CONF
  if [ "$RC" != 0 ] || [ ! -s "$W/out.mp4" ]; then
    bad="render failed (exit $RC, ${SECS} s): ${CRASH:-$(why "$L/render.log")}"
  else
    n=$(grep -o 'APPLIED  [0-9]* frames' "$L/render.log" | grep -o '[0-9]*' | head -1)
    msf=$(grep -o '([0-9.]* ms a frame' "$L/render.log" | grep -o '[0-9.]*' | head -1)
    fps=$(ffprobe -v error -select_streams v:0 -show_entries stream=r_frame_rate -of csv=p=0 "$W/out.mp4" | python3 -c "import sys; a,b=sys.stdin.read().strip().split('/'); print(round(int(a)/int(b), 3))")
    size=$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=s=x:p=0 "$W/out.mp4")
    want=$(echo "$CONF" | grep -o -- '--frames [0-9]*' | grep -o '[0-9]*$')
    notes="render ${n} frames ${size}@${fps} in ${SECS} s (${msf} ms/f)"
    [ -z "$want" ] || [ "$want" = "$n" ] || bad="render made $n frames, the edit is $want"
    [ -z "$CRASH" ] || bad="render: $CRASH"
    grep -q "wasn't generated" "$L/render.log" && notes="$notes; a generated sound is silent (generation off)"
  fi
  rm -f /tmp/raea-corpus/work/out.mp4

  # 2. render vs baked play (timelines, 3D)
  if [ -z "$bad" ] && [ "$KIND" != game ]; then
    frames=$(python3 -c "n=$n; print(' '.join(str(f) for f in sorted({0, n // 3, (2 * n) // 3, max(0, n - 2)})))")
    snaps=""
    for f in $frames; do snaps="$snaps --snapshot $(python3 -c "print($f / $fps)") $W/play-$f.png"; done
    secs=$(python3 -c "print(round(($n - 1) / $fps + 1.5, 2))")
    # shellcheck disable=SC2086
    d3=""
    [ "$KIND" = 3d ] && d3=--3d
    # shellcheck disable=SC2086
    run "$L/play.log" 900 "$BIN" play "$edit" --baked --offscreen $d3 --seconds "$secs" --json "$L/play.json" $snaps
    if [ "$RC" != 0 ]; then
      bad="baked play failed (exit $RC): ${CRASH:-$(why "$L/play.log")}"
    else
      [ -z "$CRASH" ] || bad="baked play: $CRASH"
      cpu=$(python3 -c "import json; d=json.load(open('$L/play.json')); c=d.get('cpu_paint', {}); w={k: v for k, v in c.get('why', {}).items() if v}; print(c.get('frames', 0), c.get('stretches', 0), w or '')" 2>/dev/null)
      set -- $cpu
      [ "${1:-0}" = 0 ] && [ "${2:-0}" = 0 ] || notes="$notes; cpu_paint frames $1 stretches $2 ${*:3}"
      worst=1
      line=""
      for f in $frames; do
        if [ ! -s "$W/play-$f.png" ]; then line="$line $f:no-snapshot"; worst=0; continue; fi
        # shellcheck disable=SC2086
        run "$L/frame-$f.log" 600 env RAEA_READBACK=rgba "$BIN" render "$edit" --frame "$f" -o "$W/render-$f.png" $CONF
        if [ "$RC" != 0 ] || [ ! -s "$W/render-$f.png" ]; then line="$line $f:render-failed"; worst=0; bad="frame $f render failed: ${CRASH:-$(why "$L/frame-$f.log")}"; continue; fi
        s=$(ssim "$W/render-$f.png" "$W/play-$f.png")
        [ -n "$s" ] || s="0(sizes $(dims "$W/render-$f.png") vs $(dims "$W/play-$f.png"))"
        line="$line $f:$s"
        worst=$(python3 -c "print(min($worst, ${s%%(*}))")
        if python3 -c "import sys; sys.exit(0 if ${s%%(*} < 0.999 else 1)"; then
          cp "$W/render-$f.png" "$L/render-$f.png"
          cp "$W/play-$f.png" "$L/play-$f.png"
        fi
      done
      notes="$notes; render vs baked play$line"
      [ -n "$bad" ] || python3 -c "import sys; sys.exit(0 if $worst >= 0.999 else 1)" || bad="render vs baked play: worst SSIM $worst"
    fi
  fi

  # 3. games: a live play with input, then the same frames rendered from its log
  if [ -z "$bad" ] && [ "$KIND" = game ]; then
    run "$L/frame0.log" 600 env RAEA_READBACK=rgba "$BIN" render "$edit" --frame 0 -o "$W/size.png"
    gsize=$(dims "$W/size.png")
    secs="0.5 1 1.5 2 2.5"
    snaps=""
    for s in $secs; do snaps="$snaps --snapshot $s $W/play-$s.png"; done
    # shellcheck disable=SC2086
    run "$L/play.log" 300 env RAEA_INPUT_KEYS=",ArrowUp,ArrowDown,ArrowRight,ArrowRight+ArrowUp,ArrowLeft,Space" "$BIN" play "$edit" --input --offscreen --size "${gsize:-1080x1920}" \
      --seconds 3 --json "$L/play.json" --input-log "$L/input.json" $snaps
    if [ "$RC" != 0 ]; then
      bad="live play failed (exit $RC): ${CRASH:-$(why "$L/play.log")}"
    else
      [ -z "$CRASH" ] || bad="live play: $CRASH"
      cpu=$(python3 -c "import json; d=json.load(open('$L/play.json')); c=d.get('cpu_paint', {}); w={k: v for k, v in c.get('why', {}).items() if v}; print(c.get('frames', 0), c.get('stretches', 0), w or '')" 2>/dev/null)
      set -- $cpu
      [ "${1:-0}" = 0 ] && [ "${2:-0}" = 0 ] || notes="$notes; cpu_paint frames $1 stretches $2 ${*:3}"
      worst=1
      line=""
      for s in $secs; do
        f=$(python3 -c "print(round($s * $fps))")
        if [ ! -s "$W/play-$s.png" ]; then line="$line $f:no-snapshot"; worst=0; continue; fi
        run "$L/frame-$f.log" 600 env RAEA_READBACK=rgba "$BIN" render "$edit" --frame "$f" --input "$L/input.json" -o "$W/render-$s.png"
        if [ "$RC" != 0 ] || [ ! -s "$W/render-$s.png" ]; then line="$line $f:render-failed"; worst=0; bad="frame $f render failed: ${CRASH:-$(why "$L/frame-$f.log")}"; continue; fi
        v=$(ssim "$W/render-$s.png" "$W/play-$s.png")
        [ -n "$v" ] || v="0(sizes $(dims "$W/render-$s.png") vs $(dims "$W/play-$s.png"))"
        line="$line $f:$v"
        worst=$(python3 -c "print(min($worst, ${v%%(*}))")
        if python3 -c "import sys; sys.exit(0 if ${v%%(*} < 0.999 else 1)"; then
          cp "$W/render-$s.png" "$L/render-$f.png"
          cp "$W/play-$s.png" "$L/play-$f.png"
        fi
      done
      keyed=$(python3 -c "import json; d=json.load(open('$L/input.json')); print(sum(1 for v in d.values() if v['keys']), 'of', len(d))" 2>/dev/null)
      notes="$notes; replay from input (keys down in $keyed frames)$line"
      [ -n "$bad" ] || python3 -c "import sys; sys.exit(0 if $worst >= 0.999 else 1)" || bad="replay from input: worst SSIM $worst"
    fi
  fi

  rm -f /tmp/raea-corpus/work/*
  if [ -n "$bad" ]; then
    echo "FAIL  $KIND $name: $bad  [$notes]"
    fail=$((fail + 1))
    status=1
  else
    echo "PASS  $KIND $name: $notes"
    pass=$((pass + 1))
  fi
done < "$LIST"
echo "corpus: $pass passed, $fail failed, $skip skipped (logs in /tmp/raea-corpus/logs)"
exit $status
