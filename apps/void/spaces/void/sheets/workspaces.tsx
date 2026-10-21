// Which folder void works in (void:workspaces): the recent ones, newest first (a press opens one), then Open folder… and
// New workspace…. It covers the stage at launch when there's nothing to reopen; × or Escape goes back to the open one.
import { AbsoluteFill, useGameState, voidIn } from "@raea/motion";
import type { FrameInput } from "@raea/motion";
import { C, Frame, NO_ASKS, Row, ask, onClose, rowAt, typed, type Asks } from "./sheet";

type Recent = { path: string; name: string; ago: string; current: boolean };
type S = { open: boolean; items: Recent[]; asks: Asks; prev: string[]; down: boolean };
const W = 460, SHOWN = 8;
const MORE: [string, string][] = [["Open folder…", "open-folder"], ["New workspace…", "new-workspace"]];

const step = (s: S, input: FrameInput): S => {
  const got = voidIn<{ open: boolean; items: Recent[] }>(input, "workspaces");
  if (got) s = { ...s, ...got };
  const items = s.items.slice(0, SHOWN);
  const { keys, down: pressed } = typed(s.prev, input);
  if (pressed.includes("Escape") && s.open) s = { ...s, asks: ask(s.asks, "close") };
  s = { ...s, prev: keys };
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down) {
    const r = rowAt(p.y);
    if (onClose(p.x, p.y, W) && s.open) s = { ...s, asks: ask(s.asks, "close") };
    else if (r >= 0 && r < items.length) s = { ...s, asks: ask(ask(s.asks, "close"), "open-workspace", items[r].path) };
    else if (r >= items.length && r < items.length + MORE.length) s = { ...s, asks: ask(ask(s.asks, "close"), MORE[r - items.length][1]) };
  }
  return down === s.down ? s : { ...s, down };
};

export default function Workspaces() {
  const s = useGameState<S>({ open: false, items: [], asks: NO_ASKS, prev: [], down: false }, step);
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ do: s.asks.list })}>
      <Frame title="Workspaces" count={s.items.length} foot="A workspace is a folder: its objects, spaces and chats live in it">
        {s.items.slice(0, SHOWN).map((r) => (
          <Row key={r.path} current={r.current}>
            <div style={{ flex: 1, overflow: "hidden", whiteSpace: "nowrap" }}>{r.name}</div>
            <div style={{ color: C.faint, fontSize: 12 }}>{r.ago}</div>
          </Row>
        ))}
        {MORE.map(([label]) => (
          <Row key={label}>
            <div style={{ color: C.dim }}>{label}</div>
          </Row>
        ))}
      </Frame>
    </AbsoluteFill>
  );
}
