// The story's state at a beat (docs/story.md, "Any length"): what the derived systems hold just before beat k, small
// enough to stand in for every beat before it. A model that writes, reads or judges a long telling is handed this and
// a few neighbouring beats, never the whole telling, so length is a number of steps, not a context window:
//   knows      facts the audience holds        open      questions still open        suspense  who lacks what
//   levels     each value's level              tension   0 to 1                      unpaid    setups planted, not paid
//   shown      events seen so far (count)      ahead     events the telling hasn't shown yet
//   unsettled  each agent's want, need or lie no shown event has ended
// windowAt() is that state plus the beats around k as story lines: the prompt a windowed step reads.
import type { World } from "./ecs.ts";
import { analyze } from "./checks.ts";
import { beatsOf, eventsOf, keysOf, type RAgent } from "./load.ts";
import type { AnyStory } from "./model.ts";
import { listOf } from "./model.ts";
import { toData } from "./data.ts";
import { toLines } from "./lines.ts";
import type { Audience } from "./systems/know.ts";
import type { Curve } from "./systems/values.ts";

export type State = {
  at: number;
  knows: string[];
  open: string[];
  suspense: { who: string; fact: string }[];
  levels: Record<string, number>;
  tension: number;
  unpaid: string[];
  shown: number;
  ahead: string[];
  unsettled: Record<string, string[]>;
};

/** The state just before beat k (k = the beat count: after the last). */
export function stateIn(w: World, k: number): State {
  const beats = beatsOf(w);
  const prev = k > 0 ? beats[k - 1] : undefined;
  const a = prev ? w.must<Audience>(prev.e, "Audience") : { knows: [], open: [], suspense: [] };
  const curves = w.resource<Record<string, Curve>>("Curves") ?? {};
  const levels = Object.fromEntries(Object.entries(curves).map(([v, c]) => [v, c.levels[k] ?? 0]));
  const before = beats.slice(0, k);
  const shown = new Set(before.flatMap((b) => b.beat.shows));
  const planted = before.flatMap((b) => listOf(b.beat.setup).map(String));
  const paid = new Set(before.flatMap((b) => listOf(b.beat.pays).map(String)));
  const events = eventsOf(w).sort((x, y) => x.order - y.order);
  const ended: Record<string, Set<string>> = {};
  for (const { key, ev } of events) if (shown.has(key)) for (const [who, what] of Object.entries(ev.ends ?? {})) (ended[who] ??= new Set()).add(String(what).replace("-refused", ""));
  const unsettled: Record<string, string[]> = {};
  for (const [who, e] of Object.entries(keysOf(w).agents)) {
    const ag = w.must<RAgent>(e, "Agent");
    const left = (["want", "need", "lie"] as const).filter((t) => ag[t] && !ended[who]?.has(t));
    if (left.length) unsettled[who] = left;
  }
  return {
    at: k,
    knows: a.knows,
    open: a.open,
    suspense: a.suspense,
    levels,
    tension: prev ? (w.get<number>(prev.e, "Tension") ?? 0) : 0,
    unpaid: [...new Set(planted.filter((s) => !paid.has(s)))],
    shown: shown.size,
    ahead: events.map((x) => x.key).filter((key) => !shown.has(key)),
    unsettled,
  };
}

export const stateAt = (s: AnyStory, telling: string | undefined, k: number, seed = 0): State => stateIn(analyze(s, telling, seed), k);

/** A state as lines, in the story-lines style. */
export function stateLines(st: State): string {
  const out = [`STATE before beat ${st.at + 1}: shown ${st.shown} events | tension ${st.tension}`];
  if (Object.keys(st.levels).length) out.push(`LEVELS ${Object.entries(st.levels).map(([v, n]) => `${v}${n >= 0 ? "+" : ""}${n}`).join(" ")}`);
  if (st.knows.length) out.push(`KNOWS ${st.knows.join(",")}`);
  if (st.open.length) out.push(`OPEN ${st.open.join(",")}`);
  if (st.suspense.length) out.push(`SUSPENSE ${st.suspense.map((x) => `${x.who}:${x.fact}`).join(" ")}`);
  if (st.unpaid.length) out.push(`UNPAID ${st.unpaid.join(",")}`);
  if (Object.keys(st.unsettled).length) out.push(`UNSETTLED ${Object.entries(st.unsettled).map(([a, t]) => `${a}:${t.join("+")}`).join(" ")}`);
  if (st.ahead.length) out.push(`AHEAD ${st.ahead.join(",")}`);
  return out.join("\n");
}

/**
 * What a windowed step reads to write beat k: the story's world (values, facts, agents, and only the events a step
 * could use: the ones not yet shown and the ones its neighbours show), the state before k, and up to `before` beats
 * before and `after` beats after it, as story lines. Its size doesn't grow with the telling.
 */
export function windowAt(s: AnyStory | Record<string, any>, telling: string, k: number, o: { before?: number; after?: number; seed?: number; skip?: boolean } = {}): string {
  const before = o.before ?? 3, after = o.after ?? 2;
  const d: Record<string, any> = (s as AnyStory).kind === "story" ? toData(s as AnyStory, o.seed ?? 0) : s;
  const beats: any[] = d.tellings[telling];
  // skip: beat k is missing from the telling (the generator's task: write it back); else beat k is there, and left out
  const rest = o.skip ? beats : beats.filter((_, n) => n !== k);
  const w = analyze({ ...d, kind: "story", tellings: { [telling]: rest } } as unknown as AnyStory, telling, o.seed ?? 0);
  const st = stateIn(w, k);
  const near = [...rest.slice(Math.max(0, k - before), k), ...rest.slice(k, k + after)];
  const keep = new Set([...st.ahead, ...near.flatMap((b) => b.shows ?? [])]);
  const world = Object.fromEntries(Object.entries(d.world).filter(([key]) => keep.has(key)));
  const lines = toLines({ ...d, world, tellings: {} }).split("\n").filter((l) => !l.startsWith("TELLING"));
  const beatLines = (bs: any[]) => toLines({ values: {}, agents: {}, world: {}, tellings: { t: bs } }).split("\n").filter((l) => !l.startsWith("TELLING"));
  return [
    ...lines,
    stateLines(st),
    `BEFORE (beats ${Math.max(0, k - before) + 1} to ${k} of ${rest.length + 1})`,
    ...beatLines(rest.slice(Math.max(0, k - before), k)),
    `WRITE beat ${k + 1}`,
    `AFTER`,
    ...beatLines(rest.slice(k, k + after)),
  ].join("\n");
}
