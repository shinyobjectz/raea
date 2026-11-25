// Images, media, style, iframe, canvas and SVG elements, and the tag-to-class table that makes elements.
import * as wire from "../wire";
import { canvasClass } from "../canvas";
import { outline, pointAtLength, totalLength } from "./svg-length";
import { DocumentFragment_, Element_, Event_, HTMLAnchorElement_, HTMLButtonElement_, HTMLDivElement_, HTMLElement_, HTMLFormElement_, HTMLInputElement_, HTMLLIElement_, HTMLLabelElement_, HTMLOptionElement_, HTMLSelectElement_, HTMLSpanElement_, HTMLTextAreaElement_, HTMLUListElement_, MediaNews, Node_, S, SVG_NS, SVG_TAGS, Text_, byWid, dispatchDirect, focusTo, g, pageUrl, transition, dispatch, focusable, query } from "./all";

export class HTMLImageElement_ extends HTMLElement_ {
  _media?: MediaNews;
  get src() {
    return this.getAttribute("src") ?? "";
  }
  set src(v: string) {
    this.setAttribute("src", pageUrl(String(v)));
  }
  get complete() {
    return !this.getAttribute("src") || this._media?.type === "load" || this._media?.type === "error";
  }
  get naturalWidth() {
    return this._media?.w ?? 0;
  }
  get naturalHeight() {
    return this._media?.h ?? 0;
  }
  get currentSrc() {
    return this.complete ? (this.getAttribute("src") ?? "") : "";
  }
  decode() {
    return Promise.resolve();
  }
}

/** video and audio: the source's size and duration as the host decoded them; currentTime, whose set seeks the host's
 *  decoder (`__seek`: the time and a count); play() and pause(), which start and stop the host's player (`__play`: on,
 *  the time, a count and loop; raea-runtime-js media_play.rs plays on the sound's clock, its frames this element's
 *  picture), muted (`__muted`) and volume (`__volume`, 0..1). The host's news keeps currentTime (`time`) and fires playing, timeupdate and ended;
 *  play and pause fire here, as a browser fires them, when they're asked. */
