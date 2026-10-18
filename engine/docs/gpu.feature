@owner:unicorn @ticket:T-048
Feature: Faster renders with MLX, Core ML, or the GPU?
  Investigation, 2026-10-09. The question: can raea render faster by moving work to MLX or Core ML, or by
  rewriting rendering code for them? The short answer is not with MLX or Core ML. The work worth moving is
  per-pixel raster, and the tools for that are Metal: `vello_gpu`, the wgpu sibling of the vello_cpu rasterizer
  used today, or a few targeted Metal/Accelerate kernels.

  All measurements are on the M4 Mac mini unless they say iPhone. The machine was shared with other builds (load
  20 to 45), so absolute CPU times are noisy; the ratios held across runs. The benchmarks live in `bench/gpu/` (see
  the rule "Reproduce" at the end). Converted from the markdown version for T-048.

  Rule: Where the time goes today
    Profiles with `bench/profile.py` of the render thread (`--workers 1`, `--format rgba`), on two eval edits the
    in-app Agent made. 75 to 85% of each frame is per-pixel work. That work is parallel and suits a GPU. The DOM,
    style, layout and JS work is small and branchy, and stays on the CPU whatever happens.

    The talking-head shadows come from the cut tool's caption style. Each caption word is an inline-block with three
    text-shadows: two `0 0 3px` and one `0 4px 16px`. Blitz paints every blurred shadow of every glyph run as its
    own clipped filter layer, so a three-word caption is nine blur layers per frame.

    On the iPhone 13 Pro Max (A15, 2 workers on the 2 P-cores, per the host app-a2's device bench), a 20 s 1080x1920
    edit renders in 6.1 to 7.0 s. That is about 3x real time, with a 740 MB peak footprint and the thermal state
    nominal.

    Scenario: Profile of the render thread on two eval edits
      Given the talking head with captions (round-5/05-broll, 300 frames)
      And the motion scene (scene-2 ingredients, 150 frames)
      When `bench/profile.py` profiles the render thread with `--workers 1` and `--format rgba`
      Then the time per frame splits like this:
        | measure                                                    | talking head + captions | motion scene  |
        | render thread CPU per frame                                | 13.2 ms                 | 14.6 ms       |
        | raster: filters (text-shadow blur, drop-shadow)            | 4.5 ms (34%)            | 4.4 ms (30%)  |
        | raster: fine (video draw, fills, gradients)                | 3.4 ms (26%)            | 5.5 ms (38%)  |
        | memcpy / memset / alloc (frame clears, image convert)      | 2.0 ms (15%)            | 2.3 ms (16%)  |
        | reading ffmpeg frames from the pipe (Mac CLI only)         | 1.7 ms (13%)            | 0             |
        | JS (Boa), Stylo, Taffy/Parley, paint commands, GSAP seek   | ~1.2 ms (9%)            | ~1.8 ms (12%) |
      And 75 to 85% of each frame is per-pixel work

  Rule: MLX does not fit
    `gpu_bench.py` and `mlx_parts.py` (MLX 0.32.3) run the shadow blur (sigma 8) of the caption band plus a
    source-over composite onto the video, on the M4 GPU. For comparison, the whole talking-head filter cost on the
    CPU today is 4.5 ms, and MPS does the same band blur in 0.47 ms (see the Metal rule).

    Why MLX doesn't fit:
    - It has no rasterizer. Path coverage, glyphs, gradients, clips and blend modes are not array ops. "Writing the
      renderer for MLX" means writing a GPU 2D renderer out of custom Metal kernels inside an ML framework. Metal or
      `vello_gpu` already provide that, along with texture sampling, blending hardware and render passes, which MLX
      does not expose.
    - Its image ops are slow here. MLX's convolutions are tuned for ML shapes (many channels, small images), not
      1-channel 1080-pixel rows. The fastest MLX version is 5x slower than MPS.
    - It costs a sync per eval. The fixed ~0.35 ms launch-and-sync per `mx.eval` adds up over many small layers
      per frame.
    - Integration is awkward. The renderer is Rust, and MLX has no binding in this tree (mlx-rs would add a second
      copy of the MLX C++ runtime). The app already links MLX Swift for Pocket TTS, and two copies of the C++
      library in one binary invite symbol clashes.

    Scenario: MLX formulations of the caption-band shadow blur
      Given the caption band's shadow blur (sigma 8) and a source-over composite onto the video
      When `gpu_bench.py` and `mlx_parts.py` run it on the M4 GPU with MLX 0.32.3
      Then each formulation costs:
        | MLX formulation                                                         | ms / frame |
        | `mx.conv2d` 1x49 then 49x1, fp16, then the composite as array ops       | 10.0       |
        | the same with the upload from numpy each frame                          | 11.1       |
        | conv2d blur alone                                                       | 5.0        |
        | blur as two Toeplitz matmuls                                            | 2.6        |
        | matmul blur + a hand-written `mx.fast.metal_kernel` composite           | 2.7        |
        | the smallest possible op + `mx.eval` (launch and sync overhead)         | 0.35       |
      And the fastest of them is 5x slower than MPS's 0.47 ms

  Rule: Core ML does not fit
    `gpu_bench.py coreml` builds the shadow blur as a two-conv MIL program (`cm_plan.py` prints the compute plan).
    The times are identical because Core ML runs it on the CPU in every mode. The compute plan puts both convs on
    `MLCPUComputeDevice`, and the Neural Engine isn't even listed as supported for them. The 0.67 ms is Apple's
    BNNS CPU convolution. A plain Accelerate call gives the same speed with no model, no MLMultiArray and no compile
    step (`vimage_bench.swift`: 0.45 to 0.48 ms for the band's alpha, three box passes, 1 thread).

    Core ML can't rasterize either. Its place in this product is new ML effects rather than speeding up the
    renderer:
    - person segmentation for background blur or replacement;
    - face-tracked auto-reframe of landscape takes;
    - upscaling.

    Scenario: Core ML compute units for the two-conv blur
      Given the shadow blur as a two-conv MIL program
      When `gpu_bench.py coreml` runs it on each compute unit setting
      Then it costs:
        | compute units | ms / frame |
        | CPU_ONLY      | 0.67       |
        | CPU_AND_GPU   | 0.67       |
        | CPU_AND_NE    | 0.68       |
        | ALL           | 0.71       |
      And `cm_plan.py` shows both convs on `MLCPUComputeDevice` in every mode

  Rule: What Metal and the GPU actually buy
    The copies cost more than the GPU work. On Apple silicon they can be avoided by wrapping IOSurfaces:
    - AVAssetReader already gives IOSurface-backed BGRA buffers;
    - the AVAssetWriter pool buffers are IOSurfaces too;
    - CVMetalTextureCache turns both into textures without a copy.

    `vello_gpu` 0.3 (`raster/`, formerly vello_hybrid) is the drop-in candidate. It is the wgpu (Metal) sibling of
    `vello_cpu` 0.3 and shares the same `vello_common` 0.3 (strips, filter math):
    - `anyrender_vello_hybrid` 0.11 targets our anyrender 0.14;
    - it supports the filters we use: Gaussian blur, drop shadow, flood and offset;
    - it takes external textures (`ImageSource::external_texture` + `TextureBindings`), so a decoded video frame
      can be sampled with no CPU copy.

    Scenario: MPS reference on one 1080x1920 frame
      Given `mps_bench.swift`
      When it renders one 1080x1920 frame
      Then each stage costs:
        | stage                                                              | ms / frame |
        | MPSImageGaussianBlur, sigma 8, caption band                        | 0.47       |
        | blur + full-frame composite kernel, data on the GPU                | 0.94       |
        | the same, 3 frames in flight                                       | 0.71       |
        | the same + upload two layers and read back with replace/getBytes   | 4.02       |
      And the copies cost more than the GPU work

    Scenario: vello_cpu against vello_gpu on a frame shaped like the eval edit
      Given a frame shaped like the eval edit:
        | part                                       |
        | a 1080x1920 video frame                    |
        | five caption words with three shadows each |
        | three title words with two shadows each    |
        | 21 blur layers in total                    |
      When the bench draws the same frame through both renderers
      Then they cost:
        | part                                              | vello_cpu (1 thread)       | vello_gpu (Metal)                                                    |
        | video frame only                                  | 2.74 ms                    | 1.13 ms (2.69 with an 8 MB `write_texture` each frame)               |
        | caption and title shadows only (21 blur layers)   | 22.2 ms                    | 9.9 ms                                                               |
        | text without shadows                              | 0.87 ms                    | 1.33 ms (fixed submit + wait overhead)                               |
        | whole frame                                       | 29.0 ms                    | 12.8 ms; 11.9 pipelined                                              |
        | whole frame, one blur layer per shadow per group  | (slower: 69 to 104 ms)     | 6.2 ms                                                               |
        | output difference vs vello_cpu                    |                            | mean 0.09 / 255; 0.37% of channels off by more than 2                |
      And the raster gets about 2.3x faster on the M4, with near-identical output
      And the parity gate (`bench/parity.sh`) would still need to confirm it against Chrome

    Scenario: Two caveats matter more than the headline
      Given vello_gpu's 2.3x on the M4
      Then caveat 1 is that `vello_gpu` spends its time on the CPU: it schedules a pass per filter layer, and a loaded run timed `render()` plus command encoding alone at 16.4 ms per frame, against 16.2 ms for the full GPU round trip
      And on the phone that cost scales with the A15 CPU, roughly 1.7x slower than an M4 P-core, and the GPU is idle much of the time
      And merging layers (one per shadow per caption group) halves it to 6.2 ms, but doing that in blitz-paint changes paint order between words, which is a parity risk
      And caveat 2 is that the A15 GPU has 5 cores against the M4's 10, and both render workers would share it
      And scaling M4 to A15 by CPU (~1.7x) and GPU (~3x) puts vello_gpu on a shadow-heavy phone frame somewhere between break-even and 2x faster, which only a device run can settle

  Rule: Recommendation, ranked by speed-up per unit of work and risk

    Scenario: 1. Faster text shadows on the CPU, now
      Given a text-shadow is a blurred coverage mask in one color
      And Skia (Chrome) blurs it as A8, while vello_cpu blurs full premultiplied RGBA per glyph run
      Then blurring alpha only is about 2.7x less work (vImage Planar8 0.48 ms against ARGB8888 1.29 ms)
      And Accelerate's box blur, or NEON in Rust, beats the current direct convolution
      And coloring at composite time is free
      And this targets the largest bucket on both edits (30 to 34%), keeps the exact pipeline, and needs no new dependency on iOS
      And it is a change to vendored vello_cpu (filter) or blitz-paint (text.rs), run through the parity gate

    Scenario: 2. Spread frames across workers by demand, and use the E-cores
      Given round-robin dealing makes the slowest worker set the pace, which is why framework/docs/ios.feature caps workers at 2
      When frames are spread across workers by demand instead of round-robin
      Then a third or fourth worker can use the E-cores, since on the phone 4 E-cores sit idle
      And each extra worker costs about 50 to 80 MB, which the 740 MB peak can absorb

    Scenario: 3. Skip wasted pixel copies around video
      Given the video frame is copied into a vello image every frame (`convert_image`, 0.8 ms)
      And the frame is cleared even when an opaque full-frame video covers it (`bzero`, 0.4 ms)
      Then both can be skipped
      And on iOS the frame provider's vImage copy could also go

    Scenario: 4. vello_gpu as a third backend, as a measured experiment
      Given items 1 to 3 have been done and still leave renders too slow
      When `vello_gpu` becomes a third backend (`--backend gpu`)
      Then anyrender_vello_cpu's local changes (CoreText glyph masks via Skia, non-isolating clips) are ported to `anyrender_vello_hybrid`
      And the zero-copy IOSurface path is wired for video in and frames out
      And the parity gate runs, then the device bench
      And its gain on the A15 is uncertain, and it adds wgpu to the app (a few MB)

    Scenario: 5. MLX and Core ML: don't
      Then MLX and Core ML are not used for render speed
      And Core ML stays interesting for new ML effects (segmentation, reframe)

  Rule: Skia on Metal (Ganesh), measured 2026-10-15 (the experiment the user asked for)
    # Profiling product-promo (the heaviest registry page, Skia backend) put its paint in gradient shading, source-over
    # blending, Skia's three-box blur and row fills: per-pixel work. JS (the timeline seek) is 1-23% of a frame on GSAP
    # pages and 1% on React, so moving JS anywhere (GPU kernels, keys) has a small ceiling; raster is the lever.
    # Skia's own GPU backend keeps the drawing pipeline parity is tuned on. Built as an opt-in: the engine's
    # `skia-gpu` feature (anyrender_skia `metal`: skia-safe metal + ganesh, a prebuilt binary, no source build) and
    # RAEA_SKIA_GPU=1 at run time; the renderer draws into a Ganesh render target and reads the frame back.
    Scenario: Speed, M4, ms per frame (scratchpad bench for the synthetic frame; examples/stage_bench.rs for pages)
      Then
        | frame                                                        | Skia CPU | Skia Metal (incl. readback) |
        | synthetic 1080x1920: 3 gradients, 24 blurred-shadow cards   | 224.9    | 13.6 (16.5x)                |
        | product-promo, paint                                         | 7.54     | 4.36 (1.7x)                 |
        | kinetic-type, paint                                          | 1.51     | 1.39 (same)                 |
        | react/board, paint (many small elements and text)            | 8.37     | 9.90 (slower)               |
      And the synthetic frame's pixels match the CPU's closely: mean |diff| 0.21/255, 0.51% of channels off by > 2
    Scenario: Parity against Chrome, the whole corpus through the Skia backend
      Then
        | raster      | exact | ok | fail |
        | Skia CPU    | 104   | 60 | 6    |
        | Skia Metal  | 61    | 81 | 28   |
      And so Metal stays an experiment: Ganesh's GPU antialiasing, gradient dithering and glyph atlas differ from the
        raster pipeline Chrome's references come from, enough to fail 28 frames
    Scenario: What would make it a default
      Then the 28 failing frames are diagnosed first (likely: dithering off, analytic AA vs MSAA, text as the CPU's
        glyph masks uploaded as images rather than GPU glyphs)
      And the choice is per page, from the document (heavy filters, gradients, big blends), never per frame: small-
        element pages are slower on the GPU (fixed submit and readback costs)
      And the phone is measured before anything ships (the A15's GPU is half the M4's)

  Rule: Reproduce
    The commands below rebuild every measurement in this file. `PARTS` takes any of video, text, noshadow, batch.

    Scenario: Re-run the GPU benchmarks
      Given a Python 3.12 venv with MLX, coremltools and numpy
      When these commands run:
        """
        uv venv -p 3.12 /tmp/gpuvenv && uv pip install -p /tmp/gpuvenv/bin/python mlx coremltools numpy
        cd bench/gpu
        /tmp/gpuvenv/bin/python gpu_bench.py            # numpy, MLX, Core ML
        /tmp/gpuvenv/bin/python mlx_parts.py            # MLX formulations
        /tmp/gpuvenv/bin/python cm_plan.py              # Core ML compute plan (which device runs each op)
        swiftc -O mps_bench.swift -o /tmp/mps && /tmp/mps
        swiftc -O vimage_bench.swift -o /tmp/vi && /tmp/vi
        cd raster && cargo build --release && PARTS=video,text ./target/release/uv-gpu-raster-bench
        #   PARTS: any of video, text, noshadow, batch
        """
      Then they print the tables above
