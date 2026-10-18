// React -> rrl rows. A tiny react-reconciler host whose host elements are rrl kinds (<box>, <title>, <text>, <image>)
// plus <rule> (an rrl rule line, kept once per edit by its label). Rendering a component gives a tree; `toRrl` walks
// it into rrl lines: one fact per host element (`in parent` from nesting), one key line per `anim` entry.
import Reconciler from "react-reconciler";
import type { ReactNode } from "react";

export type Node = { type: string; props: Record<string, any>; children: Node[]; text?: string };
type Root = { children: Node[] };

const host: any = {
  supportsMutation: true, supportsPersistence: false, supportsHydration: false, isPrimaryRenderer: true,
  noTimeout: -1, scheduleTimeout: setTimeout, cancelTimeout: clearTimeout,
  supportsMicrotasks: true, scheduleMicrotask: queueMicrotask,
  getRootHostContext: () => ({}), getChildHostContext: (p: any) => p, getPublicInstance: (i: any) => i,
  prepareForCommit: () => null, resetAfterCommit: () => {}, preparePortalMount: () => {},
  getCurrentUpdatePriority: () => 32, setCurrentUpdatePriority: () => {}, resolveUpdatePriority: () => 32,
  resolveEventType: () => null, resolveEventTimeStamp: () => -1, NotPendingTransition: null,
  HostTransitionContext: { $$typeof: Symbol.for("react.context"), _currentValue: null, _currentValue2: null },
  resetFormInstance: () => {}, requestPostPaintCallback: () => {}, shouldAttemptEagerTransition: () => false,
  trackSchedulerEvent: () => {}, maySuspendCommit: () => false, preloadInstance: () => true,
  startSuspendingCommit: () => {}, suspendInstance: () => {}, waitForCommitToBeReady: () => null,
  shouldSetTextContent: (_t: string, p: any) => typeof p.children === "string" || typeof p.children === "number",
  createInstance: (type: string, props: any): Node => ({
    type, props, children: [],
    text: typeof props.children === "string" || typeof props.children === "number" ? String(props.children) : undefined,
  }),
  createTextInstance: (text: string): Node => ({ type: "#text", props: {}, children: [], text }),
  appendInitialChild: (p: Node, c: Node) => p.children.push(c),
  appendChild: (p: Node, c: Node) => p.children.push(c),
  appendChildToContainer: (r: Root, c: Node) => r.children.push(c),
  insertBefore: (p: Node, c: Node, b: Node) => p.children.splice(p.children.indexOf(b), 0, c),
  insertInContainerBefore: (r: Root, c: Node, b: Node) => r.children.splice(r.children.indexOf(b), 0, c),
  removeChild: (p: Node, c: Node) => p.children.splice(p.children.indexOf(c), 1),
  removeChildFromContainer: (r: Root, c: Node) => r.children.splice(r.children.indexOf(c), 1),
  finalizeInitialChildren: () => false, commitUpdate: () => {}, commitTextUpdate: () => {}, resetTextContent: () => {},
  clearContainer: (r: Root) => { r.children.length = 0; }, detachDeletedInstance: () => {},
  getInstanceFromNode: () => null, beforeActiveInstanceBlur: () => {}, afterActiveInstanceBlur: () => {},
  prepareScopeUpdate: () => {}, getInstanceFromScope: () => null,
};
const reconciler = Reconciler(host);

export function render(el: ReactNode): Node[] {
  const root: Root = { children: [] };
  // a part that throws (a missing theme value, a cue that isn't said) fails the render with its own message
  let failed: unknown;
  const err = (e: unknown) => { failed ??= e; };
  const c = reconciler.createContainer(root, 0, null, false, null, "", err, err, err, () => {});
  reconciler.updateContainerSync(el, c, null, null);
  reconciler.flushSyncWork();
  if (failed) throw failed;
  return root.children;
}

// ---- rrl text -----------------------------------------------------------------------------------------------------
export const sym = (s: string) => ({ sym: s }); // a bare name (style: cap)
const snake = (k: string) => k.replace(/[A-Z]/g, (m) => "_" + m.toLowerCase());
export function val(v: any): string {
  if (typeof v === "number") return String(Math.round(v * 1000) / 1000);
  if (v && typeof v === "object" && "sym" in v) return v.sym;
  if (typeof v === "string" && /^#[0-9a-fA-F]{3,8}$/.test(v)) return v;
  return JSON.stringify(String(v));
}
const SKIP = new Set(["id", "children", "anim", "kind"]);
/** anim={{y: k({0: 286, 5: 260}, "ease out")}} -> `id.y {0: 286, 5: 260} ease out` */
export const k = (keys: Record<number, number | string>, how = "ease out") => ({ keys, how });

export function toRrl(nodes: Node[], seenRules = new Set<string>()): string[] {
  const out: string[] = [];
  const walk = (n: Node, parent?: Node) => {
    if (n.type === "#text") return;
    if (n.type === "rule") {
      const label = n.props.label as string;
      if (!seenRules.has(label)) { seenRules.add(label); out.push(`${label}: ${n.text ?? n.children.map((c) => c.text ?? "").join("")}`); }
      return;
    }
    if (n.type === "comment") { out.push(`-- ${n.text ?? ""}`); return; }
    const p = { ...n.props };
    // a part with no time of its own lives as long as its parent
    if (parent && p.from === undefined) { p.from = parent.props.from; p.to = parent.props.to; }
    if (n.text !== undefined && p.text === undefined) p.text = n.text;
    const cols = Object.entries(p).filter(([c, v]) => !SKIP.has(c) && v !== undefined && v !== null)
      .map(([c, v]) => `${snake(c)}: ${val(v)}`);
    out.push(`${p.kind ?? n.type} ${p.id}${parent ? " in " + parent.props.id : ""} {${cols.join(", ")}}`);
    for (const [col, a] of Object.entries(p.anim ?? {}) as [string, any][]) {
      const ks = Object.entries(a.keys).map(([f, v]) => `${f}: ${val(v)}`).join(", ");
      out.push(`${p.id}.${snake(col)} {${ks}} ${a.how}`.trimEnd());
    }
    n.props = p;
    for (const c of n.children) walk(c, n);
  };
  for (const n of nodes) walk(n);
  return out;
}
