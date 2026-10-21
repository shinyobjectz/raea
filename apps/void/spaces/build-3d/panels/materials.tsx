// Materials across the top: a swatch each, a click picks one, and the object on the stage hears which.
import { AbsoluteFill, applyActions, useGameState } from "@raea/motion";
import type { FrameInput, PanelActions } from "@raea/motion";

const SWATCHES: [string, string][] = [["Stone", "#8a8178"], ["Moss", "#5f7a3c"], ["Sand", "#d8c49a"], ["Water", "#3d7fb8"], ["Ember", "#e0612f"], ["Snow", "#eef1f4"]];
const LEFT = 12, W = 92, GAP = 6;

type S = { picked: number; down: boolean };

export const actions: PanelActions<S> = {
  pick: { about: `Pick the material by name: ${SWATCHES.map((s) => s[0]).join(", ")}`, args: { name: "string" }, run: (s, a) => {
    const i = SWATCHES.findIndex(([n]) => n.toLowerCase() === String(a.name).toLowerCase());
    return i < 0 ? s : { ...s, picked: i };
  } },
};

const step = (state: S, input: FrameInput): S => {
  let s = applyActions(actions, state, input);
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down) {
    const i = Math.floor((p.x - LEFT) / (W + GAP));
    if (i >= 0 && i < SWATCHES.length && (p.x - LEFT) % (W + GAP) < W) s = actions.pick.run(s, { name: SWATCHES[i][0] });
  }
  return down === s.down ? s : { ...s, down };
};

export default function Materials() {
  const s = useGameState<S>({ picked: 0, down: false }, step);
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ material: SWATCHES[s.picked][0], color: SWATCHES[s.picked][1] })} style={{ background: "#111113", flexDirection: "row", alignItems: "center", gap: GAP, paddingLeft: LEFT, fontFamily: "Inter", fontSize: 12, color: "#e8e8ea" }}>
      {SWATCHES.map(([name, c], i) => (
        <div key={name} style={{ width: W, height: 32, display: "flex", alignItems: "center", gap: 8, padding: "0 8px", borderRadius: 8, background: i === s.picked ? "#26262b" : "transparent", boxShadow: i === s.picked ? "inset 0 0 0 1px #3a3a42" : undefined }}>
          <div style={{ width: 16, height: 16, borderRadius: 5, background: c }} />
          {name}
        </div>
      ))}
    </AbsoluteFill>
  );
}
