// The observers, matchMedia, and the frame: snapshot, observers, requestAnimationFrame.
import * as wire from "../wire";
import { DOMRectReadOnly_, Document_, Element_, EventTarget_, Event_, Node_, S, STRIDE, UIEvent_, body, computedOf, dispatch, dispatchDirect, doc, g, geo, html, nodeList, reportMisses, run, scanning } from "./all";

// ---------------------------------------------------------------- observers

export type MutationRecord_ = { type: string; target: Node_; addedNodes: Node_[]; removedNodes: Node_[]; previousSibling: Node_ | null; nextSibling: Node_ | null; attributeName: string | null; attributeNamespace: null; oldValue: string | null };

export const observers = new Set<MutationObserver_>();

export class MutationObserver_ {
  _targets = new Map<Node_, any>();
  _records: MutationRecord_[] = [];
  constructor(public _cb: (records: MutationRecord_[], o: MutationObserver_) => void) {
    if (typeof _cb !== "function") throw new TypeError("MutationObserver: the callback isn't a function");
  }
  observe(target: Node_, opts: any = {}) {
    const o = { ...opts };
    if (o.attributeOldValue || o.attributeFilter) o.attributes ??= true;
    if (o.characterDataOldValue) o.characterData ??= true;
    if (!o.childList && !o.attributes && !o.characterData) throw new TypeError("MutationObserver.observe: childList, attributes or characterData must be true");
    this._targets.set(target, o);
    observers.add(this);
  }
  disconnect() {
    this._targets.clear();
    this._records = [];
    observers.delete(this);
  }
  takeRecords() {
    const r = this._records;
    this._records = [];
    return r;
  }
}

let moScheduled = false;

export function queueRecord(target: Node_, rec: Partial<MutationRecord_>) {
  for (const mo of observers) {
    let opts: any = null;
    for (let n: Node_ | null = target; n; n = n.parentNode) {
      const o = mo._targets.get(n);
      if (o && (n === target || o.subtree)) {
        opts = o;
        break;
      }
    }
    if (!opts) continue;
    if (rec.type === "childList" && !opts.childList) continue;
    if (rec.type === "attributes" && (!opts.attributes || (opts.attributeFilter && !opts.attributeFilter.includes(rec.attributeName)))) continue;
    if (rec.type === "characterData" && !opts.characterData) continue;
    const keepOld = (rec.type === "attributes" && opts.attributeOldValue) || (rec.type === "characterData" && opts.characterDataOldValue);
    mo._records.push({ type: rec.type!, target, addedNodes: nodeList(rec.addedNodes ?? []), removedNodes: nodeList(rec.removedNodes ?? []), previousSibling: rec.previousSibling ?? null, nextSibling: rec.nextSibling ?? null, attributeName: rec.attributeName ?? null, attributeNamespace: null, oldValue: keepOld ? (rec.oldValue ?? null) : null });
    if (!moScheduled) {
      moScheduled = true;
      queueMicrotask(deliverMutations);
    }
  }
}

function deliverMutations() {
  moScheduled = false;
  for (const mo of [...observers]) {
    const r = mo.takeRecords();
    if (r.length) run(() => mo._cb(r, mo));
  }
}

const resizeObservers = new Set<ResizeObserver_>();

export class ResizeObserver_ {
  _targets = new Map<Element_, { box: string; last: [number, number] | null }>();
  constructor(public _cb: (entries: any[], o: ResizeObserver_) => void) {
    if (typeof _cb !== "function") throw new TypeError("ResizeObserver: the callback isn't a function");
  }
  observe(el: Element_, opts?: { box?: string }) {
    this._targets.set(el, { box: opts?.box ?? "content-box", last: null });
    resizeObservers.add(this);
  }
  unobserve(el: Element_) {
    this._targets.delete(el);
  }
  disconnect() {
    this._targets.clear();
    resizeObservers.delete(this);
  }
}

