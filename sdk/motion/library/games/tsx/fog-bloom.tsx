// record: bevy-3d-rendering-fog, bevy-3d-rendering-bloom-3d
// Bevy's "fog" and "bloom 3d" examples in raea's SDK: a corridor of pillars fading into fog that matches the
// background, with glowing orbs blooming as they pulse.
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Bloom, Box, Canvas3D, DirectionalLight, Fog, Mesh, MeshStandardMaterial, PerspectiveCamera, Sphere } from "@raea/motion/3d";

const BG = "#1d2433";

function FogBloom() {
  const f = useCurrentFrame();
  const z = -f * 0.06;
  return (
    <AbsoluteFill style={{ background: BG }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.6, 4 + z]} lookAt={[0, 1.2, -6 + z]} fov={55} />
        <AmbientLight intensity={0.3} />
        <DirectionalLight position={[2, 6, 3]} intensity={0.8} />
        <Fog color={BG} near={4} far={22} />
        <Bloom intensity={1.4} threshold={0.8} />
        <Mesh position={[0, -0.05, -15]}>
          <Box args={[8, 0.1, 50]} />
          <MeshStandardMaterial color="#3b4458" roughness={0.9} />
        </Mesh>
        {Array.from({ length: 12 }, (_, i) => (
          <Mesh key={`p${i}`} position={[i % 2 ? 2.2 : -2.2, 1.5, -i * 3]}>
            <Box args={[0.5, 3, 0.5]} />
            <MeshStandardMaterial color="#c9c2b2" roughness={0.7} />
          </Mesh>
        ))}
        {Array.from({ length: 6 }, (_, i) => (
          <Mesh key={`o${i}`} position={[0, 1.2 + 0.2 * Math.sin(f * 0.1 + i), -2 - i * 5]}>
            <Sphere args={[0.25]} />
            <MeshStandardMaterial color="#000000" emissive="#58e1ff" emissiveIntensity={4 + 3 * Math.sin(f * 0.15 + i)} />
          </Mesh>
        ))}
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="fogbloom" component={FogBloom} width={1280} height={720} fps={30} durationInFrames={120} />);
