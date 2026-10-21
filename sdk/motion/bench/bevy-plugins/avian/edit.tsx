// avian3d through the generated SDK (sdk/motion/docs/bevy-plugins.md): balls and crates dropped on a tilted floor, a
// gravity resource, bounciness and friction, and a crate kicked sideways at frame 60 (a component row from a frame).
// Every physics component comes from avian3d.tsx (beside it), written by raea-bevy-sdk from avian's TypeRegistry; no
// rdb system is per component. Render and play must agree: sdk/motion/bench/bevy/check.sh with
// RAEA_MOTION_BIN=bevy-plugins/motion.sh.
import { AbsoluteFill, Composition, registerRoot } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, Sphere } from "@raea/motion/3d";
import { AngularVelocity, ColliderConstructor, Friction, GravityResource, LinearVelocity, Restitution, RigidBody } from "./avian3d";

const FPS = 30;
const FRAMES = 150;

const balls = [
  { x: -1.6, y: 3.0, r: 0.35, c: "#e11d48", bounce: 0.8 },
  { x: -0.6, y: 4.2, r: 0.3, c: "#f59e0b", bounce: 0.5 },
  { x: 0.5, y: 3.6, r: 0.4, c: "#10b981", bounce: 0.2 },
  { x: 1.5, y: 5.0, r: 0.25, c: "#3b82f6", bounce: 0.9 },
];
const crates = [
  { x: -1.0, y: 6.0, s: 0.6, c: "#8b5cf6" },
  { x: 1.0, y: 7.0, s: 0.5, c: "#d946ef", kick: true },
];

function Drop() {
  return (
    <AbsoluteFill style={{ background: "#0f1720" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 2.5, 9]} lookAt={[0, 1, 0]} fov={45} />
        <AmbientLight intensity={0.5} />
        <DirectionalLight position={[3, 6, 4]} intensity={1.3} />
        <GravityResource value={[0, -9.81, 0]} />
        <Mesh name="floor" position={[0, 0, 0]} rotation={[0, 0, -0.12]}>
          <Box args={[8, 0.3, 4]} />
          <MeshStandardMaterial color="#334155" roughness={0.8} />
          <RigidBody value="Static" />
          <ColliderConstructor value={{ Cuboid: { x_length: 8, y_length: 0.3, z_length: 4 } }} />
          <Friction value={{ dynamic_coefficient: 0.6, static_coefficient: 0.6, combine_rule: "Average" }} />
        </Mesh>
        {balls.map((b, i) => (
          <Mesh key={`ball${i}`} name={`ball${i}`} driven position={[b.x, b.y, 0]}>
            <Sphere args={[b.r]} />
            <MeshStandardMaterial color={b.c} roughness={0.4} />
            <RigidBody value="Dynamic" />
            <ColliderConstructor value={{ Sphere: { radius: b.r } }} />
            <Restitution value={{ coefficient: b.bounce, combine_rule: "Max" }} />
          </Mesh>
        ))}
        {crates.map((c, i) => (
          <Mesh key={`crate${i}`} name={`crate${i}`} driven position={[c.x, c.y, 0]} rotation={[0.3, 0.2, 0.4]}>
            <Box args={[c.s, c.s, c.s]} />
            <MeshStandardMaterial color={c.c} roughness={0.5} />
            <RigidBody value="Dynamic" />
            <ColliderConstructor value={{ Cuboid: { x_length: c.s, y_length: c.s, z_length: c.s } }} />
            <AngularVelocity value={[0, 2, 1]} />
            {c.kick ? <LinearVelocity value={[-4, 3, 0]} from={60} /> : null}
          </Mesh>
        ))}
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="drop" component={Drop} width={1280} height={720} fps={FPS} durationInFrames={FRAMES} />);