function sizes(el: Element_) {
  const b = el === body || el === html ? [0, 0, S.vw, S.vh, 0, 0, S.vw, S.vh, 0, 0, 0, 0, 0, 0, 0, 0, 0] : el._box();
  if (!b || computedOf(el).display === "none") return { border: [0, 0], content: [0, 0], pad: [0, 0] };
  return { border: [b[2], b[3]], content: [Math.max(0, b[2] - b[8] - b[10] - b[12] - b[14]), Math.max(0, b[3] - b[9] - b[11] - b[13] - b[15])], pad: [b[8], b[9]] };
}

function checkResize() {
  for (const ro of [...resizeObservers]) {
    const entries: any[] = [];
    for (const [el, t] of ro._targets) {
      const s = sizes(el);
      const want = t.box === "border-box" ? s.border : s.content;
      if (t.last && t.last[0] === want[0] && t.last[1] === want[1]) continue;
      t.last = [want[0], want[1]];
      entries.push({
        target: el,
        contentRect: new DOMRectReadOnly_(s.pad[0], s.pad[1], s.content[0], s.content[1]),
        borderBoxSize: [{ inlineSize: s.border[0], blockSize: s.border[1] }],
        contentBoxSize: [{ inlineSize: s.content[0], blockSize: s.content[1] }],
        devicePixelContentBoxSize: [{ inlineSize: s.content[0] * S.dpr, blockSize: s.content[1] * S.dpr }],
      });
    }
    if (entries.length) run(() => ro._cb(entries, ro));
  }
}

const intersectionObservers = new Set<IntersectionObserver_>();

export class IntersectionObserver_ {
  root: Element_ | Document_ | null;
  rootMargin: string;
  thresholds: number[];
  _targets = new Map<Element_, { idx: number; hit: boolean } | null>();
  constructor(
    public _cb: (entries: any[], o: IntersectionObserver_) => void,
    opts: any = {},
  ) {
    this.root = opts.root ?? null;
    this.rootMargin = opts.rootMargin ?? "0px 0px 0px 0px";
    const t = opts.threshold ?? 0;
    this.thresholds = (Array.isArray(t) ? t : [t]).map(Number).sort((a, b) => a - b);
  }
  observe(el: Element_) {
    if (!this._targets.has(el)) this._targets.set(el, null);
    intersectionObservers.add(this);
  }
  unobserve(el: Element_) {
    this._targets.delete(el);
  }
  disconnect() {
    this._targets.clear();
    intersectionObservers.delete(this);
  }
  takeRecords() {
    return [];
  }
}

function margins(m: string, w: number, h: number): number[] {
  const p = m.trim().split(/\s+/).map((x, i) => (x.endsWith("%") ? (parseFloat(x) / 100) * (i % 2 ? w : h) : parseFloat(x) || 0));
  const [t, r = t, b = t, l = r] = p;
  return [t, r, b, l];
}

function rectOf(el: Element_ | Document_ | null): number[] {
  if (!el || el === doc || el === html || el === body) return [0, 0, S.vw, S.vh];
  const r = (el as Element_).getBoundingClientRect();
  return [r.left, r.top, r.right, r.bottom];
}

