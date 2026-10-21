// Layers fixture: a layered Canvas3D between page elements. An orange disc and a label before the Canvas3D are under
// the 3D (seen around the sphere, hidden by it); a green bar after it is over the sphere. The page's own background is
// under everything. layers/check.py measures occlusion and that the page under the 3D is the page alone where nothing
// covers it; render equals its baked play (check.sh).
import { AbsoluteFill, Composition, interpolate, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, Sphere } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;

function Shot() {
  const f = useCurrentFrame();
  const x = interpolate(f, [0, FRAMES - 1], [-0.6, 0.6]);
  return (
    <AbsoluteFill style={{ background: "#e8eef6" }}>
      {/* under the 3D */}
      <div style={{ position: "absolute", left: 300, top: 90, width: 360, height: 360, borderRadius: 180, background: "#ff8a1f" }} />
      <div style={{ position: "absolute", left: 40, top: 40, font: "800 44px Inter", color: "#1d2a44" }}>UNDER</div>
      <Canvas3D layered>{/* layers:3d */}
        <PerspectiveCamera position={[0, 0, 6]} lookAt={[0, 0, 0]} fov={40} />
        <AmbientLight intensity={0.4} />
        <DirectionalLight position={[3, 4, 5]} intensity={1.2} />
        <Mesh name="ball" position={[x, 0, 0]}>
          <Sphere args={[1.1]} />
          <MeshStandardMaterial color="#3a6cff" roughness={0.4} />
        </Mesh>
      </Canvas3D>{/* layers:3d */}
      {/* over the 3D */}
      <div style={{ position: "absolute", left: 0, top: 250, width: 960, height: 40, background: "#22b573" }} />
      <div style={{ position: "absolute", right: 40, bottom: 40, font: "800 44px Inter", color: "#1d2a44" }}>OVER</div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="layers" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
