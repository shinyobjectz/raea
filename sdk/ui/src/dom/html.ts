// HTMLElement and the form controls: focusability, values, selection, forms, anchors and labels.
import * as wire from "../wire";
import { Element_, Event_, FOCUSABLE_TAGS, PointerEvent_, S, activate, body, computedOf, dispatch, doc, focusTo, focusedHost, g, isEditable, isEditingHost, isField, nodeList, query, submitForm, transition } from "./all";

export class HTMLElement_ extends Element_ {
  get hidden() {
    return this.hasAttribute("hidden");
  }
  set hidden(v: boolean) {
    this.toggleAttribute("hidden", !!v);
  }
  get title() {
    return this.getAttribute("title") ?? "";
  }
  set title(v: string) {
    this.setAttribute("title", v);
  }
  get lang() {
    return this.getAttribute("lang") ?? "";
  }
  set lang(v: string) {
    this.setAttribute("lang", String(v));
  }
  get dir() {
    return this.getAttribute("dir") ?? "";
  }
  set dir(v: string) {
    this.setAttribute("dir", v);
  }
  get tabIndex(): number {
    const a = this.getAttribute("tabindex");
    if (a !== null && a !== "" && !Number.isNaN(Number(a))) return Math.trunc(Number(a));
    return nativelyFocusable(this) ? 0 : -1;
  }
  set tabIndex(v: number) {
    this.setAttribute("tabindex", String(v));
  }
  get inert() {
    return this.hasAttribute("inert");
  }
  set inert(v: boolean) {
    this.toggleAttribute("inert", !!v);
  }
  get draggable() {
    return this.getAttribute("draggable") === "true";
  }
  set draggable(v: boolean) {
    this.setAttribute("draggable", String(!!v));
  }
  get contentEditable() {
    const a = this.getAttribute("contenteditable");
    if (a === null) return "inherit";
    const l = a.toLowerCase();
    return l === "" || l === "true" ? "true" : l === "false" || l === "plaintext-only" ? l : "inherit";
  }
  set contentEditable(v: string) {
    const l = String(v).toLowerCase();
    if (l === "inherit") this.removeAttribute("contenteditable");
    else if (l === "true" || l === "false" || l === "plaintext-only") this.setAttribute("contenteditable", l);
    else throw new Error(`SyntaxError: contentEditable can't be ${v}`);
  }
  get isContentEditable() {
    return this.isConnected && isEditable(this);
  }
  get accessKey() {
    return this.getAttribute("accesskey") ?? "";
  }
  set accessKey(v: string) {
    this.setAttribute("accesskey", String(v));
  }
  get offsetParent(): Element_ | null {
    if (!this.isConnected || computedOf(this).display === "none") return null;
    for (let p = this.parentNode; p instanceof Element_; p = p.parentNode) {
      if (p === body) return body;
      if (computedOf(p).position !== "static") return p;
    }
    return body;
  }
  get offsetTop() {
    return Math.round(offset(this, 1));
  }
  get offsetLeft() {
    return Math.round(offset(this, 0));
  }
  get offsetWidth() {
    return Math.round(this.getBoundingClientRect().width);
  }
  get offsetHeight() {
    return Math.round(this.getBoundingClientRect().height);
  }
  focus(opts?: { preventScroll?: boolean; focusVisible?: boolean }) {
    void opts?.preventScroll;
    focusTo(this, true);
    if (S.active === this) focusedHost(this);
  }
  blur() {
    if (S.active === this) transition(null, true);
  }
  click() {
    if ((this as any).disabled) return;
    const r = this.getBoundingClientRect();
    const ev = new PointerEvent_("click", { bubbles: true, cancelable: true, composed: true, view: g, detail: 0, pointerId: -1, pointerType: "", clientX: 0, clientY: 0, screenX: 0, screenY: 0 });
    void r;
    activate(this, ev, false);
  }
  get autofocus() {
    return this.hasAttribute("autofocus");
  }
  set autofocus(v: boolean) {
    this.toggleAttribute("autofocus", !!v);
  }
}

