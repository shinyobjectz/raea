"""The narration as ONE Fish take: every line of script.json in a single request on the drama model, with one fixed
narrator (reference_id), so the voice can't change between lines (Fish holds a voice only across the chunks of one
request). Parakeet (parakeet-mlx) times the take's words; the script's words are aligned onto them (difflib); each line is cut out
to media/voice/<id>.mp3 and its words go to voice.json. timeline.json keeps its gaps between lines, with each line's
length from the new take; `hit` is "Casca" and the score starts 88 s before it (its climax).

  doppler run --project client --config dev -- python3 tools/voice.py [--force]

FISH_API_KEY comes from the environment only. The take is cached by (voice, model, settings, text).
"""
import difflib, hashlib, json, os, re, subprocess, sys, tempfile, urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent
VOICE_DIR = HERE / "media/voice"
VOICE = "80e34d5e0b2b4577a486f3a77e357261"  # Fish "New History": deep, slightly raspy, grave
MODEL = "drama-3-preview"
SETTINGS = dict(temperature=0.3, top_p=0.7, chunk_length=300, condition_on_previous_chunks=True, normalize=True, prosody=dict(speed=0.9))
SCORE_CLIMAX = 88.0  # seconds into score-a.mp3
WHISPER_MODEL = Path.home() / ".cache/ggml-base.en.bin"

script = json.loads((HERE / "script.json").read_text())
lines = script["lines"]
text = "\n\n".join(l["text"] for l in lines)
sig = hashlib.sha256(json.dumps([VOICE, MODEL, SETTINGS, text], sort_keys=True).encode()).hexdigest()[:16]
take = VOICE_DIR / "take.mp3"
meta = VOICE_DIR / "take.json"

if "--force" in sys.argv or not take.exists() or json.loads(meta.read_text()).get("sig") != sig if meta.exists() else True:
    key = os.environ.get("FISH_API_KEY") or sys.exit("FISH_API_KEY is not set (doppler run --project client --config dev -- ...)")
    print(f"recording one take: {len(text)} chars, {MODEL}, voice {VOICE}")
    body = dict(text=text, reference_id=VOICE, format="mp3", mp3_bitrate=192, **SETTINGS)
    req = urllib.request.Request("https://api.fish.audio/v1/tts", data=json.dumps(body).encode(), method="POST",
                                 headers={"authorization": f"Bearer {key}", "content-type": "application/json", "model": MODEL})
    with urllib.request.urlopen(req, timeout=600) as r:
        audio = r.read()
    VOICE_DIR.mkdir(parents=True, exist_ok=True)
    take.write_bytes(audio)
    whisper = None
else:
    print("take is current")

# ------------------------------------------------------------------ word times
tmp = Path(tempfile.mkdtemp())
subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", take, "-ar", "16000", "-ac", "1", tmp / "take.wav"], check=True)
# Parakeet (TDT, parakeet-mlx), not Whisper: Whisper's word times run late, 0.2-0.5 s a line and up to 1.5 s, which
# put every caption and every cut on a word late (measured on rough cut 8 by sdk/motion/bench/legible/sync.py).
subprocess.run(["parakeet-mlx", tmp / "take.wav", "--output-format", "json", "--output-dir", tmp], check=True, capture_output=True)
norm = lambda w: re.sub(r"[^a-z0-9]", "", w.lower())
heard = []
for s in json.loads((tmp / "take.json").read_text())["sentences"]:
    for tok in s["tokens"]:  # sub-word tokens; a word starts with a space
        if tok["text"].startswith(" ") or not heard:
            heard.append(dict(w=tok["text"].strip(), a=tok["start"], b=tok["end"]))
        else:
            heard[-1]["w"] += tok["text"]
            heard[-1]["b"] = tok["end"]
heard = [h for h in heard if norm(h["w"])]

said = [(i, w) for i, l in enumerate(lines) for w in l["text"].split()]
m = difflib.SequenceMatcher(None, [norm(w) for _, w in said], [norm(h["w"]) for h in heard], autojunk=False)
times = [None] * len(said)
hix = [None] * len(said)
for blk in m.get_matching_blocks():
    for k in range(blk.size):
        h = heard[blk.b + k]
        times[blk.a + k] = (h["a"], h["b"])
        hix[blk.a + k] = blk.b + k
