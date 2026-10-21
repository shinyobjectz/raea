// void's terminal, written as a panel: a header row (the mic, the chat's name, stop while the agent works), the
// conversation (void's transcript, voidIn), and the prompt. Typing is a fold over the keys pressed; return sends the line
// ({ send }), Cmd-V pastes (void hands the clipboard in as void:paste), up and down walk the lines sent before; the
// header's buttons ask void's chrome ({ do }). Make it anything: fork the space and change this file.
import { AbsoluteFill, applyActions, keyEdges, useGameState, voidIn } from "@raea/motion";
import type { FrameInput, PanelActions } from "@raea/motion";

type Said = { who: string; text: string; live?: boolean };
type State = { busy: boolean; speaking: boolean; listening: boolean; chat: string };
type Do = { n: number; what: string; x: number; y: number };
type S = { draft: string; send: { n: number; text: string }; do: Do; prev: string[]; down: boolean; state: State; sent: string[]; back: number };

const HEAD = 36, MIC = [10, 40], CHAT = [48, 248], STOP = [256, 288];
const ask = (s: S, what: string, x: number, y: number): S => ({ ...s, do: { n: s.do.n + 1, what, x, y } });
const within = (x: number, [a, b]: number[]) => x >= a && x < b;

export const actions: PanelActions<S> = {
  stop: { about: "Stop the agent's turn and the voice", run: (s) => ask(s, "stop", 0, 0) },
  listen: { about: "Turn the voice (the mic) on or off", run: (s) => ask(s, "listen", 0, 0) },
  chats: { about: "List this project's chats", run: (s) => ask(s, "chats", CHAT[0], HEAD) },
};

const step = (state: S, input: FrameInput): S => {
  let s = applyActions(actions, state, input);
  const st = voidIn<State>(input, "state");
  if (st) s = { ...s, state: st };
  // the prompt: keys newly down, typed (void's own keys and actions aren't typing)
  const keys = (input.keys ?? []).filter((k) => !k.startsWith("void:") && !k.startsWith("action:") && !k.startsWith("space:"));
  let { draft, send, sent, back } = s;
  const pasted = voidIn<string>(input, "paste");
  if (pasted) draft += pasted.replace(/\r?\n/g, " ");
  for (const e of keyEdges(s.prev, keys)) {
    if (!e.startsWith("keydown:")) continue;
    const k = e.slice(8);
    if (k === "Enter") {
      if (draft.trim()) {
        send = { n: send.n + 1, text: draft };
        sent = [...sent, draft].slice(-50);
        draft = "";
      }
      back = 0;
    } else if (k === "ArrowUp" || k === "ArrowDown") {
      // back counts lines up from the newest; 0 is the line being written (left empty)
      const to = Math.max(0, Math.min(sent.length, back + (k === "ArrowUp" ? 1 : -1)));
      if (to !== back) draft = to ? sent[sent.length - to] : "";
      back = to;
    } else if (k === "Backspace") draft = draft.slice(0, -1);
    else if (k.length === 1) draft += k;
  }
  s = { ...s, draft, send, sent, back, prev: keys };
  // the header's buttons
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down && p.y < HEAD) {
    if (within(p.x, MIC)) s = actions.listen.run(s, {});
    else if (within(p.x, CHAT)) s = actions.chats.run(s, {});
    else if (within(p.x, STOP) && s.state.busy) s = actions.stop.run(s, {});
  }
  return down === s.down ? s : { ...s, down };
};

const COLOR: Record<string, string> = { you: "#ffffff", void: "#c9c9d1", tool: "#6b6b74", note: "#7f95c9" };

