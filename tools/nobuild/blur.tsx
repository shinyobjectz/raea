import { Composition, registerRoot, useCurrentFrame } from "raea";
import { MotionBlur } from "raea/post_process";
import { Camera3d } from "raea/camera";
import { DirectionalLight } from "raea/light";
import { Cuboid, Mesh3d } from "raea/mesh";
import { StandardMaterial } from "raea/pbr";
import { Transform } from "raea/transform";

// docs/nobuild.feature NB1a: <MotionBlur kind> against RAEA_MOTION_BLUR (tools/nobuild.sh props). A block whipping
// across a still floor.
function Scene({ kind }: { kind: "shutter" | "bevy" }) {
  const f = useCurrentFrame();
  return (
    <>
      <Camera3d transform={Transform.fromXyz(0, 1.5, 5).lookingAt([0, 0.5, 0])} />
      <MotionBlur shutterAngle={1} kind={kind} />
      <DirectionalLight illuminance={6000} transform={Transform.fromXyz(3, 5, 2).lookingAt([0, 0, 0])} />
      <Mesh3d><Cuboid half_size={[5, 0.02, 5]} /><StandardMaterial base_color="#3a3d44" /></Mesh3d>
      <Mesh3d transform={Transform.fromXyz(-3 + f * 0.6, 0.5, 0)}><Cuboid half_size={[0.4, 0.4, 0.4]} /><StandardMaterial base_color="#f1c40f" /></Mesh3d>
    </>
  );
}

registerRoot(() => (
  <>
    <Composition id="shutter" component={() => <Scene kind="shutter" />} width={320} height={180} fps={30} durationInFrames={8} />
    <Composition id="bevy" component={() => <Scene kind="bevy" />} width={320} height={180} fps={30} durationInFrames={8} />
  </>
));