matched = sum(t is not None for t in times)
# words heard differently ("Some one", "23"): onto what was heard between the matched neighbours, which keeps them off
# the pauses (an even spread over a gap put "Someone" 0.6 s early, in the beat before it)
k = 0
while k < len(said):
    if times[k] is not None:
        k += 1
        continue
    n = next((j for j in range(k, len(said)) if times[j] is not None), len(said))
    hp = hix[k - 1] if k else -1
    hn = hix[n] if n < len(said) else len(heard)
    H = heard[hp + 1:hn]
    if H:
        step = (H[-1]["b"] - H[0]["a"]) / (n - k)
        for i, j in enumerate(range(k, n)):
            times[j] = (H[i]["a"], H[i]["b"]) if len(H) == n - k else (H[0]["a"] + step * i, H[0]["a"] + step * (i + 1))
    k = n
# anything left: spread between the matched neighbours
for k, t in enumerate(times):
    if t is None:
        p = next((j for j in range(k - 1, -1, -1) if times[j]), None)
        n = next((j for j in range(k + 1, len(times)) if times[j]), None)
        a = times[p][1] if p is not None else 0.0
        b = times[n][0] if n is not None else a + 0.4
        gap_n = (n if n is not None else len(times)) - (p if p is not None else -1)
        step = (b - a) / gap_n
        i = k - (p if p is not None else -1)
        times[k] = (a + step * (i - 1), a + step * i)
print(f"aligned {matched}/{len(said)} script words to the take, {sum(t is not None for t in times) - matched} more onto what was heard between")

# ------------------------------------------------------------------ cut the lines
# Each line is cut inside the pause before and after it (the take's silences, ffmpeg silencedetect), never at Whisper's
# times: Whisper runs a line's last word on to the next line's first, so a cut there clips both.
total = float(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", take], capture_output=True, text=True).stdout)
det = subprocess.run(["ffmpeg", "-hide_banner", "-i", take, "-af", "silencedetect=noise=-40dB:d=0.2", "-f", "null", "-"], capture_output=True, text=True).stderr
starts = [float(x) for x in re.findall(r"silence_start: ([0-9.]+)", det)]
ends = [float(x) for x in re.findall(r"silence_end: ([0-9.]+)", det)]
silences = list(zip(starts, ends + [total] * (len(starts) - len(ends))))
spans = []
for i in range(len(lines)):
    ks = [k for k, (li, _) in enumerate(said) if li == i]
    spans.append((times[ks[0]][0], times[ks[-1]][1], ks))
# the pause between line i and i+1: of the silences near where Whisper puts line i+1's first word, the first (best
# fitting first) where a listen just before it ends on line i's last word and just after it starts on line i+1's first.
# Whisper runs late by up to a second, and a narrator pauses inside lines too (a comma, a beat before a quote), so
# neither the nearest nor the longest silence is reliably the break.
def heard(a, b):
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", f"{max(0, a):.3f}", "-to", f"{b:.3f}", "-i", take, "-ar", "16000", "-ac", "1", tmp / "w.wav"], check=True)
    subprocess.run(["whisper-cli", "-m", WHISPER_MODEL, "-f", tmp / "w.wav", "-otxt", "-of", tmp / "w", "-np"], capture_output=True)
    return [norm(w) for w in (tmp / "w.txt").read_text().split()] or [""]
NUM = {"twenty": "2", "two": "2", "one": "1", "fifteen": "1", "sixty": "6"}
same = lambda h, w: h == norm(w) or (h[:1].isdigit() and NUM.get(norm(w), "") == h[:1])
pauses = []
for i in range(len(lines) - 1):
    e = spans[i + 1][0]
    near = sorted((s for s in silences if s[1] > e - 3 and s[0] < e + 1.5), key=lambda s: -((s[1] - s[0]) - 0.6 * abs(s[1] - e)))
    if not near:
        sys.exit(f"no pause found between {lines[i]['id']} and {lines[i + 1]['id']} near {e:.2f} s")
    last, first = lines[i]["text"].split()[-1], lines[i + 1]["text"].split()[0]
    pick = next((s for s in near if same(heard(s[0] - 1.8, s[0] + 0.05)[-1], last) and same(heard(s[1] - 0.05, s[1] + 1.8)[0], first)), None)
    if pick is None:
        print(f"warning: no pause between {lines[i]['id']} and {lines[i + 1]['id']} hears right; taking the best fit")
        pick = near[0]
    pauses.append(pick)
voice = []
for i, (a, b, ks) in enumerate(spans):
    speech_a = pauses[i - 1][1] if i else max(0.0, a - 0.1)
    speech_b = pauses[i][0] if i < len(pauses) else min(total, b + 0.1)
    lo = max(0.0, speech_a - 0.12) if not i else max(0.0, speech_a - 0.12)
    lo = max(lo, (pauses[i - 1][0] + pauses[i - 1][1]) / 2) if i else lo
    hi = min(speech_b + 0.35, (pauses[i][0] + pauses[i][1]) / 2) if i < len(pauses) else min(total, speech_b + 0.6)
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", f"{lo:.3f}", "-to", f"{hi:.3f}", "-i", take,
                    "-af", "afade=t=in:d=0.02,areverse,afade=t=in:d=0.1,areverse", "-c:a", "libmp3lame", "-b:a", "192k",
                    VOICE_DIR / f"{lines[i]['id']}.mp3"], check=True)
    # word times inside the clip, kept within the line's speech
    ws = []
    for k in ks:
        wa = min(max(times[k][0], speech_a), speech_b) - lo
        wb = min(max(times[k][1], speech_a), speech_b) - lo
        ws.append(dict(w=said[k][1], a=round(wa, 3), b=round(max(wb, wa + 0.05), 3)))
    voice.append(dict(id=lines[i]["id"], seconds=round(hi - lo, 3), words=ws))
    print(f"{lines[i]['id']:9} clip {lo:7.2f}-{hi:7.2f}  speech {speech_a:7.2f}-{speech_b:7.2f}")
