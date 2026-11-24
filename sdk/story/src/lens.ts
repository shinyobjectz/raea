// Lenses (docs/story.feature ST3a): a framework as code. A lens reads a derived world: it maps its stages onto beats (a
// beat's own `stage` label wins over inference), checks what its framework asks, and proposes beats for stages that are
// missing. Every lens has the same shape, so a person's or an agency's own format is one more module.
// The inference here is shared: the protagonist, the brand, the idea's value, the inciting event, the crisis, the
// climax. It reads only derived data and the story's own fields, never prose.
import type { World } from "./ecs.ts";
import { analyze, unmeant, type Finding, type Level } from "./checks.ts";
import { beatsOf, eventsOf, infoOf, keysOf, type RAgent, type RBeat, type REvent } from "./load.ts";
import type { AnyStory } from "./model.ts";

export type StageMap = Record<string, string[]>;
export type Proposal = { stage: string; after?: string; onScreen: string; feel?: string; why: string };
export type Reading = { map: StageMap; findings: Finding[]; proposals: Proposal[] };
export type Lens = { name: string; about: string; stages: string[]; read: (w: World, s: Structure) => Reading };
export type LensResult = Reading & { lens: string; passes: boolean };

export const lens = (l: Lens): Lens => l;

/** What every lens reads: the structure inferred once per world. */
export type Structure = {
  beats: { key: string; beat: RBeat; index: number; turns: Record<string, number>; level: Record<string, number>; tension: number; linked: boolean }[];
  events: { key: string; ev: REvent; order: number }[];
  focus: string | undefined;
  hero: string | undefined;
  brand: string | undefined;
  agents: Record<string, RAgent>;
  inciting: string | undefined;
  crisis: string | undefined;
  climax: string | undefined;
  /** the first beat showing an event, and the last */
  firstShowing: (event: string | undefined) => string | undefined;
  lastShowing: (event: string | undefined) => string | undefined;
  focusTurn: (turns: Record<string, number>) => number;
  /** the focus value's level after a beat (its sum over values when the story names no focus) */
  focusLevel: (key: string | undefined) => number;
  /** the beats from one key to another, both left out */
  between: (from: string | undefined, to: string | undefined) => string[];
  /** the first beat a lens's stage is labelled on (`stage: { campbell: "call" }`), which wins over inference */
  label: (lens: string, stage: string) => string | undefined;
};

const size = (t: Partial<Record<string, number>> | undefined) => Object.values(t ?? {}).reduce<number>((a, d) => a + Math.abs(d ?? 0), 0);

/** Stage names, in any lens, that pin an anchor when a beat carries them. */
const INCITING = ["inciting", "one-day", "call", "catalyst", "need"];
const CLIMAX = ["climax", "finally", "resurrection", "finale"];