function offset(el: Element_, axis: 0 | 1): number {
  const op = (el as HTMLElement_).offsetParent;
  const r = el.getBoundingClientRect();
  const v = axis ? r.top : r.left;
  if (!op || op === body) return v;
  const pr = op.getBoundingClientRect();
  const b = op._box();
  return v - (axis ? pr.top : pr.left) - (b ? b[axis ? 13 : 12] : 0) + (axis ? op.scrollTop : op.scrollLeft);
}

export function nativelyFocusable(el: Element_): boolean {
  const t = el.localName;
  if (t === "a" || t === "area") return el.hasAttribute("href");
  if (FOCUSABLE_TAGS.has(t)) return !(t === "input" && el.getAttribute("type") === "hidden");
  if (t === "summary" || t === "iframe") return true;
  if (isEditingHost(el)) return true;
  return false;
}

export function disabledEl(el: Element_): boolean {
  return FOCUSABLE_TAGS.has(el.localName) && el.hasAttribute("disabled");
}

/** What focus() and a press can focus: a tabindex, or a natively focusable element that isn't disabled. */
export function focusable(el: Element_): boolean {
  if (!(el instanceof Element_) || !el.isConnected || disabledEl(el)) return false;
  if (el.hasAttribute("tabindex") && el.getAttribute("tabindex") !== "") return true;
  return nativelyFocusable(el);
}

/** Form controls: the value a field holds (the host's text field), checked, the form they're in. */
export class Control_ extends HTMLElement_ {
  _value: string | undefined;
  _checked: boolean | undefined;
  _valueAtFocus: string | undefined;
  get name() {
    return this.getAttribute("name") ?? "";
  }
  set name(v: string) {
    this.setAttribute("name", v);
  }
  get disabled() {
    return this.hasAttribute("disabled");
  }
  set disabled(v: boolean) {
    this.toggleAttribute("disabled", !!v);
  }
  get required() {
    return this.hasAttribute("required");
  }
  set required(v: boolean) {
    this.toggleAttribute("required", !!v);
  }
  get readOnly() {
    return this.hasAttribute("readonly");
  }
  set readOnly(v: boolean) {
    this.toggleAttribute("readonly", !!v);
  }
  get form(): HTMLFormElement_ | null {
    return this.closest("form") as HTMLFormElement_ | null;
  }
  get type(): string {
    const t = (this.getAttribute("type") ?? "").toLowerCase();
    if (this.localName === "button") return t === "button" || t === "reset" ? t : "submit";
    if (this.localName === "textarea") return "textarea";
    if (this.localName === "select") return this.hasAttribute("multiple") ? "select-multiple" : "select-one";
    return t || "text";
  }
  set type(v: string) {
    this.setAttribute("type", v);
  }
  get value(): string {
    if (this.localName === "select") return (this as any).selectedOptions?.[0]?.value ?? this._value ?? "";
    if (this.localName === "button") return this.getAttribute("value") ?? "";
    if (this.localName === "input" && (this.type === "checkbox" || this.type === "radio")) return this.getAttribute("value") ?? "on";
    return this._value ?? this.getAttribute("value") ?? "";
  }
  set value(v: string) {
    const s = v === null || v === undefined ? "" : String(v);
    this._value = s;
    if (!isField(this)) return;
    // kept for the host node too when it's made later: a field JS made and filled before placing it (Excalidraw's text
    // editor) is created with its value (materialize), else it showed empty
    this._extra.value = s;
    if (this._wired && this.__wid !== undefined) wire.set(this.__wid, "value", s);
  }
  get defaultValue() {
    return this.getAttribute("value") ?? "";
  }
  set defaultValue(v: string) {
    this.setAttribute("value", v);
  }
  get checked(): boolean {
    return this._checked ?? this.hasAttribute("checked");
  }
  set checked(v: boolean) {
    this._checked = !!v;
    if (this._wired && this.__wid !== undefined) wire.set(this.__wid, "checked", !!v);
    // a radio checked unchecks the rest of its group (same name, same form)
    const name = this.getAttribute("name");
    if (v && name && this.localName === "input" && this.type === "radio" && this.isConnected) {
      const form = (this as any).form ?? null;
      for (const o of query(this.ownerDocument as any, "input", false) as Control_[]) {
        if (o !== this && o.type === "radio" && o.getAttribute("name") === name && ((o as any).form ?? null) === form && o.checked) o.checked = false;
      }
    }
  }
  get defaultChecked() {
    return this.hasAttribute("checked");
  }
  set defaultChecked(v: boolean) {
    this.toggleAttribute("checked", !!v);
  }
  get placeholder() {
    return this.getAttribute("placeholder") ?? "";
  }
  set placeholder(v: string) {
    this.setAttribute("placeholder", String(v));
  }
  get valueAsNumber() {
    return this.value === "" ? NaN : Number(this.value);
  }
  set valueAsNumber(v: number) {
    this.value = Number.isFinite(v) ? String(v) : "";
  }
  get files() {
    return null;
  }
  get labels() {
    const id = this.id;
    return nodeList(id ? query(doc, `label[for="${id}"]`, false) : []);
  }
  get selectionStart() {
    return this.value.length;
  }
  get selectionEnd() {
    return this.value.length;
  }
  get selectionDirection() {
    return "none";
  }
  setSelectionRange() {}
  setRangeText() {}
  select() {}
  get validity() {
    const missing = this.required && !this.value;
    return { valid: !missing, valueMissing: missing, typeMismatch: false, patternMismatch: false, tooLong: false, tooShort: false, rangeUnderflow: false, rangeOverflow: false, stepMismatch: false, badInput: false, customError: !!this._custom };
  }
  _custom = "";
  get validationMessage() {
    return this._custom || (this.validity.valueMissing ? "Please fill out this field." : "");
  }
  get willValidate() {
    return !this.disabled;
  }
  setCustomValidity(m: string) {
    this._custom = String(m);
  }
  checkValidity() {
    const ok = this.validity.valid && !this._custom;
    if (!ok) dispatch(this, new Event_("invalid", { cancelable: true }), true);
    return ok;
  }
  reportValidity() {
    return this.checkValidity();
  }
  showPicker() {}
}