export class HTMLMediaElement_ extends HTMLElement_ {
  _media?: MediaNews;
  get src() {
    return this.getAttribute("src") ?? "";
  }
  set src(v: string) {
    this.setAttribute("src", pageUrl(String(v)));
  }
  _time = 0;
  _seeks = 0;
  _muted?: boolean;
  _volume?: number;
  _playing = false;
  _plays = 0;
  /** the host told this element (a prop of its own beside its attributes) */
  _tell(k: string, v: unknown) {
    this._extra[k] = v;
    if (this._wired && this.__wid !== undefined) wire.set(this.__wid, k, v);
  }
  get paused() {
    return !this._playing;
  }
  setAttribute(n: string, v: unknown) {
    const was = this.getAttribute("src");
    super.setAttribute(n, v);
    if (this._playing && this._an(n) === "src" && this.getAttribute("src") !== was) {
      // a new source pauses while it loads, as a browser's load algorithm does (the page plays it again)
      this._playing = false;
      this._tell("__play", [false, this._time, ++this._plays, this.loop]);
      dispatchDirect(this, new Event_("pause", { bubbles: false }));
    }
  }
  /** how far the source has loaded (1 its metadata, 2 a frame), for the source it loaded: a new src starts again */
  _ready: [string, number] = ["", 0];
  _ended = false;
  get ended() {
    return this._ended;
  }
  get loop() {
    return this._props.loop === true || this.hasAttribute("loop");
  }
  set loop(v: boolean) {
    this.toggleAttribute("loop", !!v);
  }
  get autoplay() {
    return this._props.autoPlay === true || this._props.autoplay === true || this.hasAttribute("autoplay");
  }
  set autoplay(v: boolean) {
    this.toggleAttribute("autoplay", !!v);
  }
  play() {
    if (!this._playing) {
      // played to its end, it starts again from the start, as a browser's does, and currentTime is 0 from now: it was
      // left at the end until the host's first frame came, so a pause before then paused it back at the end (11-16)
      if (this._ended || this._time >= (this.duration || Infinity)) this._time = 0;
      this._playing = true;
      this._ended = false;
      this._tell("__muted", this.muted);
      if (this._volume !== undefined) this._tell("__volume", this._volume);
      this._tell("__play", [true, this._time, ++this._plays, this.loop]);
      dispatchDirect(this, new Event_("play", { bubbles: false }));
    }
    return Promise.resolve();
  }
  pause() {
    if (!this._playing) return;
    this._playing = false;
    this._tell("__play", [false, this._time, ++this._plays, this.loop]);
    dispatchDirect(this, new Event_("pause", { bubbles: false }));
  }
  /** Its source again from the start, as a browser's load algorithm: paused (with no pause event), at time 0, nothing
   *  loaded, and the host's decoder opened again (`__load`), or dropped when there's no src. */
  load() {
    if (this._playing) {
      this._playing = false;
      this._tell("__play", [false, 0, ++this._plays, this.loop]);
    }
    this._time = 0;
    this._ended = false;
    this._ready = ["", 0];
    this._tell("__seek", [0, ++this._seeks]);
    this._media = undefined;
    this._tell("__load", ++this._loads);
    dispatchDirect(this, new Event_("emptied", { bubbles: false }));
  }
  _loads = 0;
  /** what the host's decoders take (AVFoundation's and the ffmpeg CLI's): the usual containers and codecs, "maybe" */
  canPlayType(type: string) {
    return /^(video|audio)\/(mp4|quicktime|x-m4v|webm|ogg|mpeg|mp3|aac|x-m4a|mp4a-latm|wav|x-wav|wave|flac)\b/i.test(String(type)) ? "maybe" : "";
  }
  get preload() {
    return this.getAttribute("preload") ?? "auto";
  }
  set preload(v: string) {
    this.setAttribute("preload", String(v));
  }
  get muted() {
    return this._muted ?? (this._props.muted === true || this.hasAttribute("muted"));
  }
  set muted(v: boolean) {
    const was = this.muted;
    this._muted = !!v;
    this._tell("__muted", this._muted);
    if (was !== this._muted) dispatchDirect(this, new Event_("volumechange", { bubbles: false }));
  }
  get volume() {
    return this._volume ?? 1;
  }
  /** 0..1, as a browser's: outside it throws, as a browser's IndexSizeError does */
  set volume(v: number) {
    const x = Number(v);
    if (!Number.isFinite(x) || x < 0 || x > 1) throw new RangeError(`The volume provided (${v}) is outside the range [0, 1].`);
    if (x === this.volume && this._volume !== undefined) return;
    this._volume = x;
    this._tell("__volume", x);
    dispatchDirect(this, new Event_("volumechange", { bubbles: false }));
  }
  get duration() {
    return this._media?.duration ?? NaN;
  }
  get readyState() {
    return this._ready[0] === this.src ? this._ready[1] : 0;
  }
  get currentTime() {
    return this._time;
  }
  set currentTime(t: number) {
    this._time = Number(t) || 0;
    this._ended = false;
    this._extra.__seek = [this._time, ++this._seeks];
    if (this._wired && this.__wid !== undefined) wire.set(this.__wid, "__seek", this._extra.__seek);
    dispatchDirect(this, new Event_("seeking", { bubbles: false }));
  }
  get videoWidth() {
    return this._media?.w ?? 0;
  }
  get videoHeight() {
    return this._media?.h ?? 0;
  }
  get currentSrc() {
    return this.getAttribute("src") ?? "";
  }
}

/** A media element's news from the host: its state, then the event at it. */
export function mediaNews(v: MediaNews & { id: number }) {
  const el = byWid(v.id) as (HTMLImageElement_ | HTMLMediaElement_) | null;
  if (!el || !v.type) return;
  el._media = { ...el._media, ...v, w: v.w ?? el._media?.w, h: v.h ?? el._media?.h, duration: v.duration ?? el._media?.duration };
  if (el instanceof HTMLMediaElement_) {
    if (typeof v.time === "number") el._time = v.time;
    const r = v.type === "loadedmetadata" ? 1 : v.type === "loadeddata" || v.type === "seeked" ? 2 : 0;
    if (r) el._ready = [el.src, Math.max(r, el._ready[0] === el.src ? el._ready[1] : 0)];
  }
  // a playing element's time: its currentTime, with no event (timeupdate comes on its own, four times a second)
  if (v.type === "time") return;
  if (el instanceof HTMLMediaElement_ && v.type === "ended") {
    // played to its end: paused there, as a browser's is (pause, then ended)
    if (!el._playing) return;
    el._playing = false;
    el._ended = true;
    dispatchDirect(el, new Event_("pause", { bubbles: false }));
  }
  dispatchDirect(el, new Event_(v.type, { bubbles: false }));
  if (el instanceof HTMLMediaElement_ && v.type === "loadeddata") {
    dispatchDirect(el, new Event_("canplay", { bubbles: false }));
    if (el.autoplay && !el._plays) void el.play();
  }
}

