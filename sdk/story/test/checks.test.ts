import { expect, test } from "bun:test";
import { analyze, asker, checkStory, stateAt, type Curve, type Audience, beatsOf } from "../src/index.ts";
import { launch } from "./fixture.ts";

test("the launch story passes every exact check", () => {
  const f = checkStory(launch, "30s");
  expect(f.filter((x) => x.level !== "note")).toEqual([]);
});

test("derived: freedom falls then rises; the opening asks a question the last beat answers", () => {
  const w = analyze(launch, "30s");
  const curves = w.resource<Record<string, Curve>>("Curves")!;
  expect(curves.freedom.levels).toEqual([0, 0, -3, -3, 0]);
  expect(curves.freedom.shape).toBe("fall-rise");
  const aud = beatsOf(w).map((b) => w.must<Audience>(b.e, "Audience"));
  expect(aud[0].open).toEqual(["recipeRuns"]);
  expect(aud[3].answered).toEqual(["recipeRuns"]);
  expect(aud[3].open).toEqual([]);
});

test("the same seed gives the same story", () => {
  const a = analyze(launch, "30s", 7).toJSON(), b = analyze(launch, "30s", 7).toJSON();
  expect(a).toEqual(b);
});

const tiny = (world: Record<string, any>, beats: any[], extra: Record<string, any> = {}) =>
  ({ kind: "story", values: { v: { neg: "low", pos: "high" } }, agents: { a: {} }, world, tellings: { t: beats }, ...extra }) as any;

test("an event that unsets a fact takes it back from the audience and from a claim (bench/edges.ts)", () => {
  const s = tiny(
    { says: { who: "a", does: "says he was home", sets: ["alibi"], turns: { v: 1 } }, cam: { who: "a", does: "a camera shows him leaving", because: "says", unsets: ["alibi"], turns: { v: -2 } },
      claim: { who: "a", does: "swears to it again", because: "cam", claim: "he was home", proof: ["alibi"], turns: { v: 1 } } },
    [{ shows: ["says"] }, { shows: ["cam"] }, { shows: ["claim"] }],
    { facts: { alibi: "he was home all night" } },
  );
  expect(stateAt(s, "t", 2).knows).not.toContain("alibi");
  expect(checkStory(s, "t").some((f) => f.check === "claim" && f.about === "claim")).toBe(true);
});

test("two beats sharing an id, and a telling that shows nothing, are findings", () => {
  const e = { x: { who: "a", does: "x", turns: { v: 1 } }, y: { who: "a", does: "y", because: "x", turns: { v: 1 } } };
  expect(checkStory(tiny(e, [{ id: "k", shows: ["x"] }, { id: "k", shows: ["y"] }]), "t").some((f) => f.check === "id" && f.level === "error")).toBe(true);
  expect(checkStory(tiny(e, [{ shows: [] }, { shows: [] }]), "t").some((f) => f.check === "turns" && f.about === "story")).toBe(true);
});

test("asking about a beat the telling doesn't have says so", () => {
  expect(() => asker(tiny({ x: { who: "a", does: "x" } }, []), "t")(0)).toThrow(/no beat 1/);
});
