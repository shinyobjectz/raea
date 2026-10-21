// A terminal written as a panel: the conversation (void's transcript, read with voidIn) on a sheet of paper, and a line
// to type in. Typing is a fold over the keys pressed; return sends the line by publishing { send: { n, text } }.
import { AbsoluteFill, keyEdges, useGameState, voidIn } from "@raea/motion";
import type { FrameInput } from "@raea/motion";

type Said = { who: string; text: string; live?: boolean };
type S = { draft: string; n: number; sent: string; prev: string[] };

const step = (s: S, input: FrameInput): S => {
  const keys = (input.keys ?? []).filter((k) => !k.startsWith("void:") && !k.startsWith("action:"));
  let { draft, n, sent } = s;
  for (const e of keyEdges(s.prev, keys)) {
    if (!e.startsWith("keydown:")) continue;
    const k = e.slice(8);
    if (k === "Enter") {
      if (draft.trim()) {
        n += 1;
        sent = draft;
        draft = "";
      }
    } else if (k === "Backspace") draft = draft.slice(0, -1);
    else if (k.length === 1) draft += k;
  }
  return { draft, n, sent, prev: keys };
};

const INK = "#2a2620";
const who = (w: string) => (w === "you" ? "YOU" : w === "void" ? "VOID" : w === "tool" ? "··" : "");

export default function Terminal(_: { width: number; height: number }) {
  const s = useGameState<S>({ draft: "", n: 0, sent: "", prev: [] }, step);
  return (
    <AbsoluteFill
      data-void-publish={JSON.stringify({ send: { n: s.n, text: s.sent } })}
      style={{ background: "#f3eee2", color: INK, fontFamily: "Courier New, Courier, monospace", fontSize: 14, padding: "18px 20px", display: "flex", flexDirection: "column" }}
    >
      <Lines />
      <div style={{ borderTop: `1px solid ${INK}33`, paddingTop: 10, marginTop: 8, display: "flex", gap: 8 }}>
        <span style={{ fontWeight: 700 }}>›</span>
        <span>{s.draft}</span>
        <span style={{ width: 8, height: 16, background: INK, opacity: 0.8 }} />
      </div>
    </AbsoluteFill>
  );
}

// the conversation as void tells it, newest at the bottom (what doesn't fit scrolls off the top)
function Lines() {
  // void hands the transcript in on the frame it changes; the panel keeps it
  const lines = useGameState<Said[]>([], (s, input) => voidIn<Said[]>(input, "transcript") ?? s);
  return (
    <div style={{ flex: 1, overflow: "hidden", display: "flex", flexDirection: "column", justifyContent: "flex-end", gap: 8 }}>
      {lines.filter((l) => l.text.trim()).slice(-24).map((l, i) => (
        <div key={i} style={{ display: "flex", gap: 10, opacity: l.live ? 0.55 : l.who === "tool" || l.who === "note" ? 0.6 : 1 }}>
          <span style={{ width: 34, flexShrink: 0, fontWeight: 700, fontSize: 11, paddingTop: 2 }}>{who(l.who)}</span>
          <span style={{ whiteSpace: "pre-wrap" }}>{l.text}</span>
        </div>
      ))}
    </div>
  );
}