export default function Terminal(_: { width: number; height: number }) {
  const s = useGameState<S>({ draft: "", send: { n: 0, text: "" }, do: { n: 0, what: "", x: 0, y: 0 }, prev: [], down: false, state: { busy: false, speaking: false, listening: false, chat: "void" }, sent: [], back: 0 }, step);
  const { busy, speaking, listening, chat } = s.state;
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ send: s.send, do: s.do })} style={{ background: "#0e0e10", color: "#c9c9d1", fontFamily: "Menlo, monospace", fontSize: 13, display: "flex", flexDirection: "column" }}>
      <div style={{ height: HEAD, flexShrink: 0, display: "flex", alignItems: "center", borderBottom: "1px solid #1f1f25", fontFamily: "Inter", fontSize: 12 }}>
        <div style={{ position: "absolute", left: MIC[0], width: MIC[1] - MIC[0], height: 24, display: "flex", alignItems: "center", justifyContent: "center", gap: 2 }}>
          {[0.5, 0.9, 0.7, 0.4].map((h, i) => (
            <div key={i} style={{ width: 3, height: 6 + (listening || speaking ? h * 12 : 2), borderRadius: 2, background: speaking ? "#a78bfa" : listening ? "#22d3ee" : "#4a4a52" }} />
          ))}
        </div>
        <div style={{ position: "absolute", left: CHAT[0], width: CHAT[1] - CHAT[0], color: "#e8e8ea", overflow: "hidden", whiteSpace: "nowrap" }}>
          {chat} <span style={{ color: "#6b6b74" }}>▾</span>
          {speaking ? <span style={{ color: "#a78bfa", marginLeft: 8 }}>speaking</span> : null}
        </div>
        {busy ? (
          <div style={{ position: "absolute", left: STOP[0], width: STOP[1] - STOP[0], height: 24, borderRadius: 6, background: "#2a1416", display: "flex", alignItems: "center", justifyContent: "center" }}>
            <div style={{ width: 9, height: 9, borderRadius: 2, background: "#ff6b6b" }} />
          </div>
        ) : null}
      </div>
      <Lines />
      <div style={{ flexShrink: 0, padding: "8px 14px 12px", display: "flex", gap: 8, color: "#ffffff" }}>
        <span style={{ color: "#4f7bff" }}>›</span>
        <span style={{ whiteSpace: "pre" }}>{s.draft}</span>
        <span style={{ width: 8, height: 16, background: "#c9c9d1", opacity: 0.8 }} />
      </div>
    </AbsoluteFill>
  );
}

// the conversation as void tells it, newest at the bottom (what doesn't fit goes off the top); the wheel scrolls back
// through it a line per 18 points, and back down to the newest
type Shown = { lines: Said[]; back: number; rest: number };
const LINE = 18;
function Lines() {
  const s = useGameState<Shown>({ lines: [], back: 0, rest: 0 }, (s, input) => {
    const lines = voidIn<Said[]>(input, "transcript") ?? s.lines;
    const dy = voidIn<number>(input, "wheel");
    if (!dy) return lines === s.lines ? s : { ...s, lines };
    const total = s.rest - dy;
    const n = Math.trunc(total / LINE);
    const said = lines.filter((l) => l.text.trim()).length;
    return { lines, back: Math.max(0, Math.min(said - 1, s.back + n)), rest: total - n * LINE };
  });
  const said = s.lines.filter((l) => l.text.trim());
  const shown = said.slice(0, said.length - s.back).slice(-30);
  return (
    <div style={{ flex: 1, overflow: "hidden", display: "flex", flexDirection: "column", justifyContent: "flex-end", gap: 6, padding: "8px 14px 0" }}>
      {shown.map((l, i) => (
        <div key={i} style={{ whiteSpace: "pre-wrap", color: COLOR[l.who] ?? "#c9c9d1", opacity: l.live ? 0.6 : 1 }}>
          {l.who === "you" ? "› " : ""}{l.text}
        </div>
      ))}
      {s.back > 0 ? <div style={{ color: "#4f7bff", fontFamily: "Inter", fontSize: 11 }}>↓ {s.back} newer</div> : null}
    </div>
  );
}
