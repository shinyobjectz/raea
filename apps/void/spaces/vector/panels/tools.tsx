// The tools across the top: a tool and a colour picked by a click (or the agent's tools.tool and tools.color), and the
// drawing on the stage hears which (data-void-publish).
import { AbsoluteFill, applyActions, useGameState } from "@raea/motion";
import type { FrameInput, PanelActions } from "@raea/motion";

const TOOLS = ["Pen", "Rect", "Ellipse"];
const COLORS: [string, string][] = [["Ink", "#1b1b1f"], ["Red", "#e5484d"], ["Blue", "#2b6cff"], ["Yellow", "#f5c400"], ["Green", "#2f9e5b"]];
const LEFT = 12, TW = 78, CW = 32, GAP = 6, SPLIT = 28;
const COLORS_AT = LEFT + TOOLS.length * (TW + GAP) + SPLIT;

type S = { tool: number; color: number; down: boolean };

const named = (list: string[], name: unknown) => list.findIndex((n) => n.toLowerCase() === String(name).toLowerCase());

export const actions: PanelActions<S> = {
  tool: { about: `Pick the tool: ${TOOLS.join(", ")}`, args: { name: "string" }, run: (s, a) => {
    const i = named(TOOLS, a.name);
    return i < 0 ? s : { ...s, tool: i };
  } },
  color: { about: `Pick the colour: ${COLORS.map((c) => c[0]).join(", ")}`, args: { name: "string" }, run: (s, a) => {
    const i = named(COLORS.map((c) => c[0]), a.name);
    return i < 0 ? s : { ...s, color: i };
  } },
};

const step = (state: S, input: FrameInput): S => {
  let s = applyActions(actions, state, input);
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down) {
    const t = Math.floor((p.x - LEFT) / (TW + GAP));
    const c = Math.floor((p.x - COLORS_AT) / (CW + GAP));
    if (p.x >= LEFT && t < TOOLS.length && (p.x - LEFT) % (TW + GAP) < TW) s = { ...s, tool: t };
    else if (p.x >= COLORS_AT && c < COLORS.length && (p.x - COLORS_AT) % (CW + GAP) < CW) s = { ...s, color: c };
  }
  return down === s.down ? s : { ...s, down };
};

export default function Tools() {
  const s = useGameState<S>({ tool: 0, color: 0, down: false }, step);
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ tool: TOOLS[s.tool].toLowerCase(), color: COLORS[s.color][1] })} style={{ background: "#111113", flexDirection: "row", alignItems: "center", gap: GAP, paddingLeft: LEFT, fontFamily: "Inter", fontSize: 12, color: "#e8e8ea" }}>
      {TOOLS.map((name, i) => (
        <div key={name} style={{ width: TW, height: 32, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: 8, background: i === s.tool ? "#26262b" : "transparent", boxShadow: i === s.tool ? "inset 0 0 0 1px #3a3a42" : undefined }}>
          {name}
        </div>
      ))}
      <div style={{ width: SPLIT - GAP * 2, height: 20, borderLeft: "1px solid #2a2a30", marginLeft: (SPLIT - GAP * 2) / 2 }} />
      {COLORS.map(([name, c], i) => (
        <div key={name} style={{ width: CW, height: 32, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: 8, boxShadow: i === s.color ? "inset 0 0 0 1px #5a5a66" : undefined }}>
          <div style={{ width: 18, height: 18, borderRadius: 9, background: c, boxShadow: "0 0 0 1px #2a2a30" }} />
        </div>
      ))}
    </AbsoluteFill>
  );
}
