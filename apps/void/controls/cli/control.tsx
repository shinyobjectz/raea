// The cli (apps/void/shell.feature: The cli terminal): the person's login shell in the project's folder, drawn from the
// screen void hands in (void:pty: rows of styled runs, each from a column, and the cursor; apps/void/src/pty.rs). Keys don't
// come here: void sends them to the shell as a terminal's bytes, and the wheel scrolls its screen back.
import { AbsoluteFill } from "@raea/motion";
import { usePty } from "@raea/terminal";

// a cell and the padding, in points: as pty.rs sizes the shell (CELL_W, CELL_H, PAD)
const CW = 7.23, CH = 16, PAD = 8;

export default function Cli(_: { width: number; height: number }) {
  const screen = usePty();
  return (
    <AbsoluteFill style={{ background: "#0e0e10", color: "#e2e4eb", fontFamily: "Menlo", fontSize: 12, lineHeight: `${CH}px` }}>
      {screen?.lines.map((row, y) =>
        row.map(([x, text, fg, bg, f], k) => (
          <span key={`${y}.${k}`} style={{ position: "absolute", left: PAD + x * CW, top: PAD + y * CH, height: CH, whiteSpace: "pre", color: fg, background: bg ?? undefined, fontWeight: f & 1 ? 700 : 400, opacity: f & 2 ? 0.6 : 1, fontStyle: f & 4 ? "italic" : "normal", textDecoration: f & 8 ? "underline" : "none" }}>
            {text}
          </span>
        )),
      )}
      {screen?.cursor ? <div style={{ position: "absolute", left: PAD + screen.cursor[0] * CW, top: PAD + screen.cursor[1] * CH, width: CW, height: CH, background: "#e2e4eb", opacity: 0.55 }} /> : null}
      {screen?.back ? <div style={{ position: "absolute", right: 12, bottom: 8, fontFamily: "Inter", fontSize: 11, color: "#4f7bff" }}>↑ {screen.back} lines back</div> : null}
      {screen?.exited ? <div style={{ position: "absolute", left: PAD, bottom: 8, fontFamily: "Inter", fontSize: 11, color: "#7a7e8c" }}>The shell exited. Close this tab, or + for a new one.</div> : null}
      {!screen ? <div style={{ position: "absolute", left: PAD, top: PAD, color: "#7a7e8c" }}>Starting your shell…</div> : null}
    </AbsoluteFill>
  );
}
