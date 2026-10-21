// @raea/motion/music: a shorthand for notes on named instruments (a Transport with a tempo and time signature, Parts and
// Sequences of notes, MIDI in and out), played through @raea/motion/tone, the one music API: each instrument name is a
// Tone instrument (kick a MembraneSynth, snare a NoiseSynth, hat a MetalSynth, bass a MonoSynth, pad a PolySynth of fat
// saws, pluck a PluckSynth, sine/triangle/square/saw a PolySynth), and each note a Tone event on its own sample. Write
// Tone itself for anything more (effects, channels, the Transport's callbacks); this module's notes mix with it.
//
//   const t = new Transport({ bpm: 120, timeSignature: [4, 4] });
//   const drums = new Sequence("kick", ["C1", null, "C1", null], "4n", { loop: true });
//   const bass = new Part("bass", [{ time: "0:0:0", note: "E2", duration: "8n" }, ...], { loop: "1m" });
//   <Music transport={t} parts={[drums, bass]} />

import { useCurrentFrame, useVideoConfig } from "./clock";
import * as Tone from "./tone";

/** A time as Tone writes one: seconds (a number), "bars:beats:sixteenths", or a note value ("4n", "8t", "2n.", "1m"). */
export type Time = number | string;

export const INSTRUMENTS = ["sine", "triangle", "square", "saw", "bass", "pad", "pluck", "kick", "snare", "hat", "open-hat", "clap"] as const;
export type Instrument = (typeof INSTRUMENTS)[number];

export class Transport {
  bpm: number;
  timeSignature: [number, number];
  /** ticks per quarter note, for MIDI out */
  ppq: number;

  constructor(opts: { bpm?: number; timeSignature?: [number, number] | number; ppq?: number } = {}) {
    this.bpm = opts.bpm ?? 120;
    const ts = opts.timeSignature ?? [4, 4];
    this.timeSignature = typeof ts === "number" ? [ts, 4] : ts;
    this.ppq = opts.ppq ?? 480;
    if (!(this.bpm > 0)) throw new RangeError(`Transport: bpm must be positive, got ${this.bpm}`);
  }

  /** seconds in a quarter note */
  get beat(): number {
    return 60 / this.bpm;
  }

  /** seconds in a bar */
  get bar(): number {
    const [n, d] = this.timeSignature;
    return n * this.beat * (4 / d);
  }

  /** A Time in seconds. */
  toSeconds(t: Time): number {
    if (typeof t === "number") return t;
    const s = t.trim();
    let m = /^(-?\d+(?:\.\d+)?):(\d+(?:\.\d+)?)(?::(\d+(?:\.\d+)?))?$/.exec(s);
    if (m) return Number(m[1]) * this.bar + Number(m[2]) * this.beat * (4 / this.timeSignature[1]) + Number(m[3] ?? 0) * this.beat / 4;
    m = /^(\d+(?:\.\d+)?)(n|t|m)(\.?)$/.exec(s);
    if (m) {
      const v = Number(m[1]);
      let secs = m[2] === "m" ? v * this.bar : (4 / v) * this.beat;
      if (m[2] === "t") secs *= 2 / 3;
      if (m[3] === ".") secs *= 1.5;
      return secs;
    }
    if (/^-?\d+(\.\d+)?s?$/.test(s)) return parseFloat(s);
    throw new RangeError(`Transport: "${t}" isn't a time (seconds, "bars:beats:sixteenths" or a note value like "8n")`);
  }

  /** A time in seconds as MIDI ticks. */
  toTicks(seconds: number): number {
    return Math.round((seconds / this.beat) * this.ppq);
  }
}

const NAMES: Record<string, number> = { c: 0, d: 2, e: 4, f: 5, g: 7, a: 9, b: 11 };

