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
  /** a host element's output, the place and text context it was made for (render: an element rendered again at the
   * same place gives the same output, when nothing under it is a component) */
  _o: any;
  _s: any;
  _t: boolean;
}

export function jsx(type: any, props: any, key?: any): Element {
  const p = props ?? {};
  let ref;
  if (p.ref !== undefined && typeof type === "string") ref = p.ref;
  return { $$typeof: ELEMENT, type, key: key === undefined || key === null ? (p.key != null ? String(p.key) : null) : String(key), props: p, ref, _o: null, _s: null, _t: false };
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
  /** the component instances under it, by place */
  kids: Map<Slot, Instance>;
  seen: boolean;
  id: Slot | string;
  parent: Instance | null;
  // Z3's bailout (below): the element it was last rendered from, whether that was inside text, and, when it may be
  // reused, its last output with the contexts its subtree read (context, value, ...) and the components it rendered
  el: any;
  inText: boolean;
  out: Out[] | null;
  reads: any[] | null;
  codes: number[] | null;
  /** a state cell in it or under it changed since its last render */
  stale: boolean;
  /** reuses that failed in a row, and frames left before it's cached again (a subtree that reads the frame) */
  misses: number;
  cool: number;
  /** Z3's compiled element sites (engine/bundle/src/z3.rs): per site, its records this render; and which render this is */
  sites: Map<(r: Rec) => any, Sites> | null;
  stamp: number;
}

// ---- Z3's compiled sites (engine/bundle/src/z3.rs)

/** One occurrence of an element site in a component instance: the values it was last given and the element built
 * from them. */
interface Rec {
  v: any[];
  el: Element | null;
  t: any;
  k: any;
  /** a value differed from the last render's (or there was no element yet) */
  ch: boolean;
  /** the render that last reached it (keyed records unreached are let go) */
  u: number;
  /** the site's builder (engine/bundle/src/z3.rs): the props from the values */
  b: (r: Rec) => any;
  /** a key's literal prefix (__atk): the key is kp + k, made when the element is built */
  kp: string | null;
}
interface Sites {
  stamp: number;
  n: number;
  list: Rec[];
  keyed: Map<any, Rec> | null;
  /** keyed records reached this render */
  kn: number;
}
let stampSerial = 0;
const newRec = (b: (r: Rec) => any): Rec => ({ v: [], el: null, t: undefined, k: null, ch: true, u: 0, b, kp: null });

/** The record for a site's next occurrence in the component rendering now (by its key, or its order among the
 * site's unkeyed occurrences); outside a render, a new one. */
export function __at(site: (r: Rec) => any, key: any): Rec {
  const inst = current;
  if (!inst) {
    const r = newRec(site);
    r.k = key;
    return r;
  }
  const map = inst.sites ?? (inst.sites = new Map());
  let st = map.get(site);
  if (st === undefined) map.set(site, (st = { stamp: -1, n: 0, list: [], keyed: null, kn: 0 }));
  if (st.stamp !== inst.stamp) {
    // a new render of the instance: keyed records left far behind (a game's sprites come and go) are let go
    if (st.keyed && st.keyed.size > 2 * st.kn + 32) for (const [k, r] of st.keyed) if (r.u !== st.stamp) st.keyed.delete(k);
    st.stamp = inst.stamp;
    st.n = 0;
    st.kn = 0;
  }
  let r: Rec | undefined;
  if (key === null || key === undefined) {
    r = st.list[st.n];
    if (r === undefined) st.list.push((r = newRec(site)));
    st.n++;
  } else {
    const m = st.keyed ?? (st.keyed = new Map());
    r = m.get(key);
    if (r === undefined) m.set(key, (r = newRec(site)));
    st.kn++;
  }
  r.u = inst.stamp;
  r.ch = r.el === null || !Object.is(r.k, key);
  r.k = key;
  r.kp = null;
  return r;
}

/** __at for a key made of a literal prefix and a value ("s" + i): looked up by the value, so the key's string isn't
 * made every render, only when the element is built. */
export function __atk(site: (r: Rec) => any, prefix: string, x: any): Rec {
  if (x === null || x === undefined || typeof x === "object") return __at(site, prefix + x);
  const r = __at(site, x);
  r.kp = prefix;
  return r;
}

