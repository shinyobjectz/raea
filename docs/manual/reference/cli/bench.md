---
title: "raea bench"
summary: "Each case on each denoiser against its converged truth: FLIP, RMSE, bias, flicker, blotches, lag after a disocclusion or a light going out, a frame's cost, and how many of MetalFX's numbers each meets."
order: 49
sources: ["engine/cli/src/cli.rs", "engine/render/denoise/bench/src/lib.rs", "engine/render/denoise/bench/src/metrics.rs", "engine/render/denoise/bench/src/report.rs"]
---

Each case on each denoiser against its converged truth: FLIP, RMSE, bias, flicker, blotches, lag after a disocclusion or a light going out, a frame's cost, and how many of MetalFX's numbers each meets.

`raea bench denoise` holds each denoiser to the same truth (docs/denoise.feature DN0). Every case is drawn with
Solari on each backend and against its reference: the same frames path-traced, each drawn `--spp` times so it
converges (raea light check's reference, kept between runs until `--fresh`). Each backend draws 16 frames before
each still and each shot, so its history is its own. For each case and backend it reports:

- FLIP (LDR-FLIP, written from Andersson et al. 2020, at 67 pixels a degree), RMSE, and the bias in stops (brighter
  is positive), the mean over the case's stills;
- blotches: the soft low-frequency patches a denoiser leaves, as the error in stops of the band between a 2-pixel and
  a 1/64-of-the-width blur;
- flicker over a short shot: the error's second difference in time against the same shot path-traced, so motion
  cancels and only light that changes when the truth doesn't counts. A whole-frame spike is the worst frame whose
  mean light jumps alone;
- the lag at each of the case's events (a disocclusion, a light going out): frames until the error settles back
  within 1.25 times what it was before;
- what a still costs, and, for a case that says `"live"`, what a frame costs played live.

Against MetalFX, the first bar on the Mac, the table says how many of its numbers each backend meets.

The backends are `off` (Solari with no denoiser), `metalfx` (the Mac's MetalFX denoised scaler), `raea` (raea's own,
DN1-DN8) and `dlss` (DLSS Ray Reconstruction, DN8, on an NVIDIA RTX machine). One that can't run here is reported
with why. The cases are raea light check's scene file with `"events": [{ "at": FRAME, "what": "disocclusion" }]`
inside each shot; bench/denoise/cases.json is the default, and a relative `dir` is the file's own folder's.

```sh
raea bench denoise --backend metalfx,off
raea bench denoise doorway mirror --backend off --spp 512 -o out/denoise/
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea bench denoise [--backend raea,metalfx,dlss,off] [CASE...] [--cases FILE] [-o DIR] [--spp N] [--fresh]   (each case on each denoiser against its converged truth: FLIP, RMSE, bias, flicker, blotches, lag after a disocclusion or a light going out, a frame's cost, and how many of MetalFX's numbers each meets; DIR/report.json)
```

Headless: it needs nothing running and opens no window. Takes any number of arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--backend` | `raea,metalfx,dlss,off` | the backends to run, comma-separated (all four by default) |
| `--cases` | `FILE` | the cases file (bench/denoise/cases.json) |
| `--fresh` | – | path-trace the truth again, not the kept one |
| `--spp` | `N` | the truth's samples a frame, where a case doesn't set `"spp"` (256) |
| `-o` | `DIR` | where the frames and report.json go (runs/ beside the cases file) |

<!-- generated:end -->
