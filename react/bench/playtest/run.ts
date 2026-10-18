// A tier (i) playtest (react/docs/playtest.md): random, novelty and beam bots on a game's `playtest` export, then a
// ceiling and softlock analysis. Writes, under OUT:
//   report.json     finished or not, frames to finish, progress ceiling, deaths and stuck spots, softlocks, bot costs
//   run.input.json  the winning run (or the furthest one) as the input timeline `raea-motion render --input` takes
//   trace.jsonl     that run frame by frame (frame, keys, observation, events): the facts views and rules read
//   moments.json    the run's events and the search's death and stuck spots, by frame (views are timed from these)
//   bun react/bench/playtest/run.ts EDIT.tsx OUT [--seed N] [--quick]
import fs from "node:fs";
import path from "node:path";
import { loadGame } from "./load";
import { Game, beam, novelty, path as movesOf, random, timeline, type Event, type Node } from "./search";

const [edit, out] = process.argv.slice(2).filter((a) => !a.startsWith("--"));
if (!edit || !out) {
  console.error("bun react/bench/playtest/run.ts EDIT.tsx OUT [--seed N] [--quick]");
  process.exit(2);
}
const arg = (k: string, d: number) => { const i = process.argv.indexOf(k); return i > 0 ? Number(process.argv[i + 1]) : d; };
const seed = arg("--seed", 1);
const quick = process.argv.includes("--quick");
fs.mkdirSync(out, { recursive: true });
const t0 = performance.now();
const { playtest: p } = await loadGame(edit);
const g = new Game(p);
const FPS = 120;
const MAX = quick ? 6000 : 12000; // frames a run may take (100 s of play at 120 fps)
const bucket = (x: number, w: number) => Math.floor(x / w) * w;

// 1. random: the floor any search must beat, and where blind play dies
const rnd = random(g, { episodes: quick ? 20 : 60, frames: 3000, seed });
const tRandom = g.steps;
// 2. novelty: the reachable set
const nov = novelty(g, { iterations: quick ? 1500 : 5000, explore: 12, seed: seed + 1, maxFrames: MAX });
const tNovelty = g.steps - tRandom;
// 3. beam: the shortest finish it can find, or the furthest point
const bm = beam(g, { width: quick ? 120 : 300, holds: [6, 14], maxFrames: MAX, stall: 2400 });
const tBeam = g.steps - tRandom - tNovelty;

const won = bm.won ?? nov.won ?? rnd.won;
const archive = [...nov.archive.values()].map((c) => c.n);
const furthest = [bm.best, ...archive].reduce((a, b) => (g.progress(b.s, b.f) > g.progress(a.s, a.f) ? b : a));
const ceiling = won ? 1 : Math.max(...archive.map((n) => n.best), bm.best.best, rnd.best);
const best: Node = won ?? furthest;

// 4. stuck spots: novelty cells chosen often that found nothing new (the explorer keeps coming back and can't move on)
const cells = [...nov.archive.values()];
const stuckCells = cells.filter((c) => c.chosen >= 8 && c.grew === 0).sort((a, b) => b.chosen - a.chosen).slice(0, 8);
const stuck = stuckCells.map((c) => ({ frame: c.n.f, chosen: c.chosen, at: g.where(c.n.s, c.n.f) }));

// 5. deaths: where every bot died, bucketed (by x when the game observes one, else progress)
const allDeaths: Event[] = [...rnd.deaths, ...nov.deaths];
const hist = new Map<number, number>();
for (const d of allDeaths) {
  const k = typeof d.at.x === "number" ? bucket(d.at.x as number, 200) : bucket(Math.round((d.at.progress as number) * 1000), 10);
  hist.set(k, (hist.get(k) ?? 0) + 1);
}
const deathSpots = [...hist.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8).map(([k, n]) => ({ [typeof allDeaths[0]?.at.x === "number" ? "x" : "progress_permille"]: k, deaths: n }));

