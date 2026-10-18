Feature: Perception: footage read the way an editor reads it, before it's used
  Status 2026-10-14: moved into raea from the host app (it is core to editing and rendering, not to one app). The source is
  perception/Perception.swift; the host app compiles the same file through a symlink at its old path
  (~/host/app/Host/Agent/Perception.swift -> raea/perception/Perception.swift), the same way ~/host/framework
  points at raea/engine. One file, so the phone, the Mac app and the evals can't drift.

  Rule: What it is
    Scenario: The fact sheet and the tracks
      Given a photo, video or sound
      Then Perception writes facts.json: its shots and how each begins (cut, dissolve, fade), each shot's framing,
        faces, subject, layout (docs: ~/host/docs/previz.feature), scene labels and look, the words on screen over
        time, and what's heard (speech, music, silence)
      And on request it writes tracks next to the footage (engine/docs/ml-tracks.feature): person, hair, face and depth
        mattes, and object-<name> tracks from EdgeTAM
      And it also reads performance (the creator each second), product candidates and tracks, the camera's move, and
        the framing coach on a still

    Scenario: What it runs on (2,949 lines, 2026-10-14)
      Then it uses
        | dependency       | uses | for                                                                                   |
        | Apple Vision     | 40   | face rectangles, person and foreground masks, text, saliency, aesthetics, rectangles, image classes, feature prints |
        | Core ML          | 18   | MediaPipe face, hands and hair, Depth Anything 3 Small (+ camera head), EdgeTAM, sound and taste models |
        | AVFoundation     | 19   | decoding frames and audio                                                             |
        | Core Image       | 16   | crops, scaling, mattes                                                                |
        | Sound Analysis   | 2    | what's heard                                                                          |
      And the rest, about half the file, is plain logic: shot bounds, layout and split seams, coach, camera, summaries

  Rule: Who compiles it
    Scenario: Three builds, one file
      Then these compile perception/Perception.swift
        | build                     | how                                                                       |
        | the host app iPhone app        | app/project.yml compiles the Host folder, which holds the symlink          |
        | the host app Mac app           | mac/project.yml lists ../app/Host/Agent/Perception.swift (the symlink)     |
        | ugc-perceive (evals' host) | ~/host/agent/tools/perceive/build.sh: swiftc with main.swift, Mac only    |
      And ugc-perceive is the way to run it from a shell: `ugc-perceive <file> <dir>`, `track`, `layout`, `coach`,
        `camera`, `performance`, `product-candidates`, `product-track`, `check-hair|check-face|check-object`

    Scenario: Checked after the move, 2026-10-14
      When ugc-perceive was built through the symlink (lane quick, swiftc -O)
      Then it read media/fixtures/take-h264.mp4 in 31 s: 1 shot, 2 faces, close-up, a layout with two subject zones
      And `layout` read a still of it the same way
      And reference-check.sh passed: face points vs MediaPipe mean 0.46 px (0.28% of face width), followed 0.58 px;
        EdgeTAM objects vs the official predictor mean IoU 0.9922 (box) and 0.9928 (click)
      And the two apps were not rebuilt for the move (their project files are unchanged; the next app build compiles
        the file through the symlink)

  Rule: The Rust port so far (raea/perception, crate raea-perception, binary raea-perceive)
    Each part is ported as it is and checked against the Swift on the same inputs. ugc-perceive grew two commands for
    this: `samples <video>` (the shot finder's samples and bounds) and `layout-inputs <image> [next...]` (what Vision
    saw, the grey copies, and the layout); Perception.swift's layout was split into layoutInputs (Vision) and
    layout(from:) (the logic) for it, behaviour unchanged.

    Scenario: Shots (A1, 2026-10-14)
      Given perception/bench/shots_parity.py on 12 videos (the three fixtures and 9 of Buddy's renders)
      When the Rust shot_bounds runs on the Swift's own samples
      Then the bounds are identical on 12 of 12
      When the Rust samples the video itself (raea-media's VideoToolbox decode, each 192 x 192 pixel the mean of the
        source pixels under it, the first frame at or after each sample time)
      Then the grids differ from the Swift's by 0.012-0.023 per channel (VideoToolbox's scaling filter against the
        mean), and the bounds match on 10 of 12; the other two differ on borderline transitions (one dissolve read as
        a cut, one extra shot), which A4 settles against true cut times rather than against the Swift
      And the Swift's reader samples the stored, unrotated picture (a rotated clip's grid differs by 0.31); raea
        samples the picture as displayed, which doesn't change shots

    Scenario: Layout (A1, 2026-10-14)
      Given perception/bench/layout_parity.py on 60 distinct stills from Buddy's runs (faces, person-mask subjects,
        captions, screens, one grid split), plus built frames (stacked, side by side, a 25° diagonal, a letterbox)
      When the Rust layout reads the same inputs as the Swift's layout(from:)
      Then the layouts are equal on 60 of 60 stills and 4 of 4 built frames (ratio, split, zones as a set; the Swift
        groups caption blocks through a Dictionary, whose order isn't fixed)
      And the built stacked, side-by-side and diagonal frames read as no split in both: the T-401 frames that did
        read as splits weren't kept, so those branches are checked equal but not yet seen to fire

    Scenario: The framing coach (A1, 2026-10-14)
      Given perception/bench/coach_parity.py on 60 distinct stills from Buddy's runs (26 with a face, 24 with enough
        skin to read the light); Perception.swift's coach split into coachInputs (Vision's face and person mask) and
        coach(from:) (the logic, reading pixels through a closure), behaviour unchanged; `ugc-perceive coach-inputs`
        writes the frame's RGB bytes beside the inputs
      When the Rust coach reads the same inputs and pixels
      Then the coach is equal on 60 of 60 (face, eye line, light, warning, face level, backlight, key-fill, mixed)

    Scenario: The camera summary (A1, 2026-10-14)
      Given perception/bench/camera_parity.py on 8 videos (the fixtures and 5 of Buddy's renders): `ugc-perceive camera
        VIDEO --inputs` logs each shot's Depth Anything 3 camera windows (rotations, centres, fx, separation) and its
        first frame's side and room vector; Perception.swift's camera split into the model and Vision part and
        cameraSummary (the arithmetic), behaviour unchanged
      When the Rust summary reads the same windows
      Then the cameras are equal on 17 of 17 shots (moves seen: pan, tilt, still, handheld, push-in; zoom and dolly),
        the room vector compared at f32 precision (it is Float in the Swift)

    Scenario: The performance read (A1, 2026-10-14)
      Given perception/bench/performance_parity.py on 8 videos with a face (the fixture take and 7 of Buddy's
        renders): `ugc-perceive performance VIDEO --inputs` logs every frame's Look, the frame times, the loudness,
        the gestures, and the first 40 frames' face meshes (478 points, 52 scores) with their Looks
      When the Rust reduces the same meshes and summarises the same Looks
      Then 320 of 320 Looks match to 1e-9 and 8 of 8 performances are equal (141 seconds: eye contact, lens share,
        blinks, energy, speaking, reading, gestures, and the lip-sync offset and confidence)

  Rule: The models from Rust (A2): Core ML through objc2 on Apple
    perception/src/coreml.rs loads the apps' .mlpackage files (compiled once into ~/Library/Caches/raea-perception,
    recompiled when the package is newer), fills BGRA pixel buffers and reads multi-arrays through their strides
    (Float32, Float16, Double, Int32). `raea-perceive check-<model> <reference dir>` grades each against the same
    reference files and bars as ugc-perceive's checks.

    Scenario: Hair (A2a, 2026-10-14)
      When `raea-perceive check-hair ~/host/models/mediapipe-hair-segmenter/reference` runs
      Then IoU against MediaPipe is 0.9986, 0.9989 and 0.9977 on the three references (bar 0.997), the Swift's own
        numbers to the fourth place, and 0 levels off the converted model's reference mattes; 4-10 ms a frame on M4

    Scenario: Depth (A2b, 2026-10-14)
      Given Depth Anything 3 Small through the same Core ML path, the picture letterboxed into 504 x 504 the way the
        Swift's Core Image does it (one scale for both axes, origin bottom-left, the picture's fractional edges
        blended over black, bilinear at pixel centres); `ugc-perceive depth-values` writes the Swift's 1/depth
      When a picture needs no resizing (a 279 x 504 PNG)
      Then the Rust and the Swift 1/depth are identical (largest difference 0.0): the model path is exact
      When 24 eval stills (303 x 540, scaled by 0.933) go through both (perception/bench/depth_parity.py)
      Then 1/depth correlates at r 0.999 on average (worst 0.990), and the 8-bit track levels (each clamped to its
        own 1st-99th percentiles) differ by 2.5 levels on average (worst still 10.8): short of the bar set for it
        (99% of pixels within 2 levels)
      And the rest is Core Image's own downscaling sampler, which isn't plain bilinear (a plain bilinear resize gave a
        1% scale bias; the exact geometry took it to 0.06%); raea's ingest resamples frames itself, so matching Core
        Image's sampler is left open rather than chased: what the track needs is the same model on the same picture

    Scenario: The face (A2c, 2026-10-14)
      Given the three face models through Core ML (BlazeFace letterboxed to 128 with MediaPipe's weighted
        suppression; the landmark model on a 256 crop turned upright; the blendshape model on 146 of the points, a
        multi-array input); the Core Image geometry shared in perception/src/picture.rs
      When `raea-perceive check-face ~/host/models/mediapipe-face-landmarker/reference` runs
      Then the points are 0.34%, 0.38% and 0.33% of face width from MediaPipe's (bar 0.6%; the Swift: 0.45%, 0.29%,
        0.28%), blendshapes within 0.107 of MediaPipe's, 2-5 ms a face on M4
      And a face followed from its own points (the crop the next frame uses) is 0.56%, 0.56% and 0.35%, the Swift's
        numbers exactly: the crop warp and the two later models are equal; the first detection's crop differs from
        the Swift's by 1.5-3.7 degrees, which is the 128-pixel letterbox's resampling again

    Scenario: Hands, and the detectors' downscale (A2d, 2026-10-14)
      Given the palm detector (192 letterbox), the hand landmarks (224 crop turned up), the gesture embedder and
        classifier through Core ML; `ugc-perceive hands` prints the Swift's hands per still
      And picture.rs's letterbox averages a copy down by the whole factor before sampling when it shrinks past half,
        as Core Image's downscaling does (the face detector's crop then sits within 0.5 degrees of the Swift's
        rotation, from 1.6-3.5; points 0.53%, 0.33%, 0.25% of face width, still under the 0.6% bar)
      When perception/bench/hands_parity.py compares the two on the 66 distinct eval stills
      Then hands and gestures are equal on 8 of the 9 stills where the Swift finds a hand, the Rust finds a hand on 3
        stills where the Swift finds none, and the points are 3.1% of the hand's size from the Swift's on average
        (worst 6.0%): just over the 3% bar set for it; the differences are borderline palms near the 0.5 score

    Scenario: Objects tracked through a clip, EdgeTAM (A2e, 2026-10-14)
      Given EdgeTAM's four Core ML models chained from Rust (perception/src/object.rs: the encoder on the frame
        stretched to 1024, memory attention over the prompted frame and the 6 before, pointers of the prompted frame
        and the 15 before, the decoder, the memory encoder on the GPU), frames from raea-media's VideoToolbox decode
      When `raea-perceive check-object ~/host/models/edgetam/reference` tracks the bird clip from a box (frame 10)
        and from a click (frame 0)
      Then against the official predictor: box mean IoU 0.9947, min 0.9851; click mean 0.9958, min 0.9855; no empty
        frames (bar: mean 0.99, min 0.97); the Swift's own numbers are 0.9922/0.9821 and 0.9928/0.9808
      And about 137 ms a frame on M4

  Rule: Vision behind a Rust trait (A3)
    perception/src/vision.rs: `trait Vision { fn see(&self, picture) -> Seen }` (faces, text lines with the Swift's
    tilt and confidence filters, rectangles, the person mask, screenshot); AppleVision runs Vision's requests through
    objc2-vision with the Swift's settings; vision::layout reads a frame's layout end to end in Rust.

    Scenario: The layout end to end (A3a, 2026-10-14)
      When perception/bench/layout_e2e.py reads the 66 distinct eval stills with `raea-perceive see-layout` (all Rust)
        and `ugc-perceive layout` (all Swift)
      Then the splits agree on 66 of 66, the parts (people, captions, screens, panes, framed video) on 58 of 66, and
        the matching boxes' edges differ by 0.001 of the frame at the median (90th percentile 0.043) once Vision is
        handed an sRGB CGImage as the Swift does (from a bare pixel buffer: 0.003 and 0.106)
      And the Swift doesn't agree with itself exactly: two runs give equal layouts on 59 of 66 (the parts on 66);
        the Rust gives 66 of 66; the 8 part differences are caption lines and person-mask pieces, from the two
        decoders' JPEG pixels and the grey copy feeding Vision's fast OCR; in ingest both read frames from one decoder

    Scenario: The framing coach end to end (A3b, 2026-10-15)
      Given the Vision trait's people read (faces and the fast person mask, through a CGImage) and vision::coach (the
        tallest face, the mask, the Rust coach over the frame's pixels); `raea-perceive see-coach`
      When perception/bench/coach_e2e.py compares it with `ugc-perceive coach` on the 66 distinct eval stills
      Then both find a face on the same 28, the light agrees on 65 of 66 (one borderline soft/even front light), the
        warning on 66 of 66, and the eye line within 0.002 at the median (worst 0.009)

    Scenario: The camera end to end (A3c, 2026-10-15)
      Given `raea-perceive see-camera VIDEO`: the Rust shots, frames every 0.5 s (720 px at most) from raea-media,
        Depth Anything 3's camera package through Core ML on windows of four (letterboxed like depth; frame 0's
        separation from the person mask read on the letterboxed frame), the side from the biggest face and the room
        vector (the feature print with the person blacked out) through the Vision trait, the Rust summary
      When perception/bench/camera_e2e.py compares it with the Swift's cameras on the 17 shots of 8 videos
      Then the move, the side and the lens agree on 17 of 17, zoom or dolly on 16; the median differences are pan
        0.12 and tilt 0.10 degrees a second, shake 0.15, focal length 0.5 mm, separation 0.12; the room vectors are
        0.97 alike at the median (lowest 0.93)

  Rule: Visible cuts in Rust (A4)
    Scenario: The detector's signals without ffmpeg (A4, 2026-10-15)
      Given perception/src/cuts.rs: every frame from raea-media's NV12 decode, its planes area-resized to 480 wide (as
        `scale=480:-2`), ffmpeg's `select` scene score and `signalstats` YDIF computed in Rust, and engine/src/
        measure.rs's sensitive_shot_changes rule; `raea-perceive cuts VIDEO`
      And the scene score is luma's alone: computed over Y, U and V it read 0.72 of ffmpeg's at peaks, while U and V
        themselves matched signalstats' UDIF and VDIF; over Y alone it matches (0.0488 vs 0.0492)
      When perception/bench/cuts_rust.py compares both on 20 of Buddy's renders (58 visible cuts; cuts_eval's truth
        and split)
      Then the signals correlate at r 1.0 (scene and YDIF, medians), with the same frames on 20 of 20
      And the detector scores P 0.915 R 0.672 F1 0.775 on Rust's signals against 0.898 / 0.672 / 0.769 on ffmpeg's,
        held out 0.875 / 0.739 / 0.801 against 0.840 / 0.739 / 0.786
      And TransNetV2 joins later as a column through ort with its ONNX export (meaning.feature), not as the detector

  Rule: Clip ingestion in Rust (A5)
    Scenario: A clip's facts (A5a, 2026-10-14)
      Given `raea-perceive analyze VIDEO DIR` (perception/src/ingest.rs): raea-media's VideoToolbox decode, the Rust
        shot finder, each shot's middle frame read through the Vision trait (faces, bodies, saliency, scene labels,
        aesthetics, accurate OCR; extra reads every 2 s in long shots), the Rust layout with two later frames, facts.json
        and a still per shot in the Swift's format (sound and performance not yet)
      When perception/bench/facts_parity.py compares it with `ugc-perceive VIDEO DIR` on the 2 fixtures and 6 renders
      Then the shot count is the same on 7 of 8 videos (edit-1: the borderline transition of A1a's reading check)
      And on the 19 shots compared: framing, faces, motion, brightness (within 0.02) and the top scene label agree on
        19 of 19, the layout's parts on 14 of 19; text spans and the clip's scene labels are near-identical
      And the Rust takes 30.8 s for the 8 clips against the Swift's 51.9 s

    Scenario: What's heard (A5b, 2026-10-14)
      Given perception/src/sound.rs: YAMNet (Apache-2.0, AudioSet's 521 classes, the Core ML package the apps carry) on
        16 kHz mono from raea-media, each 0.96 s patch's top class mapped to speech, music, singing, laughter,
        applause, silence or noise, merged and filtered as the Swift does; the Swift asks Apple's SoundAnalysis
      When perception/bench/sound_parity.py compares the spans every 0.1 s on the fixture take and 9 renders
      Then they agree on 97.6% of the timeline; the rest is span edges (about half a second at the ends) and short
        noise or music blips Apple's classifier splits out of speech
      And facts.json from `raea-perceive analyze` now carries the sound

    Scenario: The performance read end to end (A5c, 2026-10-14)
      Given `raea-perceive performance-video VIDEO` (ingest.rs): raea-media's decode at up to 15 frames a second, frames
        over 1280 px scaled as Core Image does, the face followed from its last points (found again when lost), hands
        every third frame, the Looks, loudness every 10 ms from raea-media's audio, the summary
      When perception/bench/performance_e2e.py compares it with the Swift's performance on 8 videos (141 seconds)
      Then the lip-sync offset is within 10 ms of the Swift's on all 8 (the same on 5)
      And second by second: face share 100%, blink 99.3%, energy 98.6%, speaking 97.2%, reading 96.5%, gesture
        95.7%, eye contact 93.6% agree

    Scenario: One ingest, keyed by content (A5d, 2026-10-14)
      Given `raea-perceive ingest CLIP [--store DIR]` (ingest.rs): the clip's sha-256 is its key, and into
        <store>/<hash[..16]>/ it writes facts.json (shots, layout, words, scene, sound) with a still per shot,
        performance.json when a face is on camera (10 minutes at most), SigLIP 2 columns at 8 frames a second
        (through raea-meaning), and manifest.json (source, hash, version, each step's seconds); the store is
        $RAEA_INGEST, else ~/.cache/raea-ingest
      When the fixture take (25.6 s) and a 16 s render are ingested twice
      Then the first runs take 75 s (facts 17, performance 22, SigLIP 35) and 22 s (3.3, 3.0, 15.6)
      And the second runs take 0.16 s and 0.20 s: every step is already there; a new INGEST_VERSION runs them again

  Rule: Tiny models, one ingest step each (Track H, the user 2026-10-15)
    Scenario: Beat This! in Rust (H1, 2026-10-15)
      Given perception/src/beatthis.rs: the small checkpoint (small0, 2.1M parameters, MIT) exported to ONNX (10 MB,
        within 1e-5 of PyTorch at 1500 and 700 frames), its LogMelSpect front end written in Rust (22 050 Hz, 1024-point
        STFT every 441 samples, 128 Slaney bands 30 Hz-11 kHz, log1p(1000 x)), the package's chunking (1500 frames,
        6-frame borders, earlier chunks win) and its minimal postprocessing (the DBN option needs madmom's
        non-commercial models); `raea-perceive beats FILE [--store DIR]` keeps beats.json in the source's ingest
        folder; unit tests pass (3 of 3: peak picking, downbeats moved to beats, the mel bank's edges and frame count)
      When perception/bench/beatthis_parity.py runs it beside the beat_this package on the UGC bed and the UGC render
      Then on the package's own samples the spectrograms agree to 1.2e-4 and the beats and downbeats are identical
        (5 of 5 on both files)
      And from the files (raea-media's ffmpeg decode and resampler against torchaudio's and soxr) the spectrogram's
        mean difference is 0.01 and 13 of the bed's 14 beats and 9 of its 10 downbeats are within 20 ms; the render's 6
        beats all match
      And raea-media's Apple decode reads the render 2 frames (40 ms) shorter than ffmpeg does, which moves every beat
        off (0 of 6 within 20 ms) against the package's ffmpeg-decoded reference
      And the Apple decode was the right one (an agent session, 2026-10-15): our exports carried AudioToolbox AAC's 2112
        samples of encoder delay with no edit list; Apple trims the standard delay, ffmpeg plays it as sound, so an
        ffmpeg decode of an export (the reference's included) runs 44 ms late, and the two matched by sharing it; the
        remux now writes an edts/elst and both decoders read clicks at 1.000 and 2.000 s. Exports from before keep the
        offset in ffmpeg; beats decode with the default (Apple) backend
      And on click tracks it finds 39 of 40 clicks at 120 BPM (median error 10 ms) but places only 19 beats for 32
        clicks at 95 BPM (each within 20 ms): bare clicks aren't music, and rdb's model-free tracker is exact on both;
        on the bed (a weak pulse) it places 13 beats in 40 s where the model-free tracker places a steady 96 BPM
      And previz's cut_timing uses ingest beats when every music row has them ("beats_from": "beat_this"): the UGC edit
        has 9 beats under it and 2 of 9 cuts on one (0.33 by chance), against 4 of 9 (1.46) from the model-free tracker
      And the bench caught one mistake of its own: upmixing a mono file to stereo first scaled the package's input by
        1/sqrt(2); it now keeps each file's own channels

    Scenario: The sound steps of ingest (Track H, 2026-10-15)
      Given ingest::ingest_sound, run by `raea-perceive ingest` on every file with sound (and alone on a file with no
        picture: a voice, a music bed): vad.json (Silero's spans, the talking's level and the floor between), beats.json
        (Beat This! on the whole sound), stems.json (HTDemucs: each stem's level 20 times a second, and Beat This! on the
        music stem), denoise.json (+ denoised.f32, 16 kHz, when it runs)
      And DeepFilterNet3 runs only on speech whose floor between words is over -40 dB and flat like noise (spectral
        flatness over 0.3); two gates tried first failed: the whole sound's floor sent the bed and the render's music
        to the denoiser, and the vocals stem's floor missed white noise (HTDemucs puts it in "other") and let the
        render's music through
      When it ingests the UGC voice, the bed, the render, and the voice with white noise mixed in at two levels
      Then the gate stays shut on the clean voice (floor -51 dB, flatness 0.03), the bed (no speech), the render (floor
        -19 dB of music, flatness 0.01) and the milder noise (-42 dB, under the line), and opens on the stronger noise
        (-30 dB, flatness 0.54), lifting its SI-SDR against the clean voice from 6.4 to 13.2 dB
      And from the render alone the stems put the music a median 15.3 dB under the voice over its speech, against the
        16.6 dB previz measures from the edit's separate lanes (the master's limiter between them): music-under-voice
        can be read off footage whose music is baked in
      And Beat This! on the render's music stem finds 1 beat (the bed alone: 13): a weak-pulse bed 15 dB under a voice
        leaves it nothing to hold; the stems take 17-27 s for 25-40 s of sound, the 174 MB model's load included

    Scenario: HTDemucs stems, measured in Python before the port (H3, 2026-10-15)
      Given demucs 4.0.1's htdemucs (MIT code and weights; trained partly on MUSDB, which is research-licensed: noted)
        through its own apply_model (one shift, 0.25 overlap) on CPU, on a mix whose parts are known: the UGC take's
        voice plus the music bed under it (perception/bench/stems_bench.py; SI-SDR of the vocals stem against the voice
        and of drums+bass+other against the bed)
      When the bed is 16.6 dB under the voice (what previz measured in the UGC edit)
      Then the vocals go from 16.6 to 24.0 dB and the music from -16.9 to 6.9 dB
      When the bed is 6 dB under
      Then the vocals go from 6.0 to 19.8 dB and the music from -6.1 to 13.9 dB
      And it runs at 3.1 x real time on the CPU in PyTorch (9.3 x on the first, cold run)

    Scenario: HTDemucs in Rust (H3, 2026-10-15)
      Given perception/src/stems.rs: htdemucs between its STFT and its inverse exported to ONNX (174 MB fp32, 3e-4 from
        PyTorch on the spectral branch, 5e-6 on the waveform; ONNX can't hold complex tensors, so the STFT stays
        outside, the usual route), and around it in Rust: htdemucs._spec and _ispec (4096-point, hop 1024, periodic
        Hann, normalised, its 1536-sample reflect padding, real and imaginary parts as channels) and apply_model's
        chunking (7.8 s segments centre-padded from the mix, a quarter overlap, a triangle cross-fade, no random
        shift); `raea-perceive stems FILE -o DIR`
      When perception/bench/stems_parity.py runs it beside apply_model(shifts=0) on the same known mixes
      Then every stem is within 1e-5 of Python's (Rust against Python: 52-101 dB SI-SDR), and the vocals (23.93 and
        19.69 dB) and the music (6.79 and 13.75 dB) score exactly as Python's do
      And it runs at 0.35 x real time on the CPU through ort once loaded (1.13 x with the 174 MB model's load), about
        9 times PyTorch's 3.1 x: fast enough to split a clip's sound at ingest; the GPU (ort's Core ML provider or a
        Core ML conversion) is a later speed-up, not a need

    Scenario: DeepFilterNet3 in Rust (H2, 2026-10-15)
      Given perception/src/denoise.rs (feature `denoise`): the authors' own Rust library, libDF v0.5.6 (MIT/Apache-2.0),
        with its built-in DFN3 model run by tract, 48 kHz mono one 480-sample hop at a time, its STFT and lookahead
        delay taken out so the output lines up (best lag 0 against the input); `raea-perceive denoise FILE -o OUT.f32`;
        about 0.27 x real time on this Mac
      When perception/bench/denoise_bench.py runs it on the UGC take's clean voice and on it mixed with white noise or
        with the music bed at 0, 5 and 10 dB (SI-SDR against the clean voice; level in Silero's speech spans and between)
      Then on white noise it lifts SI-SDR from -0.4 to 11.8 dB, 4.6 to 14.9 and 9.6 to 16.4, the speech level within
        1.7 dB at 5 and 10 dB SNR and the floor between words gone (-23 to -120 dB at 0 dB SNR)
      And on the music bed as the noise it lifts -0.5 to 7.6 dB and 4.6 to 10.4, but at 10 dB SNR it does slightly
        worse than doing nothing (9.6 to 8.8): it isn't a music remover (that is H3's stems)
      And on the clean voice it changes the voice: 6.8 dB SI-SDR to the input, the speech 2.3 dB quieter, the room tone
        between words taken from -51 to -71 dB; so the bar (loudness within 0.5 LU) fails on clean sound, and the step
        runs only on sources whose floor between speech is high (the take's -51 dB is clean; the white-noise mixes are
        -23 to -33), a gate to set when the step joins ingest

    Scenario: Silero VAD in Rust (H4, 2026-10-15)
      Given perception/src/vad.rs: Silero VAD v5 (MIT, the 2.3 MB ONNX the silero-vad 5.1.2 package carries) run as
        the package runs it (16 kHz mono, 512-sample windows with the previous 64 samples in front, the recurrent state
        carried) and its get_speech_timestamps with the defaults; `raea-perceive vad FILE [--store DIR]` (vad.json in
        the source's ingest folder); unit test passes (1 of 1: a 64 ms dip doesn't split speech, a 0.1 s blip isn't
        speech, padding applied)
      When perception/bench/vad_parity.py runs it beside the package on the UGC take's voice (take-voice.m4a)
      Then all 10 span edges are identical to the package's, on the package's samples and from the file (5 spans,
        23.2 s of speech in 25.6 s, 0.23 s to run)
      And the spans cover 0.914 of the transcript segments' time; the rest is mostly the first segment's opening:
        the transcript starts "All right" at 0.24 s, but the voice is at -52 dB until 1.44 s (-21 dB while talking) and
        the model's probability stays under 0.25 there, so the segment's start is early, not the model late
      And so the UGC edit opens on 1.2 s of near-silence before its first word (s1 starts at the take's 0.24 s): a
        hook that waits, which previz's speech rows (from the transcript) hide; YAMNet's one speech span starts at 0.48
        s because its windows are 0.96 s

    Scenario: CLAP against YAMNet and descriptions (H5, tested 2026-10-15; not an ingest step yet)
      Given laion/clap-htsat-unfused (Apache-2.0 on its model card; Microsoft's CLAP weights carry no clear licence and
        were left aside) through transformers 4.46.3 in a scratch venv, on take-voice.m4a, music-bed.m4a and the UGC
        render (voice and bed mixed); windows labelled by YAMNet's spans (raea-perceive sound) and by CLAP's best of
        seven prompts, plus eight free descriptions ranked by summed similarity
      When the windows are 1 s
      Then CLAP agrees with YAMNet on 0.28 of the voice's windows, 0.05 of the bed's and 0.125 of the mix's, and
        ranks "a calm electronic music bed" first for the plain voice: its encoder is trained on 10 s clips
      When the windows are 5 s with the paper's prompt form ("This is a sound of ...")
      Then the voice is speech in 5 of 5 windows and "a man talking to the camera" ranks first; the mix is speech in 4
        of 4; the bed is music in 8 of 8, where YAMNet calls it noise (its music spans are 3 short ones), and "a calm
        electronic music bed" ranks first
      And so CLAP is usable for sound-to-description matching (Foley search, diegetic sounds) at 5 s or longer, and
        reads music YAMNet misses; a short event (a door, a click) needs a test with such sounds before it is trusted
        at its own length, and the step waits for an ONNX export and a Rust front end

  Rule: Reading rendered video as layers (Track J, the user 2026-10-15)
    Scenario: Text read off a render, graded on rows we wrote (J1, 2026-10-15)
      Given perception/src/textread.rs: every frame's lines from Apple Vision's accurate text recognition (no
        language correction, lines 1.5% of the frame high or more; vision::text_lines), chained into rows (the same
        words case and punctuation aside, an overlapping box, up to 3 missed frames bridged), and lines that come and
        go together, centred and stacked within a line's height, joined into one wrapped text; `raea-perceive
        read-text VIDEO`; unit tests pass (2 of 2: a dropped frame bridged and a moved copy kept apart; a two-line
        caption joined)
      When perception/bench/textread_grade.py reads the UGC render (rdb/examples/out/ugc-g8.mp4) and sets the rows read
        against ugc.sql's 16 caption rows and 2 title rows (the same words at 0.85 similarity, overlapping spans)
      Then all 18 are found; 16 spans are exact to the frame and the other 2 (the titles, which fade in over 8 frames)
        start 1 and 2 frames late; the boxes' tops are within 0.014 of the frame and their centres within 0.025 (the
        titles' padding); the words slip twice ("AI" read "Al", "≠" read "#")
      And it reads the footage's own text too: 703 rows no caption explains, nearly all the whiteboard behind the
        creator, flickering and read at 0.3-0.4 confidence where every caption reads at 0.58 or more; only 3 are steady
        and confident (a "DEV" sticker in the footage, a caption read once with an accent): telling those apart from
        the edit's overlays is J2
      And it takes 77 s for 738 frames (about 0.1 s a frame, every frame read)

    Scenario: Overlay or footage, for text (J2's first part, 2026-10-15)
      Given the truth from rendering each edit twice, with its titles and captions at opacity 0 the second time (rdb
        render; the UGC edit and the hook, 21 s for all four renders): a row read by J1 is an overlay when its box
        differs between the two on its middle frame (mean absolute difference over 8 levels), footage text otherwise
      And the rule: an overlay reads at confidence 0.5 or more, in 80% of its span or more, for 6 frames or more
        (perception/bench/layers_text.py)
      When it runs on both renders
      Then on the UGC render 18 of its 19 overlay rows are called overlays and none of its 132 footage rows (the
        whiteboard behind the creator) is; the one missed is a 3-frame misreading of a caption ("thèm"); accuracy 0.993
      And on the hook render all 4 overlay rows are found (its titles fade and slide in); it has no text in its
        footage, so it can't test false alarms, and the rule was set after looking at the UGC render: a third edit with
        text in moving footage is the test still owed
      And shapes, stickers and other overlays that aren't text are J2's next part (pixel masks from the same two-render
        truth)

    Scenario: Motion read back into keys (J3's first part, 2026-10-15)
      Given perception/src/motion.rs: each text row's box left and top over the frames it was read in (pixels), split
        into moves of 1% of the frame or more, each fitted with rdb's own eases (eval.rs ease_fn: linear, in, out,
        inout) over every nearby start and end frame by least squares; of the fits about as good as the best, the
        shortest; a move under way from an element's first sighting starts one frame before it (the tail of an eased
        move is itself nearly the same ease, so an unseen start can't come from the data: two fits tried first put it 7
        frames early, then 1 frame late); unit tests pass (4 of 4)
      When `raea-perceive read-text` reads the hook's render and its rows' keys are set against hook.sql's
      Then the "Stop scrolling" title reads frames 0-17, -82 px, ease out, against the key's 0-18, -80, out; "this
        changes your mornings" reads 14-30, -65 px, out, against 14-32, -60, out; the caption, moved by a rule that
        follows the face's linear key (60-120, -220 px), reads 62-127, -218 px, linear
      And the ends come 1-2 frames early: an ease-out's last stretch moves under a pixel a frame, and the box is
        measured in whole pixels; opacity (both titles fade in) isn't read yet, and rendering the rows read back
        through rdb against the original frames is the rest of J3's bar

    Scenario: The rows read, rendered back (J3's bar, 2026-10-15)
      Given perception/bench/renderback.py: the overlay rows J2 keeps become caption rows over the render without
        overlays (their words and span; size from the box height per line and top from the box, one constant each
        fitted on the UGC edit's single-line captions: 1.158 box heights per size unit, top 0.098 of the size above;
        centred when the box is, wrapping at the read width when read as several lines; colour the median of the
        brightest changed pixels; J3's keys as rdb keys), rendered by rdb, and set against the original frames inside the
        true overlays (pixels over 40 levels from the bare render), frames streamed one at a time
      When it runs on the UGC render (calibration) and the hook (held out)
      Then the read-back explains 0.276 and 0.521 of the overlays' pixel difference; as masks with 2 px of tolerance
        it scores F 0.687 (recall 0.594, precision 0.814) and F 0.759 (0.738, 0.782)
      And side by side the same words sit in the same places, in the same colours, moving the same way; what's missing
        is what isn't read yet: the captions' black stroke and shadow and the UGC title's pill (most of the UGC
        recall gap), the font weight (the hook's subtitle is 600 and comes back 800), and the titles' fades
      And its first version crashed the Mac: it decoded the three full-HD videos whole into arrays (a 738-frame render
        is 4.6 GB as raw RGB), so the machine thrashed and froze; memguard (~/.local/bin/memguard, a LaunchAgent) now
        stops any job past 6 GB or the largest one at the edge, and the bench streams frames (peak 0.5 GB)

    Scenario: Text styles read from the frame alone (J3, 2026-10-15)
      Given perception/bench/textstyle.py: at each overlay row's middle frame, its box's pixels in four colour
        clusters; light text with a stroke when the brightest cluster is ringed (2 px out) mostly by the darkest, which
        is dark, far from it and unlike what surrounds the box; else the glyphs are the cluster standing out most from
        the surround (a ring a quarter of a line wide outside the box), which is a pill when it is one colour that ends
        within 0.8 of a line (its colour and padding); no overlay-free render used (footage won't come with one)
      And three rules tried first failed and are written down there: the smaller of two clusters (busy footage behind
        the text wins), the edgiest of four (the stroke wins), the largest edgy one (the shadow wins)
      When perception/bench/textstyle_grade.py sets them against the edits' rows
      Then on the UGC render pill or not is right on 17 of 18 rows (the yellow pill found, padding 24 px against 22),
        stroke or not on 17 of 18, the text colour within 24 levels on 16 of 18; on the hook all three on 4 of 4; the
        misses are text over the colour-bar b-roll (a yellow bar taken for the letters) and a pill of 55% black, which
        isn't one colour
      And rendered back with them (strokes as stroke, pills as titles with their background and padding), the UGC
        render's overlay masks go from F 0.687 to 0.780 (recall 0.594 to 0.745); the hook, which has neither, stays at
        0.759; shadows, weight and fades are still not read

    Scenario: Motion as hypervectors, matched across edits (J5, 2026-10-15)
      Given perception/src/motion.rs motion_hv: each move read by J3 as role vectors bound to values (axis and way,
        ease, length and distance on log scales), the features bundled by majority, an element's moves bundled in
        order (each shifted by its place), "still" a vector of its own; 4096 bits; similarity is the share of agreeing
        bits; every text row read carries it (read-text's "motion"); unit test passes (two ease-out rises match at over
        0.7, a slide-in under 0.6, still against a move near 0.5)
      When the hook is rendered three ways (its titles rising easing out, rising at a steady speed, sliding in from the
        left easing in and out; the caption the same in all three) and perception/bench/motion_match.py reads them
      Then 12 of the 13 overlay rows' nearest other row has the same motion; the miss is a partial read of a title
        still cut off at the left edge as it slides in, which reads as still and matches the still caption
      And similarity averages 0.694 within a kind of motion and 0.619 across kinds: moves sharing an axis share bits, so
        the vectors rank well (nearest) but a fixed threshold between "same" and "different" would be thin

    Scenario: Non-text overlays as the pixels that hold still: a negative result (J2, 2026-10-15)
      Given perception/bench/layers_steady.py: per pixel at a quarter of the size, the spread of luma over 9 frames; a
        pixel within 6 levels counted as overlay; the truth the render against its overlay-free twin
      When it runs on the UGC render and the hook
      Then it finds 0.746 and 0.825 of the overlay pixels but only 0.023 and 0.035 of what it calls overlay is overlay
        (F 0.044 and 0.067): the wall behind the creator and the hook's flat backdrop hold just as still as a title
      And so holding still isn't what makes an overlay; the next try uses what the text reader already shows works
        (edges sharp and uniform against the picture, J3's styles) on regions that move unlike the footage around them

    Scenario: Overlay patches learned from two renders: ranks, doesn't yet transfer (J2b, 2026-10-15)
      Given raea-gbm layers (tabular/gbm/src/layers.rs, all Rust): every 24 px patch of every 2nd frame at half size
        (decoded by raea-media's Apple backend) described by what the frame alone shows (brightness, spread, colour,
        saturation, uniformity, edges, change since the last frame and 8 frames back, place), labelled overlay when over
        half its pixels differ from the overlay-free render; Perpetual trained on one render, scored on the other
      When it runs on the UGC render and the hook (overlays 0.7% and 2.2% of patches)
      Then trained on UGC it ranks the hook's overlay patches at AUC 0.946, trained on the hook it ranks UGC's at 0.581
      And at a threshold chosen on the training render (0.94, 0.99) it finds none on the other: F 0 both ways; two
        renders don't carry a calibration across edits, so the plan stands: thousands of generated render pairs over
        the corpus's footage (J2b); 1.75 GB peak, 87 s for both

    Scenario: Overlay patches learned from 60 generated render pairs (J2b, 2026-10-15)
      Given raea-gbm layers-gen (tabular/gbm/src/gen.rs): 60 two-second edits over raea's fixture footage, each with 1-4
        random overlays (boxes with borders, image stickers, titles on pills, stroked captions; still, sliding in, fading
        in or rotated), rendered by rdb as written and with every overlay at opacity 0; the patch features of J2b's first
        try; folds by edit (an edit's patches on one side), then a model on all 60 scored on the real renders
      Then held out by edit: AUC 0.977-0.995 per fold, F 0.617 (precision 0.758, recall 0.52) at the training threshold
      And on the UGC render it ranks the overlays at AUC 0.972 (0.581 when trained on the hook alone), but at the
        threshold carried over F is 0.163 (precision 0.279, recall 0.115); on the hook AUC 0.704, F 0.068
      And so variety is what made it rank: the generated edits have no 3D scene or flat backdrop (the hook) and no soft
        caption shadows (the UGC captions), the gap between generated and real; next: those in the generator, finer
        patches than 24 px, and calibrating the threshold per edit (the share of overlay a frame holds varies)

    Scenario: Generated renders closer to real edits (J2b, 2026-10-15)
      Given the generator changed where the two real renders differed from it: a third of the edits over a rendered 3D
        scene (the hook's kind: a flat backdrop, a floor, moving shapes, colours varied) and half the captions with a soft
        shadow (the UGC captions'); 90 pairs
      Then held out by edit: AUC 0.971-0.991, F 0.578 (precision 0.684, recall 0.5): harder, being more varied
      And on the hook: AUC 0.704 to 0.994, F 0.068 to 0.671 (precision 0.732, recall 0.619)
      And on the UGC render: AUC 0.974, F 0.163 to 0.359 (precision 0.478, recall 0.287); its captions over a moving
        handheld take are what's left; the generated footage is three fixture clips, so more real footage under the
        overlays is the next lever, then finer patches

    Scenario: More real footage under the overlays didn't help (J2b, 2026-10-15)
      Given the generator's footage widened from 3 fixture clips to 23 (the host app's 20 takes added), 120 pairs; patch
        features cached per pair (decoding 240 renders took most of lane's 15 minutes); training on every overlay patch
        and every 4th footage patch, the threshold chosen as if all footage patches were there
      Then held out by edit F 0.616 (AUC 0.977-0.994), about as before (0.578)
      And on the real renders it got worse, not better: the hook F 0.524 (0.671 with the 3 clips), UGC F 0.308 (0.359),
        AUCs 0.982 and 0.971; the takes bring busier backgrounds (whiteboards with writing, people moving) that make the
        generated overlays harder to tell from the picture without matching the real edits any better
      And so the 90-pair generator (the fixtures, 3D backdrops, caption shadows) stays the best so far; what the real
        renders' overlays have that the generated ones don't (their placement, sizes and timing as editors use them) is
        the next thing to copy, not more footage

    Scenario: Overlays placed and timed as editors do (J2b, 2026-10-15)
      Given the generator's captions as 2-4 chunks one after another (0.5-1.3 s each) centred in the lower third, stroked
        and mostly shadowed, its titles mostly in the top band; footage back to the 3 fixture clips and 3D scenes; 90
        pairs, training thinned with the weighted threshold
      Then held out by edit F 0.671 (precision 0.65, recall 0.694), the best yet
      And on the UGC render F 0.442 (precision 0.36, recall 0.573; 0.359 before), the best yet: captions over a
        moving take are what the placement taught it
      And on the hook F 0.286 (precision 0.198, recall 0.517; 0.671 before): it now takes parts of the moving 3D scene
        for overlays; the two real renders pull opposite ways, so the next step is a third and fourth real edit to
        judge by, rather than tuning the generator to two

    Scenario: Four real edits to judge by: the ranking transfers, the threshold doesn't (J2b, 2026-10-15)
      Given the overnight bases dr.rrl and demo.rrl rendered by `rrl render` as written and bare (title, caption and box
        rules at opacity 0, the bases' own title fade rules taken out of the bare copies, since they set opacity over
        the zero rule); checked on frames side by side: every overlay gone, a thin grey bar in dr and small marks in
        demo in both renders (so counted as footage); the model unchanged (90 generated pairs, editors' placement)
      Then AUC on all four real renders is 0.90-0.98: UGC 0.975, the hook 0.96, dr 0.982, demo 0.903
      But F at the threshold carried over from generated renders (0.73) is 0.21-0.64: dr 0.64 (precision 0.582, recall
        0.71), UGC 0.442, the hook 0.286, demo 0.206 (precision 0.285, recall 0.161; dark callout pills over colour bars)
      So the patch model orders overlay above footage on edits it never saw, and what fails to transfer is one fixed
        cut; the next step is a threshold set per edit without its truth (a share of each frame, or the score's gap),
        scored on these four, before any more generator tuning

    Scenario: A cut per edit doesn't lift it either: the patch features are the ceiling (J2b, 2026-10-15)
      Given two cuts set from each edit's own scores, no truth (Otsu's split of the score histogram; the cut flagging as
        many patches as the probabilities sum to), and the best cut chosen with the truth as the ceiling (raea-gbm
        layers-gen, layers.rs `otsu`, `expected_count`)
      Then F (carried-over 0.73 / Otsu / expected count / best possible):
        | edit | 0.73  | Otsu  | expected | best  |
        | dr   | 0.640 | 0.628 | 0.627    | 0.654 |
        | UGC  | 0.442 | 0.359 | 0.308    | 0.465 |
        | hook | 0.286 | 0.310 | 0.308    | 0.384 |
        | demo | 0.206 | 0.279 | 0.330    | 0.364 |
      So no cut, even one chosen with the truth, gets past F 0.36-0.65 on three of the four edits: overlays are 1-4% of
        patches, and at AUC 0.90-0.98 the footage patches scored above them still outnumber them; the patch features
        (still pixel statistics per patch) are the limit, not the threshold
      And J2b is blocked on the route the roadmap planned: a small segmenter trained on the generated pairs (which see
        shape and context, not one patch), and with no Python that means training it in Rust (burn or candle), a choice
        for the user

    Scenario: A small U-Net trained in Rust beats the patch model on real edits (J2b, 2026-10-15)
      Given perception/seg (raea-overlay-seg): each frame at 1/8 size, 136 x 240 cells of 5 channels (colour, the change
        since the frame before and since 8 before), a U-Net of 16, 32, 64 and 64 channels with skips (about 300k
        weights), trained with burn 0.22 on the Mac's GPU (Metal) on the 90 generated render pairs (81 to train, 9 held
        out by pair to pick the epoch and the cut), overlay cells weighted 20x (2.1% of cells), Adam at 1e-3 falling to
        1e-4 with gradients clipped at norm 1 (2e-3 unclipped collapsed to a constant at the 5th epoch); 50 s an epoch,
        2.4 GB at most; the best epoch's weights saved to $RAEA_MODELS/overlay-seg/ (1.2 MB) with its cut, and
        `--load` scores them again to the same numbers
      When it's scored as the patch model is (48 px patches, overlay when over half their pixels differ; the cells'
        mean as the score), at the cut chosen on the held-out generated pairs (0.72)
      Then held out by pair: AUC 0.993, F 0.706
      And on the real edits (F at that cut / F at the best cut, against the patch model's 0.73 cut / best):
        | edit | AUC   | F     | best  | patch model F | patch best |
        | demo | 0.991 | 0.743 | 0.751 | 0.206         | 0.364      |
        | dr   | 0.961 | 0.535 | 0.558 | 0.640         | 0.654      |
        | hook | 0.987 | 0.466 | 0.883 | 0.286         | 0.384      |
        | UGC  | 0.984 | 0.324 | 0.359 | 0.442         | 0.465      |
      And so the mean F is 0.52 against 0.39 (best cuts 0.64 against 0.47): it takes the demo's dark pills over colour
        bars and the hook's titles over a moving 3D scene, which the patch statistics couldn't; the patch model still
        does better on dr and UGC, and UGC's captions over a moving take are the weakest (0.32: it flags caption-like
        footage, precision 0.20); another training run gave 0.54 (GPU sums aren't bit-reproducible); the U-Net is the
        one kept (the consolidation's M10)

  Rule: Faces count once confirmed (the user via raea-9f, 2026-10-15: "faces are confirmed with confidence")
    Scenario: Raw confidence kept, faces confirmed over time at ingest
      Given raea-9f's finding: a face appearing in one frame made the hook's caption-dodge rule jump 68 px, and rules
        see one frame at a time; so ingest keeps Vision's confidence per face per sample (faces.json "conf", raw, as
        an agent session asked) and adds confirmed "tracks" (ingest::confirm_faces): faces followed across samples by overlap
        or by centre distance and size, tracks unseen for 2 s closed, pieces that follow on stitched, a face visible
        after two sightings in a row at 0.8 or more, held through gaps, gone after 0.5 s under 0.5, its box the median of
        its last three sightings; unit tests pass (3 of 3: entering, held through a dropout, let go; a one-sample blip
        never shows; two faces keep their own tracks)
      When the UGC take (two people, 103 samples at 4 a second) is ingested
      Then matched by overlap alone its 2 people came out as 11 tracks (heads move between samples); with distance,
        closing and stitching, 6: the creator (100 sightings) and the second person (92) and four one-off false faces
      And the faces switch on or off 21 times raw and 7 times confirmed: the creator's track is steady but for one
        2-sample dropout near the end, the second person's is one span of 87 samples, and of the four false faces only
        one shows (2 samples); Vision's confidence alone can't tell them apart (median 0.875), which is why two
        sightings are needed
      And rdb's previz reads the confirmed tracks (previz::face_track; raw only for an older ingest): on the UGC edit
        captions over the face go from 514 frames to 509
      And rules that read faces still get a transition in the edit (raea-9f's part: void's prompt; the hook example with
        an agent session's gate)

  Rule: the host app through the C interface (A6)
    Scenario: The interface (A6a, 2026-10-14)
      Given perception/src/ffi.rs and perception/include/raea_perception.h: raea_perception_ingest(clip, store),
        raea_perception_analyze(clip, dir), raea_perception_free; UTF-8 paths in, JSON out, {"error"} on failure
      When a Swift program links the crate as a static library (78 MB unstripped) and calls raea_perception_analyze on
        the fixture take
      Then it gets the same facts as the CLI (1 shot) in 15.5 s; `cargo test --lib` covers a missing clip and a null path
    Scenario: Linked into HostCore, off by default (A6b, 2026-10-14)
      Given ugc-core's new `perception` feature (~/host/agent/crates/ugc-core) re-exports raea_perception_* on Apple
      Then `cargo check -p ugc-core` is green with it off (the apps' build, unchanged) and on
      And raea-perception checks for aarch64-apple-ios with rustup's cargo (Homebrew's cargo has no iOS std)
      And next (A6c): the apps build with it on, and the phone's perceive-test screen (RenderTest.swift) runs Swift and
        Rust on the same clip
    Scenario: The app runs both, Swift and Rust, on the same clip (A6c, 2026-10-15, the Simulator)
      Given HostCore's iPhone and Simulator slices built with `perception` (2 minutes each in lane; the iPhone slice links
        ONNX Runtime statically, OrtGetApiBase in the archive), packaged privately with raea_perception.h in the module
        (build-xcframework.sh now does that when the feature is on), and the app built from a host worktree with
        -DUGC_PERCEPTION, so nobody's shared framework changed; the perceive-test screen (RenderTest.swift) runs
        Perception.analyze and then raea_perception_analyze on an 8 s cut of the fixture take
      When it runs in an iPhone 17 Pro Simulator (iOS 26.3; the platform and the Metal toolchain had to be installed
        into Xcode 26.2 first, as no iPhone build worked without them)
      Then both find 1 shot and agree on framing (close-up), faces (2), motion (moving), brightness (0.53) and the
        layout's parts; text spans 11 (Swift) against 9 (Rust); Swift's facts also carry audio and performance, which
        Rust's analyze leaves to ingest's own steps
      And Rust took 7.7 s and Swift 26.0 s, but the Swift is a Debug build (unoptimised pixel loops), so the times
        aren't a comparison
      But Rust reads Core ML packages from $UGC_MODELS (.mlpackage, compiled at first use), and the app bundles them
        compiled (.mlmodelc): the run pointed UGC_MODELS at a copy of ~/host/models in the app's Documents; before the
        switch ships, coreml.rs should also load the bundle's compiled models
      And on a phone: not run, as neither paired iPhone is connected (devicectl lists both unavailable)

  Rule: Toward Rust (the founder, 2026-10-14: run perception in Rust for raea)
    Scenario: The plan
      Then the plain logic ports to Rust as it is, beside raea's K kernels
      And the models run from Rust: Core ML through objc2 on Apple (keeps the Neural Engine) or ONNX through ort
        elsewhere (as ugc-embed does); decoding comes from raea-media
      And Apple Vision sits behind one Rust trait: an objc2-vision implementation on Apple, models elsewhere
        (MediaPipe face for faces, selfie-multiclass for people, YAMNet for sound, UNISAL for saliency); OCR and
        aesthetics have no good open replacement and stay Apple-only
      And the Swift file stays the reference until the Rust port is checked equal to it (facts.json on the eval
        footage, reference-check.sh); then the host app calls the Rust one through HostCore.xcframework and the Swift goes
