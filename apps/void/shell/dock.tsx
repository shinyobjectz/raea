// void's terminal dock (apps/void/shell.feature: The dock): its bar of tabs, one per terminal open (a terminal.tsx laying out
// controllers, or a controller alone), + for the picker of terminals and controllers there are, ⇅ to move the dock
// round the stage's edges and × to hide it (as ⌃` does). The terminal shown is its controllers' panels, each laid over
// the dock under this bar in its part (space_ui.rs); with + open, the picker is drawn here instead.
import { AbsoluteFill, applyActions, useGameState, voidIn } from "@raea/motion";
import type { FrameInput, PanelActions } from "@raea/motion";

const BAR = 30, TAB = 132, GAP = 4, LEFT = 8, BTN = 24, ROW = 28, X = 18;

type Tab = { id: number; terminal: string; name: string };
type Pick = { slug: string; name: string; own: boolean; kind: "terminal" | "control" };
type Dock = { side: string; active: number; tabs: Tab[]; controls: Pick[] };
type Do = { n: number; what: string; x: number; y: number; arg: string };
type S = { dock: Dock; do: Do; down: boolean };

const ask = (s: S, what: string, arg = ""): S => ({ ...s, do: { n: s.do.n + 1, what, x: 0, y: BAR, arg } });

export const actions: PanelActions<S> = {
  open: { about: "Open a terminal (edit, shell, the project's) in the dock, or a controller (cli: a shell; timeline; audio) in a terminal of its own", args: { terminal: "string" }, run: (s, a) => ask(s, "dock-new", String(a.terminal ?? a.control ?? "cli")) },
  show: { about: "Show one of the dock's terminals, by its tab's id", args: { id: "number" }, run: (s, a) => ask(s, "dock-tab", String(a.id)) },
  close: { about: "Close one of the dock's terminals, by its tab's id", args: { id: "number" }, run: (s, a) => ask(s, "dock-close", String(a.id)) },
  move: { about: "Move the dock to an edge of the stage: bottom, left, top or right", args: { side: "string" }, run: (s, a) => ask(s, "dock-side", String(a.side ?? "")) },
  hide: { about: "Hide the dock", run: (s) => ask(s, "toggle", "dock") },
};

const within = (x: number, [a, b]: number[]) => x >= a && x < b;
const tabAt = (i: number) => [LEFT + i * (TAB + GAP), LEFT + i * (TAB + GAP) + TAB];

// the panel's width, as last laid out (⇅ and × sit at its right end)
let W = 800;
const hideAt = () => [W - 6 - BTN, W - 6];
const sideAt = () => [W - 6 - 2 * BTN - 2, W - 6 - BTN - 2];

const step = (state: S, input: FrameInput): S => {
  let s = applyActions(actions, state, input);
  const d = voidIn<Dock>(input, "dock");
  if (d) s = { ...s, dock: d };
  const p = input.pointer;
  const down = !!p?.down;
  if (p && down && !s.down) {
    const n = s.dock.tabs.length;
    if (p.y < BAR) {
      const i = s.dock.tabs.findIndex((_, i) => within(p.x, tabAt(i)));
      if (within(p.x, hideAt())) s = actions.hide.run(s, {});
      else if (within(p.x, sideAt())) s = ask(s, "dock-side");
      else if (i >= 0) s = p.x >= tabAt(i)[1] - X ? ask(s, "dock-close", String(s.dock.tabs[i].id)) : ask(s, "dock-tab", String(s.dock.tabs[i].id));
      else if (within(p.x, [tabAt(n)[0], tabAt(n)[0] + BTN])) s = ask(s, "dock-tab", s.dock.active ? "0" : String(s.dock.tabs[n - 1]?.id ?? 0));
    } else if (!s.dock.active) {
      const c = s.dock.controls[Math.floor((p.y - BAR - 6) / ROW)];
      if (c) s = ask(s, "dock-new", c.slug);
    }
  }
  return down === s.down ? s : { ...s, down };
};

export default function DockBar({ width }: { width: number; height: number }) {
  W = width;
  const s = useGameState<S>({ dock: { side: "bottom", active: 0, tabs: [], controls: [] }, do: { n: 0, what: "", x: 0, y: 0, arg: "" }, down: false }, step);
  const { tabs, active, controls } = s.dock;
  const plus = tabAt(tabs.length)[0];
  const icon = (d: string, x: number) => (
    <div style={{ position: "absolute", left: x, top: 3, width: BTN, height: BTN, display: "flex", alignItems: "center", justifyContent: "center" }}>
      <svg width={14} height={14} viewBox="0 0 24 24">
        <path d={d} fill="none" stroke="#8b8b94" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </div>
  );
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ do: s.do })} style={{ background: "#0e0e10", borderTop: "1px solid #1c1c21", fontFamily: "Inter", fontSize: 12, color: "#c9c9d1" }}>
      <div style={{ position: "absolute", left: 0, right: 0, top: 0, height: BAR, borderBottom: "1px solid #1c1c21" }} />
      {tabs.map((t, i) => {
        const on = t.id === active;
        return (
          <div key={t.id} style={{ position: "absolute", left: tabAt(i)[0], top: 4, width: TAB, height: BAR - 8, borderRadius: 6, background: on ? "#1f1f25" : "transparent", display: "flex", alignItems: "center", padding: "0 6px 0 10px", boxSizing: "border-box", gap: 6, whiteSpace: "nowrap", overflow: "hidden" }}>
            <span style={{ color: on ? "#e8e8ea" : "#8b8b94", flexGrow: 1, overflow: "hidden" }}>{t.name}</span>
            <span style={{ color: "#6b6b74", width: X - 6, textAlign: "center" }}>×</span>
          </div>
        );
      })}
      {icon("M12 5v14 M5 12h14", plus)}
      {icon("M8 4v16 M4 8l4-4 4 4 M16 20V4 M12 16l4 4 4-4", sideAt()[0])}
      {icon("M6 6l12 12 M18 6L6 18", hideAt()[0])}
      {!active ? (
        <div style={{ position: "absolute", left: 0, right: 0, top: BAR + 6 }}>
          {controls.map((c) => (
            <div key={c.kind + c.slug} style={{ height: ROW, display: "flex", alignItems: "center", gap: 10, padding: "0 14px" }}>
              <span style={{ color: "#e8e8ea" }}>{c.name}</span>
              <span style={{ color: "#6b6b74" }}>{[c.kind === "terminal" ? "terminal" : c.slug === "cli" ? "a shell in the project" : "controller", c.own ? "the project's" : "void's"].join(" · ")}</span>
            </div>
          ))}
        </div>
      ) : null}
    </AbsoluteFill>
  );
}
