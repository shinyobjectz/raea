// Fixture: declared 3D animation (<Mesh animate>, a material's animate), sampled by the engine from the frame clock
// (raea-animate) with no useCurrentFrame: a box drops in on a spring curve and turns, a sphere rises from a Sequence,
// its colour and glow blend in Oklab. Render and the baked play with its 3D must agree (check.sh).
import { AbsoluteFill, Composition, Sequence, curve, registerRoot, track } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, Sphere } from "@raea/motion/3d";

const DROP = { y: track({ 0: 3, 24: 0.5 }, curve.spring({ damping: 12, stiffness: 140 })), rotateY: track({ 0: 0, 59: Math.PI }, "in-out-sine") };
const RISE = { y: track({ 0: -1.5, 14: 0.6 }, "out-back"), scale: track({ 0: 0.2, 14: 1 }, "out-cubic") };
const GLOW = { color: track({ 0: "#3060ff", 30: "#ff4070" }, "in-out-cubic"), emissiveIntensity: track({ 0: 0, 30: 2.5 }, "out-cubic") };

function Shot() {
  return (
    <AbsoluteFill style={{ background: "#141a26" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 2.2, 7]} lookAt={[0, 0.6, 0]} fov={40} />
        <AmbientLight intensity={0.5} />
        <DirectionalLight position={[3, 6, 4]} intensity={1.3} />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[10, 0.1, 6]} />
          <MeshStandardMaterial color="#d9d4c7" roughness={0.9} />
        </Mesh>
        <Mesh name="box" position={[-1.2, 3, 0]} animate={DROP}>
          <Box args={[1, 1, 1]} />
          <MeshStandardMaterial color="#ffb020" roughness={0.4} />
        </Mesh>
        <Sequence from={20} layout="none">
          <Mesh name="ball" position={[1.4, -1.5, 0]} animate={RISE}>
            <Sphere args={[0.6]} />
            <MeshStandardMaterial color="#3060ff" emissive="#ff4070" roughness={0.3} animate={GLOW} />
          </Mesh>
        </Sequence>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="animate3d" component={Shot} width={960} height={540} fps={30} durationInFrames={60} />);
