// The header: the objects (a click types /open to void, the menu void's cube opens), the object on the stage by name,
// then the spaces to switch to (a click types /space NAME, as the terminal would). Named header, it stands in for void's
// row over the stage.
import { AbsoluteFill, applyActions, useGameState, voidIn } from "@raea/motion";
import type { FrameInput, PanelActions } from "@raea/motion";

const SPACES: [string, string][] = [["void", "void"], ["Video", "video"], ["Build 3D", "build-3d"], ["Vector", "vector"], ["Typewriter", "typewriter"]];
const LEFT = 8, CUBE = 32, TITLE = 260, W = 92, GAP = 4;

type S = { object: { name?: string; kind?: string } | null; send: { n: number; text: string }; down: boolean };

export const actions: PanelActions<S> = {
  objects: { about: "Open the menu of this workspace's objects", run: (s) => ({ ...s, send: { n: s.send.n + 1, text: "/open" } }) },
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
    const from = LEFT + CUBE + TITLE;
    if (p.x >= LEFT && p.x < LEFT + CUBE) s = actions.objects.run(s, {});
    const i = Math.floor((p.x - from) / (W + GAP));
    if (p.x >= from && i < SPACES.length && (p.x - from) % (W + GAP) < W) s = actions.space.run(s, { name: SPACES[i][1] });
  }
  return down === s.down ? s : { ...s, down };
};

export default function Header() {
  const s = useGameState<S>({ object: null, send: { n: 0, text: "" }, down: false }, step);
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ send: s.send })} style={{ background: "#111113", flexDirection: "row", alignItems: "center", paddingLeft: LEFT, fontFamily: "Inter", fontSize: 13, color: "#e8e8ea" }}>
      <div style={{ width: CUBE, height: 28, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: 7, background: "#1c1c21" }}>
        <svg width={16} height={16} viewBox="0 0 24 24">
          <path d="M12 2.5 21 7v10l-9 4.5L3 17V7z M3 7l9 4.5L21 7 M12 11.5v10" fill="none" stroke="#c9c9d1" strokeWidth={1.6} strokeLinejoin="round" />
        </svg>
      </div>
      <div style={{ width: TITLE, boxSizing: "border-box", paddingLeft: 10, display: "flex", alignItems: "center", gap: 10, overflow: "hidden" }}>
        <div style={{ fontWeight: 600, whiteSpace: "nowrap" }}>{s.object?.name ?? "Nothing open"}</div>
        {s.object?.kind ? <div style={{ color: "#8b8b94", fontSize: 12 }}>{s.object.kind}</div> : null}
      </div>
      {SPACES.map(([name, slug]) => (
        <div key={slug} style={{ width: W, marginRight: GAP, height: 28, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: 7, fontSize: 12, background: slug === "video" ? "#26262b" : "transparent", color: slug === "video" ? "#e8e8ea" : "#9a9aa3" }}>
          {name}
        </div>
      ))}
    </AbsoluteFill>
  );
}
