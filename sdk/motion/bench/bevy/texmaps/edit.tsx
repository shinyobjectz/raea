// Texture maps fixture: Poly Haven CC0 sets (media/, 1k) on a tiled marble floor (a Plane, separate rough and ao
// maps), a brick wall (a Box, its packed _arm map as both metallicRoughnessMap and aoMap) and a marble plinth, under a
// raking light that sweeps across them so the normal maps catch it. The camera dollies; the plinth's roughness and the
// light move per frame.
import { AbsoluteFill, Composition, interpolate, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshPhysicalMaterial, MeshStandardMaterial, PerspectiveCamera, Plane, SpotLight } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;

const marble = {
  map: staticFile("marble_01_diff_1k.jpg"),
  normalMap: staticFile("marble_01_nor_gl_1k.jpg"),
  roughnessMap: staticFile("marble_01_rough_1k.jpg"),
  aoMap: staticFile("marble_01_ao_1k.jpg"),
};
const brick = {
  map: staticFile("castle_brick_07_diff_1k.jpg"),
  normalMap: staticFile("castle_brick_07_nor_gl_1k.jpg"),
  metallicRoughnessMap: staticFile("castle_brick_07_arm_1k.jpg"),
  aoMap: staticFile("castle_brick_07_arm_1k.jpg"),
};

function Shot() {
  const f = useCurrentFrame();
  const sweep = interpolate(f, [0, FRAMES - 1], [-4, 4]);
  const cx = interpolate(f, [0, FRAMES - 1], [-1.2, 1.2]);
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Canvas3D>
        <PerspectiveCamera position={[cx, 1.7, 5.5]} lookAt={[0, 0.9, -1]} fov={50} />
        <AmbientLight intensity={0.15} />
        <DirectionalLight position={[-3, 5, 4]} intensity={0.5} color="#dfe6ff" />
        <SpotLight position={[sweep, 2.6, 0.6]} target={[sweep * 0.3, 1.2, -3]} angle={0.7} penumbra={0.5} intensity={3} color="#ffe2b8" castShadow />
        <Mesh name="floor" rotation={[-Math.PI / 2, 0, 0]}>
          <Plane args={[12, 12]} />
          <MeshStandardMaterial {...marble} repeat={[6, 6]} />
        </Mesh>
        <Mesh name="wall" position={[0, 2, -3]}>
          <Box args={[12, 4, 0.4]} />
          <MeshStandardMaterial {...brick} repeat={[6, 2]} />
        </Mesh>
        <Mesh name="plinth" position={[0, 0.6, -0.5]} rotation={[0, f * 0.01, 0]}>
          <Box args={[0.9, 1.2, 0.9]} />
          <MeshPhysicalMaterial {...marble} color="#f0ece4" roughness={interpolate(f, [0, FRAMES - 1], [1, 0.4])} clearcoat={0.3} />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="texmaps" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
