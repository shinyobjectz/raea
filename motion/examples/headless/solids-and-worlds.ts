// Headless TypeScript against the engine (raea-motion run): Manifold solids as native handles, and an ECS world whose
// tables live in Rust, each checked against answers known in closed form, and a world stepped by a system.
//   raea-motion run motion/examples/headless/solids-and-worlds.ts
import { manifold } from "@raea/motion/3d";
import { createWorld } from "@raea/motion/ecs";
import { print } from "@raea/motion/native";

let failed = 0;
function check(what: string, got: number, want: number, tol = 1e-6) {
  const ok = Math.abs(got - want) <= tol;
  if (!ok) failed++;
  print(`${ok ? "PASS" : "FAIL"}  ${what}: ${got.toFixed(6)} (want ${want.toFixed(6)})`);
}
const now = () => (globalThis as any).__raea_now() as number;

export default function main() {
  // solids
  const box = manifold.box(2, 1, 0.5);
  check("box(2, 1, 0.5) volume", box.volume(), 1);
  const bored = manifold.box(2, 2, 2).minus(manifold.cylinder(0.5, 3));
  check("a box with a 0.5 bore through it", bored.volume(), 8 - Math.PI * 0.25 * 2, 0.02);
  check("its middle slice's area", bored.slice(0).area, 4 - Math.PI * 0.25, 0.02);
  check("the bore is a hole: contours in the slice", bored.slice(0).contours, 2, 0);
  const moved = box.move(0, 0, 5);
  check("moved up 5: bottom", moved.bounds().min[2], 4.75);
  const sign = manifold.roundedBox(2.4, 1.1, 0.25, 0.08).minus(manifold.text3d("SALE", 0.62, 0.6));
  check("SALE cut through the sign takes volume away", sign.volume() < manifold.roundedBox(2.4, 1.1, 0.25, 0.08).volume() ? 1 : 0, 1, 0);
  print(`      the sign: ${sign.triangles()} triangles, expression ${sign.expr.slice(0, 60)}...`);
  const nothing = box.minus(manifold.box(3, 3, 3));
  check("a boolean that leaves nothing is empty", nothing.isEmpty() ? 1 : 0, 1, 0);

  // a world: 10 000 balls falling one unit a frame; a system takes away those past y 95
  const w = createWorld();
  check("the world's tables are native", w.native ? 1 : 0, 1, 0);
  for (let i = 0; i < 10000; i++) w.spawn("ball", { x: i % 100, y: Math.floor(i / 100), color: i % 2 ? "red" : "blue" });
  w.system((world) => {
    for (const id of world.query("ball", [["y", ">=", 95]])) world.despawn(id);
    for (const id of world.query("ball")) world.set(id, { y: world.get(id).y as number + 1 });
  });
  check("before frame 0: 10 000 balls", w.count("ball"), 10000, 0);
  // rows 0..99 start at y 0..99; after f frames the ones that started at y >= 95 - f + 1 are gone
  check("after 10 frames (frame 10)", w.at(10).count("ball"), 100 * 86, 0);
  check("frame 10 asked again is the same", w.at(10).count("ball"), 100 * 86, 0);
  check("a query by string column", w.query("ball", [["color", "==", "red"]]).length, 5000, 0);

  // speed: one filter over 100 000 rows, native tables against TypeScript's, in the same (jitless) V8
  for (const native of [true, false]) {
    const big = createWorld({ native });
    for (let i = 0; i < 100000; i++) big.spawn("p", { v: i % 1000 });
    const t = now();
    let n = 0;
    for (let k = 0; k < 20; k++) n = big.query("p", [["v", "<", 100]]).length;
    print(`      ${native ? "native" : "TypeScript"} tables: 20 filters over 100 000 rows in ${(now() - t).toFixed(1)} ms (${n} each)`);
  }
  print(failed === 0 ? "all checks pass" : `${failed} checks FAIL`);
  if (failed) throw new Error(`${failed} checks failed`);
}
