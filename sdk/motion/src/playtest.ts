// The playtest DSL (sdk/react/docs/playtest.md): views say what clef-omni is shown, questions are written in clef's own
// question types, and the whole compiles to the check-pack JSON void's checks.rs runs. Every primitive maps one to one
// onto clef-omni (@cf/cloudflare/clef-omni, verified 2026-10-18 against its schema and with probe calls):
//
//   yes(...)     -> noul    (the probability of yes; optional descriptions of what yes and no mean)
//   choose(...)  -> choice  (2..255 named options; a probability for each, the top one, and a confidence)
//   grade(...)   -> choice  over ordered levels, read as the expected level (what checks.rs's "score" asks today)
//   rate(...)    -> score   (clef's native rubric: 2..10 ordered levels; a probability-weighted level, a probability
//                            per level, a confidence)
//   decide(...)  -> choice  over a game's actions (a bot's move)
//   rule(...)    -> no model call: the trace answers it
//
// clef returns no free text (it rejects any other question type, and extra parameters such as temperature), and the
// same request gets the same answer. Its limits are in CLEF and in the types below, so an invalid question doesn't
// compile, or throws where a type can't express the limit (counts above a handful, durations).
export type { Playtest } from "./games";
import type { AnyMachine, StateName } from "./machine";

// ---- clef-omni's limits -------------------------------------------------------------------------------------------
export const CLEF = {
  /** questions per call (65 is refused) */
  questionsPerCall: 64,
  /** question ids: letters, digits, '_', '.', '-'; 1 to 100 characters */
  id: /^[A-Za-z0-9_.-]{1,100}$/,
  choiceOptions: { min: 2, max: 255 },
  scoreLevels: { min: 2, max: 10 },
  /** PNG, JPEG or WebP; 4 MiB and 16 megapixels each, 8 MiB in all; 64 to 1,024 tokens each */
  images: { max: 4, bytes: 4 << 20, totalBytes: 8 << 20 },
  /** video/* (mp4 and webm verified); 16 MiB and 60 s each, sampled at 2 fps; up to 256 tokens a second */
  videos: { max: 2, seconds: 60, bytes: 16 << 20 },
  /** audio/* (wav and mpeg verified); 8 MiB and 300 s each; about 780 tokens a minute */
  audio: { max: 4, seconds: 300, bytes: 8 << 20 },
  /** audio and video together, decoded */
  avBytes: 16 << 20,
  contextTokens: 64_000,
  usdPerMillionInputTokens: 0.15,
} as const;

type IdChar =
  | "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" | "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t" | "u" | "v" | "w" | "x" | "y" | "z"
  | "A" | "B" | "C" | "D" | "E" | "F" | "G" | "H" | "I" | "J" | "K" | "L" | "M" | "N" | "O" | "P" | "Q" | "R" | "S" | "T" | "U" | "V" | "W" | "X" | "Y" | "Z"
  | "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "_" | "." | "-";
type IdOk<S extends string> = S extends `${infer C}${infer R}` ? (C extends IdChar ? IdOk<R> : false) : true;
/** A question id clef accepts: an id with any other character is a type error (and length is checked when built). */
export type ClefId<S extends string> = S extends "" ? never : IdOk<S> extends true ? S : never;

/** 2 to 255 options: at least two is a type; at most 255 is checked when built. */
export type Options = readonly [string, string, ...string[]];
/** 2 to 10 ordered levels, lowest first. */
export type Levels =
  | readonly [string, string] | readonly [string, string, string] | readonly [string, string, string, string]
  | readonly [string, string, string, string, string] | readonly [string, string, string, string, string, string]
  | readonly [string, string, string, string, string, string, string]
  | readonly [string, string, string, string, string, string, string, string]
  | readonly [string, string, string, string, string, string, string, string, string]
  | readonly [string, string, string, string, string, string, string, string, string, string];
/** checks.rs's five levels: a grade over exactly these is a "score" check void runs as it is today. */
export const LEVELS = ["poor", "weak", "acceptable", "strong", "excellent"] as const;

// ---- views: what clef is shown --------------------------------------------------------------------------------------
/** A moment of a run: an event the game's `playtest.events` named, or one the bots found (stuck, furthest, finish). */
export interface Moment { name: string; frame: number; at: Record<string, unknown> }
/** One frame of the run's trace: the frame, the keys held, the game's observation and the events that fired. */
export type TraceRow = { frame: number; keys: string[]; events: string[]; progress: number } & Record<string, unknown>;

