// The drawing's layers, topmost first: what the drawing on the stage publishes (voidIn "stage"), a row each. A click
// picks a row; the agent adds, removes and clears shapes through this panel's actions, which it publishes as numbered
// ops the drawing applies once each.
import { AbsoluteFill, applyActions, useGameState, voidIn } from "@raea/motion";
import type { FrameInput, PanelActions } from "@raea/motion";

type Shape = { kind: "rect" | "ellipse" | "path"; color: string };
type Op = { n: number; op: "add" | "remove" | "clear"; shape?: any; row?: number };
type S = { shapes: Shape[]; picked: number; ops: Op[]; n: number; down: boolean };

const TOP = 36, ROW = 28;
const push = (s: S, op: Omit<Op, "n">): S => ({ ...s, n: s.n + 1, ops: [...s.ops, { ...op, n: s.n + 1 }].slice(-32) });

export const actions: PanelActions<S> = {
  add: {
    about: "Draw a shape on the drawing (its pixels, 1280×720): a rect {kind:'rect', x, y, w, h}, an ellipse {kind:'ellipse', cx, cy, rx, ry} or a path {kind:'path', points:[[x,y],...], closed}; color is a CSS colour",
    args: { shape: "object: {kind, ..., color}" },
    run: (s, a) => (a.shape && ["rect", "ellipse", "path"].includes(a.shape.kind) ? push(s, { op: "add", shape: a.shape }) : s),
  },
  remove: { about: "Remove the shape on a row of the layers (0 is the topmost)", args: { row: "number, from 0" }, run: (s, a) => push(s, { op: "remove", row: Number(a.row) }) },
  clear: { about: "Remove every shape", run: (s) => push(s, { op: "clear" }) },
  pick: { about: "Pick a row of the layers (the drawing outlines its shape)", args: { row: "number, from 0" }, run: (s, a) => ({ ...s, picked: Number(a.row) }) },
};

const step = (state: S, input: FrameInput): S => {
  let s = applyActions(actions, state, input);
  const seen = voidIn<{ shapes: Shape[] }>(input, "stage");
  if (seen?.shapes) s = { ...s, shapes: seen.shapes };
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down) {
    const row = Math.floor((p.y - TOP) / ROW);
    if (row >= 0 && row < s.shapes.length) s = { ...s, picked: row };
  }
  return down === s.down ? s : { ...s, down };
};

export default function Layers() {
  const s = useGameState<S>({ shapes: [], picked: -1, ops: [], n: 0, down: false }, step);
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ picked: s.picked, ops: s.ops })} style={{ background: "#111113", color: "#e8e8ea", fontFamily: "Inter", fontSize: 13, padding: "10px 8px" }}>
      <div style={{ height: 26, color: "#8b8b94", fontSize: 11, letterSpacing: 1, paddingLeft: 8 }}>LAYERS</div>
      {s.shapes.length === 0 ? <div style={{ color: "#6b6b74", padding: "6px 8px" }}>Nothing drawn yet</div> : null}
      {s.shapes.map((sh, i) => (
        <div key={i} style={{ height: ROW, display: "flex", alignItems: "center", gap: 8, padding: "0 8px", borderRadius: 6, background: i === s.picked ? "#2b4bff" : "transparent" }}>
          <div style={{ width: 12, height: 12, borderRadius: sh.kind === "ellipse" ? 6 : 3, background: sh.color, boxShadow: "0 0 0 1px #3a3a42" }} />
          {`${sh.kind[0].toUpperCase()}${sh.kind.slice(1)} ${s.shapes.length - i}`}
        </div>
      ))}
    </AbsoluteFill>
  );
}