export class HTMLStyleElement_ extends HTMLElement_ {
  get sheet() {
    return { cssRules: [], insertRule: () => 0, deleteRule: () => {} };
  }
}

export class HTMLBodyElement_ extends HTMLElement_ {}

export class HTMLHtmlElement_ extends HTMLElement_ {}

export class HTMLHeadElement_ extends HTMLElement_ {}

export class HTMLIFrameElement_ extends HTMLElement_ {
  /** the frame's window as its parent sees it, raea-page's proxy (RAEA_PAGE_IFRAMES, docs/blitz-bevy.feature B5c);
   *  undefined without it, as before */
  get contentWindow(): unknown {
    return g.__raea_frames?.window(this);
  }
}

/** The modal dialogs open, the topmost last: Escape asks the topmost to close (its cancel event). */
export const modals: Element_[] = [];

/** <dialog>, as Chrome's: open is its attribute; show and showModal open it (showModal focuses what's in it: its
 *  autofocus, else its first focusable); close shuts it, keeps returnValue, and fires close after; Escape on a modal one
 *  fires cancel, and shuts it unless that's prevented. raea cloud's sheets (sheet.js) are one opened with showModal:
 *  without it the rail's + and every sheet threw in raea.app (11-23). Blitz never matches :modal, so a modal one is
 *  placed by the page's CSS (the sheet's is fixed) and not raised to a top layer. */
export class HTMLDialogElement_ extends HTMLElement_ {
  _returnValue = "";
  get open() {
    return this.hasAttribute("open");
  }
  set open(v: boolean) {
    this.toggleAttribute("open", !!v);
  }
  get returnValue() {
    return this._returnValue;
  }
  set returnValue(v: string) {
    this._returnValue = String(v);
  }
  show() {
    if (!this.open) this.setAttribute("open", "");
  }
  showModal() {
    if (this.open) {
      if (modals.includes(this)) return;
      throw new DOMException("The dialog is already open as a non-modal dialog.", "InvalidStateError");
    }
    if (!this.isConnected) throw new DOMException("The dialog is not connected.", "InvalidStateError");
    this.setAttribute("open", "");
    modals.push(this);
    const all = query(this, "*", false) as Element_[];
    const first = all.find((e) => e.hasAttribute("autofocus") && focusable(e)) ?? all.find((e) => focusable(e));
    if (first) focusTo(first, true);
  }
  close(returnValue?: string) {
    if (!this.open) return;
    if (returnValue !== undefined) this._returnValue = String(returnValue);
    this.removeAttribute("open");
    const i = modals.indexOf(this);
    if (i >= 0) modals.splice(i, 1);
    // after, as a task (Chrome queues it): what closed it has finished first
    setTimeout(() => dispatch(this, new Event_("close"), true), 0);
  }
  requestClose(returnValue?: string) {
    if (!this.open) return;
    if (dispatch(this, new Event_("cancel", { cancelable: true }), true)) this.close(returnValue);
  }
}

/** canvas: getContext("2d") draws on the host (src/canvas.ts) */
export const HTMLCanvasElement_ = canvasClass(HTMLElement_);

export class SVGElement_ extends Element_ {
  get ownerSVGElement() {
    return this.closest("svg");
  }
  focus() {
    focusTo(this, true);
  }
  blur() {
    if (S.active === this) transition(null, true);
  }
  get tabIndex() {
    return Number(this.getAttribute("tabindex") ?? -1);
  }
  get dataset() {
    return super.dataset;
  }
  /** the transform attribute's functions (SVGAnimatedTransformList: the CSS `transform` property is not in it) */
  get transform() {
    const items = (this.getAttribute("transform") ?? "").match(/[a-zA-Z]+\([^)]*\)/g) ?? [];
    const list = { length: items.length, numberOfItems: items.length, getItem: (i: number) => items[i] };
    return { baseVal: list, animVal: list };
  }
}

export class SVGSVGElement_ extends SVGElement_ {}

