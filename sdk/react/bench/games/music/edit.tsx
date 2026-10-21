// MUSIC: a short loop on @raea/motion/music (drums, bass and chords at 120 bpm in 4/4, two bars looped over four),
// played by the one mixer core's instruments. The bounce of a render and the tap of a
// play must be the same samples (raea-motion ci bounce-vs-live, sdk/react/src/ci.rs).
import { AbsoluteFill, Composition, registerRoot } from "@raea/motion";
import { Music, Part, Sequence, Transport } from "@raea/motion/music";

const transport = new Transport({ bpm: 120, timeSignature: [4, 4] });

const kick = new Sequence("kick", ["C1", null, null, null, "C1", null, "C1", null], "8n", { loop: true, name: "kick" });
const snare = new Sequence("snare", [null, "D1", null, "D1"], "4n", { loop: true, name: "snare" });
const hats = new Sequence("hat", ["F#1", "F#1", "F#1", ["F#1", "F#1"], "F#1", "F#1", "F#1", "F#1"], "8n", { loop: true, velocity: 0.6, name: "hats" });
const bass = new Part(
  "bass",
  [
    { time: "0:0:0", note: "A1", duration: "8n" },
    { time: "0:0:2", note: "A2", duration: "16n" },
    { time: "0:1:0", note: "A1", duration: "8n" },
    { time: "0:2:0", note: "C2", duration: "8n" },
    { time: "0:3:0", note: "E2", duration: "8n" },
    { time: "1:0:0", note: "F1", duration: "8n" },
    { time: "1:1:0", note: "F1", duration: "8n" },
    { time: "1:2:0", note: "G1", duration: "8n" },
    { time: "1:3:2", note: "G2", duration: "16n" },
  ],
  { loop: "2m", name: "bass" },
);
const chords = new Part(
  "pad",
  [
    { time: "0:0:0", note: ["A3", "C4", "E4"], duration: "1m", velocity: 0.7 },
    { time: "1:0:0", note: ["F3", "A3", "C4"], duration: "2n", velocity: 0.7 },
    { time: "1:2:0", note: ["G3", "B3", "D4"], duration: "2n", velocity: 0.7 },
  ],
  { loop: "2m", name: "chords" },
);
export const parts = [kick, snare, hats, bass, chords];

function Loop() {
  return (
    <AbsoluteFill style={{ background: "#14121c" }}>
      <Music transport={transport} parts={parts} />
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="music" component={Loop} width={640} height={360} fps={30} durationInFrames={240} />);
