// @raea/motion/ml: ONNX models in a composition or a game, on the engine's ONNX Runtime (raea-ml, the `ml.*` native
// calls). One session per model file and options per process, shared by every caller and freed a while after the
// last release.
//
//   const net = loadModel("/media/policy.onnx");            // or staticFile("policy.onnx"), or "cache:silero-vad/silero_vad.onnx"
//   net.inputs;                                              // [{ name, type, shape }]
//   const out = net.run({ obs: tensor(new Float32Array(8), [1, 8]) });   // waits; pure
//   const job = net.request({ obs });  ...  job.result();   // started now, joined on a later frame; pure
//   job.poll();                                              // display-only: done yet?
//   net.release();
//
// Determinism. A CPU model (the default) gives the same outputs for the same inputs every run, so a game may call it
// inside its step and fold the result into its state: live play and a render of the play get the same answer. A job is
// named by its model and inputs, so `result()` is just as pure, wherever it's joined; only *when* it finishes depends
// on the machine, so `poll()` is for display only. A Core ML model (`provider: "coreml"`) isn't bit-exact across runs
// or machines: its `deterministic` is false and its results are for display only (effects, never game state).

import { native } from "./native";

export type TensorType = "float32" | "float64" | "float16" | "int64" | "int32" | "int16" | "int8" | "uint8" | "uint16" | "uint32" | "uint64" | "bool";
export type TensorData = Float32Array | Float64Array | BigInt64Array | BigUint64Array | Int32Array | Int16Array | Int8Array | Uint8Array | Uint16Array | Uint32Array;

/** A tensor: its element type, shape and values (float16 as its bits in a Uint16Array, bool as a Uint8Array). */
export interface Tensor {
  type: TensorType;
  shape: number[];
  data: TensorData;
}

export interface IoInfo {
  name: string;
  type: string;
  /** -1 where the dimension is dynamic */
  shape: number[];
}

export interface ModelOptions {
  /** "cpu" (the default: deterministic) or "coreml" (display-only) */
  provider?: "cpu" | "coreml";
  /** intra-op threads (the machine's cores, at most 4) */
  threads?: number;
  /** graph optimisation 0-3 (3) */
  optimization?: 0 | 1 | 2 | 3;
}

const CTOR: Record<TensorType, { new (b: ArrayBuffer): TensorData }> = {
  float32: Float32Array, float64: Float64Array, float16: Uint16Array, int64: BigInt64Array, int32: Int32Array, int16: Int16Array,
  int8: Int8Array, uint8: Uint8Array, uint16: Uint16Array, uint32: Uint32Array, uint64: BigUint64Array, bool: Uint8Array,
};

function typeOf(d: TensorData): TensorType {
  if (d instanceof Float32Array) return "float32";
  if (d instanceof Float64Array) return "float64";
  if (d instanceof BigInt64Array) return "int64";
  if (d instanceof BigUint64Array) return "uint64";
  if (d instanceof Int32Array) return "int32";
  if (d instanceof Int16Array) return "int16";
  if (d instanceof Int8Array) return "int8";
  if (d instanceof Uint16Array) return "uint16";
  if (d instanceof Uint32Array) return "uint32";
  return "uint8";
}

/** A tensor from values and a shape; plain numbers become float32 unless `type` says otherwise. */
export function tensor(data: TensorData | number[], shape: number[], type?: TensorType): Tensor {
  if (Array.isArray(data)) {
    const t = type ?? "float32";
    const C = CTOR[t];
    const out = new C(new ArrayBuffer(data.length * (C as any).BYTES_PER_ELEMENT));
    for (let i = 0; i < data.length; i++) (out as any)[i] = t === "int64" || t === "uint64" ? BigInt(Math.trunc(data[i])) : data[i];
    return { type: t, shape, data: out };
  }
  return { type: type ?? typeOf(data), shape, data };
}

/** A tensor's values as plain numbers (int64 included). */
export function numbers(t: Tensor): number[] {
  return Array.from(t.data as ArrayLike<number | bigint>, (x) => Number(x));
}

// base64, both ways (tensors cross to Rust as their bytes)
const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const UNB64 = new Uint8Array(128);
for (let i = 0; i < 64; i++) UNB64[B64.charCodeAt(i)] = i;

function encode(b: Uint8Array): string {
  let out = "";
  const parts: string[] = [];
  let i = 0;
  for (; i + 2 < b.length; i += 3) {
    const n = (b[i] << 16) | (b[i + 1] << 8) | b[i + 2];
    out += B64[n >> 18] + B64[(n >> 12) & 63] + B64[(n >> 6) & 63] + B64[n & 63];
    if (out.length > 8192) {
      parts.push(out);
      out = "";
    }
  }
  if (i < b.length) {
    const n = (b[i] << 16) | ((i + 1 < b.length ? b[i + 1] : 0) << 8);
    out += B64[n >> 18] + B64[(n >> 12) & 63] + (i + 1 < b.length ? B64[(n >> 6) & 63] : "=") + "=";
  }
  parts.push(out);
  return parts.join("");
}

