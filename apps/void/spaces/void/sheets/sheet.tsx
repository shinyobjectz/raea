// What void's sheets share: the frame (a title, a count, + and ×), rows to press, Escape to close, and the asks a sheet
// makes of void's chrome, published as a list of numbered asks ({ do: [{ n, what, arg }] }) that void runs once each.
import { keyEdges } from "@raea/motion";
import type { FrameInput } from "@raea/motion";

export type Ask = { n: number; what: string; arg?: string };
export type Asks = { n: number; list: Ask[] };
export const NO_ASKS: Asks = { n: 0, list: [] };
export const ask = (a: Asks, what: string, arg = ""): Asks => ({ n: a.n + 1, list: [...a.list, { n: a.n + 1, what, arg }].slice(-8) });

export const HEAD = 46, ROW = 34, PAD = 8;
/** The row under y (rows start under the head), or -1. */
export const rowAt = (y: number) => (y < HEAD + PAD ? -1 : Math.floor((y - HEAD - PAD) / ROW));
/** Whether a press at (x, y) is on the × (the head's right end) or the + (left of it), the sheet `width` wide. */
export const onClose = (x: number, y: number, width: number) => y < HEAD && x > width - 40;
export const onPlus = (x: number, y: number, width: number) => y < HEAD && x > width - 76 && x <= width - 40;

/** The keys typed this frame (void's own keys aren't typing), and whether one was newly pressed. */
export function typed(prev: string[], input: FrameInput): { keys: string[]; down: string[] } {
  const keys = (input.keys ?? []).filter((k) => !k.startsWith("void:") && !k.startsWith("action:") && !k.startsWith("space:"));
  const down = keyEdges(prev, keys).filter((e) => e.startsWith("keydown:")).map((e) => e.slice(8));
  return { keys, down };
}

export const C = { ground: "#141417", line: "#24242b", ink: "#e8e8ea", dim: "#8b8b94", faint: "#5b5b66", on: "#2b4bff", row: "#1c1c21" };

export function Frame({ title, count, plus, children, foot }: { title: string; count?: number | string; plus?: boolean; children?: any; foot?: any }) {
  return (
    <div style={{ position: "absolute", inset: 0, background: C.ground, borderRadius: 14, border: `1px solid ${C.line}`, color: C.ink, fontFamily: "Inter", fontSize: 13, display: "flex", flexDirection: "column", overflow: "hidden" }}>
      <div style={{ height: HEAD, flexShrink: 0, display: "flex", alignItems: "center", padding: "0 6px 0 16px", borderBottom: `1px solid ${C.line}` }}>
        <div style={{ fontWeight: 600 }}>{title}</div>
        {count !== undefined ? <div style={{ color: C.faint, marginLeft: 8 }}>{count}</div> : null}
        <div style={{ flex: 1 }} />
        {plus ? <div style={{ width: 34, height: 30, display: "flex", alignItems: "center", justifyContent: "center", fontSize: 18, color: C.dim }}>+</div> : <div style={{ width: 34 }} />}
        <div style={{ width: 34, height: 30, display: "flex", alignItems: "center", justifyContent: "center", fontSize: 16, color: C.dim }}>×</div>
      </div>
      <div style={{ flex: 1, padding: `${PAD}px 8px`, overflow: "hidden" }}>{children}</div>
      {foot ? <div style={{ flexShrink: 0, borderTop: `1px solid ${C.line}`, padding: "10px 14px", color: C.dim, fontSize: 12 }}>{foot}</div> : null}
    </div>
  );
}

export function Row({ on, current, children }: { on?: boolean; current?: boolean; children: any }) {
  return (
    <div style={{ height: ROW, display: "flex", alignItems: "center", gap: 10, padding: "0 10px", borderRadius: 8, background: on ? C.on : current ? C.row : "transparent" }}>
      {children}
    </div>
  );
}
