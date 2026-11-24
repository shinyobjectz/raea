// A held-out story through the whole stack (docs/story.feature ST6b): a story none of the models, lenses or checks were
// built on, written by someone else for a film of their own. It prints what the exact checks and every lens find,
// then writes rows in the models' own format so story_models.py (predict, frontier, onnx) and score.ts run on it as on
// the corpus's test split:
//   extract   the telling as prose -> the story (scored against the author's own story)
//   generate  each beat cut in turn -> the beat the model writes (does the story still hold?)
//   critic    the telling as written, then each beat cut and each pair of neighbours swapped, labelled exactly
//
//   bun models/validate.ts FILE.story.ts [--telling NAME] [--name NAME] [--out ~/.vd/story/validate]
// (a story module that imports raea/story by name needs a tsconfig mapping it: bun --tsconfig-override ...)
import { copyFileSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { basename, join, resolve } from "node:path";
import { beatSchema, checkStory, LENSES, readThrough, toData, windowAt, type AnyStory } from "../src/index.ts";
import { bare, breaks, CHUNK, chunkSteps, prose } from "./lib.ts";

type J = Record<string, any>;
const arg = (k: string, d?: string) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const FILE = resolve(process.argv[2] ?? "");
const story: AnyStory = (await import(FILE)).default;
const TELLING = arg("--telling") ?? Object.keys(story.tellings)[0];
const NAME = arg("--name") ?? basename(FILE).replace(/\.story\.ts$/, "");
const OUT = join(arg("--out", join(homedir(), ".vd/story/validate"))!.replace(/^~/, homedir()), NAME);

// the story as data with its telling as "main", the corpus's shape
const d = toData(story) as J;
const beats: J[] = d.tellings[TELLING];
const s: J = { ...d, tellings: { main: beats } };

console.log(`${d.title ?? NAME}: ${Object.keys(d.world).length} events, ${beats.length} beats in "${TELLING}", ${Object.keys(d.agents).length} agents, ${Object.keys(d.values).length} values`);
const exact = checkStory(story, TELLING);
console.log(`exact checks: ${exact.length} (${exact.filter((f) => f.level === "error").length} errors)`);
for (const f of exact) console.log(`  ${f.level.padEnd(5)} ${f.check.padEnd(10)} ${f.about}: ${f.says}`);
for (const r of readThrough(story, Object.values(LENSES), TELLING)) {
  const bad = r.findings.filter((f) => f.level !== "note");
  console.log(`lens ${r.lens.padEnd(14)} ${r.passes ? "passes" : "FAILS "}  ${bad.map((f) => `${f.check}@${f.about}`).join(" ")}`);
}

mkdirSync(OUT, { recursive: true });
for (const t of ["extract", "generate", "critic"]) mkdirSync(join(OUT, t), { recursive: true });
const rowsDir = join(homedir(), ".vd/story/rows");
if (existsSync(join(rowsDir, "schemas.json"))) copyFileSync(join(rowsDir, "schemas.json"), join(OUT, "schemas.json"));
const write = (task: string, rows: J[]) => writeFileSync(join(OUT, task, `${NAME}.jsonl`), rows.map((r) => JSON.stringify(r)).join("\n") + "\n");

for (const t of ["steps"]) mkdirSync(join(OUT, t), { recursive: true });
write("steps", chunkSteps(s).map((st, j) => ({ i: 0, at: j, of: Math.ceil(beats.length / CHUNK), prompt: st.prompt, script: st.script, answer: JSON.stringify(st.answer), gold: j === 0 ? JSON.stringify(bare(s)) : undefined })));
write("extract", [{ i: 0, kind: "script", prompt: prose(beats), answer: JSON.stringify(bare(s)) }]);
write("generate", beats.map((b, k) => ({ i: 0, at: k, prompt: windowAt(s, "main", k), story: { ...s, tellings: { main: beats.filter((_, n) => n !== k) } }, answer: JSON.stringify(b), schema: beatSchema(s) })));
const critic: J[] = [{ i: 0, kind: "clean", prompt: prose(beats), answer: JSON.stringify({ breaks: breaks(s, beats) }) }];
beats.forEach((_, k) => { const b2 = beats.filter((_, n) => n !== k); critic.push({ i: 0, kind: `cut${k + 1}`, prompt: prose(b2), answer: JSON.stringify({ breaks: breaks(s, b2) }) }); });
beats.slice(0, -1).forEach((_, k) => { const b2 = [...beats]; [b2[k], b2[k + 1]] = [b2[k + 1], b2[k]]; critic.push({ i: 0, kind: `swap${k + 1}`, prompt: prose(b2), answer: JSON.stringify({ breaks: breaks(s, b2) }) }); });
write("critic", critic);
const broken = critic.filter((r) => JSON.parse(r.answer).breaks.length).length;
console.log(`rows -> ${OUT}: extract 1, generate ${beats.length}, critic ${critic.length} (${broken} with a break, ${critic.length - broken} clean)`);
console.log(`next: python story_models.py predict TASK --rows ${OUT} --split ${NAME} --adapter ...; bun models/score.ts TASK --rows ${OUT} --split ${NAME}`);
