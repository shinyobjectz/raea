Feature: Lightspeed: raea as fast as anything could be, with the best quality, measured on everything
  Set by the user 2026-10-18: "I want Lightspeed and how do we get to Lightspeed? And like void, when I think of void is
  zero, meaning like the fastest that could ever be. Obviously it's crazy, but that's kind of the fun of it, and it's
  worth being experimental."

  void means zero: zero waits, zero copies, zero allocations, zero missed frames, zero time-to-first-frame, zero
  difference between play and render. Each will never quite reach zero, which is the point: every row below has a
  measured value, a target, and a path toward zero.

  Owner: the lightspeed agent (session "lightspeed"), orchestrated by an agent session. It loops: pick the row with the biggest
  measured gap, attribute where the time or bytes go, take the structural lever that moves it, measure before and after,
  commit, update this file. Individual spikes are knocked down only after the structural levers. Companion plan:
  docs/zeroheap.feature (lever L4).

  Rule: The loop (how lightspeed works this file)
    Scenario: One turn of the loop
      Given the scoreboard below and bench/ci.sh's history (~/Library/Logs/raea/ci/history.jsonl)
      When lightspeed starts a turn
      Then it picks the row whose value is furthest from its target, weighted by how much a person feels it: feel >
        throughput > resources > quality polish
      And it attributes first: a per-frame trace of where every millisecond or byte goes (the flight recorder,
        quads/rdb GPU pass timestamps, antsy spans, JSC_logGC, ZeroHeap's counters). The biggest bar picks the lever,
        not the last complaint
      And it measures before and after under whatever load the machine has, never waiting for a quiet one (the user,
        2026-10-18: "we are still waiting way too long"): a lever is judged by a paired, interleaved A/B in one go, or
        by counts that hold under load (CPU time, instructions, allocations, frames, timeouts), and a row is marked
        "under load N". bench/ci.sh's nightly pass (03:00, RAEA_LIGHTSPEED_QUIET=1) re-measures every row quietly and
        the scoreboard shows that value beside the day's
      And it makes the structural change, checks the quality rows didn't move (render = play, parity, audio exact),
        and commits only its own changes: a private index from HEAD plus its exact edits, compile-checked in a scratch
        worktree (cargo check --locked) before the branch moves, with the numbers in the message
      And it updates the row here (value, date, commit) and appends a line to the log at the bottom
      And when an experiment doesn't pay, it records the numbers and reverts the change. A measured no is a result

  Rule: Lightspeed's goals (set 2026-10-19, after the four multi-day levers; the user: "just focus on lightspeed work")
    Scenario: Who owns what now
      Then lightspeed owns performance and optimization end to end: it builds its levers itself, not only measures them
        (the user moved an agent session off them, 2026-10-19)
      And an agent session owns docs/vite-in-raea.md: A, Bun's build crates in process (the 7 places raea starts the bun CLI);
        B, bun_jsc as raea's JS engine, an evaluation; C, the page engine's 11 gaps from jetway. Lightspeed measures B
        the way T3 was (check.sh titles full and incremental, pong's three jsheap fixtures) and counts its bytes against
        R7; the exported player keeps the system JSC unless the user accepts those bytes; B tells lightspeed before it
        changes the C API the host calls (Z4 builds on it)
      And not lightspeed's: Q6 (the new-stack eval's agent), the page engine's API coverage (C)
    Scenario: The goals, in order (each done when its check holds and its row is updated)
      Then G1, R7: met 2026-10-19 (9b8f768). Every exported game <= 65 MB with no feature dropped: media's soft backend split into
        soft-audio and h264, so the player leaves openh264 out (~0.86 MB of code) and decodes video on the platform.
        Done: total_mb <= 65 for pong, the platformer, arena3d, the 50-game and the void game; export.sh SSIM >= 0.999
      And G2, Z1: count every layer. The Rust counter landed (f910193); bridge bytes were already counted (commit_kb). A counting allocator in raea-motion (sdk/react/src/zeroheap.rs, RAEA_ZEROHEAP=1,
        per thread), bridge bytes per frame, wgpu resources created per frame, mixer allocations. Done: play's report
        carries each layer's count a frame for pong, the platformer, arena3d and titles, and zeroheap.feature has the
        table by layer and site
      And G3, Z4: steps 1-3 landed (f910193, f3b0005, 417cc5a: updates read in place): Rust allocations applying ops a frame,
        pong 172 -> 29.3, arena3d 84.7 -> 12.1, the platformer 328.7 -> 227.8 (its updates' style text re-parsed by Stylo).
        Closed there on numbers (2026-10-19): a whole extra JSON parse and stringify of every batch costs <= ~1 KB of JS garbage
        a frame (arena3d 53.5 -> 54.3, pong within noise: < 2% of the layer), and the Rust side's parse is already read in
        place; what a binary bridge would remove is that and the batch's one string copy (~2 allocations a frame). Not worth
        its two new codecs now; reopened if the JS layer's other garbage gets that small. The bridge carries no strings: per-frame ops through a typed ring in Rust-owned shared memory
        (JSObjectMakeArrayBufferWithBytesNoCopy), applied in place, JSON kept for edit time. Done: bridge bytes = 0 and
        Rust allocations from op parsing = 0 on pong, with replay x3, check.sh incremental, audio.sh, video.sh ts1 holding
      And G4, Z2: met 2026-10-19 another way (3c36ba8). An identity fold made pong's step+fold garbage too, so the cost was
        the SDK's per-call objects (useGameEvents, useLocalTime, validate), not the game's state: step+fold 9.2 -> 1.4 KB a
        frame, 30 -> 12 GC/min, games unchanged; a fixed buffer isn't needed for it. Was: a game's state in one fixed double buffer, an opt-in API beside today's (games written today keep
        working), pong first. Done: pong's step+fold at the empty edit's floor, R2 down by step+fold's 30 a minute, iOS
        checked (jitless typed-array access)
      And G5, met 2026-10-19 at the floor (Shane's call: done at these numbers, zero would take another renderer):
        every Rust thread's allocations a frame, pong 6,797 -> 317 (630cfe4 generated sounds found by key), platformer 5,304 -> 470, arena3d
        12,440 -> ~2,100 (812ddb3 no attribute snapshot for a style write without [style] selectors; style text in a kept
        buffer; 1df1853 3D holders made see-through only after their props change; 507b491 Stylo restyles an element from its last style's reset structs, fixed up exactly; 46937e7 glyph draws pushed as placed; 2466dab a frame's settle skips the settings; ce4c43f JS
        calls without Rust allocations; 9b64fa6 sound lists kept; 54a242b a live 3D frame reads its scene without copying names; 813c380 Stylo's builder keeps a struct shared when set to the value it has; 6d50d56 var()
        substitution borrows a value's text; 82e657b Stylo keeps what each substituted value parsed to; 9b2e358 GPU submissions' names static; e888288, 7abc06d paint's effect, clip and background paths
        only when drawn; 865e364 Stylo's shorthand cache kept; af933b2, 8ccacf3 Blitz paint's box paths only where drawn;
        2cee461 quads' pass lists kept; 3c122a0 sounds share their names; 1c35fbc audio queries kept; 8517f79 the live React app on one thread; d50c696 the frame event written in place, ramp keys,
        the mixer's sounds borrowed; 34fe69b the GPU pass timer and the element query kept; 354d73a a known live 3D frame diffed in place from fixed arrays, after 8d7a96f's columns-only rows, both checked by RAEA_LIVE3D_CHECK; (e1218fb bind groups kept, Z6; 417cc5a updates read in place; 1499b4b 3D props parsed once per change;
        8f64fb5 tracks shared; 52f7415 box paths pre-sized; b0736b1 the recorder's shape tests without Vecs) (3b7c4de quads binning, 3fbe7ac the 3D scene walked once, 141fa1e sound props in place, 3eb4bdf an
        atom kept, 397487a quads' frame lists kept; frames byte-identical). pong's rest: Stylo's restyle 38%, Blitz's paint
        (paths, layers) 32%, quads 7%, the op parse 7%, wgpu bind groups and encoders 6%, sound 6%: the next levers are
        structural (L6's incremental paint record, Stylo's restyles of var()-styled moving parts); arena3d's live 3D path now has
        its own diff (8d7a96f, 354d73a); its rest is the same Stylo, paint and wgpu work as pong's.
        Measured noes (2026-10-19): skipping an attribute set to the value it has (909 -> 909: the ~30 elements restyled a
        frame are the moving parts themselves, not repeats); pong's Stylo share is 47% of sampled allocations, 19% of it
        var() re-substitution (the moving parts' `background: var(--brand-*)` re-parses the background shorthand each
        restyle), the rest StyleBuilder's copy-on-write style structs: inside Stylo, not reachable from raea's side.
        Where it stands after e888288 (pong, 200 sampled sites): Stylo 75% (each restyled element's new
        ComputedValues and copied style structs, and var() shorthands re-parsed), wgpu's own command encoding and
        submission ~15%, raea's code what's left (the mixer's frame sync, the JS call's argument string). Zero needs
        Stylo's cascade itself changed (a parsed-substitution cache, structs reused in place): the next lever, a
        decision of its own rather than more of this pass.
        Taken (82e657b, 6d50d56, 813c380): pong 652 -> 448. What's left in Stylo is each restyled element's new
        ComputedValues and one copy of each struct whose values really differ from where it starts; removing those
        means starting a restyle from the element's last style when its declarations are the same ones (incremental
        style), a redesign of Stylo's cascade. arena3d's rest is Bevy's renderer: bevy_pbr's preprocess and
        mesh bind groups made each frame, its render systems' task-pool scopes, wgpu's command encoding, and the
        render diagnostics recorder (kept: it's the trace's bevy_passes).
        Measured no (2026-10-19): style-attribute declarations parsed once per text and pushed as clones (platformer
        628 -> 642: a shorthand's longhands cost as much to clone as to parse); in-place set_style_property stays out
        too (a block Stylo has put in the rule tree is copied whole, and changing it in place would serve stale
        rule-cache structs).
        Measured no (2026-10-19): Bevy's CPU mesh preprocessing (PbrPlugin use_gpu_instance_buffer_builder: false)
        takes arena3d 2,245 -> 1,526 but draws differently (golden 04-06 SSIM 0.89-0.93 < 0.97); the GPU path's
        per-frame bind groups need bevy_pbr's own cache (a patched crate), not the CPU path.
        Measured no (2026-10-19): bevy_render patched to keep bind groups by what they bind (two frames' worth):
        arena3d 2,245 -> 2,219, ~25 misses a frame. The preprocess and indirect-parameter groups bind buffers and
        sizes that follow the frame's visible meshes, so they are new bind groups each frame; not worth a vendored
        bevy_render.
        arena3d's rest (2026-10-19, after 1df1853): steady at ~2,130 a frame over 10, 20 and 40 s; Bevy is 86% of its
        sampled allocations (its command encoding and submit, bevy_pbr's preprocess and mesh bind groups, task-pool
        scopes, query transmutes), raea's own 7%. Measured no (2026-10-19, the fork tried): one command encoder shared by a frame's render systems
        (bevy_render vendored) 2,109 -> 2,091; with the bind-group cache's 26 earlier, a Bevy fork buys ~2% of
        arena3d. Its rest is wgpu-core's tracking of the resources a frame's passes use (it grows with what's bound,
        not with encoders) and bevy_pbr's per-view preprocess groups: not a patch, a different renderer.
        arena3d 2,138 -> ~2,100 (4c6f552 3D props updated in place). Then Z5 and Z6, driven by G2's table: Rust frame threads allocate nothing after warmup; GPU and audio pools
        fixed at open (no per-frame buffers or bind groups, a fixed glyph atlas, a fixed voice pool). Done: those
        counters 0; F2's raea-side share (its per-frame buffers) and R4 re-measured
      And G6, L2: measured 2026-10-19 (b646140): frames still cross, 3.1 MB a frame, but the two copies are ~0.13 ms of a
        ~3.8 ms frame and every frame waits ~3.2 ms on the media engine, so zero-copy can't move T1; left undone. Was: measure
        export readback bytes first (never measured). If frames still cross to the CPU on any export
        path, hand the encoder the GPU surface (IOSurface / CVPixelBuffer into VideoToolbox). Done: readback bytes = 0,
        T1 and T2 measured against the H.264 engine's floor, Q3 unchanged
      And G7, F6: root cause found and fixed 2026-10-19 (6d4d9a6): wgpu's clear of the first full-page group target, made on
        the card's frame (a 3.24 ms pass in a Metal trace); now made and cleared at open. GPU side gone in the trace; the
        quiet nightly's F6 confirms the frame times (G8). Earlier: the hitch
        is first use, not clocks: the card's text drawn at 1% in frames 10-30 removes
        it (3/3), a separate throwaway page warmed at open doesn't (3/3, a measured no, reverted). Open: an engine-side warm
        of the page's own glyphs-in-a-group pass. Was: its work drawn once offscreen in the first second
      And G8, the quiet confirmations: R6's open-timeline idle (0.017 W under load) and pong's watts against L8's
        "halved", F3 at load 20, Z7's CI gate once G2-G5 land, all on the nightly quiet pass. Not met (2026-10-20): no
        quiet nightly has measured anything yet (see "The quiet nightly that didn't run")
      And G9, quality polish: Q2's last 2. Met 2026-10-19: the full corpus has 0 failing (95 exact, 75 ok)
    Scenario: The second goal list (set 2026-10-19 evening, after G5: the best moves aside from Bevy, in order)
      Given the founder's list: 1 compositor-only motion, 2 an incremental paint record, 3 the quads compositor on wgpu-hal,
        4 a binary path for 3D transforms, 5 Bevy trims by configuration only, 6 frame time and power as the scoreboard
        with allocations as a guard, 7 the Stylo and Blitz patches offered upstream
      Then 1, step 1 (5843067): a left/top-only change moves the box where it's laid out (no restyle, no relayout),
        exactly as Taffy places it, checked by RAEA_MOTION_CHECK=N (every node's layout and paint overflow); found on
        the way: the frame marker's data-raea-* attributes made the whole composition restyle every frame (574b6b5:
        pong 317 -> 161), and a style attribute's change restyled the parent's subtree (e775d90, element-only:
        platformer 505 -> 395, pong -> 149). Transform and opacity changes now cost their element's restyle alone;
        moving them past Stylo too (the computed style patched) is step 2
      And 5 (c51a8f1): Bevy's render diagnostics behind RAEA_BEVY_PASSES (47 allocations a frame on arena3d, kept on
        since both apps always trace); shadow maps were already off for lights without them
      And 6 (ee8cd00): bench/ci.sh's zeroheap gate holds pong 172, platformer 455, arena3d 2,354 (today's +15%);
        frame time and power stay G8's quiet nightly pass
    Scenario: The quiet nightly that didn't run (G8, read 2026-10-20 05:17)
      Given the 03:00 full tier (bench/ci.sh, launchd) on 2026-10-19 (5aa81a489) and 2026-10-20 (3598bf97a)
      Then both stopped at their first gate: disk FAIL (the data volume under RAEA_CI_MIN_FREE_GB=12; on 10-20 it was
        full, ENOSPC machine-wide), and finish() then died on `NOTQUIET: unbound variable` (set -u: NOTQUIET is first
        set at line 214, after the disk check at 168), so neither wrote a summary, history or results row
      And ~/Library/Logs/raea/lightspeed/results.jsonl has no row dated 2026-10-20 and none with quiet=true; the only
        quiet=true rows in ci/history.jsonl are quick-tier games:pong (attempts 1-2, 1 missed frame each), not G8's set
      So R6, F2/F3/F6, R1.js / R1.rust, T1-T3 and Q1-Q4 have no quiet values; nothing in a quiet run contradicts the
        under-load rows, and nothing confirms them: G8 stays open
      And it needs two things before the next 03:00: NOTQUIET="" set with FAILS/FLAKIES (line 91) so a failed disk gate
        reports instead of crashing, and 12 GB free on the data volume (9.3 GB at 05:17)
    Scenario: What waits on the user
      Then F1 in void's window needs a visible window the user agrees to (lightspeed never shows one unasked)
      And R5 and Q5 need the iPhone unlocked and connected
      And R7 above 65 MB, if B ships a vendored WebKit in the player, is the founder's call on the counted bytes
    Scenario: What stays blocked, with its reason
      Then T1, T2, T4: the media engine's encode floor (~275 fps at 1080x1920 for H.264, 261 for HEVC Main10), one
        engine on this chip; G6 is the only lever left on our side
      And T3: the system JavaScriptCore (~50 us a titles frame against bun's ~13): B is the lever
      And F2's CPU half (Bevy's render app) and F6's clocks (the GPU's), R3's idle (Metal's baseline): upstream or the
        hardware's, re-checked when Bevy or macOS moves

  Rule: The scoreboard (values as of 2026-10-18 evening; bench/scoreboard.sh prints the live one)
    Scenario: Feel: what a person notices first
      Then these rows are tracked
        | id | metric | harness | value | target |
        | F1 | input to screen p99, games in void's window | apps/void/bench/probe.rs, gates.py; offscreen: bench/lightspeed/feel.py | 28.0 ms on 60 Hz (29a5417); offscreen 120 fps 10.6 ms (not quiet) | <= 1 refresh + 1 game frame |
        | F2 | frame work p99 / p99.9, games at 120 fps | clean run (load 3-5, no other jobs, 02b4852): arena3d p99 3.87 / p99.9 4.58 ms (its steady 3D frame; Bevy's render ~2 ms), pong 1.72 / 2.80, platformer 1.12 / 1.52 | settle race fixed (aa4f78a: p99.9 507 -> 18-36 ms paired); arena3d p99 3.7-4.1 -> 2.7-3.2 ms (single-threaded Bevy, 4353e23), under load | p99.9 <= 4 ms |
        | F3 | missed frames in 5 min, pong and platformer | met: 0 missed frames in 3 x 60 s (clean run, load 3-5, 02b4852) | 3 in 2 x 60 s at load 7 (aa4f78a); 0 needs a quiet run | 0 at load <= 6 and at load 20 |
        | F4 | scrub / seek p99 | met: 1080p (ts1) 25.8 -> 12.5-14.6 ms (range copies 5cd890d, font reload skipped on seeks b0f6e8e); 4K (k4) 61.1 -> 16.6-26.6 ms (f83ae45, b4e7cc8); seeks = render 1.000000, load 3-6 | 1080p 25.8 ms (quiet, e0498ab); 4K 52.2 -> 17.7-28.2 ms, p50 18 -> 5 ms from a ProRes 422 copy (f83ae45, paired, load ~15; 3.1 GB for k4's 61 s, VMAF 99.78) | 1080p <= 16.7 ms; 4K <= 33 ms |
        | F5 | spawn to first frame: game, timeline, bake, void launch, void object open; a 3D export's first launch | game 70, timeline 81, bake 28 ms (44028f1: frame 0 before the sound), void object open 74-81 ms; void launch 1187 -> 265-272 -> 206-220 ms (an agent session's 56cc5ac: the launch object opened before the app mounts, and the stage paints frame 0 before its sound; paired, under load; video.sh ts1 PASS, seeks 1.000000), void launch's ~200 ms accepted by the user (2026-10-19), masked by a launch reveal instead of restructured | void launch 1187 -> ~310 ms (under load 10-27), 275 ms at load 3-6 (f894348's run: game 291, timeline 217, bake 30, void open 76 ms); exported 3D game: first paint 55 ms, playable 343 ms warm / 4.4 s cold (Metal compiles ~35 pipelines) | each < 100 ms; void launch ~200 ms with a pixel reveal over its first frame |
        | F6 | first-use hitches: frames past the first second over 2 ms | 37 in 60 s (max 7.26 ms), all pong's score card appearing (its first GPU work at clocked-down clocks); platformer 0; arena3d's steady frames are F2's | pong 2-3 in 8 s, max 7-8 ms: the score card's full-page group arriving with the GPU clocked down (attributed: no cache or buffer misses there); pong's page pass 3.48 -> 2.82 ms GPU (4b76f93), then 2.99 -> 2.45 ms (d06d52e), groups p90 0.93 -> 0.35 ms (eae9f02), paired under load | none > 2 ms |
    Scenario: Throughput
      Then these rows are tracked
        | id | metric | harness | value | target |
        | T1 | export of the 20 s edit (ts1), warm; a repeated or ahead-of-time export | 2.456 s quiet (export.py on HEAD, cache off since 4fce2f0); 0.04 s from the export cache; the H.264 engine's floor (~2.4 s) | 2.51 s cold (the H.264 encoder's ~250 fps floor, ~2.4 s); 0.04 s from the export cache; void exports an idle object ahead; titles 10.7 -> 3.6 s (three-box blur, 00d156f) | <= 1 s |
        | T2 | 3D export, 600 frames | 2.702 s quiet (export.py, cache off): the H.264 engine's floor plus ~0.3 s Bevy setup | 3.1-3.4 s at load 7 (process: ~0.3 s Bevy init, ~0.2 s other setup, frames 3.6-4.3 ms each, 3.3 of them feeding the H.264 engine): the encoder's floor, as T1's | <= 1 s |
        | T3 | full bake of a 20 s edit / incremental re-bake | quality.py; RAEA_BAKE_TRACE; check.sh titles | full bake JS 19.8 ms, incremental 25-27 ms (e0452c6: one Intl.NumberFormat per locale+options; ec41abe: the range bake; was ~50 / 47-58); void's re-bakes 23 / 29 ms. Blocked: the system JavaScriptCore runs titles' frames at ~50 us (bun's JSC ~13), and each runtime opens in ~8 ms | 600 frames 107-116 -> 49-54 ms on 4 runtimes (0f36ae7); a cached bake reads in 4-17 ms (324006d) | <= 50 ms / <= 10 ms |
        | T4 | batch: renders per hour, all cores busy | 1553 renders/hour (titles at 1080x1920; was 311 before the blur work), encoder-bound | 311/h at 4 processes, x1.55 over one (not quiet); one titles render 15.9 s | encoder-bound on every core |
    Scenario: Resources
      Then these rows are tracked
        | id | metric | harness | value | target |
        | R1 | bytes allocated per frame after warmup, per layer | Rust (Z1): pong's op apply 74.6 allocations, 9.6 KB a frame (f3b0005: put in place; f910193: the mirror; was 172, 17.0). JS: pong 33.4 KB/frame, step+fold 1.4 (3c36ba8: useLocalTime, validate; f3b0005: useGameEvents; was 9.2), view alone ~14-18 (c59c6f7); animate 6, twin 39, titles 26 | jsheap.py on pong's fixtures, nightly since 419f39f (R1.js and R1.rust) | 0 |
        | R2 | GC collections during play | feel.py, jsheap.py (JSC_logGC) | pong 63-66/min (was 192 before Z3), step+fold 12 (was 30), view alone ~39-48 | 0 |
        | R3 | void RSS: idle / pong / titles; and footprint | footprint 215 / 236 / 241 MB idle / pong / titles (startup.py, load ~4.7): the stage's growth 26 MB is within +40 (parked targets let go, 7ef9ad7; was 51), idle 215 against 200 (blur scratch, rings, groups, atlas: c68aca2, 762805c, a676f30, 195945d, 5ff5843; was 653) | RSS 204 / 222 / 233, footprint 234 / 254 / 290 MB (startup.py records both since f894348; load 3-6). Idle footprint was 653 MB, 566 of it GPU scratch for the blur's per-thread rings (c68aca2); RSS never saw it. Left: GPU ~126 MB dirty (the device counts 65-96), malloc 44 MB | idle footprint <= 200 MB, a stage <= +40 MB (was idle <= 100 MB RSS) |
        | R4 | GPU memory per stage | quads::gpu_allocated_mb (play report gpu_mb), gpumem.py | pong 47.7 -> 19.75 MB, titles 62.8 -> 44.25 MB, platformer 28.1 -> 12.7 MB (group targets at their bounds 195945d, a growing glyph atlas 5ff5843), deterministic | 1 texture per page size |
        | R7 | exported game size | app totals (total_mb): met, pong 63.8, the 50-game 63.3, void-pong 62.6, arena3d 62.5, the platformer 62.3 (9b8f768: openh264 out of the player, load-only crates at opt-level z, fonts written once); the player 62.50 MB | export.sh: self-test PASS and SSIM 1.000000 on all five; was pong 67.2, player 65.68 on HEAD before | <= 65 MB |
        | R5 | iPhone peak footprint | bench/lightspeed/ios.py | 373 MB (4279d3f); the harness needs the phone unlocked | <= 200 MB |
        | R6 | energy: CPU watts playing pong / a timeline / idle stage | 0.415-0.53 / 0.395-0.48 / 0.039 W (CPU, process) pong / timeline / idle stage (load 5-9); the idle stage's audio graph stops while paused (31d5bf6; 0.047-0.05 before); was 0.69 / 0.97 / 0.035 at load 100 | 0.69 / 0.97 / 0.035 W (not quiet) | race to idle; idle stage ~0 W |
    Scenario: Quality
      Then these rows are tracked
        | id | metric | harness | value | target |
        | Q1 | render = play, every medium (SSIM per frame, seeks included) | quality.py, check.sh, replay.sh, video/gate.sh, bevy check | 1.000000 on HEAD: replay (pong, platformer, arena3d), timelines (aa4f78a); void's stage = render on ts1's full-page seeks, 0.997160-0.998681 -> 1.000000 (4100b50: emboldening per document), stage scale 0.9995 -> 0.9998 | exact everywhere, every fixture in the corpus |
        | Q2 | Chrome parity | full corpus (engine/html/bench/parity.sh, HEAD after an agent session's page fixes 2e9ca65, 81e4a25): 95 exact, 75 ok, 0 fail; CI's react group 8 / 8 / 0 | 93 exact, 75 ok, 2 fail | all exact |
        | Q3 | export fidelity against the source (VMAF), colour accuracy | --10bit (HEVC Main10, f1ca6ee): colour patches 0/255, 1024 random colours worst 1, ts1 VMAF 99.52 (worst frame 89.22); H.264 8-bit, still the default: colour 2/255 (its format's floor), VMAF 98.95 | export.py --only Q3 measures both exports (exact 10-bit decode: swscale accurate_rnd+full_chroma_int); floors over all 16.7M colours: 8-bit limited 2, full range 1, 10-bit limited 0 | visually lossless (VMAF >= 95), BT.709 / P3 exact |
        | Q4 | audio: tap = bounce, LUFS, true peak, clicks | quality.py (audio.sh) | PASS: live = render -14.9 LUFS, tap = bounce exact, 0 clicks (quiet, e0498ab) | holds for every edit |
        | Q5 | cross-device determinism: Mac = iPhone, byte for byte | ios.py | frame 0 exact; glyph edges differ (<= 14/255) (4279d3f); needs the phone unlocked | byte-identical |
        | Q6 | agent: first-try success, brief to render time, judge score | void evals, sdk/motion/bench/bevy/eval | 15/18 first-try (3D skills) | measured nightly; rising |

  Rule: The levers, biggest chunks first (each is a work item with a done-check)
    Scenario: L0 The scoreboard itself
      Then bench/scoreboard.sh prints every row above from CI history and the newest harness outputs, with value,
        target, gap and trend, and flags rows that regressed
      And the missing harnesses exist: startup (F5), hitches (F6), batch (T4), energy (R6), VMAF and colour (Q3)
      And done: one command prints the whole scoreboard, and CI runs it nightly
    Scenario: L1 Parallel and pipelined work (frames are pure)
      Then bakes and exports split frame ranges across all cores, each worker stepping its own range from the nearest
        keyframe; decode, paint and encode overlap in bounded queues
      And done: T1 <= 1 s, T2 <= 1 s, T3 full bake <= 50 ms, with Q1 unchanged
    Scenario: L2 GPU-resident end to end, zero copies
      Then the encoder takes the GPU surface directly (IOSurface or CVPixelBuffer into VideoToolbox), with no NV12
        readback to the CPU, and no page, frame or texture crosses to the CPU on any play or export path
      And done: export readback bytes = 0, T1 improves, Q3 unchanged
    Scenario: L3 Zero waits
      Then nothing on a frame path blocks on another thread, lock, device or present: decoder handoff, synchronous GPU
        reads and mixer locks become bounded queues or lock-free handoffs (the wgpu present lock is done: ad90377)
      And done: every wait on a frame path is listed in a trace and is under 0.1 ms p99
    Scenario: L4 ZeroHeap (docs/zeroheap.feature)
      Then Z1 instruments every layer, Z2 fixed double-buffer game state, Z3 a compiled view, Z4 a typed shared-memory
        bridge, Z5 Rust frame arenas, Z6 fixed GPU and audio pools, Z7 the CI gate
      And done: R1 = 0 on every layer, R2 = 0, F3 = 0 at load 20
    Scenario: L5 Compile instead of interpret
      Then each TSX view is compiled once into update functions (with Z3), hot game steps are compiled ahead of time
        (a TypeScript subset to WASM or Rust) for jitless iOS, and every shader and pipeline is compiled and cached at
        load (Bevy and quads), so nothing compiles mid-play
      And done: F6 = no hitch over 2 ms; iPhone frame time (525 ms today, titles) falls to real time
    Scenario: L6 Do less
      Then damage regions (repaint only what changed), 3D level of detail, duplicate-frame skips in every export path,
        and painting at display size everywhere (the stage does: ad90377)
      And done: GPU time per frame on an unchanged page ~0; R4 down
    Scenario: L7 Instant startup
      Then snapshot what's slow to rebuild: JSC bytecode caches, prebuilt bundles, warm decoder and VideoToolbox
        sessions, preloaded fonts and pipelines, lazily loaded models (the speech model is: 28f3221)
      And done: F5 < 100 ms for every row
    Scenario: L8 Energy
      Then pace at the display's rate, pause anything unseen (the stage does), batch GPU work, and race to idle
      And done: R6 measured, idle stage ~0 W, pong's watts halved
    Scenario: L9 Quality to exact
      Then colour managed end to end (BT.709 / Display P3, tone mapping), text rendered exactly as Chrome (Q2's last 2:
        regenerate the shadcn reference through the composition format's render), the iPhone byte-identical to the Mac (Q5), and
        exports checked by VMAF (Q3)
      And done: Q2 all exact, Q3 >= 95, Q5 byte-identical
    Scenario: L10 Experiments: the fun part (try, measure, keep or drop)
      Then ideas worth a measured try, each with a recorded result:
        | idea | why it might pay |
        | hypervector world memory (HDC) for agent and perception state | memory that can't grow, by construction |
        | the game step on the GPU (compute shader) for many-entity games | thousands of entities in one dispatch |
        | predictive input: step the next frame on the likely input, confirm when it lands | input latency below one frame |
        | a frame-time scheduler that starts each frame as late as it safely can | the newest input with no missed frame |
        | 120 Hz path when the display allows (ProMotion) | halves F1 |
        | metal-direct present for the stage, bypassing wgpu's surface | the fewest layers between paint and glass |

  Rule: How lightspeed works beside everyone else
    Scenario: Shared machine, shared tree
      Then builds run through `lane build --` under `nice -n 15` (memguard kills jobs over 24 GB), long jobs under
        `antsy run --max ...`, and waits through `antsy wait` (a hook blocks sleep loops)
      And /tmp/raea-measure.lock is held only by the nightly quiet pass and by short paired A/Bs (at most 10 minutes);
        builds are capped at -j 6 so three agents' builds don't drive the machine to load 100
      And it commits only its own hunks (check `git diff --cached`, stage explicit paths, filtered patches for shared
        files), with "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
      And it never restarts, kills or quits a void the user may be using, and never shows a window on the founder's
        screen: test voids run headless or windowless
      And only two API models exist: GLM 5.3 Flash and clef-omni. Small local models are fine. Spend under $10 without
        asking
      And areas other agents are working on as of 2026-10-18 (coordinate through an agent session before touching them): live
        3D games in void, export-game, void video and re-bake on change, the new-stack agent eval, the Tone.js layer and
        engine/mixer/. Video playback is always fixed scale, never responsive
    Scenario: Reporting
      Then after each turn lightspeed messages an agent session one line: the row, before and after, the commit
      And it never claims a row moved without a quiet-machine measurement behind it

  Rule: Log (newest first; lightspeed appends)
    Scenario: 2026-10-19 evening: G7's root cause, by Metal System Trace, and its fix (6d4d9a6)
      Then traced at load ~4 (xctrace's Metal System Trace, metal-driver-intervals): on the card's frame a wgpu-internal
        PendingWrites command buffer ran 3.64 ms, its render pass 3.24 ms: wgpu clearing the new 1280x720 group target before
        its first use. The glyphs, the GPU's clocks and the target's creation itself weren't it; a clear made on the same
        frame (touch, RENDER_ATTACHMENT) only moved it within the frame
      And the fix: one page-sized group target made and cleared when a page size is first bound (at open), handed to the
        first group needing a quarter of the page or more. Traced again: its clear at open (1.8 ms at 0.88 s), no pong GPU
        interval over 3 ms in play. Frames byte-identical (895, 900, 960 rendered; the card played); +1.9 MB GPU (R4)
      And measured no on the way: the first version handed the spare to the first group at all (a 320x192 one took it)
    Scenario: 2026-10-19 evening: G3 step 3, updates read in place, and two measured noes on Stylo
      Then rawops (417cc5a): a batch split into its ops' raw text; an update's props walked by a borrowing visitor and set
        in the kept props in place (scalars without allocating, a string's buffer reused, an escaped string compared and
        decoded by JsonChars, arrays and objects element by element). Applying ops, Rust allocations a frame: arena3d 84.7 ->
        12.1, pong 74.8 -> 29.3, the platformer 328.7 -> 227.8; frames byte-identical, replay, check.sh and audio.sh exact
      And on the way: a failed borrowed &str parse of an escaped string allocates its error (arena3d's data-props carry
        JSON): a backslash is checked first (178 -> 12 a frame)
      And measured no: restyling only the element on a style attribute change, not its subtree and its parent's (Blitz's
        coarse TODO): pong 1,346 either way; the restyles that allocate are the moving elements' own (their var()s
        substituted again each restyle). Reverted
      And measured no: changed style fields set one declaration at a time (set_style_property, on longhands whose
        shorthands the style doesn't have), the attribute's text kept without a parse: the platformer 227.8 -> 218. Stylo's
        rule tree shares the declaration block, so each set copies it whole (mutable_style_attribute: 2,907 of 3,847
        samples). Reverted
      And next for arena3d: motion::read3d parses every part's data-props JSON each frame (~2,100 samples): cache a part's
        parsed props by its text
    Scenario: 2026-10-19 evening: G7, the card's frame is the first full-page group, and what that rules out
      Then traced (group_target): pong's card overlay is the first group to need the whole page; at frame 892 slot 4 grows
        to 1280x720 (every earlier group was 64-320 px). The frame after it waits 4-7 ms on the GPU (gpu_ms), 0 glyph
        misses, no raster fallbacks or uploads, and quads' own timed passes don't hold it (group max 0.91, page ~3 ms
        under load): the cost is outside them
      And measured noes, load 12-17: the card's glyphs pre-drawn outside any group (5.6-7.0 ms, same as without); group
        targets given RENDER_ATTACHMENT so wgpu clears them with a render pass (7.0-7.2); a new target touched with a clear
        as it's made (4.0, same as without). Every group target made page-sized from the start: 2.7-2.9 in one round,
        4.0-4.2 in the next, so not a clean win, and +3.7 MB a slot of GPU memory (R4): not taken
      And so G7 stops at: the first full-page group's first pass. Its last step is a Metal System Trace of frames 890-894
        on a quiet machine (which command, clear or page-in, the 4 ms is)
    Scenario: 2026-10-19 afternoon: G5, every Rust thread counted, and the two biggest sites removed
      Then Z1 now counts every thread (19e7dc9): pong 6,797 allocations and 868 KB a frame, the platformer 5,304, arena3d
        12,440 and 2.4 MB; the main thread is ~99% of it (the JS thread 10, the mixer and task pools ~1 each)
      And by sampled site (RAEA_ZEROHEAP_SITES, resolved after the run): quads' Gpu::encode made a Vec per 16x16 tile per
        pass, 82% of pong's samples. A counting sort over scratch kept between frames writes the same bins (3b7c4de): pong
        1,576 a frame, binning 0.16 -> 0.05 ms, present p50 ~0.1 ms shorter (paired x3, load 10-12)
      And arena3d's next 65% was scene3d: a live frame walked its tree 11 times through items(), cloning every part's
        props. Walked once now (3fbe7ac): 8,837 -> 6,675
      And both are exact: titles, pong, arena3d and t3d frames rendered before and after are byte-identical PNGs; replay x3,
        check.sh and the full Chrome parity (95 / 75 / 0) unchanged
      And what's left, by site: pong's is spread over Stylo's restyle, Blitz's paint paths (kurbo), the op parse and live
        sound; arena3d's is scene3d's rows rebuilt each frame (BTreeMap rows, then diffed against what was sent)
    Scenario: 2026-10-19 afternoon: G4 met through the SDK, and a feature-unification regression bisected
      Then pong's step+fold garbage, split: an identity fold makes the same as pong's step (7.2 against 7.0 KB a frame);
        without its five useGameEvents calls it's 1.6. Each call made isEvent.toString(), a copy of the kept frames, a new
        record and a filtered copy (f3b0005: a key per call site, the record in place, a frozen list handed out again);
        useLocalTime built a new object per call outside a Sequence, and validate a new fold.checked (3c36ba8). step+fold
        9.2 -> 1.4 KB, 30 -> 12 GC/min; pong 36.9 -> 33.4 KB; gates exact (replay, check.sh 360 of 600, audio.sh)
      And put (f3b0005): a changed prop's value is set in place, not insert(key.clone()): pong's op apply 87 -> 74.6
      And an agent session's f60e0b7 (Rolldown) had raised pong's op apply 87 -> 159 (git bisect run, clean worktree builds): its
        crates turned on serde_json's preserve_order and arbitrary_precision for the whole binary, which also changed key
        order and number handling everywhere raea uses serde_json. Fixed in d91a34f (an opt-in feature) and 18d2801
        (raea-bundle loaded at run time, never a Cargo dependency); confirmed back at baseline
      And a seedling bounce = live failure (max diff 0.0118) under a loaded 7-minute gate run didn't repeat (passed twice,
        with and without the change): a flake of the live path under load, noted
    Scenario: 2026-10-19 afternoon: G7, pong's card hitch is first use on the page, not the GPU's clocks
      Then pong in attract mode (no keys, 12 s x 3): frames 891-892, the card's first, 6.6-6.8 ms (present 5.9-6.0); by
        RAEA_QUADS_STATS the card's first frame (892) has 13 glyph misses at 0.42 ms GPU, the next (893) 4.18 ms GPU with
        none: the wait for 892's GPU work. Glyph rasters are 0.004-0.037 ms each and the atlas never grows (traced)
      And what removes it, paired with the plain edit: the card's text drawn at 1% opacity inside a stage-sized group in
        frames 10-30 (3/3; so did "." and "—" alone, and the text without the dash); what doesn't: the card's box and shadow
        without text, a 12 px dot in a small group, the same text and group painted on a separate throwaway page at open
        (warm_gpu, 3/3: a measured no, reverted). So it's the page's own first pass that draws glyphs inside a group
      And the 2026-10-18 attribution to clocks the light game let fall is withdrawn: the first-frame cost moves with what was
        drawn before, not with the GPU's load. The game-side pre-draw changes pixels (1%), so it isn't taken. Open: which
        state of the page's pass is first-use (a group target's first sampling of the atlas); not the insts or bins buffers
        (traced: they never grow in pong's run)
    Scenario: 2026-10-19 afternoon: G6 measured (L2 can't move T1) and G9 met (Q2 0 fail)
      Then ts1's export, per frame (load 7-13; RAEA_ENC_TRACE=1, b646140): the encoder thread waits 3.20-3.23 ms for room in
        flight and a buffer, copies 0.066 ms into the CVPixelBuffer, calls encode_frame 0.27 ms; the render thread makes the
        page 3.30 ms (fed), copies 0.065 ms out of the readback ring, and waits 1.31 ms on the encoder's channel
      And so frames still cross to the CPU (3.1 MB a frame, 1.85 GB for ts1), but the copies are ~0.13 ms of ~3.8: the media
        engine bounds T1 and an IOSurface path would move it ~nothing. Measured no: 8 frames in flight against 3, paired x3,
        258-264 fps both
      And Q2 on HEAD, the full corpus: 95 exact, 75 ok, 0 fail (an agent session's page fixes landed meanwhile), G9's last two gone
    Scenario: 2026-10-19 midday: Z1's Rust counter, Z4 step 1, and T3's re-bake regression fixed (f910193, cd367dd)
      Then Z1: sdk/react/src/zeroheap.rs counts each thread's allocations under RAEA_ZEROHEAP=1 (raea-motion only; off it costs
        one relaxed load), and play's report carries rust_allocs: pong's op apply 172 allocations and 17.0 KB a frame, the
        flush 2; by span the JSON parse 64, the page mirror 203, the entity loop 14 (means with warmup)
      And Z4 step 1: the mirror's update merges its delta key by key and sets only what changed (it cloned the node's
        props four times and set every attribute again, the style as a string per field): 172 -> 88 allocations, 17.0 ->
        10.0 KB a frame; work and apply p50 the same within load (paired, 3 pairs, load 6-7); replay, check.sh, audio.sh
        exact. Next: the JSON parse (~64) and the entity loop (~14), the typed op ring
      And T3 had regressed since 0857a00: the session bundler ran bun from sdk/react/js, so each module's path comment named
        the process's work folder, inside the bundle's outer IIFE, and every edit re-baked every frame (600 of 600). The
        bundler now runs each build from the entry's folder: titles' incremental 360 of 600 again (31 ms), cache on and off.
        Measured noes on the way: RAEA_BUNDLE_SERVE=0 and RAEA_LITE_COMPILE=0 alone didn't fix it (the old bake's bundle
        came from the cache the session bundler had filled)
    Scenario: 2026-10-19 midday: R7 met, every exported game under 65 MB (9b8f768)
      Then the player on HEAD was 65.68 MB (it had grown 0.4 since the logged 65.28); attributed by symbol (the dSYM's
        text, sized by address gaps): openh264's encoder and decoder ~0.86 MB, sqlparser ~2, image_webp 0.34, zune_jpeg
        0.29, manifold 0.33, bevy_reflect 2.8, stylo ~2.3
      And three parts, no feature dropped: media's soft backend split into soft-audio and h264, the player taking
        soft-audio and decoding video on the platform (65.68 -> 64.39 MB); the crates it only runs while loading at
        opt-level z (sqlparser, the image, SVG and audio decoders, the MP4 demuxer and muxer, deflate, manifold:
        64.39 -> 62.50); each packaged font face written once (pong's sheet inlined 8 faces 16 times: content 1.91 ->
        1.61 MB)
      And the apps: pong 67.2 -> 63.8, the 50-game 66.4 -> 63.3, void-pong 66.0 -> 62.6, arena3d 65.7 -> 62.5, the
        platformer 65.5 -> 62.3; export.sh self-test PASS and SSIM 1.000000 on all five
      And opt-level z cost nothing measurable: paired on pong (4 pairs, load ~4), open 102-106 ms both, work p50 0.82-0.98
        against 0.83-1.04 ms, p99 the same within noise
    Scenario: 2026-10-19 morning: Q3.colour met with 10-bit export (an agent session's Q3 agent, f1ca6ee)
      Then `raea-motion render --10bit` (also `rdb render --10bit`, RAEA_EXPORT_10BIT=1) exports HEVC Main10: a GPU P010
        conversion (kgpu p010buf.k) into VideoToolbox; H.264 8-bit stays the default for playback reach
      And turn 3's muxer failure is fixed: muxide read HEVC's profile and level at fixed byte offsets of the stream
        header, which VideoToolbox's emulation-prevention bytes moved, and always declared 8-bit Main; it parses the
        header now, ffmpeg and AVFoundation both decode the result, muxide's 124 tests pass
      And the floors, offline over all 16.7M colours: 8-bit limited range 2, full range 1, 10-bit limited 0, so full range
        was never enough
      And paired, H.264 -> --10bit (the agent's runs, load 7-8): colour patches 2 -> 0/255, 1024 random colours worst
        4 -> 1; ts1 VMAF 98.95 -> 99.52 (worst frame 88.31 -> 89.22), 263 -> 252.5 fps (-4%), 25.09 -> 25.20 MB; t3d600
        275 -> 261 fps (-5%), 15.31 -> 9.89 MB
      And an agent session's check on the merged build: export.py --only Q3 gives Q3.colour 0, Q3.vmaf 98.95 (H.264); titles' check
        and replay x3 1.000000
      And what's left is the codec's, not a lever: ~20 of 1024 busy colours miss by 1 (VideoToolbox's compression; an
        ideal 10-bit input misses the same ones), and AVFoundation's own RGB conversion errs 1 on Main10 (Apple's decoder)
    Scenario: 2026-10-19 morning: the four multi-day levers at their stopping points
      Then Z3 is closed (pong 120.3 -> 34.6 KB a frame; the rest is Z4's and Z2's), T3 is blocked on the system
        JavaScriptCore (incremental 24-27 ms against 10), F5.void_launch is decided by the user (~200 ms behind the
        launch reveal; edit-to-swap 132-163 -> 33-61 ms), and Q3 is met (10-bit export)
    Scenario: 2026-10-19 morning: void's launch reveal (an agent session, f329b0f)
      Then a last pass in quads' window.rs, opt-in, turned on only by void's vendored dioxus-native (renders, exports and
        play never reach it): 280 ms from the first present, 12 px cells, Bayer x fbm thresholds ranked so the share shown
        follows a smoothstepped t, unrevealed cells black stepping through the site's greys 14 / 30 / 62; a plain fade
        under Reduce Motion; once a launch
      And first paint, paired on one build (RAEA_VOID_REVEAL on / off): 255 / 257 / 277 against 274 / 286 / 267 ms,
        unchanged within noise; the mask costs 1-4 us a frame and one image fill, nothing after it ends
      And R6, a void with titles open and paused: 0.017 W against the logged 0.039 (under load; includes 0857a00's FSEvents
        watch), to confirm on a quiet nightly run; video.sh ts1 PASS (re-bakes 52 and 62 ms); the 105 ms edit-to-swap tail
        hasn't repeated (52, 62, 33, 60, 61)
      And a headless void's window is created invisible, so the MCP screenshot shows nothing of it: the look is checked
        offscreen through the quads compositor (window.rs's test runs the real mask: 0.1 / 6 / 28 / 65 / 100% shown at 0 /
        15 / 35 / 60 / 100%; RAEA_REVEAL_OUT=DIR writes the frames)
    Scenario: 2026-10-19 morning: void's edit path, 132-163 -> 33-61 ms (an agent session, 0857a00)
      Then void's edit-to-swap is 33-61 ms (one run of five at 105), from 132-163, in two parts
      And a session-long bundler (react/js/lite/serve.ts, a line per request) keeps each source's compile by content:
        bundle 38-41 -> 10-13 ms (a fresh bundle's compile alone was 12-15, bun's start ~11); no bundler outlives void
      And the larger part: the stage's watch was a 50 ms poll plus a second poll for the file to hold still, so 50-100 ms
        passed before a re-bake began; it now takes FSEvents (notify 8, macos_fsevent) with a 10 ms quiet window and a
        1 s fallback poll, so an open timeline no longer wakes a thread every 50 ms either (R6's idle, to measure)
      And the gates hold: video.sh ts1 PASS x3, each re-bake equal to a fresh bake (1.000000); check.sh titles, animate,
        solids 1.000000 (incremental 24 ms); replay x3 1.000000
    Scenario: 2026-10-19 morning: F5.void_launch decided by the user: keep ~200 ms, mask it
      Then the user: ~200 ms to void's first frame is standard and happens once, so it isn't restructured; a quick launch
        animation masks it instead, the website's pixel diffusion (apps/site/src/components/effects/dither-field.tsx: 4x4
        Bayer ordered dither, square cells, an fbm field)
      And so an agent session's remaining step (window and stage painted before the chrome mounts) is dropped, step 1 was a
        measured no (the stage's box waits on the chrome's first style, layout and render: 215-222 ms either way), and
        Phase A stays out of the tree
      And the reveal is an agent session's, after the session-long bundler: void's window only (engine/html/src/quads/window.rs, a
        flag only void sets), never renders or exports; done when first paint is unchanged within noise, the reveal's
        pass costs < 0.2 ms a frame while it runs and nothing after, and it plays once a launch (not on workspace
        switches), a plain fade under Reduce Motion
    Scenario: 2026-10-19 morning: F5.void_launch's design, measured (an agent session, 26edce8: docs/void-launch.md)
      Then the critical path to first paint is no longer the stage's open (its 63-75 ms runs off the path since 56cc5ac):
        it is the window and chrome mount (~100-130 ms), the chrome's first render, the launch's enter (a task spawned at
        mount, so a render late), the stage's box laid out (its rect at mount + ~80), then the paint
      And measured no: the stage thread started in main() before the window (Phase A) finishes the open at ~92 ms,
        before mount, but first paint doesn't move (RAEA_VOID_EARLY_STAGE on/off on one build, paired: 226 / 230 / 253
        against 211 / 220 / 228 ms); the patch is kept outside the tree until step 2
      And the steps, both in void's launch (raea-01's, waiting on it or the user): 1, the recent workspace entered in
        the app's initial state so the stage's box is in the chrome's first frame (~60-80 ms off); 2, window and stage
        painted before the chrome mounts, in dioxus-native and stage_native (~90-100 ms off)
      And the edit path: an uncached bundle is 35-39 ms, bun's start only ~7 of it; a session-long bundler with in-place
        resolution and the compile plugin cached by content should take it to ~10-15 ms (an agent session's, scheduled now)
      And Q3's encoder agent hit its weekly limit (~08:30) and resumes after the 10:00 reset. Its route is HEVC Main10
        on the media engine: this M5 Pro's VideoToolbox lists HEVC and ProRes hardware encoders and no AV1 encoder at
        all (AV1 decode only), and rav1e measured 547 ms a frame against H.264's 0.75 at the same bytes (bakeoff)
    Scenario: 2026-10-19 morning: T3 step 2, and T3 blocked on the system JavaScriptCore (an agent session, e0452c6)
      Then titles' Counter calls toLocaleString every frame, and the system JSC built a new Intl.NumberFormat each call
        (56-64% of titles' frame, sampled in bun); the engine prelude keeps one formatter per locale and options
        (identical strings over 91 cases): full bake JS 26.5 -> 19.8 ms, check.sh's incremental 25-27 ms ("incremental,
        360 of 600 frames stepped, 25 ms"); check.sh titles, animate, solids 1.000000, replay x3 1.000000
      And T3 is blocked on the JS engine: the system JavaScriptCore runs titles' frames at ~50 us where bun's JSC runs the
        same bundle at ~13 (bun with FTL off still 12.6; the system build has useFTLJIT=false), and sampling puts the
        engine's JS thread in JIT code, not the bridge (flush 3%, JSON 2%), already at user-interactive QoS
      And on each runtime's ~8 ms open (JSC context 3.2, prelude 0.9, bundle eval 2.6, first render): a warm spare
        context, measured before it was built, saves ~3 ms end to end (the other three runtimes still open in
        parallel), not worth a stage's footprint: left out
      And the lever is multi-week, logged beside Z4: raea on bun's JSC build (a vendored WebKit) instead of the system
        framework, ~4x on every JS frame (live play, bakes, renders). Its cost to weigh first is R7: a vendored WebKit
        ships inside every exported player, already at 65.28 MB against 65
      And void's edit-to-swap is 132-163 ms, of which bun bundling is 38-41 and the bake 23-29: the bundle step is the
        larger and the cheaper lever (an incremental or cached bundle), a row for F5's edit path
    Scenario: 2026-10-19 morning: T3 step 1, the range bake (an agent session, ec41abe)
      Then check.sh's titles edit at 4 / 8 runtimes: incremental 47-58 -> 25-29 ms (paired: 1 runtime 25-42, 4 runtimes
        27-30), full bake ~50 -> 25-32 ms; RAEA_BAKE_TRACE "incremental, 360 of 600 frames stepped", and per range
        "frames 240..600 in 19.0 ms: waited on React 6.1 ms, applying 3.6 ms, JS 19.0 ms"
      And helper attribution holds since c59c6f7's inline builders: the edit re-steps only the Cards frames (240-600)
      And the levers: one raea-frames event per range, one commit applied per update (OneBatch); bake steps run only
        PreUpdate (the whole main schedule was ~50 us a frame, more than React's render); incremental runs split across
        runtimes (steadier, not faster on average). Measured noes: more JSC compiler threads, FTL off, lower tier-up
        thresholds, 2 or 3 runtimes
      And the gates hold: check.sh titles, animate, solids 1.000000 with both incremental re-bakes; void video.sh ts1
        PASS; void's re-bakes 23 ms (spring) and 29 ms (constant), from 43 and 44, each equal to a fresh bake
      And what's left is JS: ~50 us a frame warm, more in a fresh runtime, and ~8 ms to open one (JSC context 3.2,
        prelude and install 0.9, bundle eval 2.6, first render). Under 10 ms: less JS a frame in titles first (no
        footprint), then a warm spare JSC context (~4 ms), held only while a timeline is open so it counts against a
        stage's +40 MB (26 used), never against idle (215 against 200), and let go when the stage closes
    Scenario: 2026-10-19 morning: Z3 step 2, and where Z3 stops (an agent session, c59c6f7)
      Then pong under jsheap.py (12 s x 2, load ~3), whole / step+fold / view alone: 34.6 / 9.2 / 18.0 KB a frame and
        58 / 30 / 48 GC/min; an empty edit is 5.2 KB, so view alone sits 12.8 above the floor (74.5 before Z3); view
        alone's JS p50 0.087 -> 0.043 ms
      And the gates hold: replay.sh 1.000000 x3; check.sh titles 1.000000, its incremental re-bake at 360 of 600 frames
        as before; audio.sh max diff 0; video/gate.sh ts1 PASS (scrub p99 13.7 ms against 16.5 for the build before,
        back to back)
      And step 1's builders at module top had turned a constant edit into "a changed function no frame recorded" and a
        full re-bake (T3); they now sit inline in their component (__S.bHASH ??= ...), so the bake attributes them again
      And pong's static court costs nothing now (taking it out moves 0 KB: host reuse covers it). What's left, by
        jsalloc.ts over the first 12 s (about half of live KB): ~3.2 KB playing sounds (each cue's per-frame LocalTime,
        two provider elements, Audio's attribute object, the data-source-time op and its JSON: protocol churn, Z4's
        typed bridge or the time model), ~2.0 pong's own body (closures in render, Score and Paddle re-rendering), ~2.7
        the floor (Clock state, the marker op, the flush JSON)
      And Z3 stops here: the rest is Z4 (a typed bridge, no per-frame JSON) and Z2 (fixed state), not the view compile.
        an agent session moves to T3
    Scenario: 2026-10-19 morning: Z3 step 1, pong's garbage a third of what it was (an agent session, 36d4b06)
      Then pong under jsheap.py (12 s x 2, load ~4), whole / step+fold / view alone, re-baselined at HEAD on the committed
        fixtures (sdk/react/bench/games/pong/edit.stepfold.tsx, edit.view.tsx): 120.3 / 9.7 / 81.3 -> 39.4 / 9.6 / 26.4 KB a
        frame, 192 / 30 / 170 -> 72 / 30 / 60 GC/min; JS p50 0.106 -> 0.094 ms
      And it is four parts: each JSX site compiled at bundle time into a per-instance cached element, rebuilt only when
        one of its dynamic values changes, with a host subtree of identical elements reusing its output record whole
        (so 87e77b2's bailout fires); the lite runtime's Map sweeps no longer make a [k, v] array per component a frame;
        hook calls no longer make an init closure each; and motion's Sequence skips generated-sound cues that have
        ended, which was 39 of view alone's 48 KB
      And Q1 holds (replay.sh pong, platformer, arena3d 1.000000; check.sh titles 1.000000 with the incremental re-bake;
        audio.sh max diff 0); Q2's React render path isn't touched
      And view alone has ~20 KB a frame left above an empty edit's 6.8 KB floor: an agent session's next Z3 step. A new tool,
        bench/lightspeed/jsalloc.ts, runs a lite bundle in bun's JSC and reads about half of live KB, the same proportions
    Scenario: 2026-10-19 early: R7 reopened, an exact lever
      Then R7's value is export-game's total_mb, the whole app: pong's now carries 2.9 MB of uncompressed WAV assets (one
        2.1 MB) and fonts.css 0.49 MB of base64. Encoding the assets losslessly at export (ALAC or FLAC: the same PCM, so
        tap = bounce) halves them, and fonts as files save ~0.12 MB more, with no feature dropped (not WebP). Handed to
        an agent session (export-game). Until it lands, R7 stays blocked as logged (the player's last 0.66 MB is WebP or rdb's
        SQL); if the apps measure under 65 MB with it, the row is met
    Scenario: 2026-10-19 early: where the loop stops
      Then every row is met (F3, F4, F5 game/timeline/bake/object open, F1 offscreen, R3's stage, Q1, Q2) or blocked with
        its reason in this log and sent to an agent session (F5.void_launch, F2, F6, T1, T2, T3, T4, R1, R2, R3's idle, R4, R5, R6,
        R7, Q3, Q5, Q6, F1 in a window)
      And the targets changed tonight (R7 <= 65 MB, R3 <= 200 MB idle footprint) and the offline player were decided
        under the founder's own instruction while out ("take executive decision", 2026-10-18 night), recorded with their
        reasons above; they stand unless the user changes them
      And the next multi-day levers, in order of what they unblock: Z3, the view compiled at bundle time (R1, R2, and
        pong's R6 watts); a bake that steps a range of frames per round trip, with helper-level coverage (T3); void's
        launch restructured so a native window and the stage come before dioxus mounts the chrome (F5.void_launch, with
        raea-01); 10-bit or full-range export (Q3 colour, T1 via HEVC once its muxer works)
    Scenario: 2026-10-19 early: F5.void_launch, blocked above 100 ms (final; an agent session's measurements)
      Then a headless void's launch (an agent session's marks, two runs): app mounted 106-121 ms (the window, winit and AppKit, and
        dioxus's first render), stage show 117-130 (the early open of the launch's object fires), stage ready 178-182
        (titles' open, ~50 ms), frame 0 stepped 185-200, first paint 254-267: the ~60 ms between them is the first
        frame's sound waiting on the mixer's startup clock (RAEA_LIVE_TRACE "sound 62.0/67.7 ms")
      And an agent session paints frame 0 before its sound, as play does (44028f1; step_world and sync_sound made pub in 31d5bf6):
        launch 265.0 / 271.8 -> 219.9 / 206.4 ms, paired under load (another agent's stress job); object open 78-81
        unchanged
      And below that it's blocked: the window and dioxus mount alone (106-121 ms) are past 100 before the stage can show
        anything, and the object's open (~50 ms) follows; under 100 needs the window and the chrome's first render
        restructured (a native window up before dioxus mounts, the chrome rendered after the stage's first paint), a
        change to void's launch architecture and look (raea-01's), not tonight's
    Scenario: 2026-10-19 early: R6, the paused stage's audio rests
      Then the stage rests its mixer's device stream a second into a pause, and its spare 2 s after it opens (31d5bf6):
        a paused void sampled 3 s shows no Firewheel processing; a 3 s pause advanced the device clock 1.01 s (the
        fade-outs) where it ran 3 s before; the play after it placed the timeline afresh (5.5 ms of drift slewed out);
        idle stage 0.047-0.05 -> 0.039 W; the audio gate 4/4
      And what's left of the idle stage's 0.039 W is its threads' wakeups (the stage's 250 ms wait, the event loop):
        the target's "~0" would mean an idle void with nothing waking, the app parked; that's where R6 stops
    Scenario: 2026-10-19 early: more blocked rows, final (sent to an agent session)
      Then R7 (65.66 against 65) is blocked on features: what's left in the player that it could drop is image-webp
        (0.34 MB, anyrender_svg's image_format_webp: WebP images in a game's pages and SVGs would stop drawing) and
        sqlparser (2.3 MB, rdb's core: a refactor); tokenizers isn't in the player. 0.66 MB isn't worth losing WebP
      And Q3.colour (2/255 against 0) is blocked on the 8-bit format: its own round trip errs 2 (above)
      And F2 (arena3d p99.9 4.58 against 4) is blocked on Bevy: its CPU half is Bevy's render app (11 queue submits a
        frame, GPU buffers created each frame, GPU clustering's CPU side), upstream Bevy's to change; its GPU half's
        exact items are done (a47055d), the rest (shadow cascades, clustering mode) change pixels: a look call
      And T3's incremental re-bake (48-58 against 10) is blocked on the bake's per-frame JS <-> Bevy round trip and on
        helper-level coverage, both changes in motion's clock (the animation agent's) and the lite runtime: multi-day
      And R4 (graphics footprint pong 84 / titles 116 / ts1 118 MB against one texture a page) is blocked on Metal's own
        memory as R3's idle is: the device holds pong 15.1 and titles 38.6 MB (from 47.7 and 62.8 tonight), the
        remaining ~65 MB of a stage's graphics footprint is the driver's beyond any allocation
      And T4 (1553 renders an hour, titles at 4 processes; 311 before the blur work) is encoder-bound as T1 and T2 are,
        which is its target's own wording
      And Q6 (the agent's first-try rate) is the new-stack eval's, measured nightly by its own agent: not lightspeed's
      And F5.void_launch (~280 against 100) and R6's idle stage are open, an agent session's next items
    Scenario: 2026-10-19 early: R6's idle stage and Q3's colour floor
      Then a headless void with titles paused (energy.py's idle, 0.047-0.05 W): nearly every thread waits, but CoreAudio's
        IOThread calls Firewheel's process each callback (VoicesProc zeroing buffers, the master limiter on silence),
        the output stream open while paused; stopping it while paused and starting it again on play is the lever,
        an agent session's (mixer/ and the stage). Measured no: the voice speak thread's 20 ms wakeups (sleeping while idle left
        idle W unchanged, reverted)
      And Q3.colour is blocked on the format: an exact 8-bit limited-range BT.709 round trip (RGB -> Y'CbCr -> RGB, no
        encoder) has a worst error of 2/255 and 1 on 83% of 200,000 random colours, so the export's 2 is that floor;
        exact needs 10-bit (HEVC Main10, ProRes) or full-range video
    Scenario: 2026-10-18 night: T3's incremental re-bake attributed
      Then check.sh's "incremental" edit (titles' last spring retuned) never re-bakes incrementally: the edited function
        is a helper no recorded component is (RAEA_BAKE_TRACE: "a changed function isn't a component any frame
        rendered"), so it bakes every frame again, 47-58 ms on 4 runtimes at load 3-5
      And that bake isn't CPU-bound: 4, 8 or 12 runtimes all take 47-53 ms, and a frame costs ~0.29 ms per runtime in
        parallel against ~0.09 ms on one, so the per-frame JS <-> Bevy handoff (settle_frame's flush wait) is the cost
      And the levers: function-level coverage per frame (helpers attributed to the components that call them, so an
        edit re-steps only their frames), and a bake step that doesn't wait on the app's flush round trip per frame.
        Measured no: handing a mostly-changed edit to the parallel whole bake (it wasn't the path taken)
    Scenario: 2026-10-18 night: F5 game and timeline under 100 ms, the player offline
      Then play presents frame 0 as soon as the object is open, the sound readied after it (44028f1): pong's spawn to
        first frame 339 -> 91-94 ms; startup.py F5.game 70.2, F5.timeline 81.3 ms (load 26-30); the audio gate's
        seedling passes both ways (its one failure was load 32)
      And R7 (an agent session, e0f14b3): the exported player runs offline, fonts inlined at export: 65.66 MB
      And F2's arena3d frame on the GPU (an agent session's Metal trace): ~1.8 ms (quads 0.52, opaque 0.48, four shadow
        cascades 0.38, wgpu internals 0.21, clustering 0.11); the CPU side (Bevy's update, extract and encode) is the
        larger half of its ~3 ms. an agent session takes the exact GPU items (indirect-draw validation off in release, batched
        staging), then the CPU side
    Scenario: 2026-10-18 night: blocked rows, final for tonight (sent to an agent session)
      Then T1 and T2 are blocked on the hardware: H.264 on the media engine is ~275 fps at 1080x1920 with speed already
        prioritized and non-realtime, and there's one engine (two sessions halve each other); 600 frames can't go under
        ~2.2 s. Unblocks with another encode path: HEVC (the muxer failed, turn 3) or an export split across the
        encoder and a second one
      And R1 and R2 are blocked on Z3, a multi-day build: pong's garbage is 90% the view (80 of 126 KB a frame, the
        step and fold 10), which only a view compiled at bundle time (update functions, no per-frame elements) removes;
        bun bundles today with no transform hook, so it needs a build script and a new runtime path. Z2 follows for the
        last 10 KB. This is the next multi-day item
      And R3's idle footprint (215 against 200) is blocked on Metal's own baseline: owned GPU memory ~137 MB of which
        the device holds ~45; offscreen pong shows the same ~65 MB of driver memory beyond its allocations. The
        remaining levers (the backdrop's blurred page at its region, 2-3 MB) can't close 15 MB. The stage's growth is
        met (+26 against +40)
      And F6 is blocked on the GPU's clocks: pong's card frame is its first GPU work after the light game let clocks fall
        (the same group pass is 0.9 ms the first frame, 0.38 two frames later); the app can't hold clocks up, and making
        the card's work smaller didn't move it (two measured noes)
      And R7 is in progress, not blocked: an agent session's offline player (no rustls/ureq, fonts embedded at export) should
        take 66.77 MB under 65
    Scenario: 2026-10-18 night: executive decisions (the user, out until morning: "take executive decision")
      Then R7's target is <= 65 MB, not 50: opt-level s reached 53.8 MB at +20-22% frame work, and feel outranks bytes
      And the exported player runs offline: export-game embeds every font the game uses and the player drops the
        network (rustls/ureq, ~1 MB), taking it under 65 MB (an agent session)
      And R3's target is an idle footprint <= 200 MB (a stage <= +40): offscreen raea-motion alone is 149-151 MB, so
        100 MB needs JSC or the GPU work out of process, logged as a later lever, not tonight's
      And ZeroHeap Z2 goes in as an opt-in fixed-state API beside today's SDK (games written today keep working),
        pong first, measured on R1/R2 (lightspeed). Revised after measuring (below): Z3 first, Z2 after
    Scenario: 2026-10-18 night: the clean feel run, and pong's card frame
      Then an agent session's clean run (no other jobs, load 5.2 -> 3.0): F3 met (0 missed), F2 worst arena3d 4.58 ms p99.9, F6
        37 (pong's card; the logged 7117 predates da1dbe3, which adds the work p50 3d1ecde's rule reads), R1 125 KB, R2
        196/min
      And pong's card frame: the frame after the card first draws waits 4.3 ms with ~2.4 ms of timed GPU passes; its
        group pass is 0.9 ms the first frame and 0.38 two frames later, so it's the card's first GPU work at clocks the
        light game let fall. Measured noes: page-sized group targets from the start (2-3 against 2-4 hitches in 20 s,
        the same worst frame), shading nothing outside the clip early (the shadow's hole: group pass 0.91 either way)
    Scenario: 2026-10-18 night: R3's stage growth within target
      Then titles on void's stage added 51 MB of footprint, 34 of it owned GPU memory: quads kept a closed object's
        page and groups parked until two other sizes pushed them out; parked sizes unbound for 240 frames are let go
        (7ef9ad7): +15-16 MB, owned GPU memory below idle after the switch
      And the blur's rings are their windows' length with the first box's sums in 16-bit halves (762805c): titles'
        GPU memory 50.2 -> 43.25 MB; a modulo a pixel cost 2.5% of a render, counters wrapped by hand cost nothing
      And idle (215 against 200) is Metal's and the driver's baseline (owned GPU ~137 MB, the device itself ~49),
        malloc ~45 and JSC: no one lever left there; the backdrop's blurred page at its region would give 2-3 MB
    Scenario: 2026-10-18 night: R1/R2 attributed, Z3 before Z2
      Then pong's JS garbage split by variants (jsheap.py, 12 s x 2, counts hold under load): the whole game 126.5 KB a
        frame and 198 GC/min; the step, fold and events alone (view rendering nothing) 10.3 KB and 30/min; the view
        alone over a frozen state 80.1 KB and 160/min. About 90% is the view: pong's render code making its elements,
        props, inline style objects and strings each frame, and the lite runtime's output record and child array per
        host element (~10-15 KB of it)
      And so the lever for R1/R2 is Z3, the view compiled at bundle time into update functions bound to what changed
        (no per-frame elements), with Z2's fixed state after it for the last ~10 KB. useGameEvents' per-frame copies of
        its frame lists are a few KB and returning a shared array would break a game that mutates it: left as is
      And F1 in a window stays blocked: no window on the founder's screen without their consent
    Scenario: 2026-10-18 night: where each open row stands, and what blocks it
      Then met: F4 (scrub 12.5-14.6 / 16.6-26.6 ms), Q1 (render = play 1.000000 everywhere checked), Q2 (react/ 0 fail)
      And blocked on the hardware: T1 and T2 sit at the H.264 engine's floor (~275 fps at 1080x1920, one engine, speed
        already prioritized, non-realtime); <= 1 s needs another encode path (HEVC failed in the muxer, turn 3)
      And blocked on the person: F1 in void's window needs a visible window the user agrees to (the lever to try:
        desired_maximum_frame_latency 1); R5 and Q5 need the iPhone unlocked; R7's target (50 MB costs +20-22% frame work
        with opt-level s; ~66-67 MB without it) and the offline player (rustls, ~1 MB) are the founder's calls; R1/R2 to 0
        need ZeroHeap Z2 (game state in one fixed buffer), which changes the SDK games are written against
      And F2 arena3d attributed further: offscreen, a frame's present is Live::prepare (Bevy's update and render
        submit), the quads composite (now 0.72 ms GPU) and the wait for the frame before (two in flight), so its work
        is about one frame's GPU time, ~2 ms of it Bevy's render at low clocks. Measured no: MSAA off (work and present
        unchanged, 3 pairs). Bevy's main passes report no GPU time (Apple GPUs time only at pass boundaries), so the next
        step is a Metal trace of arena3d's frame, in the live-3D area an agent session coordinates
      And L0 complete: bench/scoreboard.sh fills 27 of 27 rows. Two harness faults fixed: export.py timed export-cache
        hits since 3479eea (4fce2f0: T1 read 0.024 s, T4 1.1 M/h; now 2.456 s, T2 2.702 s, T4 1553/h), and F6 counted
        every arena3d frame (3d1ecde: games whose median frame is near 2 ms are F2's, listed). an agent session's e051025 marks a
        run NOT QUIET when another agent's job overlapped it
      And F6/F2 attributed: platformer's 4.6-18.9 ms steps in the last feel.py run were another agent's export.sh
        overlapping it (an agent session: four quiet repeats never spiked, GC pauses <= 1.13 ms, p99.9 3.46 ms); the harness is
        getting a NOT QUIET mark for runs other agents' jobs overlap. Pong's remaining hitch is the score card's first
        frame: its group's first GPU work at clocked-down GPU clocks, the next frame waiting 3.5-5.0 ms. Measured no:
        page-sized group targets from the start (the regrow at that frame isn't the cost)
      And F2 arena3d: a tile one instance covers whole skips the fine pass's staging (0471084, exact): page pass 1.93
        -> 0.72 ms GPU, pong 1.5-2.6 -> 1.5-2.2; frame work level (Bevy and the wait are the rest)
      And R7 (an agent session, 9012fdc): blitz-v8-script and raea-modules gated out of the player (default-on features void,
        browser and the iOS builds keep): 72.33 -> 66.77 MB, export selftests and renders 1.000000
      And R3 against its target: offscreen raea-motion playing pong (no window, no chrome, no void) is already 149-151 MB
        of footprint, so an idle void at <= 100 MB needs another process shape (JSC or the GPU work out of process),
        a design call for the user. Meanwhile the GPU half keeps falling: blur scratch rings out of the threads
        (c68aca2: void idle 653 -> 272 MB), bounded group targets and a growing atlas (195945d, 5ff5843), the blur's
        8-bit scratch (a676f30: titles 56.8 -> 50.1 MB)
      And open, being worked: F6/F2 (pong's card frame on a clocked-down GPU; platformer's step spikes, an agent session
        attributing; arena3d's steady 3D frame: its page pass is one full-coverage texture per tile, 1.9 ms at low
        clocks, so a fast path for single-instance tiles is next), F3 (5 missed in 3 x 60 s, at those same frames)
    Scenario: 2026-10-18, turn 5: the 2D compositor's GPU time and memory (F6, R4)
      Then F6 attributed: pong's one first-use spike (frame 185, ~7 ms) is its score card's full-page group arriving
        while the GPU runs clocked down: the glyph atlas, the instance and bin buffers and the group pool don't grow
        there. The steady cost was the page pass, 1-3.5 ms GPU, from four stacked full-page layers each shaded per pixel
      And quads bins drop what an opaque solid covers whole (4b76f93, exact: an interior span at alpha 255 is the
        source alone): pong's page pass 15532 -> 12182 tile entries, 3.48 -> 2.82 ms median GPU, paired, load 12
      And a group's pass paints only its picture's tiles (eae9f02): group passes median 0.197 -> 0.12 ms, p90 0.93 ->
        0.33-0.41 ms. Its target is sized to those tiles (195945d) and the glyph atlas starts at 1 MB and grows
        (5ff5843): R4 pong 47.7 -> 19.75 MB, titles 62.8 -> 44.25, platformer 28.1 -> 12.7
      And each lever is byte for byte (quads::tests::opaque_cull_is_exact against RAEA_QUADS_CULL=0, atlas_grows), each
        test checked by a mutation that fails it; replay and titles SSIM 1.000000 on every build
      And a fill covering a whole tile is only painted there (d06d52e: the bin entry's FULL flag, exact): pong's page
        pass 2.99 -> 2.45 ms, frame GPU p99 3.84-3.96 -> 3.03-3.45 ms, paired. Dropped, measured: staging instances in
        workgroup memory (~4x slower: occupancy)
      And L9 (an agent session, 4100b50): void's stage drew thickened glyphs because dioxus-native's default features compiled
        blitz-paint's apple-font-embolden into the whole process; it's per document now (on for void's chrome only).
        ts1 full-page seeks vs render 0.997160-0.998681 -> 1.000000, paired
      And F4 attributed (an agent session): k4's 4K seeks are the hardware decoder's (1.6-1.8 ms a reference frame, ~25 per seek
        deep in a 1-2 s GOP), not restarts. Lever chosen: an intra-only mezzanine for long-GOP sources over 1080p
      And R3 attributed: an idle void's footprint was 653 MB, 566 of it GPU memory neither RSS nor Metal's device count
        shows: the three-box blur's (00d156f) two 4 KB rings per thread, which the driver backs with scratch for every
        thread the GPU holds. In a buffer sized to the blur (c68aca2, byte for byte): void idle 653 -> 272-278 MB,
        titles offscreen 535-542 -> 165-173 MB, a titles render 3.6-3.8 -> 3.4-3.5 s. R3's RSS row never saw it, so R3
        now carries footprint too
      And the export cache keys the build (7230a2b): a rebuilt engine was answered with the last one's video
      And an agent session: F4's 4K scrub from a ProRes 422 copy made in the background (f83ae45: p99 52.2 -> 17.7-28.2 ms,
        seeks = render 1.000000); R7's player keeps a dSYM beside it and crash files carry UUID and slide (af0c12f)
      And T2 attributed: a 3D export is the H.264 engine's (frames 3.6-4.3 ms, 3.3 ms of it feeding the encoder, which
        already prioritizes speed and runs non-realtime); setup is ~0.5 s. Its 1 s target needs another encode path
      And R3's rest attributed (idle void, dirty): GPU 126 MB (the device counts 65-96), malloc 44 MB spread thin
        (Firewheel graph buffers 8, AppKit/winit 9, wgpu command encoding 4.6, a kept serde_json Value 3.3, dioxus
        hooks 2). Measured no: passing the fine shader's instance by index instead of by value (scratch unchanged)
      And F2 on arena3d attributed: Bevy's passes measure ~0 ms on the GPU; the frame's GPU cost is quads' page pass
        compositing the 3D view (one nearest-sampled full-page texture) and the HUD. It also shaded a full-page fill at
        alpha 0; binning skips alpha-0 instances (0f2a95c, exact): page pass 2.46 -> 1.93 ms, paired. What's left is
        the pass's fixed cost over 3600 tiles at a clocked-down GPU (pong's 12k entries take ~2.4 ms, arena3d's 3.6k
        1.93), so the next lever is the 3D view not being composited at all where nothing covers it
      And glyph misses (the scrub tail and pong's first-use frames): a miss trimmed its raster with a byte-at-a-time
        scan of a canvas ~35x the glyph (83c1881: trim 0.221 -> 0.062 ms), and rendered on that canvas (28e4ad4: from
        the outline's bounds, an edge check falling back to the full canvas; byte for byte, glyph_canvas_is_exact).
        Pong's 13-miss frame 1.42 -> 0.47 ms record, the 30-miss one 1.13-1.21 -> 0.52-0.55
      And feel.py at load ~4 after them: pong 57 hitches (its card frame, GPU), platformer 35 (game steps of 4.6-18.9
        ms around 14.6 s, being attributed), counts swinging run to run; F3 5 missed in 3 x 60 s
      And R7 (an agent session, bc82df0): AV1 gated out of the player, 74.55 -> 72.33 MB, export selftests and renders 1.000000
      And F4 met (an agent session, b0f6e8e): a seek's present was 6.2 ms of Dom::load_fonts re-parsing every stylesheet (the
        injected base64 @font-face included) after each reset re-created the <style>s; it's skipped while the font CSS
        is unchanged (60/61 hits, 0.02 ms). ts1 scrub p99 12.5-12.8 ms with range copies; check.sh, gate.sh and parity
        unchanged. Left in the tail: glyph misses on a caption's first sight (3-5 ms record on 10 of 61 seeks)
      And F4 (an agent session): ProRes copies of just the ranges an edit uses (5cd890d, mapped by time, 449 MB for ts1): ts1
        scrub p99 23.5-24.5 -> 19.7-20.0 ms, step p99 12.4 -> 7.8; present (p99 12 ms) is now the larger half. R3:
        decoding 4K copies through AVAssetReaderVideoCompositionOutput kept its own pool of 4K frames (footprint 182
        -> 687-705 MB); identity tracks read through AVAssetReaderTrackOutput, pixels identical: 290-292 MB (b4e7cc8)
      And F4/Q1 in void (an agent session, 9e5f358): copies made mid-session left the stage's decoders on the sources while
        renders opened the copies (seeks vs render 0.993-0.996); void now reopens the decoders once a copy is made.
        k4 in a headless void: scrub p99 52.2 -> 9.1 ms, missed 2 -> 0, seeks 1.000000. R3 cost: RSS max 280 -> 434 MB
        with 4K ProRes in the decoder
      And Q2 (an agent session, 71a349d): every react/ parity case passes (8 exact, 8 ok, 0 fail); shadcn 0.9997
      And CI (an agent session, a599687): the games gates failed every CI run since ~13:30 because a Standard launchd agent's GPU
        work is scheduled low (present p99 11.4 ms, 170-260 missed); the plists are Interactive now (1 missed, p99 3.6
        ms). The rest (~3000 hitches over 2 ms in 30 s against 3 from a terminal) is frontmost-app priority, so CI's
        F-rows read void-not-frontmost, and F2/F3 history before a599687 is polluted. mezz's cap is min(20 GB, free -
        20 GB) with a no-room refusal (bd31c4e)
      And next: the card's group is still a full-page pass while it fades; backdrop pages at their bounds; R3's GPU
        rest (the device's 65-96 MB at idle, by resource)
    Scenario: 2026-10-18, turn 4: 3D games, exports, the blur, the bake
      Then F2 3D: arena3d's frame was all rdb's Bevy update; the live engine runs single-threaded (4353e23), p99 3.7-4.1
        -> 2.7-3.2 ms paired; neutral on the hanabi and avian renders
      And F5 3D first launch: pipelines compile on the task pool with each frame waiting for what it needs (87b8388,
        ~300 ms of 4.5 s); the exported player paints its window's own background first (1f9c519: 55 ms) and starts its
        clock when playable. Deferred, with a measured estimate: vendoring bevy_pbr to make SSAO, atmosphere and light
        probe pipelines lazy would take a first-ever launch ~4.5 -> ~3 s (they're 11 of 26 compute pipelines; the rest,
        mesh preprocessing and culling, can't go: the CPU path loses meshes). Metal's cache is keyed by executable path,
        so a shipped cache doesn't help
      And T3: the bake splits across 4 JS runtimes (0f36ae7), 600 frames 107-116 -> 49-54 ms
      And Z3: with <Animate> giving back identical elements (fafd132), the lite runtime skips a component given the same
        element with unchanged context reads and no stale state (87e77b2): animate fixture 31 -> 6 KB/frame
      And T1: the export cache (3479eea) answers a repeated export in 0.04 s, byte-identical; void exports an idle
        object ahead at background priority (3e33a9c) so the agent's render is a copy; a filter's blur runs Skia's three
        boxes as running sums (00d156f), byte-identical, titles 18.5 -> 5.9 ms a frame
      And R7: exports ship 74.5-77 MB with the game profile's player (351c3cb) instead of 129-132 MB
    Scenario: 2026-10-18, turn 3: void launch to ~310 ms, T1 is the encoder's, R4 attributed
      Then F5.void_launch: frame 0's sound step waited 112-119 ms for the one mixer core's clock (0.1 s of device audio,
        settle_clock); the stage keeps a spare mixer opened ahead (8c0a71c), so the step is ~0 and spawn -> first stage
        paint went 453-485 -> 397-399 ms, then 309-316 ms as load fell (under load 10-27). Where the rest goes (startup
        marks): process 15 ms, window and dioxus to app mounted ~125 ms, dioxus's first render before the launch task
        ~65 ms, workspace and show <5 ms, open ~70 ms (bundle + cached bake), paint ~12 ms
      And T1 is bound by the hardware encoder: VideoToolbox H.264 at 1080x1920 encodes ~250 fps alone (1.17-1.19 s for
        300 frames), and two sessions at once take twice as long each (one engine). ts1's 595 frames need ~2.4 s, and T1
        is 2.51 s, so painting in parallel (L1) can't reach <= 1 s. What could: a faster codec path (HEVC needs muxer
        work; the experiment failed in the muxer) or a smaller output. Until then T1's floor is the encoder's
      And R4, attributed with quads::gpu_allocated_mb (Metal's own count): pong 24.2 MB at device creation (the glyph
        atlas alone is 16 MB, 4096^2 R8), 36.3 after the first frame, 47.7 after a second, flat after (no leak); titles
        109 MB at 1080x1920, from page-sized pools (page, a group texture per nesting level, the backdrop's two pages and
        a half-float scratch for a blur). Levers: an atlas that starts small and grows; group and backdrop textures at
        their bounds, not the page's
    Scenario: 2026-10-18, turn 2: F5 opens, Q1 chips, and a measured no on Z3's bailout
      Then F5: CoreAudio warmed while an object opens (b841331: mixer open ~220 -> 130-148 ms), the bake kept on disk
        by bundle bytes + build_id (324006d: a hit equals a fresh bake whole), the bundle kept by its inputs (94051fd:
        titles' open 80-95 -> 47-57 ms). All under load 36-150, so the board waits on quiet runs
      And Q1: inline-block chips laid out ~17 px taller after any restyle, from a Taffy cache hit dropping baselines;
        fixed in vendor/blitz-dom (ddf67c7), the repro 0.972 -> 1.000000, parity unchanged (93/75/2)
      And Z3, tried and reverted: a React.memo-style bailout in sdk/react/js/lite (props shallow-equal, no state changed,
        every context the subtree read unchanged) made JS garbage worse on all four edits (animate 65 -> 83, twin 39 ->
        48, titles 31 -> 39, pong 113 -> 146 KB a frame; tries at load 9-17), because nothing bailed: <Animate>'s
        useTime reads the Clock and LocalTime contexts every render even when Rust samples it, Sequence's LocalTime is
        a new object each frame, and props are new objects (inline styles, pop(), track()). The payoff needs, first,
        components that don't read the frame when the engine samples for them (sdk/motion/src/animate.tsx, the animation
        agent's), and then props compared by content (animate.tsx's fingerprint shows the way), or Z3's compiled view
    Scenario: 2026-10-18, turn 1: F5 void launch and the settle race (aa4f78a)
      Then attribution: the void launch's 1187 ms went 105 ms to MCP, 248 + 252 ms to two 250 ms ticks of the stage
        watcher, and about 1 s to the stage's open. The bake in it lost a React flush wakeup and waited out 0.5 s
        (562 ms for 600 frames against 67 ms in raea-motion)
      And the fix: settle reads the flush count before its first update (every render, bake and live frame goes
        through it), and the watcher wakes on a change of space, ticking 16 ms only while an open is pending
      And the result, under load (quiet pending): launch 517-552 ms; the stage's open 141-155 ms; a paired games A/B
        shows p99.9 507 -> 18-36 ms and worst frame 509-606 -> 34-92 ms. Quality unchanged (replay, timelines, audio)
      And measured noes: making the GPU stage early saved 5 ms but delayed the first drawable ~1 s, so it was dropped;
        the menu's WritingToolsUI load is 5 ms (the sampler made it look like most of launch)
      And next for F5: cache the bundle and bake on disk by source hash (open ~150 ms -> ~0), and start the mixer and
        the mastered sound beside the bake (+130 ms "ready" after the open)
    Scenario: 2026-10-18, L0: the scoreboard (e0498ab)
      Then bench/scoreboard.sh and bench/lightspeed/ (startup, feel, export, quality, gpumem, energy, ios) give every
        row a harness; CI's full tier runs them nightly. T2's old value was a 60-frame render (fixed: t3d600)
    Scenario: 2026-10-18
      Then the plan was set; the scoreboard values above come from today's commits (615d301, ad90377, 29a5417, 28f3221,
        d2fc732, ef23996 and the gate runs in their messages)
