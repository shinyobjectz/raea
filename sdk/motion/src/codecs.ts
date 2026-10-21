// @raea/motion/codecs: mediabunny (MPL-2.0) as the media API, with the engine's coders underneath. Decoding and
// encoding run on VideoToolbox (raea-media's packet coders) and Symphonia; a decoded frame stays in the engine and a
// VideoSample holds only its handle (NativeFrame), so Input -> Conversion -> Output code moves pixels between native
// coders without copying them into JavaScript. Files are read and written by the engine.
//
//   useNativeMedia();                                  // register the coders (once)
//   const input = fileInput("take.mp4");
//   const conv = await Conversion.init({ input, output, trim: { start: 1, end: 3 } });
//   await conv.execute(); saveTarget(output.target as BufferTarget, "out.mp4");

import {
  ALL_FORMATS,
  AudioSample,
  BufferTarget,
  CustomAudioDecoder,
  CustomAudioEncoder,
  CustomSource,
  CustomVideoDecoder,
  CustomVideoEncoder,
  EncodedPacket,
  Input,
  VideoSample,
  VideoSampleColorSpace,
  VideoSampleResource,
  registerDecoder,
  registerEncoder,
  registerVideoSampleTransformer,
  StreamTarget,
} from "mediabunny";
import { hasNative, native } from "./native";

const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64_INDEX = new Int16Array(128).fill(-1);
for (let i = 0; i < B64.length; i++) B64_INDEX[B64.charCodeAt(i)] = i;

