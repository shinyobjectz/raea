Feature: The meaning plane: what footage shows, as columns
  Status 2026-10-14: working and checked end to end on this Mac. raea-meaning turns footage into frame-embedding
  columns once, and everything after that (search, find-or-make, cut spikes, temporal fingerprints) is K kernels over
  those columns (src/k.rs). The same k.rs is compiled into rdb (rdb/src/lib.rs `#[path]`), so the hypervectors previz
  makes and the ones the meaning plane makes are one implementation.

  (2026-10-15: the embedder crate ugc-embed is now raea-embed, its models in ~/.local/share/raea-models; the old names
  below are what they were called when measured, and still work: agent/agent.feature)

  Rule: One model, run once, at ingest
    The vision model is the host app's embedder (ugc-embed, ~/host/agent/crates/ugc-embed): SigLIP 2 base p16-256
    (Apache-2.0), ONNX on ONNX Runtime, 768 floats per frame, L2-normalised. The phone runs its Core ML build
    (coreml/ImageEncoder.mlpackage, 8-bit palettised), which matches the ONNX reference at cosine 0.9956 mean and
    0.987 worst on our frames (T-410, 2026-10-13). The models are in ~/.local/share/ugc-models/siglip2-base-p16-256/.
    A composition never calls the model; it only reads what ingest wrote (the rule from engine/docs/ml-tracks.feature).

    Scenario: The commands
      Given meaning/target/release/raea-meaning (built with `lane build -- cargo build --release` in meaning/)
      Then it does
        | command                                   | what                                                                 |
        | ingest VIDEO -o COL [--fps N] [--int8]    | every frame (or N a second) -> SigLIP 2 -> COL.f32 + COL.json        |
        | dist COL                                  | the distance between each frame and the one before it                |
        | cuts COL [--w --ratio --floor --gap]      | shot boundaries: spikes in that distance against its local median   |
        | search COL "text" [-k]                    | the frames most like a phrase, with times and scores                |
        | find LIBDIR "text" [-k]                   | the best frame of each column in a folder, best first (a library)    |
      And a column is COL.f32 (n x 768 little-endian floats) beside COL.json (source, model, fps, n, dim)
      And the source is stored as an absolute path (since 2026-10-14), because find() cuts frames out of it later
      And `cargo test --release` in meaning/ passes 5 of 5 (the K kernels)

    Scenario: Ingest cost, measured 2026-10-14 on the M4 Mac mini (shared with an eval run)
      Then the UGC example's footage ingested at
        | clip              | length | frames | seconds | ms per frame |
        | take-h264.mp4     | 25.6 s | 769    | 99.8    | 130          |
        | broll-screen.mov  | 4.0 s  | 100    | 18.3    | 183          |
        | broll-rot.mov     | 8.0 s  | 240    | 31.2    | 130          |
      And that is ONNX on 4 CPU threads; the phone's Core ML build is the fast path and is not measured here

  Rule: Find-or-make: snippets ask for footage by meaning
    rdb's `find('…')` and `make('…', seed)` (rdb/src/assets.rs, G4) search the footage library first and fall back to
    generation (a deterministic stand-in today). Rows hold only `asset:<sha256>`; how the asset came to be is a
    provenance row, used only by an explicit `remake`, which is a recorded move.

    Scenario: Where things live
      Then the library is every column in $RAEA_LIBRARY, default ~/.cache/raea-meaning/cols
      And the binary is $RAEA_MEANING, default meaning/target/release/raea-meaning beside rdb
        (fixed 2026-10-14: the default still pointed at ~/raea-meaning/ from before the rename, so find() failed)
      And a found asset is the source's frame at the hit's time, cut out by ffmpeg and stored under its hash
      And evals never write into the library: cuts_eval.py and cuts_fusion.py cache their columns in
        ~/.cache/raea-meaning/eval-cols (they wrote into the library before 2026-10-14)

    Scenario: The threshold
      Given FIND_THRESHOLD = 0.10 (SigLIP 2 text-image score)
      When the UGC example's three clips are the library (2026-10-14)
      Then the scores for phrases fall either side of it
        | phrase                                   | best hit                  | score |
        | a man with glasses talking to the camera | take-h264.mp4 at 0.7 s    | 0.146 |
        | a woman holding a phone                  | take-h264.mp4 at 10.1 s   | 0.105 |
        | a computer screen                        | broll-screen.mov at 3.4 s | 0.108 |
        | a skincare bottle (not in the footage)   | take-h264.mp4 at 22.1 s   | 0.043 |
      And rdb/examples/assets/g4.py passes 7 of 7 (found for a phrase the library has, made for one it lacks,
        hashes in rows, bytes stored once, a byte-identical re-render with retrieval and generation off, remake as
        a recorded move, undo)

  Rule: Cuts from embeddings, and from TransNetV2
    Scenario: Against the current detector, on renders whose cuts are known
      Given meaning/bench/cuts_eval.py: Buddy's renders (~/host/agent/evals/runs), truth from their timelines,
        scoring and the train/held-out split from ~/host/knowledge/drive/bin/shot_detector_fit.py (precision
        against every cut, recall against the visible ones, within 0.1 s), every detector's thresholds
        grid-searched on the train half
      And TransNetV2 (official MIT weights, converted 2026-10-14 to ONNX in meaning/bench/transnetv2/; per-frame
        probability within 5.2e-7 of the TF original) scored as peaks of its per-frame transition probability
      When it ran on the 10 newest renders on 2026-10-14 (28 visible cuts; tuned on 5, scored on 5)
      Then
        | detector                                   | train F1 | held out P | R     | F1    |
        | current (ffmpeg scene score + luma change) | 0.837    | 0.448      | 0.909 | 0.600 |
        | SigLIP 2 neighbour-distance spikes          | 0.583    | 0.347      | 0.909 | 0.502 |
        | TransNetV2 (single head, p >= 0.10)         | 0.710    | 0.800      | 0.727 | 0.762 |
        | current + TransNetV2 (many head, p >= 0.6)  | 0.870    | 0.448      | 0.909 | 0.600 |
      When it ran on the 40 newest renders on 2026-10-14 (110 visible cuts; tuned on 20, scored on 20)
      Then
        | detector                                   | train F1 | held out P | R     | F1    |
        | current (ffmpeg scene score + luma change) | 0.704    | 0.662      | 0.667 | 0.664 |
        | SigLIP 2 neighbour-distance spikes          | 0.543    | 0.326      | 0.719 | 0.449 |
        | TransNetV2 (single head, p >= 0.15)         | 0.512    | 0.590      | 0.404 | 0.479 |
        | current + TransNetV2 (many head, p >= 0.6)  | 0.715    | 0.671      | 0.702 | 0.686 |
      And TransNetV2's 10-render lead doesn't survive 40: it finds a cut to another source (24 of 29 across the 40,
        ffmpeg 23) but not a jump within one take (21 of 81, ffmpeg 54); it gives most jump cuts a probability
        under 0.01, because a jump inside one shot is not a shot boundary in its training data
      And so (2026-10-14) raea keeps the ffmpeg detector for "every visible cut"; SigLIP spikes are out.
        TransNetV2 is the better answer to "where does the shot change" (source changes, dissolves) and adds a
        little as a second voice (F1 0.664 -> 0.686 on 20 held-out renders, about two cuts); not worth a 31 MB
        model for that alone
      And the current detector's own 40-render fit (2026-10-12, an older 40) scored held out P 0.90 R 0.65; the
        newest 40 are harder for it (P 0.66 R 0.67)
      And meaning/bench/cuts_fusion.py fuses ffmpeg's scene score, the luma change and the SigLIP distance into one
        classifier (TabICL against gradient boosting); it needs a venv with tabicl and scikit-learn and has no
        recorded result yet. TransNetV2's probability is a natural extra feature for it
      And next, not built: run TransNetV2 on rdb previz sketches (rdb/src/previz.rs) to answer whether an authored
        cut "reads as a cut", for cuts between different shots only (it is blind to jumps within a take)

    Scenario: How TransNetV2 would run in the apps (noted, not built)
      Given the model is 31 MB of 3D convolutions on 100 frames of 48x27 RGB, the same 48x27 grid Perception
        already samples (perception/Perception.swift)
      Then on the phone: Core ML from the PyTorch port (coremltools; the colour-histogram scatter_add and the
        bit shifts may need rewriting as one-hot sums), checked against the ONNX on the same frames
      And in Rust for raea, where perception is moving: ort with the existing ONNX (CPU, or the CoreML execution
        provider), or Core ML through objc2; ort is the shortest path since parity is already measured there
      And at native frame rate it costs ~12-22 ms per frame on an M4 CPU (each frame is computed twice: windows
        of 100 step by 50), so a 30 s clip at 30 fps is ~15 s on CPU unless the Neural Engine takes it

  Rule: Time, without a video model (bench 2026-10-13)
    Scenario: SigLIP 2 frames plus HDC transitions against V-JEPA 2
      Given meaning/bench/temporal/: 359 two-second windows from 21 source groups, classification by GroupKFold,
        retrieval of each window among 718 forward and reversed windows (RESULTS.txt has the method)
      Then
        | method                              | forward/reverse acc | R@1   | twin  | cost per window            |
        | SigLIP mean-pooled                  | 0.500               | 0.485 | 0.500 | ~960 ms (16 frames, CPU)   |
        | HDC transitions, position permuted  | 0.542               | 0.982 | 1.000 | +0.8 ms (8192 bits)        |
        | HDC mean + transitions              | 0.528               | 0.985 | 1.000 | +0.8 ms                    |
        | V-JEPA 2-L time-split               | 0.648               | 0.967 | 0.989 | 699 ms Core ML CPU, 581 MB |
      And so raea keeps SigLIP 2 plus HDC for "which moment is this" and ships no video model; V-JEPA only reads
        the arrow of time better, which no feature needs today
      And superseded 2026-10-15 by the roadmap's Track K: the HDC sandwich (frames, change and order as slots) reads
        the arrow of time better than V-JEPA after all (0.667 against 0.561) and speed too (0.812 against 0.699), at
        0.045 ms a window on an iPhone 13 Pro Max; the records that grew from it are Track L. Current results live
        there, not here

  Rule: Reproduce
    Scenario: Commands
      Then the library: `raea-meaning ingest <clip> -o ~/.cache/raea-meaning/cols/<name>` for each clip
      And find-or-make: `python3 rdb/examples/assets/g4.py` (needs rdb built)
      And cuts: `lane eval -- python3 meaning/bench/cuts_eval.py --renders N --jobs 2` (keep N so it ends in 15 minutes;
        for 40, first repeat `... --renders 40 --prepare --budget 540` until it prints complete: true)
      And TransNetV2: venv from meaning/bench/transnetv2/requirements.txt at ~/.venvs/transnetv2, then
        `convert.py <videos>` (weights in ~/.local/share/ugc-models/transnetv2, SOURCE.txt there)
      And temporal: siglip_frames.py -> vjepa_windows.py -> analyse.py in meaning/bench/temporal (a torch venv)
