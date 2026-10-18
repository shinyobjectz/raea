# build-cases.py — bundle each src/cases/*.tsx and write tests/parity/react/<case>/{index.html,times}.
import pathlib, subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = ROOT / "tests/parity/react"
GSAP = '<script src="https://cdn.jsdelivr.net/npm/gsap@3.14.2/dist/gsap.min.js"></script>'
SIZE = {"board": (1200, 720)}
TIMES = {"timeline": "0,0.3,0.8,1.6"}
# Static cases still give the HTML composition format a 2 s timeline to seek.
STATIC_TL = """<script>window.__timelines = window.__timelines || {};
window.__timelines["main"] = gsap.timeline({ paused: true }).set({}, {}, 2);</script>"""

HERE = pathlib.Path(__file__).resolve().parent
ECO = HERE.parent / "eco"
# (case, source, stylesheet): fixture/ cases are React 18; eco/ cases are React 19 + libraries.
CASES = [(p.stem, p, HERE / ("board.css" if p.stem == "board" else "cases.css")) for p in sorted((HERE / "src/cases").glob("*.tsx"))]
CASES += [("shadcn", ECO / "src/shadcn.tsx", ECO / "dist/tw.css"), ("emotion", ECO / "src/emotion.tsx", None),
          ("recharts", ECO / "src/recharts.tsx", None)]
SIZE["shadcn"] = (960, 600)

for name, src, css_path in CASES:
    out_js = HERE / f"dist/cases/{name}.js"
    subprocess.run(["bun", "build", str(src), "--format=iife", "--target=browser", "--minify",
                    "--define", 'process.env.NODE_ENV="production"', f"--outfile={out_js}"], check=True, capture_output=True, cwd=src.parent)
    js = out_js.read_text().replace("</script", "<\\/script")
    css = css_path.read_text() if css_path else ""
    w, h = SIZE.get(name, (960, 540))
    page = f"""<!doctype html>
<html><head><meta charset="utf-8">
{GSAP}
<style>
html,body{{margin:0;width:{w}px;height:{h}px;overflow:hidden;background:#fff}}
#root{{position:relative;width:{w}px;height:{h}px;overflow:hidden}}
{css}
</style></head><body>
<div id="root" data-composition-id="main" data-start="0" data-duration="2" data-width="{w}" data-height="{h}"><div id="app"></div></div>
{"" if name == "timeline" else STATIC_TL}
<script>{js}</script>
</body></html>
"""
    d = OUT / name
    d.mkdir(parents=True, exist_ok=True)
    (d / "index.html").write_text(page)
    (d / "times").write_text(TIMES.get(name, "0,1") + "\n")
    print(f"{name}: {len(page) // 1024} KB")
