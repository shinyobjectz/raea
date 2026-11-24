// The same prompts with and without raea/story (docs/story.md). Three arms, one writer model:
//   A  plain        the prompt, a beat script back, one call
//   C  plain+edit   A, then a story editor's critique and rewrite in the writer's own head: the same knowledge the
//                   checks and the brief's lenses hold, said in words, with no machinery (one more call)
//   B  raea/story   the story as data under the guide; checkStory and the lenses find what's wrong; the writer
//                   repairs from the findings, up to two rounds; its beats become the same beat script
// Every arm ends in the same format (numbered beats: what's on screen, who says what), scored the same blind ways:
//   1. extract: another model reads the script (nothing else) back into story data, recording only what's shown,
//      and checkStory reads that: structure measured from the screen alone, not from the arm's own claims
//   2. judges: two models of other families rank the three scripts for a prompt, letters shuffled
//   D  raea/story as editor   A's script read back blind into story data; checkStory and the lenses find its holes,
//                             handed to the writer by beat number; the writer rewrites the script (C's round, with the
//                             editor's eye replaced by the exact checks)
//   E  raea/story, then write B's checked story as story lines; the writer writes the script from it as a screenwriter
//   bun bench/ab.ts [--samples 2] [--only id,id] [--out DIR] [--writer M] [--jobs 8] [--reuse RUN_DIR] [--arms A,C,B,D,E]
import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { checkStory, fromData, LENSES, readThrough, resolved, rng, scriptOf, toData, toLines, type AnyStory } from "../src/index.ts";
import { launch } from "../test/fixture.ts";

const arg = (k: string, d: string) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const SAMPLES = Number(arg("--samples", "2"));
const ONLY = arg("--only", "").split(",").filter(Boolean);
const OUT = arg("--out", join(homedir(), ".vd/story/ab", new Date().toISOString().slice(0, 16).replace(/[:T]/g, "-")));
const WRITER = arg("--writer", "anthropic/claude-sonnet-5.5");
const EXTRACTOR = arg("--extractor", "google/gemini-3.7-flash");
const JUDGES = arg("--judges", "openai/gpt-5.4,google/gemini-3.1-pro-preview").split(",");
const JOBS = Number(arg("--jobs", "8"));
const REUSE = arg("--reuse", "");
const ARMS = arg("--arms", "A,C,B,D,E").split(",");
const KEY = readFileSync(join(homedir(), ".raea/env"), "utf8").match(/^OPENROUTER_API_KEY=(.+)$/m)?.[1]?.trim();
if (!KEY) throw new Error("no OPENROUTER_API_KEY in ~/.raea/env");
mkdirSync(OUT, { recursive: true });

