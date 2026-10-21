// @raea/motion/tone: Tone.js's API, running on raea's one mixer core (no Web Audio). `import * as Tone from
// "@raea/motion/tone"` and write Tone as its docs show: instruments, effects, channels, connect/toDestination/chain,
// triggerAttackRelease, the Transport with Loop, Part, Sequence and Pattern, Tone's time notation.
//
// What's different, because a composition is a function of its frame (sdk/react/docs/audio-stack.md, "Tone.js"):
// - Nothing is scheduled on a wall clock. Tone code builds a graph and an event list; the Transport's callbacks run in
//   time order as the frames need them; each event lands on its own sample of the timeline. A render and a play of
//   the same edit are the same samples.
// - Time 0 is the composition's frame 0. Tone.now() is 0 at the top level and the event's time inside a callback.
// - The graph (instruments, effects, channels and how they connect) is the one the edit has when its first frame
//   renders: make instruments and effects at the top level. Param changes and ramps after that are events.
// - Draw.schedule, Offline, analysers and user media have no meaning here (see the compatibility table).
// raea's additions: buses (music, sfx, voice, ambience, ui) with ducking, generated sources (sao, tokensynth) for
// Sampler and Player, seeded variation, <Sfx> for a game's one-shots, and Time(...).toFrames().

import { type ReactNode } from "react";
import { useCurrentFrame, useVideoConfig } from "./clock";
import { isGeneratedAsset, tokensynthNote, type GeneratedAsset } from "./assets";

// ---- the edit's Tone: its graph and its events -----------------------------------------------------------------------

type Ev = { k: string; n: number; t: number; e: string; f?: number[]; d?: number; v?: number; o?: number; p?: string; val?: number; ramp?: number };

type Item = { time: number; seq: number; run: (time: number) => void };

class Context {
  nodes = new Map<number, ToneAudioNode>();
  edges: [number, number, number][] = [];
  events: Ev[] = [];
  sorted = true;
  seq = 0;
  nextId = 1;
  /** the time a callback runs at (Tone.now() inside it) */
  now = 0;
  /** where triggers go while an <Sfx> plays (its frame's events), else the edit's list */
  sink: Ev[] | null = null;
  sinkKey = "";
  pending: Item[] = [];
  compiled = 0;
  graph: string | null = null;
  readonly sampleRate = 48000;
  readonly lookAhead = 0;
  readonly latencyHint = "interactive";
  readonly state = "running";
  get currentTime() {
    return this.now;
  }
  resume() {
    return Promise.resolve();
  }
  emit(ev: Omit<Ev, "k"> & { k?: string }) {
    const k = ev.k ?? (this.sink ? `${this.sinkKey}:${this.seq++}` : `${ev.n}:${this.seq++}`);
    const e = { ...ev, k } as Ev;
    if (this.sink) this.sink.push(e);
    else {
      if (this.events.length && this.events[this.events.length - 1].t > e.t) this.sorted = false;
      this.events.push(e);
    }
  }
  schedule(item: Omit<Item, "seq">) {
    const it = { ...item, seq: this.seq++ };
    // sorted insert by (time, seq)
    let lo = 0;
    let hi = this.pending.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      const p = this.pending[mid];
      if (p.time < it.time || (p.time === it.time && p.seq < it.seq)) lo = mid + 1;
      else hi = mid;
    }
    this.pending.splice(lo, 0, it);
  }
  /** runs the scheduled callbacks up to `until` seconds, in time order (the same order whatever frames ask) */
  compileTo(until: number) {
    while (this.pending.length && this.pending[0].time < until) {
      const it = this.pending.shift()!;
      this.now = it.time;
      it.run(it.time);
    }
    this.compiled = Math.max(this.compiled, until);
    this.now = 0;
  }
  window(t0: number, t1: number): Ev[] {
    if (!this.sorted) {
      this.events.sort((a, b) => a.t - b.t || (a.k < b.k ? -1 : 1));
      this.sorted = true;
    }
    // the first event at or after t0
    let lo = 0;
    let hi = this.events.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (this.events[mid].t < t0) lo = mid + 1;
      else hi = mid;
    }
    const out: Ev[] = [];
    for (let i = lo; i < this.events.length && this.events[i].t < t1; i++) out.push(this.events[i]);
    return out;
  }
  graphJson(): string {
    if (this.graph === null) {
      for (const n of this.nodes.values()) n.beforeFreeze();
      const nodes = [...this.nodes.values()].map((n) => n.spec());
      this.graph = JSON.stringify({ nodes, edges: this.edges, mods });
    }
    return this.graph;
  }
}

const ctx = new Context();
/** LFOs connected to parameters: [lfo id, node id, parameter name] */
let mods: [number, number, string][] = [];

export function getContext() {
  return ctx;
}
export const context = ctx;
export function setContext(_c: unknown) {}
export function start() {
  return Promise.resolve();
}
export function loaded() {
  return Promise.resolve();
}
export function now() {
  return ctx.now;
}
export function immediate() {
  return ctx.now;
}
export const version = "15.1.22-raea";

// ---- units --------------------------------------------------------------------------------------------------------

export type Time = number | string;
export type Frequency = number | string;

