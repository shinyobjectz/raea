// #125: twelve full-frame wrappers at 2160x3840, each blurred and each around a 200 px white square. A wrapper's box
// draws nothing, so its filter's layer is the square grown by the blur's reach, as Chrome sizes a filter's input. Made
// from the wrapper's box, each layer was a frame-sized image, Vello's atlas held six and the frame was refused from the
// seventh. bench/filter-atlas/check.sh
import { AbsoluteFill, Composition, registerRoot } from "raea";
const N = 12;
function C() {
  return (
    <AbsoluteFill style={{ background: "#333" }}>
      {Array.from({ length: N }, (_, i) => (
        <AbsoluteFill key={i} style={{ filter: "blur(4px)" }}>
          <div style={{ position: "absolute", left: 100 + (i % 6) * 340, top: 1500 + Math.floor(i / 6) * 500, width: 200, height: 200, background: "white" }} />
        </AbsoluteFill>
      ))}
    </AbsoluteFill>
  );
}
registerRoot(() => <Composition id="squares" component={C} width={2160} height={3840} fps={30} durationInFrames={1} />);
