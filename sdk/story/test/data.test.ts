import { expect, test } from "bun:test";
import { beatSchema, checkStory, fromData, toData } from "../src/index.ts";
import { launch } from "./fixture.ts";

test("a story round-trips through JSON and reads back clean", () => {
  const j = toData(launch);
  const { story, errors } = fromData(j);
  expect(errors).toEqual([]);
  expect(checkStory(story!, "30s").filter((f) => f.level === "error")).toEqual(checkStory(launch, "30s").filter((f) => f.level === "error"));
});

test("fromData names a bad reference and a cause out of order", () => {
  const j = toData(launch) as any;
  j.tellings["30s"][1].shows.push("nope");
  j.world.week.because = "ships";
  const msgs = fromData(j).errors.map((e) => e.at);
  expect(msgs).toContain("tellings.30s[1].shows[2]");
  expect(msgs).toContain("world.week.because");
});

test("a beat's schema holds the story's own keys", () => {
  const s = beatSchema(launch) as any;
  expect(s.properties.shows.items.enum).toEqual(["week", "doubles", "tries", "ships"]);
  expect(s.properties.pays.anyOf[0].enum).toEqual(["how"]);
});

test("fromData refuses a field of the wrong type before any reader sees it (bench/fuzz.ts)", () => {
  const bad = (f: (j: any) => void) => { const j = toData(launch) as any; f(j); return fromData(j); };
  for (const [what, f] of [
    ["an agent that isn't an object", (j: any) => { j.agents.maya = null; }],
    ["a value with no poles", (j: any) => { j.values.freedom = null; }],
    ["a dilemma as text", (j: any) => { j.world.week.dilemma = "stay or go"; }],
    ["a beat id that isn't text", (j: any) => { j.tellings["30s"][0].id = 3; }],
    ["turns written on a beat", (j: any) => { j.tellings["30s"][0].turns = { freedom: 1 }; }],
    ["negative seconds", (j: any) => { j.tellings["30s"][0].seconds = -2; }],
    ["no telling", (j: any) => { j.tellings = {}; }],
  ] as const) {
    const r = bad(f);
    expect(r.errors.length, what).toBeGreaterThan(0);
    expect(r.story, what).toBeNull();
  }
});

test("fromData refuses two beats with one id in a telling", () => {
  const j = toData(launch) as any;
  j.tellings["30s"][1].id = j.tellings["30s"][0].id = "same";
  expect(fromData(j).errors.map((e) => e.at)).toContain("tellings.30s[1].id");
});
