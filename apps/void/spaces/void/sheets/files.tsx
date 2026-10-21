// The object's files (void:files): a press picks one; the row of buttons opens it, shows it in Finder, asks void about
// it, or brings in more files. × or Escape closes.
import { AbsoluteFill, useGameState, voidIn } from "@raea/motion";
import type { FrameInput } from "@raea/motion";
import { C, Frame, NO_ASKS, Row, ask, onClose, rowAt, typed, type Asks } from "./sheet";

type F = { rel: string; name: string; kind: string; bytes: number };
type S = { files: F[]; picked: number; asks: Asks; prev: string[]; down: boolean };
const W = 460, H = 520, BAR = 52, BTN = 104;
const BUTTONS: [string, string][] = [["Open", "file-open"], ["Finder", "file-reveal"], ["Ask void", "file-ask"], ["Bring in…", "import"]];

const size = (b: number) => (b > 1e9 ? `${(b / 1e9).toFixed(1)} GB` : b > 1e6 ? `${(b / 1e6).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1e3))} KB`);

const step = (s: S, input: FrameInput): S => {
  const got = voidIn<F[]>(input, "files");
  if (got) s = { ...s, files: got, picked: Math.min(s.picked, got.length - 1) };
  const { keys, down: pressed } = typed(s.prev, input);
  if (pressed.includes("Escape")) s = { ...s, asks: ask(s.asks, "close") };
  s = { ...s, prev: keys };
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down) {
    if (onClose(p.x, p.y, W)) s = { ...s, asks: ask(s.asks, "close") };
    else if (p.y > H - BAR) {
      const b = BUTTONS[Math.floor((p.x - 10) / (BTN + 4))];
      const f = s.files[s.picked];
      if (b && b[1] === "import") s = { ...s, asks: ask(s.asks, "import") };
      else if (b && f) s = { ...s, asks: ask(s.asks, b[1], f.rel) };
    } else {
      const r = rowAt(p.y);
      if (r >= 0 && r < s.files.length) s = { ...s, picked: r };
    }
  }
  return down === s.down ? s : { ...s, down };
};

export default function Files() {
  const s = useGameState<S>({ files: [], picked: 0, asks: NO_ASKS, prev: [], down: false }, step);
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ do: s.asks.list })}>
      <Frame title="Files" count={s.files.length}>
        {s.files.length === 0 ? <div style={{ color: C.faint, padding: 10 }}>No files in this object yet: Bring in…</div> : null}
        {s.files.slice(0, 11).map((f, i) => (
          <Row key={f.rel} on={i === s.picked}>
            <div style={{ flex: 1, overflow: "hidden", whiteSpace: "nowrap" }}>{f.rel}</div>
            <div style={{ color: i === s.picked ? C.ink : C.faint, fontSize: 12 }}>{`${f.kind} · ${size(f.bytes)}`}</div>
          </Row>
        ))}
      </Frame>
      <div style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: BAR, display: "flex", alignItems: "center", gap: 4, paddingLeft: 10, borderTop: `1px solid ${C.line}`, background: C.ground, borderRadius: "0 0 14px 14px", fontFamily: "Inter", fontSize: 12, color: C.ink }}>
        {BUTTONS.map(([label]) => (
          <div key={label} style={{ width: BTN, height: 32, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: 8, background: C.row }}>{label}</div>
        ))}
      </div>
    </AbsoluteFill>
  );
}
