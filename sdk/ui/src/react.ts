// React on @raea/ui's wire: a react-reconciler host config whose host operations emit the
// wire's ops (wire.ts), through its emitter, handler registry and host bindings. React keeps its own diff and its own
// list moves.
import { createContext, type ReactNode } from "react";
import "./platform";
import { installDnd } from "./dnd";
import Reconciler from "react-reconciler";
import { DefaultEventPriority, NoEventPriority } from "react-reconciler/constants";
import * as dom from "./dom";
import * as wire from "./wire";
import { EVENTS, ROOT, type Props } from "./wire";

// <view3d> and its parts, which the page host draws and React doesn't type (its import puts them in the program)
export type { View3dAttributes, Mesh3dAttributes, Light3dAttributes } from "./elements";

/** What a React `ref` on a host element receives: its wire id, and focus. */
export type Instance = { id: number; type: string; focus(): void };
type Container = { id: number };

/** Structural equality for prop values (an inline style object that didn't change isn't a change). */
function same(a: unknown, b: unknown, depth = 8): boolean {
  if (Object.is(a, b)) return true;
  if (depth === 0 || typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  const ka = Object.keys(a as object), kb = Object.keys(b as object);
  if (ka.length !== kb.length) return false;
  for (const k of ka) if (!same((a as Props)[k], (b as Props)[k], depth - 1)) return false;
  return true;
}

/** A host element's props as they cross: no children, key, ref, handlers or other functions; bigints as numbers. */
function wireProps(props: Props): Props {
  const out: Props = {};
  for (const k in props) {
    const v = props[k];
    if (k === "children" || k === "key" || k === "ref" || v === undefined || typeof v === "function" || k in EVENTS) continue;
    out[k] = typeof v === "bigint" ? Number(v) : v;
  }
  return out;
}
function listenAll(id: number, props: Props) {
  for (const k in props) if (k in EVENTS && typeof props[k] === "function") wire.listen(id, EVENTS[k], props[k] as (p?: unknown) => void);
}
const inlineText = (type: string, props: Props) => type === "text" && (typeof props.children === "string" || typeof props.children === "number");

// DOM mode (mount's `dom`, or createRoot): host instances are DOM-like elements (dom.ts, docs/ts-native.feature P18),
// what `ref.current` gets; the tree is mirrored as React commits, and events go through DOM capture and bubble paths
// with React's handlers run from its root container. Off, instances are `{id, type, focus}` as before.
let domMode = false;
/** a node's wire id: a DOM element's `__wid` (body is the host's root), or the plain instance's id */
const wid = (x: any): number => (domMode ? (x.__wid ?? (x === (globalThis as any).document?.body ? ROOT : x.id)) : x.id);

let priority: number = NoEventPriority;
/** A form's status when no action of it runs (react-dom's useFormStatus reads it through the host transition context) */
const NotPending = Object.freeze({ pending: false, data: null, method: null, action: null });
const HostTransitionContext = createContext<unknown>(NotPending) as unknown as Reconciler.ReactContext<unknown>;

// Suspense fallbacks and <Activity mode="hidden"> hide what they keep mounted, as React DOM does: display none on the
// element (its own style back when shown), a text's string emptied. Off DOM mode, the style a plain instance was given
const plainStyle = new WeakMap<object, Props>();
function hide(i: any) {
  if (domMode) return void i.style?.setProperty("display", "none", "important");
  wire.set(i.id, "style", { ...((plainStyle.get(i)?.style as object) ?? {}), display: "none" });
}
function unhide(i: any, props: Props) {
  const st = props.style as Record<string, unknown> | undefined;
  if (domMode) {
    if (st?.display != null) i.style?.setProperty("display", String(st.display));
    else i.style?.removeProperty("display");
    return;
  }
  if (st && typeof st === "object") wire.set(i.id, "style", st);
  else wire.unset(i.id, "style");
}

const HTML_CTX = {};
const SVG_CTX = { svg: true };
const SVG_AMBIGUOUS = new Set(["text", "image", "title"]);

const config: Reconciler.HostConfig<string, Props, Container, Instance, { id: number }, never, never, never, Instance, object, never, number, -1, unknown> = {
  supportsMutation: true,
  supportsPersistence: false,
  supportsHydration: false,
  isPrimaryRenderer: true,
  noTimeout: -1,
  scheduleTimeout: (fn: (...a: unknown[]) => void, delay?: number) => setTimeout(fn, delay) as unknown as number,
  cancelTimeout: (h: number) => clearTimeout(h),
  supportsMicrotasks: true,
  scheduleMicrotask: (fn: () => void) => queueMicrotask(fn),
  // whether an element is inside an <svg> (its text, image and title are SVG's there, the host's own elsewhere)
  getRootHostContext: () => HTML_CTX,
  getChildHostContext: (parent: object, type: string) => (type === "svg" ? SVG_CTX : type === "foreignObject" ? HTML_CTX : parent),
  getPublicInstance: (i: Instance) => i,
  prepareForCommit: () => null,
  resetAfterCommit: () => wire.flush(),
  preparePortalMount: (c: any) => void (domMode && dom.reactContainer(c)),
  getCurrentUpdatePriority: () => priority,
  setCurrentUpdatePriority: (p: number) => void (priority = p),
  resolveUpdatePriority: () => (priority !== NoEventPriority ? priority : DefaultEventPriority),
  resolveEventType: () => null,
  resolveEventTimeStamp: () => -1,
  NotPendingTransition: NotPending,
  HostTransitionContext,
  // a form's action ended: its fields back to their defaults (React DOM's form.reset())
  resetFormInstance: (f: any) => void (domMode && f?.reset?.()),
  hideInstance: hide,
  unhideInstance: unhide,
  hideTextInstance: (t: any) => {
    wire.text(wid(t), "");
    if (domMode) dom.mirrorText(t, "");
  },
  unhideTextInstance: (t: any, text: string) => {
    wire.text(wid(t), text);
    if (domMode) dom.mirrorText(t, text);
  },
  requestPostPaintCallback: () => {},
  shouldAttemptEagerTransition: () => false,
  trackSchedulerEvent: () => {},
  maySuspendCommit: () => false,
  preloadInstance: () => true,
  startSuspendingCommit: () => {},
  suspendInstance: () => {},
  waitForCommitToBeReady: () => null,
  // a text whose only child is a string carries it as its text (no child of its own), as the core does
  shouldSetTextContent: inlineText,

  createInstance(type: string, props: Props, _root: unknown, ctx: object, fiber: unknown): Instance {
    const id = wire.allocId();
    if (domMode) {
      const h = dom.hostElement(id, type, props, fiber, ctx === SVG_CTX);
      wire.create(id, h.kind, h.props, h.text);
      // dangerouslySetInnerHTML, as React DOM sets it: the markup parsed into this element's children (an inline SVG
      // icon, a sanitizer's HTML), each on the host now that the element is
      dom.innerHtmlProp(h.el, undefined, props);
      dom.listenFor(h.el);
      return h.el as unknown as Instance;
    }
    const wp = wireProps(props);
    wire.create(id, type, ctx === SVG_CTX && SVG_AMBIGUOUS.has(type) ? { ...wp, $ns: "svg" } : wp, inlineText(type, props) ? String(props.children) : undefined);
    listenAll(id, props);
    const inst = { id, type, focus: () => wire.emit("focus", id) };
    plainStyle.set(inst, props);
    return inst;
  },
  // a string among other children is a text element of its own (the host makes it a span inside a text)
  createTextInstance(text: string) {
    const id = wire.allocId();
    wire.create(id, "text", {}, text);
    return domMode ? (dom.hostText(id, text) as unknown as { id: number }) : { id };
  },
  appendInitialChild: (p: Instance, c: { id: number }) => place(p, c, null),
  finalizeInitialChildren: () => false,
  appendChild: (p: Instance, c: { id: number }) => place(p, c, null),
  appendChildToContainer: (p: Container, c: { id: number }) => place(p, c, null),
  insertBefore: (p: Instance, c: { id: number }, b: { id: number }) => place(p, c, b),
  insertInContainerBefore: (p: Container, c: { id: number }, b: { id: number }) => place(p, c, b),
  removeChild: (p: Instance, c: { id: number }) => unplace(p, c),
  removeChildFromContainer: (p: Container, c: { id: number }) => unplace(p, c),

  // the props that changed, each sent whole (style too); handlers listened or unlistened as they come and go, their
  // closures refreshed with no op
  commitUpdate(i: Instance, type: string, old: Props, next: Props, fiber?: unknown) {
    if (domMode) return dom.hostUpdate(i as unknown as dom.Element_, type, old, next, fiber);
    plainStyle.set(i, next);
    const id = i.id;
    for (const k in old) {
      if (k === "children" || k === "key" || k === "ref") continue;
      if (k in EVENTS) {
        if (typeof old[k] === "function" && typeof next[k] !== "function") wire.unlisten(id, EVENTS[k]);
      } else if (old[k] !== undefined && typeof old[k] !== "function" && (next[k] === undefined || typeof next[k] === "function")) wire.unset(id, k);
    }
    for (const k in next) {
      const v = next[k];
      if (k === "children" || k === "key" || k === "ref" || v === undefined) continue;
      if (k in EVENTS) {
        if (typeof v === "function") wire.listen(id, EVENTS[k], v as (p?: unknown) => void);
      } else if (typeof v !== "function" && !same(old[k], v)) wire.set(id, k, typeof v === "bigint" ? Number(v) : v);
    }
    if (inlineText(type, next) && String(next.children) !== (inlineText(type, old) ? String(old.children) : undefined)) wire.text(id, String(next.children));
  },
  commitTextUpdate: (t: { id: number }, _old: string, next: string) => {
    wire.text(wid(t), next);
    if (domMode) dom.mirrorText(t as unknown as dom.Text_, next);
  },
  // a text going from a string to element children: its own string goes first
  resetTextContent: (i: Instance) => {
    wire.text(wid(i), "");
    if (domMode) (i as unknown as dom.Element_)._inline = undefined;
  },
  clearContainer: (c: any) => void (domMode && dom.mirrorClear(c)),
  detachDeletedInstance: (i: { id: number }) => wire.drop(wid(i)),
  getInstanceFromNode: () => null,
  beforeActiveInstanceBlur: () => {},
  afterActiveInstanceBlur: () => {},
  prepareScopeUpdate: () => {},
  getInstanceFromScope: () => null,
} as any;

function place(p: any, c: any, b: any) {
  wire.place(wid(p), wid(c), b ? wid(b) : undefined);
  if (domMode) dom.mirrorPlace(p, c, b);
}
function unplace(p: any, c: any) {
  wire.remove(wid(c));
  if (domMode) dom.mirrorRemove(p, c);
}

const reconciler = Reconciler(config);
// in development, React Refresh's runtime (the window's dev runner, apps/app blitz/runner.ts) installs the devtools hook
// before React loads; registered with it, an edited component re-renders in place with its state kept
if ((globalThis as { __REACT_DEVTOOLS_GLOBAL_HOOK__?: unknown }).__REACT_DEVTOOLS_GLOBAL_HOOK__) reconciler.injectIntoDevTools();
const ConcurrentRoot = 1;

/** Runs `fn` and commits what it set synchronously (each host event is run this way). */
export const flushSync = (fn: () => void) => reconciler.flushSyncFromReconciler(fn);

/** A root's error callbacks (react-dom/client's createRoot options): React DOM's defaults report to the console. */
export type RootOptions = {
  dom?: boolean;
  identifierPrefix?: string;
  onUncaughtError?: (error: unknown, info: { componentStack?: string }) => void;
  onCaughtError?: (error: unknown, info: { componentStack?: string; errorBoundary?: unknown }) => void;
  onRecoverableError?: (error: unknown, info: { componentStack?: string }) => void;
};
const logError = (e: unknown) => console.error(`[ui] react: ${(e as Error)?.stack ?? e}`);
/** React DOM's default for an error no boundary caught: reported as an uncaught error is (window's error event) */
const uncaught = (e: unknown) => {
  const r = (globalThis as { reportError?: (e: unknown) => void }).reportError;
  if (r) r(e);
  else logError(e);
};

function start(opts: RootOptions) {
  wire.setDispatch((fn) => {
    flushSync(fn);
    wire.flush();
  });
  if (opts.dom) {
    domMode = true;
    dom.install();
    dom.setRunner((fn) => flushSync(fn));
    formActions();
    installDnd();
  }
}
function container(at: number, opts: RootOptions) {
  const box = domMode ? dom.container(at) : { id: at };
  // React's handlers run from its root container's listeners: the host's root has them from the start, and a root
  // mounted on another element (raea.app's welcome and pages over the dashboard, 11-23) needs them as a portal does,
  // or it draws and no press reaches its handlers
  if (domMode && at !== ROOT) dom.reactContainer(box);
  return reconciler.createContainer(box, ConcurrentRoot, null, false, null, opts.identifierPrefix ?? "", opts.onUncaughtError ?? uncaught, opts.onCaughtError ?? logError, opts.onRecoverableError ?? logError, () => {}, null);
}

/** Mounts a React tree under the host's root (or under the element `at`: a slot a core region owns), its commits
 *  applied synchronously, each host event's handler in flushSync. Returns the unmount. */
export function mount(element: ReactNode, at: number = ROOT, opts: RootOptions = {}): () => void {
  start(opts);
  if (at === ROOT) wire.clear();
  if (domMode && at === ROOT) dom.resetMount();
  const root = container(at, opts);
  wire.quiet(() => flushSync(() => reconciler.updateContainer(element, root, null, null)));
  wire.flush();
  return () => {
    flushSync(() => reconciler.updateContainer(null, root, null, null));
    wire.flush();
  };
}

/** react-dom/client's createRoot over the DOM mode: `createRoot(document.getElementById("root")).render(<App />)`,
 *  as an app written for React DOM mounts. One root per call, kept across renders (its state with it); render is
 *  scheduled as React DOM's is (a default-priority update, committed by the scheduler), unmount is synchronous. */
export function createRoot(el: { __wid?: number } | null, opts: Omit<RootOptions, "dom"> = {}) {
  if (!el) throw new Error("createRoot(...): Target container is not a DOM element.");
  const at = el.__wid ?? ROOT;
  start({ ...opts, dom: true });
  if (at === ROOT) {
    wire.clear();
    dom.resetMount();
  }
  let root: ReturnType<typeof container> | null = container(at, opts);
  return {
    render(children: ReactNode) {
      if (!root) throw new Error("Cannot update an unmounted root.");
      reconciler.updateContainer(children, root, null, null);
    },
    unmount() {
      if (!root) return;
      const r = root;
      root = null;
      flushSync(() => reconciler.updateContainer(null, r, null, null));
      wire.flush();
    },
  };
}

// Form actions, as React DOM's FormActionEventPlugin runs them: a submit that no handler prevented, on a form React
// rendered whose action (or the submitter's formAction) is a function, is prevented and runs that function in a host
// transition with the form's FormData (useFormStatus pending meanwhile); the form resets once it ends. Heard on window
// in the bubble phase, after every React handler has run.
let formsHeard = false;
const coerce = (a: unknown) => (typeof a === "function" ? a : null);
function formActions() {
  if (formsHeard) return;
  formsHeard = true;
  (globalThis as any).addEventListener?.("submit", (ev: any) => {
    const form = ev.target;
    const fiber = form?._fiber;
    if (!fiber || fiber.tag !== 5 || form.localName !== "form") return;
    let action = coerce(form._props?.action) as ((fd: FormData) => unknown) | null;
    let submitter = ev.submitter;
    if (submitter) {
      const fa = submitter._props ? coerce(submitter._props.formAction) : null;
      if (fa) {
        action = fa as (fd: FormData) => unknown;
        submitter = null;
      }
    }
    if (ev.defaultPrevented || !action) return;
    ev.preventDefault();
    const FD = (globalThis as any).FormData;
    const fd = new FD(form, submitter ?? undefined);
    (reconciler as any).startHostTransition(fiber, { pending: true, data: fd, method: form.method ?? "get", action } as any, action as any, fd);
  });
}

/** react-dom's createPortal: children rendered into another element (document.body, an element a library made). */
export function createPortal(children: ReactNode, container: unknown, key: string | null = null): ReactNode {
  return { $$typeof: Symbol.for("react.portal"), key: key == null ? null : String(key), children, containerInfo: container, implementation: null } as unknown as ReactNode;
}
