// The glyph atlas (docs/stage-guards.feature SG3). bevy_rich_text3d's atlas starts at 512x512 and doubles its height when
// a glyph doesn't fit. "cold" draws the film's glyph set in League Gothic and Space Mono into a fresh atlas, so the
// atlas doubles while those glyphs are allocated; "warm" first fills it with six weights of Inter out of shot, so it
// has doubled before them. Before SG3 the glyph whose allocation doubled the atlas was drawn into the old bounds and
// stayed cut ("cold" cut a Space Mono H; "warm" cut nothing, which is why the film warmed its atlas). The two must match
// (tools/frame-pure-3d.sh compares them).
import React from "react";
import { AbsoluteFill, Composition, registerRoot } from "raea";
import { Canvas3D, Entity, RichText } from "raea/3d";
import { Camera3d } from "raea/camera";
import { Transform } from "raea/transform";

const G = "ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz 0123456789 .,:;!?()[]{}<>/|-_=+*#&%$@~^`·–—…’“”×'\"\\\\";

function Probe({ warm }: { warm: number }) {
  const weights = [100, 300, 500, 700, 900, 200].slice(0, warm);
  return (
    <AbsoluteFill style={{ background: "#F2EADB" }}>
      <Canvas3D>
        <Camera3d transform={Transform.fromXyz(0, 0, 6).lookingAt([0, 0, 0])} />
        {weights.map((w, i) => <Entity key={i} transform={Transform.fromXyz(0, -60 - i, 0)}><RichText size={0.1} font="Inter" weight={w} unlit>{G}</RichText></Entity>)}
        <Entity transform={Transform.fromXyz(0, 0.6, 0)}><RichText size={0.3} maxWidth={8} font="League Gothic" weight={400} color="#141414" unlit>{G}</RichText></Entity>
        <Entity transform={Transform.fromXyz(0, -0.9, 0)}><RichText size={0.22} maxWidth={8} font="Space Mono" weight={700} color="#141414" unlit>{G}</RichText></Entity>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => (
  <>
    <Composition id="cold" component={() => <Probe warm={0} />} width={1920} height={1080} fps={30} durationInFrames={1} />
    <Composition id="warm" component={() => <Probe warm={6} />} width={1920} height={1080} fps={30} durationInFrames={1} />
  </>
));
