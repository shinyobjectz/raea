// Keys and activation: focus targets, Space and Enter clicks, and form submission.
import * as wire from "../wire";
import { Control_, Element_, HTMLDialogElement_, modals, Event_, HTMLButtonElement_, HTMLFormElement_, HTMLInputElement_, HTMLLabelElement_, HTMLOptionElement_, HTMLSelectElement_, InputEvent_, KeyboardEvent_, MODS, Node_, PointerEvent_, S, SubmitEvent_, disabledEl, dispatch, doc, editKey, focusTo, focusable, g, installed, isField, keyInit, query } from "./all";

/** An edit's inputType and data, from a field's value before and after it: what was put in (insertText, the text),
 *  else what was taken out (deleteContentBackward, or Forward when the caret's side stayed). */
export function inputKind(was: string, now: string): { inputType: string; data: string | null } {
  let a = 0;
  while (a < was.length && a < now.length && was[a] === now[a]) a++;
  let b = 0;
  while (b < was.length - a && b < now.length - a && was[was.length - 1 - b] === now[now.length - 1 - b]) b++;
  const added = now.slice(a, now.length - b);
  if (added) return { inputType: added.includes("\n") && added.trim() === "" ? "insertLineBreak" : "insertText", data: added };
  return { inputType: "deleteContentBackward", data: null };
}

/** The pointer's capture, for the host: what it captured is what the pointer is over (hover, enter and leave follow
 *  it, as Chrome's), until it's let go. */
export function tellCapture(el: Element_ | null) {
  if (installed) wire.emit("capture", el?._wired && el.__wid !== undefined ? el.__wid : null);
}

/** The element on the host as wire id `id`. */
export function byWid(id: number): Element_ | null {
  for (const n of query(doc, "*", false)) if (n.__wid === id) return n as Element_;
  return null;
}

export function focusTarget(t: Element_): Element_ | null {
  for (let n: Node_ | null = t; n instanceof Element_; n = n.parentNode) if (focusable(n)) return n;
  return null;
}

/** A key from the host at the focused element (or body): keydown, keyup, and the default actions Chrome takes for
 *  what wasn't prevented (Enter and Space press a button, Enter submits a form, Tab's move undone if prevented). */
export function key(t: Element_, payload: any) {
  if (payload && t instanceof Control_ && typeof payload.value === "string") t._value = payload.value;
  const init = keyInit(payload ?? {});
  const ok = dispatch(t, new KeyboardEvent_("keydown", init), true);
  // the host's default actions (Tab's focus move) wait on this; an editable's key is its edit (editing.ts)
  if (!ok || (!isField(t) && editKey(t, init))) wire.emit("prevented");
  S.spaceDown = init.key === " " ? { t, ok } : null;
  const tag = t.localName;
  // a closed select's type-ahead: a letter picks the next option whose text starts with it (input, then change)
  if (ok && tag === "select" && init.key.length === 1 && init.key !== " " && !init.ctrlKey && !init.metaKey && !init.altKey) {
    const sel = t as HTMLSelectElement_;
    const opts = query(sel, "option", false) as HTMLOptionElement_[];
    const at = sel.selectedIndex;
    const k = init.key.toLowerCase();
    for (let i = 1; i <= opts.length; i++) {
      const o = opts[(at + i) % opts.length];
      if (o.hasAttribute("disabled") || !(o.textContent ?? "").trim().toLowerCase().startsWith(k)) continue;
      if (o !== opts[at]) {
        sel.value = o.value;
        dispatch(sel, new InputEvent_("input", { bubbles: true, composed: true, view: g }), true);
        dispatch(sel, new Event_("change", { bubbles: true }), true);
      }
      break;
    }
  }
  // Escape on an open modal dialog: its cancel, and it closes unless that's prevented (as Chrome's)
  if (ok && init.key === "Escape") {
    const top = modals.at(-1) as HTMLDialogElement_ | undefined;
    if (top?.isConnected && top.open) top.requestClose();
  }
  const type = (t as Control_).type;
  // Enter's activation is keydown's default action (before keyup); Space's is keyup's
  if (ok && init.key === "Enter" && !init.altKey) {
    if (tag === "button" || (tag === "a" && t.hasAttribute("href")) || (tag === "input" && (type === "submit" || type === "button" || type === "reset"))) activate(t, keyClick(t), true);
    else if (tag === "input" && isField(t)) {
      const form = (t as Control_).form;
      if (form) {
        const sub = form.querySelector('button:not([type]), button[type="submit"], input[type="submit"]') as Element_ | null;
        if (sub) activate(sub, keyClick(sub), true);
        else submitForm(form as HTMLFormElement_, null);
      }
    }
  }
}

