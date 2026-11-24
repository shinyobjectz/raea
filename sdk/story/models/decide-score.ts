// Scores the story decision model's predictions against the exact answers (docs/story.md, "Story as decisions"),
// every model on the windows all of them answered: Clef and Clef-flash (zero-shot), the local model, and "prior" (the
// training set's base rates, the floor any model has to clear).
//   noul    Brier (lower is better), AUC, accuracy at 0.5        breaks.* pooled as "breaks", and each on its own
//   choice  accuracy (the most likely option), Brier over the options
//   score   accuracy (the most likely level), mean error of the expected level
//   bun models/decide-score.ts [--split test] [--rows ~/.vd/story/rows/decide] [--by as]
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

type J = Record<string, any>;
const arg = (k: string, d: string) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const SPLIT = arg("--split", "test"), DIR = arg("--rows", join(homedir(), ".vd/story/rows/decide")).replace(/^~/, homedir());
const read = (f: string): J[] => readFileSync(join(DIR, f), "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l));
const keyOf = (r: J) => `${r.i}:${r.kind}:${r.at}:${r.as}`;
const rows = new Map(read(`${SPLIT}.jsonl`).map((r) => [keyOf(r), r]));

// the prior: base rates on train, by question id (choices whose options change per story fall back to uniform)
const train = read("train.jsonl");
const freq: Record<string, Record<string, number>> = {};
for (const r of train) for (const [id, v] of Object.entries(r.answers)) { const o = (freq[id] ??= {}); const k = String(v === true ? "yes" : v === false ? "no" : v); o[k] = (o[k] ?? 0) + 1; }
const priorFor = (id: string, q: J): Record<string, number> => {
  const opts = q.type === "noul" ? ["no", "yes"] : q.type === "choice" ? Object.keys(q.criteria) : q.criteria.map((_: string, i: number) => String(i));
  const f = id === "shows" ? {} : freq[id] ?? {};
  const tot = opts.reduce((a: number, o: string) => a + (f[o] ?? 0) + 0.5, 0);
  return Object.fromEntries(opts.map((o: string) => [o, ((f[o] ?? 0) + 0.5) / tot]));
};

const preds: Record<string, Map<string, J>> = {};
for (const f of readdirSync(DIR).filter((f) => f.startsWith("pred-") && f.endsWith(`-${SPLIT}.jsonl`))) preds[f.slice(5, -(`-${SPLIT}.jsonl`.length))] = new Map(read(f).map((p) => [p.key, p.probs]));
const common = [...rows.keys()].filter((k) => Object.values(preds).every((m) => m.has(k)));
preds.prior = new Map(common.map((k) => [k, Object.fromEntries(Object.entries(rows.get(k)!.questions as J).map(([id, q]) => [id, priorFor(id, q)]))]));

const truth = (q: J, v: unknown) => (q.type === "noul" ? (v ? "yes" : "no") : String(v));
function auc(ps: number[], ys: boolean[]): number | null {
  const pos = ps.filter((_, i) => ys[i]), neg = ps.filter((_, i) => !ys[i]);
  if (!pos.length || !neg.length) return null;
  let s = 0; for (const p of pos) for (const n of neg) s += p > n ? 1 : p === n ? 0.5 : 0;
  return s / (pos.length * neg.length);
}
function score(who: string, keys: string[]): J {
  const groups: Record<string, { p: number[]; y: boolean[]; acc: number[]; brier: number[]; err: number[] }> = {};
  const g = (k: string) => (groups[k] ??= { p: [], y: [], acc: [], brier: [], err: [] });
  for (const key of keys) {
    const r = rows.get(key)!, pr = preds[who].get(key)!;
    for (const [id, q] of Object.entries(r.questions as J)) {
      const probs: Record<string, number> = pr[id] ?? {};
      const t = truth(q, r.answers[id]);
      const opts = Object.keys(probs).length ? Object.keys(probs) : Object.keys(priorFor(id, q));
      const best = opts.reduce((a, b) => ((probs[b] ?? 0) > (probs[a] ?? 0) ? b : a), opts[0]);
      const brier = opts.reduce((a, o) => a + ((probs[o] ?? 0) - (o === t ? 1 : 0)) ** 2, 0);
      const names = id.startsWith("breaks.") ? ["breaks", id] : id.startsWith("stage.") ? ["stage", id] : [id];
      for (const name of names) {
        const x = g(name);
        x.acc.push(best === t ? 1 : 0); x.brier.push(brier);
        if (q.type === "noul") { x.p.push(probs.yes ?? 0); x.y.push(t === "yes"); }
        if (q.type === "score") x.err.push(Math.abs(opts.reduce((a, o) => a + Number(o) * (probs[o] ?? 0), 0) - Number(t)));
      }
    }
  }
  const mean = (a: number[]) => (a.length ? a.reduce((x, y) => x + y, 0) / a.length : null);
  return Object.fromEntries(Object.entries(groups).map(([k, x]) => [k, { n: x.acc.length, acc: mean(x.acc), brier: mean(x.brier), auc: x.p.length ? auc(x.p, x.y) : null, err: mean(x.err), pos: x.y.filter(Boolean).length || undefined }]));
}
const by = process.argv.includes("--by") ? arg("--by", "as") : null;
const slices: [string, string[]][] = by ? [...new Set(common.map((k) => rows.get(k)![by]))].map((v) => [`${by}=${v}`, common.filter((k) => rows.get(k)![by] === v)]) : [["all", common]];
const result: J = {};
for (const [slice, keys] of slices) {
  console.log(`\n== ${slice}: ${keys.length} windows`);
  const all = Object.fromEntries(Object.keys(preds).map((who) => [who, score(who, keys)]));
  result[slice] = all;
  const names = Object.keys(all.prior).sort((a, b) => (a.includes(".") ? 1 : 0) - (b.includes(".") ? 1 : 0) || a.localeCompare(b));
  const f = (x: number | null | undefined) => (x == null ? "  -  " : x.toFixed(3));
  console.log(["question".padEnd(22), "pos", ...Object.keys(all).map((w) => `${w}: acc brier auc`.padEnd(26))].join(" "));
  for (const nm of names) console.log([nm.padEnd(22), String(all.prior[nm].pos ?? "").padStart(3), ...Object.keys(all).map((w) => `${f(all[w][nm]?.acc)} ${f(all[w][nm]?.brier)} ${f(all[w][nm]?.auc ?? all[w][nm]?.err)}`.padEnd(26))].join(" "));
}
writeFileSync(join(DIR, `scores-${SPLIT}.json`), JSON.stringify(result, null, 1));
