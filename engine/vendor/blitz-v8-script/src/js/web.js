// Web platform APIs that React and its ecosystem (Radix, Emotion, Recharts, routers, i18n) call,
// defined in JS over the native DOM. Loaded after the bootstrap and geometry.js.
//
// Rendering is a deterministic, single-frame-at-a-time job, so a few APIs are deliberately
// simplified: crypto is a seeded PRNG (same output every render), storage is in memory, the
// document is always visible and focused, and pointer capture is a no-op.
(function () {
    const define = (obj, name, value) =>
        Object.defineProperty(obj, name, { value, writable: true, configurable: true });
    const getter = (obj, name, get, set) =>
        Object.defineProperty(obj, name, { get, set, configurable: true });
    const html = document.documentElement;
    const elementProto = html ? Object.getPrototypeOf(html) : null;
    const nodeProto = Object.getPrototypeOf(Object.getPrototypeOf(document));
    const SVG_NS = "http://www.w3.org/2000/svg";

    // --- Event subclasses -------------------------------------------------------------------
    // The bootstrap's MouseEvent, KeyboardEvent, ... are instanceof-only stubs. These build a
    // real Event and give it the subclass prototype plus the init dictionary's fields.
    const eventFields = {
        UIEvent: { view: null, detail: 0 },
        MouseEvent: {
            screenX: 0, screenY: 0, clientX: 0, clientY: 0, pageX: 0, pageY: 0, offsetX: 0, offsetY: 0,
            movementX: 0, movementY: 0, button: 0, buttons: 0, relatedTarget: null,
            ctrlKey: false, shiftKey: false, altKey: false, metaKey: false,
        },
        PointerEvent: {
            pointerId: 0, width: 1, height: 1, pressure: 0, tangentialPressure: 0, tiltX: 0, tiltY: 0,
            twist: 0, pointerType: "", isPrimary: false,
        },
        WheelEvent: { deltaX: 0, deltaY: 0, deltaZ: 0, deltaMode: 0 },
        KeyboardEvent: {
            key: "", code: "", location: 0, repeat: false, isComposing: false, charCode: 0, keyCode: 0, which: 0,
            ctrlKey: false, shiftKey: false, altKey: false, metaKey: false,
        },
        FocusEvent: { relatedTarget: null },
        InputEvent: { data: null, inputType: "", isComposing: false, dataTransfer: null },
        CompositionEvent: { data: "" },
        AnimationEvent: { animationName: "", elapsedTime: 0, pseudoElement: "" },
        TransitionEvent: { propertyName: "", elapsedTime: 0, pseudoElement: "" },
    };
    const eventParents = {
        UIEvent: null, MouseEvent: "UIEvent", PointerEvent: "MouseEvent", WheelEvent: "MouseEvent",
        KeyboardEvent: "UIEvent", FocusEvent: "UIEvent", InputEvent: "UIEvent", CompositionEvent: "UIEvent",
        AnimationEvent: null, TransitionEvent: null,
    };
    const fieldsOf = (name) => {
        const own = eventFields[name];
        const parent = eventParents[name];
        return parent ? Object.assign({}, fieldsOf(parent), own) : Object.assign({}, eventFields.UIEvent, own);
    };
    for (const name of Object.keys(eventFields)) {
        const stub = globalThis[name];
        const proto =
            stub && stub.prototype && typeof stub.prototype === "object" ? stub.prototype : {};
        const parentName = eventParents[name];
        const parentProto = parentName ? globalThis[parentName].prototype : Event.prototype;
        if (Object.getPrototypeOf(proto) !== parentProto) Object.setPrototypeOf(proto, parentProto);
        const ctor = function (type, init) {
            if (!new.target) throw new TypeError(`Failed to construct '${name}': Please use the 'new' operator`);
            init = init || {};
            const event = new Event(type, init);
            Object.setPrototypeOf(event, new.target.prototype);
            const fields = fieldsOf(name);
            for (const key of Object.keys(fields)) {
                define(event, key, key in init ? init[key] : fields[key]);
            }
            if (name === "KeyboardEvent" || name.endsWith("MouseEvent") || name === "PointerEvent") {
                define(event, "getModifierState", (k) =>
                    ({ Control: event.ctrlKey, Shift: event.shiftKey, Alt: event.altKey, Meta: event.metaKey })[k] || false);
            }
            return event;
        };
        Object.defineProperty(ctor, "name", { value: name, configurable: true });
        ctor.prototype = proto;
        define(proto, "constructor", ctor);
        globalThis[name] = ctor;
    }
    for (const name of ["PointerEvent", "WheelEvent"]) Object.setPrototypeOf(globalThis[name], globalThis.MouseEvent);

    // --- instanceof for tag-specific interfaces --------------------------------------------
    // All elements share one native prototype, so answer instanceof from the tag and namespace.
    const htmlTags = {
        HTMLInputElement: "input", HTMLTextAreaElement: "textarea", HTMLSelectElement: "select",
        HTMLButtonElement: "button", HTMLAnchorElement: "a", HTMLIFrameElement: "iframe",
        HTMLImageElement: "img", HTMLCanvasElement: "canvas", HTMLVideoElement: "video",
        HTMLAudioElement: "audio", HTMLDivElement: "div", HTMLSpanElement: "span", HTMLFormElement: "form",
        HTMLLabelElement: "label", HTMLStyleElement: "style", HTMLScriptElement: "script",
        HTMLLinkElement: "link", HTMLTemplateElement: "template", HTMLOptionElement: "option",
        HTMLParagraphElement: "p", HTMLUListElement: "ul", HTMLLIElement: "li", HTMLTableElement: "table",
        HTMLBodyElement: "body", HTMLHeadElement: "head", HTMLHtmlElement: "html", HTMLDialogElement: "dialog",
    };
    const isElement = (v) => v != null && typeof v === "object" && v.nodeType === 1;
    const tagCheck = (name, test) => {
        if (typeof globalThis[name] !== "function") {
            const iface = function () { throw new TypeError("Illegal constructor"); };
            Object.defineProperty(iface, "name", { value: name, configurable: true });
            iface.prototype = Object.create(elementProto || Object.prototype);
            globalThis[name] = iface;
        }
        Object.defineProperty(globalThis[name], Symbol.hasInstance, { value: test, configurable: true });
    };
    for (const [name, tag] of Object.entries(htmlTags)) {
        tagCheck(name, (v) => isElement(v) && v.namespaceURI !== SVG_NS && v.localName === tag);
    }
    tagCheck("SVGElement", (v) => isElement(v) && v.namespaceURI === SVG_NS);
    tagCheck("SVGGraphicsElement", (v) => isElement(v) && v.namespaceURI === SVG_NS);
    tagCheck("SVGSVGElement", (v) => isElement(v) && v.namespaceURI === SVG_NS && v.localName === "svg");
    tagCheck("SVGPathElement", (v) => isElement(v) && v.namespaceURI === SVG_NS && v.localName === "path");
    tagCheck("SVGTextElement", (v) => isElement(v) && v.namespaceURI === SVG_NS && v.localName === "text");
    tagCheck("SVGGElement", (v) => isElement(v) && v.namespaceURI === SVG_NS && v.localName === "g");
    if (typeof globalThis.ShadowRoot !== "function") {
        tagCheck("ShadowRoot", () => false);
    }

    // --- Element and Node methods -----------------------------------------------------------
    if (elementProto) {
        const P = elementProto;
        if (!P.insertAdjacentElement) {
            define(P, "insertAdjacentElement", function (where, el) {
                switch (String(where).toLowerCase()) {
                    case "beforebegin": return this.parentNode ? this.parentNode.insertBefore(el, this) : null;
                    case "afterbegin": return this.insertBefore(el, this.firstChild);
                    case "beforeend": return this.appendChild(el);
                    case "afterend": return this.parentNode ? this.parentNode.insertBefore(el, this.nextSibling) : null;
                }
                throw new SyntaxError(`'${where}' is not a valid position`);
            });
        }
        if (!P.insertAdjacentText) {
            define(P, "insertAdjacentText", function (where, text) {
                this.insertAdjacentElement(where, document.createTextNode(String(text)));
            });
        }
        if (!P.toggleAttribute) {
            define(P, "toggleAttribute", function (name, force) {
                const has = this.hasAttribute(name);
                const want = force === undefined ? !has : Boolean(force);
                if (want && !has) this.setAttribute(name, "");
                if (!want && has) this.removeAttribute(name);
                return want;
            });
        }
        for (const [ns, plain] of [["setAttributeNS", "setAttribute"], ["getAttributeNS", "getAttribute"],
                                   ["removeAttributeNS", "removeAttribute"], ["hasAttributeNS", "hasAttribute"]]) {
            if (!P[ns]) {
                define(P, ns, function (_ns, name, value) {
                    // xlink:href and friends keep their qualified name; the namespace is not modelled.
                    return plain === "setAttribute" ? this.setAttribute(name, value) : this[plain](name);
                });
            }
        }
        if (P.getAttributeNames && !("attributes" in P)) {
            getter(P, "attributes", function () {
                const list = this.getAttributeNames().map((name) => ({
                    name, localName: name, value: this.getAttribute(name), namespaceURI: null, prefix: null,
                    specified: true, ownerElement: this,
                }));
                list.item = (i) => list[i] || null;
                list.getNamedItem = (n) => list.find((a) => a.name === n) || null;
                return list;
            });
        }
        const focusable = new Set(["a", "button", "input", "select", "textarea", "iframe", "summary"]);
        if (!("tabIndex" in P)) {
            getter(P, "tabIndex",
                function () {
                    const v = this.getAttribute("tabindex");
                    if (v !== null && !isNaN(parseInt(v))) return parseInt(v);
                    return focusable.has(this.localName) ? 0 : -1;
                },
                function (v) { this.setAttribute("tabindex", String(v)); });
        }
        for (const flag of ["inert", "draggable", "spellcheck"]) {
            if (!(flag in P)) {
                getter(P, flag, function () { return this.hasAttribute(flag); },
                       function (v) { this.toggleAttribute(flag, Boolean(v)); });
            }
        }
        if (!P.checkVisibility) {
            define(P, "checkVisibility", function () {
                const r = this.getBoundingClientRect();
                if (r.width === 0 && r.height === 0) return false;
                const cs = getComputedStyle(this);
                return cs.display !== "none" && cs.visibility !== "hidden";
            });
        }
        // The native getter answers null for every element; this follows CSS positioning.
        {
            getter(P, "offsetParent", function () {
                if (!this.isConnected || getComputedStyle(this).display === "none") return null;
                for (let p = this.parentElement; p; p = p.parentElement) {
                    if (p.localName === "body" || getComputedStyle(p).position !== "static") return p;
                }
                return document.body;
            });
        }
        for (const name of ["setPointerCapture", "releasePointerCapture"]) {
            if (!P[name]) define(P, name, function () {});
        }
        if (!P.hasPointerCapture) define(P, "hasPointerCapture", () => false);
        if (!P.click) {
            define(P, "click", function () {
                if (this.disabled) return;
                this.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, composed: true, view: window }));
            });
        }
        if (!P.select) {
            define(P, "select", function () {
                if (typeof this.setSelectionRange === "function") this.setSelectionRange(0, String(this.value || "").length);
            });
        }
        if (!P.getAnimations) define(P, "getAnimations", () => []);

        // `children` is a fresh array on each read; give it HTMLCollection's methods.
        const childrenDesc = Object.getOwnPropertyDescriptor(P, "children");
        if (childrenDesc && childrenDesc.get) {
            getter(P, "children", function () {
                const list = childrenDesc.get.call(this);
                if (list && !list.item) {
                    define(list, "item", (i) => list[i] || null);
                    define(list, "namedItem", (n) => list.find((e) => e.id === n || e.getAttribute("name") === n) || null);
                }
                return list;
            });
        }

        // SVG geometry
        const svgRoot = (el) => {
            let s = el;
            while (s && !(s.namespaceURI === SVG_NS && s.localName === "svg" && !(s.parentElement && s.parentElement.namespaceURI === SVG_NS))) s = s.parentElement;
            return s;
        };
        if (!P.getComputedTextLength) {
            define(P, "getComputedTextLength", function () {
                if (typeof this.getBBox === "function") return this.getBBox().width;
                return this.getBoundingClientRect().width;
            });
        }
        if (!P.getScreenCTM) {
            const ctm = function () {
                const root = svgRoot(this);
                if (!root) return null;
                const r = root.getBoundingClientRect();
                let m = new DOMMatrix().translate(r.left, r.top);
                const vb = (root.getAttribute("viewBox") || "").trim().split(/[\s,]+/).map(Number);
                if (vb.length === 4 && vb[2] > 0 && vb[3] > 0) {
                    // preserveAspectRatio xMidYMid meet (the default)
                    const s = Math.min(r.width / vb[2], r.height / vb[3]);
                    m = m.translate((r.width - vb[2] * s) / 2, (r.height - vb[3] * s) / 2).scale(s).translate(-vb[0], -vb[1]);
                }
                return m;
            };
            define(P, "getScreenCTM", ctm);
            define(P, "getCTM", ctm);
        }
        if (!P.createSVGPoint) define(P, "createSVGPoint", () => new DOMPoint());
        if (!P.createSVGMatrix) define(P, "createSVGMatrix", () => new DOMMatrix());
    }

    if (nodeProto) {
        if (!nodeProto.getRootNode) {
            define(nodeProto, "getRootNode", function () {
                let n = this;
                while (n.parentNode) n = n.parentNode;
                return n;
            });
        }
        if (!nodeProto.compareDocumentPosition) {
            const chain = (n) => { const c = []; for (; n; n = n.parentNode) c.unshift(n); return c; };
            define(nodeProto, "compareDocumentPosition", function (other) {
                if (other === this) return 0;
                const a = chain(this), b = chain(other);
                if (a[0] !== b[0]) return 1 | 32 | 2; // DISCONNECTED | IMPLEMENTATION_SPECIFIC | PRECEDING
                let i = 0;
                while (i < a.length && i < b.length && a[i] === b[i]) i++;
                if (i === a.length) return 16 | 4; // other is contained by this, and follows
                if (i === b.length) return 8 | 2;  // other contains this, and precedes
                const kids = Array.from(a[i - 1].childNodes);
                return kids.indexOf(b[i]) > kids.indexOf(a[i]) ? 4 : 2;
            });
        }
        for (const [k, v] of Object.entries({
            DOCUMENT_POSITION_DISCONNECTED: 1, DOCUMENT_POSITION_PRECEDING: 2, DOCUMENT_POSITION_FOLLOWING: 4,
            DOCUMENT_POSITION_CONTAINS: 8, DOCUMENT_POSITION_CONTAINED_BY: 16, DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC: 32,
        })) {
            if (typeof Node === "function" && Node[k] === undefined) Node[k] = v;
            if (nodeProto[k] === undefined) define(nodeProto, k, v);
        }
    }

    // --- Document -----------------------------------------------------------------------------
    if (!document.hasFocus) define(document, "hasFocus", () => true);
    if (document.visibilityState === undefined) {
        getter(document, "visibilityState", () => "visible");
        getter(document, "hidden", () => false);
    }
    if (typeof document.cookie !== "string") {
        let jar = {};
        getter(document, "cookie",
            () => Object.entries(jar).map(([k, v]) => `${k}=${v}`).join("; "),
            (s) => { const [kv] = String(s).split(";"); const i = kv.indexOf("="); if (i > 0) jar[kv.slice(0, i).trim()] = kv.slice(i + 1).trim(); });
    }

    // Constructable stylesheets: each adopted sheet is mirrored into a <style> in <head>.
    if (typeof globalThis.CSSStyleSheet !== "function" || (() => { try { new CSSStyleSheet(); return false; } catch (e) { return true; } })()) {
        class CSSStyleSheet {
            constructor() { this._rules = []; this._el = null; }
            get cssRules() { return this._rules.map((cssText) => ({ cssText })); }
            insertRule(rule, index = 0) { this._rules.splice(index, 0, String(rule)); this._sync(); return index; }
            deleteRule(index) { this._rules.splice(index, 1); this._sync(); }
            replaceSync(text) { this._rules = [String(text)]; this._sync(); }
            replace(text) { this.replaceSync(text); return Promise.resolve(this); }
            _sync() { if (this._el) this._el.textContent = this._rules.join("\n"); }
        }
        globalThis.CSSStyleSheet = CSSStyleSheet;
    }
    if (!Array.isArray(document.adoptedStyleSheets)) {
        let adopted = [];
        getter(document, "adoptedStyleSheets", () => adopted, (sheets) => {
            for (const s of adopted) if (s._el && !sheets.includes(s)) { s._el.remove(); s._el = null; }
            adopted = Array.from(sheets);
            for (const s of adopted) {
                if (!s._el) { s._el = document.createElement("style"); s._el.setAttribute("data-adopted", ""); document.head.appendChild(s._el); }
                s._sync();
            }
        });
    }

    // Range: enough for selectNodeContents/getBoundingClientRect and createContextualFragment.
    if (!document.createRange) {
        class Range {
            constructor() { this.startContainer = document; this.startOffset = 0; this.endContainer = document; this.endOffset = 0; }
            get collapsed() { return this.startContainer === this.endContainer && this.startOffset === this.endOffset; }
            get commonAncestorContainer() {
                const anc = []; for (let n = this.startContainer; n; n = n.parentNode) anc.push(n);
                for (let n = this.endContainer; n; n = n.parentNode) if (anc.includes(n)) return n;
                return document;
            }
            setStart(n, o) { this.startContainer = n; this.startOffset = o; }
            setEnd(n, o) { this.endContainer = n; this.endOffset = o; }
            setStartBefore(n) { this.setStart(n.parentNode, Array.from(n.parentNode.childNodes).indexOf(n)); }
            setEndAfter(n) { this.setEnd(n.parentNode, Array.from(n.parentNode.childNodes).indexOf(n) + 1); }
            selectNode(n) { this.setStartBefore(n); this.setEndAfter(n); }
            selectNodeContents(n) { this.setStart(n, 0); this.setEnd(n, n.childNodes.length || (n.nodeValue || "").length); }
            collapse(toStart) { if (toStart) this.setEnd(this.startContainer, this.startOffset); else this.setStart(this.endContainer, this.endOffset); }
            cloneRange() { const r = new Range(); r.setStart(this.startContainer, this.startOffset); r.setEnd(this.endContainer, this.endOffset); return r; }
            detach() {}
            _box() { let n = this.commonAncestorContainer; while (n && n.nodeType !== 1) n = n.parentNode; return n; }
            getBoundingClientRect() { const el = this._box(); return el && el.getBoundingClientRect ? el.getBoundingClientRect() : new DOMRect(); }
            getClientRects() { return [this.getBoundingClientRect()]; }
            createContextualFragment(markup) {
                const t = document.createElement("template");
                t.innerHTML = markup;
                const f = document.createDocumentFragment();
                for (const c of Array.from((t.content || t).childNodes)) f.appendChild(c);
                return f;
            }
            toString() { const el = this._box(); return el ? el.textContent : ""; }
        }
        globalThis.Range = Range;
        define(document, "createRange", () => new Range());
    }

    // --- Globals -------------------------------------------------------------------------------
    // Seeded so a composition renders the same pixels every time (ids, shuffles, jitter).
    if (typeof globalThis.crypto === "undefined") {
        let seed = 0x9e3779b9;
        const next = () => {
            seed = (seed + 0x6d2b79f5) | 0;
            let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
            t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
            return (t ^ (t >>> 14)) >>> 0;
        };
        const getRandomValues = (arr) => {
            const bytes = new Uint8Array(arr.buffer, arr.byteOffset, arr.byteLength);
            for (let i = 0; i < bytes.length; i++) bytes[i] = next() & 255;
            return arr;
        };
        globalThis.crypto = {
            getRandomValues,
            randomUUID() {
                const b = getRandomValues(new Uint8Array(16));
                b[6] = (b[6] & 15) | 64; b[8] = (b[8] & 63) | 128;
                const h = Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
                return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
            },
        };
    }

    class Storage {
        constructor() { define(this, "_m", new Map()); }
        get length() { return this._m.size; }
        key(i) { return Array.from(this._m.keys())[i] ?? null; }
        getItem(k) { return this._m.has(String(k)) ? this._m.get(String(k)) : null; }
        setItem(k, v) { this._m.set(String(k), String(v)); }
        removeItem(k) { this._m.delete(String(k)); }
        clear() { this._m.clear(); }
    }
    for (const name of ["localStorage", "sessionStorage"]) {
        if (typeof globalThis[name] === "undefined") globalThis[name] = new Storage();
    }
    globalThis.Storage = globalThis.Storage || Storage;

    if (typeof URL === "function") {
        try { new URL("https://a.b/?x=1").searchParams.get("x"); }
        catch (e) {
            const cache = new WeakMap();
            getter(URL.prototype, "searchParams", function () {
                let p = cache.get(this);
                if (!p) {
                    const url = this;
                    p = new URLSearchParams(url.search);
                    // Writes go back to the URL, as in browsers.
                    for (const m of ["append", "delete", "set", "sort"]) {
                        if (typeof p[m] !== "function") continue;
                        const orig = p[m].bind(p);
                        define(p, m, (...a) => { orig(...a); const s = p.toString(); url.search = s ? "?" + s : ""; });
                    }
                    cache.set(this, p);
                }
                return p;
            });
        }
    }

    if (typeof navigator === "object") {
        if (!navigator.language) getter(navigator, "language", () => "en-US");
        if (!Array.isArray(navigator.languages)) getter(navigator, "languages", () => ["en-US", "en"]);
        if (navigator.onLine === undefined) getter(navigator, "onLine", () => true);
        if (navigator.hardwareConcurrency === undefined) getter(navigator, "hardwareConcurrency", () => 8);
        if (navigator.maxTouchPoints === undefined) getter(navigator, "maxTouchPoints", () => 0);
    }

    if (typeof globalThis.visualViewport === "undefined") {
        globalThis.visualViewport = {
            get width() { return innerWidth; }, get height() { return innerHeight; },
            offsetLeft: 0, offsetTop: 0, pageLeft: 0, pageTop: 0, scale: 1,
            addEventListener() {}, removeEventListener() {},
        };
    }

    if (typeof globalThis.reportError !== "function") {
        globalThis.reportError = (error) => {
            try {
                const ev = new Event("error", { cancelable: true });
                define(ev, "error", error);
                define(ev, "message", String(error && error.message || error));
                window.dispatchEvent(ev);
            } catch (e) {}
            console.error(error);
        };
    }

    if (typeof globalThis.DOMParser === "undefined") {
        globalThis.DOMParser = class DOMParser {
            parseFromString(markup, type) {
                // A detached tree with document-like accessors (not a separate Document).
                const root = document.createElement("html");
                const body = document.createElement("body");
                root.appendChild(body);
                if (/svg|xml/.test(String(type))) body.innerHTML = markup; else body.innerHTML = String(markup).replace(/^[\s\S]*<body[^>]*>|<\/body>[\s\S]*$/gi, "");
                return {
                    documentElement: /svg|xml/.test(String(type)) ? body.firstElementChild : root,
                    body, head: null,
                    querySelector: (s) => root.querySelector(s),
                    querySelectorAll: (s) => root.querySelectorAll(s),
                    getElementById: (id) => root.querySelector(`#${CSS.escape(id)}`),
                    getElementsByTagName: (t) => root.getElementsByTagName(t),
                };
            }
        };
    }

    // Intl gaps in Boa: RelativeTimeFormat (English only), and toLocale*String via DateTimeFormat.
    if (typeof Intl === "object") {
        try { new Intl.RelativeTimeFormat("en"); }
        catch (e) {
            Intl.RelativeTimeFormat = class RelativeTimeFormat {
                constructor(locale, opts = {}) { this.numeric = opts.numeric || "always"; }
                format(value, unit) {
                    unit = String(unit).replace(/s$/, "");
                    const n = Number(value), abs = Math.abs(n);
                    if (this.numeric === "auto") {
                        const words = { day: ["yesterday", "today", "tomorrow"], year: ["last year", "this year", "next year"],
                                        month: ["last month", "this month", "next month"], week: ["last week", "this week", "next week"] };
                        if (words[unit] && abs <= 1 && Number.isInteger(n)) return words[unit][n + 1];
                    }
                    const label = `${new Intl.NumberFormat("en").format(abs)} ${unit}${abs === 1 ? "" : "s"}`;
                    return n < 0 || Object.is(n, -0) ? `${label} ago` : `in ${label}`;
                }
                resolvedOptions() { return { locale: "en", style: "long", numeric: this.numeric }; }
            };
        }
        // Boa prints year: "numeric" with two digits ("10/1/26"); browsers print all four.
        if (Intl.DateTimeFormat && new Intl.DateTimeFormat("en-US", { year: "numeric", month: "numeric", day: "numeric" }).format(new Date(2026, 0, 1)) !== "1/1/2026") {
            const Native = Intl.DateTimeFormat;
            const Fixed = function DateTimeFormat(locales, options) {
                const inner = new Native(locales, options);
                const numericYear = !options || options.year === "numeric" ||
                    (options.year === undefined && !["month", "day", "weekday", "hour", "minute", "second", "dateStyle", "timeStyle"].some((k) => k in options));
                const fix = (text, date) => {
                    if (!numericYear) return text;
                    const d = date === undefined ? new Date() : new Date(date);
                    const full = String(d.getFullYear()), short = full.slice(-2);
                    const i = text.lastIndexOf(short);
                    return i >= 0 && !/\d/.test(text[i - 1] || "") && !/\d/.test(text[i + 2] || "") ? text.slice(0, i) + full + text.slice(i + 2) : text;
                };
                return new Proxy(inner, {
                    get(target, prop) {
                        if (prop === "format") return (date) => fix(target.format(date), date);
                        const v = Reflect.get(target, prop, target);
                        return typeof v === "function" ? v.bind(target) : v;
                    },
                });
            };
            Fixed.supportedLocalesOf = Native.supportedLocalesOf;
            Fixed.prototype = Native.prototype;
            Intl.DateTimeFormat = Fixed;
        }
        if (Intl.DateTimeFormat) {
            const fmt = (date, locales, options, defaults) => {
                const o = Object.assign({}, options || {});
                const hasAny = ["weekday", "era", "year", "month", "day", "hour", "minute", "second", "dateStyle", "timeStyle"].some((k) => k in o);
                return new Intl.DateTimeFormat(locales, hasAny ? o : Object.assign(o, defaults)).format(date);
            };
            const D = Date.prototype;
            if (new Date(2026, 9, 1).toLocaleDateString("en-US") !== "10/1/2026") {
                define(D, "toLocaleDateString", function (l, o) { return fmt(this, l, o, { year: "numeric", month: "numeric", day: "numeric" }); });
                define(D, "toLocaleTimeString", function (l, o) { return fmt(this, l, o, { hour: "numeric", minute: "2-digit", second: "2-digit" }); });
                define(D, "toLocaleString", function (l, o) { return fmt(this, l, o, { year: "numeric", month: "numeric", day: "numeric", hour: "numeric", minute: "2-digit", second: "2-digit" }); });
            }
        }
    }
})();
