// Lights fixture: a point light orbiting with shadows, a soft spot from above, a dim sun; the camera dollies in
// (position and lookAt animated); a Group turns its children together; a pillar grows (non-uniform scale); a sphere
// appears at frame 30 from a Sequence.
import { AbsoluteFill, Composition, interpolate, registerRoot, Sequence, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, Cylinder, DirectionalLight, Group, Mesh, MeshStandardMaterial, PerspectiveCamera, PointLight, Sphere, SpotLight, Torus } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;

function Shot() {
  const f = useCurrentFrame();
  const a = (f / FRAMES) * Math.PI * 2;
  const dolly = interpolate(f, [0, FRAMES - 1], [9, 6.5]);
  const grow = interpolate(f, [0, FRAMES - 1], [0.4, 1.6]);
  return (
    <AbsoluteFill style={{ background: "#0b0d12" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 3, dolly]} lookAt={[0, 0.6, 0]} fov={45} />
        <AmbientLight intensity={0.15} />
        <DirectionalLight position={[-4, 6, 2]} intensity={0.25} castShadow={false} />
        <PointLight position={[Math.cos(a) * 2.5, 1.8, Math.sin(a) * 2.5]} intensity={1.2} color="#ffd29a" distance={12} castShadow />
        <SpotLight position={[0, 6, 0]} target={[0, 0, 0]} angle={0.5} penumbra={0.4} intensity={2.5} color="#9ad0ff" castShadow />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[12, 0.1, 12]} />
          <MeshStandardMaterial color="#9aa0a6" roughness={0.8} />
        </Mesh>
        <Group position={[0, 0.8, 0]} rotation={[0, a / 2, 0]}>
          <Mesh position={[-1.4, 0, 0]}>
            <Box args={[0.8, 0.8, 0.8]} />
            <MeshStandardMaterial color="#e4572e" roughness={0.4} />
          </Mesh>
          <Mesh position={[1.4, 0, 0]} rotation={[Math.PI / 2, 0, 0]}>
            <Torus args={[0.45, 0.16]} />
            <MeshStandardMaterial color="#29335c" roughness={0.3} metalness={0.5} />
          </Mesh>
        </Group>
        <Mesh position={[0, grow / 2, -1.5]} scale={[0.5, grow, 0.5]}>
          <Cylinder args={[0.5, 0.5, 1]} />
          <MeshStandardMaterial color="#f3a712" roughness={0.5} />
        </Mesh>
        <Sequence from={30} layout="none">
          <Mesh name="late-ball" position={[0, 0.5, 1.6]}>
            <Sphere args={[0.5]} />
            <MeshStandardMaterial color="#669bbc" roughness={0.2} />
          </Mesh>
        </Sequence>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="lights" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
