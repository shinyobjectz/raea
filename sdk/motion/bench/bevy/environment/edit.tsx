// Environment fixture: Poly Haven CC0 panoramas (media/, 1k .hdr) as image-based light and sky. A mirror sphere, a
// matte one and a gold torus on a floor; the panorama turns and brightens over the shot, and at frame 30 it cuts from
// an exterior (venice_sunset) to an interior (museum_of_ethnography), so the light and the sky change on that frame. A
// small AmbientLight composes with it.
import { AbsoluteFill, Composition, interpolate, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, Environment, Mesh, MeshStandardMaterial, PerspectiveCamera, Sphere, Torus } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;

function Shot() {
  const f = useCurrentFrame();
  const outside = f < 30;
  const turn = interpolate(f, [0, FRAMES - 1], [0, 1.2]);
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.2, 5]} lookAt={[0, 0.6, 0]} fov={50} />
        <AmbientLight intensity={0.05} />
        <Environment
          src={staticFile(outside ? "venice_sunset_1k.hdr" : "museum_of_ethnography_1k.hdr")}
          intensity={interpolate(f, [0, FRAMES - 1], [0.8, 1.2])}
          rotation={turn}
          background
        />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[8, 0.1, 8]} />
          <MeshStandardMaterial color="#8a8580" roughness={0.6} />
        </Mesh>
        <Mesh position={[-1.4, 0.6, 0]}>
          <Sphere args={[0.6]} />
          <MeshStandardMaterial color="#ffffff" metalness={1} roughness={0.04} />
        </Mesh>
        <Mesh position={[0, 0.6, -0.4]}>
          <Sphere args={[0.6]} />
          <MeshStandardMaterial color="#e8e4dc" roughness={0.9} />
        </Mesh>
        <Mesh position={[1.4, 0.6, 0]} rotation={[0.4, f * 0.03, 0]}>
          <Torus args={[0.45, 0.16]} />
          <MeshStandardMaterial color="#e0b050" metalness={1} roughness={0.25} />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="environment" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