function decode(s: string): Uint8Array {
  const pad = s.endsWith("==") ? 2 : s.endsWith("=") ? 1 : 0;
  const out = new Uint8Array((s.length / 4) * 3 - pad);
  let o = 0;
  for (let i = 0; i < s.length; i += 4) {
    const n = (UNB64[s.charCodeAt(i)] << 18) | (UNB64[s.charCodeAt(i + 1)] << 12) | (UNB64[s.charCodeAt(i + 2)] << 6) | UNB64[s.charCodeAt(i + 3)];
    if (o < out.length) out[o++] = n >> 16;
    if (o < out.length) out[o++] = (n >> 8) & 255;
    if (o < out.length) out[o++] = n & 255;
  }
  return out;
}

type Wire = { type: string; shape: number[]; data: string };

function toWire(t: Tensor): Wire {
  const d = t.data;
  return { type: t.type, shape: t.shape, data: encode(new Uint8Array(d.buffer, d.byteOffset, d.byteLength)) };
}

function fromWire(w: Wire): Tensor {
  const t = w.type as TensorType;
  const bytes = decode(w.data);
  const C = CTOR[t] ?? Uint8Array;
  return { type: t, shape: w.shape, data: new C(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer) };
}

function wireFeeds(feeds: Record<string, Tensor>): Record<string, Wire> {
  const out: Record<string, Wire> = {};
  for (const k of Object.keys(feeds)) out[k] = toWire(feeds[k]);
  return out;
}

function fromOutputs(o: Record<string, Wire>): Record<string, Tensor> {
  const out: Record<string, Tensor> = {};
  for (const k of Object.keys(o)) out[k] = fromWire(o[k]);
  return out;
}

/** A run started on a worker: `result()` waits for it (pure); `poll()` doesn't (display-only). */
export class Job {
  constructor(readonly id: string) {}

  /** The outputs, waiting for the run to finish if it hasn't: the same answer wherever it's joined. */
  result(): Record<string, Tensor> {
    return fromOutputs(native("ml.result", { job: this.id }) as Record<string, Wire>);
  }

  /** The outputs if the run has finished, else null. Display-only: don't fold it into a game's state. */
  poll(): Record<string, Tensor> | null {
    const r = native("ml.poll", { job: this.id }) as { done: boolean; outputs?: Record<string, Wire>; error?: string };
    if (r.error) throw new Error(r.error);
    return r.done && r.outputs ? fromOutputs(r.outputs) : null;
  }
}

/** A loaded model. */
export class Model {
  readonly inputs: IoInfo[];
  readonly outputs: IoInfo[];
  readonly deterministic: boolean;
  readonly file: string;
  private released = false;

  constructor(readonly id: number, info: { file: string; inputs: IoInfo[]; outputs: IoInfo[]; deterministic: boolean }, private readonly key: string) {
    this.file = info.file;
    this.inputs = info.inputs;
    this.outputs = info.outputs;
    this.deterministic = info.deterministic;
  }

  /** One run (waits for it): the outputs by name, or only those in `outputs`. */
  run(feeds: Record<string, Tensor>, outputs?: string[]): Record<string, Tensor> {
    return fromOutputs(native("ml.run", { id: this.id, inputs: wireFeeds(feeds), outputs }) as Record<string, Wire>);
  }

  /** A run started on a worker now, to join later with `result()` (heavy models: ask a few frames ahead). */
  request(feeds: Record<string, Tensor>): Job {
    return new Job((native("ml.request", { id: this.id, inputs: wireFeeds(feeds) }) as { job: string }).job);
  }

  /** This hold let go: the session is freed 30 s after the last one (`now`: at once). loadModel loads it again. */
  release(now = false): void {
    if (this.released) return;
    this.released = true;
    loaded.delete(this.key);
    native("ml.release", { id: this.id, now });
  }
}

const loaded = new Map<string, Model>();

/**
 * The model at `src` ("/media/x.onnx" or staticFile("x.onnx"): the composition's media folder; "cache:name/file.onnx":
 * raea's model cache; or an absolute path), loaded once and kept: every call with the same source and options returns
 * the same Model, so calling it from a component each frame costs a map lookup.
 */
export function loadModel(src: string, options: ModelOptions = {}): Model {
  const key = src + "\u0000" + JSON.stringify(options);
  const have = loaded.get(key);
  if (have) return have;
  const r = native("ml.load", { src, ...options }) as { id: number; file: string; inputs: IoInfo[]; outputs: IoInfo[]; deterministic: boolean };
  const m = new Model(r.id, r, key);
  loaded.set(key, m);
  return m;
}

/** The models loaded in this process (id, file, holds, runs, total ms). */
export function loadedModels(): { id: number; file: string; holds: number; runs: number; ms: number }[] {
  return native("ml.loaded", {}) as { id: number; file: string; holds: number; runs: number; ms: number }[];
}
