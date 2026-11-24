// Story as decisions (docs/story.md, "Story as decisions"): a beat's window as a decision model's state, and typed
// questions about that beat in the Jev / Clef request format (noul: yes or no; choice: one of named options; score:
// an ordered scale), each with its exact answer from the checks, the systems and the lenses. The answers are free
// and exact, so a small model can be trained to give them in one pass with calibrated probabilities (System 1),
// and measured against the slow, exact reading that made them (System 2) and against Clef itself.
//
//   questionsAt(story, telling, k, { as: "lines" })   the window as story lines: the world, the state, the beats
//   questionsAt(story, telling, k, { as: "script" })  the window as a script a person would write: no structure
import type { World } from "./ecs.ts";
import { analyze, checkWorld, type Finding } from "./checks.ts";
import { beatsOf, eventsOf } from "./load.ts";
import { LENSES } from "./lenses/index.ts";
import { readThrough } from "./lens.ts";
import { scriptOf, toLines } from "./lines.ts";
import type { AnyStory } from "./model.ts";
import { toData } from "./data.ts";
import { stateIn, stateLines } from "./state.ts";
import type { Audience } from "./systems/know.ts";

export type Question =
  | { type: "noul"; instructions: string }
  | { type: "choice"; instructions: string; criteria: Record<string, string> }
  | { type: "score"; instructions: string; criteria: string[] };
export type Answer = boolean | string | number;
export type Asked = { state: string; questions: Record<string, Question>; answers: Record<string, Answer> };

/** What each check means, as a question a reader of one beat can answer. */
export const CHECK_QUESTIONS: Record<string, string> = {
  turns: "Does this beat change nothing: no value moves, nothing is learned (a non-event)?",
  "and-then": "Does this beat only follow the one before ('and then') instead of being caused by it ('therefore') or turning it ('but')?",
  surprise: "Is this beat a big turn whose causes the audience never saw, with nothing set up for it?",
  cause: "Does something happen here whose cause the story never shows?",
  "dead-end": "Is this an event that leads nowhere: nothing later follows from it?",
  setup: "Is something set up here that is never paid off, or paid off here without having been set up?",
  belief: "Does a character act here on something they can't know yet?",
  rhythm: "Is this beat the end of a long run of beats that all turn the same way?",
  arc: "Does a character's arc break here (a need met with no change, a lie never faced)?",
  claim: "Is a claim made here that the story never shows proof of?",
  idea: "Does the story's controlling idea go unargued here (no beat on its value, or the counter-idea unvoiced)?",
};
const TENSION = ["none", "low", "some", "high", "peak"];

/** The beat a finding is about: a beat key, or the first beat to show the event it names. */
function beatOfFinding(w: World, f: Finding): number {
  const beats = beatsOf(w);
  const at = beats.findIndex((b) => b.key === f.about);
  return at >= 0 ? at : beats.findIndex((b) => b.beat.shows.includes(f.about));
}

/** Every finding (error or warn) by beat index: computed once per telling, read by every window. */
export function findingsByBeat(w: World): Map<number, Set<string>> {
  const out = new Map<number, Set<string>>();
  for (const f of checkWorld(w)) {
    if (f.level === "note") continue;
    const k = beatOfFinding(w, f);
    if (k >= 0) (out.get(k) ?? out.set(k, new Set()).get(k)!).add(f.check);
  }
  return out;
}

/** Each lens's stage for each beat index (the first stage whose map holds it). */
function stagesByBeat(s: AnyStory, telling: string, lenses: string[], seed: number): Record<string, Record<number, string>> {
  const known = lenses.map((n) => LENSES[n]).filter(Boolean);
  const w = analyze(s, telling, seed);
  const idx = new Map(beatsOf(w).map((b) => [b.key, b.index]));
  return Object.fromEntries(readThrough(s, known, telling, seed).map((r) => {
    const by: Record<number, string> = {};
    for (const [stage, keys] of Object.entries(r.map)) for (const key of keys) { const i = idx.get(key); if (i !== undefined && by[i] === undefined) by[i] = stage; }
    return [r.lens, by];
  }));
}

export type AskOptions = { as?: "lines" | "script"; before?: number; after?: number; lenses?: string[]; seed?: number; maxEvents?: number };