export type Layer = "clean" | "debug";
/** What a view makes, and the clef input it becomes: text -> state only; stills and sheet -> images (stills: up to 3,
 *  the window's ends and the moment; sheet: one); clip -> a video (at most 60 s); audio -> an audio clip (at most 300 s). */
export type MediaKind = "text" | "stills" | "sheet" | "clip" | "audio";
export type Input = "state" | "images" | "videos" | "audio";
export const INPUT: Record<MediaKind, Input> = { text: "state", stills: "images", sheet: "images", clip: "videos", audio: "audio" };

export interface ViewSpec<M extends MediaKind> {
  /** a moment predicate, a trace predicate (the first frame of each stretch where it holds), or the whole run */
  when: ((m: Moment) => boolean) | { frame: (row: TraceRow) => boolean } | "session";
  /** frames before and after the moment ([0, 0]: the moment's frame alone) */
  window?: [number, number];
  as: M;
  /** clip and sheet: every n-th frame */
  stride?: number;
  /** at most this many instances (the first ones in the run) */
  limit?: number;
}
export interface View<L extends Layer = Layer, M extends MediaKind = MediaKind> extends ViewSpec<M> {
  name: string;
  layer: L;
  overlays: string[];
}

function checkView<M extends MediaKind>(name: string, spec: ViewSpec<M>) {
  if (!CLEF.id.test(name)) throw new Error(`view ${name}: a view's name is a clef id (letters, digits, _ . -)`);
  if (spec.as === "sheet" && spec.stride === undefined && spec.when !== "session") throw new Error(`view ${name}: a sheet needs a stride`);
}
/** A view for taste and quality questions: the game's picture only. */
export function clean<M extends MediaKind>(name: string, spec: ViewSpec<M>): View<"clean", M> {
  checkView(name, spec);
  return { name, layer: "clean", overlays: [], ...spec };
}
/** A view for diagnosis: the picture with the game's named <Overlay>s drawn (never for a taste question). */
export function debug<M extends MediaKind>(name: string, overlays: readonly [string, ...string[]], spec: ViewSpec<M>): View<"debug", M> {
  checkView(name, spec);
  return { name, layer: "debug", overlays: [...overlays], ...spec };
}
/** The clip's length in seconds at `fps`, when the view's window fixes it (a session clip is checked when rendered). */
export function clipSeconds(v: View, fps: number): number | null {
  return v.as === "clip" && v.when !== "session" && v.window ? (v.window[1] - v.window[0]) / fps : null;
}

// ---- questions ------------------------------------------------------------------------------------------------------
export type Scope = "render" | "hook" | "sheet" | "session" | "moment" | "page" | "turntable";
/** What calibration measured (checks.rs calibrate()): separation on judged examples and the cut it chose. */
export interface Calibration { auc: number; cut: number; n: number; judged: string }
/** A check warns only once calibration has measured it; until then it's advisory and only recorded. */
type Standing = { status?: "advisory"; calibration?: null } | { status: "warn"; calibration: Calibration };
type Common = Standing & { scope: Scope; fix: string; context?: string[]; labelWords?: string[] };
/** A rule is a measurement, exact by construction: it may warn without calibration. */
type RuleCommon = { scope: Scope; fix: string; status?: "advisory" | "warn"; calibration?: null; context?: undefined; labelWords?: string[] };
/** A taste question takes only a clean view: a debug overlay in a taste question is a type error. */
type Seen<T extends boolean, M extends MediaKind> = T extends true ? { taste: T; view: View<"clean", M> } : { taste?: T; view: View<Layer, M> };

/** The native clef question each check sends, verbatim. */
export type ClefQuestion =
  | { type: "noul"; instructions: string; criteria?: { true: string; false: string } }
  | { type: "choice"; instructions: string; criteria: Record<string, string | null> }
  | { type: "score"; instructions: string; criteria: string[] };

