// Value curves (docs/story.feature ST1a). In the world, each event moves the values it turns, in story order, so every
// event has the world's level after it. In a telling, a beat's level is the world's level after the latest event it
// shows (a flash-forward shows a later level, a flashback an earlier one), and its turn is the sum of what its events
// turn. Beats get "Turns", "Level" and "Continues" (events the beat before showed too, counted once); the world gets "Curves", each value's levels over the telling and its shape.
import type { World } from "../ecs.ts";
import { beatsOf, eventsOf, keysOf, type REvent } from "../load.ts";

export type Shape = "flat" | "rise" | "fall" | "fall-rise" | "rise-fall" | "rise-fall-rise" | "fall-rise-fall" | "complex";
export type Curve = { levels: number[]; shape: Shape };

const SHAPES: Record<string, Shape> = { "": "flat", "+": "rise", "-": "fall", "-+": "fall-rise", "+-": "rise-fall", "+-+": "rise-fall-rise", "-+-": "fall-rise-fall" };

/** The shape of a series: the signs of its moves, runs of one sign merged (Reagan et al.'s six arcs, and flat). */
export function shapeOf(levels: number[]): Shape {
  let signs = "";
  for (let i = 1; i < levels.length; i++) {
    const d = levels[i] - levels[i - 1];
    if (d === 0) continue;
    const s = d > 0 ? "+" : "-";
    if (!signs.endsWith(s)) signs += s;
  }
  return SHAPES[signs] ?? "complex";
}

export function values(w: World): void {
  const names = Object.keys(keysOf(w).values);
  const level: Record<string, number> = Object.fromEntries(names.map((n) => [n, 0]));
  const after: Record<string, Record<string, number>> = {};
  for (const { e, key, ev } of eventsOf(w).sort((a, b) => a.order - b.order)) {
    for (const [v, d] of Object.entries((ev as REvent).turns ?? {})) level[v] = (level[v] ?? 0) + (d ?? 0);
    after[key] = { ...level };
    w.insert(e, "LevelAfter", after[key]);
  }
  const start: Record<string, number> = Object.fromEntries(names.map((n) => [n, 0]));
  let last = start;
  const series: Record<string, number[]> = Object.fromEntries(names.map((n) => [n, [0]]));
  const order = Object.fromEntries(eventsOf(w).map((x) => [x.key, x.order]));
  // a film tells one event over several shots: an event the beat before also showed continues there, and its turn
  // counts once, at the first shot of the run (a hook that shows the ending and a return to it later both count)
  let prev = new Set<string>();
  for (const { e, beat } of beatsOf(w)) {
    const turns: Record<string, number> = {};
    const continued = beat.shows.filter((k) => prev.has(k));
    prev = new Set(beat.shows);
    w.insert(e, "Continues", continued);
    for (const k of beat.shows.filter((x) => !continued.includes(x))) for (const [v, d] of Object.entries(eventsOf(w).find((x) => x.key === k)?.ev.turns ?? {})) turns[v] = (turns[v] ?? 0) + (d ?? 0);
    const latest = [...beat.shows].sort((a, b) => (order[b] ?? -1) - (order[a] ?? -1))[0];
    const lvl = latest !== undefined && after[latest] ? after[latest] : last;
    w.insert(e, "Turns", turns);
    w.insert(e, "Level", lvl);
    for (const n of names) series[n].push(lvl[n]);
    last = lvl;
  }
  w.setResource<Record<string, Curve>>("Curves", Object.fromEntries(names.map((n) => [n, { levels: series[n], shape: shapeOf(series[n]) }])));
}
