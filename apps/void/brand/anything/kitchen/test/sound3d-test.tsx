// <Sound3D> heard from a turning camera: a 440 Hz tone 3 m to the camera's right at the start; the camera turns 180
// degrees over 4 s, so it ends on the left. Render (RAEA_AUDIO_BOUNCE) and live play (RAEA_AUDIO_TAP) alike.
import { AbsoluteFill, Composition, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, Mesh, MeshStandardMaterial, PerspectiveCamera } from "@raea/motion/3d";
import { Sound3D } from "@raea/motion/seedling";

const FPS = 30, SECONDS = 4;
function Scene() {
  const t = Math.min(1, useCurrentFrame() / (FPS * SECONDS - 1));
  const a = Math.PI * t;
  return (
    <AbsoluteFill style={{ background: "#000" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 0, 0]} lookAt={[Math.sin(a), 0, -Math.cos(a)]} fov={60} />
        <AmbientLight intensity={800} />
        <Mesh name="emitter" position={[3, 0, 0]}><Box args={[0.3, 0.3, 0.3]} /><MeshStandardMaterial color="#e0a040" /></Mesh>
        <Sound3D src={staticFile("assets/sounds/tone440.wav")} at={[3, 0, 0]} loop volume={0.5} />
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="turn" component={Scene} width={640} height={360} fps={FPS} durationInFrames={FPS * SECONDS} />);
