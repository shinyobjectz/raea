// Pictures and shapes in space: a poster on an ImagePlane swinging in, a picture-mapped box (map), a Cone, and a
// Text3D headline, under a point light.
import { AbsoluteFill, Composition, interpolate, registerRoot, spring, staticFile, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, Cone, DirectionalLight, ImagePlane, Mesh, MeshStandardMaterial, PerspectiveCamera, PointLight, Text3D } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;

function Shot() {
  const f = useCurrentFrame();
  const s = spring({ frame: f, fps: FPS, config: { damping: 14 } });
  const swing = interpolate(s, [0, 1], [-1.2, -0.25]);
  return (
    <AbsoluteFill style={{ background: "#f4efe6" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 0.8, 6]} lookAt={[0, 0.3, 0]} fov={42} />
        <AmbientLight intensity={0.6} />
        <DirectionalLight position={[2, 4, 5]} intensity={1} />
        <PointLight position={[2, 1.5, 2]} intensity={0.8} color="#ffe0b0" />
        <ImagePlane src={staticFile("poster.png")} width={2.2} position={[-1.4, 0.6, 0]} rotation={[0, swing * -1, 0]} />
        <Mesh position={[1.4, 0.6, 0]} rotation={[0.3, f * 0.04, 0]}>
          <Box args={[1, 1, 1]} />
          <MeshStandardMaterial map={staticFile("poster.png")} roughness={0.6} />
        </Mesh>
        <Mesh position={[2.6, -0.4, -1]}>
          <Cone args={[0.4, 1]} />
          <MeshStandardMaterial color="#2a9d8f" roughness={0.5} />
        </Mesh>
        <Mesh position={[-1.5, -1.0, 0.5]} rotation={[0, 0.1, 0]}>
          <Text3D size={0.45} depth={0.15}>RAEA</Text3D>
          <MeshStandardMaterial color="#e76f51" roughness={0.4} />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="planes" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
