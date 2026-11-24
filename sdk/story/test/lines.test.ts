import { expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { fromData, fromLines, stateAt, storySchema, toData, toLines, windowAt } from "../src/index.ts";

// only what the schema defines (a corpus story can carry a field the model has no place for)
const prune = (x: any, sc: any): any => {
  if (!sc || x === null || typeof x !== "object") return x;
  if (sc.anyOf) return prune(x, sc.anyOf.find((o: any) => (o.type === "array") === Array.isArray(x)) ?? sc.anyOf[0]);
  if (Array.isArray(x)) return x.map((y) => prune(y, sc.items));
  const props = sc.properties ?? {}, extra = sc.additionalProperties;
  return Object.fromEntries(Object.entries(x).filter(([k]) => k in props || typeof extra === "object").map(([k, v]) => [k, prune(v, props[k] ?? extra)]));
};
import { launch } from "./fixture.ts";

// the same story, whatever the spelling: a one-item list or its item, an empty list or none, key order
const norm = (x: any, k = ""): any => {
  if (x === null || x === "" || x === false) return undefined;
  if (k === "says" && Array.isArray(x)) x = x.filter((l) => l?.says != null && l.says !== "");
  if (Array.isArray(x)) return x.length ? x.map((y) => norm(y)) : undefined;
  if (x && typeof x === "object") {
    const o = Object.fromEntries(Object.keys(x).sort().map((key) => [key, norm(x[key], key)]).filter(([, v]) => v !== undefined && !(typeof v === "object" && !Array.isArray(v) && !Object.keys(v).length)));
    return Object.keys(o).length ? o : undefined;
  }
  if (["because", "setup", "pays", "meant", "believes", "needs", "sets", "unsets", "learns", "proof", "shows", "reveals", "hides", "asks", "with", "dilemma", "lenses"].includes(k) && typeof x === "string") return [x];
  return x;
};

test("the launch story reads back from its lines exactly", () => {
  const lines = toLines(launch);
  const { story, errors } = fromLines(lines);
  expect(errors).toEqual([]);
  expect(norm(toData(story!))).toEqual(norm(toData(launch)));
  expect(lines).toContain("EVENT ships: team with raea -- ships fifty checked cuts by Friday | because tries | sets recipeRuns | proof recipeRuns | turns trust+2 freedom+3 | ends team:need | claim fifty checked cuts");
});

test("a broken line says where", () => {
  const { errors } = fromLines("VALUE v: low / high\nEVENT a: no arrow here\nBEAT b0: shows a");
  expect(errors.map((e) => e.at)).toEqual(["line 2", "line 3", "tellings"]);
});

const corpus = `${homedir()}/.vd/story/stories.jsonl`;
test.skipIf(!existsSync(corpus))("every corpus story reads back from its lines exactly", () => {
  const rows = readFileSync(corpus, "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l).story).filter((s) => !fromData(s).errors.length);
  expect(rows.length).toBeGreaterThan(1000);
  const bad = rows.filter((s) => { const r = fromLines(toLines(s)); return r.errors.length || JSON.stringify(norm({ ...toData(r.story!) })) !== JSON.stringify(norm(prune(s, storySchema()))); });
  if (bad.length) { const r = fromLines(toLines(bad[0])); console.log(r.errors.slice(0, 3)); const a = JSON.stringify(norm(toData(r.story!))), b = JSON.stringify(norm(prune(bad[0], storySchema()))); let i = 0; while (a[i] === b[i]) i++; console.log(i, a.slice(Math.max(0, i - 80), i + 80), "\n---\n", b.slice(Math.max(0, i - 80), i + 80)); }
  expect(bad.length).toBe(0);
});

test("the state before a beat stands in for the beats before it", () => {
  const st0 = stateAt(launch, "30s", 0);
  expect(st0.shown).toBe(0);
  expect(st0.ahead.length).toBe(Object.keys(launch.world).length);
  const end = stateAt(launch, "30s", (launch.tellings["30s"] as unknown[]).length);
  expect(end.ahead).toEqual([]);
  expect(end.unpaid).toEqual([]);
  expect(end.unsettled.team ?? []).not.toContain("need");
});

test("a window holds the state and the beats around the one to write, not the whole telling", () => {
  const w = windowAt(launch, "30s", 2, { before: 1, after: 1 });
  expect(w).toContain("STATE before beat 3");
  expect(w).toContain("WRITE beat 3");
  expect(w.split("\n").filter((l) => l.startsWith("BEAT")).length).toBe(2);
});
