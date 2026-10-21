// Lens fixture: a white wall ruled with black lines (unlit, so pixels are exact) seen square-on, and a fisheye that
// holds at strength 0 to frame 9, ramps to 1 by frame 40 and holds. A 2D label on the page sits over the 3D: the lens must leave it alone. lens/check.py measures straight
// centre lines, monotonic curves away from the centre, strength 0 = no lens, the page untouched and determinism;
// render equals its baked play (check.sh).
import { AbsoluteFill, Composition, registerRoot, track } from "@raea/motion";
import { Box, Canvas3D, Fisheye, Mesh, MeshBasicMaterial, PerspectiveCamera } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;
const LENS = { strength: track({ 0: 0, 9: 0, 40: 1 }, "in-out-sine") };
const XS = [-4, -3, -2, -1, 0, 1, 2, 3, 4];
const YS = [-2, -1, 0, 1, 2];

function Shot() {
  return (
    <AbsoluteFill style={{ background: "#202020" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 0, 6]} lookAt={[0, 0, 0]} fov={50} />
        {/* lens */}<Fisheye strength={0} animate={LENS} />
        <Mesh name="wall" position={[0, 0, -0.1]}>
          <Box args={[24, 14, 0.05]} />
          <MeshBasicMaterial color="#ffffff" />
        </Mesh>
        {XS.map((x) => (
          <Mesh key={`v${x}`} name={`v${x}`} position={[x, 0, 0]}>
            <Box args={[0.04, 14, 0.02]} />
            <MeshBasicMaterial color="#000000" />
          </Mesh>
        ))}
        {YS.map((y) => (
          <Mesh key={`h${y}`} name={`h${y}`} position={[0, y, 0]}>
            <Box args={[24, 0.04, 0.02]} />
            <MeshBasicMaterial color="#000000" />
          </Mesh>
        ))}
      </Canvas3D>
      <div style={{ position: "absolute", left: 24, top: 24, width: 180, height: 44, background: "#ff3366", color: "#ffffff", font: "700 26px Inter", display: "flex", alignItems: "center", justifyContent: "center" }}>STRAIGHT</div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="lens" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