export class HTMLInputElement_ extends Control_ {}

export class HTMLTextAreaElement_ extends Control_ {}

export class HTMLButtonElement_ extends Control_ {}

export class HTMLSelectElement_ extends Control_ {
  get options() {
    return nodeList(query(this, "option", false));
  }
  get selectedOptions() {
    const o = query(this, "option", false) as HTMLOptionElement_[];
    const sel = o.filter((x) => x._sel ?? false);
    if (sel.length) return nodeList(sel);
    // a value set (React's controlled `value`) picks its option; else the one marked selected, else the first
    const byValue = this._value !== undefined ? o.find((x) => x.value === this._value) : undefined;
    if (byValue) return nodeList([byValue]);
    const marked = o.filter((x) => x.selected);
    return nodeList(marked.length ? marked : o.slice(0, 1));
  }
  get value(): string {
    return (this.selectedOptions[0] as HTMLOptionElement_ | undefined)?.value ?? "";
  }
  /** The option with this value selected (none when no option has it), and the host told (its value, as Chrome's) */
  set value(v: string) {
    const s = v === null || v === undefined ? "" : String(v);
    this._value = s;
    for (const o of query(this, "option", false) as HTMLOptionElement_[]) o._sel = o.value === s;
    if (this._wired && this.__wid !== undefined) wire.set(this.__wid, "value", this.value);
  }
  get selectedIndex() {
    const o = query(this, "option", false) as HTMLOptionElement_[];
    return o.indexOf(this.selectedOptions[0] as HTMLOptionElement_);
  }
  set selectedIndex(i: number) {
    const o = (query(this, "option", false) as HTMLOptionElement_[])[i];
    for (const x of query(this, "option", false) as HTMLOptionElement_[]) x._sel = x === o;
    this._value = o?.value ?? "";
    if (this._wired && this.__wid !== undefined) wire.set(this.__wid, "value", this.value);
  }
}

