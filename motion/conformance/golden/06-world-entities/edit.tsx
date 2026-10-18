// golden case 06: an ECS world drawn by the engine with no React element per entity: a ship (rounded box) with a gun
// (cylinder) as its child, turned by a system each frame; the gun's Transform is in the ship's space. The reference
// (edit.rrl) is lowered from reference.tsx, which places the same meshes by hand with <Mesh>.
import { AbsoluteFill, useCurrentFrame } from "../../../src";
import { AmbientLight, Canvas3D, DirectionalLight, Entities, PerspectiveCamera } from "../../../src/3d";
import { createWorld } from "../../../src/ecs";

const game = createWorld();
const ship = game.add({ Mesh: { shape: "rounded_box(2, 0.6, 1, 0.1)", color: "#2563eb", roughness: 0.5, metalness: 0 }, Transform: { x: 0, y: 0, z: 0, ry: 0 } });
game.add({ Mesh: { shape: "cylinder(0.15, 1.2)", color: "#f59e0b", roughness: 0.4, metalness: 0 }, Transform: { x: 0, y: 0.6, z: 0 } }, { parent: ship });
game.system((w, f) => w.insert(ship, { Transform: { x: 0, y: 0, z: 0, ry: f * 0.05 } }));

export default function Ships() {
  const frame = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "#0b1020" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.5, 6]} fov={50} />
        <AmbientLight intensity={1} />
        <DirectionalLight position={[2, 3, 4]} intensity={1} />
        <Entities world={game.at(frame)} />
      </Canvas3D>
    </AbsoluteFill>
  );
}
