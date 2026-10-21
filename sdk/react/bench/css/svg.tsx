// React's camelCase SVG props reach the page as SVG's attributes (sdk/react/bench/css/check.sh; lib.rs attr_name):
// strokeWidth is stroke-width, fillOpacity fill-opacity, strokeDasharray stroke-dasharray. Read under their React
// names, the line was 1 px wide, the square opaque and the dashes solid.
import { AbsoluteFill, Composition, registerRoot } from "@raea/motion";
function C() {
  return (
    <AbsoluteFill style={{ background: "#fff" }}>
      <svg width={300} height={200} viewBox="0 0 300 200">
        <line x1={0} y1={40} x2={300} y2={40} stroke="rgb(255,0,0)" strokeWidth={40} />
        <rect x={20} y={100} width={80} height={80} fill="rgb(0,0,255)" fillOpacity={0.5} />
        <line x1={150} y1={140} x2={300} y2={140} stroke="rgb(0,0,0)" strokeWidth={20} strokeDasharray="50 50" />
      </svg>
    </AbsoluteFill>
  );
}
registerRoot(() => <Composition id="svg" component={C} width={300} height={200} fps={30} durationInFrames={1} />);
