// the same picture as edit.tsx, placed by hand: the ship turned by 0.05 rad a frame, the gun 0.6 above its centre (a
// point on the turning axis, so it stays put while it turns with the ship)
import { AbsoluteFill, useCurrentFrame } from "../../../src";
import { AmbientLight, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera } from "../../../src/3d";

export default function Ships() {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "#0b1020" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.5, 6]} fov={50} />
        <AmbientLight intensity={1} />
        <DirectionalLight position={[2, 3, 4]} intensity={1} />
        <Mesh shape="rounded_box(2, 0.6, 1, 0.1)" rotation={[0, f * 0.05, 0]}>
          <MeshStandardMaterial color="#2563eb" roughness={0.5} metalness={0} />
        </Mesh>
        <Mesh shape="cylinder(0.15, 1.2)" position={[0, 0.6, 0]} rotation={[0, f * 0.05, 0]}>
          <MeshStandardMaterial color="#f59e0b" roughness={0.4} metalness={0} />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}
