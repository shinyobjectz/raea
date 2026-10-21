// void's menu: the conversation (new chat, chats, clear), the object's files, the voice, the objects, the workspace
// and stop. A press asks void for it; × or Escape closes.
import { AbsoluteFill, useGameState, voidIn } from "@raea/motion";
import type { FrameInput } from "@raea/motion";
import { C, Frame, NO_ASKS, Row, ask, onClose, rowAt, typed, type Asks } from "./sheet";

const W = 300;
// [label, what void is asked, whether the sheet closes after]
const ITEMS: [string, string, string][] = [
  ["New chat", "new-chat", "close"],
  ["Chats…", "open:chats", "close"],
  ["Clear the screen", "clear", "close"],
  ["Files…", "open:files", "close"],
  ["Voice…", "open:voice", "close"],
  ["Objects…", "open:objects", "close"],
  ["Switch workspace…", "open:workspaces", "close"],
  ["Stop void", "stop", "close"],
];

type S = { busy: boolean; asks: Asks; prev: string[]; down: boolean };

const step = (s: S, input: FrameInput): S => {
  const st = voidIn<{ busy: boolean }>(input, "state");
  if (st) s = { ...s, busy: st.busy };
  const { keys, down: pressed } = typed(s.prev, input);
  if (pressed.includes("Escape")) s = { ...s, asks: ask(s.asks, "close") };
  s = { ...s, prev: keys };
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down) {
    const r = rowAt(p.y);
    if (onClose(p.x, p.y, W)) s = { ...s, asks: ask(s.asks, "close") };
    else if (r >= 0 && r < ITEMS.length) {
      // the sheet closes first, so a sheet it opens takes the keys
      const [, what, after] = ITEMS[r];
      s = { ...s, asks: ask(after ? ask(s.asks, after) : s.asks, what) };
    }
  }
  return down === s.down ? s : { ...s, down };
};

export default function Settings() {
  const s = useGameState<S>({ busy: false, asks: NO_ASKS, prev: [], down: false }, step);
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ do: s.asks.list })}>
      <Frame title="void">
        {ITEMS.map(([label, what]) => (
          <Row key={what}>
            <div style={{ color: what === "stop" && !s.busy ? C.faint : C.ink }}>{label}</div>
          </Row>
        ))}
      </Frame>
    </AbsoluteFill>
  );
}
