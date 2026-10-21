// Volumetric fixture for see-through surfaces in the froxel grid (<VolumetricFog mode="froxel">): a pane across a
// spot light's shaft and the haze, which fogs itself at its own depth (froxel_glass.rs), and a fire behind it (held at one time, so frames compare). Frames
// 0-19 an opaque pane; 20-39 the pane blended at opacity 1, which must draw as the opaque one; 40-49 blended at
// opacity 0, which must draw as no pane (50-59); 60-89 at 0.4 with a tint.
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, Fire, FogVolume, Mesh, MeshStandardMaterial, PerspectiveCamera, SpotLight, VolumetricFog } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 90;

function Shot() {
  const f = useCurrentFrame();
  const opacity = f < 40 ? 1 : f < 60 ? 0 : 0.4;
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Canvas3D>
        <PerspectiveCamera position={[3.6, 1.6, 5.4]} lookAt={[-0.4, 1.2, -1]} fov={50} />
        <VolumetricFog mode="froxel" ambient={0.05} ambientColor="#8090a8" />
        <AmbientLight intensity={0.08} />
        <SpotLight position={[-3.5, 4.5, 1.5]} target={[0.8, 0, -0.6]} angle={0.3} penumbra={0.15} intensity={3} color="#ffe6c4" castShadow volumetric />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[10, 0.1, 10]} />
          <MeshStandardMaterial color="#6a6258" roughness={0.9} />
        </Mesh>
        <Mesh position={[0, 2.5, -2.6]}>
          <Box args={[8, 5, 0.2]} />
          <MeshStandardMaterial color="#5a5248" roughness={0.9} />
        </Mesh>
        <Fire name="fire" position={[-1.2, 0, -1.4]} scale={0.8} intensity={8} seed={3} time={1.5} />
        {/* the pane, across the shaft: opaque, or see-through */}
        {f < 20 && (
          <Mesh name="opaque" position={[0.2, 1.3, 0.6]} rotation={[0, 0.5, 0]}>
            <Box args={[2.4, 2.2, 0.03]} />
            <MeshStandardMaterial color="#9fc4d8" roughness={0.2} />
          </Mesh>
        )}
        {f >= 20 && (f < 50 || f >= 60) && (
          <Mesh name="pane" position={[0.2, 1.3, 0.6]} rotation={[0, 0.5, 0]}>
            <Box args={[2.4, 2.2, 0.03]} />
            <MeshStandardMaterial color="#9fc4d8" roughness={0.2} opacity={opacity} />
          </Mesh>
        )}
        <FogVolume name="air" position={[0, 2, 0.5]} scale={[8, 4, 6]} density={0.15} color="#e8e0d4" scattering={0.45} absorption={0.15} anisotropy={0.5} />
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="volumetric-glass" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