/** A note name ("C4", "F#2", "Bb3") or MIDI number as a MIDI number (A4 = 69, C4 = 60). */
export function midiOf(note: string | number): number {
  if (typeof note === "number") return note;
  const m = /^([A-Ga-g])(#{1,2}|b{1,2})?(-?\d+)$/.exec(note.trim());
  if (!m) throw new RangeError(`"${note}" isn't a note (like "C4", "F#2" or a MIDI number)`);
  const acc = m[2] ? (m[2][0] === "#" ? m[2].length : -m[2].length) : 0;
  return 12 * (Number(m[3]) + 1) + NAMES[m[1].toLowerCase()] + acc;
}

export type NoteEvent = { time: Time; note: string | number | (string | number)[]; duration?: Time; velocity?: number };

export type PartOptions = {
  /** loop the part: true (over its own length rounded up to a bar) or the loop's length as a Time */
  loop?: boolean | Time;
  /** where the part starts on the transport */
  start?: Time;
  /** where it stops (default: the composition's end) */
  end?: Time;
  /** gain in dB, pan -1 (left) .. 1 (right) */
  gainDb?: number;
  pan?: number;
  name?: string;
};

/** One note as the mixer core plays it. */
export type NoteRow = { key: string; instrument: Instrument; midi: number; velocity: number; start: number; duration: number; gainDb: number; pan: number };

function checkInstrument(owner: string, inst: string): Instrument {
  if (!(INSTRUMENTS as readonly string[]).includes(inst)) throw new RangeError(`${owner}: "${inst}" isn't an instrument (${INSTRUMENTS.join(", ")})`);
  return inst as Instrument;
}

/** Notes at times on one instrument, as Tone's Part. */
export class Part {
  readonly instrument: Instrument;
  readonly events: NoteEvent[];
  readonly options: PartOptions;

  constructor(instrument: Instrument | string, events: NoteEvent[], options: PartOptions = {}) {
    this.instrument = checkInstrument("Part", instrument);
    this.events = events;
    this.options = options;
  }

  /**
   * The notes up to `until` seconds, keyed `${name}:${loop}:${index}:${chord note}`; with a window, only those sounding in
   * [from, to) (a loop's passes outside it aren't visited: a live composition is a billion frames long).
   */
  rows(t: Transport, until: number, name: string, from = -Infinity, to = Infinity): NoteRow[] {
    const o = this.options;
    const start = t.toSeconds(o.start ?? 0);
    const end = Math.min(until, o.end !== undefined ? t.toSeconds(o.end) : Infinity);
    const evs = this.events.map((e) => ({ at: t.toSeconds(e.time), dur: t.toSeconds(e.duration ?? "8n"), notes: Array.isArray(e.note) ? e.note : [e.note], v: e.velocity ?? 0.8 }));
    const own = Math.max(0, ...evs.map((e) => e.at + e.dur));
    const length = o.loop === true ? Math.max(t.bar, Math.ceil(own / t.bar - 1e-9) * t.bar) : o.loop ? t.toSeconds(o.loop) : Infinity;
    const out: NoteRow[] = [];
    const reach = Math.max(own, ...evs.map((e) => e.dur));
    const first = Number.isFinite(length) && Number.isFinite(from) ? Math.max(0, Math.floor((from - start - reach) / length)) : 0;
    const stop = Math.min(end, to);
    for (let k = first; ; k++) {
      const base = start + k * (Number.isFinite(length) ? length : 0);
      if (base >= stop || (k > first && !Number.isFinite(length))) break;
      evs.forEach((e, i) => {
        if (Number.isFinite(length) && e.at >= length) return;
        const at = base + e.at;
        if (at >= end) return;
        if (at >= to || at + e.dur < from) return;
        e.notes.forEach((n, j) => {
          out.push({ key: `${name}:${k}:${i}:${j}`, instrument: this.instrument, midi: midiOf(n), velocity: e.v, start: at, duration: Math.min(e.dur, end - at), gainDb: o.gainDb ?? 0, pan: o.pan ?? 0 });
        });
      });
      if (!Number.isFinite(length)) break;
    }
    return out;
  }

  /** A MIDI file's track as a Part (notes at their seconds; the tempo map already applied). */
  static fromMidiTrack(instrument: Instrument | string, track: MidiTrack, options: PartOptions = {}): Part {
    return new Part(instrument, track.notes.map((n) => ({ time: n.time, note: n.midi, duration: n.duration, velocity: n.velocity })), options);
  }
}

export type SequenceStep = string | number | null | SequenceStep[];

/** Notes one per `subdivision` (null rests; a nested array splits its step evenly), as Tone's Sequence. */
export class Sequence extends Part {
  readonly steps: SequenceStep[];
  readonly subdivision: Time;
  readonly velocity: number;
  readonly duration?: Time;

  constructor(instrument: Instrument | string, steps: SequenceStep[], subdivision: Time = "8n", options: PartOptions & { velocity?: number; duration?: Time } = {}) {
    super(instrument, [], options);
    this.steps = steps;
    this.subdivision = subdivision;
    this.velocity = options.velocity ?? 0.8;
    this.duration = options.duration;
  }

  rows(t: Transport, until: number, name: string, from = -Infinity, to = Infinity): NoteRow[] {
    const step = t.toSeconds(this.subdivision);
    const events: NoteEvent[] = [];
    const walk = (list: SequenceStep[], at: number, span: number) => {
      const w = span / list.length;
      list.forEach((s, i) => {
        if (Array.isArray(s)) walk(s, at + i * w, w);
        else if (s !== null) events.push({ time: at + i * w, note: s, duration: this.duration ?? w * 0.9, velocity: this.velocity });
      });
    };
    walk(this.steps, 0, this.steps.length * step);
    const loop = this.options.loop === true ? this.steps.length * step : this.options.loop;
    return new Part(this.instrument, events, { ...this.options, loop }).rows(t, until, name, from, to);
  }
}

/** Every part's notes to `until` seconds, in start order. */
export function compile(t: Transport, parts: Part[], until: number): NoteRow[] {
  const rows = parts.flatMap((p, i) => p.rows(t, until, p.options.name ?? `p${i}`));
  return rows.sort((a, b) => a.start - b.start || (a.key < b.key ? -1 : 1));
}

export type MusicProps = { transport: Transport; parts: Part[]; gainDb?: number };

/**
 * The parts, played: each note shown as an <audio data-note> from the frame its start falls in until it ends, which the
 * mixer core starts once, on its own sample. Nothing to see.
 */
export function Music({ transport, parts, gainDb = 0 }: MusicProps) {
  const frame = useCurrentFrame();
  const { fps, durationInFrames } = useVideoConfig();
  const until = durationInFrames / fps;
  const t0 = frame / fps;
  // the notes from this frame to half a second ahead, each a Tone event once (keyed by the note): a play that skips a
  // frame still has them, and a render makes the same list in the same order
  const t1 = (frame + 1) / fps + 0.5;
  const rows = parts.flatMap((p, i) => p.rows(transport, until, p.options.name ?? `p${i}`, t0, t1)).filter((r) => r.start >= t0 - 1 / fps && r.start < t1);
  for (const r of rows) {
    if (emitted.has(r.key)) continue;
    emitted.add(r.key);
    const inst = preset(r.instrument);
    const ctx = Tone.getContext();
    ctx.emit({ k: `music:${r.key}`, n: inst.id, t: r.start, e: "ar", f: [Tone.mtof(r.midi)], d: r.duration, v: r.velocity * Tone.dbToGain(r.gainDb + gainDb) });
  }
  return null;
}

const emitted = new Set<string>();
const presets = new Map<string, Tone.Instrument>();

/** Each instrument name's Tone instrument, made once and sent to the Destination. */
function preset(name: Instrument): Tone.Instrument {
  let i = presets.get(name);
  if (i) return i;
  switch (name) {
    case "kick":
      i = new Tone.MembraneSynth({ pitchDecay: 0.05, octaves: 6, envelope: { attack: 0.001, decay: 0.38, sustain: 0, release: 0.05 }, volume: -2, poly: true });
      break;
    case "snare":
      i = new Tone.NoiseSynth({ noise: { type: "white" }, envelope: { attack: 0.001, decay: 0.17, sustain: 0, release: 0.03 }, volume: -10, poly: true });
      break;
    case "clap":
      i = new Tone.NoiseSynth({ noise: { type: "pink" }, envelope: { attack: 0.001, decay: 0.14, sustain: 0, release: 0.03 }, volume: -10, poly: true });
      break;
    case "hat":
      i = new Tone.MetalSynth({ frequency: 400, resonance: 7000, envelope: { attack: 0.001, decay: 0.05, sustain: 0, release: 0.01 }, volume: -22, poly: true });
      break;
    case "open-hat":
      i = new Tone.MetalSynth({ frequency: 400, resonance: 7000, envelope: { attack: 0.001, decay: 0.28, sustain: 0, release: 0.05 }, volume: -24, poly: true });
      break;
    case "bass":
      i = new Tone.MonoSynth({ oscillator: { type: "sawtooth" }, filter: { Q: 2, type: "lowpass", rolloff: -24 }, filterEnvelope: { attack: 0.005, decay: 0.2, sustain: 0.3, release: 0.1, baseFrequency: 120, octaves: 3 }, envelope: { attack: 0.005, decay: 0.2, sustain: 0.5, release: 0.1 }, volume: -8 });
      break;
    case "pad":
      i = new Tone.PolySynth(Tone.Synth, { oscillator: { type: "fatsawtooth", count: 3, spread: 20 }, envelope: { attack: 0.12, decay: 0.4, sustain: 0.7, release: 0.35 }, volume: -18 });
      break;
    case "pluck":
      i = new Tone.PluckSynth({ volume: -6, poly: true });
      break;
    default:
      i = new Tone.PolySynth(Tone.Synth, { oscillator: { type: name === "saw" ? "sawtooth" : name }, volume: -10 });
  }
  i.toDestination();
  presets.set(name, i);
  return i;
}

// ---- MIDI: Standard MIDI Files in and out, in @tonejs/midi's JSON shape (tracks[].notes[] with midi, time, duration and
// velocity in seconds and 0-1; header.tempos, timeSignatures, ppq), with no dependency.

export type MidiNote = { midi: number; time: number; duration: number; velocity: number; ticks: number; durationTicks: number };
export type MidiTrack = { name: string; channel: number; notes: MidiNote[] };
export type MidiFile = { header: { ppq: number; tempos: { ticks: number; bpm: number }[]; timeSignatures: { ticks: number; timeSignature: [number, number] }[]; name: string }; tracks: MidiTrack[]; duration: number };

/** Base64 to bytes (a .mid file written into a composition as a string). */
export function bytesOfBase64(b64: string): Uint8Array {
  const abc = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  const clean = b64.replace(/[^A-Za-z0-9+/]/g, "");
  const out: number[] = [];
  for (let i = 0; i < clean.length; i += 4) {
    const n = [0, 1, 2, 3].map((k) => abc.indexOf(clean[i + k] ?? "A"));
    const v = (n[0] << 18) | (n[1] << 12) | ((n[2] & 63) << 6) | (n[3] & 63);
    out.push((v >> 16) & 255);
    if (clean[i + 2] !== undefined) out.push((v >> 8) & 255);
    if (clean[i + 3] !== undefined) out.push(v & 255);
  }
  return new Uint8Array(out);
}

/** A Standard MIDI File read (format 0 or 1): notes in seconds through its tempo map. */
export function parseMidi(bytes: Uint8Array): MidiFile {
  let p = 0;
  const u32 = () => ((bytes[p++] << 24) | (bytes[p++] << 16) | (bytes[p++] << 8) | bytes[p++]) >>> 0;
  const u16 = () => (bytes[p++] << 8) | bytes[p++];
  const vlq = () => {
    let v = 0;
    for (;;) {
      const b = bytes[p++];
      v = (v << 7) | (b & 127);
      if (!(b & 128)) return v;
    }
  };
  const tag = () => String.fromCharCode(bytes[p], bytes[p + 1], bytes[p + 2], bytes[p + 3]);
  if (tag() !== "MThd") throw new Error("parseMidi: not a MIDI file");
  p += 4;
  const hlen = u32();
  const hstart = p;
  u16();
  const ntracks = u16();
  const ppq = u16();
  p = hstart + hlen;
  const tempos: { ticks: number; bpm: number }[] = [];
  const sigs: { ticks: number; timeSignature: [number, number] }[] = [];
  const raw: { name: string; channel: number; notes: { midi: number; ticks: number; durationTicks: number; velocity: number }[] }[] = [];
  let name = "";
  for (let k = 0; k < ntracks && p < bytes.length; k++) {
    if (tag() !== "MTrk") throw new Error("parseMidi: a track without MTrk");
    p += 4;
    const len = u32();
    const end = p + len;
    let ticks = 0;
    let status = 0;
    const open = new Map<number, { ticks: number; velocity: number }[]>();
    const track = { name: "", channel: 0, notes: [] as { midi: number; ticks: number; durationTicks: number; velocity: number }[] };
    while (p < end) {
      ticks += vlq();
      let b = bytes[p];
      if (b & 128) {
        status = b;
        p++;
      }
      b = status;
      if (b === 0xff) {
        const type = bytes[p++];
        const l = vlq();
        const data = bytes.subarray(p, p + l);
        p += l;
        if (type === 0x51) tempos.push({ ticks, bpm: 60_000_000 / ((data[0] << 16) | (data[1] << 8) | data[2]) });
        else if (type === 0x58) sigs.push({ ticks, timeSignature: [data[0], 2 ** data[1]] });
        else if (type === 0x03) {
          track.name = String.fromCharCode(...data);
          if (k === 0) name = track.name;
        }
      } else if (b === 0xf0 || b === 0xf7) {
        p += vlq();
      } else {
        const kind = b & 0xf0;
        const ch = b & 15;
        const d1 = bytes[p++];
        const d2 = kind === 0xc0 || kind === 0xd0 ? 0 : bytes[p++];
        if (kind === 0x90 && d2 > 0) {
          track.channel = ch;
          (open.get(d1) ?? open.set(d1, []).get(d1)!).push({ ticks, velocity: d2 / 127 });
        } else if (kind === 0x80 || (kind === 0x90 && d2 === 0)) {
          const on = open.get(d1)?.shift();
          if (on) track.notes.push({ midi: d1, ticks: on.ticks, durationTicks: ticks - on.ticks, velocity: on.velocity });
        }
      }
    }
    p = end;
    raw.push(track);
  }
  if (!tempos.length) tempos.push({ ticks: 0, bpm: 120 });
  tempos.sort((a, b) => a.ticks - b.ticks);
  // ticks to seconds through the tempo map
  const secs = (t: number) => {
    let s = 0;
    for (let i = 0; i < tempos.length; i++) {
      const from = tempos[i].ticks;
      const to = i + 1 < tempos.length ? Math.min(tempos[i + 1].ticks, t) : t;
      if (to <= from) break;
      s += ((to - from) / ppq) * (60 / tempos[i].bpm);
    }
    return s;
  };
  const tracks = raw.map((tr) => ({
    name: tr.name,
    channel: tr.channel,
    notes: tr.notes.sort((a, b) => a.ticks - b.ticks || a.midi - b.midi).map((n) => ({ midi: n.midi, ticks: n.ticks, durationTicks: n.durationTicks, velocity: n.velocity, time: secs(n.ticks), duration: secs(n.ticks + n.durationTicks) - secs(n.ticks) })),
  }));
  const duration = Math.max(0, ...tracks.flatMap((t) => t.notes.map((n) => n.time + n.duration)));
  return { header: { ppq, tempos, timeSignatures: sigs, name }, tracks, duration };
}

/** Parts written as a Standard MIDI File (format 1, a track per part, drums on channel 10). */
export function toMidi(t: Transport, parts: Part[], until: number): Uint8Array {
  const out: number[] = [];
  const u32 = (v: number) => out.push((v >>> 24) & 255, (v >>> 16) & 255, (v >>> 8) & 255, v & 255);
  const vlq = (v: number, into: number[]) => {
    const b = [v & 127];
    while ((v >>= 7)) b.unshift((v & 127) | 128);
    into.push(...b);
  };
  const drums = new Set(["kick", "snare", "hat", "open-hat", "clap"]);
  const gm: Record<string, number> = { kick: 36, snare: 38, clap: 39, hat: 42, "open-hat": 46 };
  const tracks: number[][] = [];
  // the tempo track
  const tempo = Math.round(60_000_000 / t.bpm);
  const t0: number[] = [0, 0xff, 0x51, 3, (tempo >> 16) & 255, (tempo >> 8) & 255, tempo & 255, 0, 0xff, 0x58, 4, t.timeSignature[0], Math.round(Math.log2(t.timeSignature[1])), 24, 8, 0, 0xff, 0x2f, 0];
  tracks.push(t0);
  parts.forEach((part, i) => {
    const rows = part.rows(t, until, part.options.name ?? `p${i}`);
    const ch = drums.has(part.instrument) ? 9 : i % 9;
    const evs: [number, number, number, number][] = [];
    for (const r of rows) {
      const note = drums.has(r.instrument) ? gm[r.instrument] : r.midi;
      const vel = Math.max(1, Math.min(127, Math.round(r.velocity * 127)));
      evs.push([t.toTicks(r.start), 1, note, vel], [t.toTicks(r.start + r.duration), 0, note, 0]);
    }
    // offs before ons at the same tick
    evs.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
    const tr: number[] = [];
    const nm = [...(part.options.name ?? `${part.instrument} ${i}`)].map((c) => c.charCodeAt(0) & 127);
    tr.push(0, 0xff, 0x03, nm.length, ...nm);
    let last = 0;
    for (const [tick, on, note, vel] of evs) {
      vlq(tick - last, tr);
      last = tick;
      tr.push((on ? 0x90 : 0x80) | ch, note, vel);
    }
    tr.push(0, 0xff, 0x2f, 0);
    tracks.push(tr);
  });
  out.push(0x4d, 0x54, 0x68, 0x64);
  u32(6);
  out.push(0, 1, (tracks.length >> 8) & 255, tracks.length & 255, (t.ppq >> 8) & 255, t.ppq & 255);
  for (const tr of tracks) {
    out.push(0x4d, 0x54, 0x72, 0x6b);
    u32(tr.length);
    out.push(...tr);
  }
  return new Uint8Array(out);
}