/** Bytes as base64 (bare V8 has no btoa). */
export function toBase64(b: Uint8Array): string {
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

/** Base64 as bytes. */
export function fromBase64(s: string): Uint8Array {
  const clean = s.replace(/=+$/, "");
  const out = new Uint8Array(Math.floor((clean.length * 3) / 4));
  let o = 0;
  for (let i = 0; i < clean.length; i += 4) {
    const a = B64_INDEX[clean.charCodeAt(i)], b = B64_INDEX[clean.charCodeAt(i + 1)];
    const c = i + 2 < clean.length ? B64_INDEX[clean.charCodeAt(i + 2)] : 0;
    const d = i + 3 < clean.length ? B64_INDEX[clean.charCodeAt(i + 3)] : 0;
    const n = (a << 18) | (b << 12) | (c << 6) | d;
    out[o++] = n >> 16;
    if (i + 2 < clean.length) out[o++] = (n >> 8) & 255;
    if (i + 3 < clean.length) out[o++] = n & 255;
  }
  return out.subarray(0, o);
}

function bytesOf(src: AllowSharedBufferSource | undefined): Uint8Array {
  if (!src) return new Uint8Array(0);
  if (src instanceof Uint8Array) return src;
  if (ArrayBuffer.isView(src)) return new Uint8Array(src.buffer, src.byteOffset, src.byteLength);
  return new Uint8Array(src as ArrayBuffer);
}

/** A decoded frame held by the engine: NV12 at its size. Its pixels never come into JavaScript. */
export class NativeFrame extends VideoSampleResource {
  constructor(
    readonly frame: number,
    readonly width: number,
    readonly height: number,
  ) {
    super();
  }
  getFormat() {
    return "NV12" as const;
  }
  getCodedWidth() {
    return this.width;
  }
  getCodedHeight() {
    return this.height;
  }
  getSquarePixelWidth() {
    return this.width;
  }
  getSquarePixelHeight() {
    return this.height;
  }
  getColorSpace() {
    return new VideoSampleColorSpace({ primaries: "bt709", transfer: "bt709", matrix: "bt709", fullRange: false });
  }
  close() {
    native("frame.free", { frame: this.frame });
  }
  getDataPlanes(): never {
    throw new Error("this frame stays in the engine; give it to a native step (savePng, nativeScale, the encoder)");
  }
  toRgbSample(): never {
    throw new Error("this frame stays in the engine; give it to a native step (savePng, nativeScale, the encoder)");
  }
}

/** The engine frame a sample holds, if it holds one. */
export function frameOf(sample: VideoSample): NativeFrame | null {
  const data = (sample as unknown as { _data?: unknown })._data;
  return data instanceof NativeFrame ? data : null;
}

/** H.264 and HEVC packets decoded by VideoToolbox, as samples holding engine frames. */
export class NativeVideoDecoder extends CustomVideoDecoder {
  private id = 0;
  static override supports(codec: string): boolean {
    return hasNative() && (codec === "avc" || codec === "hevc");
  }
  init() {
    this.id = native("vdec.new", { codec: this.config.codec, description: toBase64(bytesOf(this.config.description)) }).id;
  }
  private emit(frames: { frame: number; pts: number; dur: number; width: number; height: number }[]) {
    for (const f of frames) this.onSample(new VideoSample(new NativeFrame(f.frame, f.width, f.height), { timestamp: f.pts, duration: f.dur }));
  }
  decode(packet: EncodedPacket) {
    this.emit(native("vdec.decode", { id: this.id, data: toBase64(packet.data), pts: packet.timestamp, dur: packet.duration }));
  }
  flush() {
    this.emit(native("vdec.flush", { id: this.id }));
  }
  close() {
    native("vdec.free", { id: this.id });
  }
}

/** Engine frames encoded to H.264 by VideoToolbox (scaled natively first if the encoder's size differs). */
export class NativeVideoEncoder extends CustomVideoEncoder {
  private id = 0;
  private described = false;
  static override supports(codec: string): boolean {
    // H.264 where the hardware has it; AV1 and VP9 are offered only where a chip encodes them (and then by the
    // engine's packet encoder once it writes them: today it writes H.264, so they're refused with the reason)
    return hasNative() && codec === "avc" && hardwareEncodes().avc;
  }
  init() {
    const c = this.config;
    const bitrate = typeof c.bitrate === "number" ? c.bitrate : (c.width * c.height * (c.framerate ?? 30)) / 10;
    this.id = native("venc.new", { width: c.width, height: c.height, fps: c.framerate ?? 30, kbps: Math.max(300, Math.round(bitrate / 1000)) }).id;
  }
  private emit(out: { packets: { data: string; key: boolean; pts: number; dur: number }[]; description: string | null }) {
    for (const p of out.packets) {
      let meta: EncodedVideoChunkMetadata | undefined;
      if (!this.described && out.description) {
        const d = fromBase64(out.description);
        const hex = (n: number) => n.toString(16).padStart(2, "0");
        meta = { decoderConfig: { codec: `avc1.${hex(d[1])}${hex(d[2])}${hex(d[3])}`, codedWidth: this.config.width, codedHeight: this.config.height, description: d } };
        this.described = true;
      }
      this.onPacket(new EncodedPacket(fromBase64(p.data), p.key ? "key" : "delta", p.pts, p.dur), meta);
    }
  }
  encode(sample: VideoSample) {
    const res = frameOf(sample);
    if (!res) throw new Error("NativeVideoEncoder encodes engine frames (samples from NativeVideoDecoder or nativeScale)");
    let frame = res.frame;
    let scaled: number | null = null;
    if (res.width !== this.config.width || res.height !== this.config.height) {
      scaled = native("frame.scale", { frame, width: this.config.width, height: this.config.height }).frame;
      frame = scaled;
    }
    this.emit(native("venc.encode", { id: this.id, frame, pts: sample.timestamp, dur: sample.duration }));
    if (scaled !== null) native("frame.free", { frame: scaled });
  }
  flush() {
    this.emit(native("venc.finish", { id: this.id }));
  }
  close() {}
}

let encodes: { hardware: string[]; avc: boolean; hevc: boolean; av1: boolean; vp9: boolean } | null = null;
/** What this machine's hardware encodes (VideoToolbox's list, asked once): AV1 and VP9 only where a chip has them. */
export function hardwareEncodes(): { hardware: string[]; avc: boolean; hevc: boolean; av1: boolean; vp9: boolean } {
  if (!hasNative()) return { hardware: [], avc: false, hevc: false, av1: false, vp9: false };
  return (encodes ??= native("venc.codecs", {}));
}

/** AAC, MP3 and FLAC packets decoded by Symphonia, as planar f32 samples laid end to end from the first packet's
 * time (as ffmpeg's and rrl's decoders give them: a dropout in a phone's audio timestamps closes up rather than
 * becoming silence, so the three agree sample for sample). */
export class NativeAudioDecoder extends CustomAudioDecoder {
  private id = 0;
  private next: number | null = null;
  static override supports(codec: string): boolean {
    return hasNative() && (codec === "aac" || codec === "mp3" || codec === "flac");
  }
  init() {
    this.id = native("adec.new", { codec: this.config.codec, description: toBase64(bytesOf(this.config.description)), rate: this.config.sampleRate, channels: this.config.numberOfChannels }).id;
  }
  decode(packet: EncodedPacket) {
    const r = native("adec.decode", { id: this.id, data: toBase64(packet.data) });
    if (!r.frames) return;
    const bytes = fromBase64(r.data);
    let pcm = new Float32Array(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
    let t = this.next ?? packet.timestamp;
    this.next = t + r.frames / r.rate;
    // the encoder's priming (samples before time 0, as an MP4's edit list or skip samples mark them) is dropped, as
    // ffmpeg drops it, so the two give the same samples from 0
    let frames = r.frames;
    if (t < 0) {
      const skip = Math.min(frames, Math.round(-t * r.rate));
      if (skip === frames) return;
      const kept = new Float32Array((frames - skip) * r.channels);
      for (let c = 0; c < r.channels; c++) kept.set(pcm.subarray(c * frames + skip, (c + 1) * frames), c * (frames - skip));
      pcm = kept;
      frames -= skip;
      t += skip / r.rate;
    }
    this.onSample(new AudioSample({ data: pcm, format: "f32-planar", numberOfChannels: r.channels, sampleRate: r.rate, timestamp: Math.max(0, t) }));
  }
  flush() {
    this.next = null;
  }
  close() {
    native("adec.free", { id: this.id });
  }
}

/** Samples encoded to AAC-LC by AudioToolbox (the engine's, on Apple), 1024 frames a packet. The encoder's priming
 * (2112 frames) is in the first packets' timestamps (before 0), so a muxer can mark it to skip and the decoded sound
 * lines up with the input. */
export class NativeAudioEncoder extends CustomAudioEncoder {
  private id = 0;
  private described = false;
  private description: Uint8Array | null = null;
  static override supports(codec: string): boolean {
    return hasNative() && codec === "aac" && native("host.platform", {}) !== "linux";
  }
  init() {
    const c = this.config;
    const r = native("aenc.new", { rate: c.sampleRate, channels: c.numberOfChannels, kbps: Math.round((typeof c.bitrate === "number" ? c.bitrate : 128000) / 1000) });
    this.id = r.id;
    this.description = fromBase64(r.description);
  }
  private emit(packets: { data: string; pts: number; dur: number }[]) {
    for (const p of packets) {
      let meta: EncodedAudioChunkMetadata | undefined;
      if (!this.described && this.description) {
        meta = { decoderConfig: { codec: "mp4a.40.2", sampleRate: this.config.sampleRate, numberOfChannels: this.config.numberOfChannels, description: this.description } };
        this.described = true;
      }
      this.onPacket(new EncodedPacket(fromBase64(p.data), "key", p.pts, p.dur), meta);
    }
  }
  encode(sample: AudioSample) {
    const ch = sample.numberOfChannels;
    const n = sample.numberOfFrames;
    const planes = new Float32Array(ch * n);
    for (let c = 0; c < ch; c++) sample.copyTo(planes.subarray(c * n, (c + 1) * n), { planeIndex: c, format: "f32-planar" });
    this.emit(native("aenc.encode", { id: this.id, data: toBase64(new Uint8Array(planes.buffer)), channels: ch }).packets);
  }
  flush() {
    this.emit(native("aenc.finish", { id: this.id }).packets);
  }
  close() {
    native("aenc.free", { id: this.id });
  }
}

let registered = false;
/** Register the engine's coders with mediabunny (once). */
export function useNativeMedia(): void {
  if (registered) return;
  registerDecoder(NativeVideoDecoder);
  registerDecoder(NativeAudioDecoder);
  registerEncoder(NativeVideoEncoder);
  registerEncoder(NativeAudioEncoder);
  // rotation, flip, crop and resize of engine frames, natively (there's no canvas here)
  registerVideoSampleTransformer((sample, d) => {
    const f = frameOf(sample);
    if (!f) return null;
    const r = native("frame.transform", { frame: f.frame, rotation: d.rotation, flip: d.flip, crop: d.crop, width: d.width, height: d.height, fit: d.fit });
    return new VideoSample(new NativeFrame(r.frame, r.width, r.height), { timestamp: sample.timestamp, duration: sample.duration });
  });
  registered = true;
}

/** A file as mediabunny input, read by the engine. */
export function fileInput(path: string): Input {
  const size = native("media.size", { path });
  return new Input({
    source: new CustomSource({ getSize: () => size, read: (start, end) => fromBase64(native("media.read", { path, start, end })) }),
    formats: ALL_FORMATS,
  });
}

/** A finished BufferTarget written to `path` by the engine. */
export function saveTarget(target: BufferTarget, path: string): void {
  if (!target.buffer) throw new Error("saveTarget: the output isn't finalized yet");
  native("media.write", { path, data: toBase64(new Uint8Array(target.buffer)), truncate: true });
}

/** An output written straight to a file by the engine, chunk by chunk as the muxer makes it, so a long file is never
 * held whole in JavaScript (a BufferTarget is). `new Output({ format, target: fileTarget("out.mp4") })`. */
export function fileTarget(path: string, o: { chunkSize?: number } = {}): StreamTarget {
  native("media.write", { path, data: "", truncate: true });
  const writable = new WritableStream<{ type: string; data: Uint8Array; position: number }>({
    write(chunk) {
      native("media.write", { path, data: toBase64(chunk.data), at: chunk.position });
    },
  });
  return new StreamTarget(writable, { chunked: true, chunkSize: o.chunkSize ?? 4 << 20 });
}

/** A sample's frame written as a PNG by the engine. */
export function savePng(sample: VideoSample, path: string): void {
  const f = frameOf(sample);
  if (!f) throw new Error("savePng takes samples holding engine frames");
  // as it's shown: a track's rotation applied (a phone's portrait take is stored landscape)
  const rot = ((sample.rotation ?? 0) % 360 + 360) % 360;
  if (!rot) {
    native("frame.png", { frame: f.frame, path });
    return;
  }
  const [w, h] = rot % 180 ? [f.height, f.width] : [f.width, f.height];
  const r = native("frame.transform", { frame: f.frame, rotation: rot, width: w, height: h, fit: "fill" });
  native("frame.png", { frame: r.frame, path });
  native("frame.free", { frame: r.frame });
}

/** A Conversion `process` step that scales frames natively to width x height (set processedWidth/Height to match). */
export function nativeScale(width: number, height: number): (sample: VideoSample) => VideoSampleResource {
  return (sample) => {
    const f = frameOf(sample);
    if (!f) throw new Error("nativeScale takes samples holding engine frames");
    const r = native("frame.scale", { frame: f.frame, width, height });
    return new NativeFrame(r.frame, r.width, r.height);
  };
}

export interface TranscodeOptions {
  /** start and end, seconds */
  from?: number;
  to?: number;
  /** the output height in pixels (never upscaled; the width keeps the shape) */
  height?: number;
  kbps?: number;
}

/** A heavy job handed whole to the engine: `path` decoded, trimmed, scaled and encoded with its sound to `out` (MP4),
 * natively end to end; nothing passes through JavaScript. Mediabunny's per-frame pipeline is for work that needs a
 * step per frame; a straight trim or proxy goes here. */
export function transcode(path: string, out: string, opts: TranscodeOptions = {}): { frames: number; width: number; height: number; fps: number; kbps: number; bytes: number } {
  return native("media.transcode", { path, out, ...opts });
}

/** Captions as WebVTT text from the transcript's words ({text, from, to} in frames, as useWords gives them): words
 * grouped into cues of up to `perCue`, a cue ending early at a pause longer than `gap` frames or after punctuation.
 * Hand it to mediabunny's TextSubtitleSource("webvtt") and an Output's addSubtitleTrack. */
export function wordsToWebVTT(words: readonly { text: string; from: number; to: number }[], fps: number, o: { perCue?: number; gap?: number } = {}): string {
  const per = o.perCue ?? 4;
  const gap = o.gap ?? 8;
  const time = (f: number) => {
    const ms = Math.max(0, Math.round((f / fps) * 1000));
    const h = Math.floor(ms / 3600000), m = Math.floor((ms % 3600000) / 60000), s = Math.floor((ms % 60000) / 1000), r = ms % 1000;
    return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}.${String(r).padStart(3, "0")}`;
  };
  const cues: { from: number; to: number; text: string[] }[] = [];
  for (const w of words) {
    const last = cues[cues.length - 1];
    const prev = last?.text[last.text.length - 1] ?? "";
    if (!last || last.text.length >= per || w.from - last.to > gap || /[.!?,;:]$/.test(prev)) cues.push({ from: w.from, to: w.to, text: [w.text] });
    else {
      last.text.push(w.text);
      last.to = w.to;
    }
  }
  return "WEBVTT\n\n" + cues.map((c) => `${time(c.from)} --> ${time(c.to)}\n${c.text.join(" ")}\n`).join("\n");
}
