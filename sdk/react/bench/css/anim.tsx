// CSS animations and transitions in a composition (sdk/react/bench/css/check.sh): A runs from 0, B from its <Sequence>s
// start (frame 10), C transitions when its style changes at frame 1. A frame alone and the same frame in a render agree
// for A and B.
import { AbsoluteFill, Composition, Sequence, registerRoot, useCurrentFrame } from "@raea/motion";
const CSS = `@keyframes slide { from { transform: translateX(0px) } to { transform: translateX(600px) } }
.k { animation: slide 1s linear forwards }
.t { transition: transform 1s linear }`;
function C() {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "#fff" }}>
      <style>{CSS}</style>
      <div className="k" style={{ position: "absolute", left: 0, top: 0, width: 50, height: 50, background: "rgb(255,0,0)" }} />
      <Sequence from={10}>
        <div className="k" style={{ position: "absolute", left: 0, top: 100, width: 50, height: 50, background: "rgb(0,0,255)" }} />
      </Sequence>
      <div className="t" style={{ position: "absolute", left: 0, top: 200, width: 50, height: 50, background: "rgb(0,160,0)", transform: f > 0 ? "translateX(600px)" : "translateX(0px)" }} />
    </AbsoluteFill>
  );
}
registerRoot(() => <Composition id="css" component={C} width={700} height={260} fps={30} durationInFrames={40} />);
