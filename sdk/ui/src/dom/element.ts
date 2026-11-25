// Element: attributes, classes, geometry from the layout snapshot, focus and pointer capture.
import { VOID } from "../html/parse";
import * as wire from "../wire";
import { ATTR_PROP, Comment_, DOMRect_, HTML_NS, editableChanged, forceLayout, setInnerHtml, Node_, S, body, computedOf, cssText, doc, geo, html, htmlDoc, installed, kebab, makeStyle, matchesSelector, mutated, nodeList, parseCss, parseSelector, pointerEventsOf, query, tellCapture, wireStyle } from "./all";

// ---------------------------------------------------------------- elements

/** void's own host kinds: what a React tag names on the wire when it isn't an HTML tag */
export const FOCUSABLE_TAGS = new Set(["button", "input", "select", "textarea"]);

export const TEXT_INPUTS = new Set(["", "text", "email", "password", "search", "tel", "url", "number"]);

let elementSeq = 0;

export class Element_ extends Node_ {
  nodeType = 1;
  localName: string;
  namespaceURI: string;
  /** React's props as last committed (or the attributes a JS-made element was given, as props) */
  _props: Record<string, unknown> = {};
  /** props JS set on it (attributes, scroll offsets, a field's value): React's props are frozen, these sit over them */
  _extra: Record<string, unknown> = {};
  /** scrollTop and scrollLeft values written to the host and not yet seen back in its scroll event (setScroll) */
  _ahead?: Record<string, number[]>;
  _attrs = new Map<string, string>();
  _style: Record<string, unknown> = {};
  _styleObj: any;
  /** an HTML page's cascaded declarations (src/html/cascade.ts), under `_style`; `!important` ones over it */
  _sheet?: Record<string, unknown>;
  _sheetImportant?: Record<string, unknown>;
  /** a text element's own string (React's shouldSetTextContent) */
  _inline: string | undefined;
  /** the fiber (React DOM's internal instance), for React's event paths through portals */
  _fiber: any;
  _seq = elementSeq++;
  constructor(tag: string, ns = HTML_NS) {
    super();
    this.localName = ns === HTML_NS ? tag.toLowerCase() : tag;
    this.namespaceURI = ns;
  }
  get tagName() {
    return this.namespaceURI === HTML_NS ? this.localName.toUpperCase() : this.localName;
  }
  get nodeName() {
    return this.tagName;
  }
  get prefix() {
    return null;
  }
  // ---- attributes
  get id() {
    return this._attrs.get("id") ?? "";
  }
  set id(v: string) {
    this.setAttribute("id", v);
  }
  get className(): string {
    return this._attrs.get("class") ?? "";
  }
  set className(v: string) {
    this.setAttribute("class", v);
  }
  get classList() {
    const el = this;
    const list = () => el.className.split(/\s+/).filter(Boolean);
    const write = (l: string[]) => el.setAttribute("class", l.join(" "));
    return {
      get length() {
        return list().length;
      },
      get value() {
        return el.className;
      },
      contains: (c: string) => list().includes(c),
      add: (...cs: string[]) => write([...new Set([...list(), ...cs])]),
      remove: (...cs: string[]) => write(list().filter((c) => !cs.includes(c))),
      toggle: (c: string, force?: boolean) => {
        const has = list().includes(c);
        const want = force ?? !has;
        if (want !== has) write(want ? [...list(), c] : list().filter((x) => x !== c));
        return want;
      },
      replace: (a: string, b: string) => {
        const l = list();
        const i = l.indexOf(a);
        if (i < 0) return false;
        l[i] = b;
        write(l);
        return true;
      },
      item: (i: number) => list()[i] ?? null,
      forEach: (fn: (c: string, i: number) => void) => list().forEach(fn),
      toString: () => el.className,
      [Symbol.iterator]: () => list()[Symbol.iterator](),
    };
  }
  get attributes() {
    const a = [...this._attrs].map(([name, value]) => ({ name, value, localName: name, nodeName: name, namespaceURI: null, prefix: null, specified: true, ownerElement: this }));
    return Object.assign(a, { getNamedItem: (n: string) => a.find((x) => x.name === n.toLowerCase()) ?? null, item: (i: number) => a[i] ?? null });
  }
  getAttribute(n: string): string | null {
    return this._attrs.get(this._an(n)) ?? null;
  }
  getAttributeNS(_ns: string | null, n: string) {
    return this.getAttribute(n);
  }
  hasAttribute(n: string) {
    return this._attrs.has(this._an(n));
  }
  hasAttributes() {
    return this._attrs.size > 0;
  }
  getAttributeNames() {
    return [...this._attrs.keys()];
  }
  _an(n: string) {
    return this.namespaceURI === HTML_NS ? String(n).toLowerCase() : String(n);
  }
  setAttribute(n: string, v: unknown) {
    const name = this._an(n);
    const value = String(v);
    const old = this._attrs.get(name) ?? null;
    if (name === "style") {
      this._replaceStyle(parseCss(value));
      return;
    }
    this._attrs.set(name, value);
    this._toWire(name, value);
    if (name === "contenteditable") editableChanged(this);
    mutated(this, { type: "attributes", attributeName: name, oldValue: old });
  }
  setAttributeNS(_ns: string | null, n: string, v: unknown) {
    this.setAttribute(n, v);
  }
  removeAttribute(n: string) {
    const name = this._an(n);
    if (!this._attrs.has(name)) return;
    const old = this._attrs.get(name)!;
    if (name === "style") {
      this._replaceStyle({});
      return;
    }
    this._attrs.delete(name);
    this._toWire(name, null);
    if (name === "contenteditable") editableChanged(this);
    mutated(this, { type: "attributes", attributeName: name, oldValue: old });
  }
  removeAttributeNS(_ns: string | null, n: string) {
    this.removeAttribute(n);
  }
  toggleAttribute(n: string, force?: boolean) {
    const has = this.hasAttribute(n);
    const want = force ?? !has;
    if (want && !has) this.setAttribute(n, "");
    if (!want && has) this.removeAttribute(n);
    return want;
  }
  /** An attribute set in JS, as the host's prop (its React name); the root's own props are the host's, not ours */
  _toWire(name: string, value: string | null) {
    const prop = ATTR_PROP[name] ?? name;
    if (value === null) delete this._extra[prop];
    else this._extra[prop] = prop === "tabIndex" ? Number(value) : value;
    if (!this._wired || this.__wid === undefined || this.__wid === 0) return;
    // an HTML page's classes are its stylesheets' (src/html/cascade.ts), not the host's Tailwind
    if (htmlDoc && prop === "className") return;
    if (value === null) wire.unset(this.__wid, prop);
    else wire.set(this.__wid, prop, prop === "tabIndex" ? Number(value) : value);
  }
  get dataset() {
    const el = this;
    return new Proxy({} as Record<string, string>, {
      get: (_t, p) => (typeof p === "string" ? (el.getAttribute(`data-${kebab(p)}`) ?? undefined) : undefined),
      set: (_t, p, v) => (typeof p === "string" && el.setAttribute(`data-${kebab(p)}`, v), true),
      deleteProperty: (_t, p) => (typeof p === "string" && el.removeAttribute(`data-${kebab(p)}`), true),
      has: (_t, p) => typeof p === "string" && el.hasAttribute(`data-${kebab(p)}`),
      ownKeys: () => el.getAttributeNames().filter((n) => n.startsWith("data-")).map((n) => n.slice(5).replace(/-([a-z])/g, (_m, c) => c.toUpperCase())),
      getOwnPropertyDescriptor: (_t, p) => (typeof p === "string" && el.hasAttribute(`data-${kebab(p)}`) ? { enumerable: true, configurable: true, value: el.getAttribute(`data-${kebab(p)}`) } : undefined),
    });
  }
  // ---- style
  get style(): any {
    return (this._styleObj ??= makeStyle(this));
  }
  set style(v: string) {
    this._replaceStyle(parseCss(String(v)));
  }
  _setStyle(k: string, v: unknown) {
    const next = { ...this._style };
    if (v === undefined) delete next[k];
    else next[k] = v;
    this._replaceStyle(next);
  }
  _replaceStyle(next: Record<string, unknown>) {
    const old = this._attrs.get("style") ?? null;
    this._style = next;
    const text = cssText(next);
    if (text) this._attrs.set("style", text);
    else this._attrs.delete("style");
    if (this._wired && this.__wid !== undefined && this.__wid !== 0) {
      const eff = this._style;
      if (Object.keys(eff).length) wire.set(this.__wid, "style", wireStyle(eff));
      else wire.unset(this.__wid, "style");
    }
    // body's and html's pointer-events reach the host's root (a modal's `body { pointer-events: none }`)
    if (installed && (this === body || this === html)) wire.emit("rootPointerEvents", pointerEventsOf(body));
    if (old !== (this._attrs.get("style") ?? null)) mutated(this, { type: "attributes", attributeName: "style", oldValue: old });
  }
  // ---- tree, elements only
  get children(): Element_[] {
    return nodeList(this._kids.filter((k): k is Element_ => k instanceof Element_));
  }
  get childElementCount() {
    return this.children.length;
  }
  get firstElementChild() {
    return this.children[0] ?? null;
  }
  get lastElementChild() {
    const c = this.children;
    return c[c.length - 1] ?? null;
  }
  get nextElementSibling(): Element_ | null {
    for (let n = this.nextSibling; n; n = n.nextSibling) if (n instanceof Element_) return n;
    return null;
  }
  get previousElementSibling(): Element_ | null {
    for (let n = this.previousSibling; n; n = n.previousSibling) if (n instanceof Element_) return n;
    return null;
  }
  get textContent(): string {
    if (this._inline !== undefined) return this._inline;
    return this._kids.map((k) => (k instanceof Comment_ ? "" : (k.textContent ?? ""))).join("");
  }
  set textContent(v: string | null) {
    if (this._inline !== undefined) {
      this._inline = String(v ?? "");
      if (this._wired && this.__wid !== undefined) wire.text(this.__wid, this._inline);
      return;
    }
    for (const k of this._kids.slice()) this.removeChild(k);
    if (v) this.appendChild(doc.createTextNode(String(v)));
  }
  get innerText() {
    return this.textContent;
  }
  set innerText(v: string) {
    this.textContent = v;
  }
  get innerHTML(): string {
    return this._kids.map((k) => (k instanceof Element_ ? k.outerHTML : escapeHtml(k.textContent ?? ""))).join("");
  }
  set innerHTML(v: string) {
    setInnerHtml(this, String(v ?? ""));
  }
  get outerHTML(): string {
    const attrs = [...this._attrs].map(([k, v]) => ` ${k}="${escapeHtml(v)}"`).join("");
    if (VOID.has(this.localName)) return `<${this.localName}${attrs}>`;
    return `<${this.localName}${attrs}>${this.innerHTML}</${this.localName}>`;
  }
  /** the element replaced by what the markup makes (nothing, for one that isn't in a tree) */
  set outerHTML(v: string) {
    if (!this.parentNode) return;
    const box = this.ownerDocument.createElement("div") as unknown as Element_;
    box.innerHTML = String(v ?? "");
    this.replaceWith(...([...box.childNodes] as any[]));
  }
  querySelector(s: string): Element_ | null {
    return query(this, s, true)[0] ?? null;
  }
  querySelectorAll(s: string): Element_[] {
    return nodeList(query(this, s, false));
  }
  getElementsByTagName(t: string) {
    return this.querySelectorAll(t === "*" ? "*" : t);
  }
  getElementsByClassName(c: string) {
    return this.querySelectorAll(c.split(/\s+/).filter(Boolean).map((x) => `.${x}`).join(""));
  }
  matches(s: string): boolean {
    return matchesSelector(this, parseSelector(s), this);
  }
  webkitMatchesSelector(s: string) {
    return this.matches(s);
  }
  closest(s: string): Element_ | null {
    const sel = parseSelector(s);
    for (let n: Node_ | null = this; n; n = n.parentNode) if (n instanceof Element_ && matchesSelector(n, sel, n)) return n;
    return null;
  }
  insertAdjacentElement(where: string, el: Element_) {
    const w = where.toLowerCase();
    if (w === "beforebegin") this.parentNode?.insertBefore(el, this);
    else if (w === "afterbegin") this.insertBefore(el, this.firstChild);
    else if (w === "beforeend") this.appendChild(el);
    else if (w === "afterend") this.parentNode?.insertBefore(el, this.nextSibling);
    return el;
  }
  insertAdjacentText(where: string, t: string) {
    const n = doc.createTextNode(t);
    const w = where.toLowerCase();
    if (w === "beforebegin") this.parentNode?.insertBefore(n, this);
    else if (w === "afterbegin") this.insertBefore(n, this.firstChild);
    else if (w === "beforeend") this.appendChild(n);
    else if (w === "afterend") this.parentNode?.insertBefore(n, this.nextSibling);
  }
  before(...ns: (Node_ | string)[]) {
    for (const n of ns) this.parentNode?.insertBefore(typeof n === "string" ? doc.createTextNode(n) : n, this);
  }
  after(...ns: (Node_ | string)[]) {
    const next = this.nextSibling;
    for (const n of ns) this.parentNode?.insertBefore(typeof n === "string" ? doc.createTextNode(n) : n, next);
  }
  replaceWith(...ns: (Node_ | string)[]) {
    const p = this.parentNode;
    if (!p) return;
    this.before(...ns);
    p.removeChild(this);
  }
  // ---- geometry
  _box(): number[] | undefined {
    if (this === body || this === html) return undefined;
    if (!(this._wired && this.__wid !== undefined && this.isConnected)) return undefined;
    // placed since the page was last laid out: laid out now, as Chrome does for the read (forceLayout)
    return geo.get(this.__wid) ?? (forceLayout() ? geo.get(this.__wid) : undefined);
  }
  getBoundingClientRect(): DOMRect_ {
    if (this === html || this === body) return new DOMRect_(0, 0, S.vw, this === html ? S.vh : S.vh);
    const b = this._box();
    return b ? new DOMRect_(b[0], b[1], b[2], b[3]) : new DOMRect_(0, 0, 0, 0);
  }
  getClientRects(): DOMRect_[] {
    const r = this.getBoundingClientRect();
    return nodeList(r.width || r.height || this._box() ? [r] : []);
  }
  get clientWidth() {
    if (this === html || this === body) return Math.round(S.vw);
    const b = this._box();
    return b ? Math.round(b[2] - b[12] - b[14]) : 0;
  }
  get clientHeight() {
    if (this === html || this === body) return Math.round(S.vh);
    const b = this._box();
    return b ? Math.round(b[3] - b[13] - b[15]) : 0;
  }
  get clientTop() {
    return Math.round(this._box()?.[13] ?? 0);
  }
  get clientLeft() {
    return Math.round(this._box()?.[12] ?? 0);
  }
  get scrollWidth() {
    if (this === html || this === body) return Math.round(S.vw);
    const b = this._box();
    return b ? Math.round(Math.max(b[6], b[2] - b[12] - b[14])) : 0;
  }
  get scrollHeight() {
    if (this === html || this === body) return Math.round(S.vh);
    const b = this._box();
    return b ? Math.round(Math.max(b[7], b[3] - b[13] - b[15])) : 0;
  }
  get scrollTop() {
    return this._box()?.[5] ?? 0;
  }
  set scrollTop(v: number) {
    this.scrollTo({ top: v });
  }
  get scrollLeft() {
    return this._box()?.[4] ?? 0;
  }
  set scrollLeft(v: number) {
    this.scrollTo({ left: v });
  }
  scrollTo(a?: any, b?: number) {
    const o = typeof a === "object" && a !== null ? a : { left: a, top: b };
    const box = this._box();
    if (!box || this.__wid === undefined) return;
    const clamp = (v: number, max: number) => Math.max(0, Math.min(Number.isFinite(v) ? v : 0, Math.max(0, max)));
    if (o.top !== undefined) setScroll(this, box, "scrollTop", 5, clamp(Number(o.top), box[7] - (box[3] - box[13] - box[15])));
    if (o.left !== undefined) setScroll(this, box, "scrollLeft", 4, clamp(Number(o.left), box[6] - (box[2] - box[12] - box[14])));
  }
  scroll(a?: any, b?: number) {
    this.scrollTo(a, b);
  }
  scrollBy(a?: any, b?: number) {
    const o = typeof a === "object" && a !== null ? a : { left: a, top: b };
    this.scrollTo({ top: o.top !== undefined ? this.scrollTop + Number(o.top) : undefined, left: o.left !== undefined ? this.scrollLeft + Number(o.left) : undefined });
  }
  scrollIntoView(arg?: boolean | { block?: string; inline?: string }) {
    const block = arg === false ? "end" : typeof arg === "object" ? (arg.block ?? "start") : "start";
    const inline = typeof arg === "object" ? (arg.inline ?? "nearest") : "nearest";
    scrollIntoView(this, block, inline);
  }
  scrollIntoViewIfNeeded() {
    scrollIntoView(this, "nearest", "nearest");
  }
  checkVisibility(opts?: { checkVisibilityCSS?: boolean; visibilityProperty?: boolean; checkOpacity?: boolean; opacityProperty?: boolean }) {
    for (let n: Node_ | null = this; n instanceof Element_; n = n.parentNode) {
      if (computedOf(n).display === "none") return false;
      if ((opts?.checkOpacity || opts?.opacityProperty) && computedOf(n).opacity === "0") return false;
    }
    if ((opts?.checkVisibilityCSS || opts?.visibilityProperty) && computedOf(this).visibility !== "visible") return false;
    return this.isConnected;
  }
  // ---- pointer capture
  setPointerCapture(_id: number) {
    S.pointerCapture = this;
    tellCapture(this);
  }
  releasePointerCapture(_id: number) {
    if (S.pointerCapture === this) {
      S.pointerCapture = null;
      tellCapture(null);
    }
  }
  hasPointerCapture(_id: number) {
    return S.pointerCapture === this;
  }
  getAnimations() {
    return [];
  }
  attachShadow(): never {
    throw new Error("NotSupportedError: shadow DOM is out of P18's scope");
  }
  get shadowRoot() {
    return null;
  }
  get assignedSlot() {
    return null;
  }
  toString() {
    return `[object ${this.constructor.name.replace(/_$/, "")}]`;
  }
}

