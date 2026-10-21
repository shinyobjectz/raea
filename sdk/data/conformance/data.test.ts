import { describe, expect, test } from "bun:test";
import { component, copy, entity, f32, has, join, read, str, table, tag, u16, u8, without, world, write } from "../src/index";
import { facesTable, graphTables, renderTables, textOverFace } from "../src/facts";

const Position = component("Position", { x: f32, y: f32 });
const Velocity = component("Velocity", { x: f32, y: f32 });
const Health = component("Health", { current: u16, maximum: u16 });
const Sleeping = tag("Sleeping");

describe("components and queries", () => {
  test("a query binds what it asks for, in order, typed, and filters by has and without", () => {
    const w = world({ native: false });
    const a = w.spawn(Position.of({ x: 0, y: 0 }), Velocity.of({ x: 3, y: 1 }));
    w.spawn(Position.of({ x: 5, y: 5 }), Velocity.of({ x: 1, y: 1 }), Sleeping);
    w.spawn(Position.of({ x: 9, y: 9 }));
    const seen: number[] = [];
    for (const [e, p, v] of w.query(entity, write(Position), read(Velocity), without(Sleeping))) {
      seen.push(e);
      p.x += v.x * 2; // number, writable
      p.y += v.y * 2;
      // @ts-expect-error a read view is readonly (checked by the type checker; at runtime it throws, tested below)
      if (false as boolean) v.x = 0;
    }
    expect(seen).toEqual([a]);
    expect(w.get(a, Position)).toEqual({ x: 6, y: 2 });
    expect([...w.query(entity, has(Sleeping))].length).toBe(1);
  });

  test("writing through a read view, and keeping a view past its row, both throw", () => {
    const w = world({ native: false });
    w.spawn(Position.of({ x: 1, y: 1 }), Velocity.of({ x: 0, y: 0 }));
    expect(() => { for (const [v] of w.query(read(Velocity))) (v as any).x = 2; }).toThrow(/read-only/);
    const kept: any[] = [];
    for (const [p] of w.query(write(Position))) kept.push(p);
    expect(() => kept[0].x).toThrow(/escaped its query/);
    const copies: any[] = [];
    for (const [p] of w.query(read(Position))) copies.push(copy(p));
    expect(copies[0].x).toBe(1);
  });

  test("storage types coerce as their columns would", () => {
    const [, h] = Health.of({ current: 70000, maximum: 100 });
    expect(h.current).toBe(70000 & 0xffff);
    const [, p] = Position.of({ x: 0.1, y: 0 });
    expect(p.x).toBe(Math.fround(0.1));
  });

  test("a system's structural changes wait for it to end, and frames replay the same", () => {
    const w = world({ native: false });
    w.spawn(Health.of({ current: 0, maximum: 10 }));
    w.spawn(Health.of({ current: 5, maximum: 10 }));
    w.system("remove the dead", { reads: [Health] }, (d, f, cmd) => {
      for (const [e, h] of d.query(entity, read(Health))) if (h.current === 0) cmd.despawn(e);
      if (f === 0) expect([...d.query(read(Health))].length).toBe(2); // on the frame it's asked for: commands apply after
    });
    expect(w.systems[0]).toEqual({ name: "remove the dead", reads: ["Health"], writes: [] });
    const later = new (w.constructor as any)(w.motion.at(2));
    expect([...later.query(read(Health))].length).toBe(1);
  });
});

