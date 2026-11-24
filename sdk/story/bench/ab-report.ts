// Reads an A/B run (bench/ab.ts) and says what it found: judges' ranks and scores by arm, blind-extracted findings per
// beat, B's repair rounds, cost and time, per-prompt winners, judge agreement, and B against C pair by pair.
//   bun bench/ab-report.ts ~/.vd/story/ab/run1 [--json]
import { readFileSync } from "node:fs";
import { join } from "node:path";

type J = Record<string, any>;
const dir = process.argv[2];
const rows: J[] = readFileSync(join(dir, "rows.jsonl"), "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l));
const ARMS = ["A", "C", "B", "D", "E"];
const NAME: Record<string, string> = { A: "plain", C: "plain+edit", B: "raea/story", D: "plain+checks", E: "story+write" };
const mean = (xs: number[]) => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : NaN);
const f2 = (x: number) => (Number.isFinite(x) ? x.toFixed(2) : "-");
const binom = (k: number, n: number) => { // two-sided sign test
  const c = (n: number, k: number) => { let r = 1; for (let i = 1; i <= k; i++) r = (r * (n - k + i)) / i; return r; };
  const lo = Math.min(k, n - k); let p = 0; for (let i = 0; i <= lo; i++) p += c(n, i); return Math.min(1, (2 * p) / 2 ** n);
};
const out: J = { rows: rows.length };

console.log(`${rows.length} prompt samples, ${new Set(rows.map((r) => r.id)).size} prompts\n`);
// judges
const crit = ["cause", "setups", "clarity", "brief", "feeling", "overall"];
console.log("JUDGES (rank 1 = best; scores 1-10)");
console.log(`${"arm".padEnd(12)} ${"rank".padStart(5)} ${"firsts".padStart(7)} ${crit.map((c) => c.padStart(8)).join("")}`);
out.judges = {};
for (const a of ARMS) {
  const ranks: number[] = [], firsts: number[] = [], sc: Record<string, number[]> = Object.fromEntries(crit.map((c) => [c, []]));
  for (const r of rows) for (const j of r.judged) {
    const i = j.rank.indexOf(a); if (i < 0) continue;
    ranks.push(i + 1); firsts.push(i === 0 ? 1 : 0);
    for (const c of crit) { const v = Number(j.scores?.[a]?.[c]); if (Number.isFinite(v)) sc[c].push(v); }
  }
  out.judges[a] = { rank: mean(ranks), firsts: mean(firsts), n: ranks.length, ...Object.fromEntries(crit.map((c) => [c, mean(sc[c])])) };
  console.log(`${NAME[a].padEnd(12)} ${f2(mean(ranks)).padStart(5)} ${(Math.round(mean(firsts) * 100) + "%").padStart(7)} ${crit.map((c) => f2(mean(sc[c])).padStart(8)).join("")}`);
}
for (const m of [...new Set(rows.flatMap((r) => r.judged.map((j: J) => j.judge)))]) {
  console.log(`  ${m}: ${ARMS.map((a) => `${NAME[a]} ${f2(mean(rows.flatMap((r) => r.judged.filter((j: J) => j.judge === m).map((j: J) => j.rank.indexOf(a) + 1).filter((x: number) => x > 0))))}`).join(", ")}`);
}
// agreement: the two judges pick the same first
const both = rows.filter((r) => r.judged.length === 2 && r.judged.every((j: J) => j.rank.length));
const agree = both.filter((r) => r.judged[0].rank[0] === r.judged[1].rank[0]).length;
console.log(`  judges pick the same best in ${agree} of ${both.length}`);
out.agree = { same: agree, of: both.length };

// pair by pair: B vs C, B vs A, C vs A on each judge's rank
console.log("\nPAIRS (each judge's ranking of a sample is one vote)");
out.pairs = {};
for (const [x, y] of [["D", "C"], ["D", "A"], ["E", "A"], ["E", "C"], ["E", "B"], ["C", "A"], ["B", "A"], ["B", "C"]]) {
  let w = 0, n = 0;
  for (const r of rows) for (const j of r.judged) { const i = j.rank.indexOf(x), k = j.rank.indexOf(y); if (i < 0 || k < 0) continue; n++; if (i < k) w++; }
  out.pairs[`${x}>${y}`] = { wins: w, of: n, p: binom(w, n) };
  console.log(`  ${NAME[x]} over ${NAME[y]}: ${w} of ${n} (sign test p = ${binom(w, n).toFixed(3)})`);
}

