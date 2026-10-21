// golden case 05: case 04's sign, made as a native solid (manifold.roundedBox(...).minus(manifold.text3d(...))) and
// drawn from an ECS world whose system turns it each frame; the same frames as the rrl edit
import { AbsoluteFill } from "../../../src";
import { Canvas3D, Scene, manifold } from "../../../src/3d";
import { createWorld } from "../../../src/ecs";

const sign = manifold.roundedBox(2.4, 1.1, 0.25, 0.08).minus(manifold.text3d("SALE", 0.62, 0.6));
const world = createWorld();
world.spawn("camera", { x: 0, y: 0, z: 6, fov: 50 });
world.spawn("ambient", { intensity: 1 });
world.spawn("light", { x: 2, y: 3, z: 4, intensity: 1 });
const m = world.spawn("mesh", { shape: sign.expr, color: "#e11d48", rough: 0.4, metal: 0, rot_y: -0.5 });
// after frame f's step the sign stands where frame f + 1 shows it: from -0.5 at frame 0 to 0.5 at frame 59
world.system((w, f) => w.set(m, { rot_y: -0.5 + (f + 1) / 59 }));

export default function WorldSign() {
  return (
    <AbsoluteFill style={{ background: "#101418" }}>
      <Canvas3D>
        <Scene world={world} />
      </Canvas3D>
    </AbsoluteFill>
  );
}