describe("tables", () => {
  const Sample = { t: f32, temp: f32, valid: u8, label: str };
  test("columns are typed arrays; rows are views; where and join work", () => {
    const s = table(Sample, [{ t: 0, temp: 20, valid: 1, label: "a" }, { t: 1, temp: 21, valid: 0, label: "b" }], "samples");
    expect(s.column("temp")).toBeInstanceOf(Float32Array);
    for (const r of s.rows("write")) if (r.valid) r.temp += 1;
    expect(s.row(0).temp).toBe(21);
    expect(s.row(1).temp).toBe(21);
    expect(s.where((r) => r.label === "b").length).toBe(1);
    for (let i = 0; i < 100; i++) s.push({ t: i, temp: i, valid: 1, label: String(i) }); // grows past its first 16
    expect(s.length).toBe(102);
    const names = table({ label: str, who: str }, [{ label: "a", who: "June" }]);
    expect(join(s, "label", names, "label")).toEqual([[{ t: 0, temp: 21, valid: 1, label: "a" }, { label: "a", who: "June" }]]);
  });
});

describe("facts", () => {
  test("a brief's graph becomes lines, parts, beats and checks", () => {
    const g = { items: [
      { type: "line", section: "hook", take: "TAKE01", speaker: "SHAWN", text: "Why so expensive?", delivery: "direct", punch: null,
        notes: [{ kind: "lockup", props: {}, utils: ["on:'expensive'", "place-top"], anchor: "expensive" }] },
      { type: "beat", section: "hook", kind: "product", phrase: "the tubs", find: ["hands", { picture: "media/pack.png" }] },
    ], checks: [{ rule: "nothing covers the face", form: "must" }] };
    const t = graphTables(g);
    expect(t.lines.row(0)).toMatchObject({ scene: "hook", ord: 1, speaker: "SHAWN", parts: "lockup" });
    expect(t.parts.row(0)).toMatchObject({ kind: "lockup", anchor: "expensive", place: "top" });
    expect(t.beats.row(0).find).toBe("hands media/pack.png");
    expect(t.checks.length).toBe(1);
  });

  test("text over the face is a join of the render's text and the faces on frame", () => {
    const faces = facesTable({ "0": { face: { x: 400, y: 600, w: 300, h: 300 } }, "10": {} }, 20);
    expect(faces.length).toBe(10); // frames 0-9 have a face; from 10 none
    const { text } = renderTables([
      { frame: 3, videos: [], text: [{ text: "WHY ARE WE", box: [380, 650, 400, 120], opacity: 1 }] },   // over the face
      { frame: 4, videos: [], text: [{ text: "so expensive", box: [100, 200, 300, 80], opacity: 1 }] },  // clear of it
      { frame: 12, videos: [], text: [{ text: "late", box: [400, 600, 300, 300], opacity: 1 }] },       // no face then
    ]);
    const bad = textOverFace(text, faces);
    expect(bad.map((b) => b.frame)).toEqual([3]);
    expect(bad[0].covers).toBeGreaterThan(0.08);
  });
});

import { Beat, Line, Part, Scene, lineAt, loadBrief, partsAt } from "../src/brief";

describe("a brief as entities", () => {
  const graph = {
    sections: [{ name: "hook" }, { name: "proof" }],
    items: [
      { type: "line", section: "hook", speaker: "SHAWN", take: "TAKE01", text: "Why so expensive?" },
      { type: "line", section: "proof", speaker: "SHAWN", take: "TAKE01", text: "It's GRAS verified." },
      { type: "beat", section: "proof", kind: "product", phrase: "the tubs on a counter", find: ["hands"] },
    ],
  };
  const ir = { timeline: {
    clips: [{ line: 0, start: 0, end: 40 }, { line: 1, start: 40, end: 100 }, { line: -1, start: 100, end: 160 }],
    parts: [{ kind: "lockup", component: "lockup", start: 10, end: 40, band: "top" }, { kind: "statcard", component: "statcard", start: 55, end: 100, band: "low" }],
  } };

  test("scenes, lines, parts and beats are entities with typed components", () => {
    const w = world({ native: false });
    const ids = loadBrief(w, graph, ir);
    expect(ids.scenes.length).toBe(2);
    expect([...w.query(read(Line))].length).toBe(2);
    expect([...w.query(read(Part))].length).toBe(2);
    // views live for one step of the loop: copy what's kept (spreading the query and reading later would throw)
    const beats = [...(function* () { for (const [b] of w.query(read(Beat))) yield copy(b); })()];
    expect(beats[0]).toMatchObject({ kind: "product", bound: true, from: 100, to: 160 });
    const names: string[] = [];
    for (const [s] of w.query(read(Scene))) names.push(s.name);
    expect(names).toEqual(["hook", "proof"]);
  });

  test("which line is said at frame f, and which parts show then, by query", () => {
    const w = world({ native: false });
    loadBrief(w, graph, ir);
    expect(lineAt(w, 20)?.text).toBe("Why so expensive?");
    expect(partsAt(w, 20).map((p) => p.kind)).toEqual(["lockup"]);
    expect(lineAt(w, 60)?.text).toBe("It's GRAS verified.");
    expect(partsAt(w, 60).map((p) => [p.kind, p.line])).toEqual([["statcard", 2]]);
    expect(lineAt(w, 120)).toBeNull(); // the beat's clip: no line said
    expect(partsAt(w, 120)).toEqual([]);
  });
});