/** A pure call's literal argument (engine/bundle/src/z3.rs): the last one while its values are the same, else one built from them. */
export function __o(r: Rec): any {
  if (!r.ch) return r.el;
  r.el = r.b(r);
  return r.el;
}

/** A site's value `i`: kept when it's the same as last render's (an array, when its items are the same ones: a
 * .map() of unchanged elements), else taken and the element marked to be built again. */
export function __v(r: Rec, i: number, x: any): Rec {
  const o = r.v[i];
  if (Object.is(o, x) || (i < r.v.length && sameItems(o, x))) return r;
  r.v[i] = x;
  r.ch = true;
  return r;
}
function sameItems(a: any, b: any): boolean {
  if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (!Object.is(a[i], b[i])) return false;
  return true;
}

/** The site's element: the last one while its type and values are the same, else one built from them. */
export function __e(r: Rec, type: any): Element {
  if (!r.ch && r.t === type) return r.el!;
  r.t = type;
  r.el = jsx(type, r.b(r), r.kp === null ? r.k : r.kp + r.k);
  return r.el;
}
export { createElement as __h, Fragment as __F };

let current: Instance | null = null;
/** function components rendered (or reused) so far: a host subtree that added none holds no component */
let components = 0;
let hookIndex = 0;
let contexts: Map<symbol, any> = new Map();
let idCounter = 0;
let dirty = false;
let rerender: (() => void) | null = null;
// this pass's context reads and rendered components, in render order: an instance's are the run its render added
// (each kept at its size, with its own length: emptied with length = 0, a log's storage was let go and grown again
// every frame, a few KB a frame for a game's hundreds of components)
const readLog: any[] = [];
const codeLog: number[] = [];
let readN = 0;
let codeN = 0;

// Each hook takes its slot first and makes its state only when the slot is new: an init closure handed to a shared
// helper was made at every call, every frame, for every hook of every component (lightspeed Z3)
function slot(): number {
  if (!current) throw new Error("a hook was called outside a component (raea lite)");
  return hookIndex++;
}

export function useContext<T>(ctx: Context<T>): T {
  const v = contexts.has(ctx._id) ? contexts.get(ctx._id) : ctx._default;
  if (current) {
    readLog[readN++] = ctx;
    readLog[readN++] = v;
  }
  return v;
}

interface Cell {
  value: any;
  set: (n: any) => void;
  /** what useState hands back: one pair per cell, its value written each render (a pair is read where it's given) */
  pair: [any, (n: any) => void];
  dispatch?: (a: any) => void;
}
function cell(init: any): Cell {
  const owner = current!;
  const c: Cell = { value: typeof init === "function" ? init() : init, set: null as any, pair: null as any };
  c.set = (n: any) => {
    const next = typeof n === "function" ? n(c.value) : n;
    if (!Object.is(next, c.value)) {
      c.value = next;
      dirty = true;
      // it and every component above it render again (none may reuse its last output)
      for (let p: Instance | null = owner; p; p = p.parent) p.stale = true;
      if (rerender) queueMicrotask(() => rerender && dirty && rerender());
    }
  };
  c.pair = [c.value, c.set];
  return c;
}

export function useState<T>(init: T | (() => T)): [T, (v: T | ((p: T) => T)) => void] {
  const i = slot();
  const h = current!.hooks;
  let c: Cell = h[i];
  if (c === undefined) h.push((c = cell(init)));
  c.pair[0] = c.value;
  return c.pair;
}

export function useReducer(reducer: any, init: any, initFn?: any): [any, (a: any) => void] {
  const i = slot();
  const h = current!.hooks;
  let c: Cell = h[i];
  if (c === undefined) {
    h.push((c = cell(initFn ? () => initFn(init) : init)));
    c.pair = [c.value, null as any];
  }
  // the dispatch reads the reducer of the render it was last handed out by, as React's does
  c.dispatch = c.dispatch ?? ((a: any) => c.set((p: any) => (c as any).reducer(p, a)));
  (c as any).reducer = reducer;
  c.pair[0] = c.value;
  c.pair[1] = c.dispatch;
  return c.pair;
}

