// Rows for the story decision model (docs/story.md, "Story as decisions"): a beat's window as the state, typed
// questions about that beat (src/questions.ts), and their exact answers. Each corpus story is read as written and with
// one beat cut and one pair swapped (where the checks find breaks); windows are story lines or a plain script.
//   train: three beats of each telling, lines or script at random; val, test: up to eight beats, both forms
//   bun models/decide-rows.ts [--in ~/.vd/story] [--out ~/.vd/story/rows/decide]
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { asker, fromData, rng, sampleMutants } from "../src/index.ts";
import { split } from "./lib.ts";

type J = Record<string, any>;
const arg = (k: string, d: string) => { const i = process.argv.indexOf(k); return (i > 0 ? process.argv[i + 1] : d).replace(/^~/, homedir()); };
const IN = arg("--in", join(homedir(), ".vd/story")), OUT = arg("--out", join(homedir(), ".vd/story/rows/decide"));
const stories = readFileSync(join(IN, "stories.jsonl"), "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l));
const out: Record<string, J[]> = {};
const yes: Record<string, number> = {};
for (const { i, story } of stories) {
  if (fromData(story).errors.length) continue;
  const sp = split(i), r = rng(i * 7919 + 3);
  const beats: J[] = story.tellings.main;
  const variants: [string, J[]][] = [["clean", beats]];
  if (beats.length > 3) {
    const c = Math.floor(r() * beats.length); variants.push([`cut${c + 1}`, beats.filter((_, n) => n !== c)]);
    const w = Math.floor(r() * (beats.length - 1)); const b2 = [...beats]; [b2[w], b2[w + 1]] = [b2[w + 1], b2[w]]; variants.push([`swap${w + 1}`, b2]);
  }
  // and two mutants (src/mutate.ts): a cause broken, a fact known early, a setup dropped ... where cuts can't reach
  const muts = (() => { try { return sampleMutants(fromData(story).story!, 2, i, "main"); } catch { return []; } })();
  const all: [string, J][] = [...variants.map(([k, bs]) => [k, { ...story, tellings: { main: bs } }] as [string, J]), ...muts.map((m) => [`mut-${m.label.kind}`, m.story as unknown as J] as [string, J])];
  for (const [kind, st] of all) {
    const bs: J[] = (st.tellings as J).main;
    let ask;
    try { ask = asker(st, "main"); } catch { continue; }
    const ks = bs.map((_, k) => k);
    const pick = sp === "train" ? [...new Set([0, 1, 2].map(() => ks[Math.floor(r() * ks.length)]))] : ks.slice(0, 8);
    for (const k of pick) for (const as of sp === "train" ? [r() < 0.5 ? "lines" : "script"] : ["lines", "script"]) {
      const q = ask(k, as as "lines" | "script");
      (out[sp] ??= []).push({ i, kind, at: k, as, ...q });
      if (sp === "train") for (const [id, v] of Object.entries(q.answers)) if (v === true) yes[id] = (yes[id] ?? 0) + 1;
    }
  }
}
mkdirSync(OUT, { recursive: true });
for (const [sp, rows] of Object.entries(out)) writeFileSync(join(OUT, `${sp}.jsonl`), rows.map((x) => JSON.stringify(x)).join("\n") + "\n");
console.log(Object.entries(out).map(([sp, rows]) => `${sp} ${rows.length}`).join(", "), "->", OUT);
console.log("yes answers on train:", JSON.stringify(yes));
