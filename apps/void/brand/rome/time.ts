// Film time: where each narration line starts (timeline.json) and when each of its words is said (voice.json), in
// seconds of the film. Scenes cut on words: TL.word("strike", "Casca") is when "Casca" is said.
import timeline from "./timeline.json";
import voice from "./voice.json";

export const FPS = 30;
/** the letterbox bars, top and bottom (about 2.39:1 at 1920x1080): captions, names and dates go in them */
export const BAR = 132;

export type Word = { w: string; a: number; b: number };
export const LINES = timeline.lines as { id: string; at: number; seconds: number }[];
const WORDS: Record<string, Word[]> = Object.fromEntries((voice as { id: string; words: Word[] }[]).map((v) => [v.id, v.words]));

export const words = (line: string) => WORDS[line];

export const TL = {
  end: timeline.end,
  hit: timeline.hit,
  scoreAt: timeline.scoreAt,
  /** each line's start */
  at: Object.fromEntries(LINES.map((l) => [l.id, l.at])) as Record<string, number>,
  /** each line's end */
  done: Object.fromEntries(LINES.map((l) => [l.id, l.at + l.seconds])) as Record<string, number>,
  /** when the k-th word of `line` that starts with `prefix` is said (film seconds) */
  word(line: string, prefix: string, k = 0): number {
    const hits = WORDS[line].filter((w) => w.w.startsWith(prefix));
    const w = hits[k];
    if (!w) throw new Error(`no word "${prefix}" (#${k}) in line ${line}`);
    return LINES.find((l) => l.id === line)!.at + w.a;
  },
};

export const clamp01 = (x: number) => Math.min(1, Math.max(0, x));
export const smooth = (u: number) => {
  const x = clamp01(u);
  return x * x * (3 - 2 * x);
};
export const ease = (t: number, a: number, b: number) => smooth((t - a) / (b - a));
export const lerp = (a: number, b: number, u: number) => a + (b - a) * u;
export const mix3 = (a: number[], b: number[], u: number) => a.map((v, i) => v + (b[i] - v) * u) as [number, number, number];
/** a seeded value in 0..1 */
export const rnd = (i: number) => {
  const x = Math.sin(i * 127.1 + 311.7) * 43758.5453;
  return x - Math.floor(x);
};
