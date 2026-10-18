// raea's live view runtime: React's API for the TSX a game is written in, without React's reconciler. Each frame the
// tree is rendered top-down (function components called, hooks kept per component instance), and the host elements
// it yields are matched to the ones already on the page by place and key: only the props, text and children that
// changed go out, as the same ops bevy-react's renderer sends (create, append, insert, remove, update, updateText),
// so Rust and the page see exactly what React would have made. No fibers, no scheduler, no work loop: one pass.
//
// A live game's bundle resolves "react" here (raea-motion's bundle_composition, lite); renders keep React, and both
// must paint the same page (checked: raea-motion play --snapshot against render --frame).

import { allocId, buildUpdateOp, dropHandlers, flush, push, ROOT_ID, serializeProps } from "../node_modules/bevy-react/src/bridge";

// ---- elements

export const Fragment = Symbol.for("react.fragment");
const ELEMENT = Symbol.for("react.transitional.element");
const PROVIDER = Symbol.for("raea.lite.provider");

export interface Element {
  $$typeof: symbol;
  type: any;
  key: string | null;
  props: any;
  ref?: any;
}

export function jsx(type: any, props: any, key?: any): Element {
  const p = props ?? {};
  let ref;
  if (p.ref !== undefined && typeof type === "string") ref = p.ref;
  return { $$typeof: ELEMENT, type, key: key === undefined || key === null ? (p.key != null ? String(p.key) : null) : String(key), props: p, ref };
}
export const jsxs = jsx;
export const jsxDEV = jsx;

export function createElement(type: any, config: any, ...children: any[]): Element {
  const props: any = {};
  let key: string | null = null;
  if (config) {
    for (const k in config) {
      if (k === "key") key = config.key != null ? String(config.key) : null;
      else props[k] = config[k];
    }
  }
  if (children.length === 1) props.children = children[0];
  else if (children.length > 1) props.children = children;
  return jsx(type, props, key);
}

export function isValidElement(v: any): v is Element {
  return typeof v === "object" && v !== null && v.$$typeof === ELEMENT;
}

export function cloneElement(el: Element, config?: any, ...children: any[]): Element {
  const props = { ...el.props, ...(config ?? {}) };
  delete props.key;
  if (children.length === 1) props.children = children[0];
  else if (children.length > 1) props.children = children;
  return jsx(el.type, props, config && config.key != null ? config.key : el.key);
}

function flat(children: any, out: any[] = []): any[] {
  if (Array.isArray(children)) {
    for (const c of children) flat(c, out);
  } else if (children !== undefined && children !== null && typeof children !== "boolean") {
    out.push(children);
  }
  return out;
}

export const Children = {
  toArray: (c: any) => flat(c),
  map: (c: any, f: (x: any, i: number) => any) => (c === undefined || c === null ? c : flat(c).map(f)),
  forEach: (c: any, f: (x: any, i: number) => void) => {
    flat(c).forEach(f);
  },
  count: (c: any) => flat(c).length,
  only: (c: any) => {
    const a = flat(c);
    if (a.length !== 1 || !isValidElement(a[0])) throw new Error("Children.only expected one element");
    return a[0];
  },
};

// ---- context

export interface Context<T> {
  Provider: any;
  Consumer: any;
  _default: T;
  _id: symbol;
}

export function createContext<T>(defaultValue: T): Context<T> {
  const ctx: any = { _default: defaultValue, _id: Symbol("context") };
  ctx.Provider = { $$provider: PROVIDER, ctx };
  ctx.Consumer = (props: any) => props.children(useContext(ctx));
  return ctx;
}

// ---- hooks

interface Instance {
  fn: any;
  hooks: any[];
  effects: (() => void)[];
  cleanups: Map<number, () => void>;
  /** the component instances under it, by place: `key or index:type` */
  kids: Map<string, Instance>;
  seen: boolean;
  id: string;
}

let current: Instance | null = null;
let hookIndex = 0;
let contexts: Map<symbol, any> = new Map();
let idCounter = 0;
let dirty = false;
let rerender: (() => void) | null = null;

