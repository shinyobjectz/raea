// Splat fixture: a procedural gaussian splat (media/test.ply, made by make_ply.py: a striped torus on a disc, degree-3
// harmonics) with a box standing in front of it, the camera circling. It reveals from the floor up (frames 0-20),
// pulses, twists, then blows away on the wind (40-59). The splats are sorted for each frame's camera alone, so frame
// 59 rendered by itself equals frame 59 reached in play, and the box in front hides the splats behind it.
import { AbsoluteFill, Composition, interpolate, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, Splat } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;

function Shot() {
  const f = useCurrentFrame();
  const a = interpolate(f, [0, FRAMES - 1], [-0.6, 0.6]);
  return (
    <AbsoluteFill style={{ background: "#0d0f14" }}>
      <Canvas3D>
        <PerspectiveCamera position={[Math.sin(a) * 3.4, 1.4, Math.cos(a) * 3.4]} lookAt={[0, 0.8, 0]} fov={45} />
        <AmbientLight intensity={0.4} />
        <DirectionalLight position={[3, 5, 2]} intensity={2} />
        <Mesh position={[0.55, 0.3, 1.0]}>
          <Box args={[0.5, 0.6, 0.5]} />
          <MeshStandardMaterial color="#3b6fd8" roughness={0.6} />
        </Mesh>
        <Splat
          name="torus"
          src={staticFile("test.ply")}
          reveal={interpolate(f, [0, 20], [0, 1], { extrapolateRight: "clamp" })}
          revealAxis={[0, 1, 0]}
          revealGlow={0.8}
          pulse={f >= 20 && f < 40 ? Math.max(0, Math.cos(((f - 20) / 10) * Math.PI)) : 0}
          pulseGlow={0.3}
          twist={interpolate(f, [25, 40], [0, 0.6], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })}
          disperse={interpolate(f, [40, 59], [0, 0.8], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })}
          seed={3}
        />
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="splat" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
