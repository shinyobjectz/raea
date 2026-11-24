// Generation without a fixed answer (docs/story.feature ST4a). A story with choice points is a space of stories; a seed
// picks one. sample() walks seeds from the one given until a story passes (no finding at the level refused, in the
// exact checks and in every lens given), so a sampled story is always one that holds together. samples() gives several
// different ones. matrix() runs one story pattern across many premises and says, per premise, how often it holds.
import { checkWorld, analyze, unmeant, type Finding, type Level } from "./checks.ts";
import { structure, type Lens } from "./lens.ts";
import type { AnyStory } from "./model.ts";
import { resolve } from "./resolve.ts";

export type SampleOptions = { seed?: number; telling?: string; lenses?: Lens[]; refuse?: Level; tries?: number };
export type Sampled = { story: AnyStory; seed: number; findings: Finding[]; passes: boolean };

const ranks: Record<Level, number> = { error: 0, warn: 1, note: 2 };

/** Every finding for one seed: the exact checks and each lens's. */
export function judge(s: AnyStory, seed: number, telling?: string, lenses: Lens[] = []): Finding[] {
  const w = analyze(s, telling, seed);
  const st = structure(w);
  return [...checkWorld(w), ...unmeant(w, lenses.flatMap((l) => l.read(w, st).findings))];
}

/** One story from the space that passes, starting at `seed`; the best found when none passes in `tries`. */
export function sample(s: AnyStory, o: SampleOptions = {}): Sampled {
  const { seed = 0, telling, lenses = [], refuse = "error", tries = 64 } = o;
  let best: Sampled | undefined;
  for (let i = 0; i < tries; i++) {
    const sd = seed + i;
    const findings = judge(s, sd, telling, lenses);
    const bad = findings.filter((f) => ranks[f.level] <= ranks[refuse]).length;
    const got: Sampled = { story: resolve(s, sd), seed: sd, findings, passes: bad === 0 };
    if (got.passes) return got;
    if (!best || bad < best.findings.filter((f) => ranks[f.level] <= ranks[refuse]).length) best = got;
  }
  return best!;
}

/** Up to n different passing stories (different once resolved), from seeds counted up from `seed`. */
export function samples(s: AnyStory, n: number, o: SampleOptions = {}): Sampled[] {
  const out: Sampled[] = [];
  const seen = new Set<string>();
  let seed = o.seed ?? 0;
  for (let guard = 0; out.length < n && guard < n * 32; guard++) {
    const got = sample(s, { ...o, seed });
    seed = got.seed + 1;
    if (!got.passes) break;
    const key = JSON.stringify(got.story);
    if (!seen.has(key)) { seen.add(key); out.push(got); }
  }
  return out;
}

export type MatrixRow = { theme: string; seed: number; passes: boolean; findings: Finding[] };
export type MatrixResult = { rows: MatrixRow[]; byTheme: Record<string, { passed: number; of: number; checks: Record<string, number> }> };

/** One story pattern across many premises: how often it holds for each, and which checks fail where. */
export function matrix<T>(pattern: (theme: T) => AnyStory, themes: Record<string, T>, o: SampleOptions & { seeds?: number[] } = {}): MatrixResult {
  const seeds = o.seeds ?? [0, 1, 2, 3, 4];
  const refuse = o.refuse ?? "error";
  const rows: MatrixRow[] = [];
  const byTheme: MatrixResult["byTheme"] = {};
  for (const [name, theme] of Object.entries(themes)) {
    const s = pattern(theme);
    const agg = (byTheme[name] = { passed: 0, of: 0, checks: {} as Record<string, number> });
    for (const seed of seeds) {
      const findings = judge(s, seed, o.telling, o.lenses ?? []);
      const bad = findings.filter((f) => ranks[f.level] <= ranks[refuse]);
      rows.push({ theme: name, seed, passes: bad.length === 0, findings });
      agg.of++;
      if (!bad.length) agg.passed++;
      for (const f of bad) agg.checks[f.check] = (agg.checks[f.check] ?? 0) + 1;
    }
  }
  return { rows, byTheme };
}