type J = Record<string, any>;
type Prompt = { id: string; brief: string; lenses: string[]; editor: string };
// made-up brands only. Each prompt names the frameworks it's held to; arm C's editor is told the same in words.
const PROMPTS: Prompt[] = [
  { id: "tent-launch", lenses: ["launch", "storynomics"], editor: "a launch film: the viewer's trouble first, the product named, the product doing its job, a proof, how to start; the customer is the hero, the product the mentor",
    brief: "A 30-second launch film for Halden Outdoor's new one-pull tent, for people who've never camped because setting up a tent scares them." },
  { id: "coffee-brand", lenses: ["storynomics", "pas"], editor: "a brand film: the customer is the hero with a problem; the brand is the mentor; a problem made to hurt, then solved, with proof",
    brief: "A 45-second brand film for Northwind Coffee, a small roaster whose beans are roasted the day they ship. Audience: people who drink bad office coffee." },
  { id: "whodunit", lenses: ["mckee"], editor: "a mystery in McKee's terms: an inciting incident, complications that progress, a crisis with a real dilemma, a climax; fair play: the detective may act only on clues the audience was shown",
    brief: "A 3-minute whodunit short in a seaside hotel. Fair play: an attentive viewer can name the culprit before the detective does." },
  { id: "kishotenketsu", lenses: ["kishotenketsu"], editor: "kishotenketsu: introduction, development, a twist that is a new point of view rather than a conflict, a reconciliation; no villain, no fight",
    brief: "A 1-minute wordless story with no conflict and no villain: a quiet turn of perspective in a city apartment block." },
  { id: "lighthouse-long", lenses: ["mckee", "campbell"], editor: "a short film in McKee's and Campbell's terms: an ordinary world, a call, a threshold, an ordeal, a reward, a return; a want against a need; a dilemma late; every setup paid",
    brief: "A 15-minute short film told in about 40 shots: a lighthouse keeper's daughter must decide whether to leave the island when the light is automated." },
  { id: "liar-quest", lenses: ["propp", "campbell"], editor: "a game quest in Propp's and Campbell's terms: a lack, a dispatcher, a departure, a test, a helper or magical agent, a struggle, a return; setups paid; a betrayal set up before it lands",
    brief: "A side quest for a fantasy RPG, 8 to 12 beats, where the quest giver is lying to the player from the start." },
  { id: "tragedy", lenses: ["mckee"], editor: "a tragedy: the hero gets what they want and loses what they need; a negative arc that is chosen, not accidental; every big turn set up",
    brief: "A 2-minute story where an ambitious young chef gets the restaurant review she wanted and loses the thing that mattered more." },
  { id: "skincare-15", lenses: ["pas", "launch"], editor: "a 15-second ad: problem, agitate, solve, proof; the product shown working on the viewer's problem",
    brief: "A 15-second ad for Lumen Skincare's mineral sunscreen that leaves no white cast, for people with dark skin who've given up on sunscreen." },
];

const PLAIN = `You are a screenwriter. Write the piece the brief asks for as a beat script: one JSON object
{"title": string, "beats": [{"onScreen": string, "says": [{"who": string, "says": string}], "seconds": number}]}.
Each beat is a shot or a short run of shots. "onScreen" is one or two concrete sentences of what the viewer sees; "says" is
any spoken lines (who is a character's name, or "narrator"); "seconds" is how long the beat runs. onScreen and says are all
the viewer gets, so they carry the story alone. Answer with the JSON object only.`;

const EDIT = (p: Prompt) => `Now read your script as a hard story editor would, then rewrite it. Hold it to ${p.editor}. Check, beat by beat:
- does each beat happen because of what came before ("therefore") or turn it ("but"), never just "and then"?
- does every beat change something (a value moves, the audience learns something)?
- does any character act on something they couldn't know yet?
- is every big turn set up, and every setup paid off?
- is the hero's want or need settled by the end, and does the main value end somewhere different from where it began?
- is every claim backed by proof the viewer sees?
List what you find in a sentence each under "critique", then fix all of it. Answer with one JSON object:
{"critique": [string], "title": string, "beats": [...same shape as before]}.`;

const GUIDE = `You write stories as data for raea/story. A story is one JSON object:
- values: what's at stake, each {neg, pos} (McKee's value, e.g. {"neg":"buried","pos":"free"}).
- facts: named facts, each a sentence.
- agents: characters, each {want, need, lie, mask, role, believes, arc}. mask is one of hero, mentor, ally, herald, threshold, shapeshifter, shadow, trickster, gift. For an ad or launch, role "customer" for the person and "brand" for the product. arc "negative" for a tragedy's hero.
- world: events IN THE ORDER THEY HAPPEN, each {who, with, does, because, needs, sets, unsets, learns, turns, ends, dilemma, claim, proof, routine}. Every event after the first names its cause in "because" (an earlier event), unless it's "routine": true. "turns" moves values in whole steps from -3 to 3; every event that matters turns something. "ends" settles an agent's want or need ({"hero": "need"}). A "dilemma" lists the two options a character chooses between. A "claim" made to the audience names its "proof" (facts that event or an earlier one sets). "unsets" undoes a fact (a lie exposed).
- tellings: one telling named "main": the beats in the order the audience sees them. Each beat: {id, shows: [event keys], feel, reveals, hides, asks, setup, pays, onScreen, says, seconds, surprise, meant}. A beat can show an event out of order (a flash-forward hook). "asks" raises a question (a fact the audience wants); a later beat "reveals" it. "setup" plants a named setup; a later beat "pays" it. EVERY beat has "onScreen" (one or two concrete sentences of what the viewer sees), "seconds", and usually "says": [{who, says}] (who is an agent key or "narrator"). onScreen and says are the only words a viewer gets, so they carry the story alone. surprise: "meant" on a beat whose shock is the point; meant: ["check"] on a beat where a finding is deliberate (a breather a journey wants).
- idea: {value, side, counter}: the value the story is about and what it argues.
- lenses: the framework names it's held to.
Facts are KEYS: believes, needs, sets, unsets, learns, proof, reveals, hides and asks hold fact keys you declared in "facts", never sentences. Knowledge is tracked: an agent can only act on ("needs") a fact they believe from the start, or that an earlier event they were in (as who or with) set or learned.
A checker reads it: every beat turns a value or reveals something; each beat follows from what was seen; a big turn (2 or more) is set up; every setup is paid; a want or need is settled; a claim has proof shown by then; the focus value moves; and each named lens finds its stages. Keys are short camelCase words. Use only the keys you declare.
One valid story, a 30-second launch film:
${JSON.stringify(toData(launch as AnyStory))}
Answer with the JSON object only.`;

