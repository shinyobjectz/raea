// <Decal> on a floor, a wall and a moving box, its opacity keyed: raea-motion render kitchen/test/decal-test.tsx --frame 0|30 -o x.png
import { AbsoluteFill, Composition, interpolate, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import { Box, Canvas3D, Decal, DirectionalLight, AmbientLight, Mesh, MeshStandardMaterial, PerspectiveCamera } from "@raea/motion/3d";

const D = (n: string) => staticFile("assets/decals/" + n);
function Scene() {
  const f = useCurrentFrame();
  const x = interpolate(f, [0, 30], [-0.6, 0.6]);
  return (
    <AbsoluteFill style={{ background: "#000" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.6, 3]} lookAt={[0, 0.6, 0]} fov={50} />
        <AmbientLight intensity={0.4} />
        <DirectionalLight position={[2, 4, 3]} intensity={2.5} />
        <Mesh position={[0, -0.05, 0]}><Box args={[4, 0.1, 4]} /><MeshStandardMaterial color="#d8d2c4" roughness={0.7} /></Mesh>
        <Mesh position={[0, 1, -1]}><Box args={[4, 2, 0.1]} /><MeshStandardMaterial color="#efe9df" roughness={0.8} /></Mesh>
        <Mesh position={[x, 0.3, 0.6]}><Box args={[0.5, 0.6, 0.5]} /><MeshStandardMaterial color="#8a6a4a" /></Mesh>
        <Decal src={D("coffee-ring.png")} position={[0.9, 0, 0.9]} size={0.35} opacity={interpolate(f, [0, 30], [0.2, 1])} />
        <Decal src={D("tiles-zellige.png")} position={[-0.6, 1.0, -0.95]} normal={[0, 0, 1]} size={[1.2, 0.9]} />
        <Decal src={D("label-honey.png")} position={[x, 0.35, 0.85]} normal={[0, 0, 1]} size={[0.32, 0.2]} />
      </Canvas3D>
    </AbsoluteFill>
  );
}
registerRoot(() => <Composition id="decal" component={Scene} width={960} height={540} fps={30} durationInFrames={31} />);
