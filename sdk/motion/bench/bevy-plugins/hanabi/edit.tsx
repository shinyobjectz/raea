// bevy_hanabi through the generated SDK (sdk/motion/docs/bevy-plugins.md): two fountains of GPU particles, one opaque and
// one alpha-blended, on small pedestals. `Particles` is raea's hand-written reflected component (engine/bevy-plugins/src/
// hanabi.rs builds the effect asset from it); its TSX wrapper is generated like any other (bevy_hanabi.tsx, beside
// it). Render and play are compared by sdk/motion/bench/bevy/check.sh with RAEA_MOTION_BIN=bevy-plugins/motion.sh.
import { AbsoluteFill, Composition, registerRoot } from "@raea/motion";
import { AmbientLight, Canvas3D, Cylinder, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera } from "@raea/motion/3d";
import { Particles } from "./bevy_hanabi";

const FPS = 30;
const FRAMES = 120;

function Fountains() {
  return (
    <AbsoluteFill style={{ background: "#0b1020" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.5, 7]} lookAt={[0, 1.2, 0]} fov={45} />
        <AmbientLight intensity={0.5} />
        <DirectionalLight position={[2, 5, 4]} intensity={1.2} />
        <Mesh name="left" position={[-1.5, 0, 0]}>
          <Cylinder args={[0.25, 0.35, 0.3]} />
          <MeshStandardMaterial color="#475569" />
          <Particles value={{ rate: 300, lifetime: 1.6, speed: 4, spread: 0.3, size: 0.06, color: [1, 0.55, 0.1, 1], gravity: 5, seed: 7, opaque: true }} />
        </Mesh>
        <Mesh name="right" position={[1.5, 0, 0]}>
          <Cylinder args={[0.25, 0.35, 0.3]} />
          <MeshStandardMaterial color="#475569" />
          <Particles value={{ rate: 300, lifetime: 1.6, speed: 4, spread: 0.3, size: 0.08, color: [0.3, 0.7, 1, 0.5], gravity: 5, seed: 7, opaque: false }} />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="fountains" component={Fountains} width={1280} height={720} fps={FPS} durationInFrames={FRAMES} />);
