// Shadows fixture: a colonnade along a 120 m street under a low sun that turns over the shot, a spot on the near
// column, and a <Shadows> that fits the sun's cascades to the street (sharp up close, still there at the far end),
// a 4096 map and a little more bias than Bevy's. The camera dollies along the columns, so their shadows cross from one
// cascade to the next.
import { AbsoluteFill, Composition, interpolate, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, Cylinder, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, Plane, Shadows, SpotLight } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;

function Shot() {
  const f = useCurrentFrame();
  const sun = interpolate(f, [0, FRAMES - 1], [-0.6, 0.6]);
  const z = interpolate(f, [0, FRAMES - 1], [6, 2]);
  const columns = Array.from({ length: 24 }, (_, i) => -i * 5);
  return (
    <AbsoluteFill style={{ background: "#9fb4c8" }}>
      <Canvas3D>
        <PerspectiveCamera position={[-3.5, 1.7, z]} lookAt={[1, 1.4, -20]} fov={45} />
        <Shadows mapSize={4096} distance={90} firstCascade={6} overlap={0.3} bias={0.03} normalBias={2} />
        <AmbientLight intensity={0.25} color="#c8d8ff" />
        <DirectionalLight position={[40 * Math.sin(sun), 22, 12 * Math.cos(sun)]} target={[0, 0, -20]} intensity={1.1} color="#ffe6c4" castShadow />
        <SpotLight position={[-2, 4, 2]} target={[1.5, 0.5, 0]} angle={0.5} penumbra={0.4} intensity={4} color="#ffb070" castShadow />
        <Mesh name="street" rotation={[-Math.PI / 2, 0, 0]} position={[0, 0, -55]}>
          <Plane args={[30, 130]} />
          <MeshStandardMaterial color="#b9a888" roughness={0.9} />
        </Mesh>
        {columns.map((cz) => (
          <Mesh key={cz} position={[1.5, 2, cz]}>
            <Cylinder args={[0.35, 0.4, 4, 24]} />
            <MeshStandardMaterial color="#e6dccb" roughness={0.7} />
          </Mesh>
        ))}
        <Mesh name="entablature" position={[1.5, 4.2, -57.5]}>
          <Box args={[1.2, 0.4, 117]} />
          <MeshStandardMaterial color="#d8ccb8" roughness={0.75} />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="shadows" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