function hook<T>(init: () => T): { v: T; i: number } {
  if (!current) throw new Error("a hook was called outside a component (raea lite)");
  const i = hookIndex++;
  if (current.hooks.length <= i) current.hooks.push(init());
  return { v: current.hooks[i], i };
}

export function useContext<T>(ctx: Context<T>): T {
  return contexts.has(ctx._id) ? contexts.get(ctx._id) : ctx._default;
}

export function useState<T>(init: T | (() => T)): [T, (v: T | ((p: T) => T)) => void] {
  const { v } = hook(() => {
    const cell: any = { value: typeof init === "function" ? (init as any)() : init };
    cell.set = (n: any) => {
      const next = typeof n === "function" ? n(cell.value) : n;
      if (!Object.is(next, cell.value)) {
        cell.value = next;
        dirty = true;
        if (rerender) queueMicrotask(() => rerender && dirty && rerender());
      }
    };
    return cell;
  });
  return [v.value, v.set];
}

export function useReducer(reducer: any, init: any, initFn?: any): [any, (a: any) => void] {
  const [s, set] = useState(initFn ? () => initFn(init) : init);
  return [s, (a: any) => set((p: any) => reducer(p, a))];
}

const same = (a?: any[], b?: any[]) => !!a && !!b && a.length === b.length && a.every((x, i) => Object.is(x, b[i]));

export function useMemo<T>(f: () => T, deps?: any[]): T {
  const { v } = hook(() => ({ deps: undefined as any[] | undefined, value: undefined as T, init: false }));
  if (!v.init || !deps || !same(v.deps, deps)) {
    v.value = f();
    v.deps = deps;
    v.init = true;
  }
  return v.value;
}

export function useCallback<T>(f: T, deps?: any[]): T {
  return useMemo(() => f, deps);
}

export function useRef<T>(init: T): { current: T } {
  return hook(() => ({ current: init })).v;
}

export function useId(): string {
  return hook(() => `:r${(idCounter++).toString(36)}:`).v;
}

function effect(f: () => void | (() => void), deps?: any[]) {
  const inst = current!;
  const { v, i } = hook(() => ({ deps: undefined as any[] | undefined, init: false }));
  if (!v.init || !deps || !same(v.deps, deps)) {
    v.deps = deps;
    v.init = true;
    inst.effects.push(() => {
      const old = inst.cleanups.get(i);
      if (old) old();
      const c = f();
      if (typeof c === "function") inst.cleanups.set(i, c);
      else inst.cleanups.delete(i);
    });
  }
}
export const useEffect = effect;
export const useLayoutEffect = effect;
export const useInsertionEffect = effect;

export function useSyncExternalStore<T>(subscribe: (cb: () => void) => () => void, get: () => T): T {
  const [, force] = useState(0);
  effect(() => subscribe(() => force((n: number) => n + 1)), [subscribe]);
  return get();
}

export function useImperativeHandle() {}
export function useDebugValue() {}
export function useTransition(): [boolean, (f: () => void) => void] {
  return [false, (f) => f()];
}
export function useDeferredValue<T>(v: T): T {
  return v;
}

export function memo(c: any) {
  return c;
}
export function forwardRef(render: (props: any, ref: any) => any) {
  return (props: any) => render(props, props.ref ?? null);
}
export function startTransition(f: () => void) {
  f();
}
export class Component<P = any, S = any> {
  props: P;
  state!: S;
  constructor(props: P) {
    this.props = props;
  }
  setState() {
    throw new Error("class components aren't in raea lite (live games): write a function component");
  }
}
export const version = "19.2.7-raea-lite";

// ---- host nodes

interface Host {
  id: number;
  type: string;
  kind: string;
  text: boolean;
  props: any;
  value?: string;
  kids: Host[];
  /** the place it was matched by */
  slot: string;
}

/** What a render yields: host elements and texts, in order, under one host parent. */
type Out = { slot: string; type: string; props: any; text?: string; kids: Out[]; inText: boolean };

