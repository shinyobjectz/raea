(function () {
    // set while history.pushState/replaceState moves location: a hash changed that way fires no hashchange
    let hashQuiet = false;
    // `window.history`: an in-memory History implementation, sufficient for
    // SPA routers (e.g. React Router): `state`, `pushState`/`replaceState`
    // (which update `location`) and no-op `go`/`back`/`forward`. There is no
    // real session history, so `popstate` events are never fired.
    if (typeof globalThis.history === "undefined") {
        const updateLocationFromUrl = (url) => {
            try {
                const resolved = new URL(String(url), location.href);
                location.href = resolved.href;
                location.protocol = resolved.protocol;
                location.host = resolved.host;
                location.hostname = resolved.hostname;
                location.pathname = resolved.pathname;
                location.search = resolved.search;
                hashQuiet = true;
                try { location.hash = resolved.hash; } finally { hashQuiet = false; }
            } catch (e) {
                // Unresolvable URL (e.g. no base): leave location unchanged
            }
        };
        let historyState = null;
        globalThis.history = {
            length: 1,
            scrollRestoration: "auto",
            get state() {
                return historyState;
            },
            pushState(state, _unused, url) {
                historyState = state;
                if (url !== undefined && url !== null) updateLocationFromUrl(url);
            },
            replaceState(state, _unused, url) {
                historyState = state;
                if (url !== undefined && url !== null) updateLocationFromUrl(url);
            },
            go() {},
            back() {},
            forward() {},
        };
    }

    if (typeof globalThis.queueMicrotask !== "function") {
        globalThis.queueMicrotask = function (callback) {
            Promise.resolve().then(callback);
        };
    }

    // `fetch()`: a minimal Fetch API on top of the synchronous `__blitz_fetch_sync`
    // native (which goes through the document's ScriptFetcher). Bodies are
    // text-only; `Headers` is a simple case-insensitive map.
    if (typeof globalThis.fetch !== "function") {
        class Headers {
            #map = new Map();
            constructor(init) {
                if (init instanceof Headers) {
                    for (const [k, v] of init) this.append(k, v);
                } else if (Array.isArray(init)) {
                    for (const [k, v] of init) this.append(k, v);
                } else if (init && typeof init === "object") {
                    for (const k of Object.keys(init)) this.append(k, init[k]);
                }
            }
            append(name, value) {
                const key = String(name).toLowerCase();
                const existing = this.#map.get(key);
                this.#map.set(key, existing === undefined ? String(value) : existing + ", " + value);
            }
            set(name, value) { this.#map.set(String(name).toLowerCase(), String(value)); }
            get(name) { return this.#map.get(String(name).toLowerCase()) ?? null; }
            has(name) { return this.#map.has(String(name).toLowerCase()); }
            delete(name) { this.#map.delete(String(name).toLowerCase()); }
            forEach(cb, thisArg) { for (const [k, v] of this) cb.call(thisArg, v, k, this); }
            *entries() { yield* [...this.#map.entries()].sort((a, b) => (a[0] < b[0] ? -1 : 1)); }
            *keys() { for (const [k] of this.entries()) yield k; }
            *values() { for (const [, v] of this.entries()) yield v; }
            [Symbol.iterator]() { return this.entries(); }
        }

        class Response {
            #body;
            #bodyUsed = false;
            constructor(body = null, init = {}) {
                this.#body = body === null || body === undefined ? "" : String(body);
                const status = init.status === undefined ? 200 : Number(init.status);
                if (!Number.isInteger(status) || status < 200 || status > 599) {
                    throw new RangeError("The status provided (" + status + ") is outside the range [200, 599].");
                }
                Object.defineProperties(this, {
                    status: { value: status, enumerable: true },
                    statusText: { value: init.statusText === undefined ? "" : String(init.statusText), enumerable: true },
                    headers: { value: new Headers(init.headers), enumerable: true },
                    url: { value: init.url === undefined ? "" : String(init.url), enumerable: true },
                    type: { value: "basic", enumerable: true },
                    redirected: { value: false, enumerable: true },
                });
            }
            get ok() { return this.status >= 200 && this.status <= 299; }
            get bodyUsed() { return this.#bodyUsed; }
            #consume() {
                if (this.#bodyUsed) {
                    return Promise.reject(new TypeError("body stream already read"));
                }
                this.#bodyUsed = true;
                return Promise.resolve(this.#body);
            }
            text() { return this.#consume(); }
            json() { return this.#consume().then((text) => JSON.parse(text)); }
            arrayBuffer() { return this.#consume().then((text) => new TextEncoder().encode(text).buffer); }
            clone() {
                if (this.#bodyUsed) throw new TypeError("Response body is already used");
                return new Response(this.#body, {
                    status: this.status, statusText: this.statusText, headers: this.headers, url: this.url,
                });
            }
        }

        globalThis.Headers = Headers;
        globalThis.Response = Response;
        globalThis.fetch = function fetch(input, init) {
            return new Promise((resolve, reject) => {
                let url;
                try {
                    url = input && typeof input === "object" && "url" in input ? String(input.url) : String(input);
                } catch (e) {
                    reject(e);
                    return;
                }
                const method = String(init?.method ?? "GET").toUpperCase();
                if (method !== "GET" && method !== "HEAD") {
                    reject(new TypeError("fetch: only GET and HEAD requests are supported"));
                    return;
                }
                try {
                    const [status, resolvedUrl, text] = __blitz_fetch_sync(url);
                    resolve(new Response(method === "HEAD" ? "" : text, {
                        status,
                        statusText: status === 200 ? "OK" : status === 404 ? "Not Found" : "",
                        url: resolvedUrl,
                    }));
                } catch (e) {
                    reject(e);
                }
            });
        };
    }

    if (typeof globalThis.DOMException !== "function") {
        const DOM_EXCEPTION_CODES = {
            IndexSizeError: 1,
            HierarchyRequestError: 3,
            WrongDocumentError: 4,
            InvalidCharacterError: 5,
            NoModificationAllowedError: 7,
            NotFoundError: 8,
            NotSupportedError: 9,
            InUseAttributeError: 10,
            InvalidStateError: 11,
            SyntaxError: 12,
            InvalidModificationError: 13,
            NamespaceError: 14,
            InvalidAccessError: 15,
            TypeMismatchError: 17,
            SecurityError: 18,
            NetworkError: 19,
            AbortError: 20,
            URLMismatchError: 21,
            QuotaExceededError: 22,
            TimeoutError: 23,
            InvalidNodeTypeError: 24,
            DataCloneError: 25,
        };
        globalThis.DOMException = class DOMException extends Error {
            constructor(message = "", name = "Error") {
                super(message);
                this.name = String(name);
            }
            get code() {
                return DOM_EXCEPTION_CODES[this.name] ?? 0;
            }
        };
        // Legacy code constants (on both the interface object and the prototype)
        const LEGACY_CODE_CONSTANTS = {
            INDEX_SIZE_ERR: 1,
            DOMSTRING_SIZE_ERR: 2,
            HIERARCHY_REQUEST_ERR: 3,
            WRONG_DOCUMENT_ERR: 4,
            INVALID_CHARACTER_ERR: 5,
            NO_DATA_ALLOWED_ERR: 6,
            NO_MODIFICATION_ALLOWED_ERR: 7,
            NOT_FOUND_ERR: 8,
            NOT_SUPPORTED_ERR: 9,
            INUSE_ATTRIBUTE_ERR: 10,
            INVALID_STATE_ERR: 11,
            SYNTAX_ERR: 12,
            INVALID_MODIFICATION_ERR: 13,
            NAMESPACE_ERR: 14,
            INVALID_ACCESS_ERR: 15,
            VALIDATION_ERR: 16,
            TYPE_MISMATCH_ERR: 17,
            SECURITY_ERR: 18,
            NETWORK_ERR: 19,
            ABORT_ERR: 20,
            URL_MISMATCH_ERR: 21,
            QUOTA_EXCEEDED_ERR: 22,
            TIMEOUT_ERR: 23,
            INVALID_NODE_TYPE_ERR: 24,
            DATA_CLONE_ERR: 25,
        };
        for (const [name, code] of Object.entries(LEGACY_CODE_CONSTANTS)) {
            DOMException[name] = code;
            DOMException.prototype[name] = code;
        }
    }

    // A Proxy wrapper for CSSStyleDeclaration objects which maps camelCase
    // property access (e.g. `style.gridTemplateColumns`) onto
    // `getPropertyValue`/`setProperty` calls with kebab-case property names.
    const KEBAB_OVERRIDES = { cssFloat: "float" };
    const toKebab = (prop) => {
        const override = KEBAB_OVERRIDES[prop];
        if (override) return override;
        const kebab = prop.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
        // Vendor-prefixed IDL attributes (webkitTransform, msFlex, mozUserSelect)
        // map to dashed prefixes (-webkit-transform, ...) per CSSOM. Uppercase
        // first letters (MozUserSelect) already gain the leading dash above.
        return /^(webkit|moz|ms)-/.test(kebab) ? "-" + kebab : kebab;
    };
    // camelCase (`gridTemplateColumns`) or kebab-case (`grid-template-columns`,
    // via indexed access) property names
    const isCssPropName = (prop) =>
        typeof prop === "string" && /^-?[a-zA-Z][a-zA-Z0-9-]*$/.test(prop);
    // Declarations exposing `length`/`item()` (e.g. `getComputedStyle()`) also
    // support indexed access (`style[0]`) and iteration.
    const isIndex = (prop) => typeof prop === "string" && /^\d+$/.test(prop);
    const isIndexed = (target) => typeof target.item === "function";
    const styleIterator = function* () {
        for (let i = 0; i < this.length; i++) yield this.item(i);
    };
    globalThis.__blitz_wrap_style = function (native) {
        return new Proxy(native, {
            get(target, prop) {
                if (isIndexed(target)) {
                    if (isIndex(prop)) {
                        return Number(prop) < target.length ? target.item(Number(prop)) : undefined;
                    }
                    if (prop === Symbol.iterator && !(prop in target)) {
                        return styleIterator.bind(target);
                    }
                }
                if (isCssPropName(prop) && !(prop in target)) {
                    return target.getPropertyValue(toKebab(prop));
                }
                const value = Reflect.get(target, prop, target);
                return typeof value === "function" ? value.bind(target) : value;
            },
            set(target, prop, value) {
                if (isCssPropName(prop) && !(prop in target)) {
                    target.setProperty(
                        toKebab(prop),
                        value === null || value === undefined ? "" : String(value)
                    );
                    return true;
                }
                return Reflect.set(target, prop, value, target);
            },
            // `'propertyName' in style` reports whether the engine supports the
            // property (used by WPT's computed-value test helpers)
            has(target, prop) {
                if (Reflect.has(target, prop)) return true;
                if (isIndexed(target)) {
                    if (isIndex(prop)) return Number(prop) < target.length;
                    if (prop === Symbol.iterator) return true;
                }
                return (
                    isCssPropName(prop) && __blitz_css_property_supported(toKebab(prop))
                );
            },
            ownKeys(target) {
                const keys = [];
                if (isIndexed(target)) {
                    for (let i = 0; i < target.length; i++) keys.push(String(i));
                }
                for (const key of Reflect.ownKeys(target)) {
                    if (!keys.includes(key)) keys.push(key);
                }
                return keys;
            },
            getOwnPropertyDescriptor(target, prop) {
                if (isIndexed(target) && isIndex(prop) && Number(prop) < target.length) {
                    return {
                        value: target.item(Number(prop)),
                        enumerable: true,
                        configurable: true,
                        writable: false,
                    };
                }
                return Reflect.getOwnPropertyDescriptor(target, prop);
            },
        });
    };

    // DOM interface objects (`Node`, `Element`, ...) wired up to the native
    // wrapper prototypes so that constants and `instanceof` checks work.
    const makeInterface = (name, proto) => {
        const iface = function () {
            throw new TypeError("Illegal constructor");
        };
        Object.defineProperty(iface, "name", { value: name, configurable: true });
        iface.prototype = proto;
        Object.defineProperty(proto, "constructor", {
            value: iface,
            writable: true,
            configurable: true,
        });
        return iface;
    };

    const documentProto = Object.getPrototypeOf(document);
    const nodeProto = Object.getPrototypeOf(documentProto);
    globalThis.Node = makeInterface("Node", nodeProto);
    globalThis.Document = makeInterface("Document", documentProto);
    globalThis.HTMLDocument = globalThis.Document;

    // Stub constructors for interfaces referenced by `instanceof` probes
    // (e.g. React probes `x instanceof HTMLInputElement`); without them such
    // probes throw "right-hand side of 'instanceof' is not an object". All
    // blitz-vibey-script elements share a single prototype, so tag-specific
    // interfaces cannot be truthfully modelled: these always answer false.
    // `Window`: the global object's interface (`window instanceof Window`).
    // Feature-detection code (and WPT's idlharness) uses `'Window' in self`
    // to tell a window global from a worker or ShadowRealm global.
    if (typeof globalThis.Window === "undefined") {
        const windowProto = Object.create(Object.getPrototypeOf(globalThis));
        let immutable = false;
        try {
            Object.setPrototypeOf(globalThis, windowProto);
        } catch (e) {
            // (raea) JavaScriptCore's global object has an immutable prototype: `Window.prototype` stands apart, and
            // `instanceof Window` answers for the global itself
            immutable = true;
        }
        globalThis.Window = makeInterface("Window", windowProto);
        if (immutable) {
            Object.defineProperty(globalThis.Window, Symbol.hasInstance, {
                value: (o) => o === globalThis || Object.prototype.isPrototypeOf.call(windowProto, o),
                configurable: true,
            });
        }
    }

    // (raea) Constructible `Event`, `CustomEvent` and `EventTarget`, and `dispatchEvent` on nodes and window.
    // Boa's runtime supplied these; JavaScriptCore has none, so without this block they were the
    // illegal-constructor stubs below and `new CustomEvent(...)`, `new AbortController()` (raea-native's
    // AbortSignal extends EventTarget) and `el.dispatchEvent(...)` threw while React mounted (Radix, Emotion and
    // Recharts pages rendered blank). Native listeners live in Rust; a JS mirror of each node's and window's
    // listeners lets a script-dispatched event reach them (capture, target and bubble phases).
    if (typeof globalThis.Event === "undefined") {
        const S = Symbol("event");
        class Event {
            constructor(type, init) {
                if (arguments.length === 0) {
                    throw new TypeError("Failed to construct 'Event': 1 argument required, but only 0 present.");
                }
                init = init || {};
                Object.defineProperty(this, S, {
                    value: {
                        type: String(type), bubbles: !!init.bubbles, cancelable: !!init.cancelable,
                        composed: !!init.composed, prevented: false, stop: false, stopNow: false,
                        target: null, currentTarget: null, phase: 0, path: [],
                        timeStamp: typeof performance !== "undefined" ? performance.now() : Date.now(),
                    },
                });
            }
            get type() { return this[S].type; }
            get bubbles() { return this[S].bubbles; }
            get cancelable() { return this[S].cancelable; }
            get composed() { return this[S].composed; }
            get defaultPrevented() { return this[S].prevented; }
            get target() { return this[S].target; }
            get srcElement() { return this[S].target; }
            get currentTarget() { return this[S].currentTarget; }
            get eventPhase() { return this[S].phase; }
            get timeStamp() { return this[S].timeStamp; }
            get isTrusted() { return false; }
            get returnValue() { return !this[S].prevented; }
            set returnValue(v) { if (!v) this.preventDefault(); }
            get cancelBubble() { return this[S].stop; }
            set cancelBubble(v) { if (v) this[S].stop = true; }
            composedPath() { return this[S].phase === 0 ? [] : this[S].path.slice(); }
            preventDefault() { if (this[S].cancelable) this[S].prevented = true; }
            stopPropagation() { this[S].stop = true; }
            stopImmediatePropagation() { this[S].stop = true; this[S].stopNow = true; }
        }
        Object.assign(Event, { NONE: 0, CAPTURING_PHASE: 1, AT_TARGET: 2, BUBBLING_PHASE: 3 });
        class CustomEvent extends Event {
            constructor(type, init) {
                super(type, init);
                Object.defineProperty(this, "detail", {
                    value: init && init.detail !== undefined ? init.detail : null,
                    configurable: true, enumerable: true,
                });
            }
        }

        // local change (raea, jetway #1): ProgressEvent, which three.js's FileLoader makes as it reads a response's
        // body as a stream
        class ProgressEvent extends Event {
            constructor(type, init) {
                super(type, init);
                for (const [k, d] of [["lengthComputable", false], ["loaded", 0], ["total", 0]]) {
                    Object.defineProperty(this, k, { value: init && init[k] !== undefined ? init[k] : d, configurable: true, enumerable: true });
                }
            }
        }

        // target -> Map(type -> [{ fn, capture, once, w }]); `w` is what the native side holds
        const mirror = new WeakMap();
        const entriesOf = (t, type, make) => {
            let m = mirror.get(t);
            if (!m) { if (!make) return null; mirror.set(t, (m = new Map())); }
            let list = m.get(type);
            if (!list && make) m.set(type, (list = []));
            return list || null;
        };
        const readOpts = (opts) => typeof opts === "object" && opts !== null
            ? { capture: !!opts.capture, once: !!opts.once, signal: opts.signal }
            : { capture: !!opts, once: false, signal: undefined };
        const call = (t, fn, ev) => {
            try {
                if (typeof fn === "function") return fn.call(t, ev);
                if (fn && typeof fn.handleEvent === "function") return fn.handleEvent(ev);
            } catch (e) {
                if (typeof globalThis.reportError === "function") globalThis.reportError(e);
                else Promise.reject(e);
            }
        };
        const drop = (t, type, entry, nativeRemove) => {
            const list = entriesOf(t, type, false);
            const i = list ? list.indexOf(entry) : -1;
            if (i >= 0) list.splice(i, 1);
            entry.removed = true;
            if (nativeRemove) nativeRemove.call(t, type, entry.w, entry.capture);
        };
        // add/remove/dispatch for `t`; nativeAdd/nativeRemove are the Rust-side registrations (none for a plain
        // EventTarget)
        const listen = (t, type, fn, opts, nativeAdd, nativeRemove) => {
            if (!fn || (typeof fn !== "function" && typeof fn !== "object")) return;
            type = String(type);
            const o = readOpts(opts);
            if (o.signal && o.signal.aborted) return;
            const list = entriesOf(t, type, true);
            if (list.some((e) => e.fn === fn && e.capture === o.capture)) return;
            const entry = { fn, capture: o.capture, once: o.once, w: fn };
            if (nativeAdd) {
                if (o.once || typeof fn !== "function") {
                    entry.w = function (ev) {
                        if (entry.once) drop(t, type, entry, null);
                        return call(this, fn, ev);
                    };
                }
                nativeAdd.call(t, type, entry.w, opts);
            }
            list.push(entry);
            if (o.signal && typeof o.signal.addEventListener === "function") {
                o.signal.addEventListener("abort", () => { if (!entry.removed) drop(t, type, entry, nativeRemove); });
            }
        };
        const unlisten = (t, type, fn, opts, nativeRemove) => {
            type = String(type);
            const capture = readOpts(opts).capture;
            const list = entriesOf(t, type, false);
            const entry = list && list.find((e) => e.fn === fn && e.capture === capture);
            if (entry) drop(t, type, entry, nativeRemove);
            else if (nativeRemove) nativeRemove.call(t, type, fn, opts);
        };
        const removers = new WeakMap();
        const invoke = (t, ev, phase) => {
            const st = ev[S];
            st.currentTarget = t;
            st.phase = phase;
            if (phase !== 1) {
                const h = t["on" + st.type];
                if (typeof h === "function" && call(t, h, ev) === false) ev.preventDefault();
                if (st.stopNow) return;
            }
            const list = entriesOf(t, st.type, false);
            if (!list) return;
            for (const e of list.slice()) {
                if (e.removed) continue;
                if (phase === 1 && !e.capture) continue;
                if (phase === 3 && e.capture) continue;
                if (e.once) drop(t, st.type, e, removers.get(t));
                call(t, e.fn, ev);
                if (st.stopNow) break;
            }
        };
        const dispatch = (t, ev) => {
            if (!(ev instanceof Event)) {
                throw new TypeError("Failed to execute 'dispatchEvent': parameter 1 is not of type 'Event'.");
            }
            const st = ev[S];
            const path = [t];
            if (t && typeof t.nodeType === "number") {
                for (let n = t.parentNode; n; n = n.parentNode) path.push(n);
                if (path[path.length - 1] === document) path.push(globalThis);
            }
            st.target = t;
            st.path = path;
            st.stop = false;
            st.stopNow = false;
            for (let i = path.length - 1; i > 0 && !st.stop; i--) invoke(path[i], ev, 1);
            if (!st.stop) {
                // at the target, capture listeners run before the others
                const list = entriesOf(t, st.type, false) || [];
                st.currentTarget = t;
                st.phase = 2;
                const cap = list.filter((e) => e.capture), rest = list.filter((e) => !e.capture);
                for (const e of cap) {
                    if (e.removed) continue;
                    if (e.once) drop(t, st.type, e, removers.get(t));
                    call(t, e.fn, ev);
                    if (st.stopNow) break;
                }
                if (!st.stopNow) {
                    const h = t["on" + st.type];
                    if (typeof h === "function" && call(t, h, ev) === false) ev.preventDefault();
                    for (const e of rest) {
                        if (st.stopNow) break;
                        if (e.removed) continue;
                        if (e.once) drop(t, st.type, e, removers.get(t));
                        call(t, e.fn, ev);
                    }
                }
            }
            if (st.bubbles) for (let i = 1; i < path.length && !st.stop; i++) invoke(path[i], ev, 3);
            st.currentTarget = null;
            st.phase = 0;
            return !st.prevented;
        };

        class EventTarget {
            addEventListener(type, fn, opts) { listen(this, type, fn, opts, null, null); }
            removeEventListener(type, fn, opts) { unlisten(this, type, fn, opts, null); }
            dispatchEvent(ev) { return dispatch(this, ev); }
        }
        const isNodeOrWindow = (o) => o === globalThis || (o != null && typeof o === "object" && typeof o.nodeType === "number");
        Object.defineProperty(EventTarget, Symbol.hasInstance, {
            value: (o) => isNodeOrWindow(o) || Object.prototype.isPrototypeOf.call(EventTarget.prototype, o),
            configurable: true,
        });

        // nodes (document included) and window keep their native registration, mirrored
        const wire = (holder, self) => {
            const nativeAdd = holder.addEventListener;
            const nativeRemove = holder.removeEventListener;
            if (typeof nativeAdd !== "function" || typeof nativeRemove !== "function") return;
            const define = (name, value) =>
                Object.defineProperty(holder, name, { value, writable: true, configurable: true });
            define("addEventListener", function (type, fn, opts) {
                const t = self || this;
                removers.set(t, nativeRemove);
                listen(t, type, fn, opts, nativeAdd, nativeRemove);
            });
            define("removeEventListener", function (type, fn, opts) { unlisten(self || this, type, fn, opts, nativeRemove); });
            define("dispatchEvent", function (ev) { return dispatch(self || this, ev); });
        };
        wire(nodeProto, null);
        wire(globalThis, globalThis);

        // local change (raea, jetway #10): native events (input, clicks, keys from the event driver) are `instanceof
        // Event`. Their prototype is the native one (dom/mod.rs __blitz_event_proto); chaining it onto Event's would
        // hand them Event's getters, which read state they don't have
        const nativeEventProto = globalThis.__blitz_event_proto;
        if (nativeEventProto) {
            Object.defineProperty(Event, Symbol.hasInstance, {
                configurable: true,
                value(o) {
                    return Function.prototype[Symbol.hasInstance].call(this, o)
                        || (this === Event && o !== null && typeof o === "object" && Object.getPrototypeOf(o) === nativeEventProto);
                },
            });
        }
        globalThis.Event = Event;
        globalThis.CustomEvent = CustomEvent;
        if (typeof globalThis.ProgressEvent === "undefined") globalThis.ProgressEvent = ProgressEvent;
        globalThis.EventTarget = EventTarget;
    }

    // (raea) HashChangeEvent and PopStateEvent, and `hashchange` fired when a script changes location.hash, as a
    // browser fires it: after the change, as a task, with the old and new URLs (pushState and replaceState fire
    // nothing, as in Chrome). Apps and routers construct these and listen for them (jetway #2, rendering Jetflow).
    if (typeof globalThis.Event === "function" && typeof globalThis.HashChangeEvent === "undefined") {
        class HashChangeEvent extends globalThis.Event {
            constructor(type, init) {
                super(type, init);
                const i = init || {};
                Object.defineProperty(this, "oldURL", { value: i.oldURL === undefined ? "" : String(i.oldURL), enumerable: true });
                Object.defineProperty(this, "newURL", { value: i.newURL === undefined ? "" : String(i.newURL), enumerable: true });
            }
        }
        globalThis.HashChangeEvent = HashChangeEvent;
        if (typeof globalThis.PopStateEvent === "undefined") {
            class PopStateEvent extends globalThis.Event {
                constructor(type, init) {
                    super(type, init);
                    Object.defineProperty(this, "state", { value: init && init.state !== undefined ? init.state : null, enumerable: true });
                }
            }
            globalThis.PopStateEvent = PopStateEvent;
        }
        const loc = globalThis.location;
        const d = loc && Object.getOwnPropertyDescriptor(loc, "hash");
        if (d && "value" in d && d.configurable) {
            let hash = d.value || "";
            Object.defineProperty(loc, "hash", {
                configurable: true,
                enumerable: true,
                get() { return hash; },
                set(v) {
                    let next = String(v);
                    if (next !== "" && next[0] !== "#") next = "#" + next;
                    if (next === "#") next = "";
                    if (next === hash) return;
                    const oldURL = String(loc.href);
                    const base = oldURL.split("#")[0];
                    hash = next;
                    loc.href = base + next;
                    const newURL = loc.href;
                    if (hashQuiet) return;
                    setTimeout(() => {
                        if (typeof globalThis.dispatchEvent === "function") {
                            globalThis.dispatchEvent(new HashChangeEvent("hashchange", { oldURL, newURL }));
                        }
                    }, 0);
                },
            });
        }
    }

    // (raea) PublicKeyCredential: present, as in every current browser, and reporting no platform authenticator
    // (a render has no user to verify), so feature checks pass and passkey flows take their fallback (jetway #3)
    if (typeof globalThis.PublicKeyCredential === "undefined") {
        class PublicKeyCredential {
            constructor() { throw new TypeError("Illegal constructor"); }
            static isUserVerifyingPlatformAuthenticatorAvailable() { return Promise.resolve(false); }
            static isConditionalMediationAvailable() { return Promise.resolve(false); }
            static getClientCapabilities() { return Promise.resolve({}); }
        }
        globalThis.PublicKeyCredential = PublicKeyCredential;
    }

    // local change (raea): SVG elements' own prototype (dom/mod.rs init_protos)
    if (typeof globalThis.SVGElement === "undefined" && globalThis.__blitz_svg_proto) {
        globalThis.SVGElement = makeInterface("SVGElement", globalThis.__blitz_svg_proto);
    }
    for (const name of [
        "EventTarget", "CharacterData", "Text", "Comment", "DocumentFragment",
        "HTMLInputElement", "HTMLTextAreaElement", "HTMLSelectElement",
        "HTMLButtonElement", "HTMLAnchorElement", "HTMLIFrameElement",
        "HTMLImageElement", "SVGElement",
        "Event", "CustomEvent", "UIEvent", "MouseEvent", "PointerEvent",
        "KeyboardEvent", "InputEvent", "FocusEvent",
    ]) {
        if (typeof globalThis[name] === "undefined") {
            globalThis[name] = makeInterface(name, {});
        }
    }
    if (document.documentElement) {
        const elementProto = Object.getPrototypeOf(document.documentElement);
        globalThis.Element = makeInterface("Element", elementProto);
        globalThis.HTMLElement = globalThis.Element;

        // `classList` (DOMTokenList), backed by the `class` attribute
        Object.defineProperty(elementProto, "classList", {
            configurable: true,
            get() {
                const el = this;
                const classes = () =>
                    (el.getAttribute("class") || "").split(/\s+/).filter(Boolean);
                const write = (list) => el.setAttribute("class", list.join(" "));
                return {
                    add(...names) {
                        const list = classes();
                        for (const name of names.map(String)) {
                            if (!list.includes(name)) list.push(name);
                        }
                        write(list);
                    },
                    remove(...names) {
                        const removed = names.map(String);
                        write(classes().filter((name) => !removed.includes(name)));
                    },
                    toggle(name, force) {
                        name = String(name);
                        const list = classes();
                        const has = list.includes(name);
                        const shouldHave = force !== undefined ? Boolean(force) : !has;
                        if (shouldHave && !has) list.push(name);
                        if (!shouldHave && has) list.splice(list.indexOf(name), 1);
                        write(list);
                        return shouldHave;
                    },
                    contains(name) {
                        return classes().includes(String(name));
                    },
                    item(index) {
                        return classes()[index] ?? null;
                    },
                    get length() {
                        return classes().length;
                    },
                    get value() {
                        return el.getAttribute("class") || "";
                    },
                    toString() {
                        return el.getAttribute("class") || "";
                    },
                    forEach(callback, thisArg) {
                        classes().forEach(callback, thisArg);
                    },
                };
            },
        });

        // `dataset` (DOMStringMap), backed by `data-*` attributes
        Object.defineProperty(elementProto, "dataset", {
            configurable: true,
            get() {
                const el = this;
                const toAttr = (prop) =>
                    "data-" + prop.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
                return new Proxy(Object.create(null), {
                    get(_, prop) {
                        if (typeof prop !== "string") return undefined;
                        const value = el.getAttribute(toAttr(prop));
                        return value === null ? undefined : value;
                    },
                    set(_, prop, value) {
                        if (typeof prop === "string") el.setAttribute(toAttr(prop), String(value));
                        return true;
                    },
                    has(_, prop) {
                        return typeof prop === "string" && el.getAttribute(toAttr(prop)) !== null;
                    },
                    deleteProperty(_, prop) {
                        if (typeof prop === "string") el.removeAttribute(toAttr(prop));
                        return true;
                    },
                });
            },
        });
    }

    // `CSS.escape` (the `CSS` namespace object itself, including `CSS.supports`,
    // is registered natively). Implements the CSSOM serialize-an-identifier
    // algorithm: https://drafts.csswg.org/cssom/#serialize-an-identifier
    if (typeof globalThis.CSS === "object" && typeof CSS.escape !== "function") {
        CSS.escape = function (value) {
            const string = String(value);
            const firstCodeUnit = string.charCodeAt(0);
            if (string.length === 1 && firstCodeUnit === 0x2d) {
                return "\\" + string;
            }
            let result = "";
            for (let index = 0; index < string.length; index++) {
                const codeUnit = string.charCodeAt(index);
                if (codeUnit === 0x0000) {
                    result += "\ufffd";
                } else if (
                    (codeUnit >= 0x0001 && codeUnit <= 0x001f) ||
                    codeUnit === 0x007f ||
                    (index === 0 && codeUnit >= 0x30 && codeUnit <= 0x39) ||
                    (index === 1 &&
                        codeUnit >= 0x30 &&
                        codeUnit <= 0x39 &&
                        firstCodeUnit === 0x2d)
                ) {
                    result += "\\" + codeUnit.toString(16) + " ";
                } else if (
                    codeUnit >= 0x0080 ||
                    codeUnit === 0x2d ||
                    codeUnit === 0x5f ||
                    (codeUnit >= 0x30 && codeUnit <= 0x39) ||
                    (codeUnit >= 0x41 && codeUnit <= 0x5a) ||
                    (codeUnit >= 0x61 && codeUnit <= 0x7a)
                ) {
                    result += string.charAt(index);
                } else {
                    result += "\\" + string.charAt(index);
                }
            }
            return result;
        };
    }

    // CSSOM stylesheet API (`document.styleSheets`, `element.sheet`,
    // `CSSStyleSheet`, `CSSRuleList`, `CSSRule` subclasses), backed by the
    // `__blitz_sheet_*` natives. Stylesheets are keyed by their owner node and
    // rules by a path of indices through nested rule lists.
    {
        const illegal = () => {
            throw new TypeError("Illegal constructor");
        };
        // Internal (constructor-bypassing) construction flag
        let constructing = false;
        const construct = (cls, init) => {
            constructing = true;
            try {
                const obj = new cls();
                init(obj);
                return obj;
            } finally {
                constructing = false;
            }
        };
        const internals = new WeakMap();
        const data = (obj) => {
            const d = internals.get(obj);
            if (!d) throw new TypeError("Illegal invocation");
            return d;
        };

        // Array-like list objects (`StyleSheetList`, `CSSRuleList`, `MediaList`)
        // support indexed access via a Proxy
        const indexedProxy = (target) =>
            new Proxy(target, {
                // Getters and methods run against the target (not the proxy):
                // their internal data is keyed by the target object
                get(t, prop) {
                    if (typeof prop === "string" && /^\d+$/.test(prop)) {
                        const item = t.item(Number(prop));
                        return item === null ? undefined : item;
                    }
                    const value = Reflect.get(t, prop, t);
                    return typeof value === "function" ? value.bind(t) : value;
                },
                set(t, prop, value) {
                    return Reflect.set(t, prop, value, t);
                },
                has(t, prop) {
                    if (typeof prop === "string" && /^\d+$/.test(prop)) {
                        return Number(prop) < t.length;
                    }
                    return Reflect.has(t, prop);
                },
                ownKeys(t) {
                    const keys = [];
                    for (let i = 0; i < t.length; i++) keys.push(String(i));
                    return keys.concat(Reflect.ownKeys(t));
                },
                getOwnPropertyDescriptor(t, prop) {
                    if (typeof prop === "string" && /^\d+$/.test(prop) && Number(prop) < t.length) {
                        return { value: t.item(Number(prop)), enumerable: true, configurable: true, writable: false };
                    }
                    return Reflect.getOwnPropertyDescriptor(t, prop);
                },
            });
        const defineIterable = (proto) => {
            proto[Symbol.iterator] = function* () {
                for (let i = 0; i < this.length; i++) yield this.item(i);
            };
            proto.forEach = function (callback, thisArg) {
                for (let i = 0; i < this.length; i++) callback.call(thisArg, this.item(i), i, this);
            };
        };

        class MediaList {
            constructor() {
                if (!constructing) illegal();
            }
            get mediaText() {
                return data(this).text;
            }
            get length() {
                return this.mediaText === "" ? 0 : this.mediaText.split(",").length;
            }
            item(index) {
                if (this.mediaText === "") return null;
                return this.mediaText.split(",").map((s) => s.trim())[index] ?? null;
            }
            toString() {
                return this.mediaText;
            }
        }
        defineIterable(MediaList.prototype);
        const makeMediaList = (text) =>
            indexedProxy(construct(MediaList, (m) => internals.set(m, { text })));

        class StyleSheet {
            constructor() {
                if (!constructing) illegal();
            }
            get type() {
                return "text/css";
            }
            get ownerNode() {
                return data(this).owner;
            }
            get href() {
                const owner = data(this).owner;
                return owner.localName === "link" ? owner.getAttribute("href") : null;
            }
            get title() {
                return data(this).owner.getAttribute("title");
            }
            get media() {
                return makeMediaList(data(this).owner.getAttribute("media") || "");
            }
            get parentStyleSheet() {
                return null;
            }
            get ownerRule() {
                return null;
            }
            get disabled() {
                return __blitz_sheet_disabled(data(this).owner);
            }
            set disabled(value) {
                __blitz_sheet_set_disabled(data(this).owner, !!value);
            }
        }

        // `CSSStyleDeclaration` for the declarations of a rule (`CSSStyleRule.style`, ...)
        class RuleStyleDeclaration {
            constructor() {
                if (!constructing) illegal();
            }
            get cssText() {
                const d = data(this);
                return __blitz_sheet_style_css_text(d.owner, d.path);
            }
            set cssText(value) {
                const d = data(this);
                __blitz_sheet_style_set_css_text(d.owner, d.path, String(value));
                touchSheet(data(d.rule).sheet);
            }
            get length() {
                const d = data(this);
                return __blitz_sheet_style_property_names(d.owner, d.path).length;
            }
            item(index) {
                const d = data(this);
                return __blitz_sheet_style_property_names(d.owner, d.path)[index] ?? "";
            }
            getPropertyValue(name) {
                const d = data(this);
                return __blitz_sheet_style_get(d.owner, d.path, String(name))[0];
            }
            getPropertyPriority(name) {
                const d = data(this);
                return __blitz_sheet_style_get(d.owner, d.path, String(name))[1] ? "important" : "";
            }
            setProperty(name, value, priority) {
                const d = data(this);
                value = value === null || value === undefined ? "" : String(value);
                const important = String(priority ?? "").toLowerCase() === "important";
                __blitz_sheet_style_set(d.owner, d.path, String(name), value, important);
                touchSheet(data(d.rule).sheet);
            }
            removeProperty(name) {
                const d = data(this);
                const removed = __blitz_sheet_style_remove(d.owner, d.path, String(name));
                touchSheet(data(d.rule).sheet);
                return removed;
            }
            get parentRule() {
                return data(this).rule;
            }
        }
        defineIterable(RuleStyleDeclaration.prototype);
        const makeRuleStyle = (owner, path, rule) =>
            __blitz_wrap_style(
                indexedProxy(
                    construct(RuleStyleDeclaration, (s) => internals.set(s, { owner, path, rule }))
                )
            );

        class CSSRuleList {
            constructor() {
                if (!constructing) illegal();
            }
            get length() {
                const d = data(this);
                return Math.max(0, __blitz_sheet_rule_count(d.owner, d.path));
            }
            item(index) {
                const d = data(this);
                index = Number(index);
                if (!(index >= 0 && index < this.length)) return null;
                return ruleAt(d.sheet, d.owner, d.path.concat(index), d.parentRule);
            }
        }
        defineIterable(CSSRuleList.prototype);
        const makeRuleList = (sheet, owner, path, parentRule) =>
            indexedProxy(
                construct(CSSRuleList, (l) => internals.set(l, { sheet, owner, path, parentRule }))
            );

        const CSS_RULE_TYPES = {
            STYLE_RULE: 1,
            CHARSET_RULE: 2,
            IMPORT_RULE: 3,
            MEDIA_RULE: 4,
            FONT_FACE_RULE: 5,
            PAGE_RULE: 6,
            KEYFRAMES_RULE: 7,
            KEYFRAME_RULE: 8,
            MARGIN_RULE: 9,
            NAMESPACE_RULE: 10,
            COUNTER_STYLE_RULE: 11,
            SUPPORTS_RULE: 12,
            DOCUMENT_RULE: 13,
            FONT_FEATURE_VALUES_RULE: 14,
            VIEWPORT_RULE: 15,
            REGION_STYLE_RULE: 16,
        };

        class CSSRule {
            constructor() {
                if (!constructing) illegal();
            }
            get type() {
                return data(this).info().type;
            }
            get cssText() {
                return data(this).info().cssText;
            }
            set cssText(_) {}
            get parentRule() {
                return data(this).parentRule;
            }
            get parentStyleSheet() {
                return data(this).sheet;
            }
        }
        for (const [name, value] of Object.entries(CSS_RULE_TYPES)) {
            CSSRule[name] = value;
            CSSRule.prototype[name] = value;
        }

        const attrGetter = (name, convert) =>
            function () {
                const value = data(this).info().attrs[name];
                return convert ? convert(value) : value;
            };
        const defineAttrs = (cls, attrs) => {
            for (const [name, convert] of Object.entries(attrs)) {
                Object.defineProperty(cls.prototype, name, {
                    get: attrGetter(name, convert),
                    configurable: true,
                    enumerable: true,
                });
            }
        };
        const cssRulesGetter = function () {
            const d = data(this);
            return d.rules || (d.rules = makeRuleList(d.sheet, d.owner, d.path, this));
        };
        const groupingMethods = {
            get cssRules() {
                return cssRulesGetter.call(this);
            },
            insertRule(rule, index) {
                const d = data(this);
                index = index === undefined ? 0 : Number(index) >>> 0;
                const result = __blitz_sheet_insert_rule(d.owner, d.path, String(rule), index);
                d.sheet && invalidateRules(d.sheet);
                return result;
            },
            deleteRule(index) {
                const d = data(this);
                __blitz_sheet_delete_rule(d.owner, d.path, Number(index) >>> 0);
                d.sheet && invalidateRules(d.sheet);
            },
        };
        const styleGetter = {
            get style() {
                const d = data(this);
                return d.style || (d.style = makeRuleStyle(d.owner, d.path, this));
            },
        };

        const ruleClasses = {};
        const defineRuleClass = (name, base, mixins, attrs) => {
            const cls = class extends base {
                constructor() {
                    super();
                }
            };
            Object.defineProperty(cls, "name", { value: name, configurable: true });
            for (const mixin of mixins) {
                Object.defineProperties(cls.prototype, Object.getOwnPropertyDescriptors(mixin));
            }
            if (attrs) defineAttrs(cls, attrs);
            ruleClasses[name] = cls;
            globalThis[name] = cls;
            return cls;
        };
        const str = (v) => (v === undefined ? "" : v);
        const CSSGroupingRule = defineRuleClass("CSSGroupingRule", CSSRule, [groupingMethods]);
        const CSSConditionRule = defineRuleClass("CSSConditionRule", CSSGroupingRule, [], {
            conditionText: str,
        });
        defineRuleClass("CSSStyleRule", CSSGroupingRule, [styleGetter], { selectorText: str });
        Object.defineProperty(ruleClasses.CSSStyleRule.prototype, "selectorText", {
            get: attrGetter("selectorText", str),
            set(value) {
                const d = data(this);
                __blitz_sheet_set_selector_text(d.owner, d.path, String(value));
                touchSheet(d.sheet);
            },
            configurable: true,
            enumerable: true,
        });
        defineRuleClass("CSSNestedDeclarations", CSSRule, [styleGetter]);
        defineRuleClass("CSSMediaRule", CSSConditionRule, [
            {
                get media() {
                    return makeMediaList(this.conditionText);
                },
            },
        ]);
        defineRuleClass("CSSSupportsRule", CSSConditionRule, []);
        defineRuleClass("CSSContainerRule", CSSConditionRule, [], {
            containerName: () => "",
            containerQuery: str,
        });
        defineRuleClass("CSSMozDocumentRule", CSSConditionRule, []);
        defineRuleClass("CSSLayerBlockRule", CSSGroupingRule, [], { name: str });
        defineRuleClass("CSSLayerStatementRule", CSSRule, [], {
            nameList: (v) => (v ? v.split(",").map((s) => s.trim()) : []),
        });
        defineRuleClass("CSSScopeRule", CSSGroupingRule, []);
        defineRuleClass("CSSStartingStyleRule", CSSGroupingRule, []);
        defineRuleClass("CSSPageRule", CSSGroupingRule, [styleGetter], { selectorText: str });
        defineRuleClass("CSSMarginRule", CSSRule, [styleGetter], { name: str });
        defineRuleClass("CSSImportRule", CSSRule, [], {
            href: str,
            media: (v) => makeMediaList(str(v)),
            styleSheet: () => null,
            layerName: () => null,
            supportsText: () => null,
        });
        defineRuleClass("CSSNamespaceRule", CSSRule, [], { namespaceURI: str, prefix: str });
        defineRuleClass("CSSFontFaceRule", CSSRule, [styleGetter]);
        defineRuleClass("CSSFontFeatureValuesRule", CSSRule, [], { fontFamily: str });
        defineRuleClass("CSSFontPaletteValuesRule", CSSRule, [], {
            name: str,
            fontFamily: str,
            basePalette: str,
            overrideColors: str,
        });
        defineRuleClass("CSSCounterStyleRule", CSSRule, [], { name: str });
        defineRuleClass("CSSPropertyRule", CSSRule, [], {
            name: str,
            syntax: str,
            inherits: (v) => v === "true",
            initialValue: (v) => (v ? v : null),
        });
        defineRuleClass("CSSPositionTryRule", CSSRule, [styleGetter], { name: str });
        defineRuleClass("CSSViewTransitionRule", CSSRule, []);
        defineRuleClass("CSSAppearanceBaseRule", CSSGroupingRule, []);
        defineRuleClass("CSSCustomMediaRule", CSSRule, []);
        defineRuleClass("CSSKeyframeRule", CSSRule, [styleGetter], { keyText: str });
        defineRuleClass(
            "CSSKeyframesRule",
            CSSRule,
            [
                {
                    get cssRules() {
                        return cssRulesGetter.call(this);
                    },
                    get length() {
                        return this.cssRules.length;
                    },
                    appendRule(rule) {
                        const d = data(this);
                        __blitz_sheet_insert_rule(d.owner, d.path, String(rule), 0);
                        d.sheet && invalidateRules(d.sheet);
                    },
                    deleteRule(select) {
                        const index = findKeyframeIndex(this, select);
                        if (index === -1) return;
                        const d = data(this);
                        __blitz_sheet_delete_rule(d.owner, d.path, index);
                        d.sheet && invalidateRules(d.sheet);
                    },
                    findRule(select) {
                        const index = findKeyframeIndex(this, select);
                        return index === -1 ? null : this.cssRules.item(index);
                    },
                },
            ],
            { name: str }
        );
        // Match a keyframe selector string the way `findRule`/`deleteRule` do
        // (normalising `from`/`to` and whitespace), returning the last match
        const findKeyframeIndex = (keyframesRule, select) => {
            const normalise = (s) =>
                String(s)
                    .split(",")
                    .map((k) => k.trim().toLowerCase())
                    .map((k) => (k === "from" ? "0%" : k === "to" ? "100%" : k))
                    .join(", ");
            const wanted = normalise(select);
            const rules = keyframesRule.cssRules;
            for (let i = rules.length - 1; i >= 0; i--) {
                if (normalise(rules.item(i).keyText) === wanted) return i;
            }
            return -1;
        };

        // Rule wrapper objects are cached per stylesheet by path so that repeated
        // `cssRules[i]` accesses return the same object; the cache is dropped
        // whenever the rule list is mutated (indices shift).
        const ruleCaches = new WeakMap();
        // Per-sheet mutation counter, so that rule wrappers re-fetch their
        // (serialized) info from the native side only when it may have changed.
        // The document-wide native counter is folded in so that replacing a
        // node's stylesheet (e.g. editing a `<style>`'s text) is detected too.
        const generations = new WeakMap();
        const sheetGeneration = (sheet) => (sheet && generations.get(sheet)) || 0;
        const generation = (sheet) =>
            __blitz_stylesheet_generation() + ":" + sheetGeneration(sheet);
        const touchSheet = (sheet) => {
            if (sheet) generations.set(sheet, sheetGeneration(sheet) + 1);
        };
        const invalidateRules = (sheet) => {
            ruleCaches.delete(sheet);
            touchSheet(sheet);
        };
        const ruleAt = (sheet, owner, path, parentRule) => {
            const key = path.join("/");
            let cache = sheet && ruleCaches.get(sheet);
            if (cache && cache.generation !== generation(sheet)) {
                ruleCaches.delete(sheet);
                cache = undefined;
            }
            if (cache && cache.has(key)) return cache.get(key);
            let info = __blitz_sheet_rule_info(owner, path);
            if (info === null) return null;
            let infoGeneration = generation(sheet);
            const cls = ruleClasses[info.interface] || CSSRule;
            const rule = construct(cls, (r) =>
                internals.set(r, {
                    sheet,
                    owner,
                    path,
                    parentRule,
                    info: () => {
                        const current = generation(sheet);
                        if (current !== infoGeneration) {
                            info = __blitz_sheet_rule_info(owner, path) || info;
                            infoGeneration = current;
                        }
                        return info;
                    },
                })
            );
            if (sheet) {
                if (!cache) {
                    cache = new Map();
                    cache.generation = infoGeneration;
                    ruleCaches.set(sheet, cache);
                }
                cache.set(key, rule);
            }
            return rule;
        };

        class CSSStyleSheet extends StyleSheet {
            constructor() {
                super();
            }
            get cssRules() {
                const d = data(this);
                return d.rules || (d.rules = makeRuleList(this, d.owner, [], null));
            }
            get rules() {
                return this.cssRules;
            }
            insertRule(rule, index) {
                const d = data(this);
                index = index === undefined ? 0 : Number(index) >>> 0;
                const result = __blitz_sheet_insert_rule(d.owner, [], String(rule), index);
                invalidateRules(this);
                return result;
            }
            deleteRule(index) {
                const d = data(this);
                __blitz_sheet_delete_rule(d.owner, [], Number(index) >>> 0);
                invalidateRules(this);
            }
            addRule(selector, block, index) {
                const rule = (selector ?? "undefined") + " {" + (block ?? "undefined") + "}";
                this.insertRule(rule, index === undefined ? this.cssRules.length : index);
                return -1;
            }
            removeRule(index) {
                this.deleteRule(index ?? 0);
            }
        }

        class StyleSheetList {
            constructor() {
                if (!constructing) illegal();
            }
            get length() {
                return data(this).sheets.length;
            }
            item(index) {
                return data(this).sheets[index] ?? null;
            }
        }
        defineIterable(StyleSheetList.prototype);

        // One `CSSStyleSheet` object per owner node (node wrappers are cached,
        // so identity is stable)
        const sheets = new WeakMap();
        globalThis.__blitz_sheet_for_node = (node) => {
            if (!__blitz_node_has_stylesheet(node)) return null;
            let sheet = sheets.get(node);
            if (!sheet) {
                sheet = construct(CSSStyleSheet, (s) => internals.set(s, { owner: node }));
                sheets.set(node, sheet);
            }
            return sheet;
        };
        globalThis.__blitz_style_sheets = () => {
            const list = __blitz_stylesheet_owner_nodes().map(__blitz_sheet_for_node);
            return indexedProxy(construct(StyleSheetList, (l) => internals.set(l, { sheets: list })));
        };

        globalThis.StyleSheet = StyleSheet;
        globalThis.CSSStyleSheet = CSSStyleSheet;
        globalThis.StyleSheetList = StyleSheetList;
        globalThis.CSSRuleList = CSSRuleList;
        globalThis.CSSRule = CSSRule;
        globalThis.MediaList = MediaList;
    }

    // `document.fonts` (FontFaceSet) stub: all fonts report as loaded
    const fontFaceSet = {
        status: "loaded",
        size: 0,
        check: () => true,
        load: () => Promise.resolve([]),
        forEach() {},
        addEventListener() {},
        removeEventListener() {},
        dispatchEvent() {
            return true;
        },
    };
    fontFaceSet.ready = Promise.resolve(fontFaceSet);
    document.fonts = fontFaceSet;

    const NODE_CONSTANTS = {
        ELEMENT_NODE: 1,
        ATTRIBUTE_NODE: 2,
        TEXT_NODE: 3,
        CDATA_SECTION_NODE: 4,
        ENTITY_REFERENCE_NODE: 5,
        ENTITY_NODE: 6,
        PROCESSING_INSTRUCTION_NODE: 7,
        COMMENT_NODE: 8,
        DOCUMENT_NODE: 9,
        DOCUMENT_TYPE_NODE: 10,
        DOCUMENT_FRAGMENT_NODE: 11,
        NOTATION_NODE: 12,
    };
    for (const [name, value] of Object.entries(NODE_CONSTANTS)) {
        globalThis.Node[name] = value;
        nodeProto[name] = value;
    }
})();
