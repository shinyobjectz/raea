// record: bevy-3d-rendering-lighting, bevy-3d-rendering-spotlight
// Bevy's "lighting" and "spotlight" examples in raea's SDK: a dim sun, a coloured bulb orbiting, and a spot sweeping
// across the floor, all moved from the frame.
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, PointLight, Sphere, SpotLight } from "@raea/motion/3d";

function Lighting() {
  const f = useCurrentFrame();
  const a = f * 0.05;
  const bulb: [number, number, number] = [Math.cos(a) * 2.5, 1.2, Math.sin(a) * 2.5];
  const sweep = Math.sin(f * 0.04) * 3;
  return (
    <AbsoluteFill style={{ background: "#07080b" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 4, 9]} lookAt={[0, 0.5, 0]} fov={40} />
        <AmbientLight intensity={0.08} />
        <DirectionalLight position={[-4, 6, 2]} intensity={0.25} castShadow />
        <PointLight position={bulb} color="#ff8a3d" intensity={2.5} distance={10} castShadow />
        <SpotLight position={[0, 6, 2]} target={[sweep, 0, 0]} angle={0.35} penumbra={0.4} intensity={6} color="#9ad1ff" />
        <Mesh name="bulb" position={bulb}>
          <Sphere args={[0.08]} />
          <MeshStandardMaterial color="#000000" emissive="#ff8a3d" emissiveIntensity={4} />
        </Mesh>
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[12, 0.1, 8]} />
          <MeshStandardMaterial color="#b9b4aa" roughness={0.85} />
        </Mesh>
        {[-2, 0, 2].map((x) => (
          <Mesh key={x} position={[x, 0.5, 0]}>
            <Box args={[1, 1, 1]} />
            <MeshStandardMaterial color="#e9e4da" roughness={0.5} />
          </Mesh>
        ))}
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="lighting" component={Lighting} width={1280} height={720} fps={30} durationInFrames={120} />);