const same = (a?: any[], b?: any[]) => {
  if (!a || !b || a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (!Object.is(a[i], b[i])) return false;
  return true;
};

interface Memo {
  deps: any[] | undefined;
  value: any;
}
export function useMemo<T>(f: () => T, deps?: any[]): T {
  const i = slot();
  const h = current!.hooks;
  let m: Memo = h[i];
  if (m === undefined) h.push((m = { deps, value: f() }));
  else if (!deps || !same(m.deps, deps)) {
    m.value = f();
    m.deps = deps;
  }
  return m.value;
}

export function useCallback<T>(f: T, deps?: any[]): T {
  const i = slot();
  const h = current!.hooks;
  let m: Memo = h[i];
  if (m === undefined) h.push((m = { deps, value: f }));
  else if (!deps || !same(m.deps, deps)) {
    m.value = f;
    m.deps = deps;
  }
  return m.value;
}

export function useRef<T>(init: T): { current: T } {
  const i = slot();
  const h = current!.hooks;
  let r = h[i];
  if (r === undefined) h.push((r = { current: init }));
  return r;
}

export function useId(): string {
  const i = slot();
  const h = current!.hooks;
  if (h.length <= i) h.push(`:r${(idCounter++).toString(36)}:`);
  return h[i];
}

interface Fx {
  deps: any[] | undefined;
  init: boolean;
}
function effect(f: () => void | (() => void), deps?: any[]) {
  const inst = current!;
  const i = slot();
  let v: Fx = inst.hooks[i];
  if (v === undefined) inst.hooks.push((v = { deps: undefined, init: false }));
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
  slot: Slot;
  /** the output it was last made or patched from: the same one again (a reused component's) changes nothing under it */
  out?: Out;
}

/** What a render yields: host elements and texts, in order, under one host parent. */
type Out = { slot: Slot; type: string; props: any; text?: string; kids: Out[]; inText: boolean };
/** The children of an output with none: shared, never written (children() maps into a new list) */
const EMPTY: Out[] = Object.freeze([]) as unknown as Out[];

// A place in the tree: the path of keys, indices, component names and host boundaries from the root, interned as one
// object per place, kept from frame to frame. Places were strings built by concatenation each frame ("r/.0:Clock:Pong
// ..."), ~57 characters, flattened for each Map lookup: of a finished pong's ~490 KB of garbage a frame, those were
// the largest part. A place is compared by identity; its sub-places are found by a key, an index or a component's
// name without building anything.
interface Slot {
  /** sub-places by an element's key (string) or its index (number) among an array's items */
  items: Map<string | number, Slot> | null;
  /** sub-places by component name (a function component rendered here) */
  named: Map<string, Slot> | null;
  /** the place of a host element's children */
  inner: Slot | null;
  /** the render pass that last reached it; places unreached for PRUNE_EVERY passes are let go */
  used: number;
}
const newSlot = (): Slot => ({ items: null, named: null, inner: null, used: pass });
let pass = 0;
const PRUNE_EVERY = 256;
function item(s: Slot, k: string | number): Slot {
  const m = s.items ?? (s.items = new Map());
  let c = m.get(k);
  if (c === undefined) m.set(k, (c = newSlot()));
  c.used = pass;
  return c;
}
function named(s: Slot, name: string): Slot {
  const m = s.named ?? (s.named = new Map());
  let c = m.get(name);
  if (c === undefined) m.set(name, (c = newSlot()));
  c.used = pass;
  return c;
}
function inner(s: Slot): Slot {
  const c = s.inner ?? (s.inner = newSlot());
  c.used = pass;
  return c;
}
/** Places no render reached in the last PRUNE_EVERY passes, let go (a game's keyed sprites come and go). */
function prune(s: Slot, before: number) {
  for (const m of [s.items, s.named]) {
    if (!m) continue;
    for (const [k, c] of m) {
      if (c.used < before) m.delete(k);
      else prune(c, before);
    }
  }
  if (s.inner) {
    if (s.inner.used < before) s.inner = null;
    else prune(s.inner, before);
  }
}
const ROOT_SLOT: Slot = newSlot();

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

// Z3's bailout (docs/lightspeed.feature): a component given the same element object as last frame (props unchanged by
// construction; <Animate> and its moves give back the identical element while their content is the same), with no
// state changed in or under it and every context its subtree read still the same value, isn't called again: its last
// output is reused, and the host diff sees the same objects and sends nothing. Frame and input reach a component only
// through context or state, so what reads them renders again. Only a component given the same element keeps a copy of
// its output, and one whose reuse keeps failing (its subtree reads the frame) stops for 64 frames, so frames that can't
// reuse allocate nothing for it. Every 128th pass renders everything (places unreached for 256 passes are let go).
// globalThis.__raea_lite_bailout = false turns it off.
let bailout = true;
function readsSame(r: any[]): boolean {
  for (let i = 0; i < r.length; i += 2) {
    const ctx = r[i] as Context<any>;
    if (!Object.is(contexts.has(ctx._id) ? contexts.get(ctx._id) : ctx._default, r[i + 1])) return false;
  }
  return true;
}
/** `log` from `at` to its end is the same items as `kept` (the kept array is reused, not a new one made). */
function sameRun<T>(kept: T[] | null, log: T[], at: number, end: number): boolean {
  if (!kept || kept.length !== end - at) return false;
  for (let i = 0; i < kept.length; i++) if (!Object.is(kept[i], log[at + i])) return false;
  return true;
}

function render(node: any, parent: Instance, slot: Slot, out: Out[], inText: boolean) {
  if (node === undefined || node === null || typeof node === "boolean") return;
  if (Array.isArray(node)) {
    for (let i = 0; i < node.length; i++) {
      const c = node[i];
      render(c, parent, item(slot, isValidElement(c) && c.key !== null ? c.key : i), out, inText);
    }
    return;
  }
  if (typeof node === "string" || typeof node === "number") {
    out.push({ slot, type: "#text", props: null, text: String(node), kids: EMPTY, inText });
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
    components++;
    const code = codeOf(t);
    renderedCodes.add(code);
    const k = named(slot, t.name || "c");
    let inst = parent.kids.get(k);
    if (!inst || inst.fn !== t) {
      if (inst) unmountInstance(inst);
      inst = { fn: t, hooks: [], effects: [], cleanups: new Map(), kids: new Map(), seen: true, id: k, parent, el: null, inText, out: null, reads: null, codes: null, stale: false, misses: 0, cool: 0, sites: null, stamp: 0 };
      parent.kids.set(k, inst);
    }
    inst.seen = true;
    const same = node === inst.el && inst.inText === inText;
    if (inst.cool > 0) inst.cool--;
    if (bailout && same && inst.out && !inst.stale) {
      if (readsSame(inst.reads!)) {
        // its output is what it made last time: it isn't called (nor anything under it), and its effects don't run
        // (as React.memo)
        const o = inst.out, r = inst.reads!, c = inst.codes!;
        for (let i = 0; i < o.length; i++) out.push(o[i]);
        for (let i = 0; i < r.length; i++) readLog[readN++] = r[i];
        for (let i = 0; i < c.length; i++) {
          codeLog[codeN++] = c[i];
          renderedCodes.add(c[i]);
        }
        inst.misses = 0;
        return;
      }
      if (++inst.misses >= 3) {
        inst.cool = 64;
        inst.misses = 0;
      }
    }
    const keep = same && inst.cool === 0;
    const outAt = out.length, readAt = readN, codeAt = codeN;
    codeLog[codeN++] = code;
    const savedCurrent = current;
    const savedIndex = hookIndex;
    current = inst;
    hookIndex = 0;
    inst.stamp = ++stampSerial;
    let rendered;
    try {
      rendered = t.prototype && t.prototype.render ? new t(node.props).render() : t(node.props);
    } finally {
      current = savedCurrent;
      hookIndex = savedIndex;
    }
    // (the loops skipped for a leaf: each for-of over a Map makes an iterator, for hundreds of components a frame)
    const had = inst.kids.size > 0;
    if (had) inst.kids.forEach(unsee);
    render(rendered, inst, k, out, inText);
    if (had || inst.kids.size > 0) {
      inst.kids.forEach(sweep, inst.kids);
    }
    inst.el = node;
    inst.inText = inText;
    inst.stale = false;
    if (keep) {
      inst.out = out.slice(outAt);
      inst.reads = sameRun(inst.reads, readLog, readAt, readN) ? inst.reads : readLog.slice(readAt, readN);
      inst.codes = sameRun(inst.codes, codeLog, codeAt, codeN) ? inst.codes : codeLog.slice(codeAt, codeN);
    } else {
      inst.out = null;
    }
    effectsQueue[effectsN++] = inst;
    return;
  }
  if (typeof t === "string") {
    // the same element at the same place, with no component under it, yields the same output: reused whole (and
    // patch skips it, h.out === o). Not on the passes that render everything (the bailout's), so every place under it
    // stays reached
    if (node._o !== null && bailout && node._s === slot && node._t === inText) {
      out.push(node._o);
      return;
    }
    const before = components;
    const childText = t === "text";
    const c = node.props.children;
    const inline = t === "text" && (typeof c === "string" || typeof c === "number");
    // an element with no children shares one empty list (each frame made one for every leaf)
    const kids: Out[] = inline || c === undefined || c === null ? EMPTY : [];
    if (kids !== EMPTY) render(c, parent, inner(slot), kids, inText || childText);
    const o: Out = { slot, type: t, props: node.props, kids, inText, text: inline ? String(c) : undefined };
    out.push(o);
    if (components === before) {
      node._o = o;
      node._s = slot;
      node._t = inText;
    }
  }
}

// A component's children marked unreached before it renders, and those still unreached after let go. By forEach
// with these, made once: a for-of made an iterator each time, and over the entries a [key, value] array for every
// component under it, every frame (lightspeed Z3)
function unsee(kid: Instance) {
  kid.seen = false;
}
function sweep(this: Map<Slot, Instance>, kid: Instance) {
  if (!kid.seen) {
    unmountInstance(kid);
    this.delete(kid.id as Slot);
  }
}

const effectsQueue: Instance[] = [];
let effectsN = 0;

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
    return { id, type: "#text", kind: "#text", text: true, props: null, value: o.text, kids: [], slot: o.slot, out: o };
  }
  const kind = kindOf(o.type, o.inText);
  push({ op: "create", id, kind, props: serializeProps(id, o.props, o.type), text: o.text });
  const h: Host = { id, type: o.type, kind, text: false, props: o.props, value: o.text, kids: [], slot: o.slot, out: o };
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
  if (h.out === o) return;
  h.out = o;
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
  // the usual frame: the same places in the same order, each patched where it is (no maps, nothing moves)
  if (old.length === next.length) {
    let same = true;
    for (let i = 0; i < old.length; i++) {
      const h = old[i];
      const o = next[i];
      if (h.slot !== o.slot || h.type !== o.type || !(h.text || h.kind === kindOf(o.type, o.inText))) {
        same = false;
        break;
      }
    }
    if (same) {
      for (let i = 0; i < old.length; i++) patch(old[i], next[i]);
      return old;
    }
  }
  const bySlot = new Map<Slot, Host>();
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