export type Check =
  | { kind: "rule"; id: string; rule: string; args?: Record<string, unknown>; common: RuleCommon }
  | { kind: "yes"; id: string; clef: ClefQuestion; view: View; fault: "yes" | "no"; threshold: number; common: Common }
  | { kind: "choose"; id: string; clef: ClefQuestion; view: View; fault: string[]; threshold: number; common: Common }
  | { kind: "grade"; id: string; clef: ClefQuestion; view: View; levels: string[]; below: number; common: Common }
  | { kind: "rate"; id: string; clef: ClefQuestion; view: View; levels: string[]; below: number; common: Common };

function idOf(id: string) {
  if (!CLEF.id.test(id)) throw new Error(`${id}: clef ids are 1 to 100 letters, digits, _ . -`);
  return id;
}
function prob(x: number, what: string) {
  if (!(x >= 0 && x <= 1)) throw new Error(`${what}: a threshold on a probability is between 0 and 1`);
  return x;
}

/** Anything the trace answers (finished, softlocks, deaths): a measurement, no model call. */
export function rule<I extends string>(id: ClefId<I>, rule: string, o: RuleCommon): Check {
  return { kind: "rule", id: idOf(id), rule, common: o };
}

// ---- state machine rules: the trace answers them, for any machine (sdk/react/docs/state-machines.md) ---------------------
// Each reads report.json's `machines` (sdk/react/bench/playtest/machines.ts), which counts what every machine did across all
// the bots' play. None knows what a machine is for: a game's player, a menu and an agent's workflow are checked alike.
type MachineRule = RuleCommon & { machine?: AnyMachine };
const machineArgs = (m: AnyMachine | undefined, more: Record<string, unknown> = {}) => (m ? { machine: m.id, ...more } : { ...more });

/** State coverage: at least `min` (0..1, default 1) of the machine's states were entered in the bots' play. */
export function coversStates<I extends string>(id: ClefId<I>, m: AnyMachine, o: RuleCommon & { min?: number }): Check {
  return { kind: "rule", id: idOf(id), rule: "state_coverage", args: machineArgs(m, { min: prob(o.min ?? 1, id) }), common: o };
}
/** Transition coverage: at least `min` (0..1, default 1) of the machine's transitions fired in the bots' play. */
export function coversTransitions<I extends string>(id: ClefId<I>, m: AnyMachine, o: RuleCommon & { min?: number }): Check {
  return { kind: "rule", id: idOf(id), rule: "transition_coverage", args: machineArgs(m, { min: prob(o.min ?? 1, id) }), common: o };
}
/** No softlock: no state that can't be left (static), and none the bots entered and never left while the input kept
 *  changing. With `loops`, also no transition firing over and over while progress stays flat (an agent's "looping
 *  without progress"). One machine, or every machine when `machine` is omitted. */
export function noSoftlock<I extends string>(id: ClefId<I>, o: MachineRule & { loops?: boolean }): Check {
  return { kind: "rule", id: idOf(id), rule: "machine_softlock", args: machineArgs(o.machine, o.loops ? { loops: true } : {}), common: o };
}
/** No unreachable states: every state some sequence of transitions reaches (static analysis of the definition). */
export function noUnreachable<I extends string>(id: ClefId<I>, o: MachineRule): Check {
  return { kind: "rule", id: idOf(id), rule: "unreachable_states", args: machineArgs(o.machine), common: o };
}
/** The bots' play entered `state` of machine `m` (a state name the machine doesn't have is a type error). */
export function reaches<I extends string, M extends AnyMachine>(id: ClefId<I>, m: M, state: StateName<M>, o: RuleCommon): Check {
  if (!m.states.includes(state)) throw new Error(`${id}: machine ${m.id} has no state ${state}`);
  return { kind: "rule", id: idOf(id), rule: "reached_state", args: { machine: m.id, state }, common: o };
}

/** A yes/no question (clef `noul`). `fault` says which answer is bad news; it faults when that answer's probability
 *  reaches `threshold` (0.5 unless calibrated otherwise). `means` describes what yes and no mean (noul's criteria). */
export function yes<I extends string, M extends MediaKind, T extends boolean = false>(
  id: ClefId<I>, ask: string,
  o: Common & Seen<T, M> & { fault: "yes" | "no"; threshold?: number; means?: { yes: string; no: string } },
): Check {
  const clef: ClefQuestion = { type: "noul", instructions: ask };
  if (o.means) clef.criteria = { true: o.means.yes, false: o.means.no };
  return { kind: "yes", id: idOf(id), clef, view: o.view, fault: o.fault, threshold: prob(o.threshold ?? 0.5, id), common: o };
}

