// The Vector space's starter object: a drawing, an SVG document whose shapes are a fold over the pointer on the stage
// and what the space's panels publish (@raea/motion spaceIn): the tool and colour from the tools row, the agent's ops
// and the picked row from the layers. It publishes its shapes (data-void-publish) for the layers to list. A pure fold
// like any game, so a recorded session replays to the same drawing.
import { AbsoluteFill, Composition, registerRoot, spaceIn, useGameState, type FrameInput } from "@raea/motion";

const CW = 1280, CH = 720;
type Pt = [number, number];
type Shape =
  | { kind: "rect"; x: number; y: number; w: number; h: number; color: string }
  | { kind: "ellipse"; cx: number; cy: number; rx: number; ry: number; color: string }
  | { kind: "path"; points: Pt[]; closed: boolean; color: string };
type Op = { n: number; op: "add" | "remove" | "clear"; shape?: Shape; row?: number };
type S = { shapes: Shape[]; draft: Pt[]; from: Pt | null; at: Pt | null; down: boolean; keys: string[]; applied: number; tool: string; color: string; picked: number };

// what's drawn before anyone draws: a sun over two hills
const START: Shape[] = [
  { kind: "path", points: [[0, 560], [260, 440], [520, 540], [520, 720], [0, 720]], closed: true, color: "#2f9e5b" },
  { kind: "path", points: [[380, 600], [760, 430], [1280, 580], [1280, 720], [380, 720]], closed: true, color: "#3fb36d" },
  { kind: "ellipse", cx: 980, cy: 190, rx: 80, ry: 80, color: "#f5c400" },
];
const INITIAL: S = { shapes: START, draft: [], from: null, at: null, down: false, keys: [], applied: 0, tool: "pen", color: "#1b1b1f", picked: -1 };
const CLOSE = 14;

const box = (a: Pt, b: Pt) => ({ x: Math.min(a[0], b[0]), y: Math.min(a[1], b[1]), w: Math.abs(a[0] - b[0]), h: Math.abs(a[1] - b[1]) });
const shapeFrom = (tool: string, a: Pt, b: Pt, color: string): Shape | null => {
  const r = box(a, b);
  if (r.w < 4 || r.h < 4) return null;
  return tool === "ellipse" ? { kind: "ellipse", cx: r.x + r.w / 2, cy: r.y + r.h / 2, rx: r.w / 2, ry: r.h / 2, color } : { kind: "rect", ...r, color };
};

const step = (state: S, input: FrameInput): S => {
  const tools = spaceIn<{ tool: string; color: string }>(input, "tools") ?? { tool: "pen", color: "#1b1b1f" };
  const layers = spaceIn<{ ops: Op[]; picked: number }>(input, "layers");
  let s = state;
  if (tools.tool !== s.tool || tools.color !== s.color) s = { ...s, tool: tools.tool, color: tools.color };
  if (layers && layers.picked !== s.picked) s = { ...s, picked: layers.picked };
  // the layers' ops, each once (rows count from the topmost shape)
  for (const o of layers?.ops ?? []) {
    if (o.n <= s.applied) continue;
    const shapes = o.op === "clear" ? [] : o.op === "add" && o.shape ? [...s.shapes, o.shape] : s.shapes.filter((_, i) => i !== s.shapes.length - 1 - (o.row ?? -1));
    s = { ...s, shapes, applied: o.n };
  }
  // keys newly down: Enter ends the pen's shape open, Escape drops it
  const keys = (input.keys ?? []).filter((k) => k === "Enter" || k === "Escape");
  const pressed = (k: string) => keys.includes(k) && !s.keys.includes(k);
  if (pressed("Enter") && s.draft.length >= 2) s = { ...s, shapes: [...s.shapes, { kind: "path", points: s.draft, closed: false, color: tools.color }], draft: [] };
  if (pressed("Escape")) s = { ...s, draft: [], from: null };
  if (keys.join() !== s.keys.join()) s = { ...s, keys };
  // the pointer
  const p = input.pointer;
  const down = !!p?.down;
  const at: Pt | null = p ? [Math.round(p.x), Math.round(p.y)] : null;
  if (at && down && !s.down) {
    if (tools.tool === "pen") {
      const first = s.draft[0];
      if (first && s.draft.length >= 3 && Math.hypot(at[0] - first[0], at[1] - first[1]) < CLOSE) s = { ...s, shapes: [...s.shapes, { kind: "path", points: s.draft, closed: true, color: tools.color }], draft: [] };
      else s = { ...s, draft: [...s.draft, at] };
    } else s = { ...s, from: at };
  }
  if (!down && s.down && s.from && at) {
    const sh = shapeFrom(tools.tool, s.from, at, tools.color);
    s = { ...s, from: null, shapes: sh ? [...s.shapes, sh] : s.shapes };
  }
  if (down !== s.down) s = { ...s, down };
  if (at && (!s.at || at[0] !== s.at[0] || at[1] !== s.at[1])) s = { ...s, at };
  return s;
};

const d = (pts: Pt[], closed: boolean) => pts.map((p, i) => `${i ? "L" : "M"}${p[0]} ${p[1]}`).join(" ") + (closed ? " Z" : "");

function Draw({ sh, outline }: { sh: Shape; outline?: boolean }) {
  const line = outline ? { fill: "none", stroke: "#2b4bff", strokeWidth: 3, strokeDasharray: "8 6" } : null;
  if (sh.kind === "rect") return <rect x={sh.x} y={sh.y} width={sh.w} height={sh.h} fill={sh.color} {...line} />;
  if (sh.kind === "ellipse") return <ellipse cx={sh.cx} cy={sh.cy} rx={sh.rx} ry={sh.ry} fill={sh.color} {...line} />;
  return <path d={d(sh.points, sh.closed)} fill={sh.closed ? sh.color : "none"} stroke={sh.closed ? "none" : sh.color} strokeWidth={4} strokeLinejoin="round" strokeLinecap="round" {...line} />;
}

function Drawing() {
  const s = useGameState(INITIAL, step);
  const top = s.shapes.length - 1 - s.picked;
  const rubber = s.from && s.at && s.down ? shapeFrom(s.tool, s.from, s.at, s.color) : null;
  // the shapes, topmost first, for the layers panel
  const listed = JSON.stringify({ shapes: [...s.shapes].reverse().map((sh) => ({ kind: sh.kind, color: sh.color })) });
  return (
    <AbsoluteFill data-void-publish={listed} style={{ background: "#f4f1ea" }}>
      <svg width={CW} height={CH} viewBox={`0 0 ${CW} ${CH}`}>
        {s.shapes.map((sh, i) => <Draw key={i} sh={sh} />)}
        {s.shapes[top] ? <Draw sh={s.shapes[top]} outline /> : null}
        {rubber ? <Draw sh={rubber} /> : null}
        {s.draft.length ? <path d={d(s.at && s.tool === "pen" ? [...s.draft, s.at] : s.draft, false)} fill="none" stroke={s.color} strokeWidth={3} strokeDasharray="6 6" /> : null}
        {s.draft.map((p, i) => <circle key={i} cx={p[0]} cy={p[1]} r={i === 0 ? 7 : 4} fill="#ffffff" stroke="#2b4bff" strokeWidth={2} />)}
      </svg>
      <div style={{ position: "absolute", left: 24, bottom: 20, fontFamily: "Inter", fontSize: 18, color: "#5b5b63" }}>{`${s.tool[0].toUpperCase()}${s.tool.slice(1)} · ${s.shapes.length} shapes`}</div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="edit" component={Drawing} width={CW} height={CH} fps={60} durationInFrames={1000000000} />);