const EXTRACT = `You read a script and record, as story data, ONLY what the script shows. Never add a cause, a setup, a fact or a
feeling the script doesn't show on screen or in its lines: if a beat happens with no shown reason, leave "because" out;
if nothing in a beat moves a value, give its event no turn. The format:
- values: what's at stake, each {neg, pos}. facts: named facts, each a sentence. agents: each {want, need, role} as far as the script shows.
- world: events in the order they happen in the story, each {who, with, does, because (an earlier event the script shows causing it), needs, sets, learns, turns (value: -3..3), ends ({agent: "want"|"need"}), claim, proof, routine}.
- tellings: {"main": one beat per numbered beat of the script, in order, each {shows: [event keys], reveals, asks, setup, pays}}: exactly as many beats as the script has.
Facts are keys. A character "needs" a fact only when the script shows them acting on it. Answer with the JSON object only.`;

const JUDGE = (brief: string, scripts: [string, string][]) => `${["", "One writer", "Two writers", "Three writers", "Four writers", "Five writers"][scripts.length]} answered the same brief. Read each script as the
viewer gets it, then rank them. The brief: ${brief}

${scripts.map(([l, s]) => `SCRIPT ${l}\n${s}`).join("\n\n")}

Score each from 1 to 10 on: cause (each beat follows from the last, no "and then"), setups (what's planted pays off;
nothing comes from nowhere), clarity (a viewer follows what happens from the screen and lines alone), brief (it does
what the brief asks, at its length), feeling (it moves the viewer), overall. Then rank all of them, best first.
Answer with one JSON object: {"scores": {"${scripts.map(([l]) => l).join('": {...}, "')}": {...}} with each {cause, setups, clarity, brief, feeling, overall},
"rank": [letters best first], "why": one or two sentences}.`;

// ---- the model ----
let spent = 0;
async function ask(model: string, messages: J[], o: { tokens?: number; reasoning?: J } = {}): Promise<{ text: string; usd: number; ms: number; finish?: string }> {
  const t = performance.now();
  for (let attempt = 0; attempt < 4; attempt++) {
    const res = await fetch("https://openrouter.ai/api/v1/chat/completions", {
      method: "POST",
      headers: { Authorization: `Bearer ${KEY}`, "Content-Type": "application/json", "X-Title": "story-ab" },
      body: JSON.stringify({ model, messages, temperature: 1, max_tokens: o.tokens ?? 40000, ...(o.reasoning ? { reasoning: o.reasoning } : {}), response_format: { type: "json_object" }, usage: { include: true } }),
    }).catch((e) => ({ status: 599, json: async () => ({ error: String(e) }), ok: false }) as unknown as Response);
    if (res.status === 429 || res.status >= 500) { await new Promise((r) => setTimeout(r, 3000 * (attempt + 1))); continue; }
    const j: J = await res.json();
    if (!res.ok) throw new Error(`${model} ${res.status} ${JSON.stringify(j).slice(0, 300)}`);
    const usd = j.usage?.cost ?? 0;
    spent += usd;
    return { text: j.choices?.[0]?.message?.content ?? "", usd, ms: performance.now() - t, finish: j.choices?.[0]?.finish_reason };
  }
  throw new Error(`${model} kept refusing`);
}
const parse = (t: string): J | null => { try { return JSON.parse(t.replace(/^\s*```(json)?/, "").replace(/```\s*$/, "").trim()); } catch { const m = t.match(/\{[\s\S]*\}/); try { return m ? JSON.parse(m[0]) : null; } catch { return null; } } };
const beatsOk = (j: J | null) => !!j && Array.isArray(j.beats) && j.beats.length > 0;
const script = (j: J) => `${j.title ?? ""}\n${scriptOf(j.beats)}`;
const seconds = (j: J) => j.beats.reduce((a: number, b: J) => a + (typeof b.seconds === "number" ? b.seconds : 0), 0);

