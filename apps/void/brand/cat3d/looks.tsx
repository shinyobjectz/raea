// void's cat in its three looks, side by side on the site's light ground, flat: solid, pixel (outlined cells), site (the
// dithered grain of raea.io). The floor is drawn as it is under all three; only the cats change.
import { AbsoluteFill, Composition, registerRoot, staticFile } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, Model, PerspectiveCamera, PointLight, ToneMapping } from "@raea/motion/3d";

const LOOKS = ["solid", "pixel", "site"] as const;

function Looks() {
  return (
    <AbsoluteFill style={{ background: "#fafafa" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.5, 5.2]} lookAt={[0, 0.55, 0]} fov={32} />
        <ToneMapping mode="none" />
        <AmbientLight intensity={0.5} />
        <DirectionalLight position={[2.5, 5, 4]} intensity={1.4} castShadow />
        <PointLight position={[-3, 2, -2]} intensity={0.8} color="#c9c9d6" />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[12, 0.1, 6]} />
          <MeshStandardMaterial color="#ececec" roughness={0.95} />
        </Mesh>
        {LOOKS.map((look, i) => (
          <Model key={look} name={look} src={staticFile("cat.glb")} animation="Idle" look={look} flat theme="light" edgeColor="#b0b0b4" position={[(i - 1) * 1.45, 0, 0]} rotation={[0, 0.55, 0]} />
        ))}
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="cat-looks" component={Looks} width={1440} height={720} fps={30} durationInFrames={90} />);
