// The workspace's objects (void:objects): a press opens one, + makes a new one, × or Escape closes.
import { AbsoluteFill, useGameState, voidIn } from "@raea/motion";
import type { FrameInput } from "@raea/motion";
import { C, Frame, NO_ASKS, Row, ask, onClose, onPlus, rowAt, typed, type Asks } from "./sheet";

type Item = { slug: string; name: string; kind: string; state?: string; current: boolean };
type S = { list: { workspace: string; items: Item[] }; asks: Asks; prev: string[]; down: boolean };
const W = 420;

const step = (s: S, input: FrameInput): S => {
  const got = voidIn<S["list"]>(input, "objects");
  if (got) s = { ...s, list: got };
  const { keys, down: pressed } = typed(s.prev, input);
  if (pressed.includes("Escape")) s = { ...s, asks: ask(s.asks, "close") };
  s = { ...s, prev: keys };
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down) {
    const r = rowAt(p.y);
    if (onClose(p.x, p.y, W)) s = { ...s, asks: ask(s.asks, "close") };
    else if (onPlus(p.x, p.y, W)) s = { ...s, asks: ask(ask(s.asks, "new-object", "Untitled"), "close") };
    else if (r >= 0 && r < s.list.items.length) s = { ...s, asks: ask(ask(s.asks, "open-object", s.list.items[r].slug), "close") };
  }
  return down === s.down ? s : { ...s, down };
};

export default function Objects() {
  const s = useGameState<S>({ list: { workspace: "Objects", items: [] }, asks: NO_ASKS, prev: [], down: false }, step);
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ do: s.asks.list })}>
      <Frame title={s.list.workspace} count={s.list.items.length} plus foot="A press opens one; + makes a new one">
        {s.list.items.length === 0 ? <div style={{ color: C.faint, padding: 10 }}>No objects yet: + makes one</div> : null}
        {s.list.items.map((o) => (
          <Row key={o.slug} current={o.current}>
            <div style={{ flex: 1, overflow: "hidden", whiteSpace: "nowrap" }}>{o.name}</div>
            <div style={{ color: C.dim, fontSize: 12 }}>{o.kind}</div>
            {o.state ? <div style={{ color: C.faint, fontSize: 12, width: 70, textAlign: "right" }}>{o.state}</div> : null}
          </Row>
        ))}
      </Frame>
    </AbsoluteFill>
  );
}
