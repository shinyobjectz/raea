// A JetFlow-style 3D UI scene (engine/html/docs/bevy.feature B4): UI cards are DOM on surfaces in a lit 3D scene, with
// meshes and a headline in 3D. Lowered into an edit by raea-react (src/lower.rs); motion is the edit's rules.
const card = (accent: string): React.CSSProperties => ({
  width: 1024, height: 640, boxSizing: "border-box", padding: 56, borderRadius: 48,
  background: "rgba(18, 20, 38, 0.92)", color: "#f4f1ea", fontFamily: "Helvetica, Arial, sans-serif",
  border: `6px solid ${accent}`, display: "flex", flexDirection: "column", gap: 24,
});

export function Scene() {
  return (
    <>
      <camera id="cam" x={0} y={1.6} z={9} lookX={0} lookY={1.4} lookZ={0} bg="#0b0c1a" />
      <light id="sun" lux={9000} dirX={-0.4} dirY={-1} dirZ={-0.5} />
      <mesh id="floor" shape="plane" size={30} color="#1d1b33" />
      <mesh id="gem" shape="cube" size={0.9} color="#ff7a59" x={-3.4} y={0.6} z={-1} rough={0.3} />
      <mesh id="orb" shape="sphere" size={1.1} color="#7c6cff" x={3.5} y={0.8} z={-1.5} rough={0.2} metal={0.4} />
      <text3d id="headline" text="Fly with JetFlow" w={6} px={2048} pxH={320} x={0} y={4.1} z={-1} color="#ffd8c2" />
      <surface id="flight" w={3.2} px={1024} pxH={640} x={-1.8} y={1.9} z={0.2} rotY={0.25}>
        <div style={card("#ff7a59")}>
          <div style={{ fontSize: 40, opacity: 0.7 }}>Next flight</div>
          <div style={{ fontSize: 96, fontWeight: 800 }}>SFO → JFK</div>
          <div style={{ fontSize: 44 }}>Boards 7:40 · Gate B12 · Seat 3A</div>
          <div style={{ marginTop: "auto", alignSelf: "flex-start", padding: "18px 40px", borderRadius: 999, background: "#ff7a59", color: "#1a1020", fontSize: 40, fontWeight: 700 }}>Check in</div>
        </div>
      </surface>
      <surface id="fare" w={2.6} px={1024} pxH={640} x={1.9} y={1.7} z={0.6} rotY={-0.3}>
        <div style={card("#7c6cff")}>
          <div style={{ fontSize: 40, opacity: 0.7 }}>Fare watch</div>
          <div style={{ fontSize: 120, fontWeight: 800 }}>$248</div>
          <div style={{ fontSize: 44 }}>Down $61 since Monday</div>
        </div>
      </surface>
    </>
  );
}
