// record: bevy-3d-rendering-pbr
// Bevy's "PBR" example in raea's SDK: a grid of spheres, metalness rising left to right and roughness rising bottom
// to top. Without an environment map fully metal surfaces read dark, so metalness tops out at 0.8 (bevy/limits).
import { AbsoluteFill, Composition, registerRoot } from "@raea/motion";
import { AmbientLight, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, PointLight, Sphere } from "@raea/motion/3d";

const N = 5;

function Pbr() {
  const cells = [];
  for (let r = 0; r < N; r++) {
    for (let c = 0; c < N; c++) {
      cells.push(
        <Mesh key={`${r}-${c}`} position={[(c - 2) * 1.2, (r - 2) * 1.2, 0]}>
          <Sphere args={[0.48]} />
          <MeshStandardMaterial color="#c8553d" metalness={(c / (N - 1)) * 0.8} roughness={0.08 + (r / (N - 1)) * 0.9} />
        </Mesh>,
      );
    }
  }
  return (
    <AbsoluteFill style={{ background: "#202329" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 0, 11]} lookAt={[0, 0, 0]} fov={38} />
        <AmbientLight intensity={0.4} />
        <DirectionalLight position={[3, 4, 6]} intensity={1.2} />
        <PointLight position={[-4, 3, 4]} intensity={1.5} color="#bfd7ff" />
        {cells}
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="pbr" component={Pbr} width={1280} height={720} fps={30} durationInFrames={30} />);
