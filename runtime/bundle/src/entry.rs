use super::*;

pub fn entry(file: &str, o: &CompOpts) -> String {
    let (comp, w, h, fps, frames) = (o.composition.as_deref(), o.w, o.h, o.fps, o.frames);
    let js = |v: &Option<Value>| v.as_ref().map(Value::to_string).unwrap_or("null".into());
    let (words, perception, input) = (js(&o.words), js(&o.perception), js(&o.input));
    let given = serde_json::to_string(&o.given).unwrap_or("[]".into());
    let props = format!("{} x{} hold {:?}", js(&o.props), o.scale.unwrap_or(1.0), o.hold);
    // the composition's element made once, so a frame re-renders only what reads the frame (its clock, through
    // useCurrentFrame), not every part (docs/control.feature FC1b). A frame is already pure (a still renders any
    // frame alone), so a part drawing Math.random or Date in render without reading the frame breaks either way.
    // RAEA_STILL_TREE=0 makes it each frame, as before. In the entry's text, so the bundle cache tells the two apart
    let still = std::env::var("RAEA_STILL_TREE").map_or(true, |v| v != "0");
    format!(
        r#"import "./props"; // --props, set before the edit's modules run: {props}
import {{ frameCallbacks, pureAgain, pureFrame, pureSession, seedOf }} from "./motion/pure";
import {{ canvas2d }} from "./motion/canvas2d";
import * as BevyReact from "bevy-react";
import {{ mount, addEventListener, BevyCanvasElement }} from "bevy-react";
canvas2d(BevyCanvasElement); // a <canvas> ref's getContext("2d") is the web's (motion canvas2d.ts, #55)
// a ref has the DOM a web component reads (getBoundingClientRect, offsetWidth, children, querySelector ...):
// on bevy-react's BevyElement, which raea's patch adds (a bridge without it keeps plain handles; layout-reads LR1, LR3, LR4)
import {{ domReads }} from "./motion/dom-reads";
domReads(BevyReact as any);
import {{ Component, useEffect, useMemo, useState }} from "react";
import * as M from "./src/{file}";
import {{ Clock, getCompositions, tickClocks }} from "./motion";
const want = {comp};
const comps = getCompositions();
const reg = comps.find((c) => want === null || c.id === want) ?? null;
if (want !== null && !reg) throw new Error(`no composition "${{want}}" (registered: ${{comps.map((c) => c.id).join(", ") || "none"}})`);
const C: any = reg ? reg.component : ((M as any).default ?? Object.values(M).find((v) => typeof v === "function"));
if (!C) throw new Error("{file} exports no component and registers no composition");
// an edit with no <Composition> may state its size and length as `export const config = {{width, height, fps,
// durationInFrames}}` (score's SDK backend writes it); its positive numbers win over the CLI's settings
const MC: any = (M as any).config ?? {{}};
const pos = (v: any, d: number) => (typeof v === "number" && v > 0 ? v : d);
const cfg = reg ?? {{ id: "main", width: pos(MC.width, {w}), height: pos(MC.height, {h}), fps: pos(MC.fps, {fps}), durationInFrames: pos(MC.durationInFrames, {frames}), defaultProps: (globalThis as any).__raea_input_props ?? {{}} }};
if (!reg && (globalThis as any).__raea_scale !== undefined) console.warn("NOT APPLIED  --scale: it draws a <Composition> at a fraction of its size; this file registers none (--w and --h size it)");
for (const [k, key, v] of {given} as [string, string, number][]) if ((cfg as any)[key] !== v) console.warn(`NOT APPLIED  ${{k}} ${{v}}: the composition says its ${{key}} is ${{(cfg as any)[key]}}; ${{k}} only sizes a file with no <Composition> or config`);
// the Clock's words (frames {{text, from, to}}, or seconds {{text, start, end}} / {{text, start_ms, end_ms}}), perception
// (one for every frame, or by frame: the latest at or before it) and input (a timeline, or inputs.rrl's step keys)
const RAW_WORDS: any[] | null = {words};
const PERC: any = {perception};
const RAW_INPUT: any = {input};
const sec = (w: any, k: string) => (w[k + "_ms"] != null ? w[k + "_ms"] / 1000 : w[k]);
const WORDS = (RAW_WORDS ?? []).map((w: any) => {{
  if (w.from != null) return {{ text: w.text ?? w.word, from: w.from, to: w.to }};
  const from = Math.round(sec(w, "start") * cfg.fps);
  return {{ text: w.text ?? w.word, from, to: Math.max(from + 1, Math.round(sec(w, "end") * cfg.fps)) }};
}});
const byFrame = PERC && typeof PERC === "object" && Object.keys(PERC).length > 0 && Object.keys(PERC).every((k) => /^\d+$/.test(k));
const percAt = (f: number) => {{
  if (!PERC) return {{}};
  if (!byFrame) return PERC;
  let v = {{}};
  for (const k of Object.keys(PERC).map(Number).sort((a, b) => a - b)) if (k <= f) v = PERC[k];
  return v;
}};
const INPUT: any = (() => {{
  if (!RAW_INPUT) return {{}};
  if (!RAW_INPUT.steps) return RAW_INPUT;
  const keys = Object.entries(RAW_INPUT.steps as Record<string, Record<string, number>>).map(([k, m]) => [k, Object.keys(m).map(Number).sort((a, b) => a - b).map((f) => [f, m[f]])] as const);
  const out: any = {{}};
  for (let f = 0; f < cfg.durationInFrames; f++) {{
    const held = keys.filter(([, st]) => {{ let v = 0; for (const [g, x] of st) if (g <= f) v = x; return v > 0; }}).map(([k]) => k);
    out[f] = {{ keys: held }};
  }}
  return out;
}})();
// a frame held until what it waits on has come (an <R3F> scene suspended on a load: drei's Environment, a model): while
// any hold is open the frame isn't marked done, and the last release draws it again (each wake), so it's taken whole
const HOLD: any = ((globalThis as any).__raeaHold ??= {{ n: 0, wake: new Set() }});
// a live session's input replayed (render --input of what play or the legacy app logged, every entry marked live): rendered as it
// was played, on an endless clock, so frames past the composition's length are the frames the person saw
const REPLAYS_LIVE = Object.values(INPUT).some((e: any) => e && e.live);
// frame purity (motion/pure.ts): Math.random and Date from the composition's seed (its `seed`, else its id), or from
// the live session a replayed input logged, so the replay draws what was played
const SESSION: any = Object.values(INPUT).find((e: any) => e && e.session)?.session;
if (SESSION) pureSession(SESSION.seed, SESSION.start);
else pureSession(typeof (cfg as any).seed === "number" ? (cfg as any).seed : seedOf(String(cfg.id ?? "main")));
// each commit's flush ends a render pass: the next pass of the same frame (an effect's setState) starts from the frame's seed
const HOST: any = (globalThis as any).__bevyHost;
if (HOST && HOST.op_flush && !HOST.__pure) {{
  const flush = HOST.op_flush;
  HOST.op_flush = (...a: any[]) => {{ const r = flush(...a); pureAgain(); return r; }};
  HOST.__pure = true;
}}
// the composition's settings as its marker carries them: its size, rate and length, and the controls it declares with
// the props they tune (docs/realtime.feature RT4b: what the stage answers in stage.status); props JSON can't write (a
// cycle, a BigInt) are left out rather than failing the composition
const CONF = (() => {{
  const base = {{ width: cfg.width, height: cfg.height, fps: cfg.fps, frames: cfg.durationInFrames }};
  try {{ return JSON.stringify({{ ...base, props: cfg.defaultProps, controls: (cfg as any).controls ?? {{}} }}); }} catch {{ return JSON.stringify(base); }}
}})();
// a throw while rendering the composition, caught and put where Rust reads it (settle), with its message (JSC's stack
// leaves it out) and React's component stack, so the render says what went wrong instead of waiting 20 s for a frame that never commits
class Catch extends Component<{{ children?: any }}, {{ err: string | null }}> {{
  state = {{ err: null as string | null }};
  static getDerivedStateFromError(e: any) {{ return {{ err: (e && e.message ? (e.name || "Error") + ": " + e.message + (e.stack ? "\n" + e.stack : "") : String(e)) }}; }}
  componentDidCatch(e: any, info: any) {{ this.setState({{ err: (e && e.message ? (e.name || "Error") + ": " + e.message + (e.stack ? "\n" + e.stack : "") : String(e)) + (info && info.componentStack ? "\ncomponents:" + info.componentStack : "") }}); }}
  render() {{ return this.state.err ? <div data-raea-error={{this.state.err.slice(0, 4000)}} /> : this.props.children; }}
}}
// live play (raea play, void's stage): each frame's event carries the person's input, kept in one timeline that
// grows a frame at a time (the same object each frame, so a game's fold only checks what's new), marked live: true
const LIVE: any = {{}};
let live = false;
function Root() {{
  const [f, setF0] = useState(-1); const setF = (n: number) => {{ tickClocks(); frameCallbacks(n, cfg.fps); setF0(n); }}; // the Clocks' frame readers told in the same batch (motion clock.tsx tickClocks), and requestAnimationFrame's callbacks (motion frame-globals.ts)
  // the stage's size, for a composition that lays out from useVideoConfig (raea-size)
  const [size, setSize] = useState<{{ w: number; h: number }} | null>(null);
  useEffect(() => addEventListener("raea-size", (v: any) => setSize(v)), []);
  useEffect(() => addEventListener("raea-frame", (v: any) => {{
    if (typeof v === "number") return setF(v);
    live = true;
    // a live session's own seed and start (live.rs), which its input log keeps for a replay
    if (v.session) pureSession(v.session.seed, v.session.start);
    // a live game keeps 4096 frames of input (motion games.ts LIVE_KEEP): it only moves forward
    if (v.input) {{ LIVE[v.frame] = {{ ...v.input, live: true }}; delete LIVE[v.frame - 4096]; }}
    setF(v.frame);
  }}), []);
  // a range of frames rendered back to back (raea-frames: the 3D's rows stepped as a bake steps, scene_frames_to):
  // each frame's commit and its done mark as one raea-frame event makes them, the next frame set off by the mark
  const [to, setTo] = useState(-1);
  useEffect(() => addEventListener("raea-frames", (v: any) => {{ setTo(v.to); setF(v.from); }}), []);
  // the composition, made again only when what Root passes it changes (by default never, RAEA_STILL_TREE=0 each frame; its parts re-render
  // when what they read changes, the frame through the Clock's context)
  // props set from outside while it plays (raea-props: a preview's controls, docs/preview.feature PV4b), over its defaults;
  // each one counted (pn), so the marker says which has landed (docs/realtime.feature RT4e: a paused stage waits for it)
  const [over, setOver] = useState<any>(null); const [pn, setPn] = useState(0);
  useEffect(() => addEventListener("raea-props", (v: any) => {{ setOver(v); setPn((n: number) => n + 1); }}), []);
  // the props it's drawn with, which useVideoConfig().defaultProps and useControl read (RT4c), one object while they hold
  const props = useMemo(() => (over ? {{ ...cfg.defaultProps, ...over }} : cfg.defaultProps), [over]);
  const body = useMemo(() => <Catch><C {{...props}} />{{(() => {{ const T = (globalThis as any).__raeaToneOut; return T ? <T /> : null; }})()}}</Catch>, {still} ? [props] : [f, size, to, props]);
  // the frame's Math.random and Date (motion/pure.ts): reseeded as the frame starts rendering, so every component
  // under it draws the same numbers each time this frame is drawn
  if (f >= 0) pureFrame(f, cfg.fps);
  if (f < 0) return <div data-raea-frame={{-1}} data-raea-config={{CONF}} data-raea-props={{0}} />;
  return (
    <>
      <Clock frame={{f}} fps={{cfg.fps}} width={{size ? size.w : cfg.width}} height={{size ? size.h : cfg.height}} durationInFrames={{live || REPLAYS_LIVE ? 1000000000 : cfg.durationInFrames}} id={{cfg.id}} defaultProps={{props}} words={{WORDS}} perception={{percAt(f)}} input={{live ? LIVE : INPUT}}>
        {{body}}
      </Clock>
      <Marker f={{f}} to={{to}} conf={{CONF}} next={{setF}} pn={{pn}} />
    </>
  );
}}
// the frame is done when its commit and the effects it set off have run: its own effect, after the composition's,
// marks it, and Rust takes the frame the moment that mark lands (settle), with no waiting for quiet. The mark is this
// component's state, not Root's: marking it in Root rendered the whole composition a second time each frame
// (many_cubes' 10 000 parts, docs/control.feature FC1a)
// The props' pass is marked the same way: data-raea-props is the last raea-props whose commit and effects have run.
function Marker({{ f, to, conf, next, pn }}: {{ f: number; to: number; conf: string; next: (f: number) => void; pn: number }}) {{
  const [done, setDone] = useState(-1);
  const [woke, setWoke] = useState(0);
  const [landed, setLanded] = useState(0);
  useEffect(() => {{ const w = () => setWoke((n: number) => n + 1); HOLD.wake.add(w); return () => void HOLD.wake.delete(w); }}, []);
  useEffect(() => {{
    if (f >= 0 && HOLD.n === 0) setDone(f);
  }}, [f, woke]);
  useEffect(() => {{
    if (f >= 0 && done === f && f + 1 < to) next(f + 1);
  }}, [done]);
  useEffect(() => setLanded(pn), [pn]);
  return <div data-raea-frame={{f}} data-raea-done={{done}} data-raea-config={{conf}} data-raea-props={{landed}} />;
}}
mount(<surface id="motion" w={{1}} px={{cfg.width}} pxH={{cfg.height}}><Root /></surface>).catch((globalThis as any).__raea_loop_died);
"#,
        comp = comp.map(|c| format!("{c:?}")).unwrap_or("null".into())
    )
}

