<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="apps/void/brand/logo/cat-white.svg">
    <img src="apps/void/brand/logo/cat-black.svg" width="72" alt="raea's cat">
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
  <a href="sdk/skills/raea/SKILL.md">Agent skill</a>
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

# the CLI: render, play, run and test compositions (one Cargo workspace, one target/)
cargo build --release -p raea-react --bin raea-motion
target/release/raea-motion render sdk/react/bench/games/pong/edit.tsx -o pong.mp4

# void, the Mac app
cargo build --release -p raea-void && target/release/void
```

void's agent needs a model. Put a key in `~/.raea/env`; OpenRouter is the default, and Anthropic, Z.ai, DeepSeek,
Moonshot or any Anthropic-compatible endpoint work through `RAEA_LLM_PROVIDER`:

```sh
mkdir -p ~/.raea && echo "OPENROUTER_API_KEY=sk-or-..." >> ~/.raea/env
```

Everything else is optional; [docs/settings.md](docs/settings.md) lists every setting, and how to run the small API
behind generated music and speech yourself.

## Use it from your agent

- **Agent skill.** `sdk/skills/raea/SKILL.md` teaches any agent to write, render and check raea compositions.
  `sdk/skills/raea-bevy` covers 3D and games drawn by Bevy.
- **Claude Code plugin.** `/plugin marketplace add shinyobjectz/raea`, then `/plugin install raea@raea`.
- **MCP.** With void running, `claude mcp add --transport http void http://127.0.0.1:7457/mcp` gives an agent void's
  tools: `state`, `screenshot`, `send`, `act`, `wait`, `logs`, `hear`, `say`.

## What's in here

| Folder | What |
|---|---|
| [`engine/html/`](engine/html) | The core crate (`raea`): HTML and CSS rendering without a browser (Blitz, Stylo, Vello), the compositor, fonts, a C API |
| [`engine/rdb/`](engine/rdb) | An edit as typed tables on Bevy's World, and the Bevy renderer that draws it |
| [`engine/kgpu/`](engine/kgpu) | K on the GPU: a small array language compiled to WGSL, bit-exact with its interpreter |
| [`engine/media/`](engine/media) | Decode and encode behind one seam: VideoToolbox, software codecs, AV1, a muxer |
| [`engine/mixer/`](engine/mixer), [`engine/seedling/`](engine/seedling) | One mixer core for live play and offline render, and bevy_seedling on it |
| [`engine/animate/`](engine/animate), [`engine/bevy-plugins/`](engine/bevy-plugins) | Frame-pure animation (easing, tweens, springs), and Bevy plugins compiled into the engine |
| [`engine/native/`](engine/native), [`engine/modules/`](engine/modules) | The native-call layer on JavaScriptCore, and the in-process TypeScript loader |
| [`engine/browser/`](engine/browser), [`engine/bundle/`](engine/bundle) | The agent's browser, and the in-process TypeScript bundler (Rolldown) |
| [`engine/sandbox/`](engine/sandbox) | A WASI sandbox for command-line tools on the iPhone (experimental) |
| [`engine/gputrace/`](engine/gputrace) | GPU telemetry |
| [`sdk/motion/`](sdk/motion) | `@raea/motion`, the TypeScript SDK: its spec, conformance suite and source |
| [`sdk/react/`](sdk/react) | `raea-motion`: runs `@raea/motion` compositions in Bevy; render, play, run, test; the iPhone build |
| [`sdk/shell/`](sdk/shell), [`sdk/data/`](sdk/data) | The agent's shell; a project's record, and a brief's facts and checks, as data |
| [`sdk/brief/`](sdk/brief) | The `.brief` plan format |
| [`sdk/skills/`](sdk/skills) | Agent skills for working in raea |
| [`apps/void/`](apps/void) | void, the Mac app: the window, the stage, the agent, voice, its MCP server, evals |
| [`apps/cli/`](apps/cli) | The agent's `raea` command |
| [`ai/agent/`](ai/agent) | The agent loop, its multi-provider model layer, a Gherkin parser, traces |
| [`ai/perception/`](ai/perception), [`ai/meaning/`](ai/meaning) | Reading footage the way an editor does; frame embeddings (SigLIP 2) as columns, searched with K kernels |
| [`ai/ml/`](ai/ml), [`ai/score/`](ai/score) | Models on ONNX Runtime; the compiler from a `.brief` to a composition |
| [`tools/`](tools) | Build helpers: the JIT-signing linker wrapper, setup scripts |
| [`docs/`](docs) | Cross-cutting docs and [settings](docs/settings.md) |

Each folder has a README saying what it is and what it depends on. Behaviour specs written in Gherkin (`.feature`)
sit beside the code they specify.

## Licence

raea is under the [Functional Source License 1.1, ALv2 Future License](LICENSE) (FSL-1.1-ALv2), copyright Shane
Murphy: use it, change it, build on it and pass it on for anything except a product that competes with raea. Each
version becomes Apache-2.0 two years after its release. Third-party parts keep their own licences; see [NOTICE](NOTICE).