// ---- the arms ----
type Arm = { arm: string; final: J | null; calls: number; usd: number; ms: number; log: J[] };
async function plain(p: Prompt): Promise<Arm> {
  const msgs = [{ role: "system", content: PLAIN }, { role: "user", content: p.brief }];
  const r = await ask(WRITER, msgs);
  return { arm: "A", final: parse(r.text), calls: 1, usd: r.usd, ms: r.ms, log: [{ text: r.text }] };
}
async function edited(p: Prompt, a: Arm): Promise<Arm> {
  if (!beatsOk(a.final)) return { arm: "C", final: null, calls: 0, usd: 0, ms: 0, log: [] };
  const msgs = [{ role: "system", content: PLAIN }, { role: "user", content: p.brief }, { role: "assistant", content: JSON.stringify(a.final) }, { role: "user", content: EDIT(p) }];
  const r = await ask(WRITER, msgs);
  const j = parse(r.text);
  return { arm: "C", final: j, calls: 2, usd: a.usd + r.usd, ms: a.ms + r.ms, log: [{ critique: j?.critique, finish: r.finish }] };
}
function findingsOf(j: J, lenses: string[]): { blocking: string[]; all: J[]; story: AnyStory | null } {
  const { story, errors } = fromData(j);
  if (!story) return { blocking: errors.map((e) => `${e.at}: ${e.says}`), all: [], story: null };
  const found = [...checkStory(story, "main"), ...readThrough(story, lenses.map((l) => LENSES[l]).filter(Boolean), "main").flatMap((r) => r.findings)];
  return { blocking: found.filter((f) => f.level !== "note").map((f) => `${f.level} ${f.check} at ${f.about}: ${f.says}${f.fix ? ` (fix: ${f.fix})` : ""}`), all: found, story };
}
async function sdk(p: Prompt): Promise<Arm> {
  const msgs: J[] = [{ role: "system", content: GUIDE }, { role: "user", content: `${p.brief}\nHold it to these lenses: ${p.lenses.join(", ")}.` }];
  let usd = 0, ms = 0, calls = 0, j: J | null = null;
  const log: J[] = [];
  for (let round = 0; round < 3; round++) {
    const r = await ask(WRITER, msgs);
    usd += r.usd; ms += r.ms; calls++;
    j = parse(r.text);
    if (!j) { log.push({ round, parse: "failed", finish: r.finish, chars: r.text.length }); msgs.push({ role: "assistant", content: r.text }, { role: "user", content: "That wasn't one JSON object. Answer with the whole story as one JSON object only." }); continue; }
    let f: ReturnType<typeof findingsOf>;
    try { f = findingsOf(j, p.lenses); }
    catch (e) { log.push({ round, crash: String((e as Error).stack).slice(0, 1200) }); writeFileSync(join(OUT, `crash-${p.id}-${Date.now()}.json`), JSON.stringify({ story: j, stack: String((e as Error).stack) })); break; }
    log.push({ round, blocking: f.blocking, notes: f.all.filter((x) => x.level === "note").length });
    if (!f.blocking.length || round === 2) break;
    msgs.push({ role: "assistant", content: r.text }, { role: "user", content: `The checker found:\n${f.blocking.map((x) => `- ${x}`).join("\n")}\nFix each one (or, where a finding is a choice you mean, mark the beat with meant: ["check"]) and answer with the whole corrected story as one JSON object.` });
  }
  let final: J | null = null;
  if (j) {
    const { story } = fromData(j);
    const tell: J[] = j.tellings?.main ?? [];
    const res = story ? resolved(story, 0) : null;
    final = { title: j.title, beats: tell.map((b) => ({ onScreen: b.onScreen ?? (b.shows ?? []).map((k: string) => res?.world?.[k]?.does ?? j.world?.[k]?.does).filter(Boolean).join(". "), says: b.says ?? [], seconds: b.seconds })), data: j };
  }
  return { arm: "B", final, calls, usd, ms, log };
}

