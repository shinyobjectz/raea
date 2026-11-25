// The layout snapshot, the runner, the API scan, install (window and document) and an HTML page: what the parts share.
import * as wire from "../wire";
import { canvasGlobals } from "../canvas";
import { AnimationEvent_, ClipboardEvent_, FontFace_, Comment_, CompositionEvent_, CustomEvent_, DOMRectReadOnly_, DOMRect_, DocumentFragment_, Document_, DragEvent_, Element_, EventTarget_, Event_, FocusEvent_, HTMLAnchorElement_, HTMLBodyElement_, HTMLButtonElement_, HTMLCanvasElement_, HTMLDialogElement_, HTMLDivElement_, HTMLElement_, HTMLFormElement_, HTMLHeadElement_, HTMLHtmlElement_, HTMLIFrameElement_, HTMLImageElement_, HTMLInputElement_, HTMLLIElement_, HTMLLabelElement_, HTMLMediaElement_, HTMLOptionElement_, HTMLSelectElement_, HTMLSpanElement_, HTMLStyleElement_, HTMLTextAreaElement_, HTMLUListElement_, InputEvent_, IntersectionObserver_, KeyboardEvent_, MediaNews, MouseEvent_, MutationObserver_, NodeFilter_, Node_, DOMParser_, Range_, Selection_, CaretPosition_, markEditing, resetSelection, selection, PointerEvent_, ResizeObserver_, SVGElement_, SVGGeometryElement_, SVGSVGElement_, ShadowRoot_, SubmitEvent_, Text_, TouchEvent_, TransitionEvent_, TreeWalker_, UIEvent_, WheelEvent_, addListener, cancelAnimationFrame, cssEscape, dispatch, fontWaits, getComputedStyle, heard, key, keyUp, makeElement, matchMedia, materialize, mediaNews, mirrorPlace, mirrorRemove, onLayout, plainClasses, query, reactContainer, removeListener, requestAnimationFrame, syncRoot, wireOf } from "./all";

export const g = globalThis as any;

// ---------------------------------------------------------------- layout snapshot

/** id -> x, y, w, h, scrollLeft, scrollTop, scrollWidth, scrollHeight, padding l t r b, border l t r b, z */
export const geo = new Map<number, number[]>();

export const STRIDE = 18;

// the viewport before the first snapshot: what the host knew as the bundle loaded (raea-runtime-js's __ui_viewport), so
// innerWidth and media queries read in a first render are the page's
const vp0 = (globalThis as { __ui_viewport?: { w: number; h: number; dpr: number } }).__ui_viewport;

/** the module's state its files assign (each field was a top-level let) */ export const S: { vw: number; vh: number; dpr: number; dark: boolean; frameNo: number; pointerCapture: Element_ | null; active: Element_ | null; wantFocus: Element_ | null | undefined; downTarget: Element_ | null; spaceDown: { t: Element_; ok: boolean } | null; focusVisible: boolean } = { vw: vp0?.w ?? 0, vh: vp0?.h ?? 0, dpr: vp0?.dpr ?? 1, dark: false, frameNo: 0, pointerCapture: null, active: null, wantFocus: undefined, downTarget: null, spaceDown: null, focusVisible: false };

// ---------------------------------------------------------------- the runner (the mounted driver's batch)

let runner: (fn: () => void) => void = (fn) => fn();

/** How callbacks the DOM layer makes on its own (observers, rAF, host events) run: React's flushSync, so what they
 *  set commits before the next event, as React DOM's discrete events do. */
export function setRunner(r: (fn: () => void) => void) {
  runner = r;
}

export function run(fn: () => void) {
  runner(() => {
    try {
      fn();
    } catch (e) {
      report(e);
    }
  });
}

export function report(e: unknown) {
  console.error(`[dom] ${(e as Error)?.stack ?? e}`);
}

// ---------------------------------------------------------------- the API scan (the Proxy instrumentation)

export const misses = new Map<string, number>();

let missReported = 0;

const IGNORE = new Set(["then", "toJSON", "$$typeof", "constructor", "asymmetricMatch", "nodeType", "__proto__", "_reactListening", "@@iterator", "prototype"]);

