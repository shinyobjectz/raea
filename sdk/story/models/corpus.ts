// The story corpus (docs/story.feature ST6a): stories written as raea/story JSON by a cheap model, across a matrix of
// kinds and made-up premises, each read back with fromData and checked with checkStory. A story with errors gets one
// repair with its findings; what still fails is kept apart (rejects.jsonl, the critic's real-world negatives). Every
// beat carries the words a viewer gets (onScreen and lines), so a telling reads as a script with no annotations: what
// the extractor reads and the critic judges.
//
//   bun models/corpus.ts N [--out ~/.vd/story] [--model anthropic/claude-haiku-5.5] [--jobs 16] [--seed 0] [--long]
import { appendFileSync, existsSync, mkdirSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { checkStory, fromData, rng, toData, type AnyStory } from "../src/index.ts";
import { launch } from "../test/fixture.ts";

const arg = (k: string, d: string) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const N = Number(process.argv[2] ?? 10);
const OUT = arg("--out", join(homedir(), ".vd/story"));
const MODEL = arg("--model", "anthropic/claude-haiku-5.5");
const JOBS = Number(arg("--jobs", "16"));
const SEED = Number(arg("--seed", "0"));
const KEY = readFileSync(join(homedir(), ".raea/env"), "utf8").match(/^OPENROUTER_API_KEY=(.+)$/m)?.[1]?.trim();
if (!KEY) throw new Error("no OPENROUTER_API_KEY in ~/.raea/env");
mkdirSync(OUT, { recursive: true });

// made-up names only (never a real brand)
const A = ["Halden", "Northwind", "Lumen", "Bramble", "Quillfield", "Tidewell", "Ferrow", "Ashgrove", "Calloway", "Merrin", "Oxley", "Pellam", "Rookery", "Saltmarsh", "Thistledown", "Umberly", "Vantor", "Wrenfield", "Yarrowby", "Zephyrine", "Cobblestone", "Driftmark", "Emberline", "Foxhollow", "Glimmerton", "Hearthway", "Ironvale", "Juniper Row", "Kestrel", "Larkspur"];
const B = { product: ["Outdoor", "Coffee", "Skincare", "Bikes", "Audio", "Notes", "Tea", "Running", "Garden", "Pet Food", "Sleep", "Bank", "Studio", "Analytics", "Kitchen", "Robotics", "Maps", "Language", "Insurance", "Fitness"] };
const KINDS: [string, number, string][] = [
  ["launch film for a software product", 30, "30 to 45 seconds, 4 to 7 beats; the customer is the hero, the product the mentor; a claim backed by proof"],
  ["launch film for a physical product", 15, "30 to 45 seconds, 4 to 7 beats; the customer is the hero, the product the mentor or gift"],
  ["30-second brand ad", 15, "3 to 6 beats; one value turns; a person's problem, agitated, then solved"],
  ["short drama scene", 12, "4 to 8 beats; two or three characters, a want against a need, a dilemma, a secret the audience learns before a character does"],
  ["game quest", 10, "4 to 8 beats; a player character, a quest giver, an obstacle, a reward; setups paid off"],
  ["explainer film", 8, "4 to 6 beats; a question raised early and answered late"],
  ["fable", 5, "4 to 7 beats; animals or objects; a lie the hero believes and drops"],
  ["documentary short", 5, "4 to 7 beats; real-feeling people, a turn from bad to better or worse"],
];
// --long: films told a beat a shot, as a short film's shot list is (the shape of a held-out story like models/validate.ts
// reads): one event can span several shots in a row, so the long telling is 20 to 40 beats over 12 to 30 events
const LONG: [string, number, string][] = [
  ["wordless animated short film, told a beat a shot", 40, "20 to 40 beats, a beat a shot; 12 to 30 events; one event can span several shots in a row; few or no lines; a journey with a mentor, a threshold, an ordeal, a reward and a return"],
  ["short live-action drama, told a beat a shot", 30, "20 to 35 beats, a beat a shot; 12 to 25 events; a want against a need, a secret, a dilemma late"],
  ["game's story mission, told a beat a shot", 15, "20 to 30 beats; 12 to 20 events; a quest giver, an obstacle, a betrayal, a reward; setups paid off"],
  ["long brand film, told a beat a shot", 15, "20 to 30 beats; 12 to 20 events; the customer is the hero, the product the mentor; a claim backed by proof shown"],
];
const LONG_RUN = process.argv.includes("--long");
if (LONG_RUN) KINDS.splice(0, KINDS.length, ...LONG);
const MOODS = ["hopeful", "funny", "tense", "quiet", "bittersweet", "urgent", "warm", "dry", "eerie", "triumphant"];

function premise(i: number): { kind: string; shape: string; brand: string; mood: string } {
  const r = rng(SEED * 100003 + i);
  const total = KINDS.reduce((a, k) => a + k[1], 0);
  let x = r() * total, k = KINDS[0];
  for (const c of KINDS) { if ((x -= c[1]) < 0) { k = c; break; } }
  const brand = `${A[Math.floor(r() * A.length)]} ${B.product[Math.floor(r() * B.product.length)]}`;
  return { kind: k[0], shape: k[2], brand, mood: MOODS[Math.floor(r() * MOODS.length)] };
}

const GUIDE = `You write stories as data for raea/story. A story is one JSON object:
- values: what's at stake, each {neg, pos} (McKee's value, e.g. {"neg":"buried","pos":"free"}).
- facts: named facts, each a sentence.
- agents: characters, each {want, need, lie, mask, role, believes}. mask is one of hero, mentor, ally, herald, threshold, shapeshifter, shadow, trickster, gift. For an ad or launch, role "customer" for the person and "brand" for the product.
- world: events IN THE ORDER THEY HAPPEN, each {who, with, does, because, needs, sets, unsets, learns, turns, ends, dilemma, claim, proof, routine}. Every event after the first names its cause in "because" (an earlier event), unless it's "routine": true (how things are every day). "turns" moves values in whole steps from -3 to 3 (e.g. {"freedom": -2}); every event that matters turns something. "ends" settles an agent's want or need ({"hero": "need"}). A "dilemma" lists the two options a character chooses between. A "claim" made to the audience names its "proof" (facts that event sets).
- tellings: one telling named "main": a list of beats in the order the audience sees them. Each beat: {shows: [event keys], feel, reveals, hides, asks, setup, pays, onScreen, says}. A beat can show an event out of order (a flash-forward hook). "asks" raises a question (a fact the audience wants); a later beat "reveals" it. "setup" plants a named setup; a later beat "pays" it. EVERY beat has "onScreen": one or two concrete sentences of what the viewer sees, and usually "says": lines [{who, says}] (who is an agent key or "narrator"). onScreen and says are the only words a viewer gets, so they must carry the story on their own.
- idea: {value, side, counter}: the value the story is about and what it argues.
Facts are KEYS: believes, needs, sets, unsets, learns, proof, reveals, hides and asks hold fact keys you declared in "facts", never sentences (a question the audience has is a fact key in asks, revealed later). Knowledge is tracked: an agent can only act on ("needs") a fact they believe from the start, or that an earlier event they were in (as who or with) set or learned.
Rules the checker holds you to: every beat turns a value or reveals something; each beat after the first follows from what was seen (a cause shown earlier or in the same beat); a big turn (2 or more) is set up; every setup is paid; a want or need is settled by the end; a claim has proof shown by then; the focus value ends somewhere different from where it began. Keys are short camelCase words. Use only the keys you declare.
Here is one valid story, a 30-second launch film:
${JSON.stringify(toData(launch as AnyStory))}
Answer with the JSON object only.`;

async function ask(messages: { role: string; content: string }[]): Promise<{ text: string; usd: number }> {
  for (let attempt = 0; attempt < 3; attempt++) {
    const res = await fetch("https://openrouter.ai/api/v1/chat/completions", {
      method: "POST",
      headers: { Authorization: `Bearer ${KEY}`, "Content-Type": "application/json", "X-Title": "story-corpus" },
      body: JSON.stringify({ model: MODEL, messages, temperature: 1, max_tokens: process.argv.includes("--long") ? 14000 : 9000, reasoning: process.argv.includes("--long") ? { enabled: false } : { max_tokens: 1500 }, response_format: { type: "json_object" }, usage: { include: true } }),
    });
    if (res.status === 429 || res.status >= 500) { await new Promise((r) => setTimeout(r, 2000 * (attempt + 1))); continue; }
    const j: any = await res.json();
    if (!res.ok) throw new Error(`${res.status} ${JSON.stringify(j).slice(0, 300)}`);
    const text = j.choices?.[0]?.message?.content ?? "";
    if (!text) appendFileSync(join(OUT, "empty.jsonl"), JSON.stringify({ finish: j.choices?.[0]?.finish_reason, native: j.choices?.[0]?.native_finish_reason, usage: j.usage, err: j.error }) + "\n");
    return { text, usd: j.usage?.cost ?? 0 };
  }
  throw new Error("openrouter kept refusing");
}

const parse = (t: string) => { try { return JSON.parse(t.replace(/^```(json)?|```$/g, "").trim()); } catch { return null; } };

function judgeIt(j: unknown) {
  const { story, errors } = fromData(j);
  if (!story) return { story, errors: errors.map((e) => `${e.at}: ${e.says}`), findings: [] as string[] };
  let findings: string[] = [];
  const n = ((j as any)?.tellings?.main ?? []).length, ev = Object.keys((j as any)?.world ?? {}).length;
  if (LONG_RUN && (n < 20 || ev < 12)) findings.push(`the telling has ${n} beats over ${ev} events; it needs 20 to 40 beats (a beat a shot) over 12 to 30 events`);
  if (!errors.length) {
    try { findings = findings.concat(checkStory(story, "main").filter((f) => f.level === "error").map((f) => `${f.check} at ${f.about}: ${f.says}`)); }
    catch (e) { findings = [`the checker couldn't read it: ${(e as Error).message}`]; }
  }
  return { story, errors: errors.map((e) => `${e.at}: ${e.says}`), findings };
}

let spent = 0, kept = 0, rejected = 0, done = 0;
const start = existsSync(join(OUT, "stories.jsonl")) ? readFileSync(join(OUT, "stories.jsonl"), "utf8").split("\n").filter(Boolean).length + (existsSync(join(OUT, "rejects.jsonl")) ? readFileSync(join(OUT, "rejects.jsonl"), "utf8").split("\n").filter(Boolean).length : 0) : 0;

async function one(i: number) {
  const p = premise(i);
  const msgs = [
    { role: "system", content: GUIDE },
    { role: "user", content: `Write a ${p.mood} ${p.kind}${p.kind.includes("launch") || p.kind.includes("ad") ? ` for ${p.brand} (a made-up company: invent what it makes, in its line)` : ""}. Shape: ${p.shape}. Make it specific and surprising, not generic.${LONG_RUN ? " This is a long telling: tellings.main must have between 20 and 40 beats, one per shot (number them in your head as you go), over 12 to 30 world events. A run of shots can show the same event." : ""}` },
  ];
  const first = await ask(msgs);
  spent += first.usd;
  let j = parse(first.text), r = judgeIt(j), repaired = false;
  if (!j) appendFileSync(join(OUT, "unparsed.txt"), `--- ${i}\n${first.text.slice(0, 400)}\n...\n${first.text.slice(-300)}\n`);
  const convo = [...msgs, { role: "assistant", content: first.text }];
  for (let round = 0; round < 2 && j && (r.errors.length || r.findings.length); round++) {
    convo.push({ role: "user", content: `The checker found:\n${[...r.errors, ...r.findings].join("\n")}\nFix every one and answer with the whole corrected JSON object only.` });
    const fix = await ask(convo);
    spent += fix.usd;
    convo.push({ role: "assistant", content: fix.text });
    const j2 = parse(fix.text), r2 = judgeIt(j2);
    appendFileSync(join(OUT, "repairs.jsonl"), JSON.stringify({ i, round, before: j, found: [...r.errors, ...r.findings], after: j2, fixed: !!j2 && !r2.errors.length && !r2.findings.length }) + "\n");
    j = j2; r = r2; repaired = true;
  }
  const ok = !!j && !r.errors.length && !r.findings.length;
  appendFileSync(join(OUT, ok ? "stories.jsonl" : "rejects.jsonl"), JSON.stringify({ i, premise: p, repaired, model: MODEL, story: j, found: [...r.errors, ...r.findings] }) + "\n");
  ok ? kept++ : rejected++;
  if (++done % 10 === 0) console.log(`${done}/${N}: kept ${kept}, rejected ${rejected}, spent $${spent.toFixed(3)}`);
}

let next = start;
const end = start + N;
await Promise.all(Array.from({ length: JOBS }, async () => {
  while (next < end) {
    const i = next++;
    try { await one(i); } catch (e) { console.log(`story ${i}: ${(e as Error).message}`); }
  }
}));
console.log(`done: kept ${kept}, rejected ${rejected} of ${N}, spent $${spent.toFixed(3)} (${OUT})`);
