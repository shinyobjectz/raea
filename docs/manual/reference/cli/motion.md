---
title: "raea motion"
summary: "Body motion made on this machine by Kimodo (NVIDIA), on MLX on a Mac and on PyTorch on Linux and Windows: a SOMA clip from a line of words and, with --keys, Kimodo constraints (full-body keyframes, hand or foot targets, a root path, as sdk/body/tools/between.ts writes them), as the clip sdk/body/tools/fromkimodo.ts reads."
order: 107
sources: ["engine/cli/src/cli.rs"]
---

Body motion made on this machine by Kimodo (NVIDIA), on MLX on a Mac and on PyTorch on Linux and Windows: a SOMA clip from a line of words and, with --keys, Kimodo constraints (full-body keyframes, hand or foot targets, a root path, as sdk/body/tools/between.ts writes them), as the clip sdk/body/tools/fromkimodo.ts reads.

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea motion "LINE" [--keys KEYS.json] [--seconds 5 | --frames N] [--seed 1] [--steps 100] [-o CLIP.json]   (body motion made on this machine by Kimodo (NVIDIA): a SOMA clip from a line of words and, with --keys, Kimodo constraints (full-body keyframes, hand or foot targets, a root path, as sdk/body/tools/between.ts writes them), as the clip sdk/body/tools/fromkimodo.ts reads. On a Mac it runs on MLX (Apple silicon's GPU): the first use fetches the motion model (1.1 GB) into raea's models folder, and the first new line its text encoder (8.8 GB: Llama 3 8B in 8 bits with LLM2Vec's adapters). On Linux and Windows it runs upstream Kimodo on PyTorch, set up once by raea setup motion: on an NVIDIA GPU with CUDA (the text encoder, 16 GB in bf16, on the CPU unless the card has 18 GB or more), else on the CPU alone; reading a new line on the CPU takes about 18 GB of memory. A line's words are read once and kept. Licences: Kimodo's weights NVIDIA's Open Model License, Meta Llama 3 the Meta Llama 3 Community License, LLM2Vec's adapters MIT, Kimodo's code Apache-2.0)
```

Headless: it needs nothing running and opens no window. Takes up to 1 argument besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--frames` | `N` | The clip's length in frames at 30 a second, 2 to 600. Instead of `--seconds`. |
| `--keys` | `KEYS.json` | Kimodo constraints the motion must meet: full-body keyframes, a hand or foot at a place on a frame, a path for the hips. `bun sdk/body/tools/between.ts` writes them between two poses of the body library. |
| `--seconds` | `5` | The clip's length in seconds (5 unless said). |
| `--seed` | `1` | Which of the motions the line could be: another seed is another take of the same line. A seed on MLX is not that seed on PyTorch (the noise is drawn differently), so the same seed is a different take on a Mac and on Linux or Windows. |
| `--steps` | `100` | Denoising steps (Kimodo's 100). Fewer is faster and rougher. |
| `-o` | `CLIP.json` | Where the clip goes: `{fps, names, root, turns, contacts, ...}`, Kimodo's 77 SOMA joints as each one's turn in the world from the T-pose. Without it, the line's words name the file in the working folder. |

<!-- generated:end -->

## Example

A stage's `do` line made into a clip: the body stands at the start and the end, and the motion between comes from the
words.

```sh
bun sdk/body/tools/between.ts skeleton.json LIBDIR aMale2_A1_Stand:0 aMale2_A1_Stand:0 stand.json 4 0,0 0
raea motion "A person waves at the camera with their right hand" --keys stand.json --seconds 4 --seed 2 -o wave.clip.json
```

A clip takes about 8 seconds on an M5 Pro. `bun sdk/body/tools/makedo.ts` does the whole of it for a stage's lines:
three seeds a line, each checked against the captures the body hands on to, and the one that does the most kept in the
project's made clips.

## Where it runs

On a Mac, Kimodo runs on MLX, on the GPU, with no Python. The first use fetches the motion model (1.1 GB), and the first
new line the text encoder (8.8 GB).

On Linux and Windows it runs upstream Kimodo (nv-tlabs/kimodo) on PyTorch. Run `raea setup motion` once first: it needs
[uv](https://docs.astral.sh/uv) on PATH, makes a Python environment in raea's models folder, and fetches the weights,
about 26 GB in all. With an NVIDIA GPU the motion runs on CUDA, and the text encoder (Llama 3 8B Instruct, 16 GB in
bf16) joins it on a GPU of 18 GB or more. On a smaller card the encoder runs on the CPU and needs about 18 GB of memory.
With no NVIDIA GPU everything runs on the CPU. `raea setup motion --check` says what is set up and where each part runs.

Both backends write the same clip file. A line's words are read once and kept, so the encoder loads once a line.

Licences: Kimodo's code is Apache-2.0 and its weights are under NVIDIA's Open Model License. The text encoder is Meta
Llama 3 8B Instruct (Meta Llama 3 Community License) with LLM2Vec's adapters (MIT).
