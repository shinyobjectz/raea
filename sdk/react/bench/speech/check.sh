#!/bin/bash
# Live speech (@raea/motion/speech) on the narrator test game (game/): lines made by Pocket TTS while the game plays,
# and the same samples in every runner. Checks, each from fresh generation (a private speech cache, off where it says):
#   start    the first line, asked for at frame 30, is silent before its start frame (speechStart: 30 + 0.15 s = 39)
#            and heard within 100 ms of it
#   cut      the second line, cut at CUT_AT, stops at the next word and the reaction follows: Parakeet hears "tilts"
#            and "careful" around the cut and not the rest of the line ("mountain"); skipped without parakeet-mlx
#   live     the play's tap (live inference, cache off) against a fresh render's bounce: max difference <= 1e-6
#   logged   a play pressed by a random hand (Space cuts lines at random frames), its input logged, rendered from the
#            log (cache off): the render's bounce against the play's tap, <= 1e-6
# Plus each play's speech report (first audio, real-time factor, waits). Plays measure timing: run it holding the
# measurement lock. Build first (cargo build --release; RAEA_MOTION names another binary).
#   sdk/react/bench/speech/check.sh
cd "$(dirname "$0")/../.."
BIN=${RAEA_MOTION:-target/release/raea-motion}
OUT=${TMPDIR:-/tmp}/raea-speech-check
rm -rf "$OUT"; mkdir -p "$OUT"
# the outputs are a few WAVs (about 15 MB each) and two small MP4s: gone when the check ends unless RAEA_SPEECH_KEEP=1
[ "${RAEA_SPEECH_KEEP:-0}" = 1 ] || trap 'rm -rf "$OUT"' EXIT
export RAEA_SPEECH_CACHE=$OUT/cache
E=bench/speech/game/edit.tsx
status=0
# a fresh render (cache off: Pocket makes every line here)
RAEA_SPEECH_CACHE=off RAEA_AUDIO_BOUNCE=$OUT/render.wav $BIN render $E -o $OUT/render.mp4 > $OUT/render.log 2>&1 || { echo "render failed (see $OUT/render.log)"; exit 1; }
# the live play of the same game, nobody pressing anything
RAEA_SPEECH_CACHE=off RAEA_PLAY_WAIT_MASTER=60 RAEA_AUDIO_MUTE=1 RAEA_AUDIO_TAP=$OUT/live.wav $BIN play $E --offscreen --seconds end --json $OUT/live-play.json > $OUT/live.log 2>&1 || { echo "play failed (see $OUT/live.log)"; exit 1; }
$BIN ci bounce-vs-live $OUT/render.wav $OUT/live.wav --json $OUT/live-same.json > /dev/null || status=1
# a play with Space pressed at random (50-250 ms apart), its input logged, then rendered from the log
RAEA_SPEECH_CACHE=off RAEA_INPUT_KEYS=' ,' RAEA_PLAY_WAIT_MASTER=60 RAEA_AUDIO_MUTE=1 RAEA_AUDIO_TAP=$OUT/pressed.wav $BIN play $E --offscreen --seconds 20 --input --input-log $OUT/input.json --json $OUT/pressed-play.json > $OUT/pressed.log 2>&1 || { echo "pressed play failed (see $OUT/pressed.log)"; exit 1; }
RAEA_SPEECH_CACHE=off RAEA_AUDIO_BOUNCE=$OUT/logged.wav $BIN render $E --input $OUT/input.json --frames 1200 -o $OUT/logged.mp4 > $OUT/logged.log 2>&1 || { echo "render of the logged play failed (see $OUT/logged.log)"; exit 1; }
$BIN ci bounce-vs-live $OUT/logged.wav $OUT/pressed.wav --json $OUT/logged-same.json > /dev/null || status=1
# the cut, heard: the stretch around CUT_AT (frame 570, 9.5 s) through the reaction
if command -v parakeet-mlx > /dev/null; then
  ffmpeg -v error -y -i $OUT/render.wav -ss 5 -to 13 -ac 1 -ar 16000 $OUT/cut.wav && parakeet-mlx $OUT/cut.wav --output-format txt --output-dir $OUT > /dev/null 2>&1
fi
python3 - "$OUT" <<'PY' || status=1
import array, json, os, struct, sys
out = sys.argv[1]
def wav(p):
    b = open(p, "rb").read(); rate = struct.unpack("<I", b[24:28])[0]; i = b.index(b"data") + 8
    a = array.array("f"); a.frombytes(b[i:]); return rate, a
ok = True
# start: silent before frame 39, heard within 100 ms of it
rate, a = wav(f"{out}/render.wav")
s0 = int(39 / 60 * rate)
before = max((abs(v) for v in a[: s0 * 2]), default=0.0)
onset = next((i // 2 for i, v in enumerate(a) if abs(v) > 1e-3), None)
late = None if onset is None else (onset - s0) / rate * 1000
good = before == 0.0 and late is not None and 0 <= late <= 100
print(f"start: {'PASS' if good else 'FAIL'}  silent before frame 39 (max {before:.2g}), heard {late if late is None else round(late, 1)} ms after it")
ok &= good
# cut
txt = f"{out}/cut.txt"
if os.path.exists(txt):
    heard = open(txt).read().strip()
    w = heard.lower()
    good = "tilts" in w and "careful" in w and "mountain" not in w
    print(f"cut: {'PASS' if good else 'FAIL'}  heard \"{heard}\"")
    ok &= good
else:
    print("cut: SKIP  (no parakeet-mlx)")
# render equals play, both plays
for name, label in (("live", "tap vs fresh render"), ("logged", "pressed tap vs render of its log")):
    j = json.load(open(f"{out}/{name}-same.json"))
    good = j["max_abs_diff"] <= 1e-6 and j["compared_s"] > 10
    late = (j.get("live") or {}).get("late_events")
    print(f"{name}: {'PASS' if good else 'FAIL'}  {label}: max diff {j['max_abs_diff']:.3g} over {j['compared_s']:.1f} s, best lag {j['best_lag_samples']}, late events {late}")
    ok &= good
    # a frame held for speech past the mixer's head (two blocks, 5 ms and half a frame) has its audio scheduled after its sample: the play
    # isn't the render there. The cure is lead (speechStart), not a longer head
    if not good:
        play = "live" if name == "live" else "pressed"
        w = json.load(open(f"{out}/{play}-play.json")).get("speech", {}).get("waits", {})
        if w.get("count"):
            print(f"    frames waited on speech {w['count']} times (longest {w['longest_ms']:.0f} ms): a line started with too little lead?")
# the speech reports
for name in ("live", "pressed"):
    sp = json.load(open(f"{out}/{name}-play.json")).get("speech", {})
    lines = sp.get("lines", [])
    fa = [l["first_audio_ms"] for l in lines if l.get("first_audio_ms") is not None]
    rtf = [l["rtf"] for l in lines if l.get("rtf") is not None]
    errs = [l["error"] for l in lines if l.get("error")]
    w = sp.get("waits", {})
    rng = lambda v, f: f"{min(v):{f}}-{max(v):{f}}" if v else "-"
    print(f"{name} speech: {len(lines)} lines, first audio {rng(fa, '.0f')} ms, rtf {rng(rtf, '.2f')}, waits {w.get('count')} ({w.get('total_ms', 0):.0f} ms)" + (f", errors {errs}" if errs else ""))
    ok &= not errs and len(lines) > 0
sys.exit(0 if ok else 1)
PY
exit $status
