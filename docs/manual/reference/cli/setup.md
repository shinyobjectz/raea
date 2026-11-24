---
title: "raea setup"
summary: "Find Blender, or download its current LTS, and install raea's add-on and the MCP add-on."
order: 129
sources: ["apps/tools/src/blender.rs"]
---

`raea setup blender` finds the Blender on this machine (from `git config raea.blender`, raea's own copy, /Applications or the `PATH`), or downloads the current LTS from blender.org into `~/.raea/tools/blender`. It then installs the add-on that
lets an agent drive Blender over MCP. [`raea blender run`](blender.md) uses the Blender it found.

```sh
raea setup blender --check
raea setup blender
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea setup blender [--check] [--download]   (find Blender or download the LTS into ~/.raea/tools/blender, and install raea's add-on and the MCP add-on)
raea setup motion [--check]   (what raea motion runs on, made where missing (--check only says): on a Mac the MLX models in raea's models folder (the motion model 1.1 GB, the text encoder 8.8 GB); on Linux and Windows, in raea's models folder's kimodo-torch, a Python 3.11 environment made by uv (which it needs on PATH: https://docs.astral.sh/uv) with PyTorch (its CUDA build where there is an NVIDIA driver, about 8 GB, else CPU only, about 2 GB) and upstream Kimodo (nv-tlabs/kimodo at e1c219c, Apache-2.0), then the weights: Kimodo-SOMA-RP-v1 (1.1 GB, NVIDIA Open Model License), LLM2Vec's two adapters (0.3 GB, MIT) and Meta Llama 3 8B Instruct in bf16 (16 GB, Meta Llama 3 Community License, from NousResearch's ungated copy, checked file by file against Meta's SHA-256); about 26 GB in all. It says what this machine has and where each network runs: the denoiser on CUDA when there is an NVIDIA GPU, else the CPU; the text encoder beside it on a GPU of 18 GB or more, else the CPU)
raea setup godot [--check]   (download Godot 4.6.1 with GodotJS, its TypeScript bindings (MIT), into ~/.raea/tools/godot: JavaScriptCore on the Mac, QuickJS on Windows and Linux; raea's own build (the Mac's notarized), else GodotJS's, each checked against its pinned sha256)
```

Headless: it needs nothing running and opens no window. Takes up to 1 argument besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--check` | – | Say which Blender would be used, without downloading or installing anything. |
| `--download` | – | Download the LTS even when a Blender is already on this machine. |

<!-- generated:end -->
