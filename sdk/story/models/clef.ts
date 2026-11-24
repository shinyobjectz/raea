// Clef as the zero-shot baseline for the story decision model: the same states and typed questions
// (models/decide-rows.ts), answered by Cloudflare's decision models on Workers AI, normalised to per-option
// probabilities like every other model's predictions (decide-score.ts reads them alike).
//   bun models/clef.ts [--model clef-flash|clef] [--split test] [--limit 200] [--jobs 8]
// Needs CLOUDFLARE_ACCOUNT_ID and CLOUDFLARE_API_TOKEN (environment or ~/.raea/env). $0.15 per million input tokens.
import { appendFileSync, existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

type J = Record<string, any>;
const arg = (k: string, d: string) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const MODEL = arg("--model", "clef-flash"), SPLIT = arg("--split", "test"), LIMIT = +arg("--limit", "200"), JOBS = +arg("--jobs", "8");
const DIR = arg("--rows", join(homedir(), ".vd/story/rows/decide")).replace(/^~/, homedir());
const env = (k: string) => process.env[k] ?? (existsSync(join(homedir(), ".raea/env")) ? readFileSync(join(homedir(), ".raea/env"), "utf8").match(new RegExp(`^${k}=(.*)$`, "m"))?.[1]?.replace(/^["']|["']$/g, "") : undefined);
const ACCT = env("CLOUDFLARE_ACCOUNT_ID"), TOKEN = env("CLOUDFLARE_API_TOKEN");
if (!ACCT || !TOKEN) throw new Error("set CLOUDFLARE_ACCOUNT_ID and CLOUDFLARE_API_TOKEN");

export const keyOf = (r: J) => `${r.i}:${r.kind}:${r.at}:${r.as}`;
/** Clef's answer to one question as {option: probability}. */
export function probsOf(q: J, a: J): Record<string, number> {
  if (!a) return {};
  if (q.type === "noul") { const p = typeof a.noul === "number" ? a.noul : 0.5; return { no: 1 - p, yes: p }; }
  return a.probabilities ?? {};
}

const rows: J[] = readFileSync(join(DIR, `${SPLIT}.jsonl`), "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l)).filter((_, j, all) => j % Math.max(1, Math.floor(all.length / LIMIT)) === 0).slice(0, LIMIT);
const out = join(DIR, `pred-${MODEL}-${SPLIT}.jsonl`);
const done = new Set(existsSync(out) ? readFileSync(out, "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l).key) : []);
let tokens = 0, n = 0, failed = 0;
const queue = rows.filter((r) => !done.has(keyOf(r)));
async function one(r: J) {
  for (let attempt = 0; attempt < 3; attempt++) {
    const t = Date.now();
    const res = await fetch(`https://api.cloudflare.com/client/v4/accounts/${ACCT}/ai/run/@cf/cloudflare/${MODEL}`, { method: "POST", headers: { authorization: `Bearer ${TOKEN}`, "content-type": "application/json" }, body: JSON.stringify({ state: r.state, questions: r.questions }) });
    const j: J = await res.json().catch(() => ({}));
    if (!j.success) { if (attempt === 2) { failed++; console.error("failed", keyOf(r), JSON.stringify(j.errors ?? res.status).slice(0, 200)); } continue; }
    tokens += j.result?.usage?.input_tokens ?? 0;
    const probs = Object.fromEntries(Object.entries(r.questions as J).map(([id, q]) => [id, probsOf(q, j.result.answers?.[id])]));
    appendFileSync(out, JSON.stringify({ key: keyOf(r), probs, secs: (Date.now() - t) / 1000 }) + "\n");
    if (++n % 25 === 0) console.log(`${n}/${queue.length}, ${tokens} tokens ($${(tokens * 0.15e-6).toFixed(3)})`);
    return;
  }
}
const it = queue[Symbol.iterator]();
await Promise.all(Array.from({ length: JOBS }, async () => { for (let x = it.next(); !x.done; x = it.next()) await one(x.value); }));
console.log(`done: ${n} answered, ${failed} failed, ${tokens} input tokens, spent $${(tokens * 0.15e-6).toFixed(3)} -> ${out}`);
