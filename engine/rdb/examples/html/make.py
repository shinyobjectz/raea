"""The scoreboard's HTML composition (the HTML composition format format): the UGC example's take as six sentence clips, word captions
spread over each sentence, and a GSAP-animated title over the first two seconds (so the page repaints every frame
there, and only per word after). python3 make.py > ugc.w.html"""
import json, html

segs = json.load(open("../ugc/media/take-segments.json"))[:6]
t0 = segs[0]["start_ms"]
clips, words, at = [], [], 0.0
for i, s in enumerate(segs):
    dur = round((s["end_ms"] - s["start_ms"]) / 1000, 4)
    clips.append((f"v{i+1}", at, dur, s["start_ms"] / 1000))
    ws = s["text"].split()
    for k, w in enumerate(ws):
        words.append((w, round(at + dur * k / len(ws), 4), round(dur / len(ws), 4)))
    at = round(at + dur, 4)
total = at
out = []
for vid, start, dur, src_in in clips:
    out.append(f'<video id="{vid}" class="take" src="../ugc/media/take-h264.mp4" data-start="{start}" data-duration="{dur}" data-media-start="{src_in}" muted playsinline></video>')
    out.append(f'<audio id="a{vid}" src="../ugc/media/take-voice.m4a" data-start="{start}" data-duration="{dur}" data-media-start="{src_in}"></audio>')
for i, (w, start, dur) in enumerate(words):
    out.append(f'<div id="w{i}" class="cw" data-start="{start}" data-duration="{dur}">{html.escape(w)}</div>')
print(f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<script src="https://cdn.jsdelivr.net/npm/gsap@3.14.2/dist/gsap.min.js"></script>
<style>
body {{ margin: 0; background: #000; }}
#root {{ position: relative; width: 100%; height: 100%; overflow: hidden; background: #000; }}
.take {{ position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }}
.cw {{ position: absolute; left: 60px; width: 960px; top: 1340px; text-align: center; font: 800 84px/1.1 "Avenir Next", sans-serif;
       color: #fff; text-shadow: 0 0 3px #000, 0 4px 16px rgba(0,0,0,.8); }}
#title {{ position: absolute; left: 80px; width: 920px; top: 360px; text-align: center; font: 900 110px/1.05 "Avenir Next", sans-serif;
          color: #ffd23f; text-shadow: 0 6px 24px rgba(0,0,0,.85); opacity: 0; }}
</style></head><body>
<div id="root" data-composition-id="main" data-width="1080" data-height="1920" data-duration="{total}">
{chr(10).join(out)}
<div id="title" data-start="0" data-duration="2.4">Stop scrolling</div>
</div>
<script>
const tl = gsap.timeline({{ paused: true }});
tl.fromTo("#title", {{ opacity: 0, scale: 0.8 }}, {{ opacity: 1, scale: 1, duration: 0.6, ease: "back.out(2)" }}, 0)
  .to("#title", {{ y: -20, duration: 1.2, ease: "sine.inOut" }}, 0.6)
  .to("#title", {{ opacity: 0, duration: 0.4 }}, 2.0);
window.__timelines = {{ main: tl }};
</script>
</body></html>""")
