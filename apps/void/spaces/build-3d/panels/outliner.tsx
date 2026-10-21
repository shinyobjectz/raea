// The scene's parts, one row each; a click picks one, and the object on the stage hears which (data-void-publish). A panel is a component played live like a game: its state a
// fold over the pointer, in its own CSS pixels.
import { AbsoluteFill, applyActions, useGameState } from "@raea/motion";
import type { FrameInput, PanelActions } from "@raea/motion";

const ROWS = ["Camera", "Sun", "Ground", "Rock 1", "Rock 2", "Rock 3"];
const TOP = 36, ROW = 30;

type S = { picked: number; down: boolean };

// what a person and the agent can do here: a click on a row calls pick, and the agent's panel_action tool hands it in
export const actions: PanelActions<S> = {
  pick: { about: "Pick one of the scene's parts in the outliner, by its row (0 Camera, 1 Sun, 2 Ground, 3-5 Rock 1-3)", args: { row: "number, 0-5" }, run: (s, a) => (a.row >= 0 && a.row < ROWS.length ? { ...s, picked: a.row } : s) },
};

const step = (state: S, input: FrameInput): S => {
  let s = applyActions(actions, state, input);
  const p = input.pointer;
  const down = !!p?.down;
  // a press (not a held button) on a row picks it
  if (p && down && !s.down) {
    const i = Math.floor((p.y - TOP) / ROW);
    s = actions.pick.run(s, { row: i });
  }
  return down === s.down ? s : { ...s, down };
};

export default function Outliner() {
  const s = useGameState<S>({ picked: 2, down: false }, step);
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ picked: s.picked, part: ROWS[s.picked] })} style={{ background: "#111113", color: "#e8e8ea", fontFamily: "Inter", fontSize: 13, padding: "10px 8px" }}>
      <div style={{ height: TOP - 10, fontSize: 10.5, letterSpacing: 1.2, color: "#77777d", paddingLeft: 8 }}>SCENE</div>
      {ROWS.map((r, i) => (
        <div key={r} style={{ height: ROW, lineHeight: `${ROW}px`, padding: "0 10px", borderRadius: 7, background: i === s.picked ? "#2b4bff" : "transparent", color: i === s.picked ? "#fff" : undefined }}>
          {r}
        </div>
      ))}
    </AbsoluteFill>
  );
}