// 6. softlocks: states the run can reach from which play can't get back to the frontier. From a spread of archive
// states, a small beam tries to reach 98% of the ceiling; a state that can't is a softlock candidate.
const spread = archive.filter((n) => n.f > 0).sort((a, b) => g.progress(a.s, a.f) - g.progress(b.s, b.f));
const probes = quick ? 10 : 24;
const softlocks: unknown[] = [];
const target = ceiling * 0.98;
for (let i = 0; i < probes && spread.length; i++) {
  const n = spread[Math.floor((i / probes) * spread.length)];
  if (g.progress(n.s, n.f) >= target) continue;
  const r = beam(g, { width: 40, holds: [6, 14], maxFrames: n.f + 6000, start: [n], target, stall: 1800 });
  if (!r.won && !r.reached) softlocks.push({ frame: n.f, at: g.where(n.s, n.f), bestFromHere: +r.best.best.toFixed(4) });
}

// 7. the run: its input log, its trace and its moments
const moves = movesOf(best);
const log = timeline(p, moves, FPS); // a second of the last keys after, so the render shows what follows
fs.writeFileSync(path.join(out, "run.input.json"), JSON.stringify(log));
const trace: string[] = [];
const runEvents: Event[] = [];
let s = p.initial;
const frames = Object.keys(log).length;
for (let f = 0; f < frames; f++) {
  const keys = log[String(f)].keys;
  const after = p.step(s, { keys: [...keys] } as any, f);
  const ev = Object.entries(p.events ?? {}).filter(([, is]) => is(s, after)).map(([n]) => n);
  for (const n of ev) runEvents.push({ name: n, frame: f + 1, at: n === "died" ? g.where(s, f) : g.where(after, f + 1) });
  s = after;
  trace.push(JSON.stringify({ frame: f + 1, keys, ...(p.observe ? p.observe(s, f + 1) : {}), progress: +g.progress(s, f + 1).toFixed(5), events: ev }));
}
fs.writeFileSync(path.join(out, "trace.jsonl"), trace.join("\n") + "\n");
fs.writeFileSync(path.join(out, "moments.json"), JSON.stringify({
  run: runEvents,
  // a stuck spot is a state the search reached, not one on the run: each gets its own input log to render from
  stuck: stuckCells.map((c, i) => {
    const log = `stuck-${i}.input.json`;
    fs.writeFileSync(path.join(out, log), JSON.stringify(timeline(p, movesOf(c.n), FPS / 2)));
    return { name: "stuck", frame: c.n.f, at: g.where(c.n.s, c.n.f), log };
  }),
  finish: won ? { frame: won.f } : null,
  furthest: { frame: furthest.f, at: g.where(furthest.s, furthest.f) },
}, null, 1));

const report = {
  game: path.relative(process.cwd(), path.resolve(edit)),
  seed,
  finished: !!won,
  framesToFinish: won ? won.f : null,
  secondsToFinish: won ? +(won.f / FPS).toFixed(2) : null,
  finishedBy: bm.won ? "beam" : nov.won ? "novelty" : rnd.won ? "random" : null,
  ceiling: { progress: +ceiling.toFixed(4), at: g.where(furthest.s, furthest.f), frame: furthest.f },
  run: { frames, deaths: runEvents.filter((e) => e.name === "died").length, events: Object.fromEntries(Object.keys(p.events ?? {}).map((k) => [k, runEvents.filter((e) => e.name === k).length])) },
  deathSpots,
  stuckSpots: stuck,
  softlocks: { probed: probes, found: softlocks.length, states: softlocks.slice(0, 8) },
  bots: {
    random: { best: +rnd.best.toFixed(4), steps: tRandom, deaths: rnd.deaths.length },
    novelty: { cells: nov.archive.size, steps: tNovelty, deaths: nov.deaths.length },
    beam: { best: +bm.best.best.toFixed(4), steps: tBeam },
  },
  steps: g.steps,
  ms: Math.round(performance.now() - t0),
};
fs.writeFileSync(path.join(out, "report.json"), JSON.stringify(report, null, 1));
console.log(JSON.stringify(report, null, 1));
