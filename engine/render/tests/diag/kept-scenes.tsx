// Diagnosis, part 5 (docs/stage-guards.feature SG9, 11-22): a film that changes place, rendered whole under Solari.
// Stay opened on black, then cut between places, each with its own sun, exposure and parts, Solari reset on each cut;
// its later shots drew far darker in one render than alone. Every single change of a light or a look followed the frame
// (kept-light.tsx), and so did a place alone ("room-only", the control). The cause: a part a scene leaves is hidden,
// not despawned, and Solari traced hidden parts unless <SolariLighting dynamic>, a trial, was on, so the yard's floor
// and box stayed in the traced scene through the room, unseen, shading it (the room at frame 60: SSIM 0.943 to the
// frame alone, luma 140 against 146; 0.997 once hidden parts left the traced scene). "solari" is Stay's shape: a black
// open, a yard, then a room; "plain" is the same with no Solari. (A point light changes nothing here: Solari traces only
// the sun and glowing parts.)
// frame-pure: compositions solari room-only
// frame-pure: bar 0.99
import React from "react";
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "raea";
import { Canvas3D, ToneMapping } from "raea/3d";
import { Cuboid, Mesh3d, Plane3d, Sphere } from "raea/mesh";
import { StandardMaterial } from "raea/pbr";
import { Srgba } from "raea/color";
import { Camera3d } from "raea/camera";
import { Transform } from "raea/transform";
import { DirectionalLight, SolariLighting } from "raea/light";

const A = 15, B = 45;

function Yard() {
  return (
    <>
      <Mesh3d name="yard-floor"><Plane3d half_size={[8, 8]} /><StandardMaterial base_color={Srgba.hex("#9fb07a")} /></Mesh3d>
      <Mesh3d name="yard-box" transform={Transform.fromXyz(0.4, 0.6, 0)}><Cuboid half_size={[0.6, 0.6, 0.6]} /><StandardMaterial base_color={Srgba.hex("#d8d0c0")} /></Mesh3d>
      <Mesh3d name="yard-post" transform={Transform.fromXyz(-0.9, 0.8, -0.4)}><Cuboid half_size={[0.2, 0.8, 0.2]} /><StandardMaterial base_color={Srgba.hex("#7a5a3a")} /></Mesh3d>
    </>
  );
}

function Room() {
  return (
    <>
      <Mesh3d name="room-floor"><Plane3d half_size={[3, 3]} /><StandardMaterial base_color={Srgba.hex("#b8a890")} /></Mesh3d>
      <Mesh3d name="room-back" transform={Transform.fromXyz(0, 1.5, -2)}><Cuboid half_size={[3, 1.5, 0.05]} /><StandardMaterial base_color={Srgba.hex("#e0d8cc")} /></Mesh3d>
      <Mesh3d name="room-ball" transform={Transform.fromXyz(-0.6, 0.5, 0)}><Sphere radius={0.5} /><StandardMaterial base_color={Srgba.hex("#c84a2e")} /></Mesh3d>
    </>
  );
}

function Film({ solari, only = false }: { solari: boolean; only?: boolean }) {
  const f = useCurrentFrame();
  const room = only || f >= B;
  const cut = only ? f === 0 : f === A || f === B;
  if (!only && f < A) return <AbsoluteFill style={{ background: "#000" }} />;
  return (
    <AbsoluteFill style={{ background: "#000" }}>
      <Canvas3D>
        <Camera3d transform={room ? Transform.fromXyz(0, 1.4, 3.2).lookingAt([0, 0.6, 0]) : Transform.fromXyz(0, 1.5, 3).lookingAt([0, 0.5, 0])}>
          {solari && <SolariLighting bounces={2} reset={cut} />}
        </Camera3d>
        <ToneMapping mode="agx" exposure={room ? 1 : -2} />
        <DirectionalLight name="sun" illuminance={room ? 2000 : 20000} color="#ffd2a8" shadow_maps_enabled transform={Transform.fromXyz(3, 6, 4).lookingAt([0, 0, 0])} />
        {room ? <Room /> : <Yard />}
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => (
  <>
    <Composition id="solari" component={() => <Film solari />} width={480} height={270} fps={30} durationInFrames={90} />
    <Composition id="plain" component={() => <Film solari={false} />} width={480} height={270} fps={30} durationInFrames={90} />
    <Composition id="room-only" component={() => <Film solari only />} width={480} height={270} fps={30} durationInFrames={90} />
  </>
));
