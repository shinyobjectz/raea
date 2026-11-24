// Diagnosis: does a 3D thing's shape follow its props from frame to frame in one render process? Five things grow over
// 30 frames. A: Sphere radius. B: Cuboid half_size. C: the same box grown by its transform's scale (control). D: a
// RichText whose words are the frame number. E: a Sphere whose React key changes every frame (remounted). Rendered as
// one sequence and as separate stills, the two should match frame for frame.
import React from "react";
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "raea";
import { Canvas3D, Entity, RichText } from "raea/3d";
import { Cuboid, Mesh3d, Sphere } from "raea/mesh";
import { StandardMaterial } from "raea/pbr";
import { Srgba } from "raea/color";
import { Camera3d } from "raea/camera";
import { Transform } from "raea/transform";
import { AmbientLight } from "raea/light";

const M = ({ c }: { c: string }) => <StandardMaterial base_color={Srgba.hex(c)} unlit />;

function Kept() {
  const f = useCurrentFrame(), t = f / 29;
  return (
    <AbsoluteFill style={{ background: "#F1E9D8" }}>
      <Canvas3D>
        <Camera3d transform={Transform.fromXyz(0, 0, 6).lookingAt([0, 0, 0])} />
        <AmbientLight brightness={1000} />
        <Mesh3d name="a" transform={Transform.fromXyz(-2.4, 0, 0)}><Sphere radius={0.1 + 0.5 * t} /><M c="#E8402E" /></Mesh3d>
        <Mesh3d name="b" transform={Transform.fromXyz(-1.2, 0, 0)}><Cuboid half_size={[0.15, 0.1 + 0.5 * t, 0.15]} /><M c="#2F4BE0" /></Mesh3d>
        <Mesh3d name="c" transform={Transform.fromXyz(0, 0, 0).withScale([1, 0.2 + t, 1])}><Cuboid half_size={[0.15, 0.5, 0.15]} /><M c="#1A1A1A" /></Mesh3d>
        <Entity transform={Transform.fromXyz(1.2, 0, 0)}><RichText name="d" size={0.5} font="Space Mono" weight={700} color="#1A1A1A" unlit>{String(f)}</RichText></Entity>
        <Mesh3d key={f} name={`e${f}`} transform={Transform.fromXyz(2.4, 0, 0)}><Sphere radius={0.1 + 0.5 * t} /><M c="#F2B92B" /></Mesh3d>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="kept" component={Kept} width={640} height={360} fps={30} durationInFrames={30} />);
