// void's chat (apps/void/shell.feature: Chat is the agent, and it comes from the right): the agent's thread, the same in
// every space. Its top row: the chat's name, which lists this workspace's chats (a click on one switches to it, + starts
// a new one), the mic, stop while the agent works, and ⋯ for void's settings. Then the conversation (void's transcript,
// voidIn), and the prompt: typing is a fold over the keys pressed, return sends the line ({ send }), Cmd-V pastes (void
// hands the clipboard in as void:paste), up and down walk the lines sent before.
import { AbsoluteFill, applyActions, keyEdges, useGameState, voidIn } from "@raea/motion";
import type { FrameInput, PanelActions } from "@raea/motion";

type Said = { who: string; text: string; live?: boolean };
type State = { busy: boolean; speaking: boolean; listening: boolean; chat: string };
type Chat = { id: string; name: string; ago: string; current: boolean };
type Do = { n: number; what: string; x: number; y: number; arg: string };
type S = { draft: string; send: { n: number; text: string }; do: Do; prev: string[]; down: boolean; state: State; chats: Chat[]; list: boolean; sent: string[]; back: number };

const HEAD = 40, ROW = 30, MIC = 34, BTN = 30;
const ask = (s: S, what: string, arg = "", x = 0, y = HEAD): S => ({ ...s, do: { n: s.do.n + 1, what, x, y, arg } });
const within = (x: number, [a, b]: number[]) => x >= a && x < b;

// the panel's width, as last laid out (the buttons sit at its right end)
let W = 380;
const menuAt = () => [W - 8 - BTN, W - 8];
const stopAt = () => [W - 8 - 2 * BTN - 4, W - 8 - BTN - 4];
const micAt = () => [W - 8 - 3 * BTN - 8, W - 8 - 2 * BTN - 8];

export const actions: PanelActions<S> = {
  stop: { about: "Stop the agent's turn and the voice", run: (s) => ask(s, "stop") },
  listen: { about: "Turn the voice (the mic) on or off", run: (s) => ask(s, "listen") },
  chats: { about: "Show or hide this workspace's chats", run: (s) => ({ ...s, list: !s.list }) },
  newChat: { about: "Start a new chat", run: (s) => ({ ...ask(s, "new-chat"), list: false }) },
  openChat: { about: "Switch to one of this workspace's chats, by name", args: { name: "string" }, run: (s, a) => {
    const c = s.chats.find((c) => c.name.toLowerCase() === String(a.name).toLowerCase() || c.id === String(a.name));
    return c ? { ...ask(s, "open-chat", c.id), list: false } : s;
  } },
};

const step = (state: S, input: FrameInput): S => {
  let s = applyActions(actions, state, input);
  const st = voidIn<State>(input, "state");
  if (st) s = { ...s, state: st };
  const chats = voidIn<Chat[]>(input, "chats");
  if (chats) s = { ...s, chats };
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
    } else if (k === "Escape") s = { ...s, list: false };
    else if (k === "Backspace") draft = draft.slice(0, -1);
    else if (k.length === 1) draft += k;
  }
  s = { ...s, draft, send, sent, back, prev: keys };
  // the top row, and the chats listed under it while open
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down) {
    if (p.y < HEAD) {
      if (within(p.x, menuAt())) s = ask(s, "settings", "", p.x, HEAD);
      else if (within(p.x, stopAt()) && s.state.busy) s = actions.stop.run(s, {});
      else if (within(p.x, micAt())) s = actions.listen.run(s, {});
      else if (p.x < micAt()[0]) s = actions.chats.run(s, {});
    } else if (s.list) {
      const i = Math.floor((p.y - HEAD) / ROW);
      if (i === 0) s = actions.newChat.run(s, {});
      else if (s.chats[i - 1]) s = { ...ask(s, "open-chat", s.chats[i - 1].id), list: false };
      else s = { ...s, list: false };
    }
  }
  return down === s.down ? s : { ...s, down };
};

const COLOR: Record<string, string> = { you: "#ffffff", void: "#c9c9d1", tool: "#6b6b74", note: "#7f95c9" };