export function structure(w: World): Structure {
  const beats = beatsOf(w).map((b) => ({ key: b.key, beat: b.beat, index: b.index, turns: w.must<Record<string, number>>(b.e, "Turns"), level: w.must<Record<string, number>>(b.e, "Level"), tension: w.must<number>(b.e, "Tension"), linked: w.must<boolean>(b.e, "Linked") }));
  const events = eventsOf(w).sort((a, b) => a.order - b.order).map(({ key, ev, order }) => ({ key, ev, order }));
  const agents = Object.fromEntries(Object.entries(keysOf(w).agents).map(([k, e]) => [k, w.must<RAgent>(e, "Agent")]));
  const hero = Object.keys(agents).find((a) => agents[a].role === "customer" || agents[a].mask === "hero") ?? events[0]?.ev.who;
  const brand = Object.keys(agents).find((a) => agents[a].role === "brand" || agents[a].mask === "mentor" || agents[a].mask === "gift");
  const focus = infoOf(w).idea?.value ?? Object.keys(keysOf(w).values)[0];
  const focusTurn = (t: Record<string, number>) => (focus ? t[focus] ?? 0 : Object.values(t).reduce((a, b) => a + b, 0));
  // labels on beats (any lens's) pin the anchors every lens reads: the event a labelled beat shows first
  const pinned = (names: string[]) => { const b = beats.find((x) => Object.values(x.beat.stage ?? {}).some((v) => names.includes(v))); return b?.beat.shows[0]; };
  const shownEv = new Set(beats.flatMap((b) => b.beat.shows));
  // the inciting incident: the first event on screen, not routine, that changes anything
  const inciting = pinned(INCITING) ?? events.find((x) => shownEv.has(x.key) && size(x.ev.turns) >= 1 && !x.ev.routine)?.key ?? events.find((x) => size(x.ev.turns) >= 2 && !x.ev.routine)?.key;
  const incOrder = events.find((x) => x.key === inciting)?.order ?? 0;
  // the crisis: the last dilemma after the inciting incident (an early one is a refusal or a debate, not the crisis)
  const crisis = pinned(["crisis"]) ?? [...events].reverse().find((x) => x.order >= incOrder && (x.ev.dilemma?.length ?? 0) >= 2)?.key;
  const crisisOrder = events.find((x) => x.key === crisis)?.order ?? incOrder;
  const after = events.filter((x) => x.order > crisisOrder);
  const climax = pinned(CLIMAX) ?? (after.length ? after.reduce((best, x) => (Math.abs(focusTurn(x.ev.turns as Record<string, number> ?? {})) > Math.abs(focusTurn(best.ev.turns as Record<string, number> ?? {})) ? x : best)).key : undefined);
  const firstShowing = (ev: string | undefined) => (ev ? beats.find((b) => b.beat.shows.includes(ev))?.key : undefined);
  const lastShowing = (ev: string | undefined) => (ev ? [...beats].reverse().find((b) => b.beat.shows.includes(ev))?.key : undefined);
  const focusLevel = (k: string | undefined) => { const b = beats.find((x) => x.key === k); return b ? focusTurn(b.level) : 0; };
  const idx = (k: string | undefined) => beats.find((x) => x.key === k)?.index;
  const between = (a: string | undefined, b: string | undefined) => beats.filter((x) => x.index > (idx(a) ?? -1) && x.index < (idx(b) ?? beats.length)).map((x) => x.key);
  const label = (l: string, st: string) => beats.find((b) => b.beat.stage?.[l] === st)?.key;
  return { beats, events, focus, hero, brand, agents, inciting, crisis, climax, firstShowing, lastShowing, focusTurn, focusLevel, between, label };
}

/** A beat's own label for this lens's stage, when it has one. */
export function labelled(s: Structure, lensName: string): StageMap {
  const map: StageMap = {};
  for (const b of s.beats) {
    const st = b.beat.stage?.[lensName];
    if (st) (map[st] ??= []).push(b.key);
  }
  return map;
}

export const finding = (check: string, level: Level, about: string, says: string, fix?: string): Finding => ({ check, level, about, says, fix });

/** A lens's map with the beats' own labels winning, and every stage nobody labelled held between the labelled stages
 *  around it: an inferred beat out of that order is dropped, and a stretch (inferred as several beats) left empty is
 *  filled with the beats between its labelled neighbours. */
export function ordered(stages: string[], inferred: StageMap, own: StageMap, s: Structure): StageMap {
  const idx = (k: string) => s.beats.find((b) => b.key === k)?.index ?? -1;
  const out: StageMap = { ...inferred, ...own };
  if (!Object.keys(own).length) return out;
  const labelledAt = new Set(Object.values(own).flat());
  stages.forEach((st, i) => {
    if (own[st]) return;
    const before = stages.slice(0, i).flatMap((x) => own[x] ?? []).map(idx);
    const after = stages.slice(i + 1).flatMap((x) => own[x] ?? []).map(idx);
    const lo = before.length ? Math.max(...before) : -1, hi = after.length ? Math.min(...after) : s.beats.length;
    const had = inferred[st] ?? [];
    const kept = had.filter((k) => idx(k) > lo && idx(k) < hi);
    out[st] = kept.length || had.length < 2 || !before.length || !after.length ? kept : s.beats.filter((b) => b.index > lo && b.index < hi && !labelledAt.has(b.key)).map((b) => b.key);
  });
  return out;
}

/** Read a story told one way through lenses (the story's own `lenses`, else all given). */
export function readThrough(s: AnyStory, lenses: Lens[], telling?: string, seed = 0): LensResult[] {
  const w = analyze(s, telling, seed);
  const st = structure(w);
  return lenses.map((l) => {
    const r0 = l.read(w, st);
    const r = { ...r0, findings: unmeant(w, r0.findings) };
    const own = labelled(st, l.name);
    const map = ordered(l.stages, r.map, own, st);
    return { lens: l.name, ...r, map, passes: !r.findings.some((f) => f.level === "error") };
  });
}
