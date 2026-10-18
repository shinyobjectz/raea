Feature: The agent's media and 3D, the same on the Mac and the iPhone
  The founder, 2026-10-16: "your goal right now is to figure out how the agent is going to do or be able to do media
  management and including 3D like Manifold stuff too ... on iPhone with raea, wherever we have to be able to run
  essentially ffmpeg-like logic and be able to use something like Manifold." Earlier the same morning: the agent works
  the way models do now, with bash and file edits, inside a sandbox (Wasmtime), every action a row for tabular
  dreaming; and "what if manifold was our syntax for both ffmpeg and 3D objects ... a manifold video type thing".

  Today the agents shell out to ffmpeg and ffprobe for media (void/src/brain.rs: probe a duration and frame rate, pull
  a still, measure loudness, extract 16 kHz audio for speech-to-text; ugc-agent watch.rs: a smaller copy for the judge),
  and Buddy on the iPhone asks the Swift host (AVFoundation) for the same through host calls. Neither works everywhere:
  there is no ffmpeg on the iPhone, and the host calls exist only in the host app. raea already holds the pieces to do all of
  it in Rust (raea-media: VideoToolbox/AVFoundation and software decoders and encoders, a muxer, loudness, resampling;
  rrl render for composition), and Manifold is planned for 3D (engine/docs/bevy.feature B4, B5c) but not yet in.

  Rule: Measured (spikes in an agent session's scratchpad, M4 Mac, 2026-10-16)
    Scenario: Manifold in pure Rust, and video as solids in (x, y, t)
      Given manifold-rust 0.16.0 (Apache-2.0; a port of Manifold 3.5 checked against the C++ reference; its dependencies
        are MIT, Apache-2.0 or BSL-1.0; no C++ and no CMake)
      When two full-frame clips are boxes in (x, y, t), a wipe from A to B over frames 60-90 is a plane tilted in x-t, and
        a 300 px hole moving across the frame is a disc extruded through time and sheared
      Then the edit's solids build in 2.9 ms (about 335 triangles each)
      And slicing them at a frame gives what each source fills on screen in 0.03 ms a source: A 78.3% and B 18.3% at
        frame 65, exactly (65.5 - 60) / 30 of the wipe, the hole cut from both
      And a brief's "the caption never covers the face" is one intersection: the caption box (y 1500-1620) against a face
        track moving down 4 px a frame meets it at frames 75-150, found in 0.08 ms over the whole edit
      And it builds for aarch64-apple-ios unchanged (a 2 MB test binary)
    Scenario: The media layer on iOS
      Then raea-media (apple, soft and av1 backends) compiles and links for aarch64-apple-ios with IPHONEOS_DEPLOYMENT_TARGET=17
      And a 96 kHz AAC track decodes at its true rate (34deaa1; before it, Symphonia's MP4 reader said 0 and the mix panicked)
    Scenario: Wasmtime on Pulley, the engine the iPhone allows (no JIT)
      Given wasmtime and wasmtime-wasi 48.0.5, the project folder preopened read-write, a fuel limit
      Then a WASI command lists and writes the project folder, can't open /etc/passwd ("No such file or directory"), and
        is logged as one action row (command, exit, fuel, compile and run time)
      And Pulley ran its busy loop in 21.7 ms against Cranelift's 3.0 ms (about 7x): fine for files and text, not for pixels
      And the host builds for aarch64-apple-ios (14 MB with Cranelift in; Pulley alone, with modules compiled ahead of
        time on the Mac, is not measured yet)

  Rule: One media layer, three ways in, all native
    The pixels and samples never run in WebAssembly: on the iPhone it would be an interpreter. Media work is raea-media
    and the renderer, called as host commands from the sandbox, the same binary logic on the Mac and in HostCore.

    Scenario: Media verbs (rdb media), what today's ffmpeg and ffprobe calls become
      Then the agent has these, each a command with the arguments a model expects and a JSON or file result:
        | verb                                             | replaces today                                          | built on                         |
        | rdb media probe FILE                             | ffprobe duration, size, fps, rotation, audio streams    | raea-media Info, index           |
        | rdb media still FILE --at S [-o PNG]             | ffmpeg -ss S -frames:v 1                                | decoder seek, as_of              |
        | rdb media sheet FILE... [--n 12]                 | contact sheets (footage scan, look)                     | still + tile                     |
        | rdb media audio FILE --rate 16000 --mono -o WAV  | ffmpeg -ar 16000 -ac 1 (speech-to-text)                 | decode_audio, soft_resample      |
        | rdb media loudness FILE [--from S --to S]        | ffmpeg -af ebur128 / volumedetect                       | audio::lufs, momentary, peak_db  |
        | rdb media proxy FILE --height 720 -o MP4         | the judge's smaller copy (watch.rs)                     | decoder + encoder (VideoToolbox) |
        | rdb media trim FILE --from S --to S -o MP4       | ffmpeg -ss -t -c copy                                   | remux (muxide), keyframe aligned |
        | rdb media concat A B ... -o MP4                  | ffmpeg concat                                           | remux when codecs match, else an edit |
        | raea render (a TypeScript project)               | filtergraphs (overlay, crop, scale, xfade, drawtext)    | the edit and the renderer        |
      And every verb runs the same on the Mac and the iPhone (the apple backend first, soft as the fallback)
    Scenario: ffmpeg and ffprobe as words the agent already knows
      Given models write ffmpeg by reflex
      When the agent runs `ffmpeg` or `ffprobe` in its shell
      Then a common subset is read and run as the verb or as a small edit: -ss/-t/-to trims, -frames:v 1 stills,
        -vf scale/crop, -ar/-ac audio, overlay, xfade wipes and fades, concat, -c copy
      And anything outside the subset is refused with the verb or edit row that does it, never run some other way
      And the action row records the verb it became, so dreaming learns one vocabulary
    Scenario: No ffmpeg binary ships
      Then ffmpeg is never bundled: its builds are LGPL or GPL, it is large, and on the iPhone it could only run
        interpreted; the Mac's ffmpeg stays a test reference (the parity and SSIM checks), as today

  Rule: Composition is solids in space and time (Manifold)
    An rrl clip row {x, y, w, h, from, to} is already an axis-aligned box in (x, y, t), and z-order is a union. Manifold
    makes the box any solid, so masks, wipes and cut-outs are shapes, not effects.

    Scenario: A row's shape in time
      Given a clip, image or box row with a `shape` column, a CSG expression over (x, y, t):
        `clip b {src: t2, from: 60, to: 150, shape: wipe(right, 60, 90)}`
        `clip a {..., shape: minus(frame, extrude(circle(150), path((200, 960) -> (880, 960))))}`
      Then the edit's solids are built once per change and cached by the expression's hash
      And each frame slices them: the 2D polygons say which source fills which region, and the GPU draws those polygons
        with the decoded frames (kgpu's composite takes a mask per source); no per-frame geometry beyond the slice
      And a row without a shape stays the box it is today, drawn as today (the gate unchanged)
    Scenario: Briefs as volumes
      Then overlap, inside the frame, keep-off-the-face and gaps are intersections and differences of solids, answered
        over the whole edit at once with the frames where they fail (the spike: 0.08 ms), alongside the per-frame checks
    Scenario: Where solids stop
      Then the third axis is time, so a 3D object moving through time is not one solid: 3D shapes are Manifold meshes with
        keyed transforms in Bevy (next rule), and space-time solids cover the 2D picture plane
      And looks (grade, blur, key) change what fills a solid, not its shape: they stay columns

  Rule: 3D shapes are Manifold meshes in Bevy
    Scenario: A shape is a value
      Given `mesh sign {shape: minus(rounded_box(2, 1, 0.2, 0.1), text3d("SALE", 0.4, 0.3)), color: #e11d48}`
      Then manifold-rust builds it, its MeshGL becomes a Bevy Mesh (positions, normals, indices), cached by the
        expression's hash and rebuilt only when it changes (bevy.feature B5c)
      And text3d is the engine's own glyph outlines (the faces the renderer already loads) as cross-sections, extruded
      And cube, sphere, plane and panel stay as they are; shape is one more column, so 3D lowered from React keeps working
    Scenario: 3D files
      Then `rrl media mesh SHAPE -o FILE.glb` writes a shape for other tools, and an imported .glb or .stl is a shape too
        (the robust engine takes real-world meshes: non-manifold, self-intersecting)

  Rule: The sandbox boundary and the action rows
    Scenario: What runs where
      Then generic commands (ls, cat, grep, rg, jq, wc, sort, head, tail, find, mv, cp) are WASI modules in Wasmtime
        (Pulley on the iPhone, Cranelift on the Mac), with only the project folder preopened
      And rrl, rrl media, the renderer and Manifold are host commands: native speed, the same arguments, called through the
        same boundary
      And every call through the boundary is one action row: command, verb, arguments as columns, files read and written,
        bytes, exit, fuel or time, error, and the project's state hash before and after; this is what tabular dreaming reads
      And a file written through the boundary is snapshotted by content hash, so any command can be undone and redone
        (edits already have this as rrl's move log)

  Rule: The sandbox as built (2026-10-16, raea/sandbox)
    Scenario: On the iPhone
      Given sandbox/build.sh phone: wsx (Wasmtime 48, runtime and Pulley only, no compiler on the phone: only Cranelift's
        bitset, entity and bforest crates come along), modules compiled ahead of time by wsxc on the Mac, without address
        maps (a quarter smaller), the probe app and its control signed with the ASC API key
      Then the probe on the iPhone 13 Pro Max runs 14 of 14 cases as they must: ls, wc, sort, head, grep (-rn), jq,
        a write and its read-back, find, sed in 0.7-17 ms each; /etc/hosts isn't there and ../../../etc/hosts is
        "Operation not permitted"; a run past its fuel and one past 64 MB of memory are stopped with that said
      And the same suite passes on the Mac (14/14) and in the iOS Simulator (14/14)
    Scenario: What it adds to the app
      Then the lean set adds 9.2 MB (WsxProbe 9352 KB against WsxControl 140 KB): the runtime 3.4 MB, rtools 0.6 MB
        (ls cat head tail wc sort uniq cut tr cp mv mkdir rm touch echo grep find sed basename dirname, std and
        regex-lite, written for the budget), jaq 5.4 MB; inside the 15 MB budget
      And the full set (uutils coreutils, ripgrep, jaq) adds 26.8 MB installed (10.4 MB compressed, as a download):
        Pulley bytecode is about three times a module's wasm code, and coreutils carries 2.8 MB of locale tables; it
        runs the same suite (12/12) but doesn't fit the budget, so it isn't the shipped set

  Rule: Not doing
    Scenario: Ruled out
      Then no ffmpeg binary in the app, and no media work in WebAssembly
      And no C++ Manifold: the Rust port builds everywhere with cargo; if it ever diverges from the reference, the C++
        library through manifold-csg is the fallback
      And no cloud sandbox: footage stays on the device

  Rule: Steps, each proved before the next
    Scenario: M1. Media verbs, on the Mac then the iPhone
      Then rrl media probe, still, sheet, audio, loudness, proxy, trim and concat land with tests against ffmpeg's answers
        (duration and fps exact, stills SSIM >= 0.98, loudness within 0.5 LU, trims frame-exact at keyframes)
      And the ffmpeg/ffprobe reader covers every ffmpeg call in void/src/brain.rs and ugc-agent watch.rs, and void's
        tools switch to the verbs (with an agent session)
      And the verbs run on the iPhone through ios-probe or HostCore with the same answers
    # 2026-10-17, the verbs moved into rdb with rrl's deletion (rdb/src/media_verbs.rs: `rdb media`, `rdb ffmpeg`,
    # `rdb ffprobe`, same answers; motion/tools/media-vs-ffmpeg.sh runs rdb). Earlier notes name rrl, where they began.
    # 2026-10-16, verbs and readers in (rrl/src/media.rs; `rrl media`, `rrl ffmpeg`, `rrl ffprobe`). rrl/bench/media-vs-ffmpeg.sh
    # runs the same command lines through ffmpeg and rrl: 60/60 on the colitt takes (H.264 and HEVC, 44.1 and 96 kHz, a
    # variable-rate phone take averaging 24 fps on a 30 fps grid) and a 55 s render. Exact: sizes, r_frame_rate and
    # avg_frame_rate (read from the sample tables, signed ctts as QuickTime writes them), container durations (mvhd; one
    # file 24 ms off where ffprobe takes the longest stream), 16 kHz mono samples (stereo folds at -3 dB a side, as -ac 1),
    # volumedetect and ebur128 to 0.1, scene scores to 3 decimals (luma only, as f_select scores YUV) and every cut time,
    # YDIF at fps=6 (each tick reads the last frame before its middle, as the fps filter does), freezedetect starts, durations and ends. Stills SSIM 0.992-0.998.
    # Trims are re-encoded, frame-exact (60 frames for 2 s); concat fits a different-shaped take inside the first's frame
    # (each part SSIM 0.98 against its source). The proxy (watch.rs's command) is the one gap: VideoToolbox at 0.08 bits a
    # pixel gives SSIM 0.932 against the source at 2.1x the bytes of x264 -crf 28 (0.949). Known ffmpeg quirk: signalstats
    # reported YDIF 7.7 between two pixel-identical black frames at a keyframe; rrl says 0. void's agent-side calls moved
    # onto `rrl ffmpeg`/`rrl ffprobe` with an agent session (void b8a6abb; eval.rs's grader stays on real ffmpeg on purpose).
    Scenario: M2. Manifold shapes in 3D
      Then a mesh row's shape column renders in Bevy (a rounded box with SALE cut through it), cached by hash, on the Mac
        and on the iPhone, and the engine's gate is unchanged
    # 2026-10-16, done on the Mac: rdb/src/shape.rs (manifold-rust 0.16, skrifa outlines of the bundled faces), the
    # `mesh` kind in rdb's engine, cached per expression; normals smoothed within 30 degrees. rrl/examples/shapes/sign.rrl
    # renders the rounded sign with SALE cut through (letters' counters kept), turning by a key, a torus and a hollow cup.
    # rdb 33/33 (3 shape tests: CSG volumes, a ringed O, errors), rrl 84/84, gate 16/16; rdb checks for aarch64-apple-ios.
    # Not yet: a frame on the iPhone itself (rides with M5's phone run).
    Scenario: M3. Space-time shapes in the picture
      Then a wipe, a split screen and a moving cut-out render from shape columns, matching the same edit made by hand
        (frame by frame), with the time per frame recorded on the CPU path and kgpu
    # 2026-10-16, done: a clip's `mask` column is a solid in (x, y, frame) (shape.rs: frame, rect, disc, wipe, during,
    # slide, and the booleans), sliced through each frame's middle and rasterised to coverage (4 sub-rows, exact along
    # the row). kgpu's composite takes it as M, a byte a slot (on = 2 marks a masked slot; opacity * coverage / 255, so
    # an unmasked slot is bit-identical: gate 16/16). comptest: a masked slot is equal on the GPU, the zero-copy kernel,
    # the K interpreter and both Rust twins. rdb/examples/masks: a wipe (30-60), a split at 540 and a hole moving 680 px,
    # 5 frames each, against clip A's and clip B's own renders spliced by the geometry worked by hand: 0 pixels differ
    # outside a 2 px edge band; the direct GPU and CPU renders give equal hashes on all 270 frames. Not covered: the
    # sketch (proxy) kernel ignores masks; the UI-image clip path refuses one.
    Scenario: M4. Briefs as volumes
      Then the face, overlap and inside-the-frame checks answer from solids, with the same verdicts as the per-frame
        checks on the gate's examples
    # 2026-10-16, done: rdb/src/volumes.rs (a row's box per frame as one-frame prisms, runs merged; failing frames read
    # back from slices) and `rrl volumes EDIT [BRIEF]`, which answers "stay inside the frame" and "sit above the face by
    # N px" from solids over the per-frame check's own rows (its Given and When), and every 2D kind's overlaps, beside
    # the per-frame answers. Same verdicts and frames everywhere: hook and ugc briefs; G2's bad snippets
    # (rdb/examples/hook/g2_volumes.py, 7 of 7; the caption on the face at 60..=149, the subtitle off frame at 0..=149);
    # overlaps in the games (breakout's ball on b6 at 63..=66 and the paddle at 162..=164, pong's at 54..=56). On these
    # small edits solids are slower than the frame loop (12-98 ms against 2-5 ms); their use is answering over spans and
    # shapes a per-frame test can't express, not speed yet.
    Scenario: M5. The sandbox
      Then Wasmtime with Pulley runs the generic commands on the iPhone with modules compiled ahead of time, the host
        commands reach M1's verbs, and every call writes its action row; the app's added size is measured (sandbox.feature
        in ~/host allows 15 MB)