const rootInstance: Instance = { fn: null, hooks: [], effects: [], cleanups: new Map(), kids: new Map(), seen: true, id: ROOT_SLOT, parent: null, el: null, inText: false, out: null, reads: null, codes: null, stale: false, misses: 0, cool: 0, sites: null, stamp: 0 };
let rootHosts: Host[] = [];
let rootElement: any = null;
const rootOut: Out[] = [];
const renderAgain = () => renderRoot(rootElement);

/** Render `element` into the page's root (the first call mounts; each later call patches what changed), then run the
 * effects the render set off and flush the frame's ops. */
export function renderRoot(element: any): void {
  rootElement = element;
  rerender = renderAgain;
  dirty = false;
  // (one map, one list and one re-render, kept: each was made again every frame)
  contexts.clear();
  readN = 0;
  codeN = 0;
  const out = rootOut;
  out.length = 0;
  pass++;
  bailout = (globalThis as any).__raea_lite_bailout !== false && pass % 128 !== 0;
  if (pass % PRUNE_EVERY === 0) prune(ROOT_SLOT, pass - PRUNE_EVERY);
  rootInstance.kids.forEach(unsee);
  render(element, rootInstance, ROOT_SLOT, out, false);
  rootInstance.kids.forEach(sweep, rootInstance.kids);
  rootHosts = children(ROOT_ID, rootHosts, out);
  flush();
  // (the queue kept at its size: splice(0) made a new array of every component rendered, each frame. An effect's
  // state change renders again in a microtask, after this loop)
  const n = effectsN;
  effectsN = 0;
  for (let i = 0; i < n; i++) {
    const inst = effectsQueue[i];
    effectsQueue[i] = undefined as any;
    // most components set off no effect this frame: nothing taken (splice made an empty array for each, every frame)
    if (inst.effects.length === 0) continue;
    const fx = inst.effects;
    inst.effects = [];
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
