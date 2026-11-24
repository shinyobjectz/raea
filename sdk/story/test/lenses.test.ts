import { expect, test } from "bun:test";
import { LENSES, checkStory, readThrough, story, value } from "../src/index.ts";
import { launch } from "./fixture.ts";

test("the launch story passes McKee, Pixar, Storynomics and PAS, mapped as expected", () => {
  const r = readThrough(launch, Object.values(LENSES), "30s");
  for (const x of r) expect([x.lens, x.findings.filter((f) => f.level === "error")]).toEqual([x.lens, []]);
  const by = Object.fromEntries(r.map((x) => [x.lens, x.map]));
  expect(by.mckee.inciting).toEqual(["b1"]);
  expect(by.mckee.crisis).toEqual(["b2"]);
  expect(by.mckee.climax).toEqual(["b3"]);
  expect(by.pas).toEqual({ problem: ["b1"], agitate: ["b2"], solve: ["b3"] });
});

test("a brand that saves the day itself fails Storynomics", () => {
  const brandHero = { ...launch, world: { ...launch.world, ships: { ...launch.world.ships, who: "raea" as const, with: ["team" as const] } } };
  const r = readThrough(brandHero, [LENSES.storynomics], "30s")[0];
  expect(r.passes).toBe(false);
  expect(r.findings.some((f) => f.check === "storynomics:hero")).toBe(true);
});

test("a story with no dilemma gets a crisis proposed from the hero's want and need", () => {
  const flat = story({
    values: { v: value("low", "high") },
    agents: { kid: { want: "win the race", need: "learn to lose", mask: "hero" } },
    world: { a: { who: "kid", does: "trains", routine: true }, b: { who: "kid", does: "falls at the start", because: "a", turns: { v: -2 } }, c: { who: "kid", does: "finishes last, laughing", because: "b", turns: { v: 2 }, ends: { kid: "need" } } },
    tellings: { t: [{ shows: ["a"] }, { shows: ["b"] }, { shows: ["c"] }] },
  });
  const r = readThrough(flat, [LENSES.mckee], "t")[0];
  expect(r.findings.some((f) => f.check === "mckee:crisis" && f.level === "error")).toBe(true);
  expect(r.proposals.find((p) => p.stage === "crisis")?.onScreen).toBe("kid must choose: win the race, or learn to lose");
});

test("the ST3b lenses read the launch story", () => {
  const r = Object.fromEntries(readThrough(launch, ["campbell", "vogler", "harmon", "save-the-cat", "kishotenketsu", "propp", "bernays"].map((k) => LENSES[k]), "30s").map((x) => [x.lens, x]));
  expect(r.campbell.map.call).toEqual(["b1"]);
  expect(r.campbell.map.boon).toEqual(["b3"]);
  expect(r.vogler.map.ordeal).toEqual(["b2"]);
  expect(r.harmon.findings.map((f) => f.check)).toContain("harmon:take");
  expect(r.kishotenketsu.map.ten).toEqual(["b3"]);
  expect(r.propp.map.lack).toEqual(["b1"]);
  expect(r.propp.map.liquidation).toEqual(["b3"]);
  expect(r.bernays.map).toMatchObject({ circumstance: ["b3"], witness: ["b3"], claim: ["b3"] });
  for (const x of Object.values(r)) expect(x.passes).toBe(true);
});

test("bernays refuses invented proof", () => {
  const bad = { ...launch, world: { ...launch.world, ships: { ...launch.world.ships, sets: [] } } };
  const r = readThrough(bad as typeof launch, [LENSES.bernays], "30s")[0];
  expect(r.findings.map((f) => f.check)).toContain("bernays:invented");
  expect(r.passes).toBe(false);
});

test("labels pin a lens's stages and hold the unlabelled ones in order", () => {
  const t = launch.tellings["30s"] as any[];
  const labelled = { ...launch, tellings: { "30s": [t[0], { ...t[1], stage: { campbell: "call" } }, t[2], { ...t[3], stage: { campbell: "boon" } }] } };
  const r = readThrough(labelled as typeof launch, [LENSES.campbell, LENSES.mckee], "30s");
  expect(r[0].map.call).toEqual(["b1"]);
  expect(r[0].map.boon).toEqual(["b3"]);
  // nothing inferred for a stage lands outside its labelled neighbours
  for (const k of r[0].map.trials ?? []) expect(["b2"]).toContain(k);
  // campbell's boon is campbell's: mckee still finds its own climax
  expect(r[1].map.climax).toEqual(["b3"]);
});

test("shots of one event count its turn once", () => {
  const t = launch.tellings["30s"] as any[];
  const shots = { ...launch, tellings: { "30s": [t[0], t[1], { ...t[2], id: "tries-a" }, { ...t[2], id: "tries-b" }, { ...t[2], id: "tries-c" }, t[3]] } };
  const found = checkStory(shots as typeof launch, "30s");
  expect(found.filter((f) => f.check === "turns")).toEqual([]);
  expect(found.filter((f) => f.check === "rhythm")).toEqual([]);
});

test("a beat can mean a finding: a breather the journey wants", () => {
  const t = launch.tellings["30s"] as any[];
  const world = { ...launch.world, tries: { ...launch.world.tries, turns: {} } };
  const noisy = { ...launch, world };
  const before = checkStory(noisy as typeof launch, "30s").filter((f) => f.about === "b2");
  expect(before.length).toBeGreaterThan(0);
  const meant = { ...launch, world, tellings: { "30s": [t[0], t[1], { ...t[2], meant: before.map((f) => f.check) }, t[3]] } };
  expect(checkStory(meant as typeof launch, "30s").filter((f) => f.about === "b2")).toEqual([]);
});

test("the launch lens reads the launch fixture and says its priors as notes", () => {
  const [r] = readThrough(launch, [LENSES.launch], "30s");
  expect(r.passes).toBe(true);
  expect(r.map.trouble.length + r.map.works.length).toBeGreaterThan(0);
  expect(r.findings.every((f) => f.level !== "error")).toBe(true);
});
