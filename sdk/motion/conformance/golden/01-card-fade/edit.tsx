// golden case 01: the same card as edit.rrl, in @raea/motion
import { AbsoluteFill, interpolate, useCurrentFrame } from "../../../src";

export default function CardFade() {
  const frame = useCurrentFrame();
  const opacity = interpolate(frame, [0, 15], [0, 1], { extrapolateRight: "clamp" });
  return (
    <AbsoluteFill style={{ background: "#000" }}>
      <div style={{ position: "absolute", left: 90, top: 700, width: 900, height: 400, background: "#111", borderRadius: 40, opacity }}>
        <div style={{ position: "absolute", top: 120, width: 900, textAlign: "center", fontFamily: "Inter", fontSize: 160, fontWeight: 800, lineHeight: 1, color: "#c6ff3a" }}>30 MIN</div>
      </div>
    </AbsoluteFill>
  );
}
