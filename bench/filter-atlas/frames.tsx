// #125: ten full-frame layers at 2160x3840 whose own boxes draw (a white background at 10%), each under a filter that
// leaves white as it is. Each is an image the size of the frame in Vello's atlas: 8192 px a side holds six, the
// device's largest texture (16384 on Apple silicon) holds 28. Over #333 the ten make 255 - 204 * 0.9^10 = 184; nine
// would be 176. bench/filter-atlas/check.sh
import { AbsoluteFill, Composition, registerRoot } from "raea";
const N = 10;
function C() {
  return (
    <AbsoluteFill style={{ background: "#333" }}>
      {Array.from({ length: N }, (_, i) => (
        <AbsoluteFill key={i} style={{ filter: "grayscale(1)", background: "rgba(255, 255, 255, 0.1)" }} />
      ))}
    </AbsoluteFill>
  );
}
registerRoot(() => <Composition id="frames" component={C} width={2160} height={3840} fps={30} durationInFrames={1} />);