// Each component rendered this frame, by its code (FNV-1a over the UTF-16 of its source, as Function.prototype.toString
// gives it; bake.rs hashes the bundle's functions the same way): a baked timeline re-steps, after an edit, only the
// frames where a changed component rendered.
const renderedCodes = new Set<number>();
const codes = new WeakMap<Function, number>();
function codeOf(f: Function): number {
  let c = codes.get(f);
  if (c === undefined) {
    const s = Function.prototype.toString.call(f);
    let h = 0x811c9dc5;
    for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 0x01000193) >>> 0;
    c = h >>> 0;
    codes.set(f, c);
  }
  return c;
}
/** The components rendered since the last call, as hex codes joined by commas; the set starts again. */
export function takeRendered(): string {
  const s = [...renderedCodes].map((c) => c.toString(16)).join(",");
  renderedCodes.clear();
  return s;
}

function render(node: any, parent: Instance, slot: string, out: Out[], inText: boolean) {
  if (node === undefined || node === null || typeof node === "boolean") return;
  if (Array.isArray(node)) {
    node.forEach((c, i) => render(c, parent, `${slot}.${isValidElement(c) && c.key !== null ? "k" + c.key : i}`, out, inText));
    return;
  }
  if (typeof node === "string" || typeof node === "number") {
    out.push({ slot, type: "#text", props: null, text: String(node), kids: [], inText });
    return;
  }
  if (!isValidElement(node)) return;
  const t = node.type;
  if (t === Fragment) {
    render(node.props.children, parent, slot, out, inText);
    return;
  }
  if (typeof t === "object" && t && t.$$provider === PROVIDER) {
    const ctx = t.ctx as Context<any>;
    const had = contexts.has(ctx._id);
    const prev = contexts.get(ctx._id);
    contexts.set(ctx._id, node.props.value);
    render(node.props.children, parent, slot, out, inText);
    if (had) contexts.set(ctx._id, prev);
    else contexts.delete(ctx._id);
    return;
  }
  if (typeof t === "function") {
    renderedCodes.add(codeOf(t));
    const name = t.name || "c";
    const k = `${slot}:${name}`;
    let inst = parent.kids.get(k);
    if (!inst || inst.fn !== t) {
      if (inst) unmountInstance(inst);
      inst = { fn: t, hooks: [], effects: [], cleanups: new Map(), kids: new Map(), seen: true, id: k };
      parent.kids.set(k, inst);
    }
    inst.seen = true;
    const saved = [current, hookIndex] as const;
    current = inst;
    hookIndex = 0;
    let rendered;
    try {
      rendered = t.prototype && t.prototype.render ? new t(node.props).render() : t(node.props);
    } finally {
      [current, hookIndex] = saved;
    }
    for (const kid of inst.kids.values()) kid.seen = false;
    render(rendered, inst, k, out, inText);
    for (const [kk, kid] of inst.kids) {
      if (!kid.seen) {
        unmountInstance(kid);
        inst.kids.delete(kk);
      }
    }
    effectsQueue.push(inst);
    return;
  }
  if (typeof t === "string") {
    const kids: Out[] = [];
    const childText = t === "text";
    const c = node.props.children;
    const inline = t === "text" && (typeof c === "string" || typeof c === "number");
    if (!inline) render(c, parent, `${slot}/`, kids, inText || childText);
    out.push({ slot, type: t, props: node.props, kids, inText, text: inline ? String(c) : undefined });
  }
}

const effectsQueue: Instance[] = [];

function unmountInstance(inst: Instance) {
  for (const c of inst.cleanups.values()) {
    try {
      c();
    } catch (e) {
      console.error("[js] an effect's cleanup threw:", e);
    }
  }
  inst.cleanups.clear();
  for (const k of inst.kids.values()) unmountInstance(k);
  inst.kids.clear();
}

function kindOf(type: string, inText: boolean) {
  return !inText ? type : type === "text" ? "textSpan" : type === "image" ? "inlineImage" : type;
}

function create(o: Out): Host {
  const id = allocId();
  if (o.type === "#text") {
    push(o.inText ? { op: "createTextSpan", id, text: o.text } : { op: "createText", id, text: o.text });
    return { id, type: "#text", kind: "#text", text: true, props: null, value: o.text, kids: [], slot: o.slot };
  }
  const kind = kindOf(o.type, o.inText);
  push({ op: "create", id, kind, props: serializeProps(id, o.props, o.type), text: o.text });
  const h: Host = { id, type: o.type, kind, text: false, props: o.props, value: o.text, kids: [], slot: o.slot };
  for (const k of o.kids) {
    const c = create(k);
    push({ op: "append", parent: id, child: c.id });
    h.kids.push(c);
  }
  return h;
}

