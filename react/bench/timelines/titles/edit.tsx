// TITLES: a 20 s motion-graphics timeline with no footage, the bake harness's fixture (react/bench/timelines/check.sh):
// headlines that slide and spring in, a counter, gradient panels, a progress bar, cards in a grid, a blurred glow. It
// bakes, plays from its bake with no JS, and renders: the three must agree.
import { AbsoluteFill, Composition, Easing, Sequence, interpolate, registerRoot, spring, useCurrentFrame, useVideoConfig } from "@raea/motion";

const W = 1080;
const H = 1920;
const FPS = 30;

function Headline({ text, y, color }: { text: string; y: number; color: string }) {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const s = spring({ frame: f, fps, config: { damping: 14, stiffness: 120 } });
  const x = interpolate(s, [0, 1], [-W, 0]);
  const o = interpolate(f, [0, 12], [0, 1], { extrapolateRight: "clamp" });
  return (
    <div style={{ position: "absolute", left: 80 + x, top: y, width: W - 160, fontFamily: "Helvetica", fontWeight: 800, fontSize: 112, lineHeight: 1.05, color, opacity: o, letterSpacing: "-0.02em" }}>
      {text}
    </div>
  );
}

function Counter({ to }: { to: number }) {
  const f = useCurrentFrame();
  const n = Math.round(interpolate(f, [0, 60], [0, to], { extrapolateRight: "clamp", easing: Easing.out(Easing.cubic) }));
  return (
    <div style={{ position: "absolute", left: 80, top: 760, fontFamily: "Helvetica", fontWeight: 700, fontSize: 260, color: "#ffd23f", fontVariantNumeric: "tabular-nums" }}>
      {n.toLocaleString("en-US")}
    </div>
  );
}

function Cards() {
  const f = useCurrentFrame();
  return (
    <>
      {Array.from({ length: 9 }, (_, i) => {
        const d = Math.max(0, f - i * 4);
        const s = spring({ frame: d, fps: FPS, config: { damping: 12 } });
        const col = i % 3;
        const row = Math.floor(i / 3);
        return (
          <div
            key={i}
            style={{
              position: "absolute", left: 80 + col * 320, top: 1100 + row * 240, width: 280, height: 200,
              borderRadius: 28, background: `linear-gradient(135deg, hsl(${200 + i * 15}, 70%, 55%), hsl(${260 + i * 10}, 60%, 35%))`,
              transform: `scale(${s}) rotate(${(1 - s) * 12}deg)`, boxShadow: "0 18px 40px rgba(0,0,0,0.35)",
            }}
          />
        );
      })}
    </>
  );
}

function Titles() {
  const f = useCurrentFrame();
  const progress = f / (20 * FPS);
  const glow = 0.5 + 0.5 * Math.sin(f / 20);
  return (
    <AbsoluteFill style={{ background: "linear-gradient(180deg, #0f1226 0%, #1d2350 100%)" }}>
      <div style={{ position: "absolute", left: 200, top: 300, width: 680, height: 680, borderRadius: 340, background: "#4b5cff", opacity: 0.25 + 0.2 * glow, filter: "blur(80px)" }} />
      <Sequence from={0} durationInFrames={210}>
        <Headline text="One engine." y={300} color="#f4f1ea" />
      </Sequence>
      <Sequence from={45} durationInFrames={165}>
        <Headline text="No V8." y={460} color="#82c7a2" />
      </Sequence>
      <Sequence from={90}>
        <Counter to={36000} />
      </Sequence>
      <Sequence from={240}>
        <Headline text="Every frame baked." y={300} color="#f4f1ea" />
        <Cards />
      </Sequence>
      <Sequence from={450}>
        <Headline text="Played as rendered." y={1700} color="#ffd23f" />
      </Sequence>
      <div style={{ position: "absolute", left: 80, bottom: 80, width: W - 160, height: 14, borderRadius: 7, background: "rgba(255,255,255,0.15)" }}>
        <div style={{ width: `${progress * 100}%`, height: "100%", borderRadius: 7, background: "#82c7a2" }} />
      </div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="edit" component={Titles} width={W} height={H} fps={FPS} durationInFrames={20 * FPS} />);
