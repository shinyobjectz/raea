<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/logo/cat-white.svg">
    <img src="docs/logo/cat-black.svg" width="72" alt="raea's cat">
  </picture>
</p>

<h1 align="center">raea</h1>

<p align="center"><b>Render Anything Engine</b> <i>(for Anyone)</i></p>

<p align="center">
  Tell your AI what to make. Videos, motion graphics, 3D and games, written as React and TypeScript<br>
  (the code your AI already writes) and rendered on your own GPU, faster than they play.
</p>

<p align="center">
  <a href="https://raea.io">raea.io</a> ·
  <a href="https://raea.io/made/">Made in raea</a> ·
  <a href="https://raea.io/why/">Why raea</a> ·
  <a href="https://raea.io/learn/">Docs</a> ·
  <a href="https://github.com/shinyobjectz/raea/releases">Releases</a>
</p>

<p align="center">
  <img alt="licence FSL-1.1-ALv2" src="https://img.shields.io/badge/licence-FSL--1.1--ALv2-black">
  <img alt="status canary" src="https://img.shields.io/badge/status-canary-black">
  <img alt="macOS, Apple Silicon" src="https://img.shields.io/badge/macOS-Apple%20Silicon-black">
  <img alt="Linux x86_64" src="https://img.shields.io/badge/Linux-x86__64-black">
  <img alt="Windows x86_64" src="https://img.shields.io/badge/Windows-x86__64-black">
</p>

---

> **Canary.** raea changes daily. Releases run on macOS (Apple Silicon), Linux (x86_64) and, new and least tested,
> Windows (x86_64). Expect breaking changes, rough edges and gaps in the docs.

## Install

```sh
curl -fsSL https://raea.io/install | sh
```