/** A multiple-choice question (clef `choice`): one of `options` (each with an optional description). It faults when
 *  the probability on the `fault` options together reaches `threshold`. */
export function choose<I extends string, const O extends Options, M extends MediaKind, T extends boolean = false>(
  id: ClefId<I>, ask: string, options: O,
  o: Common & Seen<T, M> & { fault: readonly [O[number], ...O[number][]]; threshold?: number; describe?: Partial<Record<O[number], string>> },
): Check {
  if (options.length > CLEF.choiceOptions.max) throw new Error(`${id}: clef takes at most ${CLEF.choiceOptions.max} options`);
  if (new Set(options).size !== options.length) throw new Error(`${id}: options repeat`);
  const criteria = Object.fromEntries(options.map((k) => [k, (o.describe as Record<string, string> | undefined)?.[k] ?? null]));
  return { kind: "choose", id: idOf(id), clef: { type: "choice", instructions: ask, criteria }, view: o.view, fault: [...o.fault], threshold: prob(o.threshold ?? 0.5, id), common: o };
}

/** A graded score as clef `choice` over ordered levels (lowest first), read as the expected level; it faults below the
 *  level named by `below`. Over LEVELS it is exactly checks.rs's "score" check. */
export function grade<I extends string, const L extends Levels, M extends MediaKind, T extends boolean = false>(
  id: ClefId<I>, ask: string, levels: L, o: Common & Seen<T, M> & { below: L[number] },
): Check {
  const criteria = Object.fromEntries(levels.map((k) => [k, null]));
  return { kind: "grade", id: idOf(id), clef: { type: "choice", instructions: ask, criteria }, view: o.view, levels: [...levels], below: levels.indexOf(o.below), common: o };
}

/** A rubric (clef's native `score`): ordered level descriptions, lowest first; clef returns the probability-weighted
 *  level. It faults below the level named by `below`. */
export function rate<I extends string, const L extends Levels, M extends MediaKind, T extends boolean = false>(
  id: ClefId<I>, ask: string, levels: L, o: Common & Seen<T, M> & { below: L[number] },
): Check {
  return { kind: "rate", id: idOf(id), clef: { type: "score", instructions: ask, criteria: [...levels] }, view: o.view, levels: [...levels], below: levels.indexOf(o.below), common: o };
}

/** A bot's move (clef `choice` over the game's actions, named as the model reads them). `sees` is the frame and the
 *  observation, or the observation alone (state only). */
export interface Decision { id: string; clef: ClefQuestion; actions: Record<string, string[]>; sees: "frame" | "state"; hold: number }
export function decide<I extends string, const A extends Record<string, readonly string[]>>(
  id: ClefId<I>, ask: string, actions: A, o: { sees: "frame" | "state"; hold: number; describe?: Partial<Record<keyof A & string, string>> },
): Decision {
  const names = Object.keys(actions);
  if (names.length < CLEF.choiceOptions.min || names.length > CLEF.choiceOptions.max) throw new Error(`${id}: 2 to 255 actions`);
  const criteria = Object.fromEntries(names.map((k) => [k, (o.describe as Record<string, string> | undefined)?.[k] ?? null]));
  return { id: idOf(id), clef: { type: "choice", instructions: ask, criteria }, actions: Object.fromEntries(names.map((k) => [k, [...actions[k]]])), sees: o.sees, hold: o.hold };
}

// ---- a playtest, and its compiled pack ------------------------------------------------------------------------------
export interface PlaytestSpec { pack: string; version: number; about: string; fps: number; views: View[]; checks: Check[]; decisions?: Decision[] }