import { check } from "../src/check";
import { mkdtempSync, mkdirSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join as pjoin } from "path";
import { spawnSync } from "child_process";

describe("check: a brief's rules on a render", () => {
  test("never says, black frames, caption size, the safe zone and the length, on a one-second black render", async () => {
    const space = mkdtempSync(pjoin(tmpdir(), "raea-check-test-"));
    mkdirSync(pjoin(space, ".score"));
    spawnSync("ffmpeg", ["-v", "error", "-f", "lavfi", "-i", "color=c=black:s=1080x1920:r=30:d=1", "-pix_fmt", "yuv420p", pjoin(space, "edit.mp4")]);
    const frames = Array.from({ length: 30 }, (_, f) => ({ frame: f, videos: [], text: [{ text: "tiny", box: [100, 50, 200, 20], opacity: 1 }] }));
    writeFileSync(pjoin(space, "render.json"), JSON.stringify({ fps: 30, width: 1080, height: 1920, frames }));
    writeFileSync(pjoin(space, "words.json"), JSON.stringify([{ text: "this", from: 2, to: 6 }, { text: "is", from: 6, to: 9 }, { text: "a", from: 9, to: 10 }, { text: "game", from: 10, to: 14 }, { text: "changer", from: 14, to: 22 }]));
    writeFileSync(pjoin(space, "perception.json"), JSON.stringify({ "0": { safe: { top: 220, bottom: 1500 } } }));
    writeFileSync(pjoin(space, ".score", "edit.graph.json"), JSON.stringify({ deliverables: [{ ratio: "9:16", secs: [0.85, 1.15] }], items: [], checks: [] }));
    writeFileSync(pjoin(space, "checks.json"), JSON.stringify([
      { rule: 'never says "game changer"', form: "never-says", args: ["game changer"] },
      { rule: "no black frames", form: "must", args: ["no black frames"] },
      { rule: "captions read on a phone", form: "must", args: ["captions read on a phone"] },
      { rule: "everything inside the safe zone", form: "must", args: ["everything inside the safe zone"] },
      { rule: "the length is within the format", form: "must", args: ["the length is within the format"] },
    ]));
    writeFileSync(pjoin(space, ".score", "moments.jsonl"), "");
    const r = await check(space, "edit", pjoin(space, "edit.mp4"));
    const by = Object.fromEntries(r.results.map((x) => [x.rule, x.status]));
    expect(by['never says "game changer"']).toBe("fail");
    expect(by["no black frames"]).toBe("fail");
    expect(by["captions read on a phone"]).toBe("fail");       // 20 px tall, under 2.5% of 1920
    expect(by["everything inside the safe zone"]).toBe("fail"); // y 50, above the safe top of 220
    expect(by["the length is within the format"]).toBe("pass");  // 1 s of 0.85-1.15
    expect(r.code).toBe(1);
  }, 120000);
});
