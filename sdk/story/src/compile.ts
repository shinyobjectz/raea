// Compilers (docs/story.feature ST5b): a telling into raea/brief's Script (the screenplay and A/V table the feed shows),
// the world into storylets with a drama manager for a game, and a telling's lines into Ink and Yarn Spinner.
import type { Script } from "raea/brief";
import { analyze } from "./checks.ts";
import { beatsOf, infoOf, resolved, type RBeat } from "./load.ts";
import { causesOf, listOf, type AnyStory } from "./model.ts";
import { rng } from "./resolve.ts";
import type { Audience } from "./systems/know.ts";

type ScriptBeat = Script["scenes"][number]["beats"][number];

/** What a beat shows, in words: its own onScreen, else its events' `does`. */
export const onScreenOf = (s: AnyStory, b: RBeat, seed = 0): string =>
  (b.onScreen as string | undefined) ?? b.shows.map((k) => resolved(s, seed).world[k]?.does as string).filter(Boolean).join(". ");

/** A telling as a Script: a scene a beat, its heading the beat's place and feeling, its payoff what it pays or answers. */
export function compileScript(s: AnyStory, telling?: string, seed = 0): Script & { beats: string[] } {
  const w = analyze(s, telling, seed);
  const info = infoOf(w);
  const scenes = beatsOf(w).map(({ e, key, beat }) => {
    const a = w.must<Audience>(e, "Audience");
    const lines = (beat.says ?? []).map((l) => ({ who: l.who, says: l.says as string, ...(l.how ? { how: l.how } : {}) }));
    const b: ScriptBeat = { onScreen: onScreenOf(s, beat, seed), ...(lines.length ? { lines } : {}), ...(beat.seconds ? { seconds: beat.seconds } : {}) };
    const pays = listOf(beat.pays), answered = a.answered;
    const payoff = [...pays.map((p) => `pays off ${p}`), ...answered.map((f) => `answers ${f}`)].join("; ");
    return { heading: `${key.toUpperCase()}${beat.feel ? ` · ${beat.feel}` : ""}`, beats: [b], ...(payoff ? { payoff } : {}), beat: key };
  });
  return { title: info.title ?? "Untitled", ...(info.premise ? { logline: info.premise } : {}), scenes, beats: beatsOf(w).map((b) => b.key) };
}

export type Storylet = { id: string; who: string; does: string; after: string[]; needs: string[]; sets: string[]; unsets: string[]; weight: number };

/** The world as storylets: each event playable once its causes have played and its agent holds what it needs. */
export function compileStorylets(s: AnyStory, seed = 0): Storylet[] {
  const r = resolved(s, seed);
  return Object.entries(r.world).map(([id, ev]) => ({
    id, who: ev.who, does: ev.does as string, after: causesOf(ev), needs: ev.needs ?? [], sets: ev.sets ?? [], unsets: ev.unsets ?? [],
    weight: Object.values(ev.turns ?? {}).reduce<number>((a, d) => a + Math.abs(d ?? 0), 0),
  }));
}

/** A drama manager: plays storylets one at a time, each step choosing among those available the one whose weight is
 *  nearest the target curve's next point (a target of 0 to 1 against the heaviest storylet), ties broken by the seed. */
export function dramaManager(storylets: Storylet[], target: number[], seed = 0): string[] {
  const done = new Set<string>(), facts = new Set<string>();
  const heaviest = Math.max(1, ...storylets.map((x) => x.weight));
  const r = rng(seed);
  const played: string[] = [];
  for (let step = 0; played.length < storylets.length; step++) {
    const open = storylets.filter((x) => !done.has(x.id) && x.after.every((a) => done.has(a)) && x.needs.every((f) => facts.has(f)));
    if (!open.length) break;
    const want = (target[Math.min(step, target.length - 1)] ?? 0.5) * heaviest;
    const best = open.map((x) => ({ x, d: Math.abs(x.weight - want) + r() * 1e-6 })).sort((a, b) => a.d - b.d)[0].x;
    done.add(best.id); played.push(best.id);
    for (const f of best.sets) facts.add(f);
    for (const f of best.unsets) facts.delete(f);
  }
  return played;
}

/** A telling's lines as Ink: a knot a beat, each line `who: says`, subtext as a comment. */
export function toInk(s: AnyStory, telling?: string, seed = 0): string {
  const w = analyze(s, telling, seed);
  const out: string[] = [];
  const bs = beatsOf(w);
  bs.forEach(({ key, beat }, i) => {
    out.push(`=== ${key} ===`);
    if (beat.feel) out.push(`// feel: ${beat.feel}`);
    out.push(onScreenOf(s, beat, seed) ? `// ${onScreenOf(s, beat, seed)}` : "");
    for (const l of beat.says ?? []) out.push(`${l.who}: ${l.says as string}${l.means ? ` // means: ${l.means}` : ""}`);
    out.push(i + 1 < bs.length ? `-> ${bs[i + 1].key}` : "-> END", "");
  });
  return out.filter((l, i, a) => !(l === "" && a[i - 1] === "")).join("\n");
}

/** A telling's lines as Yarn Spinner nodes. */
export function toYarn(s: AnyStory, telling?: string, seed = 0): string {
  const w = analyze(s, telling, seed);
  const bs = beatsOf(w);
  return bs.map(({ key, beat }, i) => [
    `title: ${key}`, ...(beat.feel ? [`tags: ${String(beat.feel).replace(/\s+/g, "-")}`] : []), "---",
    ...(beat.says ?? []).map((l) => `${l.who}: ${l.says as string}${l.means ? ` // means: ${l.means}` : ""}`),
    ...(i + 1 < bs.length ? [`<<jump ${bs[i + 1].key}>>`] : []), "===",
  ].join("\n")).join("\n");
}

