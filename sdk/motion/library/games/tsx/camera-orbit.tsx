// record: bevy-camera-camera-orbit, bevy-math-smooth-follow
// Bevy's "camera orbit" and "smooth follow" examples in raea's SDK. The camera is a function of the frame: it orbits
// the scene, and a marker follows a moving target with exponential smoothing, worked out in closed form per frame
// (a follower that lags by tau seconds behind a target moving on a circle) so any frame renders alone.
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame, useVideoConfig } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, Sphere } from "@raea/motion/3d";

function Orbit() {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const t = f / fps;
  const yaw = t * 0.6;
  const cam: [number, number, number] = [Math.sin(yaw) * 8, 3.5, Math.cos(yaw) * 8];
  const w = 1.5; // the target's angular speed (rad/s)
  const target: [number, number, number] = [Math.cos(t * w) * 2.5, 0.4, Math.sin(t * w) * 2.5];
  // steady state of x' = (target - x) / tau for circular motion: the same circle, smaller and phase-lagged
  const tau = 0.4;
  const gain = 1 / Math.sqrt(1 + (w * tau) ** 2);
  const lag = Math.atan(w * tau);
  const follow: [number, number, number] = [Math.cos(t * w - lag) * 2.5 * gain, 0.4, Math.sin(t * w - lag) * 2.5 * gain];
  return (
    <AbsoluteFill style={{ background: "#0f1720" }}>
      <Canvas3D>
        <PerspectiveCamera position={cam} lookAt={[0, 0.4, 0]} fov={45} />
        <AmbientLight intensity={0.4} />
        <DirectionalLight position={[4, 8, 3]} intensity={1.2} castShadow />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[10, 0.1, 10]} />
          <MeshStandardMaterial color="#c4bfb2" roughness={0.9} />
        </Mesh>
        <Mesh name="target" position={target}>
          <Sphere args={[0.3]} />
          <MeshStandardMaterial color="#ef476f" />
        </Mesh>
        <Mesh name="follower" position={follow}>
          <Box args={[0.45, 0.45, 0.45]} />
          <MeshStandardMaterial color="#06d6a0" />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="orbit" component={Orbit} width={1280} height={720} fps={30} durationInFrames={150} />);
