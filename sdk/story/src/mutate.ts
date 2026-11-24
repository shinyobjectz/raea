// Mutation (docs/story.feature ST6a): break a valid story in one known way and say exactly what was broken and where.
// Because a story is data, these labels are exact and free, as many as wanted: the training rows for a critic. Some
// breaks the exact checks find by construction (a dropped setup); others they can't (two beats swapped so the story
// drags), and those are what a learned critic is for. `expect` names the check that should find it, when one should.
import { listOf, type AnyStory } from "./model.ts";
import { rng } from "./resolve.ts";
import { resolved, type RBeat, type REvent, type RStory } from "./load.ts";

export type Kind =
  | "drop-setup" | "drop-payoff" | "drop-cause" | "early-need" | "flatten-beat" | "drop-proof" | "swap-beats" | "drop-cause-beat" | "orphan-event";
export type Label = { kind: Kind; at: string; says: string; expect?: string };
export type Mutant = { story: AnyStory; telling: string; label: Label };

type Tell = RBeat[];
type Ev = REvent;
const clone = <T>(x: T): T => JSON.parse(JSON.stringify(x)) as T;

/** Every way this story told this way can be broken, each as one mutant. */
export function mutants(s: AnyStory, telling?: string, seed = 0): Mutant[] {
  const base: RStory = resolved(s, seed);
  const t = telling ?? Object.keys(base.tellings)[0];
  const out: Mutant[] = [];
  const beats = (): Tell => clone(base.tellings[t]);
  const withTelling = (b: Tell, world = base.world): AnyStory => ({ ...clone(base), world: clone(world), tellings: { ...clone(base.tellings), [t]: b } }) as unknown as AnyStory;
  const keys = Object.keys(base.world);
  const bk = (i: number, b: Tell = base.tellings[t]) => b[i]?.id ?? `b${i}`;

  base.tellings[t].forEach((beat, i) => {
    for (const p of listOf(beat.setup)) {
      if (!base.tellings[t].some((x, j) => j > i && listOf(x.pays).includes(p))) continue;
      const b = beats(); b[i] = { ...b[i], setup: listOf(b[i].setup).filter((x) => x !== p) };
      out.push({ story: withTelling(b), telling: t, label: { kind: "drop-setup", at: bk(i), says: `the setup "${p}" was removed`, expect: "setup" } });
    }
    for (const p of listOf(beat.pays)) {
      const b = beats(); b[i] = { ...b[i], pays: listOf(b[i].pays).filter((x) => x !== p) };
      out.push({ story: withTelling(b), telling: t, label: { kind: "drop-payoff", at: bk(i), says: `the payoff of "${p}" was removed`, expect: "setup" } });
    }
    const turning = beat.shows.filter((k) => Object.values(base.world[k]?.turns ?? {}).some((d) => d));
    if (turning.length && !base.tellings[t].some((x, j) => j !== i && x.shows.some((k) => turning.includes(k)))) {
      const world = clone(base.world);
      for (const k of turning) world[k] = { ...world[k], turns: {} } as Ev;
      out.push({ story: withTelling(beats(), world), telling: t, label: { kind: "flatten-beat", at: bk(i), says: `${bk(i)}'s events no longer turn anything`, expect: "turns" } });
    }
    if (i + 1 < base.tellings[t].length) {
      const b = beats(); [b[i], b[i + 1]] = [b[i + 1], b[i]];
      out.push({ story: withTelling(b), telling: t, label: { kind: "swap-beats", at: bk(i), says: `${bk(i)} and ${bk(i + 1)} were swapped` } });
    }
  });

  keys.forEach((k, i) => {
    const ev = base.world[k];
    if (i > 0 && ev.because !== undefined && !ev.routine) {
      const world = clone(base.world); world[k] = { ...world[k], because: undefined } as Ev;
      out.push({ story: withTelling(beats(), world), telling: t, label: { kind: "drop-cause", at: k, says: `${k} lost its cause`, expect: "cause" } });
    }
    const later = keys.slice(i + 1).flatMap((l) => base.world[l].sets ?? []).find((f) => !(ev.needs ?? []).includes(f) && !(base.agents[ev.who]?.believes ?? []).includes(f));
    if (later) {
      const world = clone(base.world); world[k] = { ...world[k], needs: [...(ev.needs ?? []), later] } as Ev;
      out.push({ story: withTelling(beats(), world), telling: t, label: { kind: "early-need", at: k, says: `${ev.who} now acts in ${k} on "${later}" before it's true`, expect: "belief" } });
    }
    if (ev.claim !== undefined && (ev.proof ?? []).length) {
      const world = clone(base.world); world[k] = { ...world[k], proof: [] } as Ev;
      out.push({ story: withTelling(beats(), world), telling: t, label: { kind: "drop-proof", at: k, says: `${k}'s claim lost its proof`, expect: "claim" } });
    }
  });

  // a beat that shows the only sight of a cause of a later big turn, dropped
  base.tellings[t].forEach((beat, i) => {
    const b = beats().filter((_, j) => j !== i);
    const shownLater = new Set(b.slice(i).flatMap((x) => x.shows));
    const causeOfLater = beat.shows.some((k) => [...shownLater].some((l) => listOf(base.world[l]?.because).includes(k)));
    if (i > 0 && causeOfLater && !b.some((x) => x.shows.some((k) => beat.shows.includes(k))) && !b.some((x) => listOf(x.pays).some((p) => listOf(beat.setup).includes(p))))
      out.push({ story: withTelling(b), telling: t, label: { kind: "drop-cause-beat", at: bk(i), says: `${bk(i)}, the only sight of a cause, was cut`, expect: "and-then" } });
  });

  const world = clone(base.world);
  const who = Object.keys(base.agents)[0];
  world["orphan"] = { who, does: "does something unrelated", turns: {} } as Ev;
  const b = beats();
  const at = Math.max(1, Math.floor(b.length / 2));
  b.splice(at, 0, { shows: ["orphan"] });
  out.push({ story: withTelling(b, world), telling: t, label: { kind: "orphan-event", at: bk(at, b), says: "an uncaused event that turns nothing was inserted", expect: "cause" } });
  return out;
}

/** A random selection of mutants, for training rows. */
export function sampleMutants(s: AnyStory, n: number, seed = 0, telling?: string): Mutant[] {
  const all = mutants(s, telling, seed);
  const r = rng(seed + 1);
  return Array.from({ length: Math.min(n, all.length) }, () => all.splice(Math.floor(r() * all.length), 1)[0]);
}