/** A finding about an extracted story, said by the beat it's about (the script's numbering). */
function byBeat(data: J, f: J): string {
  const tell: J[] = data.tellings?.main ?? [];
  let k = tell.findIndex((b, i) => b.id === f.about || `b${i}` === f.about);
  if (k < 0) k = tell.findIndex((b) => (b.shows ?? []).includes(f.about));
  const where = k >= 0 ? `beat ${k + 1}` : data.agents?.[f.about] ? `${f.about}'s arc` : "the whole script";
  return `${where}: ${f.says}`;
}
async function checkedEdit(p: Prompt, a: Arm): Promise<Arm> {
  if (!beatsOk(a.final)) return { arm: "D", final: null, calls: 0, usd: 0, ms: 0, log: [] };
  const ex = await extract(script(a.final!));
  if (!ex.data) return { arm: "D", final: null, calls: 0, usd: a.usd, ms: a.ms, log: [{ extract: ex.errors }] };
  const { story } = fromData(ex.data);
  const lens = story ? readThrough(story, p.lenses.map((l) => LENSES[l]).filter(Boolean), "main").flatMap((r) => r.findings) : [];
  const found = [...(ex.checks ?? []), ...lens.map((f) => ({ check: f.check, level: f.level, about: f.about, says: f.says }))].filter((f) => f.level !== "note");
  if (!found.length) return { arm: "D", final: a.final, calls: 1, usd: a.usd, ms: a.ms, log: [{ found: [] }] };
  const list = [...new Set(found.map((f) => `- ${byBeat(ex.data!, f)}`))].join("\n");
  const msgs = [{ role: "system", content: PLAIN }, { role: "user", content: p.brief }, { role: "assistant", content: JSON.stringify(a.final) },
    { role: "user", content: `A story checker read your script back as story data, from the screen and the lines alone, and found:\n${list}\nEach is what a viewer can't see on screen: a beat with no shown reason, a turn nothing set up, a setup never paid, an arc never settled. Fix each one so the screen shows it (or leave it where it's a choice you mean), then rewrite. Answer with one JSON object {"title": string, "beats": [...same shape as before]}.` }];
  const r = await ask(WRITER, msgs);
  return { arm: "D", final: parse(r.text), calls: 2, usd: a.usd + r.usd + ex.usd, ms: a.ms + r.ms, log: [{ found: found.map((f) => `${f.level} ${f.check}`), said: list, finish: r.finish }] };
}
async function written(p: Prompt, b: Arm): Promise<Arm> {
  const data = b.final?.data;
  if (!data) return { arm: "E", final: null, calls: 0, usd: 0, ms: 0, log: [] };
  const { story } = fromData(data);
  if (!story) return { arm: "E", final: null, calls: 0, usd: b.usd, ms: b.ms, log: [{ refused: true }] };
  const msgs = [{ role: "system", content: PLAIN }, { role: "user", content: `${p.brief}\n\nThe story is already worked out and checked, as story lines (VALUE, FACT, AGENT, EVENT in story order with its causes, then the telling's BEATs in the order the viewer sees them, each with what it shows, reveals, plants and pays):\n\n${toLines(story)}\n\nWrite it as the beat script, as a screenwriter would: keep every event, cause, setup and payoff, and make what's on screen and what's said carry each one so a viewer gets it without the notes. Use as many beats as the film needs.` }];
  const r = await ask(WRITER, msgs);
  return { arm: "E", final: parse(r.text), calls: b.calls + 1, usd: b.usd + r.usd, ms: b.ms + r.ms, log: [{ finish: r.finish }] };
}

