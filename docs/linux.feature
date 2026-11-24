@spec
Feature: The Linux engine, as fast as it can be in the cloud
  Shane, 2026-11-11: "lets pinpoint and build a plan to test, validate and improve the linux engine for raea so that it
  can become more performant and as performant as possible in the cloud". It follows the raea cloud talk the same day:
  Mac minis are out (macOS's licence allows two VMs a Mac and no time-sharing), so cloud raea is Linux on NVIDIA
  graphics cards, the sdk/cloud image (docs/cloud.feature) on Modal first.

  Where the time goes today, measured 11-11 (raea 0.1.1-rc3 in sdk/cloud's image on Modal, bench/rerender's film as one
  render: 900 frames at 1920x1080 with six looks, openh264, 8 cores; ms a frame, steady frames):

    | GPU (Modal $/h)  | wall    | a frame | GPU wait | React | sync | first frame | GPU $ a film |
    | A10G ($1.10)     | 46.0 s  | 41.6    | 0.06     | 11.6  | 23.3 | 7.0 s       | 0.014        |
    | L40S ($1.95)     | 38.2 s  | 34.4    | 0.01     | 9.5   | 19.2 | 6.0 s       | 0.021        |
    | L4 ($0.80)       | 72.0 s  | 66.1    | 0.05     | 17.5  | 38.9 | 10.3 s      | 0.016        |
    | H100 ($3.95)     | 133.8 s | 137.7   | 74.6     | 12.7  | 43.0 | 7.9 s       | 0.147        |
    | H200 ($4.54)     | 127.0 s | 128.5   | 76.6     | 9.9   | 35.6 | 9.8 s       | 0.160        |

  What that says:
  - On a graphics card the GPU is idle (a wait of 0.01-0.06 ms): a frame is all CPU. React on QuickJS (no JIT; the Mac
    runs JavaScriptCore at 1.0-1.7 ms for the same frames, docs/rerender.feature RR0) and sync (the page synced and
    drawn and the 3D prepared: runtime/export/src/fed.rs's FrameLog) are 83-85% of it, the rest more of the main
    thread's own work (RAEA_TIMES' fed is 98% of the frame on the A10G).
  - So the host's CPU sets the speed, not the card: the L4 loses to the A10G on the CPU side alone, with the same idle
    GPU. Hosts under one GPU kind differ too: A10G hosts ran 1.52-2.11 s on one single-thread probe (Intel with AVX-512
    and AMD without; this Mac 0.81 s), and the same film took 46.0 s on one A10G host and 62.5 s on another. Every A/B
    here runs both builds on one host (LX1).
  - Compute cards are the wrong kind: an H100 or H200 has a small graphics part, so it's the one place raea waits on the
    GPU (75 ms a frame), 3x slower than an A10G at 10x the cost a film, and no NVENC.
  - More cores don't speed one render (4, 8 and 16 cores: 61.6, 62.5, 62.7 s on one host): a render is bound by its
    main thread and its render thread. Renders share a GPU well enough to fill a box: four at once on 16 cores made
    35 fps against one render's 14.4 (2.4x), but each frame's time outside the main thread grew from 17 to 60 ms.
  - The first frame takes 6-10 s: 0.4 s installing Bevy's plugins, 3.8 s of a page-only start thrown away when frame 0
    holds 3D (runtime/export/src/render.rs, FED_3D), then 3 s starting again with the 3D.
  - The clipping check can be most of a render: test/gpu/edit.tsx (576 blocks changing shape every frame, 90 frames)
    took 132.6 s, 96.1 s of it the check (systems/contact voxelizes every part every frame on one thread). So the
    "1229 ms a frame on an A10" recorded for that scene was mostly the check, not the GPU.
  - Agents' chunked renders cost double on Linux: six shot renders 3 at a time took 53.7 s plus a 56.6 s join (no ffmpeg
    in the image, so raea media concat re-encodes), each chunk's first frame 6.6-10.1 s.
  - A cold worker answers in 8.1 s (a T4: about 6 s Modal placing the container, 2 s our entrypoint).

  Bars for every scenario: the numbers come from bench/linux (LX0) on Modal; a change is judged by LX1's paired A/B (old
  and new builds interleaved on one host, at least two rounds each), never by runs on different hosts; the picture is
  unchanged (SSIM 1.000000 on decoded frames) unless a scenario says otherwise; a gate that can't be met goes to Shane with
  its measurements, never loosened in place; spend is logged with antsy (each round so far cost under $1).

  Rule: Measure on Linux, on the cloud's own machines

    Scenario: LX0. bench/linux: the rows, on any GPU kind and core count
      # Status 11-11: measured with a scratch version of bench.py (the table above and the findings); to commit.
      Given the plan
        | crate | file                  | lines | job                                                                                   |
        | -     | bench/linux/bench.py  | 220   | a Modal app on sdk/cloud's image: per GPU kind and core count it runs the rows below and prints one JSON line each; each GPU kind's result is printed as it ends (a kind out of capacity mustn't hold the others) |
        | -     | bench/linux/rows.sh   | 160   | the rows inside the container: the RR0 film as one render (bench/rerender/bench.sh MODES=one), N renders at once, the chunked film, test/gpu/edit.tsx, and a GPU-heavy scene; each with RAEA_TIMES, RAEA_FRAME_LOG, RAEA_STARTUP_TRACE, the clipping check's time, nvidia-smi's GPU and encoder use, and a single-thread probe of the host |
      Then `modal run bench/linux/bench.py --gpus A10G,L40S,L4 --cores 8 --rows film,x4,edit,chunked` prints a row per
        GPU kind and row with: wall, first frame, ms a frame split into React, sync, render thread, GPU wait, readback
        and encode, the clipping check, GPU and encoder use, the host's CPU (vendor, AVX-512, single-thread time)
      And a GPU-heavy row (bench/linux/heavy.tsx: ray-traced light, volumetric fog, 4K) shows the card's share, so the
        GPU kind is chosen on both a CPU-bound and a GPU-bound scene

    @needs:LX0
    Scenario: LX1. A paired A/B of any two raea builds on one host, and a Linux build of any commit
      # Status 11-11: the paired run is proven (rc3 against rc5 interleaved on one A10G host); the builder isn't.
      Given the plan
        | crate | file                  | lines | job                                                                                   |
        | -     | bench/linux/pair.sh   | 120   | two raea builds, A and B, run A B A B ... on one host, each row at least twice, and the median of each with its spread; a B that differs by less than the spread says "no change" |
        | -     | bench/linux/build.py  | 160   | raea for x86_64 Linux built at any commit in a Modal CPU container, its cargo target on a Modal Volume (an incremental build after the first), the tarball kept on the Volume for pair.sh |
      Then `bench/linux/pair.sh A=0.1.1-rc5 B=<commit>` builds B if needed and prints both builds' rows side by side from
        one host, with the host's single-thread time
      And every gate below is read from pair.sh, never from two separate runs

    @needs:LX0
    Scenario: LX2. Where a frame's milliseconds go, named
      # Modal runs containers under gVisor, where perf can't sample, so the split comes from raea's own spans.
      Given the plan
        | crate             | file                           | lines | job                                                                       |
        | raea-io-trace     | io/trace/src/spans.rs          | 140   | RAEA_SPANS=FILE: a Chrome trace of each frame's main thread (React's commit, the page's sync, each Bevy system over 0.2 ms) and render thread (extract, prepare, queue, each render graph node, submit), from Bevy's own spans plus raea's, written by the flight recorder |
        | -                 | bench/linux/spans.ts           | 120   | the trace folded into a table: each span's mean ms a frame, ranked, and its share of the frame (bun) |
      Then on the LX1 host the table names at least 90% of the main thread's and the render thread's ms a frame for the
        RR0 film and edit.tsx
      And the same trace taken on the Windows PC (the same portable path: QuickJS, Vulkan) agrees on the top five spans,
        so a fix can be found on a machine with a sampling profiler (samply) and proven on Linux

  Rule: The CPU side of a frame

    @needs:LX1
    Scenario: LX3. JavaScriptCore on Linux
      # QuickJS has no JIT: React is 11.6-17.5 ms a frame on Linux, 1.0-1.7 ms on the Mac's JavaScriptCore.
      # docs/ports.feature left "whether Linux needs another engine" to a number; this is the number.
      # Checked 11-11: Debian's libjavascriptcoregtk-6.0-1 and -4.1-0 (WebKitGTK 2.50.6, 31 MB each) both export the C
      # API (JSGlobalContextCreate, JSEvaluateScript, JSObjectMakeFunctionWithCallback, JSValueToNumber,
      # JSStringCreateWithUTF8CString) beside the GLib one; their licences are LGPL-2+ and LGPL-2.1.
      Given the plan
        | crate                 | file                                | lines | job                                                             |
        | raea-runtime-jsengine | runtime/jsengine/src/jsc_c.rs       | 380   | JavaScriptCore through its C API (JSContextRef, JSEvaluateScript, JSObjectMakeFunctionWithCallback) from libjavascriptcoregtk, loaded at run time, as Engine::Jsc off Apple |
        | raea-runtime-jsengine | runtime/jsengine/src/lib.rs         | 240   | Engine::default() on Linux: JavaScriptCore when the library loads, QuickJS when it doesn't (said once); BEVY_TSX_ENGINE still picks |
        | -                     | sdk/cloud/image/Dockerfile          | 75    | libjavascriptcoregtk-6.0-1 in the image                         |
      Then the licence is cleared first: libjavascriptcoregtk is LGPL-2.1, linked at run time and not shipped in raea's
        tarball; Shane decides whether that fits before any code
      And the conformance suite passes 19/19 on it on Linux, and the RR0 film's frames are identical to QuickJS's
      And on the LX1 host React is at most 3 ms a frame on the RR0 film and edit.tsx's React at most a quarter of today's
      And ES modules stay where they are: React's thread runs one bundled script; what needs the module loader keeps QuickJS

    @needs:LX2
    Scenario: LX4. The main thread's sync, cut where LX2 points
      # sync is 19-43 ms a frame on Linux against 6-15 ms on the Mac; LX2's table picks the work. Its plan table is
      # written when that table is in: no code is planned against a cost nobody has measured.
      Then on the LX1 host the RR0 film's sync is at most 10 ms a frame, with identical frames
      And each change lands with its pair.sh rows in its commit message

    @needs:LX2
    Scenario: LX5. The render thread off the critical path
      # One render on a slower A10G host spent 17 ms a frame outside its main thread, and four renders sharing the GPU
      # 60 ms: the render thread (raea_sys_frame::pipe) or the driver behind it. LX2's render-thread spans name the
      # files: what Bevy prepares again each frame for a scene that hasn't changed, and what four processes on one GPU
      # wait for.
      Then on the LX1 host a frame's wall is within 1 ms of its main thread's time, alone and with four renders at once
      And four renders on one A10G make at least 3.5x one render's frames a second

    @needs:LX1
    Scenario: LX6. No start thrown away
      # 3.8 s of the RR0 film's 7 s first frame is a page-only start that ends at frame 0's 3D (FED_3D) and is redone.
      Given the plan
        | crate               | file                          | lines | job                                                                     |
        | raea-runtime-export | runtime/export/src/render.rs  | 413   | a video whose first frame holds 3D starts with its 3D (frames_hold_3d on frame 0 first), so the page-only start and its FED_3D restart only happen for a piece whose 3D comes later; within the file's size today, as the crate is at its 2,000 lines |
      Then on the LX1 host the RR0 film's first frame comes at least 3.5 s sooner, with identical frames
      And a piece whose 3D starts later (bench/rerender's shot 3 alone) renders as before

    @needs:LX6,RR10
    Scenario: LX6b. The cloud keeps its pipeline cache, and the first frame comes in under 1.5 s
      # RR10 owns the cache (gpu/device's pipelines.rs, Vulkan through wgpu's PipelineCache); this keeps it across the
      # cloud's containers, whose home folder starts empty each time.
      Given the plan
        | crate | file                                 | lines | job                                                                  |
        | -     | sdk/cloud/deploy/modal/raea_cloud.py | 90    | a Modal Volume as the pipeline cache's folder, one folder per GPU kind and driver version |
      Then on the LX1 host the RR0 film's first frame comes at most 1.5 s after the spawn with a warm cache and 3 s cold
      And the six-chunk row's start-ups sum to at most a third of today's 50.4 s

    @needs:LX1
    Scenario: LX7. The clipping check never makes a render wait
      Given the plan
        | crate            | file                          | lines | job                                                                          |
        | raea-sys-contact | systems/contact/src/broad.rs  | 160   | a broad phase before the voxels: parts whose boxes don't touch on a frame are never voxelized or tested; a part's voxels kept by its shape and scale, so an unchanged part is voxelized once a render |
        | raea-sys-contact | systems/contact/src/clip.rs   | 178   | the frames checked on the cores the render leaves idle: Clipping takes each frame as it's drawn since SG4a (docs/stage-guards.feature), so off the render's thread |
      Then edit.tsx's clipping check takes at most its frames' own time (from 96.1 s), and RR0's film's at most 300 ms
      And a render's wall with the check on is within 3% of RAEA_CLIP_CHECK=0
      And the findings are identical on the RR0 film, edit.tsx and the contact fixtures (systems/contact's tests)

    @needs:LX1
    Scenario: LX8. A raea built for the cloud's CPUs
      Given the plan
        | crate | file                          | lines | job                                                                         |
        | -     | bench/linux/build.py          | 160   | the builder's variants: x86-64-v3 (every host measured has AVX2), fat LTO, and profile-guided (cargo-pgo, trained on the RR0 film and edit.tsx) |
      Then each variant is paired against the release build on one host, and a variant ships (as the image's raea, or
        as the Linux tarball) only where it's at least 10% faster a frame with identical frames

  Rule: Encode and join without the CPU

    @needs:LX0,RR3,RR8b
    Scenario: LX9. NVENC and a join that doesn't re-encode, measured in the cloud
      # docs/rerender.feature owns the code (RR3's segments, RR8b's NVENC); this is their cloud gate.
      Then with four renders on one A10G the encoder is NVENC and the frames' CPU time drops by openh264's share
      And the chunked row's join takes at most 1 s (from 56.6 s), without ffmpeg in the image

  Rule: The cloud worker

    @needs:LX0
    Scenario: LX10. The GPU kinds, chosen by what a finished film costs
      Given the plan
        | crate | file                               | lines | job                                                                    |
        | -     | sdk/cloud/deploy/modal/raea_cloud.py | 90  | the default list ordered by LX0's cost a film (A10G, L40S, L4 on 11-11's numbers, replacing L4, A10G, T4), and a compute card (H100, H200, B200, A100) refused with the measured reason unless RAEA_CLOUD_GPU_ANY=1 |
      Then LX0's table for the RR0 film and the GPU-heavy row is in this scenario, with each kind's cost a film
      And the list is re-chosen whenever LX3-LX7 change where a frame's time goes

    Scenario: LX11. The image runs the raea it was asked for
      # 11-11: RAEA_VERSION=0.1.1-rc5 built an image whose raea said 0.1.1-rc3: Modal reused its cached layer.
      Given the plan
        | crate | file                                 | lines | job                                                                 |
        | -     | sdk/cloud/deploy/modal/raea_cloud.py | 90    | the version made part of the image's identity (a layer built from it, so a new version builds a new layer) |
        | -     | sdk/cloud/image/entrypoint.sh        | 110   | RAEA_EXPECT_VERSION checked against `raea --version` at start: a mismatch is an error, never a quiet older raea |
      Then deploying with a new RAEA_VERSION serves that version (/v1/health says it) on its first request

    @needs:LX1
    Scenario: LX12. A cold worker in under 4 s, and hidden from the agent
      Given the plan
        | crate | file                              | lines | job                                                                       |
        | -     | sdk/cloud/image/entrypoint.sh     | 110   | a render worker (RAEA_CLOUD_ROLE=worker) skips agents and skills, and checks the GPU beside the server's start, not before it |
        | -     | sdk/cloud/src/index.ts            | 220   | the client wakes the GPU worker (GET /healthz) when an agent session opens, so the first render meets a warm worker |
      Then ten cold starts on Modal answer /healthz in at most 4 s at the median (8.1 s on 11-11)
      And in CL12's smoke test the agent's first render never waits for a worker to start

    @needs:LX5
    Scenario: LX13. As many renders a GPU as it holds
      Given the plan
        | crate | file                              | lines | job                                                                       |
        | -     | sdk/cloud/server/jobs.ts          | 200   | the worker's jobs run N at a time (RAEA_CLOUD_SLOTS, by default the number LX0 measures best for the GPU kind and cores), queued past that |
      Then on one A10G with 16 cores the worker's frames a second at its default N are at least 3x one render's
      And RR5's workers inside one render (a long video's segments) take the same slots

  Rule: Keep it fast

    @needs:LX1
    Scenario: LX14. Every release candidate is paired against the last release
      Then before a release's notes are written, pair.sh runs LX0's rows for the candidate against the previous release
        on one A10G host, and a row more than 5% slower a frame or at its first frame fails the release pass with the
        rows (by hand in the release lane while CI is on hold)
