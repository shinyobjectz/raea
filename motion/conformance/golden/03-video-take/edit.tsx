// golden case 03: a real take (decoded natively at its source time) under a title, with a music bed at -6 dB; media
// imported as a module resolves to its path
import { AbsoluteFill, Audio, Video } from "../../../src";
import take from "../../../../media/fixtures/take-h264.mp4";
import bed from "../../../../media/fixtures/music-bed.m4a";

export default function RealTake() {
  return (
    <AbsoluteFill style={{ background: "#000" }}>
      <Audio src={bed} volume={0.5011872} />
      <Video src={take} startFrom={60} style={{ position: "absolute", left: 0, top: 0, width: 1080, height: 1920 }} />
      <div style={{ position: "absolute", top: 300, width: 1080, textAlign: "center", fontFamily: "Inter", fontSize: 120, fontWeight: 800, lineHeight: 1, color: "#ffffff" }}>REAL TAKE</div>
    </AbsoluteFill>
  );
}