/** path, line, polyline, polygon, circle, ellipse and rect: their outline's length and a point along it (#16) */
export class SVGGeometryElement_ extends SVGElement_ {
  private _outline() {
    const num = (n: string) => parseFloat(this.getAttribute(n) ?? "") || 0;
    return outline(this.localName, num, () => this.getAttribute("d") ?? "", () => this.getAttribute("points") ?? "") ?? [];
  }
  getTotalLength(): number {
    return totalLength(this._outline());
  }
  getPointAtLength(at: number) {
    const [x, y] = pointAtLength(this._outline(), Number(at) || 0);
    const P = g.DOMPoint;
    return P ? new P(x, y) : { x, y, z: 0, w: 1 };
  }
}

const GEOMETRY_TAGS = new Set(["path", "line", "polyline", "polygon", "circle", "ellipse", "rect"]);

const TAG_CLASS: Record<string, typeof HTMLElement_> = {
  input: HTMLInputElement_,
  textarea: HTMLTextAreaElement_,
  button: HTMLButtonElement_,
  select: HTMLSelectElement_,
  option: HTMLOptionElement_,
  form: HTMLFormElement_,
  a: HTMLAnchorElement_,
  label: HTMLLabelElement_,
  div: HTMLDivElement_,
  span: HTMLSpanElement_,
  ul: HTMLUListElement_,
  ol: HTMLUListElement_,
  li: HTMLLIElement_,
  img: HTMLImageElement_,
  video: HTMLMediaElement_,
  audio: HTMLMediaElement_,
  style: HTMLStyleElement_,
  body: HTMLBodyElement_,
  html: HTMLHtmlElement_,
  head: HTMLHeadElement_,
  iframe: HTMLIFrameElement_,
  canvas: HTMLCanvasElement_ as unknown as typeof HTMLElement_,
  dialog: HTMLDialogElement_,
};

/** Chrome's per-tag classes that have no behaviour of their own here. Pages test them: Excalidraw's isWritableElement
 *  asks `instanceof HTMLBRElement` on every keydown, and with no such global every shortcut threw. Each is a window
 *  global and what its tags make. */
const PLAIN: Record<string, string[]> = {
  HTMLBRElement: ["br"], HTMLParagraphElement: ["p"], HTMLHRElement: ["hr"], HTMLPreElement: ["pre", "listing", "xmp"],
  HTMLHeadingElement: ["h1", "h2", "h3", "h4", "h5", "h6"], HTMLQuoteElement: ["blockquote", "q"], HTMLDListElement: ["dl"],
  HTMLTableElement: ["table"], HTMLTableRowElement: ["tr"], HTMLTableCellElement: ["td", "th"], HTMLTableSectionElement: ["thead", "tbody", "tfoot"],
  HTMLFieldSetElement: ["fieldset"], HTMLLegendElement: ["legend"], HTMLPictureElement: ["picture"], HTMLSourceElement: ["source"],
  HTMLLinkElement: ["link"], HTMLMetaElement: ["meta"], HTMLTimeElement: ["time"],
};
export const plainClasses: Record<string, typeof HTMLElement_> = {};
for (const [name, tags] of Object.entries(PLAIN)) {
  const C = class extends HTMLElement_ {};
  plainClasses[name] = C;
  for (const t of tags) TAG_CLASS[t] ??= C;
}

export function makeElement(tag: string, ns?: string): Element_ {
  const svg = ns === SVG_NS || (ns === undefined && SVG_TAGS.has(tag) && tag !== "text" && tag !== "image" && tag !== "title");
  if (svg) return tag === "svg" ? new SVGSVGElement_(tag, SVG_NS) : GEOMETRY_TAGS.has(tag) ? new SVGGeometryElement_(tag, SVG_NS) : new SVGElement_(tag, SVG_NS);
  const C = TAG_CLASS[tag.toLowerCase()] ?? HTMLElement_;
  return new C(tag);
}

export function cloneOf(n: Node_, deep: boolean): Node_ {
  let c: Node_;
  if (n instanceof Text_) c = new Text_(n.data);
  else if (n instanceof Element_) {
    const e = makeElement(n.localName, n.namespaceURI);
    for (const [k, v] of n._attrs) e._attrs.set(k, v);
    e._extra = { ...n._props, ...n._extra };
    e._style = { ...n._style };
    c = e;
  } else c = new DocumentFragment_();
  if (deep) for (const k of n._kids) c.appendChild(cloneOf(k, true));
  return c;
}