/** Space pressed: where, and whether its keydown went unprevented (its activation is keyup's) */

/** A key let go, from the host, at what has the focus now (after a Tab's move): keyup, and Space's activation of a
 *  button, checkbox or radio it went down on. */
export function keyUp(t: Element_, payload: any) {
  const init = keyInit(payload ?? {});
  const upOk = dispatch(t, new KeyboardEvent_("keyup", { ...init, repeat: false }), true);
  const sd = init.key === " " ? S.spaceDown : null;
  if (init.key === " ") S.spaceDown = null;
  const tag = t.localName;
  const type = (t as Control_).type;
  if (sd?.ok && upOk && sd.t === t && (tag === "button" || (tag === "input" && (type === "checkbox" || type === "radio" || type === "submit" || type === "button")))) activate(t, keyClick(t), true);
}

function keyClick(t: Element_) {
  return new PointerEvent_("click", { bubbles: true, cancelable: true, composed: true, view: g, detail: 0, pointerId: -1, pointerType: "", clientX: 0, clientY: 0, ...MODS });
}

/** A click and its activation: a checkbox toggles (undone if prevented), a submit button submits its form, a label
 *  clicks its control. */
export function activate(t: Element_, ev: Event_, trusted: boolean) {
  const btn = t.closest("button, input, a, label") as Element_ | null;
  if (btn && disabledEl(btn)) return;
  let toggled: Control_ | null = null;
  if (btn instanceof HTMLInputElement_ && (btn.type === "checkbox" || btn.type === "radio")) {
    toggled = btn;
    // through the setter, so the host's Checked follows an uncontrolled one (React puts a controlled one back after)
    btn.checked = btn.type === "radio" ? true : !btn.checked;
  }
  const ok = dispatch(t, ev, trusted);
  if (toggled) {
    if (!ok) toggled.checked = !toggled.checked;
    else {
      dispatch(toggled, new InputEvent_("input", { bubbles: true, composed: true }), true);
      dispatch(toggled, new Event_("change", { bubbles: true }), true);
    }
  }
  if (!ok || !btn) return;
  if (btn instanceof HTMLButtonElement_ && btn.form) {
    if (btn.type === "submit") submitForm(btn.form as HTMLFormElement_, btn);
    else if (btn.type === "reset") (btn.form as HTMLFormElement_).reset();
  } else if (btn.localName === "a" && btn.hasAttribute("href")) {
    // a link's default action, as Chrome's: its fragment on this page (location's hashchange, which a hash router
    // hears: raea cloud's dashboard in raea.app moved between its pages on none of its rail's links, 11-23), another
    // page of this one's origin, or a new window for one that asks (target) or is elsewhere
    const href = btn.getAttribute("href") ?? "";
    const loc = (g as { location?: { hash: string; href: string; assign?: (u: string) => void } }).location;
    if (href.startsWith("#")) {
      if (loc) loc.hash = href;
    } else if (href && !/^(javascript|mailto|tel):/i.test(href)) {
      let url: URL | null = null;
      try {
        url = new URL(href, loc?.href ?? "http://localhost/");
      } catch {}
      const blank = (btn.getAttribute("target") ?? "") !== "" && btn.getAttribute("target") !== "_self";
      if (url && !blank && loc && url.origin === new URL(loc.href).origin) loc.assign?.(url.href);
      else if (url) (g as { open?: (u: string, t?: string) => unknown }).open?.(url.href, "_blank");
    }
  } else if (btn instanceof HTMLLabelElement_) {
    // a click on a label (on it or on what's in it, its control aside) clicks its control
    const c = btn.control;
    if (c && !c.contains(t)) {
      focusTo(c, true);
      activate(c, new PointerEvent_("click", { bubbles: true, cancelable: true, composed: true, view: g }), trusted);
    }
  }
}

export function submitForm(form: HTMLFormElement_, submitter: Element_ | null) {
  if (!form.noValidate && !(submitter && submitter.hasAttribute("formnovalidate")) && !form.checkValidity()) return;
  dispatch(form, new SubmitEvent_("submit", { bubbles: true, cancelable: true, submitter }), true);
}
