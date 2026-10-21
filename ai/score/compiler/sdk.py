"""The SDK backend: a Timeline written as an @raea/motion composition (SPACE/edit.tsx), rendered by the engine
(raea-motion render). The API is raea/motion/spec/motion.feature; this uses only what's in it.

  clips       <Series> of <Series.Sequence> each holding <Video startFrom endAt> (source frames), framed explicitly:
              object-fit cover, object-position from the clip's pan, a scale for its punch-in (the spec's "how a
              Video fills its box"; framing="face" is the SDK's own when the engine reads the face itself)
  parts       <Sequence from durationInFrames> around the part at its placed y; motion (motion.py) as interpolate()
              over the composition's frame with Material's bezier curves. The space's components (components/<name>.tsx,
              Tailwind compiled beside them, or a registry component placing itself with <Region>); a part the space has no
              component for is refused (`raea add NAME` copies one from the registry)
  captions    the space's components/captions.tsx, which reads the words itself (useWords() from @raea/motion: the
              composition's words.json, passed to the engine with --words)
  sound       <Sequence from><Audio src={lyria|sao(prompt, {seconds})} volume={10 ** (gain / 20)}/></Sequence>
"""
import json, re, subprocess
from pathlib import Path

from .motion import BEZIER, plan

ENGINE = Path.home() / "raea/react/target/release/raea-motion"
TW = Path.home() / "raea/void/tw/tw.ts"


def ident(s):
    return "t_" + re.sub(r"\W", "_", s)


REGION = {"top": "top", "low": "bottom", "center": "center", "auto": "auto", "full": "auto"}  # the script's bands, as <Region> names them
PROJECT_TW = Path.home() / "raea/void/tw/project.ts"
THEME = Path.home() / "raea/cli/template/theme.css"


def placed(space, name):
    """whether a component places itself (a raea registry part: <Region> from perception, the brand theme's classes)
    rather than being placed at the y the compiler worked out"""
    p = space / "components" / f"{name}.tsx"
    return p.exists() and "<Region" in p.read_text()


def project_css(space, edit):
    """the project's stylesheet as `raea render` builds it (the brand's variables, then Tailwind with the brand theme for
    every class the project uses), written to styles.gen.ts beside the edit"""
    r = subprocess.run(["bun", str(PROJECT_TW), str(space), str(THEME)], capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"tailwind: {r.stderr[:300]}")
    (edit.parent / "styles.gen.ts").write_text("// written by score's SDK backend: the brand's variables and the project's Tailwind\nexport const css = " + json.dumps(r.stdout) + ";\n")


def styled(space, name):
    """the component's module: itself, or with its Tailwind classes compiled into a <style> beside it (void's way)"""
    src = space / "components" / f"{name}.tsx"
    text = src.read_text()
    if "className" not in text:
        return f"./components/{name}.tsx"
    css = subprocess.run(["bun", str(TW), str(src)], capture_output=True, text=True)
    if css.returncode != 0 or "export default" not in text:
        raise RuntimeError(f"tailwind: {css.stderr[:300]}")
    wrap = space / "components" / f"{name}_tw.tsx"
    wrap.write_text(text.replace("export default", "const __Part =", 1)
                    + f"\nconst __css = {json.dumps(css.stdout)};\nexport default function Styled(props: any) {{ return (<><style>{{__css}}</style><__Part {{...props}} /></>); }}\n")
    return f"./components/{name}_tw.tsx"


def js(v):
    return json.dumps(v)


