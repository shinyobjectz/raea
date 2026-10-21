// void's cat, every clip side by side, flat as it's always shown: a check that the rig moves as it should.
import { AbsoluteFill, Composition, registerRoot, staticFile } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, Model, PerspectiveCamera, ToneMapping } from "@raea/motion/3d";

const CLIPS = ["Idle", "Walk", "Trot", "Sit", "LookAround", "Pounce"];

function Clips() {
  return (
    <AbsoluteFill style={{ background: "#fafafa" }}>
      <Canvas3D>
        <PerspectiveCamera position={[1.2, 2.2, 6.5]} lookAt={[0, 0.5, 0]} fov={35} />
        <ToneMapping mode="none" />
        <AmbientLight intensity={0.6} />
        <DirectionalLight position={[3, 6, 4]} intensity={1.3} castShadow />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[14, 0.1, 8]} />
          <MeshStandardMaterial color="#ececec" roughness={0.9} />
        </Mesh>
        {CLIPS.map((c, i) => (
          <Model key={c} name={c} src={staticFile("cat.glb")} animation={c} flat position={[(i - 2.5) * 1.15, 0, 0]} rotation={[0, 0.9, 0]} />
        ))}
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="cat-clips" component={Clips} width={1440} height={540} fps={30} durationInFrames={120} />);
