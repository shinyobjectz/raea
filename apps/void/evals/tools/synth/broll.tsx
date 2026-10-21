// Synthetic b-roll for the new-stack eval's ad scenario (apps/void/evals/tools/make_synth_fixture.sh): a fictional cold brew
// can ("Plumquiver", invented) in three 5 s shots, 1080x1920. No real brand, no person, no creator's footage: it's drawn here.
// Rendered once per shot with --composition.
import { AbsoluteFill, Composition, interpolate, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, Cylinder, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, PointLight, Sphere } from "@raea/motion/3d";

const ORANGE = "#ff7a1a";
const CREAM = "#f6ead8";
const BROWN = "#3b2416";

function Can({ x = 0, z = 0, spin = 0, name }: { x?: number; z?: number; spin?: number; name: string }) {
  return (
    <>
      <Mesh name={`${name}-body`} position={[x, 0.62, z]} rotation={[0, spin, 0]}>
        <Cylinder args={[0.33, 0.33, 1.24]} />
        <MeshStandardMaterial color={ORANGE} roughness={0.35} metalness={0.35} />
      </Mesh>
      <Mesh name={`${name}-band`} position={[x, 0.7, z]} rotation={[0, spin, 0]}>
        <Cylinder args={[0.335, 0.335, 0.36]} />
        <MeshStandardMaterial color={CREAM} roughness={0.5} />
      </Mesh>
      <Mesh name={`${name}-lid`} position={[x, 1.26, z]}>
        <Cylinder args={[0.3, 0.33, 0.05]} />
        <MeshStandardMaterial color="#c9ccd1" roughness={0.25} metalness={0.6} />
      </Mesh>
    </>
  );
}

/** Shot 1: the can turns on a warm table, the camera eases in. */
function Turn() {
  const f = useCurrentFrame();
  const z = interpolate(f, [0, 149], [4.4, 3.4]);
  return (
    <AbsoluteFill style={{ background: "#f3c89a" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.2, z]} lookAt={[0, 0.65, 0]} fov={38} />
        <AmbientLight intensity={0.5} />
        <DirectionalLight position={[2.5, 4, 3]} intensity={1.4} castShadow />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[8, 0.1, 8]} />
          <MeshStandardMaterial color="#c98b5a" roughness={0.8} />
        </Mesh>
        <Can name="hero" spin={f * 0.03} />
      </Canvas3D>
    </AbsoluteFill>
  );
}

/** Shot 2: ice cubes fall around the can, low and close. */
function Ice() {
  const f = useCurrentFrame();
  const cubes = Array.from({ length: 9 }, (_, i) => {
    const r = ((i * 9301 + 49297) % 233280) / 233280;
    const start = i * 9;
    const y = Math.max(0.12, 3 - Math.max(0, f - start) * 0.06);
    return { i, x: (r - 0.5) * 1.6 + (r > 0.5 ? 0.4 : -0.4), z: ((i * 7) % 5) * 0.18 - 0.3, y, rot: f * 0.02 + i };
  });
  return (
    <AbsoluteFill style={{ background: "#16202b" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 0.9, 3.3]} lookAt={[0, 0.6, 0]} fov={42} />
        <AmbientLight intensity={0.35} />
        <DirectionalLight position={[-2, 4, 2]} intensity={1.1} />
        <PointLight position={[1.2, 1.4, 1.2]} intensity={2.5} color="#ffd2a8" />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[8, 0.1, 8]} />
          <MeshStandardMaterial color="#22313f" roughness={0.6} />
        </Mesh>
        <Can name="hero" spin={0.4} />
        {cubes.map((c) => (
          <Mesh key={c.i} name={`ice-${c.i}`} position={[c.x, c.y, c.z]} rotation={[c.rot, c.rot * 0.7, 0]}>
            <Box args={[0.2, 0.2, 0.2]} />
            <MeshStandardMaterial color="#dff3ff" roughness={0.1} transparent opacity={0.75} />
          </Mesh>
        ))}
      </Canvas3D>
    </AbsoluteFill>
  );
}

/** Shot 3: three cans in a row, a slow dolly along them, a warm sun disc behind. */
function Row() {
  const f = useCurrentFrame();
  const x = interpolate(f, [0, 149], [-1.2, 1.2]);
  return (
    <AbsoluteFill style={{ background: "#ffb36b" }}>
      <Canvas3D>
        <PerspectiveCamera position={[x, 1.0, 4.2]} lookAt={[x * 0.5, 0.7, 0]} fov={40} />
        <AmbientLight intensity={0.55} />
        <DirectionalLight position={[3, 5, 4]} intensity={1.3} castShadow />
        <Mesh position={[0, 1.6, -3]}>
          <Sphere args={[1.1]} />
          <MeshStandardMaterial color="#fff1c2" emissive="#ffd27a" emissiveIntensity={1.2} />
        </Mesh>
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[10, 0.1, 8]} />
          <MeshStandardMaterial color={BROWN} roughness={0.7} />
        </Mesh>
        <Can name="a" x={-0.9} spin={0.2} />
        <Can name="b" x={0} spin={0.6} />
        <Can name="c" x={0.9} spin={1.0} />
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => (
  <>
    <Composition id="turn" component={Turn} width={1080} height={1920} fps={30} durationInFrames={150} />
    <Composition id="ice" component={Ice} width={1080} height={1920} fps={30} durationInFrames={150} />
    <Composition id="row" component={Row} width={1080} height={1920} fps={30} durationInFrames={150} />
  </>
));
