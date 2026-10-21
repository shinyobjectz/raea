// void's files (apps/void/shell.feature: The file panel is the workspace, all of it): the workspace's folder as a tree, each
// row with its Material Icon Theme icon (void works the icon out: shell_ui.rs), and under it the spaces there are. A
// click on a folder opens or closes it, on a composition opens it, on a space opens it; the wheel scrolls the tree.
import { AbsoluteFill, applyActions, useGameState, voidIn } from "@raea/motion";
import type { FrameInput, PanelActions } from "@raea/motion";

type Row = { rel: string; name: string; dir: boolean; depth: number; open: boolean; icon: string };
type Tree = { root: string; icons: string; rows: Row[]; more: boolean };
type Shell = { space: string; spaces: { slug: string }[] };
type Do = { n: number; what: string; x: number; y: number; arg: string };
type S = { tree: Tree; shell: Shell; do: Do; down: boolean; top: number; rest: number; hover: number };

const HEAD = 30, ROW = 22, INDENT = 12, SPACE_ROW = 24;
const ask = (s: S, what: string, arg: string): S => ({ ...s, do: { n: s.do.n + 1, what, x: 0, y: 0, arg } });

export const actions: PanelActions<S> = {
  open: { about: "Open a folder in the tree, or close it (its path in the workspace)", args: { path: "string" }, run: (s, a) => {
    const r = s.tree.rows.find((r) => r.rel === String(a.path));
    return r?.dir ? ask(s, r.open ? "tree-close" : "tree-open", r.rel) : s;
  } },
  space: { about: "Open a space", args: { slug: "string" }, run: (s, a) => ask(s, "space", String(a.slug)) },
};

// the panel's height, as last laid out (the spaces sit at its foot, the tree takes the rest)
let H = 600;
const spacesH = (n: number) => HEAD + n * SPACE_ROW + 8;

const step = (state: S, input: FrameInput): S => {
  let s = applyActions(actions, state, input);
  const t = voidIn<Tree>(input, "tree");
  if (t) s = { ...s, tree: t };
  const sh = voidIn<Shell>(input, "shell");
  if (sh) s = { ...s, shell: sh };
  const treeH = H - spacesH(s.shell.spaces.length) - HEAD;
  const fits = Math.max(1, Math.floor(treeH / ROW));
  // the wheel scrolls a row per ROW points
  const dy = voidIn<number>(input, "wheel");
  if (dy) {
    const total = s.rest + dy;
    const n = Math.trunc(total / ROW);
    s = { ...s, top: Math.max(0, Math.min(Math.max(0, s.tree.rows.length - fits), s.top + n)), rest: total - n * ROW };
  }
  const p = input.pointer;
  const at = (y: number) => (y >= HEAD && y < HEAD + treeH ? s.top + Math.floor((y - HEAD) / ROW) : -1);
  const hover = p ? at(p.y) : -1;
  if (hover !== s.hover) s = { ...s, hover };
  const down = !!p?.down;
  if (p && down && !s.down) {
    const r = s.tree.rows[at(p.y)];
    const sy = H - spacesH(s.shell.spaces.length) + HEAD;
    const sp = s.shell.spaces[Math.floor((p.y - sy) / SPACE_ROW)];
    if (r) s = r.dir ? ask(s, r.open ? "tree-close" : "tree-open", r.rel) : ask(s, "tree-file", r.rel);
    else if (p.y >= sy && sp) s = ask(s, "space", sp.slug);
  }
  return down === s.down ? s : { ...s, down };
};

export default function Files({ height }: { width: number; height: number }) {
  H = height;
  const s = useGameState<S>({ tree: { root: "", icons: "", rows: [], more: false }, shell: { space: "", spaces: [] }, do: { n: 0, what: "", x: 0, y: 0, arg: "" }, down: false, top: 0, rest: 0, hover: -1 }, step);
  const treeH = height - spacesH(s.shell.spaces.length) - HEAD;
  const shown = s.tree.rows.slice(s.top, s.top + Math.ceil(treeH / ROW));
  const icon = (name: string) => `file://${s.tree.icons}/${name}.svg`;
  const title = { height: HEAD, display: "flex", alignItems: "center", padding: "0 12px", fontSize: 11, fontWeight: 600, letterSpacing: 0.6, color: "#8b8b94" };
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ do: s.do })} style={{ background: "#0e0e10", borderRight: "1px solid #1c1c21", fontFamily: "Inter", fontSize: 13, color: "#d4d4da", display: "flex", flexDirection: "column" }}>
      <div style={title}>{(s.tree.root || "files").toUpperCase()}</div>
      <div style={{ height: treeH, overflow: "hidden", flexShrink: 0 }}>
        {shown.map((r, k) => (
          <div key={r.rel} style={{ height: ROW, display: "flex", alignItems: "center", gap: 6, paddingLeft: 10 + r.depth * INDENT, background: s.top + k === s.hover ? "#18181c" : "transparent", whiteSpace: "nowrap", overflow: "hidden" }}>
            <span style={{ width: 10, color: "#6b6b74", fontSize: 10 }}>{r.dir ? (r.open ? "▾" : "▸") : ""}</span>
            <img src={icon(r.icon)} style={{ width: 16, height: 16, flexShrink: 0 }} />
            <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{r.name}</span>
          </div>
        ))}
        {s.tree.rows.length === 0 ? <div style={{ padding: "6px 14px", color: "#6b6b74" }}>No workspace open</div> : null}
      </div>
      <div style={{ borderTop: "1px solid #1c1c21", flexShrink: 0 }}>
        <div style={title}>SPACES</div>
        {s.shell.spaces.map((sp) => (
          <div key={sp.slug} style={{ height: SPACE_ROW, display: "flex", alignItems: "center", gap: 8, padding: "0 12px", color: sp.slug === s.shell.space ? "#ffffff" : "#a8a8b0", background: sp.slug === s.shell.space ? "#1a1a1f" : "transparent" }}>
            <svg width={14} height={14} viewBox="0 0 24 24">
              <path d="M12 2.5 21 7v10l-9 4.5L3 17V7z M3 7l9 4.5L21 7 M12 11.5v10" fill="none" stroke={sp.slug === s.shell.space ? "#4f7bff" : "#6b6b74"} strokeWidth={1.8} strokeLinejoin="round" />
            </svg>
            <span>{sp.slug}</span>
          </div>
        ))}
      </div>
    </AbsoluteFill>
  );
}
