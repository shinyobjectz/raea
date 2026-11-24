// Resolving choice points with a seed (docs/story.feature ST4a). The story is walked in a fixed order (keys as written),
// and each choice point draws from one seeded generator, so the same seed always gives the same story and a story with
// no choices is returned as it is (copied).
import { isChoice, type AnyStory } from "./model.ts";

/** mulberry32: small, fast, and the same in every engine. */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** One option of a choice, by its weights (equal when none). */
export function pick<T>(options: readonly T[], weights: readonly number[] | undefined, r: () => number): T {
  const ws = options.map((_, i) => Math.max(0, weights?.[i] ?? 1));
  const total = ws.reduce((a, b) => a + b, 0);
  if (total <= 0) return options[0];
  let x = r() * total;
  for (let i = 0; i < options.length; i++) if ((x -= ws[i]) < 0) return options[i];
  return options[options.length - 1];
}

function walk(x: unknown, r: () => number): unknown {
  if (isChoice(x)) return walk(pick(x.choice, x.weights, r), r);
  if (Array.isArray(x)) return x.map((v) => walk(v, r)).filter((v) => v !== null); // a choice of null drops the item
  if (x && typeof x === "object") return Object.fromEntries(Object.entries(x).map(([k, v]) => [k, walk(v, r)]));
  return x;
}

/** The story with every choice point picked, for this seed. */
export function resolve<S extends AnyStory>(s: S, seed = 0): S {
  return walk(s, rng(seed)) as S;
}

/** Whether a story still holds choice points. */
export function hasChoices(x: unknown): boolean {
  if (isChoice(x)) return true;
  if (Array.isArray(x)) return x.some(hasChoices);
  if (x && typeof x === "object") return Object.values(x).some(hasChoices);
  return false;
}
