// Fire fixture: a <Fire> on a stack of logs, burning up over the shot (its intensity keyed), the camera circling it,
// a second, smaller blue-white fire on a block that appears at frame 30 from a Sequence, under Bloom. Every pixel of a
// flame is a function of its time, so frame 59 rendered alone equals frame 59 reached in play.
import { AbsoluteFill, Composition, interpolate, registerRoot, Sequence, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Bloom, Box, Canvas3D, Cylinder, Fire, Mesh, MeshStandardMaterial, PerspectiveCamera, ToneMapping } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;

function Shot() {
  const f = useCurrentFrame();
  const a = interpolate(f, [0, FRAMES - 1], [-0.5, 0.4]);
  const burn = interpolate(f, [0, FRAMES - 1], [4, 9]);
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Canvas3D>
        <PerspectiveCamera position={[Math.sin(a) * 5, 1.6, Math.cos(a) * 5]} lookAt={[0, 1.1, 0]} fov={45} />
        <ToneMapping mode="agx" />
        <Bloom intensity={0.4} threshold={0.9} />
        <AmbientLight intensity={0.05} color="#6f86b8" />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[12, 0.1, 12]} />
          <MeshStandardMaterial color="#5a5248" roughness={0.9} />
        </Mesh>
        {[0, 1, 2, 3].map((i) => (
          <Mesh key={i} position={i < 2 ? [0, 0.11, i * 0.5 - 0.25] : [(i - 2) * 0.5 - 0.25, 0.33, 0]} rotation={i < 2 ? [0, 0, Math.PI / 2] : [Math.PI / 2, 0, 0]}>
            <Cylinder args={[0.11, 0.11, 1.4]} />
            <MeshStandardMaterial color="#3a2a1e" roughness={1} emissive="#ff4a10" emissiveIntensity={0.4} />
          </Mesh>
        ))}
        <Fire name="pyre" position={[0, 0.2, 0]} scale={2.2} intensity={burn} light={1.2} />
        <Mesh position={[2, 0.3, -1]}>
          <Box args={[0.6, 0.6, 0.6]} />
          <MeshStandardMaterial color="#8e8476" roughness={0.9} />
        </Mesh>
        <Sequence from={30} layout="none">
          <Fire name="lamp" position={[2, 0.6, -1]} scale={[0.35, 0.8, 0.35]} intensity={8} color="#8fb4ff" seed={3} light={0.4} />
        </Sequence>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="fire" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
