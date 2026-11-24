// Diagnosis, part 4 (docs/stage-guards.feature SG9, 11-22): which of a scene's lights and its look follow the frame in one
// render process. Stay rendered whole drew every shot after its first far darker than the same frame rendered alone; its
// own render script says "an engine render keeps its first frame's sun, textures and glows". Each composition here
// changes one thing at frame 15 and holds it: the tone curve's exposure, the sun's strength, the sun's colour, the
// ambient light, a point light's colour and strength, and a box's glow. All follow the frame (0.1.9: SSIM 1.0). The
// solari-* half does the same under Solari, reset at the change as a cut is, with "none" as its control (0.9975: two
// Solari renders of one frame differ by their noise): only "gone", the box unmounted at frame 15, kept anything. A part
// a frame leaves out is hidden, and Solari traced hidden parts, so its shade stayed on the floor 44 frames on (0.987;
// 0.998 once hidden parts left the traced scene: kept-scenes.tsx has the whole-film shape).
// frame-pure: compositions exposure sun-lux solari-gone
// frame-pure: bar 0.99 solari-gone
import React from "react";
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "raea";
import { Canvas3D, ToneMapping } from "raea/3d";
import { Cuboid, Mesh3d, Plane3d } from "raea/mesh";
import { StandardMaterial } from "raea/pbr";
import { Srgba } from "raea/color";
import { Camera3d } from "raea/camera";
import { Transform } from "raea/transform";
import { AmbientLight, DirectionalLight, PointLight, SolariLighting } from "raea/light";

type Change = "none" | "exposure" | "sun-lux" | "sun-colour" | "ambient" | "point" | "glow" | "camera" | "parts" | "gone";

function Scene({ change, solari = false }: { change: Change; solari?: boolean }) {
  const f = useCurrentFrame(), late = f >= 15;
  const is = (c: Change) => late && change === c;
  return (
    <AbsoluteFill style={{ background: "#000" }}>
      <Canvas3D>
        <Camera3d transform={is("camera") ? Transform.fromXyz(-2, 1.6, 4).lookingAt([0, 0.4, 0]) : Transform.fromXyz(0, 2.2, 6).lookingAt([0, 0.4, 0])}>
          {solari && <SolariLighting bounces={2} reset={f === 0 || f === 15} />}
        </Camera3d>
        <ToneMapping mode="agx" exposure={is("exposure") ? -3 : 0} />
        <AmbientLight brightness={is("ambient") ? 2000 : 80} />
        <DirectionalLight name="sun" illuminance={is("sun-lux") ? 20000 : 2000} color={is("sun-colour") ? "#4080ff" : "#ffd2a8"} shadow_maps_enabled transform={Transform.fromXyz(3, 6, 4).lookingAt([0, 0, 0])} />
        <PointLight name="lamp" intensity={is("point") ? 400000 : 20000} color={is("point") ? "#40ff60" : "#ffffff"} transform={Transform.fromXyz(-1.6, 1.4, 1)} />
        <Mesh3d name="floor"><Plane3d half_size={[6, 6]} /><StandardMaterial base_color={Srgba.hex("#b8b0a4")} /></Mesh3d>
        {is("parts") && <Mesh3d name="wall" transform={Transform.fromXyz(0, 1.2, -1.5)}><Cuboid half_size={[3, 1.2, 0.05]} /><StandardMaterial base_color={Srgba.hex("#e0d8cc")} /></Mesh3d>}
        {!is("gone") && <Mesh3d name="box" transform={Transform.fromXyz(0.9, 0.6, 0)}><Cuboid half_size={[0.6, 0.6, 0.6]} /><StandardMaterial base_color={Srgba.hex("#c84a2e")} /></Mesh3d>}
        <Mesh3d name="glow" transform={Transform.fromXyz(-1, 0.4, 0.4)}><Cuboid half_size={[0.4, 0.4, 0.4]} /><StandardMaterial base_color={Srgba.hex("#202020")} emissive={is("glow") ? [8, 6, 1, 1] : [0, 0, 0, 1]} /></Mesh3d>
      </Canvas3D>
    </AbsoluteFill>
  );
}

const CHANGES: Change[] = ["exposure", "sun-lux", "sun-colour", "ambient", "point", "glow"];
// the same under Solari, reset on the change as a cut would be, with "none" as the control, the camera, a new part, and
// "gone": the box unmounted at frame 15
const SOLARI: Change[] = ["none", "exposure", "sun-lux", "camera", "parts", "glow", "gone"];
registerRoot(() => (
  <>
    {CHANGES.map((c) => <Composition key={c} id={c} component={() => <Scene change={c} />} width={480} height={270} fps={30} durationInFrames={30} />)}
    {SOLARI.map((c) => <Composition key={`solari-${c}`} id={`solari-${c}`} component={() => <Scene change={c} solari />} width={480} height={270} fps={30} durationInFrames={60} />)}
  </>
));