// ---- scoring ----
/**
 * The extractor's mechanical slips, tidied before fromData reads it (never a story's content): null fields dropped, a
 * lone key where a list goes wrapped, a fact used but not declared declared with its own words, and a narrator who acts
 * made an agent. What's left wrong goes back to the extractor.
 */
const LISTS = { ev: ["with", "needs", "sets", "unsets", "learns", "proof", "dilemma"], beat: ["shows", "reveals", "hides", "asks"], agent: ["believes"] };
function tidy(j: J): J {
  const drop = (o: J) => { for (const k of Object.keys(o)) if (o[k] === null) delete o[k]; };
  const wrap = (o: J, ks: string[]) => { for (const k of ks) if (typeof o[k] === "string") o[k] = [o[k]]; };
  j.facts ??= {};
  for (const a of Object.values((j.agents ?? {}) as J)) if (a && typeof a === "object") { drop(a); wrap(a, LISTS.agent); }
  const facts = (xs: unknown) => { if (Array.isArray(xs)) for (const f of xs) if (typeof f === "string" && !(f in j.facts)) j.facts[f] = f; };
  for (const e of Object.values((j.world ?? {}) as J)) if (e && typeof e === "object") {
    drop(e); wrap(e, LISTS.ev);
    if (e.who === "narrator" && !j.agents?.narrator) (j.agents ??= {}).narrator = {};
    for (const k of ["needs", "sets", "unsets", "learns", "proof"]) facts(e[k]);
  }
  for (const t of Object.values((j.tellings ?? {}) as J)) if (Array.isArray(t)) for (const b of t) if (b && typeof b === "object") {
    drop(b); wrap(b, LISTS.beat); for (const k of ["reveals", "hides", "asks"]) facts(b[k]);
  }
  return j;
}
type Extracted = { checks: J[] | null; errors: string[]; usd: number; beats: number; data?: J };
async function extract(s: string): Promise<Extracted> {
  let usd = 0;
  const msgs: J[] = [{ role: "system", content: EXTRACT }, { role: "user", content: s }];
  for (let t = 0; t < 3; t++) {
    const r = await ask(EXTRACTOR, msgs, { tokens: 30000 });
    usd += r.usd;
    const raw = parse(r.text), j = raw ? tidy(raw) : null;
    const d = j ? fromData(j) : { story: null, errors: [{ at: "", says: "not JSON" }] };
    if (d.story && !d.errors.length) {
      try { return { checks: checkStory(d.story, "main").map((f) => ({ check: f.check, level: f.level, about: f.about, says: f.says })), errors: [], usd, beats: (j!.tellings?.main ?? []).length, data: j! }; }
      catch (e) { writeFileSync(join(OUT, `crash-extract-${Date.now()}.json`), JSON.stringify({ story: j, stack: String((e as Error).stack) })); return { checks: null, errors: [`crash: ${(e as Error).message}`], usd, beats: 0 }; }
    }
    msgs.push({ role: "assistant", content: r.text }, { role: "user", content: `Fix these and answer with the whole JSON object:\n${d.errors.slice(0, 20).map((e) => `- ${e.at}: ${e.says}`).join("\n")}` });
    if (t === 2) return { checks: null, errors: d.errors.slice(0, 5).map((e) => `${e.at}: ${e.says}`), usd, beats: 0 };
  }
  return { checks: null, errors: ["unreachable"], usd, beats: 0 };
}

async function pool<T, R>(xs: T[], n: number, fn: (x: T) => Promise<R>): Promise<R[]> {
  const out: R[] = new Array(xs.length);
  let i = 0;
  await Promise.all(Array.from({ length: Math.min(n, xs.length) }, async () => { while (i < xs.length) { const k = i++; out[k] = await fn(xs[k]); } }));
  return out;
}

