---
name: raea
description: Make video, motion graphics, games and 3D scenes with raea, the Render Anything Engine, by writing React/TypeScript compositions with @raea/motion and rendering them on the GPU with the raea-motion CLI (or playing games live). Use when the user wants an agent to produce a rendered video or animation from code, cut footage by its words, add captions or motion graphics, build a small playable game, render frames headlessly, or drive the void app through its MCP server.
---

# raea

raea renders compositions written in TypeScript and React (`@raea/motion`) with a GPU engine built on Bevy. A
composition is a function of the frame: give it frame N and it draws frame N. The same file renders to video, plays
live as a game, or renders single frames for checking.

## Set up (once)

1. macOS on Apple Silicon, Rust (stable), and bun.
2. Build the CLI from the repository root: `cd react && cargo build --release --bin raea-motion`. The binary is
   `react/target/release/raea-motion`; put it on `PATH` or call it by path.
3. Optional settings live in `~/.raea/env` (see `docs/settings.md`). Rendering needs none of them.

## Write a composition

One `.tsx` file whose root registers compositions:

```tsx
import { AbsoluteFill, Composition, Sequence, interpolate, registerRoot, useCurrentFrame } from "@raea/motion";

function Card({ text }: { text: string }) {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "#000", color: "#fff", justifyContent: "center", alignItems: "center", fontSize: 96 }}>
      <span style={{ opacity: interpolate(f, [0, 12], [0, 1], { extrapolateRight: "clamp" }) }}>{text}</span>
    </AbsoluteFill>
  );
}

function Piece() {
  return (
    <>
      <Sequence from={0} durationInFrames={45}><Card text="One" /></Sequence>
      <Sequence from={45} durationInFrames={45}><Card text="Two" /></Sequence>
    </>
  );
}

registerRoot(() => <Composition id="piece" component={Piece} width={1080} height={1920} fps={30} durationInFrames={90} />);
```

Rules that keep renders right:
- Everything that moves reads `useCurrentFrame()`; never timers, `Date.now()` or `Math.random()` (deterministic
  frames are what make renders repeatable and checkable).
- Footage: `<Video src=...>` and `<Audio src=...>`; time cues to speech with `word("phrase")` when a transcript
  (`--words W.json`) is given, so cues follow the words if the cut moves.
- Games: keep state in `useGameState(initial, step)`, where `step(state, input, frame)` is pure; read keys with
  `useInput()`. The game plays live and renders from a logged play to the same pixels.
- 3D: `@raea/motion/3d` (Canvas3D, Mesh, lights, cameras). Read `skills/raea-bevy/SKILL.md` first.

## Render and check

```sh
raea-motion render piece.tsx -o piece.mp4                 # the whole composition
raea-motion render piece.tsx --frame 60 -o frame60.png    # one frame, to look at
raea-motion render piece.tsx --at 0,45,89 -o frames/      # a few frames
raea-motion play game.tsx --input                          # play a game live
```

Look at the frames you rendered before you call the work done: check text fits, nothing overlaps, and cues land
where the transcript says. Fix the composition and render again.

## Go further

- Writing guides (pacing, captions, layout, sound, games, 3D): `void/prompts/craft/motion.md` and
  `void/prompts/craft/motion/*.md`.
- The SDK's behaviour: `motion/spec/motion.feature`; what 3D reaches: `motion/docs/bevy-coverage.md`.
- void, the Mac app: talk to it, or let an agent drive it. With void running, add its MCP server:
  `claude mcp add --transport http void http://127.0.0.1:7457/mcp` (tools: state, screenshot, send, act, wait, logs,
  hear, say).