function checkIntersect() {
  for (const io of [...intersectionObservers]) {
    const entries: any[] = [];
    for (const [el, was] of io._targets) {
      const r = el.getBoundingClientRect();
      const t = [r.left, r.top, r.right, r.bottom];
      const root = rectOf(io.root);
      const m = margins(io.rootMargin, root[2] - root[0], root[3] - root[1]);
      let box = [root[0] - m[3], root[1] - m[0], root[2] + m[1], root[3] + m[2]];
      const rootBounds = box.slice();
      // clipped by each scroller between the target and the root
      for (let p = el.parentNode; p instanceof Element_ && p !== io.root && p !== body; p = p.parentNode) {
        const c = computedOf(p);
        if (c.overflowX !== "visible" || c.overflowY !== "visible") {
          const pr = rectOf(p);
          box = [Math.max(box[0], pr[0]), Math.max(box[1], pr[1]), Math.min(box[2], pr[2]), Math.min(box[3], pr[3])];
        }
      }
      const connected = el.isConnected && !!el._box();
      const ix = [Math.max(t[0], box[0]), Math.max(t[1], box[1]), Math.min(t[2], box[2]), Math.min(t[3], box[3])];
      const isHit = connected && ix[2] >= ix[0] && ix[3] >= ix[1];
      const area = (t[2] - t[0]) * (t[3] - t[1]);
      const ratio = !isHit ? 0 : area > 0 ? ((ix[2] - ix[0]) * (ix[3] - ix[1])) / area : 1;
      const idx = isHit ? io.thresholds.filter((x) => ratio >= x).length : -1;
      if (was && was.idx === idx && was.hit === isHit) continue;
      io._targets.set(el, { idx, hit: isHit });
      entries.push({
        target: el,
        time: g.performance.now(),
        rootBounds: io.root ? DOMRectReadOnly_.fromRect({ x: rootBounds[0], y: rootBounds[1], width: rootBounds[2] - rootBounds[0], height: rootBounds[3] - rootBounds[1] }) : new DOMRectReadOnly_(0, 0, S.vw, S.vh),
        boundingClientRect: r,
        intersectionRect: isHit ? new DOMRectReadOnly_(ix[0], ix[1], ix[2] - ix[0], ix[3] - ix[1]) : new DOMRectReadOnly_(0, 0, 0, 0),
        isIntersecting: isHit,
        intersectionRatio: ratio,
      });
    }
    if (entries.length) run(() => io._cb(entries, io));
  }
}

// ---------------------------------------------------------------- matchMedia

const mqls = new Set<any>();

function evalMedia(q: string): boolean {
  return q.split(",").some((part) => {
    let p = part.trim().toLowerCase();
    let neg = false;
    if (p.startsWith("not ")) {
      neg = true;
      p = p.slice(4);
    }
    p = p.replace(/^only\s+/, "");
    const ok = p.split(/\s+and\s+/).every((t) => {
      t = t.trim();
      if (t === "all" || t === "screen") return true;
      if (t === "print" || t === "speech") return false;
      const m = /^\(\s*([a-z-]+)\s*(?::\s*([^)]+))?\)$/.exec(t);
      if (!m) return false;
      const [, f, v = ""] = m;
      const n = parseFloat(v) * (v.endsWith("em") || v.endsWith("rem") ? 16 : 1);
      switch (f) {
        case "min-width":
          return S.vw >= n;
        case "max-width":
          return S.vw <= n;
        case "min-height":
          return S.vh >= n;
        case "max-height":
          return S.vh <= n;
        case "width":
          return S.vw === n;
        case "orientation":
          return v.trim() === (S.vw >= S.vh ? "landscape" : "portrait");
        case "prefers-color-scheme":
          return v.trim() === (S.dark ? "dark" : "light");
        case "prefers-reduced-motion":
        case "prefers-reduced-transparency":
        case "prefers-reduced-data":
          return v.trim() === "no-preference";
        case "prefers-contrast":
          return v.trim() === "no-preference";
        case "forced-colors":
          return v.trim() === "none";
        case "hover":
        case "any-hover":
          return v === "" || v.trim() === "hover";
        case "pointer":
        case "any-pointer":
          return v === "" || v.trim() === "fine";
        case "display-mode":
          return v.trim() === "browser";
        case "min-resolution":
          return S.dpr >= parseFloat(v);
        case "-webkit-min-device-pixel-ratio":
          return S.dpr >= parseFloat(v);
        default:
          return false;
      }
    });
    return neg ? !ok : ok;
  });
}

export function matchMedia(q: string) {
  const mql: any = new EventTarget_();
  mql.media = q;
  mql._was = evalMedia(q);
  Object.defineProperty(mql, "matches", { get: () => evalMedia(q) });
  mql.onchange = null;
  mql.addListener = (fn: any) => mql.addEventListener("change", fn);
  mql.removeListener = (fn: any) => mql.removeEventListener("change", fn);
  mqls.add(mql);
  return mql;
}

// ---------------------------------------------------------------- the frame: snapshot, observers, rAF

export const scrolling = new Map<Element_, number>();

let rafSeq = 1;

let rafs = new Map<number, (t: number) => void>();

export function requestAnimationFrame(fn: (t: number) => void): number {
  const id = rafSeq++;
  rafs.set(id, fn);
  return id;
}

export function cancelAnimationFrame(id: number) {
  rafs.delete(id);
}

