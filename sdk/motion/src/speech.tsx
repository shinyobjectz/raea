// @raea/motion/speech: lines said live by Pocket TTS (Kyutai's, on the engine's ONNX Runtime), streamed into the mix,
// with a word clock a game can read back. A game renders the lines in its state; the engine says them.
//
//   // in the game's state: the line being said, asked for at frame `asked`
//   const line = { text: "Ah. You fell. Good.", voice: "watts" };
//   <Speech {...line} start={s.line.start} cut={s.line.cut} />           // heard from `start`, stopped at the next
//                                                                         // word after `cut`
//   const at = speechAt({ ...line, start, cut }, frame, FPS);            // in the step: which word, ended?, endFrame
//
// A line is named by what it says (text, voice, seed, temperature): Pocket with a fixed seed says it the same way every
// time, so its samples, the moment each word starts and where a cut lands are functions of the line alone. That makes
// `speechAt` pure: the same answer in live play, in a render of a logged play, and in a fresh render. Live play only
// differs in waiting: the engine starts making a line the moment it's asked for (`prepareSpeech`, or the first frame
// a <Speech> or a `speechAt` names it) and only holds a frame if the worker hasn't got that far by then. Pocket makes
// its first audio in ~40 ms and runs ~8x faster than real time on an M5 Pro, so a line heard SPEECH_LEAD (0.15 s)
// after it's asked for never waits; one heard on the very frame it's asked for holds that frame ~50 ms, and a hold
// longer than the mixer's head puts that frame's sound late, so that stretch of the play isn't its render.
//
// Interruptions: set `cut` to the frame the narrator is interrupted; the line stops at the next word's start (at most
// 0.8 s on), faded over 25 ms. `speechAt(...).endFrame` says where, so the next line can start right after it (ask for
// it early with prepareSpeech and it's ready). The text can be made at run time (a language model's line): nothing is
// recorded ahead.

import { native } from "./native";

export interface Line {
  text: string;
  /** a Pocket voice ("alba", "marius", ...), a clip in ~/.raea-void/voices by name ("watts"), or a WAV
   *  ("/media/narrator.wav" or staticFile("narrator.wav")); the default is "watts" when that clip's there, else alba */
  voice?: string;
  /** which take: another seed says it another way (0) */
  seed?: number;
  /** Pocket's sampling temperature: lower is calmer and more even, higher livelier (0.7) */
  temperature?: number;
}

export interface SpeechProps extends Line {
  /** the frame it's heard from; null: not yet (made ahead, not heard) */
  start: number | null;
  /** the frame it's interrupted at: it stops at the next word's start; null: said to its end */
  cut?: number | null;
  /** gain (1) */
  volume?: number;
}

/** How long after a request a line should start, so live play never waits for it (seconds). */
export const SPEECH_LEAD = 0.15;

/** The frame a line asked for at `frame` should start: SPEECH_LEAD later. */
export function speechStart(frame: number, fps: number, lead = SPEECH_LEAD): number {
  return frame + Math.round(lead * fps);
}

function lineArgs(l: Line) {
  const a: { text: string; voice?: string; seed?: number; temperature?: number } = { text: l.text };
  if (l.voice !== undefined) a.voice = l.voice;
  if (l.seed !== undefined) a.seed = l.seed;
  if (l.temperature !== undefined) a.temperature = l.temperature;
  return a;
}

/** Starts making a line now (or finds it made): call it as soon as the text is known. */
export function prepareSpeech(line: Line): void {
  native("speech.line", lineArgs(line));
}

/** Where a line is at a frame. */
export interface SpeechState {
  /** the frame is at or past its start */
  started: boolean;
  /** the word being spoken (an index into `words`), -1 before the first */
  word: number;
  /** it has finished (said to its end, or stopped at its cut) */
  ended: boolean;
  /** the frame it ends on, once that's known (always known for a cut line, and for any line once it's all made) */
  endFrame: number | null;
  /** the line's words as Pocket says them (so far: a long line's later sentences are added as they're reached) */
  words: string[];
}

const BEFORE: SpeechState = { started: false, word: -1, ended: false, endFrame: null, words: [] };

/**
 * Where `line` (heard from `start`, cut at `cut`) is at `frame`: which word, whether it has ended, and where it ends.
 * Pure: a function of the line and the frame, so a game may fold it into its state. Waits for the engine if the line
 * hasn't been made that far yet (a render always waits; live play rarely has to).
 */
export function speechAt(line: Line & { start: number; cut?: number | null }, frame: number, fps: number): SpeechState {
  if (frame < line.start) return BEFORE;
  const t = (frame - line.start) / fps;
  const args: { text: string; voice?: string; seed?: number; temperature?: number; t: number; cut?: number } = { ...lineArgs(line), t };
  if (line.cut != null && line.cut <= frame) args.cut = Math.max(0, (line.cut - line.start) / fps);
  const r = native("speech.at", args) as { word: number; ended: boolean; end: number | null; words: string[]; total: number | null };
  return { started: true, word: r.word, ended: r.ended, endFrame: r.end === null ? null : line.start + Math.ceil(r.end * fps - 1e-6), words: r.words };
}

/** A line's length in seconds (waits until all of it is made; pure). */
export function speechDuration(line: Line): number {
  return native("speech.duration", lineArgs(line)) as number;
}

/** How much of a line has been made so far. Display-only: it depends on the machine and the moment. */
export function speechMade(line: Line): { seconds: number; done: boolean } {
  return native("speech.made", lineArgs(line)) as { seconds: number; done: boolean };
}

/**
 * A line said on the frames it's rendered: heard from `start` (made ahead while `start` is null), stopped at the next
 * word after `cut`. Keep it rendered until the line ends (speechAt's `ended`): taken off the page it stops at once
 * (faded over 10 ms).
 */
export function Speech(p: SpeechProps) {
  if (p.start === null || p.start === undefined) {
    // made ahead, not heard yet
    if (typeof (globalThis as { __raea_call?: unknown }).__raea_call === "function") prepareSpeech(p);
    return null;
  }
  const v: Record<string, unknown> = { ...lineArgs(p), start: Math.round(p.start) };
  if (p.cut != null) v.cut = Math.round(p.cut);
  if (p.volume !== undefined) v.volume = p.volume;
  return <audio data-speech={JSON.stringify(v)} />;
}
