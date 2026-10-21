// Volumetric fixture for what only the froxel grid draws: <VolumetricFog mode="froxel"> with a ground fog (a wide
// <FogVolume> that thins going up, falloff), a <Fire> whose light the fog takes up around it, smoke (an absorbing
// volume) rolling in over the fire from frame 20 and overlapping the ground fog, and a volumetric spot through a
// window's blind laying its bars in the haze. check.sh holds render equal to play.
import { AbsoluteFill, Composition, interpolate, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, Fire, FogVolume, Mesh, MeshStandardMaterial, PerspectiveCamera, SpotLight, VolumetricFog } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;
const SLATS = Array.from({ length: 9 }, (_, i) => 1.28 + i * 0.16);

function Shot() {
  const f = useCurrentFrame();
  const smoke = interpolate(f, [20, 50], [0, 0.6], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Canvas3D>
        <PerspectiveCamera position={[4.2, 1.7, 5.2]} lookAt={[-0.3, 1.0, -1]} fov={52} />
        <VolumetricFog mode="froxel" />
        <AmbientLight intensity={0.05} />
        <SpotLight position={[-6, 4.2, -1]} target={[0.5, 0, 0.5]} angle={0.32} penumbra={0.1} intensity={4} color="#dfe8ff" castShadow volumetric />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[12, 0.1, 12]} />
          <MeshStandardMaterial color="#5c564e" roughness={0.9} />
        </Mesh>
        {/* a wall with a window (1.2 to 2.6 m up, 1.8 m wide), its blind's slats across it */}
        {([[0.6, 1.2, 0, 6], [3.3, 1.4, 0, 6], [1.9, 1.4, -1.95, 2.1], [1.9, 1.4, 1.95, 2.1]] as const).map(([y, h, z, w], i) => (
          <Mesh key={`w${i}`} name={`wall${i}`} position={[-3, y, z]}>
            <Box args={[0.2, h, w]} />
            <MeshStandardMaterial color="#4a4640" roughness={0.9} />
          </Mesh>
        ))}
        {SLATS.map((y, i) => (
          <Mesh key={i} name={`slat${i}`} position={[-3, y, 0]} rotation={[0, 0, 0.6]}>
            <Box args={[0.02, 0.1, 1.8]} />
            <MeshStandardMaterial color="#2a2824" />
          </Mesh>
        ))}
        <Fire name="fire" position={[1.2, 0, -0.6]} scale={1.1} intensity={10} seed={2} />
        <FogVolume name="ground" position={[0, 2, 0]} scale={[12, 4, 12]} density={0.12} falloff={0.8} color="#d8dde8" scattering={0.5} absorption={0.1} anisotropy={0.5} />
        <FogVolume name="smoke" position={[1.2, 2.2, -0.6]} scale={[1.6, 2.4, 1.6]} density={smoke} color="#3a3632" scattering={0.2} absorption={0.8} anisotropy={0.2} />
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="volumetric-grid" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
