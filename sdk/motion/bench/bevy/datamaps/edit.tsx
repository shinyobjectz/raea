// Textures given inline as data: URLs (sdk/motion/bench/bevy/check.sh): an SVG label (data:image/svg+xml, utf8) on a box
// and on an ImagePlane, and a PNG checker (data:image/png;base64) on a second box. rdb decodes them in memory; before
// 2026-10-18 a data: URL was read as a file path and the render refused.
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, ImagePlane, Mesh, MeshStandardMaterial, PerspectiveCamera } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;
const LABEL = 'data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" width="512" height="256"><rect width="512" height="256" fill="%23121724"/><circle cx="96" cy="128" r="64" fill="%23d9b877"/><rect x="190" y="96" width="280" height="64" rx="16" fill="%23efe9db"/></svg>';
const CHECKER = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAYAAADED76LAAAAIUlEQVR4nGOQUAz5D8L/q6TAGJ3PQFABLgkYn7CCQeAGAMt1g8GFTe7BAAAAAElFTkSuQmCC";

function Shot() {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "#f4efe6" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 0.6, 6]} lookAt={[0, 0.2, 0]} fov={42} />
        <AmbientLight intensity={0.6} />
        <DirectionalLight position={[2, 4, 5]} intensity={1} />
        <ImagePlane src={LABEL} width={2.2} position={[-1.5, 0.9, 0]} rotation={[0, 0.25, 0]} />
        <Mesh name="label-box" position={[1.4, 0.7, 0]} rotation={[0.3, f * 0.04, 0]}>
          <Box args={[1.2, 0.6, 0.6]} />
          <MeshStandardMaterial map={LABEL} roughness={0.6} />
        </Mesh>
        <Mesh name="checker-box" position={[0, -0.9, 0]} rotation={[0.4, 0.6 + f * 0.03, 0]}>
          <Box args={[1, 1, 1]} />
          <MeshStandardMaterial map={CHECKER} roughness={0.5} />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="datamaps" component={Shot} width={1280} height={720} fps={FPS} durationInFrames={FRAMES} />);
