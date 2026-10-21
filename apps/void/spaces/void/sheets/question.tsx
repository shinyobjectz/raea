// The agent's question (void:question, `raea ask`): each option with what it means; a press answers (several, then
// Answer, when it asks for several), or type your own words and return. Open while the agent asks; answering closes it.
import { AbsoluteFill, useGameState, voidIn } from "@raea/motion";
import type { FrameInput } from "@raea/motion";
import { C, NO_ASKS, ask, typed, type Asks } from "./sheet";

type Q = { id: string; text: string; multi?: boolean; options: { label: string; meaning?: string }[] } | null;
type S = { q: Q; chosen: string[]; other: string; asks: Asks; prev: string[]; down: boolean };
const TOP = 88, OPT = 52, W = 560;

const step = (s: S, input: FrameInput): S => {
  const got = voidIn<Q>(input, "question");
  if (got !== undefined && got?.id !== s.q?.id) s = { ...s, q: got, chosen: [], other: "" };
  const q = s.q;
  const { keys, down: pressed } = typed(s.prev, input);
  let other = s.other;
  let asks = s.asks;
  for (const k of pressed) {
    if (k === "Enter" && other.trim()) { asks = ask(asks, "answer", `other: ${other.trim()}`); other = ""; }
    else if (k === "Backspace") other = other.slice(0, -1);
    else if (k.length === 1) other += k;
  }
  s = { ...s, other, asks, prev: keys };
  const p = input.pointer;
  const down = !!p?.down;
  if (q && p && down && !s.down && p.y >= TOP) {
    const i = Math.floor((p.y - TOP) / OPT);
    const opt = q.options[i];
    if (opt && !q.multi) s = { ...s, asks: ask(s.asks, "answer", opt.label) };
    else if (opt) s = { ...s, chosen: s.chosen.includes(opt.label) ? s.chosen.filter((c) => c !== opt.label) : [...s.chosen, opt.label] };
    else if (q.multi && i === q.options.length && s.chosen.length) s = { ...s, asks: ask(s.asks, "answer", s.chosen.join(", ")), chosen: [] };
  }
  return down === s.down ? s : { ...s, down };
};

export default function Question() {
  const s = useGameState<S>({ q: null, chosen: [], other: "", asks: NO_ASKS, prev: [], down: false }, step);
  const q = s.q;
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ do: s.asks.list })} style={{ background: C.ground, borderRadius: 14, border: `1px solid ${C.line}`, color: C.ink, fontFamily: "Inter", fontSize: 14, overflow: "hidden" }}>
      <div style={{ height: TOP, boxSizing: "border-box", padding: "16px 18px", display: "flex", gap: 10 }}>
        <div style={{ color: "#4f7bff", fontSize: 12, fontWeight: 600, paddingTop: 2 }}>{q?.id ?? ""}</div>
        <div style={{ fontWeight: 600, lineHeight: 1.4 }}>{q?.text ?? "No question"}</div>
      </div>
      {(q?.options ?? []).map((o, i) => (
        <div key={i} style={{ height: OPT - 6, margin: "0 12px 6px", boxSizing: "border-box", padding: "0 12px", display: "flex", alignItems: "center", gap: 12, borderRadius: 9, background: s.chosen.includes(o.label) ? C.on : C.row, width: W - 24 }}>
          <div style={{ color: C.faint, width: 14 }}>{i + 1}</div>
          <div style={{ fontWeight: 600 }}>{o.label}</div>
          {o.meaning ? <div style={{ color: C.dim, fontSize: 12, overflow: "hidden", whiteSpace: "nowrap" }}>{o.meaning}</div> : null}
        </div>
      ))}
      {q?.multi ? (
        <div style={{ height: OPT - 6, margin: "0 12px 6px", display: "flex", alignItems: "center", justifyContent: "center", borderRadius: 9, background: s.chosen.length ? "#26262b" : "transparent", color: s.chosen.length ? C.ink : C.faint }}>Answer</div>
      ) : null}
      <div style={{ margin: "6px 18px", color: s.other ? C.ink : C.faint }}>{s.other || "Other: type your own words, then return"}</div>
    </AbsoluteFill>
  );
}
