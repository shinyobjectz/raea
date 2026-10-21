// Paint fixture: <Look mode="paint"> over a few boxes and a sphere on a ground, a low sun throwing hard shadows, a flat
// two-tone sky, a palette-free three-band painting whose grain and edge wander are keyed (they ramp over the shot) and
// whose wander boils every 4 frames, so a baked play must key them as a render does; then a box changes colour, which
// the paint copy must follow. The camera orbits.
import { AbsoluteFill, Composition, interpolate, registerRoot, useCurrentFrame } from "@raea/motion";
import { Box, Canvas3D, DirectionalLight, Look, Mesh, MeshStandardMaterial, PerspectiveCamera, Shadows, Sphere } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;

function Shot() {
  const f = useCurrentFrame();
  const a = interpolate(f, [0, FRAMES - 1], [0.4, 1.0]);
  return (
    <AbsoluteFill style={{ background: "#3d8fd6" }}>
      <Canvas3D>
        <PerspectiveCamera position={[Math.sin(a) * 9, 5, Math.cos(a) * 9]} lookAt={[0, 0.8, 0]} fov={40} />
        <Look mode="paint" bands={3} shadow="#9aa0b8" mid="#d6b4ae" sky={["#3d8fd6", "#9ccfee"]} boil={4}
          grain={interpolate(f, [0, FRAMES - 1], [0.1, 0.5])} rag={interpolate(f, [0, FRAMES - 1], [1, 3])} />
        <DirectionalLight position={[-4, 3, 3]} intensity={1.2} />
        <Shadows distance={30} firstCascade={25} cascades={1} />
        <Mesh name="ground" position={[0, -0.05, 0]}><Box args={[14, 0.1, 14]} /><MeshStandardMaterial color="#6fb04a" /></Mesh>
        <Mesh name="tower" position={[0, 1.5, 0]}><Box args={[1, 3, 1]} /><MeshStandardMaterial color="#f4f0e8" /></Mesh>
        <Mesh name="crate" position={[1.8, 0.4, 1]}>
          <Box args={[0.8, 0.8, 0.8]} />
          <MeshStandardMaterial color={f < 30 ? "#d8463a" : "#2f6fc4"} />
        </Mesh>
        <Mesh name="ball" position={[-1.6, 0.6, 1.2]}><Sphere args={[0.6]} /><MeshStandardMaterial color="#f2b632" /></Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="paint" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