/** The playtest an EDIT.playtest.ts exports: its views, its checks and its decisions, checked against clef's limits. */
export function playtest(spec: PlaytestSpec): PlaytestSpec {
  const names = new Set<string>();
  for (const v of spec.views) {
    if (names.has(v.name)) throw new Error(`view ${v.name} twice`);
    names.add(v.name);
    const s = clipSeconds(v, spec.fps);
    if (s !== null && s > CLEF.videos.seconds) throw new Error(`view ${v.name}: a ${s.toFixed(1)} s clip; clef takes at most ${CLEF.videos.seconds} s`);
  }
  const ids = new Set<string>();
  const perView = new Map<string, number>();
  for (const c of spec.checks) {
    if (ids.has(c.id)) throw new Error(`check ${c.id} twice`);
    ids.add(c.id);
    if (c.kind === "rule") continue;
    if (!names.has(c.view.name)) throw new Error(`check ${c.id}: its view ${c.view.name} isn't in the playtest's views`);
    perView.set(c.view.name, (perView.get(c.view.name) ?? 0) + 1);
  }
  for (const [v, n] of perView) if (n > CLEF.questionsPerCall) throw new Error(`view ${v}: ${n} questions; one clef call takes ${CLEF.questionsPerCall}`);
  return spec;
}

/** The check-pack JSON void's checks.rs reads (Pack and Check), with the native clef question beside the fields it
 *  knows. A yes check and a grade over LEVELS run in checks.rs as they are; choose and rate carry `clef` for a runner
 *  that sends it verbatim (sdk/react/bench/playtest/judge.ts; checks.rs once it does too). */
export function compile(spec: PlaytestSpec) {
  return {
    pack: spec.pack,
    version: spec.version,
    about: spec.about,
    checks: spec.checks.map((c) => {
      const o = c.common;
      const base: Record<string, unknown> = { id: c.id, scope: o.scope };
      if (c.kind === "rule") Object.assign(base, { eye: "rule", rule: c.rule, ...(c.args ? { args: c.args } : {}) });
      else {
        base.eye = "clef";
        base.view = c.view.name;
        base.layer = c.view.layer;
        base.input = INPUT[c.view.as];
        if (c.kind === "yes") Object.assign(base, { type: "yes", ask: c.clef.instructions, threshold: c.threshold, ...(c.fault === "no" ? { invert: true } : {}) });
        if (c.kind === "choose") Object.assign(base, { type: "choice", ask: c.clef.instructions, threshold: c.threshold, fault_options: c.fault });
        if (c.kind === "grade") {
          const standard = c.levels.length === LEVELS.length && c.levels.every((l, i) => l === LEVELS[i]);
          Object.assign(base, { type: standard ? "score" : "grade", ask: c.clef.instructions, threshold: c.below, below: true, ...(standard ? {} : { levels: c.levels }) });
        }
        if (c.kind === "rate") Object.assign(base, { type: "rate", ask: c.clef.instructions, threshold: c.below, below: true, levels: c.levels });
        if (o.context?.length) base.context = o.context;
        base.clef = c.clef;
      }
      base.status = o.status ?? "advisory";
      if (o.calibration) base.calibration = o.calibration;
      if (o.labelWords?.length) base.label_words = o.labelWords;
      base.fix = o.fix;
      return base;
    }),
  };
}

/** A check's value from clef's answer, and whether it faults (the same reading checks.rs's finding() makes). */
export function read(c: Exclude<Check, { kind: "rule" }>, answer: any): { value: number; fault: boolean; says: string } {
  if (c.kind === "yes") {
    const p = answer?.noul ?? 0;
    const bad = c.fault === "yes" ? p : 1 - p;
    return { value: p, fault: bad >= c.threshold, says: `yes p=${p.toFixed(2)}` };
  }
  if (c.kind === "choose") {
    const pr = answer?.probabilities ?? {};
    const bad = c.fault.reduce((s, k) => s + (pr[k] ?? 0), 0);
    return { value: bad, fault: bad >= c.threshold, says: `${answer?.choice} (p on ${c.fault.join("|")} = ${bad.toFixed(2)})` };
  }
  // grade: expected level over the choice's probabilities; rate: clef's own probability-weighted level
  let v: number;
  if (c.kind === "grade") {
    const pr = answer?.probabilities ?? {};
    const total = c.levels.reduce((s, l) => s + (pr[l] ?? 0), 0);
    v = total > 0 ? c.levels.reduce((s, l, i) => s + i * (pr[l] ?? 0), 0) / total : Math.max(0, c.levels.indexOf(answer?.choice));
  } else v = answer?.score ?? 0;
  return { value: v, fault: v < c.below, says: `level ${v.toFixed(2)} of ${c.levels.length - 1} (${c.levels[Math.round(v)]})` };
}
