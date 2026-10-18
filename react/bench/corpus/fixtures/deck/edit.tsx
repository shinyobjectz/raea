// DECK: a synthetic three-slide 16:9 deck for the corpus harness (react/bench/corpus/run.sh), written the way void's
// deck scenarios ask for one: a title slide, a three-column slide whose items stagger in, and a closing slide, each
// held in a Series with a cross-fade, plus a page number and a progress rule. No footage.
import { AbsoluteFill, Composition, Easing, Sequence, Series, interpolate, registerRoot, spring, useCurrentFrame, useVideoConfig } from "@raea/motion";

const W = 1920;
const H = 1080;
const FPS = 30;
const SLIDE = 90;

function Fade({ children }: { children: React.ReactNode }) {
  const f = useCurrentFrame();
  const o = interpolate(f, [0, 10, SLIDE - 10, SLIDE], [0, 1, 1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return <AbsoluteFill style={{ opacity: o }}>{children}</AbsoluteFill>;
}

function Title() {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const s = spring({ frame: f, fps, config: { damping: 16 } });
  return (
    <Fade>
      <div style={{ position: "absolute", left: 160, top: 360, fontFamily: "Helvetica", fontWeight: 800, fontSize: 140, color: "#1d1a16", transform: `translateY(${(1 - s) * 60}px)` }}>Crumb &amp; Crust</div>
      <div style={{ position: "absolute", left: 164, top: 540, fontFamily: "Georgia", fontStyle: "italic", fontSize: 56, color: "#8a6a2f" }}>an artisan bakery, since 1987</div>
      <div style={{ position: "absolute", left: 164, top: 640, width: interpolate(f, [8, 40], [0, 520], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: Easing.out(Easing.cubic) }), height: 6, background: "#c9a227" }} />
    </Fade>
  );
}

function Menu() {
  const f = useCurrentFrame();
  const items = [["Sourdough", "48-hour ferment"], ["Croissant", "French butter, 27 layers"], ["Rye", "Danish seeded loaf"]];
  return (
    <Fade>
      <div style={{ position: "absolute", left: 160, top: 140, fontFamily: "Helvetica", fontWeight: 700, fontSize: 84, color: "#1d1a16" }}>The menu</div>
      {items.map(([name, line], i) => {
        const d = Math.max(0, f - 8 - i * 8);
        const s = spring({ frame: d, fps: FPS, config: { damping: 13 } });
        return (
          <div key={i} style={{ position: "absolute", left: 160 + i * 540, top: 340, width: 480, height: 460, borderRadius: 24, background: "#fffaf0", boxShadow: "0 20px 50px rgba(60,40,10,0.18)", border: "2px solid #e9dcc0", transform: `translateY(${(1 - s) * 120}px)`, opacity: s }}>
            <div style={{ margin: 48, fontFamily: "Helvetica", fontWeight: 700, fontSize: 60, color: "#1d1a16" }}>{name}</div>
            <div style={{ marginLeft: 48, fontFamily: "Georgia", fontSize: 36, color: "#6b5a3a" }}>{line}</div>
          </div>
        );
      })}
    </Fade>
  );
}

function Visit() {
  const f = useCurrentFrame();
  const o = interpolate(f, [6, 24], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <Fade>
      <AbsoluteFill style={{ background: "linear-gradient(160deg, #2a2118, #4a3a26)" }} />
      <div style={{ position: "absolute", width: W, top: 380, textAlign: "center", fontFamily: "Helvetica", fontWeight: 800, fontSize: 120, color: "#f6eedd" }}>Visit us</div>
      <div style={{ position: "absolute", width: W, top: 560, textAlign: "center", fontFamily: "Georgia", fontSize: 54, color: "#e3c77a", opacity: o }}>12 Mill Lane · open 7 to 3</div>
    </Fade>
  );
}

function Chrome() {
  const f = useCurrentFrame();
  const page = Math.min(3, Math.floor(f / SLIDE) + 1);
  return (
    <>
      <div style={{ position: "absolute", right: 80, bottom: 56, fontFamily: "Helvetica", fontSize: 32, color: "#9a8a6a" }}>{page} / 3</div>
      <div style={{ position: "absolute", left: 0, bottom: 0, height: 8, width: (W * (f + 1)) / (SLIDE * 3), background: "#c9a227" }} />
    </>
  );
}

function Deck() {
  return (
    <AbsoluteFill style={{ background: "#f4ecdc" }}>
      <Series>
        <Series.Sequence durationInFrames={SLIDE}><Title /></Series.Sequence>
        <Series.Sequence durationInFrames={SLIDE}><Menu /></Series.Sequence>
        <Series.Sequence durationInFrames={SLIDE}><Visit /></Series.Sequence>
      </Series>
      <Sequence from={0} durationInFrames={SLIDE * 3} layout="none"><Chrome /></Sequence>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="deck" component={Deck} width={W} height={H} fps={FPS} durationInFrames={SLIDE * 3} />);
