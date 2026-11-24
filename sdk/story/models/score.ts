// Scores the story models' predictions (story_models.py) with raea/story itself (docs/story.feature ST6b):
//   extract   valid (reads back with no reference errors), holds (checkStory finds no error), beats (the script's
//             beat count), events F1 (each gold event matched by its words), arc (each beat's turn sign agrees)
//   generate  valid, holds (the story with the written beat in it has no error), clean (no warn either, where the
//             gold story had none at that beat), shows (Jaccard of the events shown, against the cut beat)
//   critic    right (it says "breaks" exactly when there is one), F1 on (beat, check), F1 on check alone
//
//   bun models/score.ts TASK [--rows ~/.vd/story/rows] [--split test]
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { checkStory, fromData, type AnyStory } from "../src/index.ts";

const arg = (k: string, d: string) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const TASK = process.argv[2];
const ROWS = arg("--rows", join(homedir(), ".vd/story/rows")).replace(/^~/, homedir());
const SPLIT = arg("--split", "test");
type J = Record<string, any>;
const read = (p: string): J[] => (existsSync(p) ? readFileSync(p, "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l)) : []);
const parse = (t: string): J | null => { try { const j = JSON.parse(String(t).replace(/^```(json)?|```$/g, "").trim()); return j && typeof j === "object" ? j : null; } catch { return null; } };
const rows = read(join(ROWS, TASK, `${SPLIT}.jsonl`));
const byKey = new Map(rows.map((r) => [`${r.i}:${r.at ?? r.kind ?? ""}`, r]));
const words = (s: string) => new Set(String(s ?? "").toLowerCase().match(/[a-z0-9']+/g) ?? []);
const jac = (a: Set<string>, b: Set<string>) => { const i = [...a].filter((x) => b.has(x)).length; const u = new Set([...a, ...b]).size; return u ? i / u : 1; };
const f1 = (tp: number, fp: number, fn: number) => (tp ? (2 * tp) / (2 * tp + fp + fn) : fp + fn ? 0 : 1);
const errorsOf = (s: AnyStory, level: "error" | "warn" = "error") => { try { return checkStory(s, "main").filter((f) => (level === "error" ? f.level === "error" : f.level !== "note")); } catch (e) { return [{ check: "unreadable", level: "error", about: "", says: String(e) }]; } };
const sign = (x: number) => (x > 0 ? 1 : x < 0 ? -1 : 0);
const beatSigns = (s: J) => (s.tellings?.main ?? []).map((b: J) => sign((b.shows ?? []).reduce((a: number, e: string) => a + Object.values((s.world?.[e]?.turns ?? {}) as Record<string, number>).reduce((x, y) => x + (Number(y) || 0), 0), 0)));

function scoreExtract(gold: J, pred: J | null): J {
  if (!pred) return { valid: 0, holds: 0, beats: 0, eventsF1: 0, arc: 0 };
  const { story, errors } = fromData(pred);
  const valid = story && !errors.length ? 1 : 0;
  const holds = valid ? (errorsOf(story!).length ? 0 : 1) : 0;
  const g = JSON.parse(gold.answer);
  const gb = g.tellings.main.length, pb = pred.tellings?.main?.length ?? 0;
  const ge = Object.values(g.world).map((e: any) => words(e.does)), pe = Object.values(pred.world ?? {}).map((e: any) => words(e?.does));
  const used = new Set<number>();
  let tp = 0;
  for (const w of ge) {
    let best = -1, bj = 0.25;
    pe.forEach((p, k) => { const j = jac(w, p); if (!used.has(k) && j >= bj) { bj = j; best = k; } });
    if (best >= 0) { used.add(best); tp++; }
  }
  const gs = beatSigns(g), ps = beatSigns(pred);
  const arc = gs.length ? gs.filter((x: number, k: number) => ps[k] === x).length / gs.length : 0;
  return { valid, holds, beats: gb === pb ? 1 : 0, eventsF1: f1(tp, pe.length - tp, ge.length - tp), arc };
}

function scoreGenerate(row: J, pred: J | null): J {
  if (!pred) return { valid: 0, holds: 0, clean: 0, shows: 0 };
  const partial = row.story ?? JSON.parse(row.prompt).story;
  const at = row.at as number;
  const beats = [...partial.tellings.main.slice(0, at), pred, ...partial.tellings.main.slice(at)];
  const full = { ...partial, tellings: { main: beats } };
  const { story, errors } = fromData(full);
  const valid = story && !errors.length ? 1 : 0;
  const gold = JSON.parse(row.answer);
  const goldStory = fromData({ ...partial, tellings: { main: [...partial.tellings.main.slice(0, at), gold, ...partial.tellings.main.slice(at)] } }).story!;
  const holds = valid ? (errorsOf(story!).length ? 0 : 1) : 0;
  const warns = valid ? errorsOf(story!, "warn").length : 99, goldWarns = errorsOf(goldStory, "warn").length;
  return { valid, holds, clean: valid && warns <= goldWarns ? 1 : 0, shows: jac(new Set(pred.shows ?? []), new Set(gold.shows ?? [])) };
}

function scoreCritic(row: J, pred: J | null): J {
  const gold: J[] = JSON.parse(row.answer).breaks;
  const p: J[] = Array.isArray(pred?.breaks) ? pred!.breaks : [];
  const k1 = (b: J) => `${b.beat}:${b.check}`, k2 = (b: J) => String(b.check);
  const set = (xs: J[], k: (b: J) => string) => new Set(xs.map(k));
  const count = (k: (b: J) => string) => { const g = set(gold, k), q = set(p, k); const tp = [...q].filter((x) => g.has(x)).length; return [tp, q.size - tp, g.size - tp]; };
  return { parsed: pred ? 1 : 0, right: (gold.length > 0) === (p.length > 0) ? 1 : 0, at: count(k1), check: count(k2) };
}

const files = readdirSync(join(ROWS, TASK)).filter((f) => f.startsWith("pred-") && f.endsWith(`-${SPLIT}.jsonl`));
const results: J = {};
for (const f of files) {
  const who0 = f.slice(5, -(`-${SPLIT}.jsonl`.length));
  const all = read(join(ROWS, TASK, f));
  // the extractor's scripts and its transcripts (lines only) are scored apart
  const kinds = TASK === "extract" ? [...new Set(all.map((p) => p.kind ?? "script"))] : [undefined];
  for (const kind of kinds) {
  const who = kind ? `${who0}/${kind}` : who0;
  const preds = kind ? all.filter((p) => (p.kind ?? "script") === kind) : all;
  const per: J[] = [];
  for (const p of preds) {
    const row = byKey.get(p.key);
    if (!row) continue;
    const j = parse(p.pred);
    per.push(TASK === "extract" ? scoreExtract(row, j) : TASK === "generate" ? scoreGenerate(row, j) : scoreCritic(row, j));
  }
  if (!per.length) continue;
  const mean = (k: string) => per.reduce((a, x) => a + x[k], 0) / per.length;
  let r: J;
  if (TASK === "critic") {
    const sum = (k: string) => per.reduce((a, x) => [a[0] + x[k][0], a[1] + x[k][1], a[2] + x[k][2]], [0, 0, 0]);
    const [a1, a2, a3] = sum("at"), [c1, c2, c3] = sum("check");
    r = { n: per.length, parsed: mean("parsed"), right: mean("right"), atF1: f1(a1, a2, a3), checkF1: f1(c1, c2, c3) };
  } else {
    r = { n: per.length, ...Object.fromEntries(Object.keys(per[0]).map((k) => [k, mean(k)])) };
  }
  const secs = preds.map((p) => p.secs).filter((x) => typeof x === "number");
  r.secsMedian = secs.sort((a, b) => a - b)[Math.floor(secs.length / 2)] ?? null;
  results[who] = r;
  }
}
const fmt = (x: unknown) => (typeof x === "number" ? (Number.isInteger(x) ? String(x) : x.toFixed(3)) : String(x));
const cols = [...new Set(Object.values(results).flatMap((r: J) => Object.keys(r)))];
console.log(["who", ...cols].join("\t"));
for (const [who, r] of Object.entries(results)) console.log([who, ...cols.map((c) => fmt((r as J)[c]))].join("\t"));
writeFileSync(join(ROWS, TASK, `scores-${SPLIT}.json`), JSON.stringify(results, null, 1));
