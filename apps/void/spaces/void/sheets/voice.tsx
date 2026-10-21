// The voice (void:voice): listening on or off, and which voice speaks. × or Escape closes.
import { AbsoluteFill, useGameState, voidIn } from "@raea/motion";
import type { FrameInput } from "@raea/motion";
import { C, Frame, NO_ASKS, Row, ask, onClose, rowAt, typed, type Asks } from "./sheet";

type V = { voices: string[]; voice: string; listening: boolean; ready: boolean; brain: string };
type S = { v: V; asks: Asks; prev: string[]; down: boolean };
const W = 360;

const step = (s: S, input: FrameInput): S => {
  const got = voidIn<V>(input, "voice");
  if (got) s = { ...s, v: got };
  const { keys, down: pressed } = typed(s.prev, input);
  if (pressed.includes("Escape")) s = { ...s, asks: ask(s.asks, "close") };
  s = { ...s, prev: keys };
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down) {
    const r = rowAt(p.y);
    if (onClose(p.x, p.y, W)) s = { ...s, asks: ask(s.asks, "close") };
    else if (r === 0) s = { ...s, asks: ask(s.asks, "listen") };
    else if (r >= 1 && r - 1 < s.v.voices.length) s = { ...s, asks: ask(s.asks, "set-voice", s.v.voices[r - 1]) };
  }
  return down === s.down ? s : { ...s, down };
};

export default function Voice() {
  const s = useGameState<S>({ v: { voices: [], voice: "", listening: false, ready: false, brain: "" }, asks: NO_ASKS, prev: [], down: false }, step);
  const { voices, voice, listening, ready, brain } = s.v;
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ do: s.asks.list })}>
      <Frame title="Voice" foot={`${ready ? "Voice ready" : "Voice loading"}${brain ? ` · Brain · ${brain}` : ""}`}>
        <Row>
          <div style={{ flex: 1 }}>Listen</div>
          <div style={{ width: 34, height: 20, borderRadius: 10, background: listening ? "#22d3ee" : C.line, position: "relative" }}>
            <div style={{ position: "absolute", top: 2, left: listening ? 16 : 2, width: 16, height: 16, borderRadius: 8, background: "#fff" }} />
          </div>
        </Row>
        {voices.map((v) => (
          <Row key={v} current={v === voice}>
            <div style={{ flex: 1, color: v === voice ? C.ink : C.dim }}>{v}</div>
            {v === voice ? <div style={{ color: C.dim, fontSize: 12 }}>speaking with this</div> : null}
          </Row>
        ))}
      </Frame>
    </AbsoluteFill>
  );
}
