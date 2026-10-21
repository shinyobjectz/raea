// A 3D scene whose holder takes its background from a class (a brand utility such as `bg-brand-bg`), not an inline
// style: the scene must still show through (motion.rs see_through). Before 2026-10-18 the class's opaque background
// painted over the 3D and every frame was that colour alone.
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera } from "@raea/motion/3d";

const css = ".bg-stage { background-color: #1b2230; } .title { color: #f4efe6; font: 700 64px Inter, sans-serif; }";

function Shot() {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill className="bg-stage">
      <style>{css}</style>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.2, 5]} lookAt={[0, 0.4, 0]} fov={42} />
        <AmbientLight intensity={0.5} />
        <DirectionalLight position={[2, 4, 5]} intensity={1.2} />
        <Mesh name="cube" position={[0, 0.5, 0]} rotation={[0.4, f * 0.05, 0]}>
          <Box args={[1.2, 1.2, 1.2]} />
          <MeshStandardMaterial color="#ff7a1a" roughness={0.5} />
        </Mesh>
      </Canvas3D>
      <div className="title" style={{ position: "absolute", left: 60, top: 40 }}>CLASSED</div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="classbg" component={Shot} width={1280} height={720} fps={30} durationInFrames={60} />);