const NOTE = /^([a-gA-G])(#{1,2}|b{1,2}|x)?(-?\d+)$/;
const OFFSETS: Record<string, number> = { c: 0, d: 2, e: 4, f: 5, g: 7, a: 9, b: 11 };

export function mtof(m: number): number {
  return 440 * Math.pow(2, (m - 69) / 12);
}
export function ftom(f: number): number {
  return Math.round(69 + 12 * Math.log2(f / 440));
}
export function dbToGain(db: number): number {
  return Math.pow(10, db / 20);
}
export function gainToDb(g: number): number {
  return 20 * Math.log10(g);
}
export function intervalToFrequencyRatio(i: number): number {
  return Math.pow(2, i / 12);
}

function toMidi(n: Frequency): number {
  if (typeof n === "number") return 69 + 12 * Math.log2(n / 440);
  const s = n.trim();
  const m = NOTE.exec(s);
  if (m) {
    const acc = m[2] === "x" ? 2 : m[2] ? (m[2][0] === "#" ? m[2].length : -m[2].length) : 0;
    return 12 * (Number(m[3]) + 1) + OFFSETS[m[1].toLowerCase()] + acc;
  }
  if (/hz$/i.test(s)) return 69 + 12 * Math.log2(parseFloat(s) / 440);
  if (/^\d+(\.\d+)?$/.test(s)) return 69 + 12 * Math.log2(parseFloat(s) / 440);
  throw new RangeError(`Tone: "${n}" isn't a note or a frequency`);
}

function toHz(n: Frequency): number {
  if (typeof n === "number") return n;
  return mtof(toMidi(n));
}

const NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

class FrequencyClass {
  constructor(private v: Frequency, private units?: string) {}
  private hz() {
    return this.units === "midi" ? mtof(Number(this.v)) : toHz(this.v);
  }
  toFrequency() {
    return this.hz();
  }
  toMidi() {
    return Math.round(69 + 12 * Math.log2(this.hz() / 440));
  }
  toNote() {
    const m = this.toMidi();
    return `${NAMES[((m % 12) + 12) % 12]}${Math.floor(m / 12) - 1}`;
  }
  toSeconds() {
    return 1 / this.hz();
  }
  transpose(i: number) {
    return new FrequencyClass(this.hz() * intervalToFrequencyRatio(i));
  }
  harmonize(is: number[]) {
    return is.map((i) => this.transpose(i));
  }
  valueOf() {
    return this.hz();
  }
}

export function Frequency(v: Frequency, units?: string) {
  return new FrequencyClass(v, units);
}
export function Midi(v: number) {
  return new FrequencyClass(v, "midi");
}

/** Tone's time notation in seconds, at the Transport's tempo and meter. */
function seconds(t: Time | undefined, relativeTo = ctx.now): number {
  if (t === undefined || t === null) return relativeTo;
  if (typeof t === "number") return t;
  let s = String(t).trim();
  if (s === "") return relativeTo;
  if (s.startsWith("+")) return relativeTo + seconds(s.slice(1), 0);
  if (s.startsWith("@")) {
    const q = seconds(s.slice(1), 0);
    return q > 0 ? Math.ceil(relativeTo / q - 1e-9) * q : relativeTo;
  }
  if (s.includes(" + ")) return s.split(" + ").reduce((a, x) => a + seconds(x, 0), 0);
  const tr = transport;
  const beat = 60 / tr.bpm.value;
  const [num, den] = tr.ts();
  const bar = num * beat * (4 / den);
  let m = /^(-?\d+(?:\.\d+)?):(\d+(?:\.\d+)?)(?::(\d+(?:\.\d+)?))?$/.exec(s);
  if (m) return Number(m[1]) * bar + Number(m[2]) * beat * (4 / den) + Number(m[3] ?? 0) * (beat / 4);
  m = /^(\d+(?:\.\d+)?)(n|t|m)(\.?)$/.exec(s);
  if (m) {
    const v = Number(m[1]);
    let x = m[2] === "m" ? v * bar : (4 / v) * beat;
    if (m[2] === "t") x *= 2 / 3;
    if (m[3] === ".") x *= 1.5;
    return x;
  }
  m = /^(\d+(?:\.\d+)?)i$/.exec(s);
  if (m) return (Number(m[1]) / tr.PPQ) * beat;
  m = /^(\d+(?:\.\d+)?)hz$/i.exec(s);
  if (m) return 1 / Number(m[1]);
  if (/^-?\d+(\.\d+)?s?$/.test(s)) return parseFloat(s);
  throw new RangeError(`Tone: "${t}" isn't a time (seconds, "4n", "8t", "1m", "+0.5", "1:2:3" or "192i")`);
}

class TimeClass {
  constructor(private t: Time) {}
  toSeconds() {
    return seconds(this.t, 0);
  }
  toMilliseconds() {
    return this.toSeconds() * 1000;
  }
  toTicks() {
    return Math.round((this.toSeconds() / (60 / transport.bpm.value)) * transport.PPQ);
  }
  toFrequency() {
    return 1 / this.toSeconds();
  }
  toBarsBeatsSixteenths() {
    return position(this.toSeconds());
  }
  /** raea: the time in frames at `fps` (cuts on beats: <Series.Sequence durationInFrames={Tone.Time("1m").toFrames(30)}>) */
  toFrames(fps = 30) {
    return Math.round(this.toSeconds() * fps);
  }
  valueOf() {
    return this.toSeconds();
  }
}

export function Time(t: Time) {
  return new TimeClass(t);
}
export const TransportTime = Time;
export const Ticks = (t: Time) => new TimeClass(t).toTicks();

function position(secs: number): string {
  const beat = 60 / transport.bpm.value;
  const [num, den] = transport.ts();
  const qn = secs / beat;
  const per = num * (4 / den);
  const bars = Math.floor(qn / per + 1e-9);
  const beats = Math.floor(qn - bars * per + 1e-9);
  const six = Math.round((qn - bars * per - beats) * 4 * 1000) / 1000;
  return `${bars}:${beats}:${six}`;
}

// ---- params -------------------------------------------------------------------------------------------------------

/** A Tone Param or Signal: a value (set before the first frame: the graph's; after: an event) and its ramps. */
export class Param {
  constructor(readonly owner: ToneAudioNode, readonly key: string[], public readonly units = "number") {}
  get value(): number {
    let o: any = this.owner.options;
    for (const k of this.key) o = o?.[k];
    return typeof o === "number" ? o : 0;
  }
  set value(v: number) {
    if (ctx.graph === null) {
      let o: any = this.owner.options;
      for (const k of this.key.slice(0, -1)) o = o[k] ??= {};
      o[this.key[this.key.length - 1]] = v;
    } else this.ramp(v, 0, ctx.now);
  }
  protected ramp(v: number, over: number, at: number, exp = false) {
    // before the graph is fixed, a change at time 0 is the graph's value; later ones are events
    if (ctx.graph === null && at <= 0 && over <= 0) {
      this.value = v;
      return this;
    }
    ctx.emit({ n: this.owner.id, t: at, e: "param", p: this.key[this.key.length - 1], val: v, ramp: over, ...(exp ? { curve: "exp" } : {}) } as any);
    return this;
  }
  setValueAtTime(v: number, time?: Time) {
    return this.ramp(v, 0, seconds(time));
  }
  linearRampToValueAtTime(v: number, time: Time) {
    const end = seconds(time);
    return this.ramp(v, Math.max(0, end - ctx.now), ctx.now);
  }
  exponentialRampToValueAtTime(v: number, time: Time) {
    const end = seconds(time);
    return this.ramp(v, Math.max(0, end - ctx.now), ctx.now, true);
  }
  rampTo(v: number, rampTime: Time = 0.1, startTime?: Time) {
    return this.ramp(v, seconds(rampTime, 0), seconds(startTime));
  }
  linearRampTo(v: number, rampTime: Time, startTime?: Time) {
    return this.rampTo(v, rampTime, startTime);
  }
  exponentialRampTo(v: number, rampTime: Time, startTime?: Time) {
    return this.ramp(v, seconds(rampTime, 0), seconds(startTime), true);
  }
  targetRampTo(v: number, rampTime: Time, startTime?: Time) {
    return this.rampTo(v, rampTime, startTime);
  }
  setTargetAtTime(v: number, startTime: Time, timeConstant: number) {
    return this.ramp(v, timeConstant * 3, seconds(startTime));
  }
  cancelScheduledValues(_t?: Time) {
    return this;
  }
  cancelAndHoldAtTime(_t?: Time) {
    return this;
  }
  getValueAtTime(_t: Time) {
    return this.value;
  }
}

export const Signal = Param;

// ---- nodes --------------------------------------------------------------------------------------------------------

type Options = Record<string, any>;

function merge(base: Options, over: Options | undefined): Options {
  const out: Options = { ...base };
  for (const [k, v] of Object.entries(over ?? {})) {
    if (v && typeof v === "object" && !Array.isArray(v) && !isGeneratedAsset(v) && base[k] && typeof base[k] === "object") out[k] = merge(base[k], v);
    else out[k] = v;
  }
  return out;
}

/** Every Tone node: an id in the edit's graph, its options (Tone's names), and its connections. */
export class ToneAudioNode {
  readonly id: number;
  options: Options;
  name: string;
  /** a bus's name (Audio and Video elements play into it with bus="...") */
  busName?: string;
  constructor(kind: string, defaults: Options, opts?: Options) {
    this.name = kind;
    this.options = merge(defaults, opts);
    this.id = kind === "Destination" ? 0 : ctx.nextId++;
    ctx.nodes.set(this.id, this);
    if (ctx.graph !== null && kind !== "Destination") {
      // a node made after the first frame: the graph is rebuilt with it (make instruments at the top level)
      ctx.graph = null;
    }
  }
  get volume(): Param {
    return new Param(this, ["volume"], "decibels");
  }
  connect(dest: ToneAudioNode | Param | AudioParamLike, _out = 0, _in = 0): this {
    // Tone.Destination (the proxy) is the Destination node
    if ((dest as unknown) === Destination) dest = getDestination();
    if (dest instanceof ToneAudioNode) {
      ctx.edges.push([this.id, dest.id, 0]);
      if (ctx.graph !== null) ctx.graph = null;
    }
    return this;
  }
  disconnect(dest?: ToneAudioNode): this {
    ctx.edges = ctx.edges.filter(([a, b]) => !(a === this.id && (dest === undefined || b === dest.id)));
    if (ctx.graph !== null) ctx.graph = null;
    return this;
  }
  toDestination(): this {
    return this.connect(getDestination());
  }
  toMaster(): this {
    return this.toDestination();
  }
  chain(...nodes: ToneAudioNode[]): this {
    let prev: ToneAudioNode = this;
    for (const n of nodes) {
      prev.connect(n);
      prev = n;
    }
    return this;
  }
  fan(...nodes: ToneAudioNode[]): this {
    for (const n of nodes) this.connect(n);
    return this;
  }
  set(o: Options): this {
    if (ctx.graph === null) this.options = merge(this.options, o);
    else for (const [k, v] of Object.entries(o)) if (typeof v === "number") ctx.emit({ n: this.id, t: ctx.now, e: "param", p: k, val: v, ramp: 0 });
    return this;
  }
  get(): Options {
    return JSON.parse(JSON.stringify(this.options));
  }
  dispose(): this {
    return this;
  }
  sync(): this {
    return this;
  }
  unsync(): this {
    return this;
  }
  get numberOfInputs() {
    return 1;
  }
  get numberOfOutputs() {
    return 1;
  }
  toString() {
    return this.name;
  }
  /** before the graph is written: settings that come from properties set after construction */
  beforeFreeze() {}
  spec(): Options {
    return { id: this.id, kind: this.name, options: this.options, ...(this.busName ? { name: this.busName } : {}) };
  }
}

type AudioParamLike = { value: number };

class DestinationClass extends ToneAudioNode {
  constructor() {
    super("Destination", { volume: 0, mute: false });
  }
  get mute(): boolean {
    return !!this.options.mute;
  }
  set mute(m: boolean) {
    this.options.mute = m;
  }
  get maxChannelCount() {
    return 2;
  }
  override connect(): this {
    return this;
  }
}

let destination: DestinationClass | null = null;
export function getDestination(): DestinationClass {
  return (destination ??= new DestinationClass());
}
export const Destination = new Proxy({} as DestinationClass, { get: (_, k) => (getDestination() as any)[k], set: (_, k, v) => (((getDestination() as any)[k] = v), true) });
export const Master = Destination;

// ---- instruments --------------------------------------------------------------------------------------------------

const ENV = { attack: 0.005, decay: 0.1, sustain: 0.3, release: 1 };

/** Tone's monophonic and polyphonic instruments: triggerAttack, triggerRelease, triggerAttackRelease, releaseAll. */
export class Instrument extends ToneAudioNode {
  get oscillator(): Options {
    return (this.options.oscillator ??= {});
  }
  get envelope(): Options {
    return (this.options.envelope ??= {});
  }
  get detune(): Param {
    return new Param(this, ["detune"], "cents");
  }
  get portamento(): number {
    return this.options.portamento ?? 0;
  }
  set portamento(p: number) {
    this.options.portamento = p;
  }
  get frequency(): Param {
    return new Param(this, ["frequency"], "frequency");
  }
  protected notes(n: Frequency | Frequency[]): number[] {
    return (Array.isArray(n) ? n : [n]).map(toHz);
  }
  triggerAttack(note: Frequency | Frequency[], time?: Time, velocity = 1): this {
    ctx.emit({ n: this.id, t: seconds(time), e: "a", f: this.notes(note), v: velocity });
    return this;
  }
  triggerRelease(note?: Frequency | Frequency[] | Time, time?: Time): this {
    // Tone's monophonic instruments take triggerRelease(time); PolySynth and Sampler take (notes, time)
    if (this.poly() && note !== undefined && (typeof note !== "number" || time !== undefined)) {
      ctx.emit({ n: this.id, t: seconds(time), e: "r", f: this.notes(note as Frequency | Frequency[]) });
    } else ctx.emit({ n: this.id, t: seconds((this.poly() ? time : (note as Time)) ?? time), e: "ra" });
    return this;
  }
  triggerAttackRelease(note: Frequency | Frequency[], duration: Time | Time[], time?: Time, velocity = 1): this {
    const t = seconds(time);
    const notes = this.notes(note);
    if (Array.isArray(duration)) notes.forEach((f, i) => ctx.emit({ n: this.id, t, e: "ar", f: [f], d: seconds(duration[i % duration.length], 0), v: velocity }));
    else ctx.emit({ n: this.id, t, e: "ar", f: notes, d: seconds(duration, 0), v: velocity });
    return this;
  }
  releaseAll(time?: Time): this {
    ctx.emit({ n: this.id, t: seconds(time), e: "ra" });
    return this;
  }
  poly(): boolean {
    return !!this.options.poly;
  }
}

function inst(kind: string, defaults: Options) {
  return class extends Instrument {
    constructor(opts?: Options) {
      super(kind, defaults, opts);
    }
  };
}

export const Synth = inst("Synth", { oscillator: { type: "triangle" }, envelope: { ...ENV }, volume: 0, detune: 0, portamento: 0 });
export const MonoSynth = inst("MonoSynth", {
  oscillator: { type: "square" },
  envelope: { attack: 0.005, decay: 0.1, sustain: 0.9, release: 1 },
  filter: { Q: 1, type: "lowpass", rolloff: -12 },
  filterEnvelope: { attack: 0.6, decay: 0.2, sustain: 0.5, release: 2, baseFrequency: 200, octaves: 3, exponent: 2 },
  volume: 0,
  detune: 0,
  portamento: 0,
});
export const FMSynth = inst("FMSynth", { harmonicity: 3, modulationIndex: 10, oscillator: { type: "sine" }, envelope: { attack: 0.01, decay: 0.01, sustain: 1, release: 0.5 }, modulation: { type: "square" }, modulationEnvelope: { attack: 0.5, decay: 0, sustain: 1, release: 0.5 }, volume: 0, detune: 0 });
export const AMSynth = inst("AMSynth", { harmonicity: 3, oscillator: { type: "sine" }, envelope: { attack: 0.01, decay: 0.01, sustain: 1, release: 0.5 }, modulation: { type: "square" }, modulationEnvelope: { attack: 0.5, decay: 0, sustain: 1, release: 0.5 }, volume: 0, detune: 0 });
export const DuoSynth = inst("DuoSynth", { harmonicity: 1.5, modulationIndex: 0.5, oscillator: { type: "sine" }, envelope: { attack: 0.01, decay: 0, sustain: 1, release: 0.5 }, volume: 0 });
export const MembraneSynth = inst("MembraneSynth", { pitchDecay: 0.05, octaves: 10, oscillator: { type: "sine" }, envelope: { attack: 0.001, decay: 0.4, sustain: 0.01, release: 1.4 }, volume: 0 });
export const NoiseSynth = inst("NoiseSynth", { noise: { type: "white" }, envelope: { attack: 0.005, decay: 0.1, sustain: 0 , release: 1 }, volume: 0 });
export const MetalSynth = inst("MetalSynth", { frequency: 200, harmonicity: 5.1, modulationIndex: 32, resonance: 4000, octaves: 1.5, envelope: { attack: 0.001, decay: 1.4, sustain: 0, release: 0.2 }, volume: 0 });
export const PluckSynth = inst("PluckSynth", { attackNoise: 1, dampening: 4000, resonance: 0.7, release: 1, volume: 0 });

// NoiseSynth and MetalSynth take (duration, time) and (note?, duration, time) as Tone's do
const noiseAR = NoiseSynth.prototype.triggerAttackRelease;
NoiseSynth.prototype.triggerAttack = function (this: Instrument, time?: Time, velocity = 1) {
  ctx.emit({ n: this.id, t: seconds(time as Time), e: "a", f: [440], v: velocity });
  return this;
} as any;
NoiseSynth.prototype.triggerAttackRelease = function (this: Instrument, duration: Time, time?: Time, velocity = 1) {
  return noiseAR.call(this, 440, duration, time, velocity);
} as any;
MetalSynth.prototype.triggerAttackRelease = function (this: Instrument, a: any, b?: any, c?: any, d?: any) {
  // (duration, time, velocity) or (note, duration, time, velocity)
  // Tone 15: (note, duration, time, velocity); older code: (duration, time, velocity). A note name, or a frequency, first
  const withNote = (typeof a === "string" && NOTE.test(a.trim())) || (typeof a === "number" && a > 20);
  return withNote ? Instrument.prototype.triggerAttackRelease.call(this, a, b, c, d) : Instrument.prototype.triggerAttackRelease.call(this, 440, a, b, c ?? 1);
} as any;

/** Tone's PolySynth: many voices of one instrument (`new PolySynth(Synth, options)`), at most maxPolyphony. */
export class PolySynth extends Instrument {
  constructor(voice?: any, opts?: Options) {
    let V = voice;
    let o = opts;
    if (voice && typeof voice === "object" && !(typeof voice === "function")) {
      o = voice;
      V = voice.voice;
    }
    if (typeof voice === "number") {
      // the legacy (polyphony, voice, options) form
      V = opts as any;
      o = arguments[2];
    }
    const proto = V ? new V() : new Synth();
    ctx.nodes.delete(proto.id);
    super(proto.name, { ...proto.options, maxPolyphony: 32, poly: true }, o && "options" in o ? o.options : o);
    this.options.poly = true;
    if (o && typeof o.maxPolyphony === "number") this.options.maxPolyphony = o.maxPolyphony;
  }
  get maxPolyphony(): number {
    return this.options.maxPolyphony;
  }
  set maxPolyphony(n: number) {
    this.options.maxPolyphony = n;
  }
}

type UrlMap = Record<string | number, string | GeneratedAsset>;

function src(u: string | GeneratedAsset, baseUrl = ""): unknown {
  if (isGeneratedAsset(u)) return { generate: u.kind, prompt: u.prompt, seconds: u.seconds, ...(u.options ?? {}) };
  return /^(https?:|\/|[a-z]:)/i.test(u as string) ? u : baseUrl + u;
}

/** Tone's Sampler: samples at notes, played at the rate that makes any other note (the nearest sample). */
export class Sampler extends Instrument {
  constructor(urls?: UrlMap | Options, onload?: (() => void) | Options, baseUrl?: string) {
    const o: Options = urls && "urls" in (urls as Options) ? (urls as Options) : { urls, ...(typeof onload === "object" ? onload : {}), baseUrl: baseUrl ?? (typeof onload === "object" ? onload.baseUrl : undefined) };
    const map: Options = {};
    for (const [k, v] of Object.entries((o.urls ?? {}) as UrlMap)) map[String(Math.round(toMidi(/^\d+$/.test(k) ? Number(k) > 127 ? Number(k) : mtof(Number(k)) : k)))] = src(v, o.baseUrl ?? "");
    super("Sampler", { attack: 0, release: 0.1, curve: "exponential", volume: 0, poly: true, maxPolyphony: 32 }, { ...o, urls: map, baseUrl: undefined });
    (typeof onload === "function" ? onload : o.onload)?.();
  }
  get loaded() {
    return true;
  }
  add(note: Frequency, url: string | GeneratedAsset, cb?: () => void) {
    this.options.urls[String(Math.round(toMidi(note)))] = src(url);
    cb?.();
    return this;
  }
  has(note: Frequency) {
    return String(Math.round(toMidi(note))) in this.options.urls;
  }
  get attack() {
    return this.options.attack;
  }
  set attack(v: number) {
    this.options.attack = v;
  }
  get release() {
    return this.options.release;
  }
  set release(v: number) {
    this.options.release = v;
  }
}

/** Tone's Player: one file, started and stopped on the timeline. */
export class Player extends Instrument {
  constructor(url?: string | GeneratedAsset | Options, onload?: () => void) {
    const o: Options = typeof url === "string" || isGeneratedAsset(url) ? { url } : (url ?? {});
    super("Player", { playbackRate: 1, loop: false, loopStart: 0, loopEnd: 0, fadeIn: 0, fadeOut: 0, volume: 0, poly: true, maxPolyphony: 32, reverse: false }, { ...o, url: o.url !== undefined ? src(o.url) : undefined });
    (onload ?? o.onload)?.();
    if (o.autostart) this.start(0);
  }
  get loaded() {
    return true;
  }
  get loop() {
    return !!this.options.loop;
  }
  set loop(v: boolean) {
    this.options.loop = v;
  }
  get playbackRate() {
    return this.options.playbackRate;
  }
  set playbackRate(v: number) {
    this.options.playbackRate = v;
  }
  get loopStart() {
    return this.options.loopStart;
  }
  set loopStart(v: number) {
    this.options.loopStart = seconds(v as Time, 0);
  }
  get loopEnd() {
    return this.options.loopEnd;
  }
  set loopEnd(v: number) {
    this.options.loopEnd = seconds(v as Time, 0);
  }
  get fadeIn() {
    return this.options.fadeIn;
  }
  set fadeIn(v: number) {
    this.options.fadeIn = v;
  }
  get fadeOut() {
    return this.options.fadeOut;
  }
  set fadeOut(v: number) {
    this.options.fadeOut = v;
  }
  load(url: string | GeneratedAsset) {
    this.options.url = src(url);
    return Promise.resolve(this);
  }
  start(time?: Time, offset?: Time, duration?: Time): this {
    ctx.emit({ n: this.id, t: seconds(time), e: "start", o: offset !== undefined ? seconds(offset, 0) : 0, ...(duration !== undefined ? { d: seconds(duration, 0) } : {}), v: 1 });
    return this;
  }
  stop(time?: Time): this {
    ctx.emit({ n: this.id, t: seconds(time), e: "stop" });
    return this;
  }
  restart(time?: Time, offset?: Time, duration?: Time): this {
    this.stop(time);
    return this.start(time, offset, duration);
  }
  seek(offset: Time, time?: Time): this {
    return this.restart(time, offset);
  }
  get state() {
    return "stopped";
  }
}

export const GrainPlayer = Player;

/** Tone's Players: named Players into one output. */
export class Players extends ToneAudioNode {
  private map = new Map<string, Player>();
  constructor(urls: Record<string, string | GeneratedAsset> = {}, onload?: (() => void) | Options) {
    super("Gain", { gain: 1, volume: 0 }, typeof onload === "object" ? onload : {});
    for (const [k, v] of Object.entries(urls)) this.add(k, v);
    (typeof onload === "function" ? onload : undefined)?.();
  }
  add(name: string, url: string | GeneratedAsset, cb?: () => void) {
    const p = new Player(url);
    p.connect(this);
    this.map.set(name, p);
    cb?.();
    return this;
  }
  player(name: string): Player {
    const p = this.map.get(name);
    if (!p) throw new Error(`Tone.Players: no player "${name}"`);
    return p;
  }
  has(name: string) {
    return this.map.has(name);
  }
  stopAll(time?: Time) {
    for (const p of this.map.values()) p.stop(time);
    return this;
  }
  get loaded() {
    return true;
  }
}

/** Tone's sources as instruments that start and stop: Oscillator and Noise (and their variants). */
export class Oscillator extends Instrument {
  constructor(frequency: Frequency | Options = 440, type = "sine") {
    const o = typeof frequency === "object" ? frequency : { frequency, type };
    super("Synth", { oscillator: { type: o.type ?? "sine" }, envelope: { attack: 0.003, decay: 0, sustain: 1, release: 0.003 }, volume: o.volume ?? 0, poly: false }, {});
    this.options.frequency = toHz(o.frequency ?? 440);
  }
  get type() {
    return this.options.oscillator.type;
  }
  set type(t: string) {
    this.options.oscillator.type = t;
  }
  start(time?: Time): this {
    ctx.emit({ n: this.id, t: seconds(time), e: "a", f: [this.options.frequency], v: 1 });
    return this;
  }
  stop(time?: Time): this {
    ctx.emit({ n: this.id, t: seconds(time), e: "ra" });
    return this;
  }
}
export const OmniOscillator = Oscillator;
export const FatOscillator = Oscillator;
export const PulseOscillator = Oscillator;

export class Noise extends Instrument {
  constructor(type: string | Options = "white") {
    const o = typeof type === "object" ? type : { type };
    super("NoiseSynth", { noise: { type: o.type ?? "white" }, envelope: { attack: 0.003, decay: 0, sustain: 1, release: 0.003 }, volume: o.volume ?? 0 }, {});
  }
  get type() {
    return this.options.noise.type;
  }
  set type(t: string) {
    this.options.noise.type = t;
  }
  start(time?: Time): this {
    ctx.emit({ n: this.id, t: seconds(time), e: "a", f: [440], v: 1 });
    return this;
  }
  stop(time?: Time): this {
    ctx.emit({ n: this.id, t: seconds(time), e: "ra" });
    return this;
  }
}

// ---- effects and mixing -------------------------------------------------------------------------------------------

/** Every effect and mixing node: its options, Params by Tone's names, wet, and start() for the LFO ones. */
export class Effect extends ToneAudioNode {
  get wet(): Param {
    return new Param(this, ["wet"]);
  }
  start(time?: Time): this {
    this.options.started = seconds(time);
    return this;
  }
  stop(_time?: Time): this {
    return this;
  }
}

function fx(kind: string, defaults: Options, positional: string[] = [], params: string[] = []) {
  const C = class extends Effect {
    constructor(...args: any[]) {
      const o: Options = args.length === 1 && args[0] && typeof args[0] === "object" ? args[0] : Object.fromEntries(positional.map((k, i) => [k, args[i]]).filter(([, v]) => v !== undefined));
      for (const k of ["delayTime", "frequency", "preDelay"]) if (typeof o[k] === "string") o[k] = k === "frequency" ? 1 / seconds(o[k], 0) : seconds(o[k], 0);
      super(kind, defaults, o);
    }
  };
  for (const p of params) Object.defineProperty(C.prototype, p, { get(this: Effect) { return new Param(this, [p]); } });
  return C;
}

export const Reverb = fx("Reverb", { decay: 1.5, preDelay: 0.01, wet: 1 }, ["decay"], ["decay", "preDelay"]);
export const Freeverb = fx("Freeverb", { roomSize: 0.7, dampening: 3000, wet: 1 }, ["roomSize", "dampening"], ["roomSize", "dampening"]);
export const JCReverb = fx("JCReverb", { roomSize: 0.5, wet: 1 }, ["roomSize"], ["roomSize"]);
export const FeedbackDelay = fx("FeedbackDelay", { delayTime: 0.25, feedback: 0.125, maxDelay: 1, wet: 1 }, ["delayTime", "feedback"], ["delayTime", "feedback"]);
export const PingPongDelay = fx("PingPongDelay", { delayTime: 0.25, feedback: 0.125, maxDelay: 1, wet: 1 }, ["delayTime", "feedback"], ["delayTime", "feedback"]);
export const Filter = fx("Filter", { frequency: 350, type: "lowpass", rolloff: -12, Q: 1, gain: 0 }, ["frequency", "type", "rolloff"], ["frequency", "Q", "gain"]);
export const AutoFilter = fx("AutoFilter", { frequency: 1, depth: 1, baseFrequency: 200, octaves: 2.6, filter: { type: "lowpass", rolloff: -12, Q: 1 }, wet: 1 }, ["frequency", "baseFrequency", "octaves"], ["frequency", "depth"]);
export const Compressor = fx("Compressor", { threshold: -24, ratio: 12, attack: 0.003, release: 0.25, knee: 30 }, ["threshold", "ratio"], ["threshold", "ratio", "attack", "release", "knee"]);
export const Limiter = fx("Limiter", { threshold: -12 }, ["threshold"], ["threshold"]);
export const EQ3 = fx("EQ3", { low: 0, mid: 0, high: 0, lowFrequency: 400, highFrequency: 2500 }, ["low", "mid", "high"], ["low", "mid", "high", "lowFrequency", "highFrequency"]);
export const Distortion = fx("Distortion", { distortion: 0.4, oversample: "none", wet: 1 }, ["distortion"]);
export const Chorus = fx("Chorus", { frequency: 1.5, delayTime: 3.5, depth: 0.7, spread: 180, type: "sine", wet: 0.5 }, ["frequency", "delayTime", "depth"], ["frequency", "depth", "spread"]);
export const BitCrusher = fx("BitCrusher", { bits: 4, wet: 1 }, ["bits"], ["bits"]);
export const Tremolo = fx("Tremolo", { frequency: 10, depth: 0.5, spread: 180, type: "sine", wet: 1 }, ["frequency", "depth"], ["frequency", "depth", "spread"]);
export const Vibrato = fx("Vibrato", { frequency: 5, depth: 0.1, type: "sine", wet: 1 }, ["frequency", "depth"], ["frequency", "depth"]);
export const Phaser = fx("Phaser", { frequency: 0.5, octaves: 3, baseFrequency: 350, Q: 10, wet: 1 }, ["frequency", "octaves", "baseFrequency"], ["frequency", "octaves", "baseFrequency", "Q"]);
export const AutoPanner = fx("AutoPanner", { frequency: 1, depth: 1, type: "sine", wet: 1 }, ["frequency"], ["frequency", "depth"]);

// Distortion's amount is its `distortion` property
Object.defineProperty(Distortion.prototype, "distortion", {
  get(this: Effect) {
    return this.options.distortion;
  },
  set(this: Effect, v: number) {
    this.options.distortion = v;
  },
});

export class Gain extends ToneAudioNode {
  constructor(gain: number | Options = 1, _units?: string) {
    super("Gain", { gain: 1 }, typeof gain === "object" ? gain : { gain: _units === "decibels" ? dbToGain(gain) : gain });
  }
  get gain(): Param {
    return new Param(this, ["gain"], "gain");
  }
}

export class Volume extends ToneAudioNode {
  constructor(volume: number | Options = 0) {
    super("Volume", { volume: 0, mute: false }, typeof volume === "object" ? volume : { volume });
  }
  get mute(): boolean {
    return !!this.options.mute;
  }
  set mute(m: boolean) {
    if (ctx.graph === null) this.options.mute = m;
    else ctx.emit({ n: this.id, t: ctx.now, e: "param", p: "mute", val: m ? 1 : 0, ramp: 0 });
  }
}

export class Panner extends ToneAudioNode {
  constructor(pan: number | Options = 0) {
    super("Panner", { pan: 0 }, typeof pan === "object" ? pan : { pan });
  }
  get pan(): Param {
    return new Param(this, ["pan"], "audioRange");
  }
}

const receivers = new Map<string, Channel>();
const pendingSends: [Channel, string, Gain][] = [];

/** Tone's Channel: volume, pan, mute and solo, with sends to named buses. raea: duck() under another channel. */
export class Channel extends ToneAudioNode {
  constructor(volume: number | Options = 0, pan = 0) {
    super("Channel", { volume: 0, pan: 0, mute: false, solo: false }, typeof volume === "object" ? volume : { volume, pan });
  }
  get pan(): Param {
    return new Param(this, ["pan"], "audioRange");
  }
  get mute(): boolean {
    return !!this.options.mute;
  }
  set mute(m: boolean) {
    if (ctx.graph === null) this.options.mute = m;
    else ctx.emit({ n: this.id, t: ctx.now, e: "param", p: "mute", val: m ? 1 : 0, ramp: 0 });
  }
  get muted(): boolean {
    return this.mute;
  }
  get solo(): boolean {
    return !!this.options.solo;
  }
  set solo(s: boolean) {
    this.options.solo = s;
    if (ctx.graph !== null) ctx.graph = null;
  }
  /** a send of this channel's signal, `volume` dB, to the channel that receive()s `name` */
  send(name: string, volume = 0): Gain {
    const g = new Gain(dbToGain(volume));
    this.connect(g);
    const r = receivers.get(name);
    if (r) g.connect(r);
    else pendingSends.push([this, name, g]);
    return g;
  }
  receive(name: string): this {
    receivers.set(name, this);
    for (const [, n, g] of pendingSends.filter((p) => p[1] === name)) g.connect(this);
    return this;
  }
  /** raea: this channel ducked under `key` (music and ambience under voice): `amount` dB when the key is over `threshold`. */
  duck(key: ToneAudioNode, opts: { amount?: number; threshold?: number; attack?: number; release?: number } = {}): this {
    this.options.duck = { amount: -12, threshold: -40, attack: 0.05, release: 0.3, ...opts };
    ctx.edges.push([key.id, this.id, 1]);
    if (ctx.graph !== null) ctx.graph = null;
    return this;
  }
}

export class PanVol extends Channel {}

export class Solo extends ToneAudioNode {
  constructor(solo: boolean | Options = false) {
    super("Solo", { solo: false }, typeof solo === "object" ? solo : { solo });
  }
  get solo(): boolean {
    return !!this.options.solo;
  }
  set solo(s: boolean) {
    this.options.solo = s;
  }
}

/** Analysis nodes have nothing to measure here: they pass sound through and read as silence. */
export class Meter extends ToneAudioNode {
  constructor(_o?: unknown) {
    super("Gain", { gain: 1 });
  }
  getValue() {
    return -Infinity;
  }
}
export class FFT extends Meter {
  getValue() {
    return new Float32Array(1024).fill(-Infinity);
  }
}
export class Waveform extends Meter {
  getValue() {
    return new Float32Array(1024);
  }
}
export const Analyser = FFT;

/** Tone's LFO: a low-frequency oscillator between `min` and `max`. Connected to a Param, its output is the parameter's
 *  value (as in Tone, the parameter's own value is overridden), sample by sample on the core, from start() on. */
export class LFO extends ToneAudioNode {
  constructor(frequency: Time | Options = "4n", min = 0, max = 1) {
    const o = typeof frequency === "object" ? frequency : { frequency, min, max };
    super("LFO", { frequency: 1, min: 0, max: 1, type: "sine", phase: 0, amplitude: 1, started: null }, o);
    if (typeof this.options.frequency === "string") this.options.frequency = 1 / seconds(this.options.frequency, 0);
  }
  get frequency(): Param {
    return new Param(this, ["frequency"], "frequency");
  }
  get min() {
    return this.options.min;
  }
  set min(v: number) {
    this.options.min = v;
  }
  get max() {
    return this.options.max;
  }
  set max(v: number) {
    this.options.max = v;
  }
  get type() {
    return this.options.type;
  }
  set type(t: string) {
    this.options.type = t;
  }
  override connect(dest: ToneAudioNode | Param | AudioParamLike): this {
    if (dest instanceof Param) {
      mods.push([this.id, dest.owner.id, dest.key[dest.key.length - 1]]);
      if (ctx.graph !== null) ctx.graph = null;
      return this;
    }
    return super.connect(dest as ToneAudioNode);
  }
  start(time?: Time): this {
    this.options.started = seconds(time);
    return this;
  }
  stop(_time?: Time): this {
    return this;
  }
  sync(): this {
    return this;
  }
}

/** Tone's Convolver: the sound convolved with an impulse response (a media file or a generated sound). */
export class Convolver extends Effect {
  constructor(url?: string | GeneratedAsset | Options) {
    const o: Options = typeof url === "string" || isGeneratedAsset(url) ? { url } : (url ?? {});
    super("Convolver", { normalize: true, wet: 1 }, { ...o, url: o.url !== undefined ? src(o.url) : undefined });
  }
  load(url: string | GeneratedAsset) {
    this.options.url = src(url);
    return Promise.resolve(this);
  }
}

/** Tone's PitchShift: two delay taps sweeping a window, cross-faded (Tone's own algorithm), `pitch` in semitones. */
export const PitchShift = fx("PitchShift", { pitch: 0, windowSize: 0.1, delayTime: 0, feedback: 0, wet: 1 }, ["pitch"], ["delayTime", "feedback"]);
Object.defineProperty(PitchShift.prototype, "pitch", {
  get(this: Effect) {
    return this.options.pitch;
  },
  set(this: Effect, v: number) {
    if (ctx.graph === null) this.options.pitch = v;
    else ctx.emit({ n: this.id, t: ctx.now, e: "param", p: "pitch", val: v, ramp: 0 });
  },
});

/** Tone.Offline renders a callback's sound into a buffer in the browser's audio engine; there is no audio engine in this
 *  JavaScript (the core renders a composition's sound), so it can't run here. */
export function Offline(): never {
  throw new Error("Tone.Offline: there's no audio engine in raea's JavaScript; render the composition (its sound is the core's bounce) instead");
}

// ---- raea: buses, ducking, generated sources -----------------------------------------------------------------------

const busMap = new Map<string, Channel>();

/** raea: a named bus, a Channel into the Destination that <Audio>/<Video bus="..."> and Tone nodes play into. */
export function bus(name: string): Channel {
  let b = busMap.get(name);
  if (!b) {
    b = new Channel();
    b.busName = name;
    b.toDestination();
    busMap.set(name, b);
  }
  return b;
}

/** raea: the standard buses (music, sfx, voice, ambience, ui). `buses.music.duck(buses.voice)` sidechains the music. */
export const buses = {
  get music() {
    return bus("music");
  },
  get sfx() {
    return bus("sfx");
  },
  get voice() {
    return bus("voice");
  },
  get ambience() {
    return bus("ambience");
  },
  get ui() {
    return bus("ui");
  },
};

/** raea: a TokenSynth instrument (MIDI to a stem by a timbre prompt, rendered locally once and cached as PCM). Use as a
 *  Sampler's urls: `new Tone.Sampler({ urls: tokensynth("a warm upright bass", ["C2", "C3", "C4"]) })`; each root note is
 *  rendered (`seconds` long) and the Sampler pitches between them. */
export function tokensynth(timbre: string, notes: Frequency[] = ["C2", "C3", "C4"], opts: { seconds?: number; velocity?: number; seed?: number; precision?: "fp32" | "fp16" | "bf16" | "int8" | "coreml" } = {}): Record<string, GeneratedAsset> {
  const out: Record<string, GeneratedAsset> = {};
  for (const n of notes) {
    const midi = Math.round(toMidi(n));
    out[String(midi)] = tokensynthNote(timbre, midi, opts);
  }
  return out;
}

export { sao, lyria, fish } from "./assets";

// ---- the Transport and its schedulers -------------------------------------------------------------------------------

/** The Transport's tempo over the timeline: knots of (time s, bpm), linear between them (Tone's bpm.rampTo and
 *  linearRampToValueAtTime; exponential ramps are taken as linear), held before the first and after the last. */
class TempoMap {
  knots: [number, number][] = [];
  constructor(public base: number) {}
  at(t: number): number {
    const k = this.knots;
    if (!k.length || t <= k[0][0]) return k.length ? k[0][1] : this.base;
    for (let i = 1; i < k.length; i++) if (t < k[i][0]) {
      const [t0, b0] = k[i - 1];
      const [t1, b1] = k[i];
      return t1 > t0 ? b0 + (b1 - b0) * ((t - t0) / (t1 - t0)) : b1;
    }
    return k[k.length - 1][1];
  }
  add(t: number, v: number) {
    this.knots.push([t, v]);
    this.knots.sort((a, b) => a[0] - b[0]);
  }
  /** the segments (from, to, bpm at from, bpm at to) from t0 on, the last one open-ended */
  private segs(t0: number): [number, number, number, number][] {
    const ts = [t0, ...this.knots.map((k) => k[0]).filter((t) => t > t0), Infinity];
    const out: [number, number, number, number][] = [];
    for (let i = 0; i + 1 < ts.length; i++) {
      const a = ts[i];
      const b = ts[i + 1];
      out.push([a, b, this.at(a + 1e-12), b === Infinity ? this.at(a + 1e-12) : this.at(b - 1e-12)]);
    }
    return out;
  }
  /** beats from t0 to t */
  beats(t0: number, t: number): number {
    let n = 0;
    for (const [a, b, ba, bb] of this.segs(t0)) {
      if (t <= a) break;
      const e = Math.min(t, b);
      const L = b === Infinity ? 1 : b - a;
      const d = e - a;
      const slope = b === Infinity ? 0 : (bb - ba) / L;
      n += (ba * d + 0.5 * slope * d * d) / 60;
      if (t <= b) break;
    }
    return n;
  }
  /** the time `beats` after t0 */
  timeFor(t0: number, beats: number): number {
    let left = beats;
    for (const [a, b, ba, bb] of this.segs(t0)) {
      const L = b === Infinity ? Infinity : b - a;
      const slope = b === Infinity ? 0 : (bb - ba) / L;
      const segBeats = b === Infinity ? Infinity : (ba * L + 0.5 * slope * L * L) / 60;
      if (left <= segBeats + 1e-12) {
        // solve ba*d + slope*d^2/2 = 60*left for d
        const c = 60 * left;
        const d = Math.abs(slope) < 1e-12 ? c / ba : (-ba + Math.sqrt(Math.max(0, ba * ba + 2 * slope * c))) / slope;
        return a + d;
      }
      left -= segBeats;
    }
    return t0 + (beats * 60) / this.base;
  }
}

/** The Transport's bpm: a Param whose ramps go to the tempo map, so the grid follows them. */
class BpmParam extends Param {
  constructor(owner: ToneAudioNode, private map: TempoMap) {
    super(owner, ["bpm"], "bpm");
  }
  get value(): number {
    return this.map.at(ctx.now);
  }
  set value(v: number) {
    if (ctx.graph === null && ctx.now <= 0 && this.map.knots.length === 0) {
      this.map.base = v;
      this.owner.options.bpm = v;
    } else {
      this.map.add(ctx.now, this.map.at(ctx.now));
      this.map.add(ctx.now + 1e-9, v);
    }
  }
  protected ramp(v: number, over: number, at: number) {
    if (over <= 0 && at <= 0 && this.map.knots.length === 0) {
      this.map.base = v;
      return this;
    }
    const from = this.map.at(at);
    this.map.add(at, from);
    this.map.add(at + Math.max(over, 1e-9), v);
    return this;
  }
}

class TransportClass {
  readonly PPQ = 192;
  readonly bpm: Param;
  readonly tempo = new TempoMap(120);
  private bpmHolder = new (class extends ToneAudioNode {})("TransportBpm", { bpm: 120 });
  timeSignature: number | [number, number] = 4;
  swing = 0;
  swingSubdivision: Time = "8n";
  loop = false;
  loopStart: Time = 0;
  loopEnd: Time = "4m";
  state: "started" | "stopped" | "paused" = "stopped";
  startTime = 0;
  offset = 0;
  /** schedule() calls waiting for start(), in transport time */
  private waiting: { at: number; seq: number; fn: (time: number) => void; repeat?: number; until?: number }[] = [];
  private ids = 0;
  private cancelled = new Set<number>();
  constructor() {
    ctx.nodes.delete(this.bpmHolder.id);
    this.bpm = new BpmParam(this.bpmHolder, this.tempo);
  }
  ts(): [number, number] {
    return Array.isArray(this.timeSignature) ? this.timeSignature : [this.timeSignature, 4];
  }
  get seconds() {
    return Math.max(0, ctx.now - this.startTime + this.offset);
  }
  set seconds(s: number) {
    this.offset = s;
  }
  get position(): string {
    return position(this.seconds);
  }
  set position(p: string | number) {
    this.offset = seconds(p, 0);
  }
  get ticks() {
    return Math.round((this.seconds / (60 / this.bpm.value)) * this.PPQ);
  }
  get progress() {
    return 0;
  }
  private swung(t: number): number {
    if (!this.swing) return t;
    const sub = seconds(this.swingSubdivision, 0);
    const k = t / sub;
    const odd = Math.abs(k - Math.round(k)) < 1e-6 && Math.round(k) % 2 === 1;
    return odd ? t + this.swing * sub * 0.5 : t;
  }
  private abs(at: number) {
    const x = this.swung(at) - this.offset;
    if (this.tempo.knots.length === 0) return this.startTime + x;
    // transport seconds are counted at the starting tempo: beats, then the time those beats take on the tempo map
    const b0 = this.tempo.at(this.startTime);
    return this.tempo.timeFor(this.startTime, (x * b0) / 60);
  }
  private enqueue(w: { at: number; seq: number; fn: (time: number) => void; repeat?: number; until?: number }) {
    if (this.loop) return this.enqueueLooped(w);
    const id = w.seq;
    const fire = (at: number) => {
      if (this.cancelled.has(id)) return;
      ctx.schedule({
        time: this.abs(at),
        run: (time) => {
          if (this.cancelled.has(id) || this.state !== "started") return;
          w.fn(time);
          if (w.repeat && w.repeat > 0) {
            const next = at + w.repeat;
            if (w.until === undefined || next < w.until) fire(next);
          }
        },
      });
    };
    if (w.at >= this.offset || w.repeat) {
      let at = w.at;
      if (w.repeat && at < this.offset) at += Math.ceil((this.offset - at) / w.repeat - 1e-9) * w.repeat;
      fire(at);
    }
  }
  /** Tone's loop: the transport goes back to loopStart at loopEnd. An event inside the window fires on every pass; one
   *  before it, once; one past its end, never. A repeating event fires at its places inside the window. */
  private enqueueLooped(w: { at: number; seq: number; fn: (time: number) => void; repeat?: number; until?: number }) {
    const id = w.seq;
    const ls = seconds(this.loopStart, 0);
    const le = seconds(this.loopEnd, 0);
    const len = le - ls;
    if (!(len > 0)) return;
    const places: number[] = [];
    if (w.repeat && w.repeat > 0) {
      for (let x = w.at; x < le && (w.until === undefined || x < w.until) && places.length < 100000; x += w.repeat) places.push(x);
    } else places.push(w.at);
    for (const x of places) {
      if (x >= le) continue;
      const fire = (pass: number) => {
        if (this.cancelled.has(id)) return;
        ctx.schedule({
          time: this.abs(x + pass * len),
          run: (time) => {
            if (this.cancelled.has(id) || this.state !== "started") return;
            w.fn(time);
            if (x >= ls && this.loop) fire(pass + 1);
          },
        });
      };
      fire(0);
    }
  }
  schedule(fn: (time: number) => void, time: Time): number {
    const w = { at: seconds(time, 0), seq: ++this.ids, fn };
    if (this.state === "started") this.enqueue(w);
    else this.waiting.push(w);
    return w.seq;
  }
  scheduleOnce(fn: (time: number) => void, time: Time): number {
    return this.schedule(fn, time);
  }
  scheduleRepeat(fn: (time: number) => void, interval: Time, startTime: Time = 0, duration?: Time): number {
    const at = seconds(startTime, 0);
    const w = { at, seq: ++this.ids, fn, repeat: seconds(interval, 0), until: duration !== undefined ? at + seconds(duration, 0) : undefined };
    if (this.state === "started") this.enqueue(w);
    else this.waiting.push(w);
    return w.seq;
  }
  clear(id: number) {
    this.cancelled.add(id);
    return this;
  }
  cancel(_after: Time = 0) {
    for (const w of this.waiting) this.cancelled.add(w.seq);
    for (let i = 1; i <= this.ids; i++) this.cancelled.add(i);
    return this;
  }
  start(time?: Time, offset?: Time) {
    this.startTime = seconds(time);
    if (offset !== undefined) this.offset = seconds(offset, 0);
    this.state = "started";
    for (const w of this.waiting.splice(0)) this.enqueue(w);
    return this;
  }
  stop(time?: Time) {
    const at = seconds(time);
    ctx.schedule({ time: at, run: () => (this.state = "stopped") });
    return this;
  }
  pause(time?: Time) {
    return this.stop(time);
  }
  toggle(time?: Time) {
    return this.state === "started" ? this.stop(time) : this.start(time);
  }
  setLoopPoints(start: Time, end: Time) {
    this.loopStart = start;
    this.loopEnd = end;
    return this;
  }
  /** raea: frames at `fps` for a transport time (cuts on bars and beats) */
  toFrames(t: Time, fps = 30) {
    return Math.round(seconds(t, 0) * fps);
  }
  nextSubdivision(sub: Time) {
    const s = seconds(sub, 0);
    return this.startTime + Math.ceil(this.seconds / s - 1e-9) * s;
  }
  getTicksAtTime(t: Time) {
    return Math.round(((seconds(t) - this.startTime) / (60 / this.bpm.value)) * this.PPQ);
  }
  getSecondsAtTime(t: Time) {
    return seconds(t) - this.startTime;
  }
  dispose() {
    return this;
  }
}

const transport = new TransportClass();
export function getTransport() {
  return transport;
}
export const Transport = transport;
export const Draw = { schedule(_cb: () => void, _time: Time) { return this; }, cancel() { return this; }, anticipation: 0.008, expiration: 0.25 };

/** A seeded random (mulberry32): Pattern's random walks and probability are the same every run. */
function rng(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

class Event<V = any> {
  probability = 1;
  humanize: boolean | Time = false;
  mute = false;
  playbackRate = 1;
  iterations = Infinity;
  protected ids: number[] = [];
  protected rand = rng(0x5eed + ctx.nextId);
  protected fire(cb: (time: number, v: V) => void, time: number, v: V) {
    if (this.mute) return;
    if (this.probability < 1 && this.rand() >= this.probability) return;
    cb(time, v);
  }
  stop(time?: Time) {
    const at = time === undefined ? 0 : seconds(time, 0);
    const ids = this.ids.splice(0);
    transport.schedule(() => ids.forEach((id) => transport.clear(id)), at);
    return this;
  }
  cancel() {
    this.ids.splice(0).forEach((id) => transport.clear(id));
    return this;
  }
  dispose() {
    return this.cancel();
  }
  get state() {
    return this.ids.length ? "started" : "stopped";
  }
}

/** Tone's Loop: the callback every `interval` from its start, in transport time. */
export class Loop extends Event<undefined> {
  interval: Time;
  constructor(private cb: ((time: number) => void) | { callback: (time: number) => void; interval?: Time }, interval: Time = "4n") {
    super();
    if (typeof cb === "object") {
      this.interval = cb.interval ?? interval;
      this.cb = cb.callback;
    } else this.interval = interval;
  }
  start(time: Time = 0) {
    let n = 0;
    const at = seconds(time, 0);
    const iv = seconds(this.interval, 0) / this.playbackRate;
    this.ids.push(transport.scheduleRepeat((t) => {
      if (n++ >= this.iterations) return;
      this.fire(this.cb as (time: number) => void, t, undefined);
    }, iv, at));
    return this;
  }
}

type PartEvent<V> = [Time, V] | (V & { time: Time });

/** Tone's Part: values at times, the callback at each, looped when asked. */
export class Part<V = any> extends Event<V> {
  loop: boolean | number = false;
  loopStart: Time = 0;
  loopEnd: Time = "1m";
  private evs: [number, V][] = [];
  constructor(private cb: (time: number, value: V) => void, events: PartEvent<V>[] = []) {
    super();
    for (const e of events) this.add(e as any);
  }
  add(time: Time | PartEvent<V>, value?: V) {
    if (Array.isArray(time)) this.evs.push([seconds(time[0], 0), time[1]]);
    else if (typeof time === "object" && time !== null && "time" in (time as any)) this.evs.push([seconds((time as any).time, 0), time as V]);
    else this.evs.push([seconds(time as Time, 0), value as V]);
    return this;
  }
  remove(time: Time) {
    const t = seconds(time, 0);
    this.evs = this.evs.filter(([x]) => Math.abs(x - t) > 1e-9);
    return this;
  }
  clear() {
    this.evs = [];
    return this;
  }
  at(time: Time, value?: V) {
    if (value !== undefined) this.add(time, value);
    return this.evs.find(([x]) => Math.abs(x - seconds(time, 0)) < 1e-9)?.[1];
  }
  get length() {
    return this.evs.length;
  }
  start(time: Time = 0, offset: Time = 0) {
    const at = seconds(time, 0);
    const off = seconds(offset, 0);
    if (this.loop) {
      const ls = seconds(this.loopStart, 0);
      const le = seconds(this.loopEnd, 0);
      const len = le - ls;
      const times = typeof this.loop === "number" ? this.loop : Infinity;
      for (const [t, v] of this.evs) {
        if (t < ls || t >= le) continue;
        let count = 0;
        this.ids.push(transport.scheduleRepeat((tt) => {
          if (count++ >= times) return;
          this.fire(this.cb, tt, v);
        }, len / this.playbackRate, at + (t - ls - off) / this.playbackRate));
      }
    } else {
      for (const [t, v] of this.evs) if (t >= off) this.ids.push(transport.schedule((tt) => this.fire(this.cb, tt, v), at + (t - off) / this.playbackRate));
    }
    return this;
  }
}

type Seq<V> = (V | null | Seq<V>)[];

/** Tone's Sequence: one value per subdivision (null rests; a nested array splits its step), looped by default. */
export class Sequence<V = any> extends Event<V> {
  loop: boolean | number = true;
  subdivision: Time;
  private evs: Seq<V>;
  loopStart = 0;
  loopEnd = 0;
  constructor(private cb: (time: number, value: V) => void, events: Seq<V> = [], subdivision: Time = "8n") {
    super();
    if (typeof cb === "object" && cb !== null) {
      const o = cb as any;
      this.cb = o.callback;
      events = o.events ?? [];
      subdivision = o.subdivision ?? "8n";
      if (o.loop !== undefined) this.loop = o.loop;
    }
    this.evs = events;
    this.subdivision = subdivision;
    this.loopEnd = events.length;
  }
  get events() {
    return this.evs;
  }
  set events(e: Seq<V>) {
    this.evs = e;
    this.loopEnd = e.length;
  }
  start(time: Time = 0, offset = 0) {
    const sub = seconds(this.subdivision, 0) / this.playbackRate;
    const at = seconds(time, 0);
    const flat: [number, V][] = [];
    const walk = (list: Seq<V>, start: number, span: number) => {
      const w = span / list.length;
      list.forEach((x, i) => {
        if (Array.isArray(x)) walk(x as Seq<V>, start + i * w, w);
        else if (x !== null && x !== undefined) flat.push([start + i * w, x as V]);
      });
    };
    walk(this.evs.slice(this.loopStart, this.loopEnd || this.evs.length), 0, (this.loopEnd || this.evs.length) - this.loopStart);
    const len = ((this.loopEnd || this.evs.length) - this.loopStart) * sub;
    const times = this.loop === true ? Infinity : typeof this.loop === "number" ? this.loop : 1;
    for (const [steps, v] of flat) {
      const t = steps * sub - offset * sub;
      if (t < 0 && times === 1) continue;
      let count = 0;
      if (times === 1) this.ids.push(transport.schedule((tt) => this.fire(this.cb, tt, v), at + t));
      else
        this.ids.push(transport.scheduleRepeat((tt) => {
          if (count++ >= times) return;
          this.fire(this.cb, tt, v);
        }, len, at + t));
    }
    return this;
  }
}

/** Tone's Pattern: the values in an order ("up", "down", "upDown", "downUp", "alternateUp", "alternateDown", "random",
 *  "randomOnce", "randomWalk"), one per interval. Random orders are seeded: the same every run. */
export class Pattern<V = any> extends Event<V> {
  interval: Time = "4n";
  private i = 0;
  private dir = 1;
  private order: number[] = [];
  constructor(private cb: (time: number, value: V) => void, public values: V[] = [], public pattern = "up") {
    super();
    if (typeof cb === "object" && cb !== null) {
      const o = cb as any;
      this.cb = o.callback;
      this.values = o.values ?? [];
      this.pattern = o.pattern ?? "up";
      if (o.interval) this.interval = o.interval;
    }
    this.order = this.values.map((_, k) => k);
    if (this.pattern === "randomOnce") for (let k = this.order.length - 1; k > 0; k--) { const j = Math.floor(this.rand() * (k + 1)); [this.order[k], this.order[j]] = [this.order[j], this.order[k]]; }
    if (this.pattern.startsWith("down")) this.i = this.values.length - 1;
  }
  private next(): V {
    const n = this.values.length;
    const p = this.pattern;
    let idx = this.i;
    if (p === "random") idx = Math.floor(this.rand() * n);
    else if (p === "randomOnce") idx = this.order[this.i % n];
    else if (p === "randomWalk") {
      idx = this.i;
      this.i = Math.max(0, Math.min(n - 1, this.i + (this.rand() < 0.5 ? -1 : 1)));
      return this.values[idx];
    }
    const v = this.values[idx];
    if (p === "up" || p === "randomOnce") this.i = (this.i + 1) % n;
    else if (p === "down") this.i = (this.i - 1 + n) % n;
    else if (p === "upDown" || p === "downUp") {
      if (n > 1) {
        if (this.i + this.dir >= n || this.i + this.dir < 0) this.dir = -this.dir;
        this.i += this.dir;
      }
    } else if (p === "alternateUp") this.i = (this.i + (this.dir > 0 ? 2 : -1) + n) % n, (this.dir = -this.dir);
    else if (p === "alternateDown") this.i = (this.i + (this.dir > 0 ? -2 : 1) + n) % n, (this.dir = -this.dir);
    return v;
  }
  start(time: Time = 0) {
    this.ids.push(transport.scheduleRepeat((t) => this.fire(this.cb, t, this.next()), seconds(this.interval, 0) / this.playbackRate, seconds(time, 0)));
    return this;
  }
}

// ---- in a composition ----------------------------------------------------------------------------------------------

/** How long a shown event stays shown (frames): a play that skips a frame still meets it (then it starts late). */
const SHOW = 3;

/**
 * The edit's Tone, for the frame: its graph (the same every frame) and the events whose time falls in the last few
 * frames. raea's registry renders it inside every composition once this module is imported; render it yourself only
 * outside one.
 */
export function ToneOut(): ReactNode {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  if (ctx.nodes.size === 0 && ctx.events.length === 0 && ctx.pending.length === 0) return null;
  const t0 = (frame - SHOW + 1) / fps;
  const t1 = (frame + 1) / fps;
  // the Transport's callbacks run up to a little past this frame, in time order (the same whatever frames ask)
  ctx.compileTo(t1 + 0.5);
  const graph = ctx.graphJson();
  const evs = ctx.window(t0, t1);
  return (
    <>
      <audio data-tone-graph={graph} />
      {evs.length > 0 && <audio data-tone-evs={JSON.stringify(evs)} />}
    </>
  );
}

(globalThis as any).__raeaToneOut = ToneOut;

/**
 * raea: a game's one-shot. When `when` is true on a frame (a state machine's transition, a game event), `play(time)`
 * runs with that frame's time and its triggers sound on that exact frame, keyed by the frame, so a replay plays the
 * same. Seeded variation comes from the instrument's `variation` option; polyphony and voice stealing from its
 * maxPolyphony.
 *   <Sfx when={hit} play={(t) => sfx.triggerAttackRelease("C4", "16n", t)} />
 * Its events are keyed by `name` (else a hash of its play code) and the frame: give two <Sfx> with the same code names.
 */
export function Sfx({ when, play, name }: { when: boolean | number | null | undefined; play: (time: number, frame: number) => void; name?: string }): ReactNode {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  // its key: the name, else its play code (React's useId and live play's runtime number elements differently, and the
  // key seeds the variation). Two <Sfx> with the same code on one frame need names.
  const id = name ?? codeKey(play);
  if (!when) return null;
  const evs: Ev[] = [];
  const prev = [ctx.sink, ctx.sinkKey, ctx.seq, ctx.now] as const;
  ctx.sink = evs;
  ctx.sinkKey = `sfx${id}:${frame}`;
  ctx.seq = 0;
  ctx.now = frame / fps;
  try {
    play(frame / fps, frame);
  } finally {
    [ctx.sink, ctx.sinkKey, ctx.seq, ctx.now] = prev;
  }
  return evs.length ? <audio data-tone-evs={JSON.stringify(evs)} /> : null;
}

function codeKey(f: Function): string {
  let h = 0x811c9dc5;
  const src = f.toString();
  for (let i = 0; i < src.length; i++) h = Math.imul(h ^ src.charCodeAt(i), 0x01000193) >>> 0;
  return h.toString(36);
}

/** raea: an instrument's seeded variation (each event's own: pitch jitter in semitones, volume jitter in dB). */
export function vary<T extends Instrument>(i: T, v: { pitch?: number; volume?: number }): T {
  i.options.variation = { pitch: v.pitch ?? 0, volume: v.volume ?? 0 };
  return i;
}

/** Testing: the edit's Tone so far (graph JSON and events up to `until` seconds). */
export function snapshot(until = 10): { graph: any; events: Ev[] } {
  ctx.compileTo(until);
  return { graph: JSON.parse(ctx.graphJson()), events: ctx.window(-Infinity, until) };
}

/** Testing: a fresh context (one edit per process otherwise). */
export function reset() {
  const fresh = new Context();
  Object.assign(ctx, fresh);
  ctx.nodes = new Map();
  ctx.edges = [];
  ctx.events = [];
  ctx.pending = [];
  destination = null;
  busMap.clear();
  receivers.clear();
  pendingSends.length = 0;
  mods = [];
  transport.tempo.knots = [];
  transport.tempo.base = 120;
  Object.assign(transport, { timeSignature: 4, swing: 0, loop: false, state: "stopped", startTime: 0, offset: 0 });
  (transport as any).waiting = [];
  (transport as any).ids = 0;
  (transport as any).cancelled = new Set();
  transport.bpm.value = 120;
}
