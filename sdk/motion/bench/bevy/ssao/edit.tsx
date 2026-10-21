// Ambient occlusion fixture: <AmbientOcclusion> (and with it SMAA in place of multisampling) on a corner of two walls,
// stacked boxes, a sphere rolling toward the wall and a torus lying on the floor, lit mostly by indirect light (an
// AmbientLight and an interior panorama) so the occlusion in the creases and under things shows. A weak sun casts
// the only shadows.
import { AbsoluteFill, Composition, interpolate, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import { AmbientLight, AmbientOcclusion, Box, Canvas3D, DirectionalLight, Environment, Mesh, MeshStandardMaterial, PerspectiveCamera, Sphere, Torus } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;

function Shot() {
  const f = useCurrentFrame();
  const x = interpolate(f, [0, FRAMES - 1], [1.6, 0.2]);
  const stone = "#cfc8bc";
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Canvas3D>
        <PerspectiveCamera position={[3.2, 2.2, 4.2]} lookAt={[0, 0.6, 0]} fov={45} />
        <AmbientOcclusion quality="high" />
        <AmbientLight intensity={0.6} />
        <Environment src={staticFile("museum_of_ethnography_1k.hdr")} intensity={0.6} />
        <DirectionalLight position={[2, 6, 3]} intensity={0.25} />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[8, 0.1, 8]} />
          <MeshStandardMaterial color={stone} roughness={0.9} />
        </Mesh>
        <Mesh position={[0, 1.5, -1.05]}>
          <Box args={[8, 3, 0.1]} />
          <MeshStandardMaterial color={stone} roughness={0.9} />
        </Mesh>
        <Mesh position={[-1.55, 1.5, 0]}>
          <Box args={[0.1, 3, 8]} />
          <MeshStandardMaterial color={stone} roughness={0.9} />
        </Mesh>
        <Mesh position={[-0.9, 0.35, -0.55]}>
          <Box args={[0.7, 0.7, 0.7]} />
          <MeshStandardMaterial color={stone} roughness={0.8} />
        </Mesh>
        <Mesh position={[-0.9, 0.95, -0.55]} rotation={[0, 0.5, 0]}>
          <Box args={[0.5, 0.5, 0.5]} />
          <MeshStandardMaterial color={stone} roughness={0.8} />
        </Mesh>
        <Mesh position={[x, 0.4, 0.2]} rotation={[0, 0, -x / 0.4]}>
          <Sphere args={[0.4]} />
          <MeshStandardMaterial color="#b8b0a4" roughness={0.7} />
        </Mesh>
        <Mesh position={[0.6, 0.12, 1.3]} rotation={[Math.PI / 2, 0, 0]}>
          <Torus args={[0.35, 0.12]} />
          <MeshStandardMaterial color="#b8b0a4" roughness={0.7} />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="ssao" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
