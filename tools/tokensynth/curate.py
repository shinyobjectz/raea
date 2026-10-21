"""Pick the cleanest TokenSynth take of every root note. For each (timbre, note) an edit rendered with several seeds (its
assets/ folder, through Samplers), measure the pitch against the note asked for, the share of energy above 4 kHz (hiss, buzz),
the spectrum's flatness (noise), whether the sound has died away by the file's end (else a held note stops with a
click), and how late it starts. Writes {timbre: {midi: seed}} of the notes that pass (apps/void/prompts/craft/motion/music.md,
"Instruments from TokenSynth").

  uv run --with numpy --with soundfile python3 -I tools/tokensynth/curate.py EDIT_DIR [--out picks.json]
EDIT_DIR is the folder holding the edit and its assets/; the picks go to EDIT_DIR/instruments.gen.json by default.
"""
import json, math, sys
from pathlib import Path

import numpy as np
import soundfile as sf

args = [a for a in sys.argv[1:] if not a.startswith("--")]
HERE = Path(args[0] if args else ".").resolve()
OUT = Path(sys.argv[sys.argv.index("--out") + 1]) if "--out" in sys.argv else HERE / "instruments.gen.json"
takes = {}
for line in (HERE / "assets/made.jsonl").read_text().splitlines():
    m = json.loads(line)
    o = m["origin"]
    if o["model"] != "tokensynth":
        continue
    p = o["params"]
    if "midi" not in p:  # made without its note (an <Audio> of a tokensynth note drops it): not a candidate
        continue
    takes.setdefault((o["prompt"], p["midi"]), []).append((p["seed"], HERE / "assets" / f"{m['hash']}.{m['ext']}"))


def f0(x, sr):
    """the fundamental by harmonic product spectrum over the first 0.8 s after the onset"""
    seg = x[: int(0.8 * sr)] * np.hanning(min(len(x), int(0.8 * sr)))
    S = np.abs(np.fft.rfft(seg, 16 * len(seg)))
    F = np.fft.rfftfreq(16 * len(seg), 1 / sr)
    hps = S.copy()
    for h in (2, 3):
        d = S[::h]
        hps[: len(d)] *= d
    band = (F > 30) & (F < 1500)
    return F[band][np.argmax(hps[band])]


def measure(path, midi):
    x, sr = sf.read(path)
    x = x.mean(1) if x.ndim > 1 else x
    env = np.sqrt(np.convolve(x ** 2, np.ones(int(0.02 * sr)) / int(0.02 * sr), "same"))
    peak = env.max() + 1e-9
    onset = np.argmax(env > 0.1 * peak) / sr
    body = x[int(onset * sr):]
    want = 440 * 2 ** ((midi - 69) / 12)
    got = f0(body, sr)
    cents = 1200 * math.log2(got / want)
    spec = np.abs(np.fft.rfft(body * np.hanning(len(body))))
    fr = np.fft.rfftfreq(len(body), 1 / sr)
    hi = spec[fr > 4000].sum() / spec.sum()
    mid = spec[(fr > 50) & (fr < 8000)] + 1e-9
    flat = np.exp(np.mean(np.log(mid))) / np.mean(mid)
    tail = env[-int(0.25 * sr):].mean() / peak
    clip = float((np.abs(x) > 0.99).mean())
    score = abs(cents) / 25 + hi * 4 + flat * 3 + tail * 3 + max(0, onset - 0.05) * 10 + clip * 50
    ok = abs(cents) < 45 and hi < 0.35 and flat < 0.35 and tail < 0.35 and onset < 0.15
    return dict(cents=round(cents), hi=round(float(hi), 2), flat=round(float(flat), 2), tail=round(float(tail), 2), onset=round(float(onset), 3), score=round(float(score), 2), ok=bool(ok))


out = {}
for (prompt, midi), seeds in sorted(takes.items()):
    rated = sorted(((measure(p, midi), s) for s, p in seeds if p.exists()), key=lambda r: r[0]["score"])
    best, seed = rated[0]
    mark = "ok " if best["ok"] else "-- "
    print(f"{mark}{prompt[:38]:38} {midi:3} seed {seed:5}  {best}  ({len(rated)} takes)")
    if best["ok"]:
        out.setdefault(prompt, {})[str(midi)] = seed
OUT.write_text(json.dumps(out, indent=1) + "\n")
print({k[:30]: len(v) for k, v in out.items()})
