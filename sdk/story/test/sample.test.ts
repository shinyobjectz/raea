import { expect, test } from "bun:test";
import { choose, compileScript, compileStorylets, dramaManager, LENSES, matrix, sample, samples, story, toInk, value } from "../src/index.ts";
import { launch } from "./fixture.ts";

// a pattern: one launch shape over any product, with optional and alternative beats
const pattern = (t: { who: string; pain: string; win: string }) => story({
  title: t.who, idea: { value: "time", side: `${t.win}` },
  values: { time: value("lost", "won") },
  facts: { works: "it works" },
  agents: { them: { want: t.win, role: "customer", mask: "hero" }, product: { role: "brand", mask: "mentor" } },
  world: {
    day: { who: "them", does: `every week, ${t.pain}`, routine: true, turns: { time: -1 } },
    worse: { who: "them", does: choose([`${t.pain} doubles`, `a deadline makes ${t.pain} worse`]), because: "day", turns: { time: -2 } },
    choice: { who: "them", does: "decides to try something new", because: "worse", dilemma: ["keep going", "try it"] },
    win: { who: "them", with: ["product"], does: t.win, because: "choice", sets: ["works"], turns: { time: 3 }, ends: { them: "want" }, claim: t.win, proof: ["works"] },
  },
  tellings: {
    ad: choose([
      [{ shows: ["day"] }, { shows: ["worse"] }, { shows: ["choice"] }, { shows: ["win"] }],
      [{ shows: ["win"], hides: ["works"], asks: ["works"] }, { shows: ["day"] }, { shows: ["worse"] }, choose([{ shows: ["choice"] }, null]), { shows: ["win"], reveals: ["works"] }],
    ]),
  },
});

test("a sampled story always passes, and seeds give different ones", () => {
  const s = pattern({ who: "Northwind Coffee", pain: "the roast log is done by hand", win: "the log fills itself" });
  const got = samples(s, 4, { lenses: [LENSES.mckee, LENSES.pas, LENSES.storynomics] });
  expect(got.length).toBeGreaterThan(1);
  for (const g of got) expect(g.passes).toBe(true);
  expect(sample(s, { seed: 3 }).story).toEqual(sample(s, { seed: 3 }).story);
});

test("the theme matrix says where a pattern breaks", () => {
  const m = matrix(pattern, {
    coffee: { who: "Northwind Coffee", pain: "the roast log is done by hand", win: "the log fills itself" },
    skincare: { who: "Lumen Skincare", pain: "every shade is shot again", win: "every shade from one shoot" },
  }, { lenses: [LENSES.mckee, LENSES.pas] });
  expect(Object.keys(m.byTheme)).toEqual(["coffee", "skincare"]);
  expect(m.rows.length).toBe(10);
});

test("a telling compiles to a Script, the world to storylets a drama manager plays in causal order, lines to Ink", () => {
  const sc = compileScript(launch, "30s");
  expect(sc.scenes.length).toBe(4);
  expect(sc.scenes[0].heading).toBe("B0 · curiosity");
  expect(sc.scenes[3].payoff).toBe("pays off how; answers recipeRuns");
  expect(sc.scenes.map((x) => x.beat)).toEqual(["b0", "b1", "b2", "b3"]);
  const played = dramaManager(compileStorylets(launch), [0.2, 0.6, 0.4, 1]);
  expect(played).toEqual(["week", "doubles", "tries", "ships"]);
  const withLines = { ...launch, tellings: { "30s": [{ shows: ["tries" as const], says: [{ who: "team" as const, says: "Fine. One recipe.", means: "I don't believe it" }] }] } };
  expect(toInk(withLines, "30s")).toContain("team: Fine. One recipe. // means: I don't believe it");
});