/** A harness's clock of frames (sdk/ui/parity app.rs, `globalThis.__FRAME_CLOCK = {step, cap}` before the bundle):
 *  performance.now() is `step` ms a frame from the first frame that runs animation callbacks, held at `cap` ms, as
 *  the harness's Chrome page counts it, so a game's clock reads the same on both. Off unless the harness sets it. */
const frameClock = (g as { __FRAME_CLOCK?: { step: number; cap: number } }).__FRAME_CLOCK;

let frameClockFrom = -1;

if (frameClock && g.performance) {
  const t0 = g.performance.now();
  const now = () => t0 + (frameClockFrom < 0 ? 0 : Math.min((S.frameNo - frameClockFrom) * frameClock.step, frameClock.cap));
  Object.defineProperty(g.performance, "now", { value: now, configurable: true, writable: true });
}

/** The ops written when the page was last laid out (a frame's snapshot, or forceLayout). */
let laidAt = -1;

/** A box script reads that the last layout doesn't have yet, laid out now, as Chrome lays out on the read: the host's
 *  layout.now (runtime/dom computed.rs) commits what's written, lays the page out and gives its snapshot, whose boxes
 *  replace the frame's; the viewport and the observers wait for the frame. Once per change: a box that still isn't
 *  there (display: none) isn't asked for again until something else is written. Off where there's no such call. */
let noLayoutNow = false;
export function forceLayout(): boolean {
  if (noLayoutNow || laidAt === wire.writes()) return false;
  let json: string | undefined;
  try {
    json = wire.callSync<string>("layout.now");
  } catch (e) {
    // a host without it (bevy_ui's) never will have it; one outside a step may next time
    if (/no synchronous host call/.test(String(e))) noLayoutNow = true;
    json = undefined;
  }
  laidAt = wire.writes();
  if (!json) return false;
  const s = JSON.parse(json) as { n: number[] };
  geo.clear();
  for (let i = 0; i + STRIDE <= s.n.length; i += STRIDE) geo.set(s.n[i], s.n.slice(i + 1, i + STRIDE));
  return true;
}

/** The host's call after each frame's layout (runtime/js/src/dom.rs): the snapshot when it changed ("" when
 *  it didn't), then resize, matchMedia changes, the observers and the frame's animation callbacks. */
export function onLayout(json: string) {
  laidAt = wire.writes();
  S.frameNo++;
  S.wantFocus = undefined;
  let resized = false;
  let rescheme = false;
  if (json) {
    const s = JSON.parse(json) as { vw: number; vh: number; dpr: number; dark?: boolean; n: number[] };
    resized = s.vw !== S.vw || s.vh !== S.vh;
    rescheme = !!s.dark !== S.dark;
    S.dark = !!s.dark;
    S.vw = s.vw;
    S.vh = s.vh;
    S.dpr = s.dpr;
    geo.clear();
    for (let i = 0; i + STRIDE <= s.n.length; i += STRIDE) geo.set(s.n[i], s.n.slice(i + 1, i + STRIDE));
  }
  if (resized) run(() => dispatch(g, new UIEvent_("resize", { view: g }), true));
  if (resized || rescheme) {
    for (const m of mqls) {
      const now = evalMedia(m.media);
      if (now !== m._was) {
        m._was = now;
        const ev = new Event_("change") as any;
        ev.matches = now;
        ev.media = m.media;
        // (its onchange too, as dispatch calls a target's handler property)
        run(() => dispatch(m, ev, true));
      }
    }
  }
  if (json) {
    checkResize();
    checkIntersect();
  }
  // scrollend: a scroller that moved last frame and not this one
  for (const [el, f] of scrolling)
    if (f < S.frameNo - 1) {
      scrolling.delete(el);
      run(() => dispatchDirect(el, new Event_("scrollend", { bubbles: false })));
    }
  if (rafs.size) {
    if (frameClock && frameClockFrom < 0) frameClockFrom = S.frameNo;
    const due = rafs;
    rafs = new Map();
    const now = g.performance.now();
    run(() => due.forEach((fn) => fn(now)));
  }
  if (scanning) reportMisses();
}