/** A reusable reader for one telling: the analysis, findings and lens stages computed once, then any beat's window. */
export function asker(s: AnyStory | Record<string, any>, telling: string, o: AskOptions = {}) {
  const story = ((s as AnyStory).kind === "story" ? s : { ...s, kind: "story" }) as AnyStory;
  const seed = o.seed ?? 0;
  const d = toData(story, seed) as Record<string, any>;
  const w = analyze(story, telling, seed);
  const found = findingsByBeat(w);
  const lenses = o.lenses ?? ["mckee", "pixar", "campbell"];
  const stages = stagesByBeat(story, telling, lenses, seed);
  const beats = beatsOf(w);
  const raw: any[] = d.tellings[telling];
  const events = eventsOf(w).sort((a, b) => a.order - b.order);

  return (k: number, as: "lines" | "script" = o.as ?? "lines"): Asked => {
    if (!Number.isInteger(k) || k < 0 || k >= beats.length) throw new RangeError(`no beat ${k + 1} in telling "${telling}": it has ${beats.length}`);
    const before = o.before ?? 3, after = o.after ?? 2;
    const from = Math.max(0, k - before), near = raw.slice(from, k + after + 1);
    const st = stateIn(w, k);
    // events a reader could name: the ones still ahead and the ones the window shows, in story order
    const inWindow = new Set(near.flatMap((b) => b.shows ?? []));
    const usable = events.filter((e) => inWindow.has(e.key) || st.ahead.includes(e.key)).slice(0, o.maxEvents ?? 40);
    let state: string;
    if (as === "lines") {
      const world = Object.fromEntries(usable.map((e) => [e.key, d.world[e.key]]));
      const head = toLines({ ...d, world, tellings: {} }).split("\n").filter((l) => !l.startsWith("TELLING"));
      const beatLines = toLines({ values: {}, agents: {}, world: {}, tellings: { t: near } }).split("\n").filter((l) => !l.startsWith("TELLING"));
      state = [...head, stateLines(st), `BEATS ${from + 1} to ${from + near.length} of ${raw.length}`, ...beatLines, `LOOK AT beat ${k + 1}`].join("\n");
    } else {
      state = [`A script, beats ${from + 1} to ${from + near.length} of ${raw.length}.`, scriptOf(near, from), `LOOK AT beat ${k + 1}`].join("\n");
    }
    const b = beats[k], beat = b.beat;
    const n = k + 1;
    const questions: Record<string, Question> = {};
    const answers: Record<string, Answer> = {};
    const has = found.get(k) ?? new Set<string>();
    for (const [check, q] of Object.entries(CHECK_QUESTIONS)) { questions[`breaks.${check}`] = { type: "noul", instructions: `Beat ${n}: ${q}` }; answers[`breaks.${check}`] = has.has(check); }

    const criteria: Record<string, string> = { none: "no event: a title, a breath, a beat of narration" };
    for (const e of usable) criteria[e.key] = `${e.ev.who}: ${e.ev.does}`;
    questions.shows = { type: "choice", instructions: `Which event does beat ${n} show first?`, criteria };
    answers.shows = beat.shows[0] && criteria[beat.shows[0]] ? beat.shows[0] : "none";

    const turns = Object.values(w.get<Record<string, number>>(b.e, "Turns") ?? {}).reduce((a, x) => a + x, 0);
    questions.turn = { type: "choice", instructions: `Taken together, which way does beat ${n} turn the story's values?`, criteria: { down: "worse for the hero: a loss, a threat, a no", none: "neither way", up: "better for the hero: a gain, a yes, relief" } };
    answers.turn = turns < 0 ? "down" : turns > 0 ? "up" : "none";
    questions.continues = { type: "noul", instructions: `Does beat ${n} continue the very event the beat before it showed (another shot of the same moment)?` };
    answers.continues = (w.get<string[]>(b.e, "Continues") ?? []).length > 0;
    const a = w.must<Audience>(b.e, "Audience");
    questions.learns = { type: "noul", instructions: `Does the audience learn a new fact in beat ${n}?` };
    answers.learns = a.learned.length > 0;
    questions.opens = { type: "noul", instructions: `Does beat ${n} open a question the audience now waits to have answered?` };
    answers.opens = (beat.asks ?? []).some((f: string) => a.open.includes(f));
    questions.tension = { type: "score", instructions: `How much tension does the audience feel at beat ${n}?`, criteria: TENSION };
    answers.tension = Math.min(4, Math.round((w.get<number>(b.e, "Tension") ?? 0) * 4));
    for (const l of lenses) {
      const lens = LENSES[l];
      if (!lens) continue;
      const crit: Record<string, string> = { none: "no stage of this framework" };
      for (const st2 of lens.stages) crit[st2] = st2;
      questions[`stage.${l}`] = { type: "choice", instructions: `Read as ${l} (${lens.about.replace(/\.$/, "")}): which stage is beat ${n}?`, criteria: crit };
      answers[`stage.${l}`] = stages[l]?.[k] ?? "none";
    }
    return { state, questions, answers };
  };
}

export const questionsAt = (s: AnyStory, telling: string, k: number, o: AskOptions = {}): Asked => asker(s, telling, o)(k, o.as);