def write(tl, space, edit, work, sound=True):
    """SPACE/edit.tsx from the Timeline; answers what didn't go in"""
    edit = edit if edit.suffix == ".tsx" else space / "edit.tsx"
    refused, unsounded, imports, body = list(tl.refused), [], {}, []
    comps = {}
    for p in tl.parts:
        name = p.component if p.kind == "component" else p.kind
        if not (space / "components" / f"{name}.tsx").exists():
            refused.append({"part": f"{name} ({p.name})", "error": "a part is a component in the space: `raea add NAME` copies the registry's, or write components/"
                            f"{name}.tsx)" if p.kind != "component" else f"no components/{name}.tsx"})
            continue
        if name not in comps:
            try:
                comps[name] = f"./components/{name}.tsx" if placed(space, name) else styled(space, name)
            except RuntimeError as e:
                refused.append({"part": f"{name} ({p.name})", "error": str(e)}); continue
            imports[f"C_{name}"] = comps[name]
    for t, f in tl.takes.items():
        imports[ident(t)] = f"./{f}"
    # the picture: one Series of the clips, each framed on its take's face (pan) and punched in (scale)
    clips = []
    for c in tl.clips:
        pos = f"{50 + 50 * (c.pan_x or 0):.1f}% {50 + 50 * (c.pan_y or 0):.1f}%"
        style = {"position": "absolute", "left": 0, "top": 0, "width": tl.width, "height": tl.height, "objectFit": "cover", "objectPosition": pos}
        if c.zoom:
            style["transform"] = f"scale({c.zoom})"
        clips.append(f"      <Series.Sequence durationInFrames={{{c.end - c.start}}}><Video src={{{ident(c.take)}}} startFrom={{{round(c.src_in * tl.fps)}}} "
                     f"endAt={{{round(c.src_out * tl.fps)}}} style={{{js(style)}}} /></Series.Sequence>")
    body.append("    <Series>\n" + "\n".join(clips) + "\n    </Series>")
    # the parts, each in its span at its placed y, moving as its utilities say
    for p in tl.parts:
        name = p.component if p.kind == "component" else p.kind
        if name not in comps:
            continue
        props = dict(p.props)
        tracks = plan(tl, p)
        if placed(space, name):
            # it finds its own band clear of the face, frame by frame, from perception.json: say which band, not where
            box = {"position": "absolute", "left": 0, "top": 0, "width": tl.width, "height": tl.height}
            if p.band and p.band != "auto":  # unplaced: the component's own default (parts top, captions bottom)
                props.setdefault("region", REGION.get(p.band, "auto"))
        else:
            box = {"position": "absolute", "left": round(tl.width * 0.06), "top": p.y or 0, "width": round(tl.width * 0.88)}
        inner = f"<C_{name} {{...{js(props)}}} />"
        moved = f"<Moved from={{{p.start}}} tracks={{{js(tracks)}}} style={{{js(box)}}}>{inner}</Moved>" if tracks else f"<div style={{{js(box)}}}>{inner}</div>"
        body.append(f"    <Sequence from={{{p.start}}} durationInFrames={{{max(1, p.end - p.start)}}} layout=\"none\">{moved}</Sequence>")
    if sound:
        for s in tl.sounds:
            fn = "lyria" if s.model == "lyria" else "sao"
            body.append(f"    <Sequence from={{{s.start}}} layout=\"none\"><Audio src={{{fn}({js(s.prompt)}, {{seconds: {s.secs}}})}} volume={{{10 ** (s.gain / 20):.4f}}} /></Sequence>")
    bg = f"{{background: {js(tl.background)}}}" if tl.background else "{}"
    # the brand's tokens for every part: the host app's brand kit (brand/brand.css: --brand-* variables and .brand-* classes),
    # as a <style> in the composition, so a component can use var(--brand-accent) or class "brand-h1". Remote @imports
    # (Google Fonts) are left out: the engine draws with the fonts it has
    brand_css = space / "brand" / "brand.css"
    if brand_css.exists():
        css = "\n".join(l for l in brand_css.read_text().splitlines() if not l.strip().startswith("@import"))
        # the kit's scale from the site's px to the frame (2.5 x width / 1080, as the space's kit uses it), so a component
        # can size type as calc(var(--brand-h1-size) * var(--brand-scale))
        css += f"\n:root {{ --brand-scale: {2.5 * tl.width / 1080:g}; }}"
        body.insert(0, f"    <style>{{{js(css)}}}</style>")
    if any(placed(space, n) for n in comps):
        project_css(space, edit)
        imports["{ css as projectCss }"] = "./styles.gen"
        body.insert(0, "    <style>{projectCss}</style>")
    lines = ['import { AbsoluteFill, Audio, Easing, Sequence, Series, Video, interpolate, lyria, sao, useCurrentFrame } from "@raea/motion";']
    lines += [f'import {k} from "{v}";' for k, v in imports.items()]
    lines += ["",
              f"const EASE = {js({k: list(v) for k, v in BEZIER.items()})};",
              "// a part's motion: each track's keyframes over the composition's frame (x and y offsets from its place); inside",
              "// its Sequence the frame counts from the part's start, so `from` puts it back on the composition's clock",
              "function Moved({ from, tracks, style, children }: any) {",
              "  const f = useCurrentFrame() + from;",
              "  const at = (col: string, rest: number) => { const t = tracks.find((x: any) => x[0] === col); if (!t) return rest;",
              "    const ks = Object.keys(t[1]).map(Number).sort((a, b) => a - b); const [a, b, c, d] = EASE[t[2]];",
              "    return interpolate(f, ks, ks.map((k) => t[1][k]), { extrapolateLeft: \"clamp\", extrapolateRight: \"clamp\", easing: Easing.bezier(a, b, c, d) }); };",
              "  const x = at(\"x\", 0), y = at(\"y\", 0), s = at(\"scale\", 1);",
              "  return <div style={{ ...style, opacity: at(\"opacity\", 1), transform: `translate(${x}px, ${y}px) scale(${s})` }}>{children}</div>;",
              "}",
              "",
              f"export const config = {js({'width': tl.width, 'height': tl.height, 'fps': tl.fps, 'durationInFrames': tl.frames})};",
              "export default function Edit() {",
              f"  return (\n  <AbsoluteFill style={{{bg}}}>",
              *body,
              "  </AbsoluteFill>\n  );",
              "}"]
    edit.write_text("\n".join(lines) + "\n")
    # the Clock's words and perception, for `raea-motion render --words --perception` (an agent session, b298db2): useWords()
    # and word() read the kept words in composition frames; Region and --face-*/--safe-* read each clip's face and the
    # platforms' safe area from the frame it starts on, and
    # the speaker's face through the clip when its take is ingested (frame-keyed: the engine reads the latest key)
    words = [{"text": w["text"], "from": round(w["start_ms"] / 1000 * tl.fps), "to": max(round(w["start_ms"] / 1000 * tl.fps) + 1, round(w["end_ms"] / 1000 * tl.fps))} for w in tl.words]
    safe = {"top": tl.vars.get("--safe-top"), "bottom": tl.vars.get("--safe-bottom")}
    perc = {}
    for c in tl.clips:
        v = c.vars
        p_ = {"safe": safe}
        if v.get("--face-top") is not None and v.get("--face-left") is not None:
            p_["face"] = {"x": round(v["--face-left"]), "y": round(v["--face-top"]), "w": round(v["--face-right"] - v["--face-left"]), "h": round(v["--face-bottom"] - v["--face-top"])}
        perc[str(c.start)] = p_
        # the speaker's face at each 4 fps sample of the clip, when the take is ingested: Region follows it
        for t, bx in v.get("--track", []):
            f = c.start + round((t - c.src_in) * tl.fps)
            if c.start < f < c.end:
                perc[str(f)] = {"safe": safe, "face": {"x": bx[0], "y": bx[1], "w": bx[2], "h": bx[3]}} if bx else {"safe": safe}
    sidecar(edit, "words.json").write_text(json.dumps(words))
    sidecar(edit, "perception.json").write_text(json.dumps(perc))
    return refused, unsounded


