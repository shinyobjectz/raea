// Model fixture: a glTF (media/spinner.glb, made by make_glb.py) with its "Spin" clip posed at the frame's time, a
// second copy playing "Bob" from a Sequence, on a floor under a key light with shadows.
import { AbsoluteFill, Composition, registerRoot, Sequence, staticFile } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, Model, PerspectiveCamera } from "@raea/motion/3d";

const FPS = 30;

function Shot() {
  return (
    <AbsoluteFill style={{ background: "#1b2230" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 2.2, 5.5]} lookAt={[0, 0.7, 0]} fov={40} />
        <AmbientLight intensity={0.4} />
        <DirectionalLight position={[3, 6, 4]} intensity={1.2} castShadow />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[8, 0.1, 8]} />
          <MeshStandardMaterial color="#d9d4c7" roughness={0.9} />
        </Mesh>
        <Model src={staticFile("spinner.glb")} animation="Spin" position={[-1, 0, 0]} />
        <Sequence from={20} layout="none">
          <Model src={staticFile("spinner.glb")} animation="Bob" position={[1.2, 0, 0]} scale={0.8} />
        </Sequence>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="model" component={Shot} width={960} height={540} fps={FPS} durationInFrames={60} />);
