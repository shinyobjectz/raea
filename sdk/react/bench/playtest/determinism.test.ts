// The ported platformer's machines are deterministic (sdk/react/docs/state-machines.md): the same input timeline gives the
// same machine states frame by frame across replays, and a fold resumed from a JSON copy of any checkpoint gives the
// same states as one run straight through. (Render against play is replay.sh's.) Run:
//   bun test sdk/react/bench/playtest/determinism.test.ts
import { describe, expect, test } from "bun:test";
import path from "node:path";
import { loadGame } from "./load";
import { MachineTrace } from "./machines";
import { rng } from "./search";

const EDIT = path.resolve(import.meta.dir, "../games/platformer/edit.tsx");
const FRAMES = 6000;

describe("the platformer's machines, frame by frame", async () => {
  const { playtest: p } = await loadGame(EDIT);
  // a timeline of random chords held 4 to 40 frames (seeded), with stretches of nothing (the autopilot plays those
  // before the first key; after it, a person standing still)
  const r = rng(7);
  const timeline: { keys: string[] }[] = [];
  while (timeline.length < FRAMES) {
    const keys = r() < 0.15 ? [] : p.actions[Math.floor(r() * p.actions.length)];
    const hold = 4 + Math.floor(r() * 36);
    for (let i = 0; i < hold; i++) timeline.push({ keys: [...keys] });
  }
  const play = () => {
    const mt = new MachineTrace(p.machines!);
    const states: string[] = [];
    const rows: unknown[] = [];
    let s = p.initial;
    for (let f = 0; f < FRAMES; f++) {
      const after = p.step(s, { keys: [...timeline[f].keys] } as any, f);
      rows.push(...mt.observe(s, after, { frame: f + 1, keys: timeline[f].keys, progress: 0, outcome: null }, true));
      s = after;
      states.push(JSON.stringify(mt.states(s)));
    }
    return { states, rows, end: s };
  };
  const a = play();

  test("two replays of one timeline give the same states and transitions on every frame", () => {
    const b = play();
    expect(b.states).toEqual(a.states);
    expect(b.rows).toEqual(a.rows);
    expect(b.end).toEqual(a.end);
    // the run is a real one: the player, the flow and the walkers all changed state
    const seen = new Set(a.rows.map((x: any) => `${x.machine}.${x.to}`));
    for (const s of ["player.jump", "player.fall", "player.land", "player.run", "walker.lookLeft"]) expect(seen.has(s)).toBe(true);
  });

  test("a fold resumed from a JSON copy of a checkpoint gives the same states", () => {
    let s = p.initial;
    const kept: unknown[] = [s];
    for (let f = 0; f < FRAMES; f++) {
      s = p.step(s, { keys: [...timeline[f].keys] } as any, f);
      kept.push(s);
    }
    for (const cut of [64, 640, 1984, 4096]) {
      let t = JSON.parse(JSON.stringify(kept[cut]));
      for (let f = cut; f < FRAMES; f++) {
        t = p.step(t, { keys: [...timeline[f].keys] } as any, f);
        if (f % 97 === 0 || f === FRAMES - 1) expect(t).toEqual(kept[f + 1]);
      }
    }
  });
});
