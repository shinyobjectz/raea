Feature: ZeroHeap: a running world allocates nothing; its memory is fixed at load
  Set by the user 2026-10-18: "how do we have no memory at all, like a world with no memory. what is even causing the
  1mb memory … turn this into a feature plan, called zeroheap. how do we have memoryless."

  "Memoryless" here means constant memory: everything a game, timeline or stage needs is allocated when it opens, and
  the frame loop never allocates, frees or grows anything again. A frame is already a pure function of the frame before
  and the input (games.ts folds, machine.ts, baked rows), so it can write into memory it was given instead of making new
  memory. Console engines work this way; raea's architecture is unusually well suited to it.

  Rule: What the ~1 MB is (measured 2026-10-18, commits 0ec4a3f, d2fc732, 28f3221)
    Scenario: It isn't a leak
      Given a 300 s gate run of pong or the platformer
      Then RSS minus reusable pages wanders about ±1 MB in steps after the first minute (+1.7 and +1.1 MB in the last
        runs; −9 to +0.5 MB earlier), while footprint falls and the JS heap after a collection stays at about 3.4 MB
      And the steps come from three sources
        | source | why it moves RSS |
        | garbage churn | pong makes ~249 KB of short-lived JS objects a frame (~30 MB/s, was 769 KB/frame); JSC's allocator (libpas) frees them but hands pages back to macOS lazily, in chunks, so RSS steps at 16 KB-page granularity |
        | high-water marks | Rust Vecs, the glyph atlas, the mixer's block cache and wgpu staging grow to a peak once, then hold: flat after warmup, but each first hit is a step |
        | not ours | the Metal driver, CoreAudio, dyld and ObjC autorelease pools allocate on their own schedule |
      And so RSS measures the OS's bookkeeping, not raea's behaviour: ZeroHeap's gate counts allocations instead

  Rule: Smoke test, 2026-10-18: worth trying (sdk/react/bench/zeroheap: step_bench.js, gc_log.py)
    Scenario: What GC costs a live game today (pong, 60 s, JSC_logGC, load ~38)
      Then 186 collections (185 eden, 1 full), 3 a second; pause total 116.6 ms, so 0.016 ms a frame on average; eden
        pause p50 0.55 ms, max 5.22 ms; 122 KB of garbage freed a frame
      And frame work p50 0.92 ms, p99 2.41 ms, 4 missed. The worst frame (507 ms) was the loaded machine, not GC: no
        pause exceeded 5.2 ms
      And so for pong GC is cheap on average, but its spikes (up to 5 ms) can tip a heavier frame past the 8.3 ms
        budget, and they grow with what a game allocates

    Scenario: The Z2 idea against today's shape, in JavaScriptCore's shell (7,200 frames, N moving entities, load 24-38)
      Then with the JIT on, FTL off, as raea's engine runs on the Mac
        | N | shape | p50 | p99 | max | GCs (pause total) |
        | 2,000 | fresh objects per frame (today) | 0.034 ms | 0.217 ms | 10.30 ms | 5 (22.3 ms) |
        | 2,000 | fixed double buffer | 0.021 ms | 0.053 ms | 2.26 ms | 0 |
        | 20,000 | fresh objects per frame | 0.344 ms | 1.550 ms | 22.54 ms | 40 (203.0 ms) |
        | 20,000 | fixed double buffer | 0.218 ms | 2.471 ms | 34.13 ms | 0 |
      And with the JIT off (jitless, as on iOS) the buffer allocates nothing and its worst frame is better (2.21 -> 0.55
        ms at 2,000; 48.2 -> 22.6 ms at 20,000), but its p50 is slower (0.112 -> 0.182 ms at 2,000), because an
        interpreter pays more per typed-array access than per allocation
      And the tails at 20,000 were dominated by the machine's load (24-30), so they need a quiet rerun

    Scenario: Verdict
      Then ZeroHeap is worth trying: it removes GC entirely (deterministically, not statistically), is 1.6x faster at
        p50 and 4x better at p99 with the JIT at a realistic entity count, and cuts the worst frame 4.5x
      And the gains are in the tails and in memory determinism. Average JS time is already small, so this is mainly
        about never missing a frame and never growing
      And iOS needs care: jitless typed-array loops cost more per access, so on the phone the step's layout must be
        measured (Z2 includes an iOS check), or the hot step compiled ahead of time (WASM, or a Rust step) later
      And the step is only part of the garbage: today's 122 KB a frame is mostly the lite view and the fold's objects,
        so Z2 and Z3 together are what make a game's JS allocation reach 0. Rust, GPU and audio allocations aren't
        measured yet (Z1)

  Rule: The gate is bytes allocated per frame after warmup = 0, counted exactly, per layer
    Scenario: The zero-allocation gate (Z0)
      Given a game (pong, the platformer, the 3D game) or a baked timeline (titles, ts1) after 120 warmup frames
      Then over the next N frames these counters are all 0
        | layer | counter |
        | JS | JSC heap bytes allocated (the GC log's allocated-since-collection; JSC_logGC or the C API heap stats) |
        | Rust | a counting #[global_allocator] wrapper: allocations and bytes on the frame threads (main, stage, JS loop) |
        | GPU | wgpu resources created per frame: buffers, textures, bind groups, pipelines |
        | audio | voices and blocks allocated on the mixer thread (the Firewheel core) |
        | bridge | bytes of JSON or strings crossing JS to Rust |
      And RSS is still printed, but it is flat by construction and no longer the judge
      And the gate runs in sdk/react/bench/games/gate.sh and bench/ci.sh, and fails naming the layer and the call site

  Rule: Phases, in order of the bytes they remove; each is done when its counter reads 0 and render still equals play
    Scenario: Z1 Count every byte (instrument first, change nothing)
      Then the counting allocator, JSC heap deltas per frame, GPU creation counts and mixer counters land behind
        RAEA_ZEROHEAP=1, with each allocation site attributed: a backtrace sample per site, or the JS allocation
        profile from JSC's heap snapshot
      And a table of today's per-frame allocations, by layer and site, is written into this file

    Scenario: Z2 Game state as one fixed buffer (the largest: most of the 249 KB/frame)
      Then a game's state lives in a preallocated typed buffer laid out as arrays per field (as bitECS does), not as
        fresh JS objects. The step reads buffer A and writes buffer B, then they swap: still pure (frame N+1 depends
        only on frame N and the input), and it allocates nothing
      And state machines (machine.ts snapshots) and entity sets (stepAll) use fixed slots with free lists, sized at load
        (MAX_ENTITIES declared by the game, with a clear error when exceeded)
      And checkpoints (games.ts fold) become fixed-size memcpy of the buffer into a preallocated ring
      And agents keep writing ordinary TSX: an authoring API (typed fields, useGameState) hides the buffer, and a lint
        or test flags a step that allocates
      And done: JS bytes allocated per frame by the step = 0; replay.sh and render-vs-play SSIM 1.0 hold

    Scenario: Z3 The view has no per-frame virtual DOM
      Then each TSX component is compiled once (at bundle time) into update functions bound to the state buffer, as
        Solid or Svelte do fine-grained updates, so a frame writes only the host fields that changed and builds no
        element tree to diff
      And the lite runtime's slot paths, hook arrays and children maps (sdk/react/js/lite) are gone from the frame path
      And done: JS bytes allocated per frame by the view = 0

    Scenario: Z4 The bridge carries no strings
      Then per-frame ops (create, update, remove; today JSON at 0.15-1.7 KB a frame) go through a fixed ring buffer of
        typed ops in Rust-owned shared memory, which JSC wraps without a copy (JSObjectMakeArrayBufferWithBytesNoCopy);
        the mirror and the ECS read ops in place
      And bevy-react's JSON bridge stays only for edit-time and React-path compositions
      And done: bridge bytes per frame = 0 and Rust allocations from op parsing = 0

    Scenario: Z5 Rust frame threads allocate nothing after warmup
      Then a per-frame bump arena (reset each frame) replaces temporary Vecs and Strings on the main, stage and JS loop
        threads; persistent Vecs are pre-sized; no format!() or serde_json on the frame path
      And done: the counting allocator reads 0 on frame threads after warmup

    Scenario: Z6 GPU and audio pools are fixed at open
      Then quads keeps one set of page textures per size (already partly: ad90377), a fixed glyph atlas, reused staging
        belts and bind groups, and no per-frame buffer creation; rdb's live engine likewise
      And the mixer has a fixed voice pool and block budget, with voice stealing instead of growth
      And done: GPU resources created per frame = 0; mixer allocations per frame = 0

    Scenario: Z7 Prove it and keep it
      Then the zero-allocation gate (Z0) runs in CI on every commit, and a regression names the site
      And the 300 s gate's RSS growth after the first minute reads 0 ± one page per layer; GC collections during play: 0
      And iOS: the same games on the iPhone hold a fixed footprint (373 MB peak today) with no growth

  Rule: What ZeroHeap buys, and what it doesn't
    Scenario: Is this ultimate performance?
      Then it is the ceiling for frame-time stability, latency and memory: no GC pauses at all (the likely cause of the
        last few missed frames under load), no allocator in the hot loop, no growth over hours, and memory that is
        proven, not measured
      And it is not, by itself, the ceiling for throughput: average frame work is already small (pong JS p50 0.15 ms,
        work p99 1.2 ms against an 8.3 ms budget); what remains is GPU compositing (~4 ms a frame in void), text layout
        and shaping, video decode and present timing, which ZeroHeap doesn't touch
      And the full "ultimate" also needs zero copies (pages, video and 3D stay on the GPU: largely done, 615d301 and
        47181d7) and zero waits (the stage never blocks on present: done, ad90377). ZeroHeap is the third leg

  Rule: The far version: a world whose memory can't grow, by construction
    Scenario: Fixed-width symbolic memory (HDC)
      Then a world's persistent symbolic memory (what happened, what matters: agent memory, perception summaries,
        playtest knowledge) is a fixed-width hypervector; bundling and binding never change its size, so it cannot grow
      And this fits symbolic and perceptual memory (docs/the roadmap Track K, meaning/), not exact physics or game state,
        which stay in Z2's fixed buffers
