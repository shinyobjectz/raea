"""A rendered film's sound, checked by ear's proxies: what a listener hears as broken or too loud, found in the mix.

  uv run --with numpy python3 -I tools/sound/lint.py FILM.mp4 [--words W.json] [--json OUT.json] [--from S --to S]

W.json is the narration's words (raea's [{text, start_ms, end_ms}], or [{w, a, b}] in seconds), on the film's clock.
With it the lint knows where the voice is and checks everything else against it; without it only the cuts and peaks.

  dead stop     a sound that ends in one 10 ms step instead of decaying or fading: a bed or a take whose source ran
                out (a render doesn't loop a sound), or a Sequence that cuts it short with no fadeOut
  over voice    a second or more with no narration that plays louder than the narration does (momentary loudness
                against the voice's median): the film's own score, a foley bed or an effect that shouts over the film
  bed up        the bed under the narration (the quietest moments of a 6 s window of it) closer than 12 dB to the
                voice (its loud tenth): the voice is fighting it
  hot           true peak above the ceiling (-1 dBTP)

Each finding is a line "WARN  m:ss.s  kind  what"; the exit status is 1 when there's any. The thresholds are where the
noir doc's broken intro (D.O.A.'s main title at narration level, a projector that stopped dead) crossed them and its
fixed cut didn't; move them with the flags rather than editing.
"""
import argparse, json, subprocess, sys
import numpy as np

ap = argparse.ArgumentParser()
ap.add_argument("film")
ap.add_argument("--words")
ap.add_argument("--json")
ap.add_argument("--from", dest="t0", type=float, default=0.0)
ap.add_argument("--to", dest="t1", type=float, default=None)
ap.add_argument("--over", type=float, default=2.0, help="LU over the voice's median that a voiceless second may reach")
ap.add_argument("--bed", type=float, default=12.0, help="dB the bed under the narration must sit under the voice")
ap.add_argument("--drop", type=float, default=24.0, help="dB a dead stop falls in one 10 ms step")
ap.add_argument("--ceiling", type=float, default=-1.0)
a = ap.parse_args()

SR, HOP = 24000, 240  # 10 ms steps
span = ["-ss", str(a.t0)] + (["-t", str(a.t1 - a.t0)] if a.t1 else [])
pcm = subprocess.run(["ffmpeg", "-v", "error", *span, "-i", a.film, "-vn", "-ac", "1", "-ar", str(SR), "-f", "f32le", "-"], capture_output=True, check=True).stdout
x = np.frombuffer(pcm, np.float32)
n = len(x) // HOP
frames = x[: n * HOP].reshape(n, HOP).astype(np.float64)
rms = 20 * np.log10(np.sqrt((frames**2).mean(1)) + 1e-9)
# the same 10 ms steps in three bands (low < 500 Hz, mid to 4 kHz, high to 12 kHz): a bed that stops under a voice
# shows in the band the voice doesn't fill (a projector's clatter in the highs) when the whole mix barely moves
spec = np.abs(np.fft.rfft(frames * np.hanning(HOP), axis=1)) ** 2
hz = np.fft.rfftfreq(HOP, 1 / SR)
bands = {name: 10 * np.log10(spec[:, (hz >= lo) & (hz < hi)].sum(1) / HOP + 1e-12) for name, lo, hi in [("low", 20, 500), ("mid", 500, 4000), ("high", 4000, 12000)]}
t = a.t0 + np.arange(n) * HOP / SR

# momentary loudness (EBU R128, 400 ms) every 100 ms, and the true peak, from ffmpeg's meter
meter = subprocess.run(["ffmpeg", "-nostats", "-hide_banner", *span, "-i", a.film, "-vn", "-af", "ebur128=peak=true:metadata=1,ametadata=print:key=lavfi.r128.M:file=-", "-f", "null", "-"], capture_output=True, text=True)
mt, mv = [], []
for line in meter.stdout.splitlines():
    if line.startswith("frame:"):
        mt.append(a.t0 + float(line.split("pts_time:")[1]))
    elif line.startswith("lavfi.r128.M="):
        mv.append(float(line.split("=")[1]))
mt, mv = np.array(mt[: len(mv)]), np.array(mv)
tpk = max((float(l.split("Peak:")[1].split()[0]) for l in meter.stderr.splitlines() if "Peak:" in l and "dBFS" in l), default=-99.0)

words = []
if a.words:
    for w in json.load(open(a.words)):
        words.append((w["start_ms"] / 1000, w["end_ms"] / 1000) if "start_ms" in w else (w["a"], w["b"]))
    words.sort()