/// The entry of a live game's bundle: the composition rendered by raea's live view runtime (sdk/react/js/lite) instead of
/// React. Each frame's event renders the tree once and sends what changed; the frame is marked done in that same pass
/// (React's entry marked it from an effect, a second commit).
pub fn entry_lite(file: &str, o: &CompOpts) -> String {
    let full = entry(file, o);
    // the shared preamble (the composition found, the Clock's words, perception and input), up to React's Root
    let start = full.find("import * as M").expect("entry's imports");
    let end = full.find("// a throw while rendering the composition").expect("entry's Root");
    let bridge = raea_io_home::home().join("sdk/react/js/node_modules/bevy-react/src/bridge.ts");
    let nodes = raea_io_home::home().join("sdk/react/js/node_modules/bevy-react/src/nodes.ts");
    let canvas = raea_io_home::home().join("sdk/react/js/node_modules/bevy-react/src/canvas.ts");
    format!(
        r#"import {{ frameCallbacks, pureAgain, pureFrame, pureSession, seedOf }} from "./motion/pure";
import {{ addEventListener, reset, runEventLoop }} from {bridge:?};
// a ref has the DOM a web component reads, as under React's entry: lite's hosts keep the same nodes (sdk/react/js/lite
// hosts.ts), and a <canvas> ref's getContext("2d") is the web's (layout-reads LR2; #55)
import {{ canvas2d }} from "./motion/canvas2d";
import {{ domReads }} from "./motion/dom-reads";
import * as BevyNodes from {nodes:?};
import {{ BevyCanvasElement }} from {canvas:?};
canvas2d(BevyCanvasElement);
domReads(BevyNodes as any);
import {{ renderRoot, takeRendered }} from "react";
{preamble}const LIVE: any = {{}};
let live = false;
let f = -1;
let size: {{ w: number; h: number }} | null = null;
const conf = CONF;
// the props it's drawn with (raea-props over its defaults; useVideoConfig().defaultProps and useControl read them), and
// how many raea-props have come, which the marker carries once drawn (docs/realtime.feature RT4e)
let PROPS: any = cfg.defaultProps;
let PN = 0;
// the composition's element and the Tone graph's output, made once (each frame made them again: lightspeed Z3)
let COMP = <C {{...PROPS}} />;
let toneEl: any = null;
function toneOut() {{
  const T = (globalThis as any).__raeaToneOut;
  if (!T) return null;
  if (!toneEl || toneEl.type !== T) toneEl = <T />;
  return toneEl;
}}
function tree() {{
  // the frame's Math.random and Date, and requestAnimationFrame's callbacks (motion/pure.ts)
  if (f >= 0) {{ frameCallbacks(f, cfg.fps); pureFrame(f, cfg.fps); }}
  if (f < 0) return <surface id="motion" w={{1}} px={{cfg.width}} pxH={{cfg.height}}><div data-raea-frame={{-1}} data-raea-config={{conf}} data-raea-props={{PN}} /></surface>;
  return (
    <surface id="motion" w={{1}} px={{cfg.width}} pxH={{cfg.height}}>
      <Clock frame={{f}} fps={{cfg.fps}} width={{size ? size.w : cfg.width}} height={{size ? size.h : cfg.height}} durationInFrames={{live || REPLAYS_LIVE ? 1000000000 : cfg.durationInFrames}} id={{cfg.id}} defaultProps={{PROPS}} words={{WORDS}} perception={{percAt(f)}} input={{live ? LIVE : INPUT}}>
        {{COMP}}{{toneOut()}}
      </Clock>
      {marker}
    </surface>
  );
}}
// --comps (a bake): the marker rendered last, carrying the code of every component this frame rendered
function Marker(p: any) {{
  return <div data-raea-frame={{p.f}} data-raea-done={{HOLD.n ? -3 : p.f}} data-raea-config={{conf}} data-raea-props={{PN}} data-raea-comps={{takeRendered()}} />;
}}
HOLD.wake.add(() => {{ if (f >= 0) draw(); }});
// a throw while rendering is put where Rust reads it (settle), as React's entry's Catch did
function draw() {{
  try {{
    renderRoot(tree());
  }} catch (e: any) {{
    renderRoot(<surface id="motion" w={{1}} px={{cfg.width}} pxH={{cfg.height}}><div data-raea-error={{(e && e.message ? (e.name || "Error") + ": " + e.message + (e.stack ? "\n" + e.stack : "") : String(e)).slice(0, 4000)}} data-raea-frame={{f}} data-raea-done={{f}} data-raea-config={{conf}} /></surface>);
  }}
}}
addEventListener("raea-size", (v: any) => {{ size = v; if (f >= 0) draw(); }});
addEventListener("raea-props", (v: any) => {{ PN++; PROPS = v ? {{ ...cfg.defaultProps, ...v }} : cfg.defaultProps; COMP = <C {{...PROPS}} />; if (f >= 0) draw(); }});
// a range of frames, rendered back to back, each its own commit (a bake: lightspeed T3); between frames the effects'
// state changes render, as between two raea-frame events
addEventListener("raea-frames", async (v: any) => {{
  for (let g = v.from; g < v.to; g++) {{
    f = g;
    draw();
    await Promise.resolve();
  }}
}});
addEventListener("raea-frame", (v: any) => {{
  if (typeof v === "number") {{ f = v; return draw(); }}
  live = true;
  if (v.session) pureSession(v.session.seed, v.session.start);
  // a live game keeps 4096 frames of input (motion games.ts LIVE_KEEP): it only moves forward
  if (v.input) {{ LIVE[v.frame] = {{ ...v.input, live: true }}; delete LIVE[v.frame - 4096]; }}
  f = v.frame;
  draw();
}});
// as bevy-react's mount: the bridge reset, the first page, then the events pulled from Bevy for good (each frame's
// render sends its own ops)
reset();
draw();
runEventLoop().catch((globalThis as any).__raea_loop_died);
"#,
        preamble = &full[start..end],
        marker = if o.comps { "<Marker f={f} />" } else { "<div data-raea-frame={f} data-raea-done={HOLD.n ? -3 : f} data-raea-config={conf} data-raea-props={PN} />" },
    )
}
