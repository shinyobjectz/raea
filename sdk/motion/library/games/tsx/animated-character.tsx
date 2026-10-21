// record: robot-expressive, bevy-animation-animated-mesh
// A rigged glTF character playing its clips: the Bevy "animated mesh" example in raea's SDK. It expects
// media/RobotExpressive.glb, fetched with `raea-meaning games-fetch robot-expressive --into media` (CC0, Quaternius).
// One robot walks across the floor (clip "Walking", its position from the frame), a second waves from a Sequence.
import { AbsoluteFill, Composition, interpolate, registerRoot, Sequence, staticFile, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, Model, PerspectiveCamera } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 90;
// the model is about 4.5 units tall at scale 1, its origin at the feet, facing +z
const SCALE = 0.4;

function Shot() {
  const f = useCurrentFrame();
  const x = interpolate(f, [0, FRAMES - 1], [-2.4, 0.6]);
  return (
    <AbsoluteFill style={{ background: "#1b2230" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 2.4, 7]} lookAt={[0, 0.9, 0]} fov={40} />
        <AmbientLight intensity={0.45} />
        <DirectionalLight position={[3, 6, 4]} intensity={1.3} castShadow />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[12, 0.1, 8]} />
          <MeshStandardMaterial color="#d9d4c7" roughness={0.9} />
        </Mesh>
        <Model name="walker" src={staticFile("RobotExpressive.glb")} animation="Walking" position={[x, 0, 0]} rotation={[0, Math.PI / 2, 0]} scale={SCALE} />
        <Sequence from={15} layout="none">
          <Model name="waver" src={staticFile("RobotExpressive.glb")} animation="Wave" position={[2, 0, -1]} rotation={[0, -0.5, 0]} scale={SCALE} />
        </Sequence>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="character" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