speaking = np.zeros(n, bool)
for wa, wb in words:
    speaking[max(0, int((wa - 0.5 - a.t0) * 100)) : max(0, int((wb + 0.5 - a.t0) * 100))] = True  # aligners drift

found = []
say = lambda at, kind, what: found.append(dict(at=round(float(at), 2), kind=kind, what=what))
mmss = lambda s: f"{int(s // 60)}:{s % 60:04.1f}"

# dead stops: a stationary sound (a bed: projector, rain, room tone, a held chord) that goes in one step instead of
# fading. Stationary: over the second before, its level smoothed to 50 ms varies by under 3 dB (narration and music
# with phrases never hold that still). The stop: the 10 ms level down 12 dB or more in one step, and the 300 ms after
# it with a floor --drop dB under the bed. In the whole mix or one band (low < 500 Hz, mid to 4 kHz, high to 12 kHz),
# so a bed that stops under a voice shows where the voice doesn't cover it.
smooth = lambda lv: np.convolve(lv, np.ones(5) / 5, mode="same")
for name, lv in [("mix", rms)] + list(bands.items()):
    sm = smooth(lv)
    for i in range(100, n - 30):
        if lv[i] - lv[i - 1] >= -12:
            continue
        pre = sm[i - 100 : i - 2]
        bed = np.median(pre)
        if bed > (-50 if name == "mix" else -65) and pre.std() < 3 and bed - np.percentile(lv[i : i + 30], 10) >= a.drop:
            if not any(f["kind"] == "dead stop" and abs(f["at"] - t[i]) < 0.5 for f in found):
                say(t[i], "dead stop", f"{'the mix' if name == 'mix' else 'the ' + name + 's'}: a steady bed at {bed:.0f} dB stops in 10 ms: its source ran out, or a Sequence cut it with no fadeOut")

if words:
    voice = np.median([v for tt, v in zip(mt, mv) if speaking[min(n - 1, max(0, int((tt - 0.2 - a.t0) * 100)))] and v > -70])
    # voiceless seconds louder than the voice: momentary loudness of windows that hold no word, in runs of 1 s or more
    quiet = np.array([not speaking[max(0, int((tt - 0.4 - a.t0) * 100)) : max(1, int((tt - a.t0) * 100))].any() for tt in mt])
    run = []
    for k in range(len(mt) + 1):
        if k < len(mt) and quiet[k] and mv[k] > voice + a.over:
            run.append(k)
            continue
        if len(run) >= 10:
            peak = mv[run].max()
            say(mt[run[0]] - 0.4, "over voice", f"{(len(run)) / 10:.1f} s with no narration at up to {peak:.1f} LUFS, {peak - voice:.1f} LU over the voice's {voice:.1f}")
        run = []
    # the bed under the narration: in each 6 s window that holds words, the voice is the loud tenth of the 10 ms steps
    # and the bed the quietest 3% (the pauses between phrases come down to it). Word times from an aligner drift by
    # half a second or more, so the gaps themselves aren't trusted, only that narration is in the window.
    W = 600
    for i in range(0, n - W, W // 2):
        if speaking[i : i + W].mean() < 0.4:
            continue
        seg = rms[i : i + W]
        voice_l, bed_l = np.percentile(seg, 90), np.percentile(seg, 3)
        if voice_l - bed_l < a.bed:
            say(t[i], "bed up", f"the bed under the narration sits {voice_l - bed_l:.0f} dB under the voice ({bed_l:.0f} vs {voice_l:.0f} dBFS)")
    # one finding per stretch of a kind, not one per gap
    merged = []
    for f in found:
        if merged and merged[-1]["kind"] == f["kind"] == "bed up" and f["at"] - merged[-1]["last"] < 3.5:
            merged[-1]["last"] = f["at"]
            merged[-1]["n"] += 1
            continue
        merged.append(dict(f, last=f["at"], n=1))
    found = [dict(at=f["at"], kind=f["kind"], what=f["what"] if f["n"] == 1 else f"{f['what']} (to {mmss(f['last'] + 6)})") for f in merged]

if tpk > a.ceiling:
    say(a.t0, "hot", f"true peak {tpk:.1f} dBTP, over the {a.ceiling:.1f} ceiling")

found.sort(key=lambda f: f["at"])
for f in found:
    print(f"WARN  {mmss(f['at'])}  {f['kind']:<10}  {f['what']}")
if words:
    print(f"voice median {voice:.1f} LUFS (momentary); true peak {tpk:.1f} dBTP; {len(found)} finding(s)")
else:
    print(f"true peak {tpk:.1f} dBTP; {len(found)} finding(s) (no --words: cuts and peaks only)")
if a.json:
    json.dump(found, open(a.json, "w"), indent=1)
sys.exit(1 if found else 0)
