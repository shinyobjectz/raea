// raea-native's web shims: the globals bundled npm packages expect from a browser or Node, for raea's bare V8. Each is
// defined only where the context lacks it. Anything with real work behind it (randomness, digests, URL parsing) is a
// native call through __raea_call; the rest is plain JS here.
(function (g) {
  "use strict";
  const call = (name, args) => {
    const out = g.__raea_call(name, JSON.stringify(args));
    return out === undefined || out === "" ? undefined : JSON.parse(out);
  };
  const def = (name, value) => {
    if (typeof g[name] === "undefined") Object.defineProperty(g, name, { value, writable: true, configurable: true, enumerable: false });
  };

  // ---- base64 -----------------------------------------------------------------------------------------------------
  const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  const B64I = new Int16Array(128).fill(-1);
  for (let i = 0; i < 64; i++) B64I[B64.charCodeAt(i)] = i;
  function bytesTo64(b) {
    let s = "";
    let i = 0;
    for (; i + 2 < b.length; i += 3) {
      const n = (b[i] << 16) | (b[i + 1] << 8) | b[i + 2];
      s += B64[n >> 18] + B64[(n >> 12) & 63] + B64[(n >> 6) & 63] + B64[n & 63];
    }
    if (i < b.length) {
      const n = (b[i] << 16) | ((i + 1 < b.length ? b[i + 1] : 0) << 8);
      s += B64[n >> 18] + B64[(n >> 12) & 63] + (i + 1 < b.length ? B64[(n >> 6) & 63] : "=") + "=";
    }
    return s;
  }
  function bytesFrom64(s) {
    const clean = String(s).replace(/[\t\n\f\r ]/g, "").replace(/=+$/, "");
    if (clean.length % 4 === 1 || /[^A-Za-z0-9+/]/.test(clean)) throw new DOMExceptionShim("The string to be decoded is not correctly encoded.", "InvalidCharacterError");
    const out = new Uint8Array(Math.floor((clean.length * 3) / 4));
    let o = 0;
    for (let i = 0; i < clean.length; i += 4) {
      const a = B64I[clean.charCodeAt(i)], b = B64I[clean.charCodeAt(i + 1)];
      const c = i + 2 < clean.length ? B64I[clean.charCodeAt(i + 2)] : 0;
      const d = i + 3 < clean.length ? B64I[clean.charCodeAt(i + 3)] : 0;
      const n = (a << 18) | (b << 12) | (c << 6) | d;
      out[o++] = n >> 16;
      if (i + 2 < clean.length) out[o++] = (n >> 8) & 255;
      if (i + 3 < clean.length) out[o++] = n & 255;
    }
    return out.subarray(0, o);
  }
  const bytesOf = (src) => {
    if (src instanceof Uint8Array) return src;
    if (ArrayBuffer.isView(src)) return new Uint8Array(src.buffer, src.byteOffset, src.byteLength);
    if (src instanceof ArrayBuffer || (typeof SharedArrayBuffer !== "undefined" && src instanceof SharedArrayBuffer)) return new Uint8Array(src);
    throw new TypeError("expected an ArrayBuffer or a view of one");
  };

  // ---- DOMException -----------------------------------------------------------------------------------------------
  const CODES = { IndexSizeError: 1, NotFoundError: 8, NotSupportedError: 9, InvalidStateError: 11, SyntaxError: 12, InvalidCharacterError: 5, NetworkError: 19, AbortError: 20, TimeoutError: 23, DataCloneError: 25 };
  class DOMExceptionShim extends Error {
    constructor(message = "", name = "Error") {
      super(message);
      Object.defineProperty(this, "name", { value: name, configurable: true, writable: true });
    }
    get code() {
      return CODES[this.name] || 0;
    }
  }
  def("DOMException", DOMExceptionShim);
  const DOMEx = g.DOMException;

  // ---- atob / btoa ------------------------------------------------------------------------------------------------
  def("btoa", (s) => {
    s = String(s);
    const b = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) {
      const c = s.charCodeAt(i);
      if (c > 255) throw new DOMEx("The string to be encoded contains characters outside of the Latin1 range.", "InvalidCharacterError");
      b[i] = c;
    }
    return bytesTo64(b);
  });
  def("atob", (s) => {
    const b = bytesFrom64(s);
    let out = "";
    for (let i = 0; i < b.length; i += 8192) out += String.fromCharCode.apply(null, b.subarray(i, i + 8192));
    return out;
  });

  // ---- TextEncoder / TextDecoder (UTF-8; TextDecoder also takes latin1/ascii and utf-16le) ---------------------------
  class TextEncoderShim {
    get encoding() {
      return "utf-8";
    }
    encode(input = "") {
      const s = String(input);
      const out = new Uint8Array(s.length * 3);
      return out.slice(0, this._into(s, out).written);
    }
    encodeInto(input, dest) {
      return this._into(String(input), dest);
    }
    _into(s, out) {
      let o = 0, i = 0;
      for (; i < s.length; i++) {
        let c = s.charCodeAt(i);
        if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length) {
          const d = s.charCodeAt(i + 1);
          if (d >= 0xdc00 && d <= 0xdfff) {
            c = 0x10000 + ((c - 0xd800) << 10) + (d - 0xdc00);
            if (o + 4 > out.length) break;
            out[o++] = 0xf0 | (c >> 18);
            out[o++] = 0x80 | ((c >> 12) & 63);
            out[o++] = 0x80 | ((c >> 6) & 63);
            out[o++] = 0x80 | (c & 63);
            i++;
            continue;
          }
        }
        if (c >= 0xd800 && c <= 0xdfff) c = 0xfffd; // a lone surrogate
        if (c < 0x80) {
          if (o + 1 > out.length) break;
          out[o++] = c;
        } else if (c < 0x800) {
          if (o + 2 > out.length) break;
          out[o++] = 0xc0 | (c >> 6);
          out[o++] = 0x80 | (c & 63);
        } else {
          if (o + 3 > out.length) break;
          out[o++] = 0xe0 | (c >> 12);
          out[o++] = 0x80 | ((c >> 6) & 63);
          out[o++] = 0x80 | (c & 63);
        }
      }
      return { read: i, written: o };
    }
  }
  def("TextEncoder", TextEncoderShim);

  const LABELS = { "utf-8": "utf-8", utf8: "utf-8", "unicode-1-1-utf-8": "utf-8", "utf-16le": "utf-16le", "utf-16": "utf-16le", latin1: "windows-1252", "iso-8859-1": "windows-1252", ascii: "windows-1252", "us-ascii": "windows-1252", "windows-1252": "windows-1252" };
  class TextDecoderShim {
    constructor(label = "utf-8", options = {}) {
      const enc = LABELS[String(label).trim().toLowerCase()];
      if (!enc) throw new RangeError(`The encoding label provided ('${label}') is invalid.`);
      this._enc = enc;
      this._fatal = !!options.fatal;
      this._ignoreBOM = !!options.ignoreBOM;
      this._pending = new Uint8Array(0);
      this._bomSeen = false;
    }
    get encoding() {
      return this._enc;
    }
    get fatal() {
      return this._fatal;
    }
    get ignoreBOM() {
      return this._ignoreBOM;
    }
    decode(input, options = {}) {
      let b = input === undefined ? new Uint8Array(0) : bytesOf(input);
      if (this._pending.length) {
        const j = new Uint8Array(this._pending.length + b.length);
        j.set(this._pending);
        j.set(b, this._pending.length);
        b = j;
        this._pending = new Uint8Array(0);
      }
      const stream = !!options.stream;
      let out;
      if (this._enc === "utf-8") out = this._utf8(b, stream);
      else if (this._enc === "utf-16le") out = this._utf16(b, stream);
      else {
        out = "";
        for (let i = 0; i < b.length; i += 8192) out += String.fromCharCode.apply(null, b.subarray(i, i + 8192));
      }
      if (!this._ignoreBOM && !this._bomSeen && out.length) {
        if (out.charCodeAt(0) === 0xfeff && this._enc !== "windows-1252") out = out.slice(1);
        this._bomSeen = true;
      }
      if (!stream) this._bomSeen = false;
      return out;
    }
    _bad() {
      if (this._fatal) throw new TypeError("The encoded data was not valid for encoding utf-8");
      return 0xfffd;
    }
    _utf8(b, stream) {
      const parts = [];
      let codes = [];
      let i = 0;
      const n = b.length;
      while (i < n) {
        const c = b[i];
        let cp, len;
        if (c < 0x80) { cp = c; len = 1; }
        else if (c >= 0xc2 && c < 0xe0) { cp = c & 31; len = 2; }
        else if (c >= 0xe0 && c < 0xf0) { cp = c & 15; len = 3; }
        else if (c >= 0xf0 && c < 0xf5) { cp = c & 7; len = 4; }
        else { codes.push(this._bad()); i++; continue; }
        if (i + len > n) {
          // an incomplete sequence at the end: keep it for the next chunk when streaming
          let ok = true;
          for (let k = 1; k < n - i; k++) if ((b[i + k] & 0xc0) !== 0x80) ok = false;
          if (stream && ok) { this._pending = b.slice(i); break; }
          codes.push(this._bad());
          i++;
          while (i < n && (b[i] & 0xc0) === 0x80) i++;
          continue;
        }
        let bad = false;
        for (let k = 1; k < len; k++) {
          const d = b[i + k];
          if ((d & 0xc0) !== 0x80) { bad = true; len = k; break; }
          cp = (cp << 6) | (d & 63);
        }
        if (!bad && ((len === 3 && (cp < 0x800 || (cp >= 0xd800 && cp <= 0xdfff))) || (len === 4 && (cp < 0x10000 || cp > 0x10ffff)))) bad = true;
        if (bad) { codes.push(this._bad()); i += Math.max(1, len === 1 ? 1 : len); continue; }
        if (cp > 0xffff) { cp -= 0x10000; codes.push(0xd800 + (cp >> 10), 0xdc00 + (cp & 1023)); }
        else codes.push(cp);
        i += len;
        if (codes.length > 8192) { parts.push(String.fromCharCode.apply(null, codes)); codes = []; }
      }
      parts.push(String.fromCharCode.apply(null, codes));
      return parts.join("");
    }
    _utf16(b, stream) {
      const even = b.length - (b.length % 2);
      if (b.length % 2) {
        if (stream) this._pending = b.slice(even);
      }
      let out = "";
      const codes = [];
      for (let i = 0; i < even; i += 2) {
        codes.push(b[i] | (b[i + 1] << 8));
        if (codes.length > 8192) { out += String.fromCharCode.apply(null, codes); codes.length = 0; }
      }
      out += String.fromCharCode.apply(null, codes);
      if (b.length % 2 && !stream) out += String.fromCharCode(this._bad());
      return out;
    }
  }
  def("TextDecoder", TextDecoderShim);

  // ---- Event / EventTarget / CustomEvent ----------------------------------------------------------------------------
  class EventShim {
    constructor(type, init = {}) {
      if (arguments.length === 0) throw new TypeError("Event needs a type");
      this.type = String(type);
      this.bubbles = !!init.bubbles;
      this.cancelable = !!init.cancelable;
      this.composed = !!init.composed;
      this.defaultPrevented = false;
      this.timeStamp = typeof g.performance !== "undefined" ? g.performance.now() : Date.now();
      this.target = null;
      this.currentTarget = null;
      this.isTrusted = false;
      this._stop = false;
    }
    preventDefault() {
      if (this.cancelable) this.defaultPrevented = true;
    }
    stopPropagation() {}
    stopImmediatePropagation() {
      this._stop = true;
    }
  }
  def("Event", EventShim);
  class CustomEventShim extends g.Event {
    constructor(type, init = {}) {
      super(type, init);
      this.detail = init.detail === undefined ? null : init.detail;
    }
  }
  def("CustomEvent", CustomEventShim);
  const LISTENERS = Symbol("listeners");
  class EventTargetShim {
    constructor() {
      Object.defineProperty(this, LISTENERS, { value: new Map() });
    }
    addEventListener(type, fn, opts) {
      if (!fn) return;
      const once = typeof opts === "object" && opts !== null && !!opts.once;
      const signal = typeof opts === "object" && opts !== null ? opts.signal : undefined;
      if (signal && signal.aborted) return;
      let list = this[LISTENERS].get(type);
      if (!list) this[LISTENERS].set(type, (list = []));
      if (list.some((l) => l.fn === fn)) return;
      const entry = { fn, once };
      list.push(entry);
      if (signal) signal.addEventListener("abort", () => this.removeEventListener(type, fn));
    }
    removeEventListener(type, fn) {
      const list = this[LISTENERS].get(type);
      if (!list) return;
      const i = list.findIndex((l) => l.fn === fn);
      if (i >= 0) list.splice(i, 1);
    }
    dispatchEvent(ev) {
      if (!(ev instanceof g.Event)) throw new TypeError("dispatchEvent takes an Event");
      try { Object.defineProperty(ev, "target", { value: this, configurable: true, writable: true }); } catch (_) {}
      try { Object.defineProperty(ev, "currentTarget", { value: this, configurable: true, writable: true }); } catch (_) {}
      const handler = this["on" + ev.type];
      const list = (this[LISTENERS].get(ev.type) || []).slice();
      if (typeof handler === "function") list.unshift({ fn: handler, once: false, prop: true });
      for (const l of list) {
        if (l.once) this.removeEventListener(ev.type, l.fn);
        try {
          if (typeof l.fn === "function") l.fn.call(this, ev);
          else if (l.fn && typeof l.fn.handleEvent === "function") l.fn.handleEvent(ev);
        } catch (e) {
          if (typeof g.reportError === "function") g.reportError(e);
          else Promise.reject(e);
        }
        if (ev._stop) break;
      }
      return !ev.defaultPrevented;
    }
  }
  def("EventTarget", EventTargetShim);

  // ---- AbortController / AbortSignal --------------------------------------------------------------------------------
  class AbortSignalShim extends g.EventTarget {
    constructor() {
      super();
      this.aborted = false;
      this.reason = undefined;
      this.onabort = null;
    }
    throwIfAborted() {
      if (this.aborted) throw this.reason;
    }
    _abort(reason) {
      if (this.aborted) return;
      this.aborted = true;
      this.reason = reason === undefined ? new DOMEx("This operation was aborted", "AbortError") : reason;
      this.dispatchEvent(new g.Event("abort"));
    }
    static abort(reason) {
      const s = new AbortSignalShim();
      s._abort(reason);
      return s;
    }
    static timeout(ms) {
      const s = new AbortSignalShim();
      g.setTimeout(() => s._abort(new DOMEx("The operation timed out.", "TimeoutError")), ms);
      return s;
    }
    static any(signals) {
      const s = new AbortSignalShim();
      for (const x of signals) {
        if (x.aborted) { s._abort(x.reason); return s; }
      }
      for (const x of signals) x.addEventListener("abort", () => s._abort(x.reason), { once: true });
      return s;
    }
  }
  def("AbortSignal", AbortSignalShim);
  class AbortControllerShim {
    constructor() {
      this.signal = new g.AbortSignal();
    }
    abort(reason) {
      this.signal._abort(reason);
    }
  }
  def("AbortController", AbortControllerShim);

  // ---- URLSearchParams / URL ----------------------------------------------------------------------------------------
  const encodeForm = (s) =>
    encodeURIComponent(s).replace(/%20/g, "+").replace(/[!'()~]/g, (c) => "%" + c.charCodeAt(0).toString(16).toUpperCase());
  const decodeForm = (s) => {
    try {
      return decodeURIComponent(s.replace(/\+/g, " "));
    } catch (_) {
      return s.replace(/\+/g, " ");
    }
  };
  class URLSearchParamsShim {
    constructor(init = "") {
      this._list = [];
      this._url = null;
      if (typeof init === "string") this._parse(init);
      else if (init && typeof init[Symbol.iterator] === "function") {
        for (const pair of init) {
          const p = Array.from(pair);
          if (p.length !== 2) throw new TypeError("each pair needs a name and a value");
          this._list.push([String(p[0]), String(p[1])]);
        }
      } else if (init && typeof init === "object") {
        for (const k of Object.keys(init)) this._list.push([k, String(init[k])]);
      }
    }
    _parse(s) {
      this._list = [];
      s = String(s);
      if (s.startsWith("?")) s = s.slice(1);
      for (const part of s.split("&")) {
        if (!part) continue;
        const i = part.indexOf("=");
        this._list.push(i < 0 ? [decodeForm(part), ""] : [decodeForm(part.slice(0, i)), decodeForm(part.slice(i + 1))]);
      }
    }
    _update() {
      if (this._url) this._url._setSearch(this.toString());
    }
    get size() {
      return this._list.length;
    }
    append(k, v) {
      this._list.push([String(k), String(v)]);
      this._update();
    }
    delete(k, v) {
      this._list = this._list.filter(([a, b]) => !(a === String(k) && (v === undefined || b === String(v))));
      this._update();
    }
    get(k) {
      const e = this._list.find(([a]) => a === String(k));
      return e ? e[1] : null;
    }
    getAll(k) {
      return this._list.filter(([a]) => a === String(k)).map(([, b]) => b);
    }
    has(k, v) {
      return this._list.some(([a, b]) => a === String(k) && (v === undefined || b === String(v)));
    }
    set(k, v) {
      k = String(k);
      const i = this._list.findIndex(([a]) => a === k);
      if (i < 0) this._list.push([k, String(v)]);
      else {
        this._list[i][1] = String(v);
        this._list = this._list.filter(([a], j) => a !== k || j === i);
      }
      this._update();
    }
    sort() {
      this._list = this._list.map((e, i) => [e, i]).sort((x, y) => (x[0][0] < y[0][0] ? -1 : x[0][0] > y[0][0] ? 1 : x[1] - y[1])).map((e) => e[0]);
      this._update();
    }
    forEach(fn, self) {
      for (const [k, v] of this._list) fn.call(self, v, k, this);
    }
    keys() {
      return this._list.map(([k]) => k)[Symbol.iterator]();
    }
    values() {
      return this._list.map(([, v]) => v)[Symbol.iterator]();
    }
    entries() {
      return this._list.map(([k, v]) => [k, v])[Symbol.iterator]();
    }
    [Symbol.iterator]() {
      return this.entries();
    }
    toString() {
      return this._list.map(([k, v]) => encodeForm(k) + "=" + encodeForm(v)).join("&");
    }
  }
  def("URLSearchParams", URLSearchParamsShim);

  const PARTS = ["href", "origin", "protocol", "username", "password", "host", "hostname", "port", "pathname", "search", "hash"];
  class URLShim {
    constructor(url, base) {
      const p = call("url.parse", base === undefined ? { url: String(url) } : { url: String(url), base: String(base) });
      if (!p) throw new TypeError(`Invalid URL: ${url}`);
      this._p = p;
      this._sp = new g.URLSearchParams(p.search);
      this._sp._url = this;
    }
    static canParse(url, base) {
      return !!call("url.parse", base === undefined ? { url: String(url) } : { url: String(url), base: String(base) });
    }
    static parse(url, base) {
      try {
        return new URLShim(url, base);
      } catch (_) {
        return null;
      }
    }
    _set(part, value) {
      this._p = call("url.set", { href: this._p.href, part, value: String(value) });
      if (part === "search" || part === "href") this._sp._parse(this._p.search);
    }
    _setSearch(q) {
      this._p = call("url.set", { href: this._p.href, part: "search", value: q });
    }
    get searchParams() {
      return this._sp;
    }
    toString() {
      return this._p.href;
    }
    toJSON() {
      return this._p.href;
    }
  }
  for (const k of PARTS) {
    Object.defineProperty(URLShim.prototype, k, {
      get() {
        return this._p[k];
      },
      set(v) {
        if (k !== "origin") this._set(k, v);
      },
      configurable: true,
    });
  }
  def("URL", URLShim);

  // ---- structuredClone ----------------------------------------------------------------------------------------------
  def("structuredClone", (value) => {
    const seen = new Map();
    const clone = (v) => {
      if (v === null || typeof v !== "object") {
        if (typeof v === "function" || typeof v === "symbol") throw new DOMEx(`${String(v)} could not be cloned.`, "DataCloneError");
        return v;
      }
      if (seen.has(v)) return seen.get(v);
      let out;
      if (v instanceof Date) out = new Date(v.getTime());
      else if (v instanceof RegExp) out = new RegExp(v.source, v.flags);
      else if (v instanceof ArrayBuffer) out = v.slice(0);
      else if (ArrayBuffer.isView(v)) {
        const buf = clone(v.buffer);
        out = v instanceof DataView ? new DataView(buf, v.byteOffset, v.byteLength) : new v.constructor(buf, v.byteOffset, v.length);
      } else if (v instanceof Map) {
        out = new Map();
        seen.set(v, out);
        for (const [k, x] of v) out.set(clone(k), clone(x));
        return out;
      } else if (v instanceof Set) {
        out = new Set();
        seen.set(v, out);
        for (const x of v) out.add(clone(x));
        return out;
      } else if (v instanceof Error) {
        out = new v.constructor(v.message);
        seen.set(v, out);
        if (v.stack) out.stack = v.stack;
        return out;
      } else if (Array.isArray(v)) {
        out = new Array(v.length);
        seen.set(v, out);
        for (let i = 0; i < v.length; i++) out[i] = clone(v[i]);
        return out;
      } else {
        out = {};
        seen.set(v, out);
        for (const k of Object.keys(v)) out[k] = clone(v[k]);
        return out;
      }
      seen.set(v, out);
      return out;
    };
    return clone(value);
  });

  // ---- crypto -------------------------------------------------------------------------------------------------------
  if (typeof g.crypto === "undefined") {
    const getRandomValues = (arr) => {
      if (!ArrayBuffer.isView(arr) || arr instanceof Float32Array || arr instanceof Float64Array) throw new TypeError("getRandomValues takes an integer typed array");
      const b = bytesFrom64(call("web.random", { n: arr.byteLength }));
      new Uint8Array(arr.buffer, arr.byteOffset, arr.byteLength).set(b);
      return arr;
    };
    const randomUUID = () => {
      const b = getRandomValues(new Uint8Array(16));
      b[6] = (b[6] & 0x0f) | 0x40;
      b[8] = (b[8] & 0x3f) | 0x80;
      const h = Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
      return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
    };
    const subtle = {
      digest(algo, data) {
        try {
          const name = typeof algo === "string" ? algo : algo.name;
          const out = bytesFrom64(call("web.digest", { algo: name, data: bytesTo64(bytesOf(data)) }));
          return Promise.resolve(out.buffer.slice(out.byteOffset, out.byteOffset + out.byteLength));
        } catch (e) {
          return Promise.reject(e);
        }
      },
    };
    def("crypto", { getRandomValues, randomUUID, subtle });
  }

  // ---- Blob ---------------------------------------------------------------------------------------------------------
  class BlobShim {
    constructor(parts = [], options = {}) {
      const chunks = [];
      for (const p of parts) {
        if (p instanceof BlobShim) chunks.push(p._bytes);
        else if (typeof p === "string") chunks.push(new g.TextEncoder().encode(p));
        else chunks.push(new Uint8Array(bytesOf(p)));
      }
      const n = chunks.reduce((a, c) => a + c.length, 0);
      this._bytes = new Uint8Array(n);
      let o = 0;
      for (const c of chunks) {
        this._bytes.set(c, o);
        o += c.length;
      }
      this.type = options.type ? String(options.type).toLowerCase() : "";
    }
    get size() {
      return this._bytes.length;
    }
    slice(start = 0, end = this.size, type = "") {
      const b = new BlobShim([], { type });
      b._bytes = this._bytes.slice(start, end);
      return b;
    }
    arrayBuffer() {
      return Promise.resolve(this._bytes.slice().buffer);
    }
    bytes() {
      return Promise.resolve(this._bytes.slice());
    }
    text() {
      return Promise.resolve(new g.TextDecoder().decode(this._bytes));
    }
  }
  def("Blob", BlobShim);

  // ---- Headers --------------------------------------------------------------------------------------------------------
  const normName = (n) => {
    n = String(n);
    if (!/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(n)) throw new TypeError(`Invalid header name: ${n}`);
    return n.toLowerCase();
  };
  class HeadersShim {
    constructor(init) {
      this._m = new Map();
      if (init instanceof HeadersShim) init.forEach((v, k) => this.append(k, v));
      else if (Array.isArray(init) || (init && typeof init[Symbol.iterator] === "function")) for (const [k, v] of init) this.append(k, v);
      else if (init && typeof init === "object") for (const k of Object.keys(init)) this.append(k, init[k]);
    }
    append(k, v) {
      k = normName(k);
      v = String(v).trim();
      this._m.set(k, this._m.has(k) ? (k === "set-cookie" ? this._m.get(k) + "\n" + v : this._m.get(k) + ", " + v) : v);
    }
    set(k, v) {
      this._m.set(normName(k), String(v).trim());
    }
    get(k) {
      const v = this._m.get(normName(k));
      return v === undefined ? null : v;
    }
    has(k) {
      return this._m.has(normName(k));
    }
    delete(k) {
      this._m.delete(normName(k));
    }
    getSetCookie() {
      const v = this._m.get("set-cookie");
      return v === undefined ? [] : v.split("\n");
    }
    forEach(fn, self) {
      for (const [k, v] of this.entries()) fn.call(self, v, k, this);
    }
    keys() {
      return [...this._m.keys()].sort()[Symbol.iterator]();
    }
    values() {
      return [...this.entries()].map((e) => e[1])[Symbol.iterator]();
    }
    entries() {
      return [...this._m.keys()].sort().map((k) => [k, this._m.get(k)])[Symbol.iterator]();
    }
    [Symbol.iterator]() {
      return this.entries();
    }
  }
  def("Headers", HeadersShim);

  // ---- Buffer (the part packages use: bytes from and to strings) ------------------------------------------------------
  if (typeof g.Buffer === "undefined") {
    class BufferShim extends Uint8Array {
      static from(v, enc) {
        if (typeof v === "string") {
          const e = (enc || "utf8").toLowerCase();
          let b;
          if (e === "base64" || e === "base64url") b = bytesFrom64(e === "base64url" ? v.replace(/-/g, "+").replace(/_/g, "/") : v);
          else if (e === "hex") {
            b = new Uint8Array(v.length >> 1);
            for (let i = 0; i < b.length; i++) b[i] = parseInt(v.substr(i * 2, 2), 16);
          } else if (e === "latin1" || e === "binary" || e === "ascii") {
            b = new Uint8Array(v.length);
            for (let i = 0; i < v.length; i++) b[i] = v.charCodeAt(i) & 255;
          } else b = new g.TextEncoder().encode(v);
          const out = new BufferShim(b.length);
          out.set(b);
          return out;
        }
        if (v instanceof ArrayBuffer) return new BufferShim(v);
        const src = ArrayBuffer.isView(v) ? new Uint8Array(v.buffer, v.byteOffset, v.byteLength) : Uint8Array.from(v);
        const out = new BufferShim(src.length);
        out.set(src);
        return out;
      }
      static alloc(n, fill = 0) {
        return new BufferShim(n).fill(typeof fill === "number" ? fill : 0);
      }
      static isBuffer(b) {
        return b instanceof BufferShim;
      }
      static byteLength(s, enc) {
        return typeof s === "string" ? BufferShim.from(s, enc).length : s.byteLength;
      }
      static concat(list, total) {
        const n = total === undefined ? list.reduce((a, b) => a + b.length, 0) : total;
        const out = new BufferShim(n);
        let o = 0;
        for (const b of list) {
          out.set(b.subarray(0, Math.max(0, Math.min(b.length, n - o))), o);
          o += b.length;
          if (o >= n) break;
        }
        return out;
      }
      toString(enc, start = 0, end = this.length) {
        const b = this.subarray(start, end);
        const e = (enc || "utf8").toLowerCase();
        if (e === "base64") return bytesTo64(b);
        if (e === "base64url") return bytesTo64(b).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
        if (e === "hex") return Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
        if (e === "latin1" || e === "binary" || e === "ascii") {
          let s = "";
          for (let i = 0; i < b.length; i += 8192) s += String.fromCharCode.apply(null, b.subarray(i, i + 8192));
          return s;
        }
        return new g.TextDecoder().decode(b);
      }
      equals(o) {
        return this.length === o.length && this.every((x, i) => x === o[i]);
      }
    }
    def("Buffer", BufferShim);
  }

  // ---- WritableStream (the part a writer to a sink needs: write, close, abort, ready; writes go in order) ------------
  class WritableStreamShim {
    constructor(sink = {}) {
      this._sink = sink;
      this._locked = false;
      this._chain = Promise.resolve();
      this._closed = false;
      if (typeof sink.start === "function") this._chain = Promise.resolve(sink.start(this));
    }
    get locked() {
      return this._locked;
    }
    _run(fn) {
      const p = this._chain.then(fn);
      this._chain = p.catch(() => {});
      return p;
    }
    getWriter() {
      if (this._locked) throw new TypeError("WritableStream is locked");
      this._locked = true;
      const st = this;
      return {
        get desiredSize() {
          return st._closed ? 0 : 1;
        },
        get ready() {
          return st._chain;
        },
        get closed() {
          return st._chain;
        },
        write(chunk) {
          if (st._closed) return Promise.reject(new TypeError("the stream is closed"));
          return st._run(() => (typeof st._sink.write === "function" ? st._sink.write(chunk, st) : undefined));
        },
        close() {
          st._closed = true;
          return st._run(() => (typeof st._sink.close === "function" ? st._sink.close() : undefined));
        },
        abort(reason) {
          st._closed = true;
          return st._run(() => (typeof st._sink.abort === "function" ? st._sink.abort(reason) : undefined));
        },
        releaseLock() {
          st._locked = false;
        },
      };
    }
    close() {
      return this.getWriter().close();
    }
    abort(reason) {
      return this.getWriter().abort(reason);
    }
  }
  def("WritableStream", WritableStreamShim);

  // ---- small globals packages probe for -------------------------------------------------------------------------------
  def("globalThis", g);
  def("self", g);
  if (typeof g.reportError === "undefined") def("reportError", (e) => { if (g.console && g.console.error) g.console.error(e && e.stack ? e.stack : String(e)); });
})(globalThis);
