// Diagnosis, part 2 (docs/stage-guards.feature SG0): which other 3D props follow the frame in one render process. Top
// row, left to right: RichText revealed by its own enter animation from frame 5; RichText whose colour changes at frame
// 15; RichText whose size grows; a box whose material colour changes at frame 15. Bottom row: a Text3D whose letters
// count the frame; RichText first mounted at frame 10, revealed from there. Before SG1, the colour, the size and the
// letters held their first frame's values in a sequence render (a picture's change, the material and the reveals
// followed the frame). The growing word starts too small to read, on purpose: read={false} keeps raea render's stage
// checks (SG10) off it.
import React from "react";
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "raea";
import { Canvas3D, Entity, RichText, Text3D } from "raea/3d";
import { Cuboid, Mesh3d } from "raea/mesh";
import { StandardMaterial } from "raea/pbr";
import { Srgba } from "raea/color";
import { Camera3d } from "raea/camera";
import { Transform } from "raea/transform";
import { AmbientLight } from "raea/light";

function Kept2() {
  const f = useCurrentFrame(), late = f >= 15;
  return (
    <AbsoluteFill style={{ background: "#F1E9D8" }}>
      <Canvas3D>
        <Camera3d transform={Transform.fromXyz(0, 0, 7).lookingAt([0, 0, 0])} />
        <AmbientLight brightness={1000} />
        <Entity transform={Transform.fromXyz(-2.7, 1, 0)}><RichText name="reveal" size={0.35} font="Space Mono" weight={700} color="#1A1A1A" unlit enter="rise" each={2} at={5}>REVEAL</RichText></Entity>
        <Entity transform={Transform.fromXyz(-0.9, 1, 0)}><RichText name="colour" size={0.35} font="Space Mono" weight={700} color={late ? "#2F4BE0" : "#E8402E"} unlit>INK</RichText></Entity>
        <Entity transform={Transform.fromXyz(0.9, 1, 0)}><RichText name="size" size={0.15 + 0.02 * f} read={false} font="Space Mono" weight={700} color="#1A1A1A" unlit>BIG</RichText></Entity>
        <Mesh3d name="mat" transform={Transform.fromXyz(2.7, 1, 0)}><Cuboid half_size={[0.4, 0.4, 0.1]} /><StandardMaterial base_color={Srgba.hex(late ? "#2F4BE0" : "#E8402E")} unlit /></Mesh3d>
        <Mesh3d name="t3d" transform={Transform.fromXyz(0, -1.2, 0)}><Text3D size={0.6} depth={0.1}>{String(f)}</Text3D><StandardMaterial base_color={Srgba.hex("#1A1A1A")} unlit /></Mesh3d>
        {f >= 10 && <Entity transform={Transform.fromXyz(2.4, -1, 0)}><RichText name="late" size={0.35} font="Space Mono" weight={700} color="#1A1A1A" unlit enter="rise" each={2} at={10}>LATE</RichText></Entity>}
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="kept2" component={Kept2} width={640} height={360} fps={30} durationInFrames={30} />);