def sidecar(edit, name):
    """a file the composition is rendered with, beside it: words.json for edit.tsx, NAME.words.json for NAME.tsx, so
    the pieces of one workspace (sdk/brief/format.feature) don't share them"""
    return edit.parent / (name if edit.stem == "edit" else f"{edit.stem}.{name}")


def render_args(space, edit):
    """the engine's flags for this composition: its size and length, and the words and perception beside it"""
    edit = edit if edit.suffix == ".tsx" else space / "edit.tsx"
    cfg = json.loads(re.search(r"export const config = (\{.*\});", edit.read_text()).group(1))
    out = ["--w", str(cfg["width"]), "--h", str(cfg["height"]), "--fps", str(cfg["fps"]), "--frames", str(cfg["durationInFrames"])]
    for name, flag in (("words.json", "--words"), ("perception.json", "--perception")):
        if sidecar(edit, name).exists():
            out += [flag, str(sidecar(edit, name))]
    return out


def check(space, edit):
    """the engine renders the composition's first frame: whether it compiles and draws"""
    edit = edit if edit.suffix == ".tsx" else space / "edit.tsx"
    r = subprocess.run([str(ENGINE), "render", str(edit), "--frame", "0", "-o", str(space / ".score" / "check.png")] + render_args(space, edit),
                       capture_output=True, text=True, cwd=space)
    said = (r.stdout + r.stderr).strip().splitlines()
    return ([] if r.returncode == 0 else said[-6:]), []
