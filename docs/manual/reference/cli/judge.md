---
title: "raea judge"
summary: "Judge a 3D scene's review sheets blind, by a model on a frozen rubric or a fresh agent session, with the pass decided in code."
order: 43
sources: ["apps/judge/src/lib.rs", "apps/judge/src/rubric.rs", "apps/judge/src/agent.rs", "apps/judge/src/eye.rs"]
---

`raea judge` reads the sheets [`raea render --review`](render.md) makes (`colour.png`, `clay.png`) and has them
judged without the code or the conversation that made them: by clef-omni on a frozen rubric (the default), or by a
fresh Kimi or Claude session on the judge prompt. The scores come back as data, and whether the scene passes is
decided in code from them. clef only advises until it has been calibrated against a person's or an agent's eye.

`--kind scene|kit|shot` judges on that kind's own rubric, for a job [`raea make`](make.md) holds: a scene on layout,
props, signage, materials, light, atmosphere, plausibility and composition; a kit on cohesion, reference fidelity,
the brief, mesh, texture, assembly, scale and labels; a shot on framing, move, focus, clearance, continuity and grade
(read from `framing.png`, which `raea make framing` writes). Form categories (layout, props, mesh, assembly, scale) are
judged on the clay sheet. The bar is a production base: don't nitpick, at most five fixes, each to do in a script.
The verdict and the log line say the kind, and `raea judge calibrate --kind KIND` fits that kind's calibration on its
own.

```sh
raea render scene.tsx --review review/
raea judge review/ --brief brief.ts
raea judge review/ --by claude --target reference.png --json
raea judge review/r3 --kind kit --by claude --brief brief.ts   # a kit job's round, on the kit rubric
raea judge eye review/ composition=4 light=3 --pass yes      # record your own scores, for calibration
raea judge calibrate
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea judge DIR [--kind scene|kit|shot] [--by clef|kimi|claude] [--brief brief.ts] [--target T.png] [--json] [--dry] | judge eye DIR (CATEGORY=SCORE ... [--pass yes|no] | --by kimi|claude) | judge calibrate [--kind scene|kit|shot] [--json]   (a --review folder's sheets judged blind: clef-omni on the frozen rubric, or a fresh kimi or claude session on the judge prompt; --kind asks a raea make job's own rubric (a shot list's framing.png); the pass decided in code; clef advises until calibrated against your eye or an agent's, each kind on its own)
```

Headless: it needs nothing running and opens no window. Takes any number of arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--brief` | `brief.ts` | The project's brief, so the judge knows what the scene is for. |
| `--by` | `clef\|kimi\|claude` | Who judges: clef-omni on the rubric (default), or a fresh Kimi or Claude session. With `judge eye`, the agent whose eye is recorded. |
| `--dry` | – | Print what the judge would be asked, with its images, without asking. |
| `--json` | – | Print the verdict as JSON: each score, the defects found and the pass. |
| `--kind` | `scene\|kit\|shot` | The rubric: `scene`, `kit` or `shot` (the make loop's categories), instead of the film rubric. |
| `--pass` | `yes\|no` | With `judge eye`: whether you'd pass it, recorded with your scores. |
| `--target` | `T.png` | A reference image the colour sheet is judged against. |

<!-- generated:end -->