function escapeHtml(s: string) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function setScroll(el: Element_, box: number[], prop: string, at: number, wrote: number) {
  // held to a hundredth, as the host's geometry gives it back each frame (systems/page snapshot.rs), as Chrome snaps
  // a written offset to its pixels at once: read back finer, a write looked like a move the next frame took back, and a
  // spring that keeps what hasn't moved yet (use-stick-to-bottom's) threw its steps away and wrote every frame for good
  const v = Math.round(wrote * 100) / 100;
  // what it holds moves at once (Chrome lays out again on the next read): its descendants' boxes by the change
  const d = v - box[at];
  if (d) {
    const walk = (n: Node_) => {
      for (const k of n._kids) {
        const b = k._wired && k.__wid !== undefined ? geo.get(k.__wid) : undefined;
        if (b) b[at === 5 ? 1 : 0] -= d;
        walk(k);
      }
    };
    walk(el);
  }
  box[at] = v;
  // a value the host was last given is sent again a hair off, so the host sees a change (it applies changed props)
  const last = el._extra[prop];
  const send = last === v ? v + 1e-4 : v;
  el._extra[prop] = send;
  // what the host hasn't told back yet (its scroll event, input.ts): an event that carries an older one is stale
  const q = ((el._ahead ??= {})[prop] ??= []);
  q.push(send);
  if (q.length > 64) q.shift();
  wire.set(el.__wid!, prop, send);
}

export function scrollable(el: Element_): boolean {
  const c = computedOf(el);
  return /auto|scroll/.test(c.overflowY) || /auto|scroll/.test(c.overflowX);
}

function scrollIntoView(el: Element_, block: string, inline: string) {
  for (let p = el.parentNode; p instanceof Element_ && p !== body; p = p.parentNode) {
    if (!scrollable(p)) continue;
    const pb = p._box();
    const r = el.getBoundingClientRect();
    if (!pb) continue;
    const top = pb[1] + pb[13];
    const vis = pb[3] - pb[13] - pb[15];
    const left = pb[0] + pb[12];
    const visW = pb[2] - pb[12] - pb[14];
    const dy = align(block, r.top - top, r.height, vis);
    const dx = align(inline, r.left - left, r.width, visW);
    if (dy) p.scrollTop = p.scrollTop + dy;
    if (dx) p.scrollLeft = p.scrollLeft + dx;
  }
}

function align(how: string, off: number, size: number, view: number): number {
  if (how === "start") return off;
  if (how === "end") return off + size - view;
  if (how === "center") return off + size / 2 - view / 2;
  // nearest
  if (off < 0) return off;
  if (off + size > view) return size > view ? off : off + size - view;
  return 0;
}
