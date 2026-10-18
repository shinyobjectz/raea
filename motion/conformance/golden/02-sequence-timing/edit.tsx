// golden case 02: the same two cards as edit.rrl, timed with Sequences and moved with interpolate
import { AbsoluteFill, Sequence, interpolate, useCurrentFrame } from "../../../src";

const card = (bg: string) => ({ position: "absolute" as const, left: 90, width: 900, height: 300, background: bg, borderRadius: 32 });
const label = { position: "absolute" as const, top: 90, width: 900, textAlign: "center" as const, fontFamily: "Inter", fontSize: 110, fontWeight: 800, lineHeight: 1, color: "#fff" };

function Second() {
  const f = useCurrentFrame();
  const left = interpolate(f, [0, 15], [290, 90], { extrapolateRight: "clamp" });
  return <div style={{ ...card("#ff5a36"), top: 1100, left }}><div style={label}>SECOND</div></div>;
}

export default function SequenceTiming() {
  return (
    <AbsoluteFill style={{ background: "#101010" }}>
      <Sequence from={0} durationInFrames={40} layout="none">
        <div style={{ ...card("#2f6bff"), top: 300 }}><div style={label}>FIRST</div></div>
      </Sequence>
      <Sequence from={30} layout="none">
        <Second />
      </Sequence>
    </AbsoluteFill>
  );
}
