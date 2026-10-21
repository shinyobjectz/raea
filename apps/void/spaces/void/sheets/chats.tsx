// The project's chats (void:chats): a press opens one, + starts a new one, × or Escape closes.
import { AbsoluteFill, useGameState, voidIn } from "@raea/motion";
import type { FrameInput } from "@raea/motion";
import { C, Frame, NO_ASKS, Row, ask, onClose, onPlus, rowAt, typed, type Asks } from "./sheet";

type Chat = { id: string; name: string; ago: string; current: boolean };
type S = { chats: Chat[]; asks: Asks; prev: string[]; down: boolean };
const W = 380;

const step = (s: S, input: FrameInput): S => {
  const got = voidIn<Chat[]>(input, "chats");
  if (got) s = { ...s, chats: got };
  const { keys, down: pressed } = typed(s.prev, input);
  if (pressed.includes("Escape")) s = { ...s, asks: ask(s.asks, "close") };
  s = { ...s, prev: keys };
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down) {
    const r = rowAt(p.y);
    if (onClose(p.x, p.y, W)) s = { ...s, asks: ask(s.asks, "close") };
    else if (onPlus(p.x, p.y, W)) s = { ...s, asks: ask(ask(s.asks, "new-chat"), "close") };
    else if (r >= 0 && r < s.chats.length) s = { ...s, asks: ask(ask(s.asks, "open-chat", s.chats[r].id), "close") };
  }
  return down === s.down ? s : { ...s, down };
};

export default function Chats() {
  const s = useGameState<S>({ chats: [], asks: NO_ASKS, prev: [], down: false }, step);
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ do: s.asks.list })}>
      <Frame title="Chats" count={s.chats.length} plus>
        {s.chats.length === 0 ? <div style={{ color: C.faint, padding: 10 }}>No chats yet: + starts one</div> : null}
        {s.chats.map((c) => (
          <Row key={c.id} current={c.current}>
            <div style={{ flex: 1, overflow: "hidden", whiteSpace: "nowrap" }}>{c.name}</div>
            <div style={{ color: C.faint, fontSize: 12 }}>{c.ago}</div>
          </Row>
        ))}
      </Frame>
    </AbsoluteFill>
  );
}
