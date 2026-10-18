// node:zlib on raea's V8: the synchronous gzip and deflate calls packages use, done natively (raea-native's zlib.*,
// flate2). Bytes in as any buffer or view, a Uint8Array out.
const call = (name: string, args: unknown) => JSON.parse((globalThis as any).__raea_call(name, JSON.stringify(args)));
const b64 = (b: Uint8Array): string => {
  let s = "";
  for (let i = 0; i < b.length; i += 8192) s += String.fromCharCode.apply(null, Array.from(b.subarray(i, i + 8192)));
  return btoa(s);
};
const unb64 = (s: string): Uint8Array => {
  const t = atob(s);
  const out = new Uint8Array(t.length);
  for (let i = 0; i < t.length; i++) out[i] = t.charCodeAt(i);
  return out;
};
const bytes = (d: unknown): Uint8Array => {
  if (typeof d === "string") return new TextEncoder().encode(d);
  if (d instanceof Uint8Array) return d;
  if (ArrayBuffer.isView(d)) return new Uint8Array(d.buffer, d.byteOffset, d.byteLength);
  if (d instanceof ArrayBuffer) return new Uint8Array(d);
  throw new TypeError("zlib: expected a string, buffer or view");
};
type Opts = { level?: number };
export const constants = {
  Z_NO_COMPRESSION: 0,
  Z_BEST_SPEED: 1,
  Z_BEST_COMPRESSION: 9,
  Z_DEFAULT_COMPRESSION: -1,
};
const lvl = (o?: Opts) => (o?.level === undefined || o.level < 0 ? 6 : o.level);
export const gzipSync = (d: unknown, o?: Opts) => unb64(call("zlib.gzip", { data: b64(bytes(d)), level: lvl(o) }));
export const gunzipSync = (d: unknown) => unb64(call("zlib.gunzip", { data: b64(bytes(d)) }));
export const deflateSync = (d: unknown, o?: Opts) => unb64(call("zlib.deflate", { data: b64(bytes(d)), level: lvl(o) }));
export const inflateSync = (d: unknown) => unb64(call("zlib.inflate", { data: b64(bytes(d)) }));
export const deflateRawSync = (d: unknown, o?: Opts) => unb64(call("zlib.deflate", { data: b64(bytes(d)), level: lvl(o), raw: true }));
export const inflateRawSync = (d: unknown) => unb64(call("zlib.inflate", { data: b64(bytes(d)), raw: true }));
export default { constants, gzipSync, gunzipSync, deflateSync, inflateSync, deflateRawSync, inflateRawSync };
