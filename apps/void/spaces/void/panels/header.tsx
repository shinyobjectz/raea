// void's header: the settings (the cat), the workspace's objects (the cube), the object on the stage by name, and the
// spaces to switch to. A click asks void's chrome ({ do }) or types a command ({ send }).
import { AbsoluteFill, applyActions, useGameState, voidIn } from "@raea/motion";
import type { FrameInput, PanelActions } from "@raea/motion";

const SPACES: [string, string][] = [["void", "void"], ["Video", "video"], ["Build 3D", "build-3d"], ["Vector", "vector"]];
const LEFT = 8, BTN = 32, GAP = 4, TITLE = 240, W = 84;
const CHIPS = LEFT + 2 * (BTN + GAP) + TITLE;
// right of the chips the header is the window's title bar: a press there drags the window
const DRAG = [[CHIPS + SPACES.length * (W + GAP), 0, 100000, 100]];

type Do = { n: number; what: string; x: number; y: number };
type S = { object: { name?: string; kind?: string } | null; send: { n: number; text: string }; do: Do; down: boolean };

const ask = (s: S, what: string, x = 0, y = 40): S => ({ ...s, do: { n: s.do.n + 1, what, x, y } });

export const actions: PanelActions<S> = {
  objects: { about: "Open the menu of this workspace's objects", run: (s) => ask(s, "objects") },
  settings: { about: "Open void's settings menu", run: (s) => ask(s, "settings", LEFT, 40) },
  space: { about: `Switch to another space: ${SPACES.map((s) => s[0]).join(", ")}`, args: { name: "string" }, run: (s, a) => {
    const hit = SPACES.find(([n, slug]) => n.toLowerCase() === String(a.name).toLowerCase() || slug === String(a.name).toLowerCase());
    return hit ? { ...s, send: { n: s.send.n + 1, text: `/space ${hit[1]}` } } : s;
  } },
};

const step = (state: S, input: FrameInput): S => {
  let s = applyActions(actions, state, input);
  const o = voidIn<{ name?: string; kind?: string } | null>(input, "object");
  if (o !== undefined) s = { ...s, object: o };
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down) {
    const i = Math.floor((p.x - CHIPS) / (W + GAP));
    if (p.x >= LEFT && p.x < LEFT + BTN) s = ask(s, "settings", p.x, 40);
    else if (p.x >= LEFT + BTN + GAP && p.x < LEFT + 2 * BTN + GAP) s = actions.objects.run(s, {});
    else if (p.x >= CHIPS && i < SPACES.length && (p.x - CHIPS) % (W + GAP) < W) s = actions.space.run(s, { name: SPACES[i][1] });
  }
  return down === s.down ? s : { ...s, down };
};

const button = { width: BTN, height: 28, marginRight: GAP, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: 7, background: "#1c1c21" };

export default function Header() {
  const s = useGameState<S>({ object: null, send: { n: 0, text: "" }, do: { n: 0, what: "", x: 0, y: 0 }, down: false }, step);
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ send: s.send, do: s.do, drag: DRAG })} style={{ background: "#111113", flexDirection: "row", alignItems: "center", paddingLeft: LEFT, fontFamily: "Inter", fontSize: 13, color: "#e8e8ea" }}>
      <div style={button}>
        <svg width={16} height={16} viewBox="0 0 24 24">
          <path d="M5 20V9l-1-5 4.5 3h7L20 4l-1 5v11z M9 13h.01 M15 13h.01" fill="none" stroke="#c9c9d1" strokeWidth={1.6} strokeLinejoin="round" strokeLinecap="round" />
        </svg>
      </div>
      <div style={button}>
        <svg width={16} height={16} viewBox="0 0 24 24">
          <path d="M12 2.5 21 7v10l-9 4.5L3 17V7z M3 7l9 4.5L21 7 M12 11.5v10" fill="none" stroke="#c9c9d1" strokeWidth={1.6} strokeLinejoin="round" />
        </svg>
      </div>
      <div style={{ width: TITLE, boxSizing: "border-box", paddingLeft: 8, display: "flex", alignItems: "center", gap: 10, overflow: "hidden" }}>
        <div style={{ fontWeight: 600, whiteSpace: "nowrap" }}>{s.object?.name ?? "Nothing open"}</div>
        {s.object?.kind ? <div style={{ color: "#8b8b94", fontSize: 12 }}>{s.object.kind}</div> : null}
      </div>
      {SPACES.map(([name, slug]) => (
        <div key={slug} style={{ width: W, marginRight: GAP, height: 28, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: 7, fontSize: 12, background: slug === "void" ? "#26262b" : "transparent", color: slug === "void" ? "#e8e8ea" : "#9a9aa3" }}>
          {name}
        </div>
      ))}
    </AbsoluteFill>
  );
}
