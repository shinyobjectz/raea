// Diagnosis, part 3: when one 3D thing unmounts and another mounts in its place in the tree (no names), does the new one
// draw as itself? Frames 0-9: "FOUR OPENINGS" top left and a small red ball bottom left. From frame 10: "FROM ITS STORE
// PAGE" top right and a big blue ball bottom right. Middle: a quote with maxWidth (the store card's missing quote).
import React from "react";
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "raea";
import { Canvas3D, Entity, RichText } from "raea/3d";
import { Mesh3d, Sphere } from "raea/mesh";
import { StandardMaterial } from "raea/pbr";
import { Srgba } from "raea/color";
import { Camera3d } from "raea/camera";
import { Transform } from "raea/transform";
import { AmbientLight } from "raea/light";

function Kept3() {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "#F1E9D8" }}>
      <Canvas3D>
        <Camera3d transform={Transform.fromXyz(0, 0, 7).lookingAt([0, 0, 0])} />
        <AmbientLight brightness={1000} />
        {f < 10 ? <>
          <Entity transform={Transform.fromXyz(-2, 1.4, 0)}><RichText size={0.35} font="League Gothic" weight={400} color="#1A1A1A" unlit>FOUR OPENINGS</RichText></Entity>
          <Mesh3d transform={Transform.fromXyz(-2, -1.4, 0)}><Sphere radius={0.15} /><StandardMaterial base_color={Srgba.hex("#E8402E")} unlit /></Mesh3d>
        </> : <>
          <Entity transform={Transform.fromXyz(2, 1.4, 0)}><RichText size={0.2} font="Space Mono" weight={700} color="#E8402E" unlit>FROM ITS STORE PAGE</RichText></Entity>
          <Mesh3d transform={Transform.fromXyz(2, -1.4, 0)}><Sphere radius={0.5} /><StandardMaterial base_color={Srgba.hex("#2F4BE0")} unlit /></Mesh3d>
        </>}
        <Entity transform={Transform.fromXyz(-1, 0.2, 0)}><RichText size={0.2} font="League Gothic" weight={400} color="#1A1A1A" align="left" maxWidth={1.9} unlit>“an Exclusive Solitaire Puzzle Game for Senior Citizens”</RichText></Entity>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="kept3" component={Kept3} width={640} height={360} fps={30} durationInFrames={20} />);
