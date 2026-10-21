// Audio (apps/void/shell.feature: Terminals): what sounds on the stage now, each source as the page has it (void:sounds: a
// timeline's <Audio> and <Video>, a game's cues; not the mastered track a timeline plays), with its file, its gain in
// dB and the bus it goes to. It reads; mute, solo and meters need the mixer to take them (a gain override per source,
// a level tap: apps/void/shell.feature P2).
import { AbsoluteFill } from "@raea/motion";
import { useSounds } from "@raea/terminal";

const HEAD = 34, ROW = 28, LEFT = 14, BAR = 120;

const db = (v: number) => (v > 0 ? 20 * Math.log10(v) : -Infinity);

export default function Audio({ width }: { width: number; height: number }) {
  const sounds = useSounds();
  return (
    <AbsoluteFill style={{ background: "#0e0e10", color: "#c9c9d1", fontFamily: "Inter", fontSize: 12 }}>
      <div style={{ position: "absolute", left: LEFT, top: 0, height: HEAD, display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{ color: "#e8e8ea" }}>Sounding now</span>
        <span style={{ color: "#6b6b74" }}>{sounds.length ? `${sounds.length} source${sounds.length > 1 ? "s" : ""}` : "nothing sounds on this frame"}</span>
      </div>
      {sounds.map((s, i) => {
        const d = db(s.volume);
        // the bar: -60 dB to +6 dB across it
        const fill = Math.max(0, Math.min(1, (d + 60) / 66));
        return (
          <div key={s.key + i} style={{ position: "absolute", left: LEFT, right: LEFT, top: HEAD + i * ROW, height: ROW - 4, borderRadius: 6, background: "#141418", display: "flex", alignItems: "center", gap: 10, padding: "0 10px", boxSizing: "border-box", whiteSpace: "nowrap", overflow: "hidden" }}>
            <span style={{ color: "#e8e8ea", flexShrink: 1, overflow: "hidden", minWidth: 60 }}>{s.file ?? s.key}</span>
            <span style={{ color: "#6b6b74", flexGrow: 1, overflow: "hidden" }}>{s.bus ?? "master"}</span>
            <div style={{ width: Math.min(BAR, width / 4), height: 6, borderRadius: 3, background: "#24242a", flexShrink: 0 }}>
              <div style={{ width: `${fill * 100}%`, height: 6, borderRadius: 3, background: "#2f9e6a" }} />
            </div>
            <span style={{ width: 54, textAlign: "right", color: "#8b8b94", fontVariantNumeric: "tabular-nums", flexShrink: 0 }}>{Number.isFinite(d) ? `${d.toFixed(1)} dB` : "−∞ dB"}</span>
          </div>
        );
      })}
    </AbsoluteFill>
  );
}
