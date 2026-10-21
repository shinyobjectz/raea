// @raea/motion/ecs under plain bun (TypeScript tables; the engine's native tables are checked by
// examples/headless/solids-and-worlds.ts through raea-motion run), and solids saying they need the engine.
import { describe, expect, test } from "bun:test";
import { createWorld } from "../src/ecs";
import { manifold } from "../src/3d";
import { hasNative } from "../src/native";

describe("ecs", () => {
  test("spawn, get, set, despawn, count", () => {
    const w = createWorld();
    expect(w.native).toBe(hasNative());
    const a = w.spawn("ball", { x: 1, y: 2, color: "red" });
    const b = w.spawn("ball", { x: 5, y: 0 });
    w.spawn("wall", { x: 0 });
    expect(w.count("ball")).toBe(2);
    expect(w.get(a)).toEqual({ id: a, kind: "ball", x: 1, y: 2, color: "red" });
    w.set(b, { y: 9 });
    expect(w.get(b).y).toBe(9);
    w.despawn(a);
    expect(w.count("ball")).toBe(1);
    expect(() => w.get(a)).toThrow(/no entity/);
  });

  test("queries filter by number and string columns", () => {
    const w = createWorld();
    for (let i = 0; i < 10; i++) w.spawn("p", { v: i, side: i % 2 ? "odd" : "even" });
    expect(w.query("p", [["v", ">=", 7]]).length).toBe(3);
    expect(w.query("p", [["side", "==", "odd"], ["v", "<", 5]]).length).toBe(2);
    expect(w.column("p", "v")).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
  });

  test("at(frame) runs the systems for frames before it, the same each time", () => {
    const w = createWorld();
    const id = w.spawn("dot", { x: 0 });
    w.system((world, f) => world.set(id, { x: (world.get(id).x as number) + f + 1 }));
    expect(w.at(0).get(id).x).toBe(0);
    expect(w.at(3).get(id).x).toBe(1 + 2 + 3);
    expect(w.at(3).get(id).x).toBe(6);
    expect(w.at(1).get(id).x).toBe(1);
    expect(w.get(id).x).toBe(0);
  });

  test.skipIf(hasNative())("solids need the engine", () => {
    expect(() => manifold.box(1, 1, 1)).toThrow(/needs the raea engine/);
  });
});

describe("ecs: components, queries, hierarchy, resources, events (the engine's Bevy world and TypeScript's agree)", () => {
  test("entities hold several components; queries take with, without and field conditions", () => {
    const w = createWorld();
    const a = w.add({ Position: { x: 1, y: 2 }, Velocity: { dx: 1, dy: 0 }, Ball: {} });
    const b = w.add({ Position: { x: 5, y: 0 }, Ball: {}, Frozen: {} });
    const c = w.add({ Position: { x: 9, y: 9 }, Wall: { solid: true } });
    expect(w.find({ with: ["Position"] })).toEqual([a, b, c]);
    expect(w.find({ with: ["Ball"], without: ["Frozen"] })).toEqual([a]);
    expect(w.find({ with: ["Position"], where: [["x", ">", 2]] })).toEqual([b, c]);
    expect(w.find({ with: ["Position", "Wall"], where: [["Wall.solid", "==", true]] })).toEqual([c]);
    expect(w.components(a)).toEqual({ Ball: {}, Position: { x: 1, y: 2 }, Velocity: { dx: 1, dy: 0 } });
    w.insert(b, { Velocity: { dx: 0, dy: -1 } });
    w.remove(b, "Frozen");
    expect(w.find({ with: ["Velocity", "Ball"] })).toEqual([a, b]);
    expect(w.component<{ dy: number }>(b, "Velocity")?.dy).toBe(-1);
    expect(w.component(c, "Velocity")).toBeUndefined();
  });

  test("parent and child; despawning a parent takes its children", () => {
    const w = createWorld();
    const ship = w.add({ Ship: {} });
    const gun = w.add({ Gun: {} }, { parent: ship });
    const barrel = w.add({ Barrel: {} });
    w.setParent(barrel, gun);
    expect(w.children(ship)).toEqual([gun]);
    expect(w.parent(barrel)).toBe(gun);
    w.setParent(barrel, null);
    expect(w.parent(barrel)).toBeNull();
    w.setParent(barrel, gun);
    w.despawn(ship);
    expect(() => w.components(gun)).toThrow(/no entity/);
    expect(() => w.components(barrel)).toThrow(/no entity/);
  });

  test("resources, and events that last one frame", () => {
    const w = createWorld();
    w.resource("Gravity", { g: 9.8 });
    expect(w.resource<{ g: number }>("Gravity")?.g).toBe(9.8);
    expect(w.resource("Nothing")).toBeNull();
    w.add({ Ball: {}, Position: { y: 10 } });
    const heard: number[] = [];
    w.system((world, f) => {
      for (const id of world.find({ with: ["Ball", "Position"] })) {
        const p = world.component<{ y: number }>(id, "Position")!;
        const y = p.y - 3;
        world.insert(id, { Position: { y } });
        if (y < 0) world.send("Bounce", { id, frame: f });
      }
    });
    w.system((world) => {
      heard.push(world.events("Bounce").length);
    });
    const at4 = w.at(4);
    expect(at4.component<{ y: number }>(at4.find({ with: ["Ball"] })[0], "Position")?.y).toBe(-2);
    expect(heard).toEqual([0, 0, 0, 1]);
  });

  test("at(frame) is the same each time and leaves the world as built", () => {
    const w = createWorld();
    const id = w.add({ Counter: { n: 0 } });
    w.system((world) => world.insert(id, { Counter: { n: (world.component<{ n: number }>(id, "Counter")!.n) + 1 } }));
    expect(w.at(5).component<{ n: number }>(id, "Counter")?.n).toBe(5);
    expect(w.at(3).component<{ n: number }>(id, "Counter")?.n).toBe(3);
    expect(w.at(5).component<{ n: number }>(id, "Counter")?.n).toBe(5);
    expect(w.component<{ n: number }>(id, "Counter")?.n).toBe(0);
  });
});
