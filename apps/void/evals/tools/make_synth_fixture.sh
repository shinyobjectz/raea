#!/bin/zsh
# The synthetic fixture for the new-stack ad scenario (evals/scenarios-newstack): no person, no creator's footage, no
# paid model. A voiceover for a fictional cold brew ("Plumquiver", an invented name with no site or company found in a web search on 2026-10-18; its address uses .example, the TLD reserved never to resolve) spoken by macOS `say` (local), sentence by sentence
# so the sentence timings are exact; each sentence's words are timed by their share of its letters (good to a word or
# so: cut in the pauses between sentences). Three 5 s b-roll shots drawn by raea-motion from tools/synth/broll.tsx.
# Writes evals/fixtures/synth/media/{vo.m4a, vo.words.json, can-turn.mp4, can-ice.mp4, can-row.mp4}.
set -eu
here=${0:A:h}
repo=${here:h:h:h}
out=$repo/apps/void/evals/fixtures/synth/media
tmp=$(mktemp -d)
trap 'rm -rf $tmp' EXIT
mkdir -p $out
lines=(
  "Your afternoon slump has a new enemy."
  "Plumquiver cold brew is steeped for twenty hours, so it's smooth, never bitter."
  "Twelve grams of protein, zero added sugar."
  "Crack one open at three p m, and actually finish your to do list."
  "Plumquiver. Find it in the cold aisle, or get your first case half off at plumquiver dot example."
)
i=0
for l in $lines; do
  i=$((i + 1))
  say -v Samantha -r 185 -o $tmp/s$i.aiff "$l"
done
# each sentence, then 0.45 s of silence; words timed inside their sentence by letters
python3 -I - "$tmp" "${(@)lines}" <<'PY'
import json, subprocess, sys
tmp, lines = sys.argv[1], sys.argv[2:]
words, t, parts = [], 0.25, []
gap = 0.45
for i, line in enumerate(lines, 1):
    d = float(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", f"{tmp}/s{i}.aiff"], capture_output=True, text=True).stdout)
    ws = line.split()
    # say leaves ~0.1 s of silence at each end of a sentence
    a, b = t + 0.08, t + d - 0.1
    total = sum(len(w) + 1 for w in ws)
    x = a
    for w in ws:
        span = (b - a) * (len(w) + 1) / total
        words.append({"text": w, "start_ms": round(x * 1000), "end_ms": round((x + span * 0.92) * 1000)})
        x += span
    parts.append((i, t))
    t += d + gap
json.dump(words, open(f"{tmp}/vo.words.json", "w"), indent=1)
# the concat: silence lead, each sentence, gap
with open(f"{tmp}/list.txt", "w") as f:
    for i, _ in parts:
        f.write(f"file '{tmp}/s{i}.aiff'\nfile '{tmp}/gap.aiff'\n")
print(round(t, 2))
PY
ffmpeg -v error -y -f lavfi -i anullsrc=r=22050:cl=mono -t 0.45 $tmp/gap.aiff
ffmpeg -v error -y -f lavfi -i anullsrc=r=22050:cl=mono -t 0.25 $tmp/lead.aiff
{ echo "file '$tmp/lead.aiff'"; cat $tmp/list.txt; } > $tmp/all.txt
ffmpeg -v error -y -f concat -safe 0 -i $tmp/all.txt -ar 48000 -ac 1 -c:a aac -b:a 128k $out/vo.m4a
cp $tmp/vo.words.json $out/vo.words.json
for c in turn ice row; do
  nice -n 15 taskpolicy -c utility $repo/sdk/react/target/release/raea-motion render $here/synth/broll.tsx --composition $c -o $tmp/$c.mp4 2>&1 | grep -E "APPLIED|REFUSED|rror" | grep -v bevy_log || true
  ffmpeg -v error -y -i $tmp/$c.mp4 -an -c:v libx264 -crf 26 -preset slow -pix_fmt yuv420p $out/can-$c.mp4
done
ls -la $out
