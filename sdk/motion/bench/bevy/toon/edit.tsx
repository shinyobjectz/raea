// Toon fixture: MeshToonMaterial with ink outlines on a Manifold banana (an ellipsoid bent by a warp), a
// sphere, a torus and a box (hard edges: the outline must stay closed), and a second, far sphere like the first (three
// bands; its outline must be as wide in pixels). One key light with shadows and an ambient fill. The banana turns, the box's
// colour blends in Oklab through an `animate` track and its outline thickens; the torus has three bands and a
// highlight. Render and the baked play with its 3D must agree (check.sh); toon/check.py measures the bands, the
// outline and determinism.
import { AbsoluteFill, Composition, registerRoot, track, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshToonMaterial, PerspectiveCamera, Sphere, Torus, manifold } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;

// the banana: a smoothed sphere stretched long, refined, bent along an arc and pinched at the ends (warp), with a stem
const BANANA = manifold.sphere(1).smooth({ refine: 4 }).scale(1.25, 0.3, 0.3).refineToLength(0.06).warp((x, y, z) => {
  const t = x / 1.25;
  const pinch = 1 - 0.35 * t * t * t * t;
  return [x, y * pinch + 0.55 * t * t - 0.3, z * pinch];
}).union(manifold.cylinder(0.055, 0.32).rotate(0, 90, 0).rotate(0, 0, 28).move(1.18, 0.3, 0));

const BOX = {
  color: track({ 0: "#7bd389", 59: "#5a6cff" }, "in-out-cubic"),
  outlineWidth: track({ 0: 2, 59: 4 }, "linear"),
};

function Shot() {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "#f3efe6" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 0.9, 7.5]} lookAt={[0, 0.2, 0]} fov={40} />
        <AmbientLight intensity={0.35} />
        <DirectionalLight position={[4, 5, 3]} intensity={1.2} />
        <Mesh name="banana" position={[-2.2, 0.2, 0]} rotation={[0.25, f * 0.05, 0.2]} shape={BANANA}>
          <MeshToonMaterial color="#ffd23f" steps={2} rim={{ intensity: 0.7, color: "#fff6c8", width: 0.22 }} outline={{ width: 3 }} />
        </Mesh>
        <Mesh name="near" position={[0.2, 0.3, 0]}>
          <Sphere args={[0.75]} />
          <MeshToonMaterial color="#3aa0ff" steps={2} rim={{ intensity: 0.6, width: 0.2 }} outline={{ width: 3 }} />
        </Mesh>
        <Mesh name="far" position={[1.4, 1.6, -9]}>
          <Sphere args={[0.75]} />
          <MeshToonMaterial color="#3aa0ff" steps={3} outline={{ width: 3 }} />
        </Mesh>
        <Mesh name="torus" position={[2.4, 0.1, 0.3]} rotation={[0.9 + f * 0.03, 0.4, 0]}>
          <Torus args={[0.6, 0.2]} />
          <MeshToonMaterial color="#ff5a7a" steps={3} highlight={{ size: 0.25 }} outline={{ width: 2, color: "#2a0a14" }} />
        </Mesh>
        <Mesh name="box" position={[0.4, -1.25, 0.8]} rotation={[0.5, 0.7 + f * 0.02, 0]}>
          <Box args={[0.7, 0.7, 0.7]} />
          <MeshToonMaterial color="#7bd389" steps={2} outline={{ width: 2 }} animate={BOX} />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="toon" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