export default function Chat({ width }: { width: number; height: number }) {
  W = width;
  const s = useGameState<S>({ draft: "", send: { n: 0, text: "" }, do: { n: 0, what: "", x: 0, y: 0, arg: "" }, prev: [], down: false, state: { busy: false, speaking: false, listening: false, chat: "void" }, chats: [], list: false, sent: [], back: 0 }, step);
  const { busy, speaking, listening, chat } = s.state;
  const box = (x: number[]) => ({ position: "absolute" as const, left: x[0], top: 5, width: BTN, height: BTN, borderRadius: 7, display: "flex", alignItems: "center", justifyContent: "center" });
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ send: s.send, do: s.do })} style={{ background: "#0e0e10", borderLeft: "1px solid #1c1c21", color: "#c9c9d1", fontFamily: "Inter", fontSize: 13, display: "flex", flexDirection: "column" }}>
      <div style={{ height: HEAD, flexShrink: 0, borderBottom: "1px solid #1c1c21", fontSize: 12 }}>
        <div style={{ position: "absolute", left: 12, top: 0, height: HEAD, width: micAt()[0] - 20, display: "flex", alignItems: "center", gap: 6, color: "#e8e8ea", overflow: "hidden", whiteSpace: "nowrap" }}>
          <span style={{ fontWeight: 600 }}>{chat}</span>
          <span style={{ color: "#6b6b74" }}>{s.list ? "▴" : "▾"}</span>
          {speaking ? <span style={{ color: "#a78bfa" }}>speaking</span> : null}
        </div>
        <div style={{ ...box(micAt()), gap: 2 }}>
          {[0.5, 0.9, 0.7, 0.4].map((h, i) => (
            <div key={i} style={{ width: 3, height: 6 + (listening || speaking ? h * 12 : 2), borderRadius: 2, background: speaking ? "#a78bfa" : listening ? "#22d3ee" : "#4a4a52" }} />
          ))}
        </div>
        {busy ? (
          <div style={{ ...box(stopAt()), background: "#2a1416" }}>
            <div style={{ width: 9, height: 9, borderRadius: 2, background: "#ff6b6b" }} />
          </div>
        ) : null}
        <div style={{ ...box(menuAt()), color: "#8b8b94", fontSize: 16 }}>⋯</div>
      </div>
      {s.list ? (
        <div style={{ position: "absolute", left: 0, right: 0, top: HEAD, bottom: 0, background: "#0e0e10", zIndex: 2 }}>
          <div style={{ height: ROW, display: "flex", alignItems: "center", padding: "0 14px", color: "#4f7bff" }}>+ New chat</div>
          {s.chats.map((c) => (
            <div key={c.id} style={{ height: ROW, display: "flex", alignItems: "center", gap: 8, padding: "0 14px", background: c.current ? "#1a1a1f" : "transparent", color: c.current ? "#ffffff" : "#c9c9d1" }}>
              <span style={{ flex: 1, overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis" }}>{c.name}</span>
              <span style={{ color: "#6b6b74", fontSize: 11 }}>{c.ago}</span>
            </div>
          ))}
        </div>
      ) : null}
      <Lines />
      <div style={{ flexShrink: 0, margin: "8px 10px 10px", padding: "10px 12px", borderRadius: 10, background: "#16161a", border: "1px solid #24242a", display: "flex", gap: 8, color: "#ffffff", minHeight: 20 }}>
        <span style={{ whiteSpace: "pre-wrap", flex: 1 }}>
          {s.draft || <span style={{ color: "#5a5a62" }}>Ask the agent anything</span>}
        </span>
        <span style={{ width: 2, height: 18, background: "#c9c9d1", opacity: 0.8 }} />
      </div>
    </AbsoluteFill>
  );
}

// the conversation as void tells it, newest at the bottom (what doesn't fit goes off the top); the wheel scrolls back
// through it a line per 18 points, and back down to the newest
type Shown = { lines: Said[]; back: number; rest: number };
const LINE = 18;
const scroll = (s: Shown, input: FrameInput): Shown => {
  const lines = voidIn<Said[]>(input, "transcript") ?? s.lines;
  const dy = voidIn<number>(input, "wheel");
  if (!dy) return lines === s.lines ? s : { ...s, lines };
  const total = s.rest - dy;
  const n = Math.trunc(total / LINE);
  const said = lines.filter((l) => l.text.trim()).length;
  return { lines, back: Math.max(0, Math.min(said - 1, s.back + n)), rest: total - n * LINE };
};
function Lines() {
  const s = useGameState<Shown>({ lines: [], back: 0, rest: 0 }, scroll);
  const said = s.lines.filter((l) => l.text.trim());
  const shown = said.slice(0, said.length - s.back).slice(-30);
  return (
    <div style={{ flex: 1, overflow: "hidden", display: "flex", flexDirection: "column", justifyContent: "flex-end", gap: 8, padding: "10px 14px 0", lineHeight: 1.45 }}>
      {shown.map((l, i) => (
        <div key={i} style={{ whiteSpace: "pre-wrap", color: COLOR[l.who] ?? "#c9c9d1", opacity: l.live ? 0.6 : 1, fontSize: l.who === "tool" ? 12 : 13, ...(l.who === "you" ? { alignSelf: "flex-end", background: "#1d1d23", padding: "6px 10px", borderRadius: 10, maxWidth: "85%" } : {}) }}>
          {l.text}
        </div>
      ))}
      {s.back > 0 ? <div style={{ color: "#4f7bff", fontSize: 11 }}>↓ {s.back} newer</div> : null}
    </div>
  );
}
