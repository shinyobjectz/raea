// void's cat turning on the site's light floor, flat (its shape and eyes, no light on it), while it changes look every two seconds: solid, then pixel, then the site's
// dithered grain. The look is the model's alone; the floor and the cat's shadow draw the same throughout.
import { AbsoluteFill, Composition, interpolate, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, Model, PerspectiveCamera, PointLight, ToneMapping } from "@raea/motion/3d";

const FPS = 30;
const LOOKS = ["solid", "pixel", "site"] as const;

function Cat() {
  const f = useCurrentFrame();
  const look = LOOKS[Math.min(2, Math.floor(f / (2 * FPS)))];
  const turn = interpolate(f, [0, 6 * FPS], [0.9, -0.9]);
  return (
    <AbsoluteFill style={{ background: "#fafafa" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.3, 3.6]} lookAt={[0, 0.5, 0]} fov={30} />
        <ToneMapping mode="none" />
        <AmbientLight intensity={0.5} />
        <DirectionalLight position={[2.5, 5, 4]} intensity={1.4} castShadow />
        <PointLight position={[-3, 2, -2]} intensity={0.8} color="#c9c9d6" />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[12, 0.1, 6]} />
          <MeshStandardMaterial color="#ececec" roughness={0.95} />
        </Mesh>
        <Model src={staticFile("cat.glb")} animation="Idle" look={look} flat theme="light" edgeColor="#b0b0b4" rotation={[0, turn, 0]} />
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="cat" component={Cat} width={1080} height={1080} fps={FPS} durationInFrames={180} />);
