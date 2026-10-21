// void's header (apps/void/shell.feature: The header): the window's title bar, the same in every space. The traffic lights
// sit over its left end; then the cube, which shows and hides the files on the left (as ⌘B does); the tabs; and the cat
// at the right, which shows and hides the chat (as ⌘J does). A click asks void's chrome ({ do }); the rest of the bar
// drags the window ({ drag }).
import { AbsoluteFill, applyActions, useGameState, voidIn } from "@raea/motion";
import type { FrameInput, PanelActions } from "@raea/motion";

const H = 40, LIGHTS = 78, BTN = 30, GAP = 6, TAB = 200;
const CUBE = [LIGHTS, LIGHTS + BTN];
const TABS = CUBE[1] + GAP;

type Shell = { files: boolean; chat: boolean; space: string; title: string };
type Do = { n: number; what: string; x: number; y: number; arg: string };
type S = { shell: Shell; object: { name?: string; kind?: string } | null; do: Do; down: boolean };

const ask = (s: S, what: string, arg = ""): S => ({ ...s, do: { n: s.do.n + 1, what, x: 0, y: H, arg } });

export const actions: PanelActions<S> = {
  files: { about: "Show or hide the files on the left", run: (s) => ask(s, "toggle", "files") },
  chat: { about: "Show or hide the chat on the right", run: (s) => ask(s, "toggle", "chat") },
};

const within = (x: number, [a, b]: number[]) => x >= a && x < b;

// the panel's width, as last laid out (the cat sits at its right end)
let W = 800;
const catAt = () => [W - 8 - BTN, W - 8];

const step = (state: S, input: FrameInput): S => {
  let s = applyActions(actions, state, input);
  const sh = voidIn<Shell>(input, "shell");
  if (sh) s = { ...s, shell: sh };
  const o = voidIn<{ name?: string; kind?: string } | null>(input, "object");
  if (o !== undefined) s = { ...s, object: o };
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down) {
    if (within(p.x, CUBE)) s = actions.files.run(s, {});
    else if (within(p.x, catAt())) s = actions.chat.run(s, {});
  }
  return down === s.down ? s : { ...s, down };
};

export default function Header({ width }: { width: number; height: number }) {
  W = width;
  const cat = catAt();
  const s = useGameState<S>({ shell: { files: true, chat: true, space: "", title: "" }, object: null, do: { n: 0, what: "", x: 0, y: 0, arg: "" }, down: false }, step);
  // the bar drags the window, but for the buttons and the tab
  const drag = [[0, 0, CUBE[0], H], [TABS + TAB, 0, cat[0], H]];
  const btn = (on: boolean) => ({ position: "absolute" as const, top: 5, width: BTN, height: BTN, borderRadius: 7, display: "flex", alignItems: "center", justifyContent: "center", background: on ? "#1f1f25" : "transparent" });
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ do: s.do, drag })} style={{ background: "#0b0b0d", borderBottom: "1px solid #1c1c21", fontFamily: "Inter", fontSize: 12, color: "#e8e8ea" }}>
      <div style={{ ...btn(s.shell.files), left: CUBE[0] }}>
        <svg width={16} height={16} viewBox="0 0 24 24">
          <path d="M12 2.5 21 7v10l-9 4.5L3 17V7z M3 7l9 4.5L21 7 M12 11.5v10" fill="none" stroke={s.shell.files ? "#e8e8ea" : "#8b8b94"} strokeWidth={1.6} strokeLinejoin="round" />
        </svg>
      </div>
      <div style={{ position: "absolute", left: TABS, top: 6, width: TAB, height: 28, borderRadius: 7, background: "#17171b", border: "1px solid #24242a", boxSizing: "border-box", display: "flex", alignItems: "center", gap: 6, padding: "0 10px", overflow: "hidden", whiteSpace: "nowrap" }}>
        <span style={{ color: "#8b8b94" }}>{s.shell.title || s.shell.space}</span>
        {s.object?.name ? <span style={{ fontWeight: 600, overflow: "hidden" }}>{s.object.name}</span> : null}
      </div>
      <div style={{ ...btn(s.shell.chat), left: cat[0] }}>
        <svg width={16} height={16} viewBox="0 0 24 24">
          <path d="M5 20V9l-1-5 4.5 3h7L20 4l-1 5v11z M9 13h.01 M15 13h.01" fill="none" stroke={s.shell.chat ? "#e8e8ea" : "#8b8b94"} strokeWidth={1.6} strokeLinejoin="round" strokeLinecap="round" />
        </svg>
      </div>
    </AbsoluteFill>
  );
}