The installer puts the `raea` command under `~/.raea`, links it into `~/.local/bin`, and adds raea's agent skills to
`~/.claude/skills`, so Claude Code (or any agent that reads that folder) knows how to write, render and check raea
work. Run it again to update. The engine is free and needs no account. Tarballs and checksums are on the
[releases page](https://github.com/shinyobjectz/raea/releases).

On Windows 10 or 11, in PowerShell:

```powershell
irm https://raea.io/install.ps1 | iex
```

It does the same under `~\.raea` and adds raea to your PATH. raea needs the Microsoft Visual C++ Redistributable
(x64), which most machines already have, and a DirectX 12 or Vulkan GPU driver.

## Hello

A composition is a React component that draws a frame for a given moment:

```tsx
import { AbsoluteFill, Composition, interpolate, registerRoot, useCurrentFrame } from "raea";

function Hello() {
  const f = useCurrentFrame();
  return <AbsoluteFill style={{ background: "#000", color: "#fff", fontSize: 120, opacity: interpolate(f, [0, 20], [0, 1]) }}>Hello</AbsoluteFill>;
}

registerRoot(() => <Composition id="hello" component={Hello} width={1920} height={1080} fps={30} durationInFrames={90} />);
```

```sh
raea render hello.tsx -o hello.mp4
```

On an Apple Silicon Mac those 90 frames render in about a second. The same file can play live as a game
(`raea play`), render one frame for an agent to look at (`-o frame.png --frame 45`), or be checked against a quality
floor (`raea check hello.mp4`). `raea init` starts a project, and `raea --help` lists every verb.

## What it does

- **Video and motion graphics.** Layout, text, Tailwind, SVG and animation, composited on the GPU and encoded in
  hardware where the machine has it. Footage is cut by what's said in it (`raea cut`), and `raea check` catches clipped
  text, low contrast, unsafe areas, freezes and loudness problems before anyone watches.
- **Games.** Input, physics, sound and 3D, played live with `raea play`, and exported as a program of their own
  (`raea export-game`).
- **3D.** Scenes in TypeScript drawn by [Bevy](https://bevyengine.org) 0.20: cameras, lights, materials, glTF models,
  Gaussian splats, and Bevy plugins compiled into the engine.
- **Sound.** One mixer for live play and offline render, instruments and effects, and local models for foley and
  instrument notes (`raea sao`, `raea tokensynth`).
- **Built for agents.** Everything it draws stays queryable as data, so an agent can render a frame, look at it, and
  fix what's wrong without a person in the loop.

## Use it from your agent

- **Agent skills.** They live in [raea-skills](https://github.com/shinyobjectz/raea-skills), pinned here as
  [`skills/`](skills): `raea` teaches an agent to write, render and check raea compositions, 3D and games
  included. The installer adds it to `~/.claude/skills`, and `raea skills install --agent NAME` to other agents.
- **Claude Code plugin.** `/plugin marketplace add shinyobjectz/raea`, then `/plugin install raea@raea`.
- **Craft guides.** `raea guide` prints the floor every edit is held to and the guides for pitches, explainers, stories,
  games, motion, music and layout. `raea context PROJECT` says what to read before working on a project.

## Build from source

Requirements: [Rust](https://rustup.rs) (rustup reads the pinned version from rust-toolchain.toml), [bun](https://bun.sh)
and CMake. On macOS, Xcode's command-line tools; on Linux, the ALSA, fontconfig, udev, xkbcommon (with its X11
part, which the app targets' windows also load at run time) and Vulkan libraries and NASM (Ubuntu: `cmake ninja-build
nasm libasound2-dev libfontconfig1-dev libudev-dev libvulkan-dev libxkbcommon-dev libxkbcommon-x11-dev`), as public CI
installs them.

```sh
git clone https://github.com/shinyobjectz/raea && cd raea
cargo build --release -p raea-cli --no-default-features
for d in sdk/raea sdk/react/js sdk/react/tw; do (cd $d && bun install); done
target/release/raea render sdk/react/bench/games/pong/edit.tsx -o pong.mp4
```

`--no-default-features` builds the public engine, as the releases are. The default features add one hosted service,
Content Credentials signed by raea's signing server, which isn't part of this repository. raea sends nothing on its
own: no telemetry, crash reports or update checks. What reaches the network is what you ask for (a remote URL in a
composition, a model downloaded on first use, a `raea gen` step on a provider you hold a key for), and `RAEA_OFFLINE=1`
refuses all of it. [docs/settings.md](docs/settings.md) lists the settings raea reads.

## What's in here

The workspace is layered, from pure building blocks up to the command.

| Folder | What |
|---|---|
| [`core/`](core) | Pure building blocks: the kernel language (K, interpreted on the CPU and compiled to WGSL), easing and animation, shapes, fonts, the scene and wire formats |
| [`gpu/`](gpu) | The one shared GPU device and its telemetry |
| [`io/`](io) | Decode and encode (VideoToolbox, software codecs, AV1, a muxer), files, network, npm packages, traces |
| [`systems/`](systems) | Engine systems: layout, canvas, compositing, filters, mixing, speech, ML on ONNX Runtime, footage reading, lighting, contact checks for 3D parts, splats |
| [`runtime/`](runtime) | Running a composition: the JavaScript engine seam (JavaScriptCore on macOS, QuickJS elsewhere), React and HTML on Bevy, baking, encode, live play |
| [`engine/`](engine) | The renderer (`engine/render`), the in-process TypeScript bundler, the Bevy plugin catalog, and the `raea` command itself (`engine/cli`) |
| [`apps/`](apps) | The crates behind the CLI's verbs: check, play, games, media |
| [`sdk/`](sdk) | `raea`, the TypeScript SDK (`sdk/raea`), its 3D, physics, audio, video and UI packages, the craft guides, and the agent skills |
| [`vendor/`](vendor) | Patched upstream crates, under their own licences ([`vendor/vendor.toml`](vendor/vendor.toml) says why each is patched) |
| [`tools/`](tools) | Build and release helpers |

Behaviour specs written in Gherkin (`.feature`) sit beside the code they describe.

## Licence

raea is under the [Functional Source License 1.1, ALv2 Future License](LICENSE) (FSL-1.1-ALv2), copyright Shane
Murphy. Use it, change it, build on it and pass it on for anything except a product that competes with raea. Each
version becomes Apache-2.0 two years after its release. Third-party parts keep their own licences; see
[NOTICE](NOTICE).