const prompts = PROMPTS.filter((p) => !ONLY.length || ONLY.includes(p.id));
const units = prompts.flatMap((p) => Array.from({ length: SAMPLES }, (_, s) => ({ p, s })));
const prior = new Map<string, J>();
if (REUSE) for (const l of readFileSync(join(REUSE, "rows.jsonl"), "utf8").split("\n").filter(Boolean)) { const r = JSON.parse(l); prior.set(`${r.id}#${r.sample}`, r); }
const reused = (id: string, s: number, arm: string): Arm | null => {
  const x = prior.get(`${id}#${s}`)?.arms?.find((y: J) => y.arm === arm);
  return x && beatsOk(x.final) ? { arm, final: x.final, calls: x.calls, usd: x.usd, ms: x.ms, log: [...x.log, { reused: REUSE }] } : null;
};
console.log(`ab: ${units.length} prompt samples x ${ARMS.join(",")}${REUSE ? ` (reusing ${REUSE})` : ""}, writer ${WRITER}, extractor ${EXTRACTOR}, judges ${JUDGES.join(" + ")} -> ${OUT}`);
const rowsPath = join(OUT, "rows.jsonl");

await pool(units, JOBS, async ({ p, s }) => {
  try {
    const [a, b] = await Promise.all([reused(p.id, s, "A") ?? plain(p), reused(p.id, s, "B") ?? sdk(p)]);
    const [c, d, e] = await Promise.all([
      ARMS.includes("C") ? reused(p.id, s, "C") ?? edited(p, a) : null,
      ARMS.includes("D") ? reused(p.id, s, "D") ?? checkedEdit(p, a) : null,
      ARMS.includes("E") ? reused(p.id, s, "E") ?? written(p, b) : null,
    ]);
    const arms = [a, c, b, d, e].filter((x): x is Arm => !!x && ARMS.includes(x.arm));
    const scored = await Promise.all(arms.map(async (x) => {
      if (!beatsOk(x.final)) return { ...x, script: null, extract: null };
      const sc = script(x.final!);
      return { ...x, script: sc, extract: await extract(sc) };
    }));
    const ready = scored.filter((x) => x.script);
    const r = rng(s * 7919 + p.id.length);
    const order = ready.map((x) => x.arm).sort(() => r() - 0.5);
    const letters = ["V", "W", "X", "Y", "Z"].slice(0, order.length);
    const key = Object.fromEntries(letters.map((l, i) => [l, order[i]]));
    const judged = await Promise.all(JUDGES.map(async (m) => {
      const res = await ask(m, [{ role: "user", content: JUDGE(p.brief, letters.map((l) => [l, ready.find((x) => x.arm === key[l])!.script!])) }], { tokens: 12000, reasoning: { max_tokens: 4000 } }).catch((e) => ({ text: "", usd: 0, ms: 0, err: String(e) }));
      const j = parse(res.text);
      const byArm = j?.scores ? Object.fromEntries(Object.entries(j.scores as J).map(([l, v]) => [key[l], v])) : null;
      return { judge: m, rank: (j?.rank ?? []).map((l: string) => key[l]), scores: byArm, why: j?.why, usd: res.usd };
    }));
    const row = { id: p.id, sample: s, brief: p.brief, lenses: p.lenses, key, arms: scored.map((x) => ({ arm: x.arm, calls: x.calls, usd: x.usd, ms: Math.round(x.ms), log: x.log, final: x.final, script: x.script, beats: x.final?.beats?.length ?? 0, seconds: x.final?.beats ? seconds(x.final) : 0, extract: x.extract ? { ...x.extract, data: undefined } : null })), judged };
    appendFileSync(rowsPath, JSON.stringify(row) + "\n");
    console.log(`${p.id}#${s}  ${judged.map((j) => `${j.judge.split("/")[1]}: ${j.rank.join(">")}`).join("  ")}  $${spent.toFixed(2)}`);
  } catch (e) {
    console.log(`${p.id}#${s} FAILED ${String((e as Error).stack).slice(0, 900)}`);
    appendFileSync(join(OUT, "failed.jsonl"), JSON.stringify({ id: p.id, sample: s, err: String(e) }) + "\n");
  }
});
writeFileSync(join(OUT, "spent.txt"), `${spent.toFixed(4)}\n`);
console.log(`done: $${spent.toFixed(2)} -> ${rowsPath}`);