function scanProxy(kind: string): object {
  const who = (r: any) => {
    try {
      return r === g ? "window" : String(r?.constructor?.name ?? kind).replace(/_$/, "");
    } catch {
      return kind;
    }
  };
  return new Proxy(Object.create(null), {
    get(_t, p, r) {
      const k = `${who(r)}.${String(p)}`;
      if (typeof p === "string" && !IGNORE.has(p) && !p.startsWith("__")) misses.set(k, (misses.get(k) ?? 0) + 1);
      return undefined;
    },
    has(_t, p) {
      if (typeof p === "string" && !IGNORE.has(p) && !p.startsWith("__")) misses.set(`${kind}.${p} (in)`, (misses.get(`${kind}.${p} (in)`) ?? 0) + 1);
      return false;
    },
  });
}

/** The properties read on host elements, events, document and window that the surface doesn't have (scan mode). */
export function apiMisses(): Record<string, number> {
  return Object.fromEntries([...misses].sort());
}

export function reportMisses() {
  if (misses.size === missReported) return;
  missReported = misses.size;
  console.warn(`[dom-miss] ${JSON.stringify(apiMisses())}`);
}

// ---------------------------------------------------------------- install: window and document

export let installed = false;

export let scanning = false;

export let doc: Document_;

export let html: Element_;

export let head: Element_;

export let body: Element_;

/** The React root container: the host's root (wire id 0), as `<div id="root">` in body. */
export let rootContainer: Element_;

// ---------------------------------------------------------------- an HTML page (src/html.ts)

/** On for an HTML page: its html and body are real elements on the host, its classes stay off the wire. */
export let htmlDoc = false;


export const HTML_HIDDEN = new Set(["script", "style", "link", "meta", "title", "noscript", "template", "head", "base"]);

/** A value an HTML page's script or sheet wrote, as the host's style reads it: custom properties (substituted already,
 *  src/html.ts) and the individual transforms' `none` dropped, GSAP's 3D forms of 2D transforms (translate3d with z 0,
 *  rotateZ, scale3d) as their 2D ones. */
