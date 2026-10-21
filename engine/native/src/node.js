// Node's core for raea's own tools on JavaScriptCore (node.rs): fs, path, os, crypto, child_process, process, Buffer
// and the modules libraries probe for, over one native (__raea_node), plus a synchronous require for CommonJS. Only
// what raea's tools and the packages they load use; anything else throws, naming itself.
(function (g) {
  "use strict";
  const native = g.__raea_node;
  delete g.__raea_node;
  const call = (op, ...a) => {
    const r = JSON.parse(native(op, JSON.stringify(a)));
    if (r && typeof r === "object" && r.error) {
      const e = new Error(r.error.message);
      e.code = r.error.code;
      throw e;
    }
    return r;
  };

  // ---- base64 and Buffer ----------------------------------------------------------------------------------------------
  const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  const B64I = new Int16Array(256).fill(-1);
  for (let i = 0; i < 64; i++) B64I[B64.charCodeAt(i)] = i;
  B64I["-".charCodeAt(0)] = 62;
  B64I["_".charCodeAt(0)] = 63;
  const toB64 = (u) => {
    let s = "";
    for (let i = 0; i < u.length; i += 3) {
      const n = (u[i] << 16) | ((u[i + 1] || 0) << 8) | (u[i + 2] || 0);
      s += B64[(n >> 18) & 63] + B64[(n >> 12) & 63] + (i + 1 < u.length ? B64[(n >> 6) & 63] : "=") + (i + 2 < u.length ? B64[n & 63] : "=");
    }
    return s;
  };
  const fromB64 = (s) => {
    s = String(s).replace(/[^A-Za-z0-9+/\-_]/g, "");
    const out = new Uint8Array(Math.floor((s.length * 3) / 4));
    let o = 0;
    for (let i = 0; i < s.length; i += 4) {
      const a = B64I[s.charCodeAt(i)], b = B64I[s.charCodeAt(i + 1)], c = B64I[s.charCodeAt(i + 2)], d = B64I[s.charCodeAt(i + 3)];
      out[o++] = (a << 2) | (b >> 4);
      if (c >= 0 && i + 2 < s.length) out[o++] = ((b & 15) << 4) | (c >> 2);
      if (d >= 0 && i + 3 < s.length) out[o++] = ((c & 3) << 6) | d;
    }
    return out.subarray(0, o);
  };
  const enc = new TextEncoder();
  const dec = new TextDecoder();
  class Buffer extends Uint8Array {
    static from(x, encoding) {
      if (typeof x === "string") {
        if (encoding === "base64" || encoding === "base64url") return Buffer.of2(fromB64(x));
        if (encoding === "hex") {
          const b = new Buffer(x.length >> 1);
          for (let i = 0; i < b.length; i++) b[i] = parseInt(x.substr(i * 2, 2), 16);
          return b;
        }
        if (encoding === "latin1" || encoding === "binary") return Buffer.of2(Uint8Array.from(x, (c) => c.charCodeAt(0) & 255));
        return Buffer.of2(enc.encode(x));
      }
      if (x instanceof ArrayBuffer) return new Buffer(x, encoding || 0);
      if (ArrayBuffer.isView(x)) return Buffer.of2(new Uint8Array(x.buffer, x.byteOffset, x.byteLength).slice());
      return Buffer.of2(Uint8Array.from(x));
    }
    static of2(u) { return new Buffer(u.buffer, u.byteOffset, u.byteLength); }
    static alloc(n, fill) { const b = new Buffer(n); if (fill) b.fill(typeof fill === "string" ? fill.charCodeAt(0) : fill); return b; }
    static allocUnsafe(n) { return new Buffer(n); }
    static isBuffer(x) { return x instanceof Buffer; }
    static byteLength(s, encoding) { return typeof s === "string" ? Buffer.from(s, encoding).length : s.byteLength; }
    static concat(list, total) {
      const n = total === undefined ? list.reduce((a, b) => a + b.length, 0) : total;
      const out = new Buffer(n);
      let at = 0;
      for (const b of list) { out.set(b.subarray(0, Math.min(b.length, n - at)), at); at += b.length; if (at >= n) break; }
      return out;
    }
    toString(encoding, start = 0, end = this.length) {
      const u = this.subarray(start, end);
      if (encoding === "base64") return toB64(u);
      if (encoding === "base64url") return toB64(u).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
      if (encoding === "hex") return Array.from(u, (b) => b.toString(16).padStart(2, "0")).join("");
      if (encoding === "latin1" || encoding === "binary") return String.fromCharCode(...u);
      return dec.decode(u);
    }
    toJSON() { return { type: "Buffer", data: Array.from(this) }; }
    equals(o) { return this.length === o.length && this.every((v, i) => v === o[i]); }
    slice(a, b) { return Buffer.of2(this.subarray(a, b)); }
    write(s, offset = 0) { const b = enc.encode(s); this.set(b.subarray(0, this.length - offset), offset); return Math.min(b.length, this.length - offset); }
  }
  const data = (x) => (typeof x === "string" ? { s: x } : { b: toB64(x instanceof ArrayBuffer ? new Uint8Array(x) : new Uint8Array(x.buffer, x.byteOffset, x.byteLength)) });
  const encodingOf = (o) => (typeof o === "string" ? o : o && o.encoding);
  const asText = (r, encoding) => {
    if (r.s !== undefined) return r.s;
    const b = Buffer.of2(fromB64(r.b));
    return encoding ? b.toString(encoding) : b;
  };

  // ---- path (POSIX) -------------------------------------------------------------------------------------------------
  const normalizeParts = (parts, abs) => {
    const out = [];
    for (const p of parts) {
      if (!p || p === ".") continue;
      if (p === "..") { if (out.length && out[out.length - 1] !== "..") out.pop(); else if (!abs) out.push(".."); }
      else out.push(p);
    }
    return out;
  };
  const path = {
    sep: "/", delimiter: ":",
    normalize(p) {
      if (p === "") return ".";
      const abs = p.startsWith("/"), trail = p.endsWith("/");
      let s = normalizeParts(p.split("/"), abs).join("/");
      if (!s && !abs) s = ".";
      if (s && trail) s += "/";
      return (abs ? "/" : "") + s;
    },
    join(...a) { const s = a.filter((x) => x !== "").join("/"); return s ? path.normalize(s) : "."; },
    resolve(...a) {
      let r = "";
      for (let i = a.length - 1; i >= 0 && !r.startsWith("/"); i--) if (a[i]) r = a[i] + (r ? "/" + r : "");
      if (!r.startsWith("/")) r = proc.cwd() + (r ? "/" + r : "");
      const s = "/" + normalizeParts(r.split("/"), true).join("/");
      return s;
    },
    isAbsolute: (p) => p.startsWith("/"),
    dirname(p) {
      if (!p) return ".";
      const s = p.replace(/\/+$/, "");
      if (!s) return "/";
      const i = s.lastIndexOf("/");
      return i < 0 ? "." : i === 0 ? "/" : s.slice(0, i);
    },
    basename(p, ext) {
      let b = p.replace(/\/+$/, "");
      b = b.slice(b.lastIndexOf("/") + 1);
      return ext && b.endsWith(ext) && b !== ext ? b.slice(0, -ext.length) : b;
    },
    extname(p) { const b = path.basename(p); const i = b.lastIndexOf("."); return i <= 0 ? "" : b.slice(i); },
    relative(from, to) {
      const f = path.resolve(from).split("/").filter(Boolean), t = path.resolve(to).split("/").filter(Boolean);
      let i = 0;
      while (i < f.length && i < t.length && f[i] === t[i]) i++;
      return [...Array(f.length - i).fill(".."), ...t.slice(i)].join("/");
    },
    parse(p) { const base = path.basename(p), ext = path.extname(p); return { root: p.startsWith("/") ? "/" : "", dir: path.dirname(p), base, ext, name: ext ? base.slice(0, -ext.length) : base }; },
    format(o) { return (o.dir ? o.dir + "/" : o.root || "") + (o.base || (o.name || "") + (o.ext || "")); },
  };
  path.posix = path;

  // ---- fs ---------------------------------------------------------------------------------------------------------------
  const stats = (s) => ({
    ...s, isDirectory: () => s.dir, isFile: () => s.file, isSymbolicLink: () => s.link,
    mtime: new Date(s.mtimeMs), atime: new Date(s.atimeMs), ctime: new Date(s.ctimeMs), birthtime: new Date(s.birthtimeMs),
  });
  const dirent = (e, dir) => ({ name: e.name, parentPath: dir, path: dir, isDirectory: () => e.dir, isFile: () => e.file, isSymbolicLink: () => e.link });
  const pathOf = (p) => (p instanceof URL ? url.fileURLToPath(p) : String(p));
  const fs = {
    existsSync: (p) => call("exists", pathOf(p)),
    readFileSync(p, o) { const e = encodingOf(o); return asText(call("readFile", pathOf(p), e === "utf8" || e === "utf-8"), e); },
    writeFileSync(p, d) { call("writeFile", pathOf(p), data(d), false); },
    appendFileSync(p, d) { call("writeFile", pathOf(p), data(d), true); },
    mkdirSync(p, o) { const r = !!(o && o.recursive); try { call("mkdir", pathOf(p), r); } catch (e) { if (!(r && e.code === "EEXIST")) throw e; } },
    readdirSync(p, o) { const d = pathOf(p); const l = call("readdir", d); return o && o.withFileTypes ? l.map((e) => dirent(e, d)) : l.map((e) => e.name); },
    statSync(p, o) { try { return stats(call("stat", pathOf(p), false)); } catch (e) { if (o && o.throwIfNoEntry === false && e.code === "ENOENT") return undefined; throw e; } },
    lstatSync: (p) => stats(call("stat", pathOf(p), true)),
    rmSync(p, o) { call("rm", pathOf(p), !!(o && o.recursive), !!(o && o.force)); },
    unlinkSync(p) { call("rm", pathOf(p), false, false); },
    rmdirSync(p, o) { call("rm", pathOf(p), !!(o && o.recursive), false); },
    renameSync: (a, b) => call("rename", pathOf(a), pathOf(b)),
    copyFileSync: (a, b) => call("copyFile", pathOf(a), pathOf(b)),
    realpathSync: (p) => call("realpath", pathOf(p)),
    mkdtempSync: (prefix) => call("mkdtemp", String(prefix)),
    constants: { F_OK: 0, R_OK: 4, W_OK: 2, X_OK: 1 },
    createWriteStream(p) { fs.writeFileSync(p, ""); return { write(d) { fs.appendFileSync(p, d); return true; }, end(d) { if (d) fs.appendFileSync(p, d); }, on() { return this; } }; },
  };
  const later = (f) => (...a) => new Promise((ok, no) => { try { ok(f(...a)); } catch (e) { no(e); } });
  const promises = {
    readFile: later(fs.readFileSync), writeFile: later(fs.writeFileSync), appendFile: later(fs.appendFileSync), mkdir: later(fs.mkdirSync),
    readdir: later(fs.readdirSync), stat: later(fs.statSync), lstat: later(fs.lstatSync), rm: later(fs.rmSync), unlink: later(fs.unlinkSync),
    rename: later(fs.renameSync), copyFile: later(fs.copyFileSync), realpath: later(fs.realpathSync), mkdtemp: later(fs.mkdtempSync),
    access: later((p) => { if (!fs.existsSync(p)) { const e = new Error("ENOENT: no such file or directory, access '" + p + "'"); e.code = "ENOENT"; throw e; } }),
  };
  fs.promises = promises;

  // ---- os, crypto, child_process --------------------------------------------------------------------------------------
  const os = {
    tmpdir: () => call("tmpdir"), homedir: () => call("homedir"), platform: () => "darwin", arch: () => "arm64", EOL: "\n",
    cpus: () => Array.from({ length: 8 }, () => ({ model: "", speed: 0 })), type: () => "Darwin", release: () => "", hostname: () => "raea", endianness: () => "LE",
  };
  const crypto = {
    createHash(alg) {
      const parts = [];
      const h = {
        update(d, encoding) { parts.push(typeof d === "string" ? Buffer.from(d, encoding) : d); return h; },
        digest(encoding) { const out = Buffer.of2(fromB64(call("hash", alg, toB64(Buffer.concat(parts.map((p) => Buffer.from(p))))))); return encoding ? out.toString(encoding) : out; },
      };
      return h;
    },
    randomBytes: (n) => Buffer.of2(fromB64(call("random", n))),
    getRandomValues(a) { const b = fromB64(call("random", a.byteLength)); new Uint8Array(a.buffer, a.byteOffset, a.byteLength).set(b); return a; },
    randomUUID() {
      const b = fromB64(call("random", 16));
      b[6] = (b[6] & 15) | 64; b[8] = (b[8] & 63) | 128;
      const h = Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
      return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
    },
  };
  crypto.webcrypto = { getRandomValues: crypto.getRandomValues, randomUUID: crypto.randomUUID };
  const childProcess = {
    spawnSync(cmd, args, o) {
      if (!Array.isArray(args)) { o = args; args = []; }
      o = o || {};
      const r = JSON.parse(native("spawn", JSON.stringify([String(cmd), args.map(String), { cwd: o.cwd, env: o.env, input: o.input === undefined ? undefined : data(o.input) }])));
      const out = (b) => { const buf = Buffer.of2(fromB64(b || "")); return o.encoding && o.encoding !== "buffer" ? buf.toString(o.encoding) : buf; };
      const res = { pid: 0, status: r.status === undefined ? null : r.status, signal: r.signal || null, stdout: out(r.stdout), stderr: out(r.stderr) };
      res.output = [null, res.stdout, res.stderr];
      if (r.error) { const e = new Error(r.error.message); e.code = r.error.code; res.error = e; }
      return res;
    },
    execFileSync(cmd, args, o) {
      const r = childProcess.spawnSync(cmd, args, o);
      if (r.error) throw r.error;
      if (r.status !== 0) { const e = new Error(`Command failed: ${cmd} ${(args || []).join(" ")}\n${r.stderr}`); e.status = r.status; e.stdout = r.stdout; e.stderr = r.stderr; throw e; }
      return r.stdout;
    },
    execSync(command, o) { return childProcess.execFileSync("/bin/sh", ["-c", String(command)], o); },
  };

  // ---- process, url, util, events, module ---------------------------------------------------------------------------
  const proc = g.process;
  let cwd = call("cwd");
  proc.cwd = () => cwd;
  proc.chdir = (d) => { cwd = path.resolve(d); };
  proc.version = "v22.0.0";
  proc.versions = { node: "22.0.0", raea: "1" };
  proc.arch = "arm64";
  proc.pid = 1;
  proc.execPath = "raea";
  proc.exitCode = undefined;
  proc.nextTick = (fn, ...a) => queueMicrotask(() => fn(...a));
  proc.hrtime = Object.assign((prev) => {
    const ns = BigInt(Math.round(performance.now() * 1e6));
    const s = [Number(ns / 1000000000n), Number(ns % 1000000000n)];
    return prev ? [s[0] - prev[0], s[1] - prev[1]] : s;
  }, { bigint: () => BigInt(Math.round(performance.now() * 1e6)) });
  proc.memoryUsage = () => ({ rss: 0, heapTotal: 0, heapUsed: 0, external: 0, arrayBuffers: 0 });
  proc.on = proc.once = proc.off = () => proc;
  proc.emitWarning = () => {};
  const url = {
    fileURLToPath: (u) => decodeURIComponent(new URL(String(u)).pathname),
    pathToFileURL: (p) => new URL("file://" + encodeURI(path.resolve(String(p)))),
    URL: g.URL, URLSearchParams: g.URLSearchParams,
  };
  const inspect = (x) => { try { return typeof x === "string" ? x : JSON.stringify(x); } catch { return String(x); } };
  const util = {
    inspect, format: (f, ...a) => String(f).replace(/%[sdifjoO]/g, () => inspect(a.shift())) + (a.length ? " " + a.map(inspect).join(" ") : ""),
    promisify: (f) => (...a) => new Promise((ok, no) => f(...a, (e, v) => (e ? no(e) : ok(v)))),
    inherits: (c, p) => { Object.setPrototypeOf(c.prototype, p.prototype); Object.setPrototypeOf(c, p); },
    types: { isUint8Array: (x) => x instanceof Uint8Array, isDate: (x) => x instanceof Date, isRegExp: (x) => x instanceof RegExp, isPromise: (x) => x instanceof Promise },
    TextEncoder: g.TextEncoder, TextDecoder: g.TextDecoder,
  };
  class EventEmitter {
    constructor() { this._ev = new Map(); }
    on(t, f) { (this._ev.get(t) || this._ev.set(t, []).get(t)).push(f); return this; }
    once(t, f) { const w = (...a) => { this.off(t, w); f(...a); }; return this.on(t, w); }
    off(t, f) { const l = this._ev.get(t); if (l) this._ev.set(t, l.filter((x) => x !== f)); return this; }
    emit(t, ...a) { const l = this._ev.get(t); if (!l || !l.length) return false; l.slice().forEach((f) => f.apply(this, a)); return true; }
    removeAllListeners(t) { t === undefined ? this._ev.clear() : this._ev.delete(t); return this; }
    listenerCount(t) { return (this._ev.get(t) || []).length; }
  }
  EventEmitter.prototype.addListener = EventEmitter.prototype.on;
  EventEmitter.prototype.removeListener = EventEmitter.prototype.off;
  EventEmitter.EventEmitter = EventEmitter;
  const missing = (name) => new Proxy({}, { get(_, k) { if (k === "__esModule" || typeof k === "symbol" || k === "then") return undefined; throw new Error(`raea's Node core has no ${name}.${String(k)}`); } });

  // ---- require ----------------------------------------------------------------------------------------------------
  const cache = new Map();
  const builtins = {
    fs, "fs/promises": promises, path, os, crypto, child_process: childProcess, process: proc, buffer: { Buffer }, url, util,
    events: EventEmitter, module: {}, perf_hooks: { performance: g.performance }, inspector: missing("inspector"), assert: Object.assign((v, m) => { if (!v) throw new Error(m || "Assertion failed"); }, { ok: (v, m) => { if (!v) throw new Error(m || "Assertion failed"); }, equal: (a, b, m) => { if (a != b) throw new Error(m || `${a} != ${b}`); } }),
    stream: missing("stream"), tty: { isatty: () => false }, worker_threads: { isMainThread: true, parentPort: null }, readline: missing("readline"),
  };
  const requireFrom = (dir) => {
    const req = (spec) => {
      const name = String(spec).replace(/^node:/, "");
      if (Object.prototype.hasOwnProperty.call(builtins, name)) return builtins[name];
      const file = call("resolve", String(spec), dir);
      if (cache.has(file)) return cache.get(file).exports;
      const src = call("source", file);
      const module = { exports: {}, id: file, filename: file, loaded: false, children: [], paths: [] };
      cache.set(file, module);
      if (src.json !== undefined) { module.exports = JSON.parse(src.json); return module.exports; }
      const fileDir = path.dirname(file);
      const fn = new Function("exports", "require", "module", "__filename", "__dirname", src.js + "\n//# sourceURL=" + file);
      const r = requireFrom(fileDir);
      fn.call(module.exports, module.exports, r, module, file, fileDir);
      module.loaded = true;
      return module.exports;
    };
    req.resolve = (spec) => (Object.prototype.hasOwnProperty.call(builtins, String(spec).replace(/^node:/, "")) ? String(spec) : call("resolve", String(spec), dir));
    req.cache = cache;
    return req;
  };
  builtins.module = { createRequire: (from) => requireFrom(path.dirname(pathOf(from).replace(/^file:\/\//, ""))), builtinModules: Object.keys(builtins) };
  g.__raea_node_builtins = builtins;
  g.__raea_node_require = requireFrom;
  g.Buffer = Buffer;
  g.global = g;
})(globalThis);
