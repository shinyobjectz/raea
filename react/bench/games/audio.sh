#!/bin/bash
# The audio gate (no-V8 goal, gate 6): loudness, true peak and clicks, live against rendered.
#   - pong played live with no input (the game its master gain was taken from) and rendered: the live sound must sit
#     within 0.5 LU of the render's loudness and under the -1.5 dBTP ceiling
#   - the tones fixture (sines started, cut, overlapped, ramped, stopped) played live: no clicks (clicks.py)
#   react/bench/games/audio.sh
cd "$(dirname "$0")/../.."
OUT=${TMPDIR:-/tmp}/raea-audio-gate
mkdir -p "$OUT"
T=bench/games/tones
[ -f $T/tone-440.wav ] || ffmpeg -v error -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=4" -ac 2 -af volume=0.5 $T/tone-440.wav
[ -f $T/tone-660.wav ] || ffmpeg -v error -f lavfi -i "sine=frequency=660:sample_rate=48000:duration=4" -ac 2 -af volume=0.5 $T/tone-660.wav
status=0
RAEA_AUDIO_MUTE=1 RAEA_AUDIO_TAP=$OUT/pong-live.wav target/release/raea-motion play bench/games/pong/edit.tsx --offscreen --size 1920x1080 --seconds 39 > $OUT/pong-play.log 2>&1 || { echo "pong: the play failed"; status=1; }
target/release/raea-motion render bench/games/pong/edit.tsx -o $OUT/pong-render.mp4 > $OUT/pong-render.log 2>&1 || { echo "pong: the render failed"; status=1; }
RAEA_AUDIO_MUTE=1 RAEA_AUDIO_TAP=$OUT/tones-live.wav target/release/raea-motion play $T/edit.tsx --offscreen --seconds 10.5 > $OUT/tones-play.log 2>&1 || { echo "tones: the play failed"; status=1; }
target/release/raea-motion render $T/edit.tsx -o $OUT/tones-render.mp4 > $OUT/tones-render.log 2>&1 || { echo "tones: the render failed"; status=1; }
python3 - "$OUT" <<'PY' || status=1
import re, subprocess, sys
out = sys.argv[1]
def run(*files):
    r = subprocess.run([sys.executable, "bench/games/clicks.py", *files], capture_output=True, text=True, check=True).stdout.strip().splitlines()
    return [(float(re.search(r": (-?[\d.]+) LUFS", l).group(1)), float(re.search(r"true peak (-?[\d.]+)", l).group(1)), int(re.search(r"clicks (\d+)", l).group(1)), l) for l in r]
(pl, pp, _, _), (rl, rp, _, _) = run(f"{out}/pong-live.wav", f"{out}/pong-render.mp4")
loud = abs(pl - rl) <= 0.5 and pp <= -1.5
print(f"pong: {'PASS' if loud else 'FAIL'}  live {pl} LUFS, true peak {pp} dBTP; render {rl} LUFS, {rp} dBTP")
(tl, tp, tc, tline), (_, _, rc, _) = run(f"{out}/tones-live.wav", f"{out}/tones-render.mp4")
clean = tc == 0
print(f"tones: {'PASS' if clean else 'FAIL'}  live clicks {tc} (render {rc}); {tline.split(': ', 1)[1]}")
sys.exit(0 if loud and clean else 1)
PY
exit $status
