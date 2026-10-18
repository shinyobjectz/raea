<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="void/brand/logo/cat-white.svg">
    <img src="void/brand/logo/cat-black.svg" width="72" alt="raea's cat">
  </picture>
</p>

<h1 align="center">raea</h1>

<p align="center"><b>Render Anything Engine</b> <i>(for Anyone)</i></p>

<p align="center">
  One GPU engine for video, motion, games, 3D and documents, written as React and TypeScript,<br>
  rendered faster than it plays, on the device you already own.
</p>

<p align="center">
  <a href="https://raea.io">raea.io</a> ·
  <a href="https://raea.io/why/">Why we built it</a> ·
  <a href="https://raea.io/learn/">How it works</a> ·
  <a href="docs/settings.md">Settings</a> ·
  <a href="skills/raea/SKILL.md">Agent skill</a>
</p>

<p align="center">
  <img alt="licence FSL-1.1-ALv2" src="https://img.shields.io/badge/licence-FSL--1.1--ALv2-black">
  <img alt="status canary" src="https://img.shields.io/badge/status-canary-black">
  <img alt="macOS, Apple Silicon" src="https://img.shields.io/badge/macOS-Apple%20Silicon-black">
</p>

---

> **Canary.** raea is in active development and changes daily. It builds and renders on a Mac with Apple Silicon,
> and parts of it are rough. Expect breaking changes, missing docs, and things that only work the way we use them.

## What it is

raea renders compositions: React components that draw a frame for a given moment, written with `@raea/motion`.
The engine runs them inside [Bevy](https://bevyengine.org) on the GPU, composites video with fused compute kernels,
encodes in hardware, and keeps everything it drew as data you can query. The same file renders to an MP4, plays live
as a game, or renders one frame for an agent to look at.

```tsx
import { AbsoluteFill, Composition, interpolate, registerRoot, useCurrentFrame } from "@raea/motion";

function Hello() {
  const f = useCurrentFrame();
  return <AbsoluteFill style={{ background: "#000", color: "#fff", fontSize: 120, opacity: interpolate(f, [0, 20], [0, 1]) }}>Hello</AbsoluteFill>;
}

registerRoot(() => <Composition id="hello" component={Hello} width={1920} height={1080} fps={30} durationInFrames={90} />);
```

```sh
raea-motion render hello.tsx -o hello.mp4
```

**void** is the Mac app built on raea: a black window you talk to. Describe a reel, a promo or a game; its agent
writes the composition, renders it, checks the frames and the sound, and plays it on its stage.

## Get started

Requirements: macOS on Apple Silicon, [Rust](https://rustup.rs) (stable), [bun](https://bun.sh), Xcode command-line
tools.

```sh
git clone https://github.com/shinyobjectz/raea && cd raea

# the CLI: render, play, run and test compositions
cd react && cargo build --release --bin raea-motion && cd ..
react/target/release/raea-motion render react/bench/games/pong/edit.tsx -o pong.mp4

# void, the Mac app
cd void && cargo build --release && ./target/release/void
```

void's agent needs a model. Put a key in `~/.raea/env`; OpenRouter is the default, and Anthropic, Z.ai, DeepSeek,
Moonshot or any Anthropic-compatible endpoint work through `RAEA_LLM_PROVIDER`:

```sh
mkdir -p ~/.raea && echo "OPENROUTER_API_KEY=sk-or-..." >> ~/.raea/env
```

Everything else is optional; [docs/settings.md](docs/settings.md) lists every setting, and how to run the small API
behind generated music and speech yourself.

## Use it from your agent

- **Agent skill.** `skills/raea/SKILL.md` teaches any agent to write, render and check raea compositions.
  `skills/raea-bevy` covers 3D and games drawn by Bevy.
- **Claude Code plugin.** `/plugin marketplace add shinyobjectz/raea`, then `/plugin install raea@raea`.
- **MCP.** With void running, `claude mcp add --transport http void http://127.0.0.1:7457/mcp` gives an agent void's
  tools: `state`, `screenshot`, `send`, `act`, `wait`, `logs`, `hear`, `say`.

## What's in here

| Folder | What |
|---|---|
| [`engine/`](engine) | The core crate (`raea`): HTML and DOM rendering (Blitz, Stylo, Vello), compositing, fonts, the GPU |
| [`react/`](react) | `raea-motion`: runs `@raea/motion` compositions in Bevy; render, play, run, test; the iPhone bindings |
| [`motion/`](motion) | `@raea/motion`, the TypeScript SDK: its spec, conformance suite and source |
| [`rdb/`](rdb) | Loading and rendering an edit, its assets and generated sound; previz before render |
| [`kgpu/`](kgpu) | K on the GPU: a small array language compiled to WGSL, bit-exact with its interpreter |
| [`media/`](media) | Decode and encode behind one seam: VideoToolbox, AV1, a muxer |
| [`perception/`](perception) | Reading footage the way an editor does: faces, speech, shots, motion |
| [`meaning/`](meaning) | Frame embeddings and hyperdimensional computing over them |
| [`void/`](void) | void, the Mac app: the terminal, the stage, the agent, its MCP server |
| [`agent/`](agent) | The agent runtime and its multi-provider model layer |
| [`native/`](native), [`modules/`](modules) | The JavaScript engine's native calls and module loader |
| [`score/`](score), [`brief/`](brief) | A brief split into beats, and the notation an edit is planned in |
| [`browser/`](browser), [`shell/`](shell), [`data/`](data), [`cli/`](cli) | The agent's browser, shell, project record and part library |
| [`sandbox/`](sandbox), [`ios-probe/`](ios-probe), [`gputrace/`](gputrace) | The iPhone sandbox and probe, GPU telemetry |
| [`tabular/`](tabular) | Tabular models over what perception and embeddings measured |
| [`skills/`](skills) | Agent skills for working in raea |
| [`docs/`](docs) | Cross-cutting docs and [settings](docs/settings.md) |

Most folders carry a `.feature` file: the spec for that part, written as behaviour, with what was measured.

## Licence

raea is under the [Functional Source License 1.1, ALv2 Future License](LICENSE.md) (FSL-1.1-ALv2): use it, change
it and build on it for anything except a competing product or service. Each release becomes Apache 2.0 two years
after it ships. Third-party parts keep their own licences; see [NOTICE](NOTICE).
