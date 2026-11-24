import { AbsoluteFill, Composition, interpolate, registerRoot, useCurrentFrame } from "raea";

// docs/nobuild.feature NB4a: a 2D card (type, a gradient, a rounded box, a border) that tools/nobuild.sh catalog draws by
// raea with the plugin catalog compiled in and by raea without it, which must give the same pixels.
function Card() {
  const f = useCurrentFrame();
  const x = interpolate(f, [0, 24], [-40, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <AbsoluteFill style={{ background: "linear-gradient(135deg, #0f172a, #1e3a8a)", justifyContent: "center", alignItems: "center" }}>
      <div style={{ transform: `translateX(${x}px)`, padding: "24px 36px", borderRadius: 18, border: "3px solid #fbbf24", background: "rgba(255,255,255,0.08)", color: "#f8fafc", fontSize: 44, fontWeight: 700 }}>
        Same pixels
        <div style={{ fontSize: 20, fontWeight: 400, opacity: 0.8 }}>with every plugin compiled in</div>
      </div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="card" component={Card} width={480} height={270} fps={30} durationInFrames={30} />);
