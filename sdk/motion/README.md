# @raea/motion

raea's TypeScript SDK for video, motion and games: React components and hooks for time, rendered by raea's engine
(Bevy, with a JavaScript runtime inside it) instead of a browser. Its API is familiar to anyone who has written React
video, so agents write it fluently, and it is raea's own work, built from its own written spec.

```tsx
import { AbsoluteFill, Composition, interpolate, registerRoot, useCurrentFrame } from "@raea/motion";

function Title() {
  const frame = useCurrentFrame();
  const opacity = interpolate(frame, [0, 20], [0, 1], { extrapolateRight: "clamp" });
  return <AbsoluteFill style={{ background: "#000", color: "#fff", fontSize: 120, opacity }}>Hello</AbsoluteFill>;
}

registerRoot(() => <Composition id="title" component={Title} width={1920} height={1080} fps={30} durationInFrames={90} />);
```

```sh
raea-motion render title.tsx -o title.mp4
```

## What raea adds

- Time anchored to words in a transcript (`word()`, `findPhrase()`), so cues follow the speech when the cut moves.
- Games as a pure step from input to state (`useGameState`, `useInput`, `useGameEvents`): played live, rendered from a
  logged play, the same pixels.
- 3D and an ECS world in the same tree, drawn by Bevy (`@raea/motion/3d`, `/ecs`); see skills/raea-bevy.
- Sound made on your Mac as values: `Tone.tokensynth()` instruments for music and `sao()` foley, cached by prompt,
  seed and settings; `raea-motion sounds` fetches open sample libraries for a Tone.js `Sampler`.
- The same code renders on the Mac and the iPhone.

## Layout

- `spec/motion.feature`: the behavioural spec.
- `conformance/`: tests written from the spec (`bun test conformance`); `raea-motion test FILE...` runs them on the
  engine's own runtime.
- `src/`: the SDK. `src/native.ts`, `ecs.ts`, `solid.ts` and `codecs.ts` are the engine modules (`/native`, `/ecs`,
  `/3d`'s `manifold` and `Solid`, `/codecs`). `/codecs` wraps mediabunny (MPL-2.0, unmodified) with the engine's
  coders underneath; Manifold runs natively as manifold-rust (Apache-2.0). Under plain bun, `/ecs` falls back to
  TypeScript tables and the others say they need the engine.
- `docs/bevy-coverage.md`: which of Bevy's features a composition can reach, and what each missing one needs.
  `bench/bevy/` holds the 3D fixtures and their render-against-play check.
- `tools/`: golden-frame checks (`golden.sh`, `golden-av.sh`) and `media-vs-ffmpeg.sh`.

void's agent learns the SDK from `apps/void/prompts/craft/motion.md` and `motion/*.md`, its guide for writing compositions.
