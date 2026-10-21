// record: bevy-3d-rendering-3d-shapes
// Bevy's "3D shapes" example in raea's SDK: a row of primitives turning on a floor under a key light.
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, Cone, Cylinder, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, Sphere, Torus } from "@raea/motion/3d";

const COLOURS = ["#e76f51", "#f4a261", "#e9c46a", "#2a9d8f", "#457b9d"];

function Shapes() {
  const f = useCurrentFrame();
  const spin = f * 0.04;
  const shapes = [<Box args={[0.9, 0.9, 0.9]} />, <Sphere args={[0.55]} />, <Cylinder args={[0.45, 0.45, 1]} />, <Cone args={[0.55, 1.1]} />, <Torus args={[0.42, 0.16]} />];
  return (
    <AbsoluteFill style={{ background: "#14181f" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 3, 8]} lookAt={[0, 0.5, 0]} fov={40} />
        <AmbientLight intensity={0.35} />
        <DirectionalLight position={[4, 8, 6]} intensity={1.3} castShadow />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[14, 0.1, 6]} />
          <MeshStandardMaterial color="#cfc8ba" roughness={0.95} />
        </Mesh>
        {shapes.map((g, i) => (
          <Mesh key={i} name={`shape-${i}`} position={[(i - 2) * 2, 0.75, 0]} rotation={[spin * 0.5, spin + i, 0]}>
            {g}
            <MeshStandardMaterial color={COLOURS[i]} roughness={0.45} />
          </Mesh>
        ))}
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="shapes" component={Shapes} width={1280} height={720} fps={30} durationInFrames={90} />);
