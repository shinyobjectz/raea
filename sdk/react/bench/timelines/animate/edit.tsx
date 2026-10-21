// ANIMATE: a 20 s caption and badge sequence written only with declared animation (<Animate>, <Stagger>, moves,
// tracks, a timeline), no interpolate and no useCurrentFrame: the engine samples every value from the frame clock
// (raea-animate). The caption's words are a flex row: inline-block chips whose box changes replay taller in a baked
// play-through than in a render, with or without <Animate> (a Blitz incremental-layout bug, open). The bake harness's fixture for it (sdk/react/bench/timelines/check.sh animate): render, baked play and
// seeks must agree, and the incremental re-bake retunes the card's spring below.
import { useMemo } from "react";
import {
  AbsoluteFill, Animate, Composition, Sequence, Stagger, curve, fade, fill, parallel, pop, registerRoot, rise, sequence,
  slide, tick, timeline, track, wipe,
} from "@raea/motion";

const FPS = 30;
const INK = "#14121f";
const PAPER = "#fbf7ef";
const ACCENT = "#ff5a7a";

// the captions: three-word groups, each word lit while it's said (frames are the group's own)
const GROUPS: { from: number; words: [string, number, number][] }[] = [
  { from: 0, words: [["Your", 0, 8], ["afternoon", 8, 22], ["slump", 22, 40]] },
  { from: 44, words: [["has", 0, 6], ["a", 6, 10], ["new", 10, 20]] },
  { from: 70, words: [["enemy.", 0, 30]] },
  { from: 110, words: [["Steeped", 0, 14], ["twenty", 14, 26], ["hours,", 26, 46]] },
  { from: 160, words: [["smooth,", 0, 18], ["never", 18, 28], ["bitter.", 28, 52]] },
  { from: 220, words: [["Twelve", 0, 12], ["grams", 12, 22], ["protein,", 22, 44]] },
  { from: 270, words: [["zero", 0, 12], ["added", 12, 22], ["sugar.", 22, 48]] },
  { from: 330, words: [["Crack", 0, 12], ["one", 12, 18], ["open.", 18, 44]] },
  { from: 390, words: [["Finish", 0, 14], ["the", 14, 18], ["list.", 18, 46]] },
  { from: 450, words: [["First", 0, 12], ["case", 12, 22], ["half", 22, 30]] },
  { from: 486, words: [["off.", 0, 40]] },
];

function Caption({ words, length }: { words: [string, number, number][]; length: number }) {
  return useMemo(() => (
    <AbsoluteFill style={{ justifyContent: "flex-end", alignItems: "center", paddingBottom: 220 }}>
      <Animate enter={pop({ from: 0.92, peak: 1.03 })} exit="fade" exitAt={length - 6} style={{ display: "flex", backgroundColor: PAPER, borderRadius: 32, padding: "18px 28px", fontFamily: "Helvetica", fontWeight: 900, fontSize: 64, color: INK }}>
        {words.map(([w, a, b], i) => (
          <Animate
            key={i}
            as="span"
            animate={{ backgroundColor: track({ 0: "rgba(255,90,122,0)", [a]: "rgba(255,90,122,0)", [a + 3]: ACCENT, [b]: ACCENT, [b + 3]: "rgba(255,90,122,0)" }, "out-cubic") }}
            style={{ padding: "2px 12px", margin: "0 2px", borderRadius: 999 }}
          >
            {w}
          </Animate>
        ))}
      </Animate>
    </AbsoluteFill>
  ), [words, length]);
}

// the badges: a tray of proof chips that slide in one after another and leave together
const chip = { display: "inline-flex", alignItems: "center", gap: 14, backgroundColor: INK, color: PAPER, borderRadius: 999, padding: "12px 30px 12px 14px", fontFamily: "Helvetica", fontWeight: 900, fontSize: 48, marginBottom: 14 } as const;
const num = { minWidth: 120, textAlign: "center", backgroundColor: ACCENT, color: INK, borderRadius: 999, padding: "4px 18px" } as const;

const BADGES = (
    <Stagger each={24} at={110} enter={slide({ distance: 64, frames: 8 })} exit={slide({ distance: 64, frames: 8 })} exitAt={430} exitEach={2} style={{ position: "absolute", left: 56, top: 120, display: "flex", flexDirection: "column", alignItems: "flex-start" }}>
      <div style={chip}><Animate as="span" enter={tick({ to: 20, frames: 30 })} at={124} style={num} /> HRS STEEPED</div>
      <div style={chip}><span style={num}>12</span> G PROTEIN</div>
      <div style={chip}><span style={num}>0</span> G SUGAR</div>
    </Stagger>
);
function Badges() {
  return BADGES;
}

// the card: a timeline of parts, landing on its spring
const plan = timeline(
  sequence(
    ["card", [rise({ distance: 80, frames: 10 }), { tracks: { scale: track({ 0: 0.86, 18: 1 }, curve.spring({ damping: 15, stiffness: 120 })) }, frames: 18 }]],
    parallel(["title", pop()], ["bar", wipe({ frames: 12 })], { part: "cta", move: fade(), offset: 6 }),
  ),
);

const CARD = (
    <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", paddingBottom: 200 }}>
      <Animate timeline={plan} part="card" style={{ width: 860, backgroundColor: PAPER, borderRadius: 44, padding: 52, display: "flex", flexDirection: "column", gap: 26 }}>
        <Animate timeline={plan} part="title" style={{ fontFamily: "Helvetica", fontWeight: 900, fontSize: 96, color: INK, lineHeight: 1 }}>First case half off</Animate>
        <div style={{ height: 26, borderRadius: 13, backgroundColor: "#e9e2d6", overflow: "hidden" }}>
          <Animate enter={fill({ frames: 30 })} at={20} style={{ height: 26, borderRadius: 13, backgroundColor: ACCENT }} />
        </div>
        <Animate timeline={plan} part="bar" style={{ height: 8, backgroundColor: INK }} />
        <Animate timeline={plan} part="cta" style={{ fontFamily: "Helvetica", fontSize: 44, color: INK }}>brightside.coffee</Animate>
      </Animate>
    </AbsoluteFill>
);
function Card() {
  return CARD;
}

// declared once: nothing in this tree depends on the frame (the engine samples every value), so the whole edit is one
// element made at load, and a frame builds nothing
const EDIT = (
  <AbsoluteFill style={{ backgroundColor: "#3b2f5c" }}>
    <AbsoluteFill style={{ background: "linear-gradient(160deg, #3b2f5c, #1d1730)" }} />
    <Badges />
    <Sequence from={450} durationInFrames={150} name="card">
      <Card />
    </Sequence>
    {GROUPS.map((g, i) => {
      const length = (GROUPS[i + 1]?.from ?? 600) - g.from;
      return (
        <Sequence key={i} from={g.from} durationInFrames={length} name={`caption ${i + 1}`}>
          <Caption words={g.words} length={length} />
        </Sequence>
      );
    })}
  </AbsoluteFill>
);

export default function Edit() {
  return EDIT;
}

registerRoot(() => <Composition id="animate" component={Edit} width={1080} height={1920} fps={FPS} durationInFrames={600} />);
