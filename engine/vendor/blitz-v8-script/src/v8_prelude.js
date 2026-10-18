// Web APIs that Boa's boa_runtime extensions supplied (console, URL, URLSearchParams, TextEncoder,
// TextDecoder, atob/btoa, structuredClone) and the document clock behind Date. V8 is a bare
// ECMAScript engine, so they are defined here, before the shared bootstrap. Natives:
// __blitz_console(level, message), __blitz_date_now(), __blitz_url_parse(input, base).
(function () {
    const g = globalThis;

    // --- console: routed to the log crate (target js_console), like Boa's LogCrateLogger ----
    const fmt = (args) => args.map((a) => {
        if (typeof a === "string") return a;
        if (a instanceof Error) return a.stack || String(a);
        try { return typeof a === "object" && a !== null ? JSON.stringify(a) : String(a); } catch (e) { return String(a); }
    }).join(" ");
    const console = {};
    for (const level of ["log", "info", "warn", "error", "debug", "trace", "dir", "table"]) {
        console[level] = (...args) => __blitz_console(level, fmt(args));
    }
    const counts = new Map(), timers = new Map();
    console.assert = (cond, ...args) => { if (!cond) __blitz_console("error", "Assertion failed: " + fmt(args)); };
    console.count = (label = "default") => { counts.set(label, (counts.get(label) || 0) + 1); __blitz_console("log", label + ": " + counts.get(label)); };
    console.countReset = (label = "default") => counts.delete(label);
    console.time = (label = "default") => timers.set(label, Date.now());
    console.timeEnd = (label = "default") => { __blitz_console("log", label + ": " + (Date.now() - (timers.get(label) || 0)) + "ms"); timers.delete(label); };
    console.timeLog = (label = "default") => __blitz_console("log", label + ": " + (Date.now() - (timers.get(label) || 0)) + "ms");
    console.group = console.groupCollapsed = console.groupEnd = () => {};
    Object.defineProperty(g, "console", { value: console, writable: true, configurable: true, enumerable: false });

    // --- Date on the document clock (virtual time stops it, as with Boa's clock adapter) -----
    const RealDate = Date;
    const now = () => __blitz_date_now();
    function DocDate(...args) {
        if (!new.target) return new RealDate(now()).toString();
        return Reflect.construct(RealDate, args.length ? args : [now()], new.target);
    }
    Object.setPrototypeOf(DocDate, RealDate);
    DocDate.prototype = RealDate.prototype;
    Object.defineProperty(RealDate.prototype, "constructor", { value: DocDate, writable: true, configurable: true });
    DocDate.now = now;
    DocDate.parse = RealDate.parse;
    DocDate.UTC = RealDate.UTC;
    Object.defineProperty(DocDate, "name", { value: "Date" });
    g.Date = DocDate;

    // --- atob / btoa ------------------------------------------------------------------------
    const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const domError = (name, msg) => (typeof DOMException === "function" ? new DOMException(msg, name) : new Error(msg));
    g.btoa = function btoa(input) {
        const s = String(input);
        let out = "";
        for (let i = 0; i < s.length; i += 3) {
            const a = s.charCodeAt(i), b = s.charCodeAt(i + 1), c = s.charCodeAt(i + 2);
            if (a > 255 || b > 255 || c > 255) throw domError("InvalidCharacterError", "btoa: the string contains characters outside of the Latin1 range");
            const n = (a << 16) | ((b || 0) << 8) | (c || 0);
            out += B64[(n >> 18) & 63] + B64[(n >> 12) & 63] + (i + 1 < s.length ? B64[(n >> 6) & 63] : "=") + (i + 2 < s.length ? B64[n & 63] : "=");
        }
        return out;
    };
    g.atob = function atob(input) {
        let s = String(input).replace(/[\t\n\f\r ]/g, "");
        if (s.length % 4 === 0) s = s.replace(/==?$/, "");
        if (s.length % 4 === 1 || /[^A-Za-z0-9+/]/.test(s)) throw domError("InvalidCharacterError", "atob: the string to be decoded is not correctly encoded");
        let out = "", bits = 0, acc = 0;
        for (const ch of s) {
            acc = (acc << 6) | B64.indexOf(ch);
            bits += 6;
            if (bits >= 8) { bits -= 8; out += String.fromCharCode((acc >> bits) & 255); }
        }
        return out;
    };

    // --- TextEncoder / TextDecoder (UTF-8; TextDecoder also latin1/utf-16le) ------------------
    class TextEncoder {
        get encoding() { return "utf-8"; }
        encode(input = "") {
            const s = String(input), out = [];
            for (let i = 0; i < s.length; i++) {
                let c = s.charCodeAt(i);
                if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length) {
                    const d = s.charCodeAt(i + 1);
                    if (d >= 0xdc00 && d <= 0xdfff) { c = 0x10000 + ((c - 0xd800) << 10) + (d - 0xdc00); i++; }
                    else c = 0xfffd;
                } else if (c >= 0xd800 && c <= 0xdfff) c = 0xfffd;
                if (c < 0x80) out.push(c);
                else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 63));
                else if (c < 0x10000) out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
                else out.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
            }
            return new Uint8Array(out);
        }
        encodeInto(input, dest) {
            const bytes = this.encode(input);
            const n = Math.min(bytes.length, dest.length);
            dest.set(bytes.subarray(0, n));
            return { read: String(input).length, written: n };
        }
    }
    class TextDecoder {
        constructor(label = "utf-8", options = {}) {
            const l = String(label).trim().toLowerCase();
            this._enc = l === "utf8" || l === "unicode-1-1-utf-8" ? "utf-8" : l === "latin1" || l === "iso-8859-1" || l === "ascii" || l === "us-ascii" || l === "windows-1252" ? "windows-1252" : l === "utf-16le" || l === "utf-16" ? "utf-16le" : l;
            if (!["utf-8", "windows-1252", "utf-16le"].includes(this._enc)) throw new RangeError(`The encoding label provided ('${label}') is invalid.`);
            this.fatal = !!options.fatal;
            this.ignoreBOM = !!options.ignoreBOM;
        }
        get encoding() { return this._enc; }
        decode(input) {
            if (input === undefined) return "";
            const b = input instanceof ArrayBuffer ? new Uint8Array(input) : new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
            if (this._enc === "windows-1252") { let s = ""; for (const x of b) s += String.fromCharCode(x); return s; }
            if (this._enc === "utf-16le") { let s = ""; for (let i = 0; i + 1 < b.length; i += 2) s += String.fromCharCode(b[i] | (b[i + 1] << 8)); return s; }
            let i = 0, out = "";
            if (!this.ignoreBOM && b[0] === 0xef && b[1] === 0xbb && b[2] === 0xbf) i = 3;
            const parts = [];
            const bad = () => { if (this.fatal) throw new TypeError("The encoded data was not valid."); return 0xfffd; };
            while (i < b.length) {
                const c = b[i];
                let cp;
                if (c < 0x80) { cp = c; i += 1; }
                else if (c >= 0xc2 && c < 0xe0 && (b[i + 1] & 0xc0) === 0x80) { cp = ((c & 31) << 6) | (b[i + 1] & 63); i += 2; }
                else if (c >= 0xe0 && c < 0xf0 && (b[i + 1] & 0xc0) === 0x80 && (b[i + 2] & 0xc0) === 0x80) { cp = ((c & 15) << 12) | ((b[i + 1] & 63) << 6) | (b[i + 2] & 63); i += 3; }
                else if (c >= 0xf0 && c < 0xf5 && (b[i + 1] & 0xc0) === 0x80 && (b[i + 2] & 0xc0) === 0x80 && (b[i + 3] & 0xc0) === 0x80) { cp = ((c & 7) << 18) | ((b[i + 1] & 63) << 12) | ((b[i + 2] & 63) << 6) | (b[i + 3] & 63); i += 4; }
                else { cp = bad(); i += 1; }
                if (cp > 0xffff) { cp -= 0x10000; parts.push(0xd800 + (cp >> 10), 0xdc00 + (cp & 1023)); } else parts.push(cp);
                if (parts.length > 8192) { out += String.fromCharCode.apply(null, parts); parts.length = 0; }
            }
            return out + String.fromCharCode.apply(null, parts);
        }
    }
    g.TextEncoder = TextEncoder;
    g.TextDecoder = TextDecoder;

    // --- URL / URLSearchParams (parsing by the url crate through __blitz_url_parse) -----------
    const FIELDS = ["href", "protocol", "username", "password", "host", "hostname", "port", "pathname", "search", "hash", "origin"];
    const encodeForm = (s) => encodeURIComponent(s).replace(/%20/g, "+").replace(/[!'()~]/g, (c) => "%" + c.charCodeAt(0).toString(16).toUpperCase());
    const decodeForm = (s) => { try { return decodeURIComponent(s.replace(/\+/g, " ")); } catch (e) { return s; } };
    class URLSearchParams {
        #list = [];
        #url = null;
        constructor(init = "") {
            if (init instanceof URLSearchParams) this.#list = init.#list.map((p) => p.slice());
            else if (Array.isArray(init) || (init && typeof init === "object" && typeof init[Symbol.iterator] === "function")) {
                for (const pair of init) this.#list.push([String(pair[0]), String(pair[1])]);
            } else if (init && typeof init === "object") {
                for (const k of Object.keys(init)) this.#list.push([k, String(init[k])]);
            } else this.#parse(String(init));
        }
        #parse(s) {
            this.#list = [];
            if (s.startsWith("?")) s = s.slice(1);
            for (const part of s.split("&")) {
                if (!part) continue;
                const i = part.indexOf("=");
                this.#list.push(i < 0 ? [decodeForm(part), ""] : [decodeForm(part.slice(0, i)), decodeForm(part.slice(i + 1))]);
            }
        }
        static _attach(params, url) { params.#url = url; }
        static _reparse(params, search) { params.#parse(search); }
        #update() { if (this.#url) { const s = this.toString(); this.#url._setSearch(s ? "?" + s : ""); } }
        get size() { return this.#list.length; }
        append(k, v) { this.#list.push([String(k), String(v)]); this.#update(); }
        delete(k, v) { k = String(k); this.#list = this.#list.filter(([a, b]) => !(a === k && (v === undefined || b === String(v)))); this.#update(); }
        get(k) { k = String(k); const p = this.#list.find(([a]) => a === k); return p ? p[1] : null; }
        getAll(k) { k = String(k); return this.#list.filter(([a]) => a === k).map(([, b]) => b); }
        has(k, v) { k = String(k); return this.#list.some(([a, b]) => a === k && (v === undefined || b === String(v))); }
        set(k, v) {
            k = String(k); v = String(v);
            const i = this.#list.findIndex(([a]) => a === k);
            if (i < 0) this.#list.push([k, v]);
            else { this.#list[i][1] = v; this.#list = this.#list.filter(([a], j) => j <= i || a !== k); }
            this.#update();
        }
        sort() { this.#list.sort((x, y) => (x[0] < y[0] ? -1 : x[0] > y[0] ? 1 : 0)); this.#update(); }
        forEach(cb, thisArg) { for (const [k, v] of this.#list) cb.call(thisArg, v, k, this); }
        *entries() { for (const p of this.#list) yield [p[0], p[1]]; }
        *keys() { for (const [k] of this.#list) yield k; }
        *values() { for (const [, v] of this.#list) yield v; }
        [Symbol.iterator]() { return this.entries(); }
        toString() { return this.#list.map(([k, v]) => encodeForm(k) + "=" + encodeForm(v)).join("&"); }
    }
    class URL {
        #f;
        #params = null;
        constructor(input, base) {
            const f = __blitz_url_parse(String(input), base === undefined ? null : String(base));
            if (f === null) throw new TypeError(`Failed to construct 'URL': Invalid URL`);
            this.#f = f;
        }
        static canParse(input, base) { return __blitz_url_parse(String(input), base === undefined ? null : String(base)) !== null; }
        static parse(input, base) { try { return new URL(input, base); } catch (e) { return null; } }
        #set(field, value) {
            const href = __blitz_url_set(this.#f[0], field, String(value));
            if (href !== null) {
                this.#f = __blitz_url_parse(href, null);
                if (this.#params) URLSearchParams._reparse(this.#params, this.search);
            }
        }
        _setSearch(s) { this.#set("search", s); }
        get searchParams() {
            if (!this.#params) { this.#params = new URLSearchParams(this.search); URLSearchParams._attach(this.#params, this); }
            return this.#params;
        }
        toString() { return this.href; }
        toJSON() { return this.href; }
        static {
            // Accessors are defined here so they can read the private fields
            FIELDS.forEach((name, i) => {
                Object.defineProperty(URL.prototype, name, {
                    get() { return this.#f[i]; },
                    set: name === "origin" ? undefined : function (v) { this.#set(name, v); },
                    enumerable: true, configurable: true,
                });
            });
        }
    }
    g.URL = URL;
    g.URLSearchParams = URLSearchParams;

    // --- structuredClone (the HTML structured clone algorithm, common types) -----------------
    g.structuredClone = function structuredClone(value) {
        const seen = new Map();
        const clone = (v) => {
            if (v === null || typeof v !== "object") {
                if (typeof v === "function" || typeof v === "symbol") throw domError("DataCloneError", `${String(v)} could not be cloned.`);
                return v;
            }
            if (seen.has(v)) return seen.get(v);
            let out;
            if (v instanceof RealDate) out = new RealDate(v.getTime());
            else if (v instanceof RegExp) out = new RegExp(v.source, v.flags);
            else if (v instanceof ArrayBuffer) out = v.slice(0);
            else if (ArrayBuffer.isView(v)) out = new v.constructor(v.buffer.slice(0), v.byteOffset, v instanceof DataView ? v.byteLength : v.length);
            else if (v instanceof Map) { out = new Map(); seen.set(v, out); for (const [k, x] of v) out.set(clone(k), clone(x)); return out; }
            else if (v instanceof Set) { out = new Set(); seen.set(v, out); for (const x of v) out.add(clone(x)); return out; }
            else if (v instanceof Error) { out = new (g[v.name] || Error)(v.message); if (v.stack) out.stack = v.stack; }
            else if (Array.isArray(v)) { out = []; seen.set(v, out); v.forEach((x, i) => (out[i] = clone(x))); return out; }
            else if (typeof v.nodeType === "number") throw domError("DataCloneError", "a DOM node could not be cloned.");
            else { out = {}; seen.set(v, out); for (const k of Object.keys(v)) out[k] = clone(v[k]); return out; }
            seen.set(v, out);
            return out;
        };
        return clone(value);
    };
})();
