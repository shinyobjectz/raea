// Repro (docs/stage-guards.feature SG1): a curve drawn over time as a chain of boxes. The film's Ribbon was a box a
// segment of a 48-point curve, its newest box growing in length each frame; in a sequence render the chain broke into
// dashes, since a box's size stayed its first frame's, while each frame rendered alone was solid. "grow" draws it over 90
// frames: a frame of the sequence must match the frame alone. "whole" is the same 48 boxes from the first frame, the
// cost's control: a growing shape (one mesh a frame, cached) renders within 10% of it.
// frame-pure: compositions grow
import React from "react";
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "raea";
import { Canvas3D } from "raea/3d";
import { Cuboid, Mesh3d } from "raea/mesh";
import { StandardMaterial } from "raea/pbr";
import { Srgba } from "raea/color";
import { Camera3d } from "raea/camera";
import { Quat, Transform } from "raea/transform";
import { AmbientLight, DirectionalLight } from "raea/light";

const N = 48;
// a rise, a peak and a long fall, x 0..4.8 m, y 0..1.6 m (the film's lifeCurve)
const PTS: [number, number][] = Array.from({ length: N + 1 }, (_, i) => {
  const x = i / N;
  const y = x < 0.22 ? Math.sin((x / 0.22) * Math.PI / 2) : Math.exp(-(x - 0.22) * 3.2);
  return [x * 4.8 - 2.4, y * 1.6 - 0.6];
});

function Ribbon({ p }: { p: number }) {
  const upto = Math.min(1, Math.max(0, p)) * N;
  const segs: React.ReactNode[] = [];
  for (let i = 0; i < N; i++) {
    const k = Math.min(1, Math.max(0, upto - i));
    if (k <= 0) break;
    const [x0, y0] = PTS[i];
    const [x1, y1] = [x0 + (PTS[i + 1][0] - x0) * k, y0 + (PTS[i + 1][1] - y0) * k];
    const len = Math.hypot(x1 - x0, y1 - y0) + 0.06;
    segs.push(
      <Mesh3d key={i} name={`rib-${i}`} allowOverlap transform={Transform.fromXyz((x0 + x1) / 2, (y0 + y1) / 2, 0).withRotation(Quat.fromEuler("XYZ", 0, 0, Math.atan2(y1 - y0, x1 - x0)))}>
        <Cuboid half_size={[len / 2, 0.035, 0.02]} />
        <StandardMaterial base_color={Srgba.hex("#d8342c")} />
      </Mesh3d>,
    );
  }
  return <>{segs}</>;
}

function Scene({ grow }: { grow: boolean }) {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "#f4efe6" }}>
      <Canvas3D>
        <Camera3d transform={Transform.fromXyz(0, 0.2, 4.2).lookingAt([0, 0.2, 0])} />
        <AmbientLight brightness={600} />
        <DirectionalLight name="sun" illuminance={6000} transform={Transform.fromXyz(2, 4, 5).lookingAt([0, 0, 0])} />
        <Ribbon p={grow ? f / 89 : 1} />
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => (
  <>
    <Composition id="grow" component={() => <Scene grow />} width={640} height={360} fps={30} durationInFrames={90} />
    <Composition id="whole" component={() => <Scene grow={false} />} width={640} height={360} fps={30} durationInFrames={90} />
  </>
));
