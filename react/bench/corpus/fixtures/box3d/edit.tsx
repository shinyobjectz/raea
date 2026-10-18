// BOX3D: a synthetic 3D scene for the corpus harness (react/bench/corpus/run.sh): a box and a torus that spring in and
// turn under a camera and two lights, with a DOM title and lower third over the 3D, as an agent's product shot would
// put them. Square, 4 s, no footage.
import { AbsoluteFill, Composition, interpolate, registerRoot, spring, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, Torus } from "@raea/motion/3d";

const FPS = 30;

function Shot() {
  const f = useCurrentFrame();
  const s = spring({ frame: f, fps: FPS, config: { damping: 12 } });
  const turn = interpolate(f, [0, 119], [0, Math.PI]);
  const o = interpolate(f, [20, 40], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <AbsoluteFill style={{ background: "linear-gradient(180deg, #0e1726, #1d2b44)" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 0.6, 6]} fov={45} />
        <AmbientLight intensity={0.6} />
        <DirectionalLight position={[3, 4, 5]} intensity={1.2} />
        <Mesh position={[-1.1, 0, 0]} rotation={[0.4, turn, 0]} scale={[s, s, s]}>
          <Box args={[1.4, 1.4, 1.4]} />
          <MeshStandardMaterial color="#f59e0b" roughness={0.35} metalness={0.1} />
        </Mesh>
        <Mesh position={[1.3, 0, 0]} rotation={[turn, 0.3, 0]} scale={[s, s, s]}>
          <Torus args={[0.8, 0.3]} />
          <MeshStandardMaterial color="#38bdf8" roughness={0.2} metalness={0.6} />
        </Mesh>
      </Canvas3D>
      <div style={{ position: "absolute", top: 90, width: 1080, textAlign: "center", fontFamily: "Helvetica", fontWeight: 800, fontSize: 96, color: "#ffffff", opacity: o }}>NEW SHAPES</div>
      <div style={{ position: "absolute", left: 80, bottom: 90, padding: "18px 32px", borderRadius: 16, background: "rgba(255,255,255,0.12)", fontFamily: "Helvetica", fontSize: 44, color: "#e2e8f0", opacity: o }}>Spring collection · in stores now</div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="box3d" component={Shot} width={1080} height={1080} fps={FPS} durationInFrames={120} />);
