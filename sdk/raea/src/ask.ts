// raea/ask: questions about a piece's picture and sound, answered by clef-omni and paid from the person's raea plan
// (docs/ask.feature AK3). It runs `raea ask --batch - --json`, so a script, a `.ts` check, a `raea run` script and plain
// Node all ask through the same runner as the command: every question about one clip goes in one call, the clips are
// cut once, the calls go in parallel, and every answer is kept for calibration. Answers are advice until a question is
// calibrated (`calibrated` on each answer).
//
//   import { ask, askMany } from "raea/ask";
//   const { answers } = ask("out/piece.mp4", "Does the music fight the voice?");
//   const all = askMany([
//     { file: "out/a.mp4", set: "av" },
//     { file: "out/b.mp4", std: ["defect.glitch"], window: { from: 0, to: 3 } },
//   ]);

import { spawnSync } from "child_process";
import type { Question, Window } from "./ask.gen";

export type * from "./ask.gen";
export { SETS } from "./ask.gen";

/** One file and what to ask of it: a line of `raea ask --batch`. */
export interface AskEntry {
  /** the video, sound or picture asked about */
  file: string;
  /** questions in your own words (yes or no) or written out */
  questions?: (string | Question)[];
  /** standard sets by name: rubric, av, defects, platform, decide, make */
  set?: string | string[];
  /** standard questions by id (`raea ask list` lists them) */
  std?: string[];
  /** the seconds to look at, for every question here (else each question's own, else the whole file) */
  window?: Window;
  /** files set beside it: to pick between, or a before and after */
  with?: string[];
  /** the project's brief (else the nearest brief.ts above the file; "none": no brief, the state saying what the piece is) */
  brief?: string;
  /** a words file (a JSON list of the words heard), so ask knows what's said and whether anyone speaks */
  words?: string;
  /** whether anyone speaks in it, when no words file says */
  voice?: boolean;
  /** more for clef to read before the media: what raea measured, the platform */
  state?: string;
}

/** One question's answer, or why it wasn't asked or still waits: a line of `raea ask --json`. */
export interface Answer {
  /** its id in the record, for `raea ask verdict` */
  answer?: string;
  /** the file asked about */
  file: string;
  /** the question's id */
  id: string;
  /** a hash of its words, type and options: a new wording is a new version, calibrated on its own */
  version: string;
  /** the window's start, in seconds */
  from: number;
  /** the window's end, in seconds (unset: the file's end) */
  to?: number;
  /** the answer in words with its probability */
  says?: string;
  /** yes's probability, a pick's probability, or a level */
  value?: number;
  /** 0 to 1, higher is likelier a problem (none for a pick) */
  problem?: number;
  /** the option or level chosen */
  choice?: string;
  /** past the question's threshold (its calibrated one, else even odds) */
  flagged: boolean;
  /** its labels separate well enough for its answers to be findings; until then they're advice */
  calibrated: boolean;
  /** why it wasn't asked (it doesn't apply to this piece, or the call failed) */
  not_asked?: string;
  /** why it still waits (this month's pool is spent) */
  waits?: string;
  /** the way it was paid: your raea plan through the app, an agent key, or your Cloudflare account */
  route?: string;
  /** answered from the cache, at no cost */
  cached: boolean;
}

/** What a run came to. */
export interface Summary {
  /** the questions sent */
  asked: number;
  /** the answers read */
  answered: number;
  /** the calls made (with `dry`, the calls that would be) */
  calls: number;
  /** what the calls cost, in dollars */
  cost: number;
  /** with `dry`, what the calls would cost, in dollars */
  estimate: number;
  /** what's left of this month's pool, in dollars, when the plan paid */
  pool_left?: number;
  /** the questions waiting for the pool */
  waiting: number;
  /** the answers past their threshold */
  flagged: number;
  /** the calibrated answers past their threshold: findings */
  flagged_calibrated: number;
  /** the ways the calls were paid */
  routes: string[];
  /** the clips cut, each once */
  clips: number;
}

/** A run's answers, the calls `dry` would make, and its summary. */
export interface Asked {
  /** every question's answer, or why it wasn't asked, in the order they came */
  answers: Answer[];
  /** with `dry`: each call that would be made (its state, questions, media sizes and estimate) */
  would: unknown[];
  /** what it came to */
  summary: Summary;
}

/** How a run goes. */
export interface AskOptions {
  /** calls at once (4 unless set) */
  jobs?: number;
  /** cut the media and say what would be sent, spending nothing */
  dry?: boolean;
  /** the raea to run (RAEA_BIN, else raea on the PATH) */
  bin?: string;
}

type Env = { process?: { env?: Record<string, string | undefined> } };

/** Many files asked in one run, in parallel; throws when nothing could be asked (the error says why). */
export function askMany(entries: AskEntry[], o: AskOptions = {}): Asked {
  const bin = o.bin ?? (globalThis as Env).process?.env?.RAEA_BIN ?? "raea";
  const args = ["ask", "--batch", "-", "--json", ...(o.dry ? ["--dry"] : []), ...(o.jobs ? ["--jobs", String(o.jobs)] : [])];
  const r = spawnSync(bin, args, { input: JSON.stringify(entries), encoding: "utf8", maxBuffer: 64 << 20 });
  if (r.error) throw new Error(`raea ask: ${r.error.message}`);
  const out: Asked = { answers: [], would: [], summary: undefined as unknown as Summary };
  for (const line of String(r.stdout ?? "").split("\n")) {
    if (!line.trim().startsWith("{")) continue;
    const j = JSON.parse(line);
    if (j.summary) out.summary = j.summary;
    else if (j.would) out.would.push(j.would);
    else out.answers.push(j as Answer);
  }
  if (r.status !== 0 || !out.summary) throw new Error(String(r.stderr ?? "").trim() || `raea ask exited ${r.status}`);
  return out;
}

/** Questions about one file: your own words (yes or no), written-out questions, or with `std`/`set` the standard ones. */
export function ask(file: string, question: string | Question | (string | Question)[] = [], more: Omit<AskEntry, "file" | "questions"> & AskOptions = {}): Asked {
  const { jobs, dry, bin, ...entry } = more;
  const questions = Array.isArray(question) ? question : [question];
  return askMany([{ file, questions, ...entry }], { jobs, dry, bin });
}