function drop(h: Host) {
  dropHandlers(h.id);
  for (const k of h.kids) drop(k);
}

function patch(h: Host, o: Out) {
  if (h.text) {
    if (h.value !== o.text) push({ op: "updateText", id: h.id, text: o.text });
    h.value = o.text;
    return;
  }
  const op = buildUpdateOp(h.id, h.props, o.props, h.type, h.kind);
  if (op) push(op);
  if (o.text !== undefined && o.text !== h.value) push({ op: "updateText", id: h.id, text: o.text });
  else if (o.text === undefined && h.value !== undefined && h.type === "text") push({ op: "updateText", id: h.id, text: "" });
  h.value = o.text;
  h.props = o.props;
  h.kids = children(h.id, h.kids, o.kids);
}

/** A host parent's children matched to what the render yields: same place and type kept (patched), the rest made or
 * removed, and moved where their order changed. */
function children(parent: number, old: Host[], next: Out[]): Host[] {
  const bySlot = new Map<string, Host>();
  for (const h of old) bySlot.set(h.slot, h);
  const kept = new Set<Host>();
  const out: Host[] = next.map((o) => {
    const h = bySlot.get(o.slot);
    if (h && h.type === o.type && !kept.has(h) && (h.text || h.kind === kindOf(o.type, o.inText))) {
      kept.add(h);
      patch(h, o);
      return h;
    }
    return create(o);
  });
  for (const h of old) {
    if (!kept.has(h)) {
      push({ op: "remove", parent, child: h.id });
      drop(h);
    }
  }
  // order: walk from the end, each one in place before the next (only those out of place move)
  const pos = new Map<Host, number>();
  old.forEach((h, i) => kept.has(h) && pos.set(h, i));
  let lastOld = Infinity;
  for (let i = out.length - 1; i >= 0; i--) {
    const h = out[i];
    const was = pos.get(h);
    if (was !== undefined && was < lastOld) {
      lastOld = was;
      continue;
    }
    const before = out[i + 1];
    push(before ? { op: "insert", parent, child: h.id, before: before.id } : { op: "append", parent, child: h.id });
  }
  return out;
}

// ---- the root

const rootInstance: Instance = { fn: null, hooks: [], effects: [], cleanups: new Map(), kids: new Map(), seen: true, id: "root" };
let rootHosts: Host[] = [];
let rootElement: any = null;

/** Render `element` into the page's root (the first call mounts; each later call patches what changed), then run the
 * effects the render set off and flush the frame's ops. */
export function renderRoot(element: any): void {
  rootElement = element;
  rerender = () => renderRoot(rootElement);
  dirty = false;
  contexts = new Map();
  const out: Out[] = [];
  for (const kid of rootInstance.kids.values()) kid.seen = false;
  render(element, rootInstance, "r", out, false);
  for (const [k, kid] of rootInstance.kids) {
    if (!kid.seen) {
      unmountInstance(kid);
      rootInstance.kids.delete(k);
    }
  }
  rootHosts = children(ROOT_ID, rootHosts, out);
  flush();
  const queue = effectsQueue.splice(0);
  for (const inst of queue) {
    const fx = inst.effects.splice(0);
    for (const f of fx) {
      try {
        f();
      } catch (e) {
        console.error("[js] an effect threw:", e);
      }
    }
  }
}

const React = {
  Fragment,
  Children,
  Component,
  createElement,
  cloneElement,
  isValidElement,
  createContext,
  useContext,
  useState,
  useReducer,
  useMemo,
  useCallback,
  useRef,
  useId,
  useEffect,
  useLayoutEffect,
  useInsertionEffect,
  useSyncExternalStore,
  useImperativeHandle,
  useDebugValue,
  useTransition,
  useDeferredValue,
  memo,
  forwardRef,
  startTransition,
  version,
};
export default React;
