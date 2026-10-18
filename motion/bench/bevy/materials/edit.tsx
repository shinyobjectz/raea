// Materials fixture: a glowing emissive ring with bloom, a see-through glass sphere fading in, a clearcoat car-paint
// sphere, an unlit sign, a sphere whose colour animates, Text3D letters; distance fog and ACES tone mapping.
import { AbsoluteFill, Composition, interpolate, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Bloom, Box, Canvas3D, DirectionalLight, Fog, Mesh, MeshBasicMaterial, MeshPhysicalMaterial, MeshStandardMaterial, PerspectiveCamera, Sphere, Text3D, ToneMapping, Torus } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;

function mix(a: number[], b: number[], t: number) {
  return "#" + a.map((x, i) => Math.round(x + (b[i] - x) * t).toString(16).padStart(2, "0")).join("");
}

function Shot() {
  const f = useCurrentFrame();
  const t = f / (FRAMES - 1);
  const glow = interpolate(f, [0, FRAMES - 1], [1, 6]);
  const glass = interpolate(f, [0, FRAMES - 1], [0.15, 0.6]);
  return (
    <AbsoluteFill style={{ background: "#10131a" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.5, 7]} lookAt={[0, 0.3, 0]} fov={45} />
        <AmbientLight intensity={0.3} />
        <DirectionalLight position={[3, 5, 4]} intensity={1.4} />
        <Bloom intensity={1.5} threshold={0.8} />
        <Fog color="#10131a" near={6} far={16} />
        <ToneMapping mode="aces" />
        <Mesh position={[-2.6, 0.4, 0]} rotation={[Math.PI / 2, 0, f * 0.03]}>
          <Torus args={[0.6, 0.12]} />
          <MeshStandardMaterial color="#1a1a1a" emissive="#ff3c78" emissiveIntensity={glow} />
        </Mesh>
        <Mesh position={[-0.9, 0.4, 0]}>
          <Sphere args={[0.6]} />
          <MeshStandardMaterial color="#bfe6ff" roughness={0.05} transparent opacity={glass} />
        </Mesh>
        <Mesh position={[0.8, 0.4, 0]}>
          <Sphere args={[0.6]} />
          <MeshPhysicalMaterial color="#8a0f1e" metalness={0.6} roughness={0.5} clearcoat={1} clearcoatRoughness={0.05} />
        </Mesh>
        <Mesh position={[2.5, 0.4, 0]}>
          <Sphere args={[0.6]} />
          <MeshStandardMaterial color={mix([40, 200, 120], [250, 200, 40], t)} roughness={0.4} />
        </Mesh>
        <Mesh position={[0, 1.9, -1]}>
          <Box args={[3.2, 0.7, 0.05]} />
          <MeshBasicMaterial color="#f5f0e1" />
        </Mesh>
        <Mesh position={[-1.2, -0.9, 0.5]}>
          <Text3D size={0.6} depth={0.2}>SALE</Text3D>
          <MeshStandardMaterial color="#ffd166" metalness={0.8} roughness={0.25} />
        </Mesh>
        <Mesh position={[0, -1.3, -6]}>
          <Box args={[30, 0.1, 30]} />
          <MeshStandardMaterial color="#2a2f3a" roughness={0.9} />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="materials" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
