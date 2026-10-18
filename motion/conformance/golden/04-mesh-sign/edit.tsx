// golden case 04: a Manifold shape in Bevy (a sign with SALE cut through it), turning, under a camera and two lights
import { AbsoluteFill, interpolate, useCurrentFrame } from "../../../src";
import { AmbientLight, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera } from "../../../src/3d";

export default function Sign() {
  const frame = useCurrentFrame();
  const turn = interpolate(frame, [0, 59], [-0.5, 0.5]);
  return (
    <AbsoluteFill style={{ background: "#101418" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 0, 6]} fov={50} />
        <AmbientLight intensity={1} />
        <DirectionalLight position={[2, 3, 4]} intensity={1} />
        <Mesh shape={'minus(rounded_box(2.4, 1.1, 0.25, 0.08), text3d("SALE", 0.62, 0.6))'} rotation={[0, turn, 0]}>
          <MeshStandardMaterial color="#e11d48" roughness={0.4} metalness={0} />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}
