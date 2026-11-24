import { expect, test } from "bun:test";
import { checkStory, mutants } from "../src/index.ts";
import { launch } from "./fixture.ts";

test("every mutant with an expected check is found by that check", () => {
  const ms = mutants(launch, "30s");
  expect(ms.length).toBeGreaterThan(8);
  const missed = ms.filter((m) => m.label.expect && !checkStory(m.story, m.telling).some((f) => f.check === m.label.expect)).map((m) => `${m.label.kind} ${m.label.at}: ${m.label.says}`);
  expect(missed).toEqual([]);
});

test("the kinds of break it makes", () => {
  const kinds = new Set(mutants(launch, "30s").map((m) => m.label.kind));
  for (const k of ["drop-setup", "drop-payoff", "drop-cause", "early-need", "flatten-beat", "drop-proof", "swap-beats", "orphan-event"]) expect(kinds.has(k as never)).toBe(true);
});