// extraction
console.log("\nBLIND EXTRACTION (the script alone read back into story data; findings from checkStory)");
out.extract = {};
for (const a of ARMS) {
  const per: number[] = [], errs: number[] = [], byCheck: Record<string, number> = {};
  let failed = 0, beats = 0;
  for (const r of rows) { const x = r.arms.find((y: J) => y.arm === a); if (!x?.extract) continue; if (!x.extract.checks) { failed++; continue; }
    const f = x.extract.checks.filter((c: J) => c.level !== "note"); const nb = Math.max(1, x.extract.beats || x.beats);
    per.push(f.length / nb); errs.push(f.filter((c: J) => c.level === "error").length / nb); beats += nb;
    for (const c of f) byCheck[c.check] = (byCheck[c.check] ?? 0) + 1; }
  const top = Object.entries(byCheck).sort((p, q) => q[1] - p[1]).slice(0, 6).map(([k, v]) => `${k} ${(v / beats).toFixed(2)}`).join(", ");
  out.extract[a] = { perBeat: mean(per), errorsPerBeat: mean(errs), failed, byCheck };
  console.log(`  ${NAME[a].padEnd(12)} errors+warns a beat ${f2(mean(per))}, errors a beat ${f2(mean(errs))}${failed ? `, ${failed} unreadable` : ""}; per beat: ${top}`);
}

// the noise floor: a B story whose own data passed every check, read back blind from its script. What the extraction
// finds there is the gap between the data and the screen (or the extractor's own noise), not a hole in the story.
{
  const passed = rows.map((r) => r.arms.find((y: J) => y.arm === "B")).filter((b) => b?.extract?.checks && (() => { const l = [...b.log].reverse().find((x: J) => x.blocking); return l && !l.blocking.length; })());
  const per = passed.map((b) => b.extract.checks.filter((c: J) => c.level !== "note").length / Math.max(1, b.extract.beats || b.beats));
  out.noiseFloor = { perBeat: mean(per), n: passed.length };
  console.log(`  floor: B stories that passed their own checks still get ${f2(mean(per))} a beat read back from the screen (n = ${passed.length})`);
}

// B's own loop
console.log("\nRAEA/STORY'S OWN LOOP");
const B = rows.map((r) => r.arms.find((y: J) => y.arm === "B")).filter(Boolean);
const first = B.filter((b) => b.log[0]?.blocking && !b.log[0].blocking.length).length;
const last = B.filter((b) => { const l = [...b.log].reverse().find((x: J) => x.blocking); return l && !l.blocking.length; }).length;
const crashes = B.filter((b) => b.log.some((x: J) => x.crash)).length;
const fixed: Record<string, number> = {};
for (const b of B) for (const l of b.log) for (const s of l.blocking ?? []) { const k = String(s).split(" ").slice(0, 2).join(" "); fixed[k] = (fixed[k] ?? 0) + 1; }
console.log(`  passed on the first draft ${first} of ${B.length}; passed by the last round ${last} of ${B.length}; mean calls ${f2(mean(B.map((b) => b.calls)))}; SDK crashes ${crashes}`);
console.log(`  findings it raised over all rounds: ${Object.entries(fixed).sort((p, q) => q[1] - p[1]).slice(0, 10).map(([k, v]) => `${k} x${v}`).join(", ")}`);
out.loop = { first, last, of: B.length, calls: mean(B.map((b) => b.calls)), crashes, raised: fixed };

// cost and shape
console.log("\nCOST AND SHAPE (writer only)");
out.cost = {};
for (const a of ARMS) {
  const xs = rows.map((r) => r.arms.find((y: J) => y.arm === a)).filter((x) => x?.final);
  out.cost[a] = { usd: mean(xs.map((x) => x.usd)), secs: mean(xs.map((x) => x.ms / 1000)), beats: mean(xs.map((x) => x.beats)), seconds: mean(xs.map((x) => x.seconds)) };
  console.log(`  ${NAME[a].padEnd(12)} $${mean(xs.map((x) => x.usd)).toFixed(3)} a story, ${f2(mean(xs.map((x) => x.ms / 1000)))} s, ${f2(mean(xs.map((x) => x.beats)))} beats, ${f2(mean(xs.map((x) => x.seconds)))} s of film`);
}

// per prompt
console.log("\nBY PROMPT (mean rank over both judges and samples; best first)");
out.byPrompt = {};
for (const id of [...new Set(rows.map((r) => r.id))]) {
  const rs = rows.filter((r) => r.id === id);
  const m = ARMS.map((a) => [a, mean(rs.flatMap((r) => r.judged.map((j: J) => j.rank.indexOf(a) + 1).filter((x: number) => x > 0)))] as [string, number]).sort((p, q) => p[1] - q[1]);
  out.byPrompt[id] = Object.fromEntries(m);
  const ex = ARMS.map((a) => `${a} ${f2(mean(rs.map((r) => { const x = r.arms.find((y: J) => y.arm === a); return x?.extract?.checks ? x.extract.checks.filter((c: J) => c.level !== "note").length / Math.max(1, x.extract.beats || x.beats) : NaN; }).filter(Number.isFinite)))}`).join(" ");
  console.log(`  ${id.padEnd(16)} ${m.map(([a, v]) => `${NAME[a]} ${f2(v)}`).join("  >  ")}    extract/beat: ${ex}`);
}
if (process.argv.includes("--json")) console.log(JSON.stringify(out));
