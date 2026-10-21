---
name: raea
description: Make video, motion graphics, games and 3D scenes with raea, the Render Anything Engine, by writing React/TypeScript compositions with @raea/motion and rendering them on the GPU with the raea-motion CLI (or playing games live). Use when the user wants an agent to produce a rendered video or animation from code, cut footage by its words, add captions or motion graphics, build a small playable game, render frames headlessly, or drive the void app through its MCP server.
---

# raea

raea renders compositions written in TypeScript and React (`@raea/motion`) with a GPU engine built on Bevy. A
composition is a function of the frame: give it frame N and it draws frame N. The same file renders to video, plays
live as a game, or renders single frames for checking.

## Set up (once)

1. macOS on Apple Silicon. Install (or update) raea: `curl -fsSL https://raea.io/install | sh`. It puts `raea-motion`
   in `~/.raea/bin`, links it from `~/.local/bin` (make sure that's on `PATH`), and adds this skill and raea-bevy to
   `~/.claude/skills`. Check it with `raea-motion render` (it prints its usage).
2. From a checkout of the repository instead: `cd react && cargo build --release --bin raea-motion` (Rust stable);
   the binary is `sdk/react/target/release/raea-motion`.
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
- 3D: `@raea/motion/3d` (Canvas3D, Mesh, lights, cameras). Read the raea-bevy skill (`sdk/skills/raea-bevy/SKILL.md`) first.
- 3D in footage (matchmove): `raea-motion track clip.mp4 --subject X,Y,W,H` solves the clip's camera once (COLMAP,
  kept in the asset store; check the APPLIED line's registered frames and error, and its notes for a rotation-only
  fallback). Then `<Matchmove src={staticFile("clip.mp4")} subject={[x, y, w, h]} crop={0.4}>` with 3D children draws
  the footage, follows its camera and keeps the subject in front. Its world is metres, ground at y = 0, frame 0 looking
  down -Z; `crop` places a 9:16 window on 16:9 footage.
- Sound: music in Tone.js (`import * as Tone from "@raea/motion/tone"`): instruments in a `Tone.Sampler` playing a
  `Part` or `Sequence`, their notes from `Tone.tokensynth("a warm upright bass", ["C2", "C3", "C4"])` (TokenSynth, on
  this Mac) or from an open sample library (`raea-motion sounds`); foley with `sao("prompt", { seconds })` in an
  `<Audio>` (Stable Audio Open, on this Mac); narration with `fish("words")`. One `<Mix lufs={-14} />` masters it.
  Both local models install themselves on first use. No generated images or video.
- Music SOP: a library instrument first; a TokenSynth instrument is curated, not trusted (several seeds a root, roots
  3-4 semitones apart, `tools/tokensynth/curate.py` picks the clean takes); under a voice, write a sparse drone score,
  not genre pastiche. Read `apps/void/prompts/craft/motion/music.md` ("Instruments from TokenSynth", "Writing music for
  picture") before scoring anything.

- Music-led pieces: before cutting anything to a song, run `raea-motion song FILE --plot FILE.song.png` and look at
  the plot. Time every cut, slam and move from the map's beats, downbeats and events (`useSong`, `song(map).cutsAt()`),
  never from a constant BPM: a tempo times bar count drifts off the real beats within a minute, and it can't see a
  build, a stop or a drop.
## Render and check

```sh
raea-motion render piece.tsx -o piece.mp4                 # the whole composition
raea-motion render piece.tsx --frame 60 -o frame60.png    # one frame, to look at
raea-motion render piece.tsx --at 0,45,89 -o frames/      # a few frames
raea-motion play game.tsx --input                          # play a game live
```

Sound from the command line, through the same cache the composition's render uses (run them in the composition's
folder, or pass `--dir`):

```sh
raea-motion sao "deep punchy impact hit, tight" --seconds 1.2 -o hit.wav     # foley (Stable Audio Open)
raea-motion tokensynth "a soft wooden marimba" --notes C3,C4,C5 -o marimba/   # an instrument's notes (TokenSynth)
raea-motion sounds piano                                                       # open sample libraries
raea-motion sounds get salamander/piano --notes C3,C4,C5                       # into media/sounds/, with its Sampler
raea-motion setup-audio                                                        # install both local models now
```

Look at the frames you rendered before you call the work done: check text fits, nothing overlaps, and cues land
where the transcript says. Fix the composition and render again.

raea-motion song song.wav --plot song.song.png                                 # the song's energy map (song.song.json)
## Go further

To cut to a song, map it first: `raea-motion song` finds beats, bars, sections (the file's `[Section]` lyrics markers
when it has them), builds, drops, stops (the beat dropping out, with its hits), hits and dips, and a cut rate per
section. Look at the plot, then pace with it: `import map from "./song.song.json"`, `useSong(map)` for the frame's
beat, energy, section, `inBuild`/`buildProgress` and `untilDrop`, and `song(map, { fps }).cutsAt()` for the frames to
cut on. Hold through low sections, cut faster through a build, smash-cut on the drop, cut on a stop's hits.

Paths here are relative to the root of the raea repository.

- Writing guides (pacing, captions, layout, sound, games, 3D): `apps/void/prompts/craft/motion.md` and
  `void/prompts/craft/motion/*.md`.
- The SDK's behaviour: `sdk/motion/spec/motion.feature`; what 3D reaches: `sdk/motion/docs/bevy-coverage.md`.
- void, the Mac app: talk to it, or let an agent drive it. With void running, add its MCP server:
  `claude mcp add --transport http void http://127.0.0.1:7457/mcp` (tools: state, screenshot, send, act, wait, logs,
  hear, say).