export class HTMLOptionElement_ extends HTMLElement_ {
  _sel: boolean | undefined;
  get value() {
    return this.getAttribute("value") ?? this.textContent;
  }
  set value(v: string) {
    this.setAttribute("value", String(v));
  }
  get selected() {
    return this._sel ?? this.hasAttribute("selected");
  }
  set selected(v: boolean) {
    this._sel = !!v;
  }
  get label() {
    return this.getAttribute("label") ?? this.textContent;
  }
  set label(v: string) {
    this.setAttribute("label", String(v));
  }
  get text() {
    return this.textContent;
  }
  set text(v: string) {
    this.textContent = String(v);
  }
  get disabled() {
    return this.hasAttribute("disabled");
  }
  set disabled(v: boolean) {
    this.toggleAttribute("disabled", !!v);
  }
}

export class HTMLFormElement_ extends HTMLElement_ {
  get elements() {
    const els = query(this, "input, select, textarea, button, fieldset, output, object", false);
    const named = Object.fromEntries(els.filter((e) => e.getAttribute("name")).map((e) => [e.getAttribute("name")!, e]));
    return Object.assign(nodeList(els), named, { namedItem: (n: string) => named[n] ?? null });
  }
  get length() {
    return this.elements.length;
  }
  get noValidate() {
    return this.hasAttribute("novalidate");
  }
  set noValidate(v: boolean) {
    this.toggleAttribute("novalidate", !!v);
  }
  get action() {
    return this.getAttribute("action") ?? "";
  }
  set action(v: string) {
    this.setAttribute("action", String(v));
  }
  get method() {
    return this.getAttribute("method") ?? "get";
  }
  set method(v: string) {
    this.setAttribute("method", String(v));
  }
  submit() {}
  requestSubmit(submitter?: Element_) {
    submitForm(this, submitter ?? null);
  }
  reset() {
    if (!dispatch(this, new Event_("reset", { bubbles: true, cancelable: true }), true)) return;
    for (const e of query(this, "input, textarea", false) as Control_[]) {
      e.value = e.defaultValue;
      e._checked = undefined;
    }
  }
  checkValidity() {
    return (query(this, "input, select, textarea", false) as Control_[]).every((e) => e.checkValidity());
  }
  reportValidity() {
    return this.checkValidity();
  }
}

export class HTMLAnchorElement_ extends HTMLElement_ {
  get href() {
    return this.getAttribute("href") ?? "";
  }
  set href(v: string) {
    this.setAttribute("href", String(v));
  }
}

export class HTMLLabelElement_ extends HTMLElement_ {
  get htmlFor() {
    return this.getAttribute("for") ?? "";
  }
  set htmlFor(v: string) {
    this.setAttribute("for", String(v));
  }
  get control(): Element_ | null {
    const f = this.htmlFor;
    return f ? doc.getElementById(f) : (this.querySelector("input, select, textarea, button") ?? null);
  }
}

export class HTMLDivElement_ extends HTMLElement_ {}

export class HTMLSpanElement_ extends HTMLElement_ {}

export class HTMLUListElement_ extends HTMLElement_ {}

export class HTMLLIElement_ extends HTMLElement_ {}

/** What the host told of a media element's source (its `media` event: runtime/js src/media_html.rs). */
export type MediaNews = { type?: string; w?: number; h?: number; duration?: number; time?: number };

/** A URL a script sets as the host loads it (an HTML page's sets this: relative to the page's folder). */
export let pageUrl = (u: string) => u;

export function setPageUrl(f: (u: string) => string) {
  pageUrl = f;
}
