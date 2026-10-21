// record: art-of-screenshake, juicing-cameras-trauma
// Screen shake as trauma (Squirrel Eiserloh, "Juicing your cameras with math"): each impact adds trauma (0..1), the
// shake is trauma squared, and the camera's offset and roll come from smooth noise rather than random jumps, so it
// reads as a shake and not a glitch. Trauma decays linearly. Everything is a function of the frame: impacts are listed,
// trauma at frame f is summed from them, and the noise is a fixed sum of sines.
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, Sphere } from "@raea/motion/3d";

const FPS = 30;
const IMPACTS = [{ f: 20, k: 0.5 }, { f: 70, k: 0.9 }, { f: 75, k: 0.4 }, { f: 120, k: 0.3 }];
const DECAY = 1.2; // trauma per second
const MAX_OFFSET = 0.35, MAX_ROLL = 0.06;

function trauma(f: number): number {
  let t = 0, last = 0;
  for (const i of IMPACTS) {
    if (i.f > f) break;
    t = Math.max(0, t - ((i.f - last) / FPS) * DECAY);
    t = Math.min(1, t + i.k);
    last = i.f;
  }
  return Math.max(0, t - ((f - last) / FPS) * DECAY);
}

// smooth noise in [-1, 1]: incommensurate sines, a different seed per channel
const noise = (t: number, seed: number) => (Math.sin(t * 13.1 + seed) + Math.sin(t * 21.7 + seed * 2.3) * 0.6 + Math.sin(t * 34.3 + seed * 4.1) * 0.3) / 1.9;

function Shake() {
  const f = useCurrentFrame();
  const s = trauma(f) ** 2;
  const t = f / FPS;
  const ox = MAX_OFFSET * s * noise(t, 1), oy = MAX_OFFSET * s * noise(t, 7), roll = MAX_ROLL * s * noise(t, 13);
  const hit = IMPACTS.find((i) => f >= i.f && f < i.f + 4);
  return (
    <AbsoluteFill style={{ background: "#1a1423" }}>
      <Canvas3D>
        <PerspectiveCamera position={[ox, 2 + oy, 7]} lookAt={[ox * 0.5 + roll * 5, 0.8 + oy * 0.5, 0]} fov={45} />
        <AmbientLight intensity={0.4} />
        <DirectionalLight position={[3, 7, 4]} intensity={1.3} castShadow />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[12, 0.1, 8]} />
          <MeshStandardMaterial color="#c9ada7" roughness={0.9} />
        </Mesh>
        {[-2, 0, 2].map((x, i) => (
          <Mesh key={i} position={[x, 0.6, 0]}>
            <Box args={[1, 1.2, 1]} />
            <MeshStandardMaterial color={hit ? "#ffffff" : "#4a4e69"} roughness={0.6} />
          </Mesh>
        ))}
        <Mesh name="meteor" position={[0, hit ? 0.3 : 6, 1.5]}>
          <Sphere args={[0.3]} />
          <MeshStandardMaterial color="#f2e9e4" emissive="#ff7b00" emissiveIntensity={2} />
        </Mesh>
      </Canvas3D>
      <div style={{ position: "absolute", left: 40, top: 28, fontFamily: "Inter", fontWeight: 700, fontSize: 30, color: "#f2e9e4" }}>{`trauma ${trauma(f).toFixed(2)}   shake ${s.toFixed(2)}`}</div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="screenshake" component={Shake} width={1280} height={720} fps={FPS} durationInFrames={150} />);