(HERE / "voice.json").write_text(json.dumps(voice, indent=1) + "\n")

# ------------------------------------------------------------------ the timeline: same gaps, new lengths
tl = json.loads((HERE / "timeline.json").read_text())
old = tl["lines"]
gaps = [old[i + 1]["at"] - old[i]["at"] - old[i]["seconds"] for i in range(len(old) - 1)]
tail = tl["end"] - old[-1]["at"] - old[-1]["seconds"]
at = tl["intro"]
new = []
for i, v in enumerate(voice):
    new.append(dict(id=v["id"], at=round(at, 3), seconds=v["seconds"]))
    at += v["seconds"] + (gaps[i] if i < len(gaps) else 0)
tl["lines"] = new
tl["end"] = round(new[-1]["at"] + new[-1]["seconds"] + tail, 3)
strike = next(v for v in voice if v["id"] == "strike")
tl["hit"] = round(next(l["at"] for l in new if l["id"] == "strike") + next(w["a"] for w in strike["words"] if w["w"].startswith("Casca")), 3)
tl["scoreAt"] = round(tl["hit"] - SCORE_CLIMAX, 3)
(HERE / "timeline.json").write_text(json.dumps(tl, indent=1) + "\n")
# every word at its film time, for sdk/motion/bench/legible/sync.py to hold a render's captions to what's heard
film = [dict(line=v["id"], w=w["w"], t=round(l["at"] + w["a"], 3)) for v, l in zip(voice, new) for w in v["words"]]
(HERE / "words.film.json").write_text(json.dumps(film) + "\n")
meta.write_text(json.dumps(dict(sig=sig, voice=VOICE, model=MODEL, settings=SETTINGS, seconds=round(total, 3)), indent=1) + "\n")
print(f"take {total:.1f} s; film {tl['end']:.1f} s; Casca at {tl['hit']:.2f} s")
