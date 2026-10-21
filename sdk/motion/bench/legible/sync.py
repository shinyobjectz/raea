#!/usr/bin/env python3
"""Caption sync lint, measured on what's heard: transcribes a render's mix with Parakeet (parakeet-mlx, TDT 0.6b v3,
on-device; word times to about 0.08 s, music and foley under the voice are fine) and holds the words a film's captions
and cuts are timed to against when each is actually said. Faults:

  drift     a line's words are off by more than 0.15 s on the median, or one by more than 0.5 s (captions late or early)
  unheard   two or more of the script's words in a row aren't in the mix, and nothing is heard where they should be
            (a clip cut short, a line ducked out)

WORDS is a JSON list of {"w": word, "t": film seconds, "line": optional id}, in script order (apps/void/brand/rome writes
words.film.json from tools/voice.py). Prints one line per fault and a summary per line, and exits 1 on any fault.

  python3 sdk/motion/bench/legible/sync.py FILM.mp4 WORDS.json
  DRIFT=0.15 WORST=0.5 to change the limits; KEEP=DIR keeps the transcript there
"""
import difflib, json, os, re, statistics, subprocess, sys, tempfile
from pathlib import Path

video, words_file = sys.argv[1], sys.argv[2]
DRIFT, WORST = float(os.environ.get("DRIFT", 0.15)), float(os.environ.get("WORST", 0.5))
out = Path(os.environ.get("KEEP") or tempfile.mkdtemp(prefix="raea-sync."))
out.mkdir(parents=True, exist_ok=True)

subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", video, "-ar", "16000", "-ac", "1", out / "mix.wav"], check=True)
subprocess.run(["parakeet-mlx", out / "mix.wav", "--output-format", "json", "--output-dir", out], check=True, capture_output=True)
norm = lambda w: re.sub(r"[^a-z0-9]", "", w.lower())
heard = []
for s in json.loads((out / "mix.json").read_text())["sentences"]:
    for tok in s["tokens"]:  # sub-word tokens; a word starts with a space
        if tok["text"].startswith(" ") or not heard:
            heard.append([tok["text"].strip(), tok["start"]])
        else:
            heard[-1][0] += tok["text"]
heard = [h for h in heard if norm(h[0])]

said = json.loads(Path(words_file).read_text())
m = difflib.SequenceMatcher(None, [norm(w["w"]) for w in said], [norm(h[0]) for h in heard], autojunk=False)
at = [None] * len(said)
for b in m.get_matching_blocks():
    for k in range(b.size):
        at[b.a + k] = heard[b.b + k][1]

def heard_again(a, b, ws):
    """Parakeet over a whole film now and then drops a sentence it hears fine on its own: listen to just [a, b] again"""
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", f"{max(0, a):.2f}", "-to", f"{b:.2f}", "-i", out / "mix.wav", out / "again.wav"], check=True)
    subprocess.run(["parakeet-mlx", out / "again.wav", "--output-format", "txt", "--output-dir", out], check=True, capture_output=True)
    got = {norm(w) for w in (out / "again.txt").read_text().split()}
    return sum(norm(w) in got for w in ws) >= len(ws) / 2


mmss = lambda t: f"{int(t // 60)}:{t % 60:04.1f}"
faults = 0
lines = list(dict.fromkeys(w.get("line", "") for w in said))
for line in lines:
    ks = [k for k, w in enumerate(said) if w.get("line", "") == line]
    off = [(said[k]["t"] - at[k], k) for k in ks if at[k] is not None]
    if off:
        med = statistics.median(o for o, _ in off)
        worst, wk = max(off, key=lambda x: abs(x[0]))
        bad = abs(med) > DRIFT or abs(worst) > WORST
        faults += bad
        late = lambda x: "late" if x > 0 else "early"
        print(f"{'FAULT ' if bad else ''}{line or 'words'}: {mmss(said[ks[0]]['t'])} median {abs(med):.2f} s {late(med)}, "
              f"worst \"{said[wk]['w']}\" {abs(worst):.2f} s {late(worst)} at {mmss(said[wk]['t'])} ({len(off)}/{len(ks)} heard)")
    # runs of the script not heard in the mix
    run = []
    for k in ks + [None]:
        if k is not None and at[k] is None:
            run.append(k)
            continue
        # ...and nothing else heard then either: the recogniser spells some words its own way ("23", "Kimber")
        if len(run) >= 2 and not any(said[run[0]]["t"] - 0.5 <= h[1] <= said[run[-1]]["t"] + 0.8 for h in heard) \
                and not heard_again(said[run[0]]["t"] - 1, said[run[-1]]["t"] + 2, [said[j]["w"] for j in run]):
            faults += 1
            print(f"FAULT {line or 'words'}: unheard at {mmss(said[run[0]]['t'])}: \"{' '.join(said[j]['w'] for j in run)}\"")
        run = []
print(f"{faults} fault(s); {sum(a is not None for a in at)}/{len(said)} words heard")
sys.exit(1 if faults else 0)
