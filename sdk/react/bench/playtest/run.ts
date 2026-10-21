// A tier (i) playtest (sdk/react/docs/playtest.md): random, novelty and beam bots on a game's `playtest` export, then a
// ceiling and softlock analysis. Writes, under OUT:
//   report.json     finished or not, frames to finish, progress ceiling, deaths and stuck spots, softlocks, bot costs
//   run.input.json  the winning run (or the furthest one) as the input timeline `raea-motion render --input` takes
//   trace.jsonl     that run frame by frame (frame, keys, observation, events): the facts views and rules read
//   moments.json    the run's events and the search's death and stuck spots, by frame (views are timed from these)
//   transitions.jsonl  with state machines (Playtest.machines): every transition of the run as a row (frame, entity,
//                   machine, step, from, to, edge, kind, decider); report.json's `machines` has their coverage,
//                   softlocks, loops and static analysis over all the bots' play (sdk/react/docs/state-machines.md)
//   bun sdk/react/bench/playtest/run.ts EDIT.tsx OUT [--seed N] [--quick] [--fps N]
import fs from "node:fs";
import path from "node:path";
import { fpsOf, loadGame } from "./load";
import { Game, beam, novelty, path as movesOf, random, timeline, type Event, type Node } from "./search";
import { MachineTrace, type TransitionRow } from "./machines";

const [edit, out] = process.argv.slice(2).filter((a) => !a.startsWith("--"));
if (!edit || !out) {
  console.error("bun sdk/react/bench/playtest/run.ts EDIT.tsx OUT [--seed N] [--quick] [--fps N]");
  process.exit(2);
}
const arg = (k: string, d: number) => { const i = process.argv.indexOf(k); return i > 0 ? Number(process.argv[i + 1]) : d; };
const seed = arg("--seed", 1);
const quick = process.argv.includes("--quick");
fs.mkdirSync(out, { recursive: true });
const t0 = performance.now();
const { playtest: p } = await loadGame(edit);
const g = new Game(p);
// a `machines` export the trace can't read (an entry that isn't a machine's watch(): the agent's platformer hand-rolled
// `{ state, states }`) is reported and left out, so the bots still play the game
let machinesNote: string | null = null;
if (p.machines) {
  const bad = Object.entries(p.machines(p.initial) ?? {}).filter(([, w]: [string, any]) => !w || !w.machine || !w.snapshot).map(([k]) => k);
  if (bad.length) machinesNote = `the playtest's machines export isn't machine.watch() for ${bad.join(", ")} (each needs { machine, snapshot }, from a machine() of @raea/motion): machine checks skipped`;
}
if (machinesNote) console.error(machinesNote);
const mt = p.machines && !machinesNote ? new MachineTrace(p.machines) : null;
if (mt) g.trace = mt;
const FPS = fpsOf(edit); // the game's own rate (120 for LEDGE): seconds in the report, and the pad after a run
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
const probed = new Set<Node>();
for (let i = 0; i < probes && spread.length; i++) {
  const n = spread[Math.floor((i / probes) * spread.length)];
  // a small archive hands the same state to several probes: each state is probed and counted once (the agent's arena
  // reported "8 of 24" for one state at frame 418)
  if (probed.has(n)) continue;
  probed.add(n);
  if (g.progress(n.s, n.f) >= target) continue;
  // a game over is an end, not a softlock: a lost state can't get back to the frontier by design (2026-10-18, the
  // agent's arena: 8 of 24 probes were its game-over screen)
  if (g.outcome(n.s, n.f) === "lost") continue;
  const r = beam(g, { width: 40, holds: [6, 14], maxFrames: n.f + 6000, start: [n], target, stall: 1800 });
  if (!r.won && !r.reached) softlocks.push({ frame: n.f, at: g.where(n.s, n.f), bestFromHere: +r.best.best.toFixed(4) });
}

// 6b. after an ending: the bots stop at a win or a game over, so what the game does next (a restart, a menu) is played
// on with no keys for 10 s from the first win and the first game over found, in linear play
const AFTER = 1200;
function playOn(n: Node | null) {
  if (!mt || !n) return;
  let s = n.s;
  mt.endPlay(n.f);
  for (let f = n.f; f < n.f + AFTER; f++) {
    const after = p.step(s, { keys: [] } as any, f);
    mt.observe(s, after, { frame: f + 1, keys: [], progress: g.progress(after, f + 1), outcome: g.outcome(after, f + 1) }, true);
    s = after;
  }
  mt.endPlay(n.f + AFTER);
}
playOn(won);
playOn(rnd.lost);

// 7. the run: its input log, its trace and its moments
const moves = movesOf(best);
const log = timeline(p, moves, FPS); // a second of the last keys after, so the render shows what follows
fs.writeFileSync(path.join(out, "run.input.json"), JSON.stringify(log));
const trace: string[] = [];
const runEvents: Event[] = [];
const runRows: TransitionRow[] = [];
const runProgress: number[] = [g.progress(p.initial, 0)];
let s = p.initial;
const frames = Object.keys(log).length;
mt?.endPlay(0);
for (let f = 0; f < frames; f++) {
  const keys = log[String(f)].keys;
  const after = p.step(s, { keys: [...keys] } as any, f);
  const ev = Object.entries(p.events ?? {}).filter(([, is]) => is(s, after)).map(([n]) => n);
  for (const n of ev) runEvents.push({ name: n, frame: f + 1, at: n === "died" ? g.where(s, f) : g.where(after, f + 1) });
  const progress = g.progress(after, f + 1);
  const rows = mt ? mt.observe(s, after, { frame: f + 1, keys, progress, outcome: g.outcome(after, f + 1) }, true) : [];
  runRows.push(...rows);
  runProgress.push(progress);
  s = after;
  const machines = mt ? { machines: mt.states(s), transitions: rows.map((r) => ({ entity: r.entity, from: r.from, to: r.to })) } : {};
  trace.push(JSON.stringify({ frame: f + 1, keys, ...(p.observe ? p.observe(s, f + 1) : {}), progress: +progress.toFixed(5), events: ev, ...machines }));
}
mt?.endPlay(frames);
fs.writeFileSync(path.join(out, "trace.jsonl"), trace.join("\n") + "\n");
if (mt) fs.writeFileSync(path.join(out, "transitions.jsonl"), runRows.map((r) => JSON.stringify(r)).join("\n") + "\n");
fs.writeFileSync(path.join(out, "moments.json"), JSON.stringify({
  run: runEvents,
  // each transition of the run as a moment named "<entity>.<state entered>" (a view's `when: m => m.name === "player.hurt"`)
  transitions: runRows.filter((r) => r.from !== null).map((r) => ({ name: `${r.entity}.${r.to}`, frame: r.frame, at: { entity: r.entity, from: r.from, to: r.to, by: r.by } })),
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
  ...(machinesNote ? { machinesNote } : {}),
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
  softlocks: { probed: probed.size, found: softlocks.length, states: softlocks.slice(0, 8) },
  bots: {
    random: { best: +rnd.best.toFixed(4), steps: tRandom, deaths: rnd.deaths.length },
    novelty: { cells: nov.archive.size, steps: tNovelty, deaths: nov.deaths.length },
    beam: { best: +bm.best.best.toFixed(4), steps: tBeam },
  },
  ...(mt ? { machines: mt.report(runRows, (f) => runProgress[Math.min(f, runProgress.length - 1)]) } : {}),
  steps: g.steps,
  ms: Math.round(performance.now() - t0),
};
fs.writeFileSync(path.join(out, "report.json"), JSON.stringify(report, null, 1));
console.log(JSON.stringify(report, null, 1));
