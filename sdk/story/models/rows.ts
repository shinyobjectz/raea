// Training rows for the three story models (docs/story.feature ST6b), from the checked corpus (corpus.ts). The split is
// by story (a story's rows are all in one of train, val and test), so a held-out number is on stories never seen.
//
//   extract   script prose -> the story as data (no onScreen or lines in the answer: they're the input)
//   generate  the story's state before a cut beat and the beats around it (src/state.ts windowAt) -> that beat, as
//             JSON under the story's own beat schema
//   critic    script prose -> where it breaks: [{beat, check, level}], labelled exactly by checkStory on the structure
//             behind the prose. Negatives are cuts a viewer could see: a beat cut, two beats swapped, a beat from
//             another story spliced in. A cut that breaks nothing is labelled clean, so the critic must read, not count.
//
//   bun models/rows.ts [--in ~/.vd/story] [--out ~/.vd/story/rows]
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { beatSchema, rng, storySchema, windowAt } from "../src/index.ts";
import { bare, breaks, CHUNK, split, chunkSteps, criticSchema, name, prose, stepSchema } from "./lib.ts";

const arg = (k: string, d: string) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const IN = arg("--in", join(homedir(), ".vd/story"));
const OUT = arg("--out", join(IN, "rows"));
mkdirSync(OUT, { recursive: true });

type J = Record<string, any>;
const stories: { i: number; s: J; premise: J }[] = readFileSync(join(IN, "stories.jsonl"), "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l)).map((d) => ({ i: d.i, s: d.story, premise: d.premise }));

/** The split a story falls in, by its index. */

const out: Record<string, Record<string, J[]>> = { extract: {}, steps: {}, generate: {}, critic: {} };
const push = (task: string, sp: string, row: J) => ((out[task][sp] ??= []).push(row));
let clean = 0, broken = 0;

for (const { i, s } of stories) {
  const sp = split(i);
  const beats: J[] = s.tellings.main;
  if (!beats?.length) continue;
  const r = rng(i * 7919 + 17);
  const p = prose(beats);
  push("extract", sp, { i, kind: "script", prompt: p, answer: JSON.stringify(bare(s)) });
  // a transcript has the words alone: the same story from its lines, as a launch film's speech reads
  const spoken = beats.flatMap((b) => (b.says ?? []).map((l: J) => `${name(l.who)}: ${l.says}`));
  // steps: the windowed extractor, a chunk at a time with what earlier chunks found (story_models.py chain runs it end to end)
  for (const [j, st] of chunkSteps(s).entries()) push("steps", sp, { i, at: j, of: Math.ceil(beats.length / CHUNK), prompt: st.prompt, script: st.script, answer: JSON.stringify(st.answer), gold: j === 0 ? JSON.stringify(bare(s)) : undefined });
  if (spoken.length >= 2) push("extract", sp, { i, kind: "lines", prompt: spoken.join("\n"), answer: JSON.stringify(bare(s)) });

  // generate, windowed: the story's state before the cut beat and the beats around it (windowAt), never the whole
  // telling, so a step reads the same amount at beat 5 or beat 500. Every beat on test and val; three at random on train.
  const cuts = sp === "train" ? [...new Set([0, 1, 2].map(() => Math.floor(r() * beats.length)))] : beats.map((_, k) => k);
  for (const k of cuts) {
    const partial = { ...s, tellings: { main: beats.filter((_, n) => n !== k) } };
    push("generate", sp, { i, at: k, prompt: windowAt(s, "main", k), story: partial, answer: JSON.stringify(beats[k]), schema: beatSchema(s) });
  }

  // critic: the story as written, then cuts a viewer could see
  const variants: { kind: string; beats: J[]; world?: J }[] = [{ kind: "clean", beats }];
  const k = Math.floor(r() * beats.length);
  if (beats.length > 2) variants.push({ kind: "cut", beats: beats.filter((_, n) => n !== k) });
  if (beats.length > 2) { const a = Math.floor(r() * (beats.length - 1)); const b2 = [...beats]; [b2[a], b2[a + 1]] = [b2[a + 1], b2[a]]; variants.push({ kind: "swap", beats: b2 }); }
  const other = stories[Math.floor(r() * stories.length)];
  if (other.i !== i && other.s.tellings?.main?.length) {
    const ob = other.s.tellings.main[Math.floor(r() * other.s.tellings.main.length)];
    const firstAgent = Object.keys(s.agents)[0];
    const ev = `spliced${other.i}`;
    const at = 1 + Math.floor(r() * (beats.length - 1));
    const b2 = [...beats.slice(0, at), { shows: [ev], onScreen: ob.onScreen, says: (ob.says ?? []).map((l: J) => ({ ...l, who: "narrator" })) }, ...beats.slice(at)];
    variants.push({ kind: "splice", beats: b2, world: { ...s.world, [ev]: { who: firstAgent, does: "something unrelated happens" } } });
  }
  for (const v of variants) {
    const base = v.world ? { ...s, world: v.world } : s;
    let label: J[];
    try { label = breaks(base, v.beats); } catch { continue; }
    label.length ? broken++ : clean++;
    push("critic", sp, { i, kind: v.kind, prompt: prose(v.beats), answer: JSON.stringify({ breaks: label }) });
  }
}

writeFileSync(join(OUT, "schemas.json"), JSON.stringify({ extract: storySchema(), steps: stepSchema(storySchema()), critic: criticSchema }, null, 1));
for (const [task, sps] of Object.entries(out)) {
  mkdirSync(join(OUT, task), { recursive: true });
  for (const [sp, rows] of Object.entries(sps)) writeFileSync(join(OUT, task, `${sp}.jsonl`), rows.map((x) => JSON.stringify(x)).join("\n") + "\n");
  console.log(task, Object.fromEntries(Object.entries(sps).map(([k, v]) => [k, v.length])));
}
console.log(`critic labels: ${clean} clean, ${broken} with a break`);
