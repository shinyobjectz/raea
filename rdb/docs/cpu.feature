Feature: raea renders headless on a CPU, in a sandbox with no GPU
  The GPU stays the main path (the founder's devices). A second deliverable runs the same edits with no GPU: a Linux
  sandbox or container, headless, slower but byte-for-byte the same frames where it can be. Asked for by the user on
  2026-10-15 ("a CPU and GPU friendly deliverable"); this is the plan and what was measured that day.

  Background: what the GPU does today, and what already has a CPU twin
    # Measured 2026-10-15 on the M4 (10 s CJ concept, 1080x1920, 300 frames)
    #   GPU direct path (gpuview: composite.k -> nv12buf.k -> VideoToolbox): 197 fps
    #   composite.k on the GPU: 15 ms of compute a frame; composite_reference (native Rust, one thread): 665 ms a frame
    #   nv12: nv12_reference exists and is bit-equal to the GPU kernel
    #   H.264 encode, 120 frames: VideoToolbox 185 fps (SSIM 0.965), openh264 ("soft") 64 fps (SSIM 0.952)
    #   decode: openh264 random seeks are slow (60 seeks in 26 s); a render decodes in order, which is cheap
    #   2D overlays (captions, titles, HTML pages): Blitz paints them; vello_cpu is always built; the CJ page took
    #   38.7 s on the CPU vs 7.8 s on the GPU path (the page painted every frame; rows repaint only when they change:
    #   39 of 300 frames for a CJ concept)
    # Every kgpu kernel has three runs checked equal: GPU, native Rust, and the K interpreter

  Scenario: C1. A CPU direct render, the same bytes as the GPU's
    Given an edit without 3D
    When rdb renders it with RAEA_CPU=1 (or when no GPU adapter is found)
    Then gpuview's composite and nv12 steps run as composite_reference and nv12_reference over row bands on every core
    And the overlay is painted by Blitz on vello_cpu, only on frames where it changes (as now)
    And the sound is mixed and mastered as now (already CPU)
    And decode and encode go through raea-media's soft backend (openh264) or ffmpeg where the sandbox has it
    And the frame hashes equal the GPU direct path's (the gate gains "render: UGC, CPU" against gpu-apple-a.txt)
    # Done 2026-10-15 (rdb cpuview.rs; kgpu composite_cpu and nv12_cpu): the UGC example's 738 frames equal G8's GPU
    # reference, and the gate checks it. On the M4 with no GPU used (10 cores), UGC 1080x1920, 24.6 s:
    #   hashes only: 35 fps (composite 11.5, nv12 6.9, decode 2.4, overlay 0.8 ms/frame; overlay painted 34 times)
    #   .mp4, Apple decode + VideoToolbox encode: 54 fps
    #   .mp4, all software (openh264 decode + encode, RAEA_MEDIA=soft): 23 fps, 33 s wall; decode 31 ms/frame leads
    #   .mp4, ffmpeg backend (libx264): 18 fps
    # Next: decode on more threads (openh264 is one), and the copies around nv12 (2.7 ms alone, 6.9 in the loop)

  Scenario: C2. The CPU composite is fast enough to matter
    # composite_reference is written for checking, not speed: one thread, per-pixel bilinear in i32
    Given the native composite at 665 ms a frame on one thread
    When it runs on row bands across cores with the source map hoisted out of the pixel loop, and the inner loop
      written for autovectorising (or std::simd)
    Then a 1080x1920 frame composites in 30 ms or less on 8 cores
    # Done 2026-10-15 (kgpu composite_cpu, f28422e): bit-equal to the reference and the GPU, 10.5 ms on 10 cores,
    # 66 ms on one thread; the reference measured 141 ms that run (665 ms earlier, under memory pressure)
    And a 10 s concept renders in about 10-20 s on 8 cores, bounded by openh264's 64 fps encode
    And the K interpreter stays the referee: CPU = native = GPU, bit for bit

  Scenario: C3. Or the GPU kernels on a software GPU, unchanged
    # wgpu on Mesa's lavapipe (Vulkan on llvmpipe) runs the same WGSL on CPU threads, JIT-compiled with SIMD
    Given a Linux sandbox with Mesa's lavapipe and no GPU
    When rdb renders with wgpu's adapter on lavapipe
    Then the gpuview kernels and the Bevy engine (3D included) run with no code change
    And it is measured against C1 on the CUDA box's WSL Ubuntu (Docker, headless): whichever is faster for 2D becomes
      the default there; lavapipe stays the only CPU route for 3D

  Scenario: C4. Perception on a CPU: what moves, what stays
    # Portable today: SigLIP 2 (ONNX, CPU, about 60 ms a frame on 4 threads), TransNetV2 (ONNX through ort),
    # DeepFilterNet3 (tract, pure Rust), MediaPipe face landmarks (performance.json)
    # Apple-only today (Vision): faces, attention saliency, OCR, person segmentation, aesthetics, feature print
    Given an ingest in a Linux sandbox
    Then the portable models run on the CPU as they are
    And Apple-only steps get open, commercial-licence replacements: faces from MediaPipe's detector, text from an
      Apache-licensed OCR, person masks from MediaPipe's segmenter, saliency from SigLIP patch attention or a small
      open saliency model (each checked against the Vision output on our takes before it replaces it)
    And anything too slow on a CPU runs as a cloud GPU call instead (Modal), keyed by the source's hash like the
      ingest store, so a sandbox only ever reads results
    And embeddinggemma-2 is a cloud call or a 1 fps sample on CPU: on MPS it took 1.2 s a frame (2026-10-15),
      20x SigLIP on CPU, and its licence is unsettled

  Scenario: C5. Agent tools in the sandbox
    Given the agent's ugc rdb tools
    Then look and stills run on C1 (one frame each, CPU cost about one composite plus one overlay paint)
    And energy and compare are already CPU work (kgpu kernels have native twins), K on the CPU through its native runs
    And the eval host can run in the sandbox with RAEA_CPU=1, so CI and agent runs need no GPU