export function htmlValue(k: string, v: unknown): unknown {
  if (k.startsWith("--")) return undefined;
  if (typeof v !== "string") return v;
  const t = v.trim();
  if ((k === "translate" || k === "rotate" || k === "scale") && t === "none") return undefined;
  // a bare number as a number (the host reads a string as a class: opacity "0" from a sheet would be lost)
  if (/^[-+]?(\d+\.?\d*|\.\d+)(e[-+]?\d+)?$/i.test(t)) return Number(t);
  if (k === "boxShadow" && t.includes(",")) {
    // the list's empty shadows dropped (Tailwind 4 stacks `0 0 #0000` placeholders before the real one)
    const parts: string[] = [];
    let depth = 0;
    let last = 0;
    for (let i = 0; i < t.length; i++) {
      if (t[i] === "(") depth++;
      else if (t[i] === ")") depth--;
      else if (t[i] === "," && depth === 0) {
        parts.push(t.slice(last, i).trim());
        last = i + 1;
      }
    }
    parts.push(t.slice(last).trim());
    const real = parts.filter((p) => !/^(0(px)?\s+){1,3}0(px)?\s+(#0000|#00000000|transparent)$/i.test(p) && !/^(#0000|#00000000|transparent)(\s+0(px)?){2,4}$/i.test(p));
    return real.length ? real.join(", ") : "none";
  }
  if (k === "transform") {
    return t
      .replace(/translate3d\(\s*([^,]+),\s*([^,]+),\s*0(?:px)?\s*\)/g, "translate($1, $2)")
      .replace(/rotateZ\(/g, "rotate(")
      .replace(/scale3d\(\s*([^,]+),\s*([^,]+),\s*1\s*\)/g, "scale($1, $2)")
      .replace(/\s+/g, " ")
      .trim();
  }
  return t;
}

/** A transform list as Chrome's getComputedStyle gives it: `matrix(a, b, c, d, e, f)`, or `none` (GSAP parses this
 *  to start a tween from the element's own transform). Percentages of translate are of the border box `box`. */
export function cssMatrix(v: unknown, box: number[] | undefined): string {
  if (v === undefined || v === null || v === "" || v === "none") return "none";
  let m = [1, 0, 0, 1, 0, 0];
  const mul = (n: number[]) => {
    const [a, b, c, d, e, f] = m;
    m = [a * n[0] + c * n[1], b * n[0] + d * n[1], a * n[2] + c * n[3], b * n[2] + d * n[3], a * n[4] + c * n[5] + e, b * n[4] + d * n[5] + f];
  };
  const len = (x: string, of: number) => (x.trim().endsWith("%") ? (parseFloat(x) / 100) * of : parseFloat(x) || 0);
  const ang = (x: string) => {
    const n = parseFloat(x) || 0;
    return x.endsWith("grad") ? (n * Math.PI) / 200 : x.endsWith("rad") ? n : x.endsWith("turn") ? n * 2 * Math.PI : (n * Math.PI) / 180;
  };
  const w = box?.[2] ?? 0;
  const h = box?.[3] ?? 0;
  for (const f of String(v).matchAll(/([a-zA-Z0-9]+)\(([^)]*)\)/g)) {
    const name = f[1];
    const a = f[2].split(",").map((x) => x.trim());
    switch (name) {
      case "matrix":
        mul(a.map(Number));
        break;
      case "translate":
      case "translate3d":
        mul([1, 0, 0, 1, len(a[0], w), len(a[1] ?? "0", h)]);
        break;
      case "translateX":
        mul([1, 0, 0, 1, len(a[0], w), 0]);
        break;
      case "translateY":
        mul([1, 0, 0, 1, 0, len(a[0], h)]);
        break;
      case "scale":
      case "scale3d": {
        const x = parseFloat(a[0]);
        const y = a[1] !== undefined ? parseFloat(a[1]) : x;
        mul([x, 0, 0, y, 0, 0]);
        break;
      }
      case "scaleX":
        mul([parseFloat(a[0]), 0, 0, 1, 0, 0]);
        break;
      case "scaleY":
        mul([1, 0, 0, parseFloat(a[0]), 0, 0]);
        break;
      case "rotate":
      case "rotateZ": {
        const r = ang(a[0]);
        mul([Math.cos(r), Math.sin(r), -Math.sin(r), Math.cos(r), 0, 0]);
        break;
      }
      case "skew":
        mul([1, Math.tan(ang(a[1] ?? "0")), Math.tan(ang(a[0])), 1, 0, 0]);
        break;
      case "skewX":
        mul([1, 0, Math.tan(ang(a[0])), 1, 0, 0]);
        break;
      case "skewY":
        mul([1, Math.tan(ang(a[0])), 0, 1, 0, 0]);
        break;
      default:
        break;
    }
  }
  const r = (x: number) => String(Math.round(x * 1e6) / 1e6);
  return `matrix(${m.map(r).join(", ")})`;
}

/** An element's style as the host takes it: its cascaded declarations, then its own, then the cascade's important. */
export function effStyle(el: Element_): Record<string, unknown> {
  if (!el._sheet && !el._sheetImportant) return el._style;
  return { ...el._sheet, ...el._style, ...el._sheetImportant };
}

/** An HTML page's document (docs/ts-native.feature P27): html and body go on the host under its root (html as the
 *  root's only child, body in it), the React root container leaves body, and classes are no longer sent. */
export function adoptDocument() {
  install();
  htmlDoc = true;
  mirrorRemove(body, rootContainer);
  // html on the host without head (what goes in head is never laid out), body in it
  html.__wid = wire.allocId();
  const w = wireOf("html", { ...html._props, ...html._extra }, html);
  wire.create(html.__wid, w.kind, w.props);
  html._wired = true;
  wire.append(0, html.__wid);
  materialize(body);
  wire.append(html.__wid, body.__wid!);
}

/** An element's cascaded declarations (camelCase keys), as this cascade has them: what getComputedStyle answers where
 *  the host has no answer, and what an HTML page's timing and text read. The host cascades the page itself (stylo), so
 *  they're never sent. */
export function setSheet(el: Element_, normal: Record<string, unknown> | undefined, important: Record<string, unknown> | undefined) {
  el._sheet = normal && Object.keys(normal).length ? normal : undefined;
  el._sheetImportant = important && Object.keys(important).length ? important : undefined;
}

/** The root container element for a mount at wire id `at` (0: the host's root, `<div id="root">` in body). */
export function container(at: number): Element_ {
  install();
  if (at === 0) return rootContainer;
  for (const n of query(doc, "*", false)) if (n.__wid === at) return n;
  // a core region's slot: an element of its own under the root
  const el = makeElement("div");
  el.__wid = at;
  el._wired = true;
  mirrorPlace(rootContainer, el, null);
  return el;
}

/** A fresh mount at the host's root (after the wire's clear): the root container and body emptied, the focus and
 *  pointer state dropped, and the root's events asked of the host again. */
export function resetMount() {
  install();
  for (const c of rootContainer._kids.slice()) mirrorRemove(rootContainer, c);
  for (const c of body._kids.slice()) if (c !== rootContainer) mirrorRemove(body, c);
  S.active = null;
  S.wantFocus = undefined;
  S.spaceDown = null;
  S.downTarget = S.pointerCapture = null;
  resetSelection();
  geo.clear();
  heard.delete(rootContainer);
  syncRoot();
}

/** The layout snapshot, as the host would push it (tests; the host calls `__ui_layout`). */
export const layout = (json: string) => onLayout(json);

/** A global as a browser has it (a host's own read-only global, as bun's navigator, replaced). */
function put(k: string, v: unknown) {
  try {
    Object.defineProperty(g, k, { value: v, writable: true, configurable: true, enumerable: false });
  } catch {
    try {
      g[k] = v;
    } catch {}
  }
}

const UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/152.0.0.0 Safari/537.36";

/** The navigator's words for the system raea's window runs on (globalThis.__raea_window.os, std's name, set before the
 *  page's bundle: raea engine/page/window chrome.rs); a Mac's when there's no window (a render). Until 0.1.8 a Windows
 *  PC's page said MacIntel, and a library that picks ⌘ or Ctrl by navigator.platform (CodeMirror's and ProseMirror's
 *  Mod-) took Ctrl+Z, Ctrl+C and Ctrl+V as a Mac's. Read when asked, so the window's word, set later than this, counts. */
export function navigatorSystem(os = (globalThis as { __raea_window?: { os?: string } }).__raea_window?.os): { userAgent: string; platform: string; data: string } {
  if (os === "windows") return { userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/152.0.0.0 Safari/537.36", platform: "Win32", data: "Windows" }
  if (os === "linux") return { userAgent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/152.0.0.0 Safari/537.36", platform: "Linux x86_64", data: "Linux" }
  return { userAgent: UA, platform: "MacIntel", data: "macOS" }
}

/** window and document as globals (window is the global object, as in a browser), the element and event
 *  classes, the observers, matchMedia, rAF and getComputedStyle. Must run before a library's modules evaluate
 *  (they check `typeof document` once): import "@raea/ui/dom-globals" first. With globalThis.__UI_DOM_SCAN set,
 *  every property read that misses on an element, event, document or window is counted (apiMisses()). */
export function install() {
  if (installed) return;
  installed = true;
  scanning = !!g.__UI_DOM_SCAN;
  doc = new Document_();
  html = makeElement("html");
  head = makeElement("head");
  body = makeElement("body");
  rootContainer = makeElement("div");
  rootContainer.__wid = 0;
  rootContainer._wired = true;
  rootContainer._attrs.set("id", "root");
  html.parentNode = doc;
  doc._kids.push(html);
  html._kids.push(head, body);
  head.parentNode = html;
  body.parentNode = html;
  body._kids.push(rootContainer);
  rootContainer.parentNode = body;

  const classes: Record<string, unknown> = {
    EventTarget: EventTarget_, Event: Event_, CustomEvent: CustomEvent_, UIEvent: UIEvent_, FocusEvent: FocusEvent_, MouseEvent: MouseEvent_, PointerEvent: PointerEvent_,
    WheelEvent: WheelEvent_, KeyboardEvent: KeyboardEvent_, InputEvent: InputEvent_, SubmitEvent: SubmitEvent_, AnimationEvent: AnimationEvent_, TransitionEvent: TransitionEvent_,
    TouchEvent: TouchEvent_, CompositionEvent: CompositionEvent_, ClipboardEvent: ClipboardEvent_, DragEvent: DragEvent_,
    Node: Node_, Element: Element_, HTMLElement: HTMLElement_, SVGElement: SVGElement_, SVGSVGElement: SVGSVGElement_, SVGGeometryElement: SVGGeometryElement_, Text: Text_, Comment: Comment_, CharacterData: Text_, Document: Document_, HTMLDocument: Document_,
    DocumentFragment: DocumentFragment_, ShadowRoot: ShadowRoot_, HTMLInputElement: HTMLInputElement_, HTMLTextAreaElement: HTMLTextAreaElement_, HTMLButtonElement: HTMLButtonElement_,
    HTMLSelectElement: HTMLSelectElement_, HTMLOptionElement: HTMLOptionElement_, HTMLFormElement: HTMLFormElement_, HTMLAnchorElement: HTMLAnchorElement_, HTMLLabelElement: HTMLLabelElement_,
    HTMLDivElement: HTMLDivElement_, HTMLSpanElement: HTMLSpanElement_, HTMLUListElement: HTMLUListElement_, HTMLLIElement: HTMLLIElement_, HTMLImageElement: HTMLImageElement_, HTMLMediaElement: HTMLMediaElement_, HTMLVideoElement: HTMLMediaElement_, HTMLAudioElement: HTMLMediaElement_, HTMLStyleElement: HTMLStyleElement_,
    HTMLBodyElement: HTMLBodyElement_, HTMLHtmlElement: HTMLHtmlElement_, HTMLHeadElement: HTMLHeadElement_, HTMLIFrameElement: HTMLIFrameElement_, HTMLDialogElement: HTMLDialogElement_, HTMLCanvasElement: HTMLCanvasElement_, ...canvasGlobals(),
    DOMParser: DOMParser_, Range: Range_, Selection: Selection_, CaretPosition: CaretPosition_,
    DOMRect: DOMRect_, DOMRectReadOnly: DOMRectReadOnly_, NodeFilter: NodeFilter_, TreeWalker: TreeWalker_,
    MutationObserver: MutationObserver_, ResizeObserver: ResizeObserver_, IntersectionObserver: IntersectionObserver_,
    FontFace: FontFace_,
    ...plainClasses,
  };
  for (const [k, v] of Object.entries(classes)) {
    put(k, v);
    // Chrome's class names (`el.constructor.name`)
    try {
      Object.defineProperty(v as object, "name", { value: k });
    } catch {}
  }
  put("window", g);
  put("self", g);
  put("top", g);
  put("parent", g);
  put("frames", g);
  put("frameElement", null);
  put("document", doc);
  put("addEventListener", (t: string, fn: any, o?: any) => addListener(g, String(t), fn, o));
  put("removeEventListener", (t: string, fn: any, o?: any) => removeListener(g, String(t), fn, o));
  put("dispatchEvent", (ev: Event_) => dispatch(g, ev, false));
  put("getComputedStyle", getComputedStyle);
  put("matchMedia", matchMedia);
  put("requestAnimationFrame", requestAnimationFrame);
  put("cancelAnimationFrame", cancelAnimationFrame);
  g.requestIdleCallback ??= (fn: (d: unknown) => void) => setTimeout(() => fn({ didTimeout: false, timeRemaining: () => 0 }), 1);
  g.cancelIdleCallback ??= (id: number) => clearTimeout(id);
  put("getSelection", () => selection);
  put("scrollTo", () => {});
  put("scrollBy", () => {});
  put("scroll", () => {});
  put("focus", () => {});
  put("blur", () => {});
  put("alert", () => {});
  put("open", () => null);
  for (const [k, get] of [
    ["innerWidth", () => S.vw],
    ["innerHeight", () => S.vh],
    ["outerWidth", () => S.vw],
    ["outerHeight", () => S.vh],
    ["devicePixelRatio", () => S.dpr],
    ["scrollX", () => 0],
    ["scrollY", () => 0],
    ["pageXOffset", () => 0],
    ["pageYOffset", () => 0],
    ["screenX", () => 0],
    ["screenY", () => 0],
  ] as [string, () => number][])
    Object.defineProperty(g, k, { get, configurable: true });
  const orientation = Object.assign(new EventTarget_(), { type: "landscape-primary", angle: 0, onchange: null });
  put("screen", Object.assign(new EventTarget_(), { width: 1920, height: 1080, availWidth: 1920, availHeight: 1080, availLeft: 0, availTop: 0, colorDepth: 24, pixelDepth: 24, orientation }));
  const vv: any = new EventTarget_();
  Object.defineProperties(vv, { width: { get: () => S.vw }, height: { get: () => S.vh }, offsetLeft: { value: 0 }, offsetTop: { value: 0 }, pageLeft: { value: 0 }, pageTop: { value: 0 }, scale: { value: 1 } });
  put("visualViewport", vv);
  put("navigator", {
    get userAgent() {
      return navigatorSystem().userAgent;
    },
    get platform() {
      return navigatorSystem().platform;
    },
    // the web's own fields, as a browser has them, on one line (taxonomy.sh S3b reads a line that starts with a
    // processor's name as a raea prop)
    vendor: "Google Inc.", language: "en-US", languages: ["en-US", "en"], maxTouchPoints: 0, onLine: true, cookieEnabled: true, hardwareConcurrency: 8,
    get userAgentData() {
      return { brands: [{ brand: "HeadlessChrome", version: "152" }], mobile: false, platform: navigatorSystem().data };
    },
    clipboard: { writeText: () => Promise.resolve(), readText: () => Promise.resolve("") },
  });
  g.location ??= { href: "http://localhost/", origin: "http://localhost", protocol: "http:", host: "localhost", hostname: "localhost", port: "", pathname: "/", search: "", hash: "", assign() {}, replace() {}, reload() {} };
  put("CSS", { supports: (p: string, v?: string) => (v === undefined ? false : !/^-webkit-backdrop-filter$/.test(p) && /^[a-z-]+$/.test(p)), escape: cssEscape });
  g.AbortController ??= class {
    signal: any = Object.assign(new EventTarget_(), { aborted: false, reason: undefined, throwIfAborted() {} });
    abort(reason?: unknown) {
      if (this.signal.aborted) return;
      this.signal.aborted = true;
      this.signal.reason = reason;
      dispatch(this.signal, new Event_("abort"), true);
    }
  };
  g.AbortSignal ??= g.AbortController;
  put("onscrollend", null);
  // react-hook-form's checks (`instanceof FileList`); there are no file inputs here
  put("FileList", class FileList {
    length = 0;
    item() {
      return null;
    }
  });
  g.__ui_layout = onLayout;
  // contenteditable edits as Chrome's (editing.ts, P14e): a rich-text editor (Tiptap) can rely on this
  markEditing();
  if (scanning) {
    Object.setPrototypeOf(EventTarget_.prototype, scanProxy("EventTarget"));
    Object.setPrototypeOf(Event_.prototype, scanProxy("Event"));
    Object.setPrototypeOf(DOMRectReadOnly_.prototype, scanProxy("DOMRect"));
    try {
      Object.setPrototypeOf(Object.getPrototypeOf(g) === Object.prototype ? g : Object.getPrototypeOf(g), scanProxy("window"));
    } catch {}
  }
  // a key the focused element doesn't hear (or with nothing focused) comes as the host's own keydown: to body
  wire.on("keydown", (v) => run(() => key(S.active ?? body, v)));
  wire.on("keyup", (v) => run(() => keyUp(S.active ?? body, v)));
  // an img's, video's or audio's source loaded, decoded or seeked (runtime/js src/media_html.rs)
  wire.on("media", (v) => run(() => mediaNews(v as MediaNews & { id: number })));
  // document.fonts: the host's word that no @font-face face is still loading (engine/page host.rs)
  wire.on("fonts.ready", () => run(() => fontWaits.splice(0).forEach((done) => done())));
  // the host's events for this root are Chrome's from now on (runtime/js host.rs: default actions after the
  // handlers, keyup, a secondary press's pointerdown, moves on hover)
  wire.emit("dom", true);
  // the root hears what the document needs from the start (a press anywhere ends a focus, starts a click)
  syncRoot();
  // React's handlers on the root container run from its listeners, in both phases
  reactContainer(rootContainer);
  trackFocusVisible();
  // raea-page's <iframe> documents (RAEA_PAGE_IFRAMES, docs/blitz-bevy.feature B5c): a frame's own window (its URL,
  // parent and top, origin) set over what's installed here, before the app's modules run; nothing without the global
  g.__raea_frame?.installed?.();
}

/** :focus-visible: set by keys, cleared by a press (Chrome's heuristic) */
function trackFocusVisible() {
  addListener(doc, "keydown", () => (S.focusVisible = true), true, true);
  addListener(doc, "pointerdown", () => (S.focusVisible = false), true, true);
}
