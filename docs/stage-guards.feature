@spec
Feature: Stage guards: a 3D render an agent can't break
  Shane, 2026-11-21, on the mobile-ads essay's 3D cut (projects/mobile-ads, compositions/film2): "every aspect it was
  totally broken looking and text, cards, overlays, etc all were so incredibly broken we must have uncovered something
  bad". Then: "diagnose the 3D issues with text, glyphs, layouts, staging, blocking and why things are flipping back
  and forth on scale/size ... how to make sure an agent using raea cannot physically make them again".

  What the diagnosis found (11-21). The repros are in projects/mobile-ads: compositions/diag/kept-props.tsx,
  kept-props2.tsx, kept-props3.tsx, compositions/glyph-atlas-probe.tsx, and the measures tools/cam-diag.ts,
  tools/text-diag.ts and tools/at-diag.ts. Each repro renders in a second or two.
    1. A 3D part's shape and a RichText's words, size and colour are made once, when the part's row is first spawned
       (engine/render/src/engine/spawn.rs, spawn_3d). A later frame's values reach the live row (apply_rows), but only
       placement, visibility, material and pictures are applied from it (runtime/scene3d sync.rs sync_meshes and
       sync_pictures; systems/text3d sync_texts).
       - kept-props.tsx: over 30 frames, a Sphere's radius, a Cuboid's half_size and a RichText's words hold their
         first frame's values. Growth by the transform's scale follows the frame, and so does a part remounted each
         frame.
       - kept-props2.tsx: a RichText's colour and size and a Text3D's letters hold too. A material's colour, an
         ImagePlane's picture and RichText's own enter reveal follow the frame.
       - In the film: every curve broke into dashes. stage.tsx's Ribbon is a chain of boxes whose newest box grows,
         and each box kept the length it was born with. A count read "0.0 MB" until its chunk ended, and typed code
         lines stopped wherever their first frame left them.
    2. A RichText mounted where another unmounted takes over its row. It draws the old words at its own place, and its
       own glyph colours land on the old glyphs. In kept-props3.tsx, "FROM ITS STORE PAGE" drew as "FOUR OPENINGS"
       with one letter red. In the film this gave "A RENDE? ENGINE", "TALL · ?N APPS", "1.? MB" and a store card headed
       "…ENINGS". Every cut glyph traced so far came from this, not from the atlas.
    3. There were two clocks. RichText's `at` is a frame of useCurrentFrame, so it counts in its composition's or
       Sequence's frames. The film's scenes took their own frame as a prop and passed scene frames. Rendered in
       150-frame chunks, a reveal at scene frame 1013 never began, which is why the store quote never appeared
       (at-diag.ts). Rendered whole, every reveal came at once. 21 texts had an `at`.
    4. The film was rendered in chunks, each in its own process, to get past 8 below. At each chunk's first frame,
       every frozen value snapped to that frame's value and then froze again. The network's red dot grew to full size
       in one frame at 1:10.06, the first frame of a chunk. Together with rows changing hands (2) and the camera (5),
       this is the "flipping back and forth on scale".
    5. The camera path (stage.tsx camAt) is a Catmull-Rom through keys set at uneven times, with every segment
       running 0 to 1 regardless of its length. Of 155 keys in 13 paths, 51 see the camera's speed change 2x or more
       (up to 9.4x). The framing reverses 24 times within 2 s: in and then out, or one way and then the other
       (cam-diag.ts).
    6. Type was set in the world. The placement wall's labels were 15–16 px tall at 1080 (text-diag.ts). Other labels
       lay on floors or on pages turned at an angle. The 2D type laid over the 3D didn't know where the parts were:
       words ran over the curves at 2:08, and a headline sat on the AppLovin card at 6:40. 45 entrances used an easing
       that overshoots full size.
    7. The glyph atlas (bevy_rich_text3d 0.8) has its own fault. The glyph whose allocation doubles the atlas is drawn
       into the old bounds and stays cut (glyph-atlas-probe.tsx).
    8. A 13,175-frame render in one process logged "Could not despawn entity" and grew from about 5 GB to 26 GB.

  None of this was caught:
    - raea check passed the film on loudness, freezes and flashes.
    - Its text checks read the 2D page only.
    - The contact check found one 1 cm touch.
    - `raea fingerprint things`, which finds camera lurches and empty frames, wasn't run.

  Each rule below makes one of these impossible to ship. Either the engine stops doing it, or the SDK refuses to write
  it, or the render fails with the part and the frame named.

  Background:
    Given the repros above, each a few seconds to render, and their measures
    And frame purity as control.feature and blitz-bevy H10 state it: a frame is the same however it is reached
    And the per-frame scene the contact check already reads (systems/contact: every part's kind, transform, shape and
      bounds; the camera)

  Rule: SG0. The bar first, before any fix

    @done
    Scenario: SG0. The repros become tests, and they fail today
      Given the plan
        | crate              | file                                    | lines | job                                                                                  |
        | (script)           | tools/frame-pure-3d.sh                  | 50    | each repro rendered as one sequence (PNGs, one process) and four frames alone (--frame N); frame N of both compared, SSIM 0.999 or more, lossless; fails naming the repro and frame |
        | raea-render (test) | engine/render/tests/diag/*.tsx          | 110   | kept-props, kept-props2 (its brand-logo picture row left out), kept-props3 moved in from the project, with what each shows |
        | (L1)               | tools/agents/check                      | +4    | the "3d frame purity" step, when core/scene, runtime/scene(3d), runtime/live, engine/render's engine, systems/text3d or sdk/render/src/3d changes |
      Then the test fails on main today on radius, half_size, Text3D letters, RichText words, colour and size, the row
        taken over, and the atlas's cut glyph, and passes on transform scale, material colour, pictures and the reveal
      And every scenario below that the engine fixes turns one of its failures green
      # Status 11-21: built and green on main after SG1 and SG2 (f4b524f896): every sampled frame of the three repros
      # matches at SSIM 0.9998 or more. The bar is tight enough: a sequence's frame 28 against frame 29 alone scores
      # 0.986, so a part held since its first frame can't pass. Before SG1 the same repros showed frame 0's sphere,
      # box, words, colour and size in a sequence. The atlas's cut glyph is one frame, not a sequence: its repro
      # (glyph-atlas-probe.tsx, a cold atlas against a warm one) came with SG3, and with it a second measure, since SSIM
      # over a whole frame scored the cut letter 0.9996: no more than 16 pixels off by over 96 levels, after an erosion
      # drops lone pixels (a revealed word's edge noise).

  Rule: The engine stops doing it

    @done
    Scenario: SG1. A part's shape and its type follow the frame
      The fix sits where rows are made, not in the engine: the engine keeps building a part once, at spawn, and a part
      whose kept columns change is spawned again. An mp4's 3D is made live frame by frame (raea-runtime-scene live.rs);
      a still's and RAEA_3D=program's rows are made from several frames at once (rows).
      Given the plan
        | crate                | file                                   | lines | job                                                                                  |
        | raea-core-scene      | core/scene/src/kept.rs                 | 200   | kept_col: the columns a row keeps from its spawn, per kind; respawns; spawn_sig, a part's kept inputs as one number; split_kept; KeptInputs, the fast path's guard. The live column tables moved here from live.rs |
        | raea-runtime-scene   | runtime/scene/src/live.rs              | +25   | diff: a row whose kept columns changed is dropped with the rows under it and spawned again; a glyph's colour sent as col_r/g/b, as a mesh's is |
        | raea-runtime-scene   | runtime/scene/src/fast.rs              | +3    | the fast path takes the full path when a part's kept inputs changed                  |
        | raea-runtime-scene   | runtime/scene/src/lib.rs               | +1    | a render's mesh, model, panel, decal and text tracks cut where their kept inputs change |
        | raea-runtime-live    | runtime/live/tests/stage_guards.rs     | 160   | the tests: a shape spawned again, new words laid out again, a glyph's colour, a still's own shape and words |
      Then kept-props.tsx and kept-props2.tsx pass SG0
      And a Ribbon of 48 boxes drawn over 90 frames is solid in a sequence render, as it is frame by frame
      And a growing shape's cost is measured (one mesh a frame, cached): a 48-part curve drawn over 90 frames renders
        within 10% of the same frames alone
      # Status 11-21 (branch stage-guards): kept.rs, live.rs, fast.rs and the track cut are built, and
      # runtime/live/tests/stage_guards.rs passes (5 tests; live_emissive and live_sheet still pass). Left: SG0's
      # renders of the repros, and a shader's `three` and `shader` columns, whose uniforms change every frame
      # (frozen uniforms keep their own fix rather than a part spawned each frame).
      # Status 11-21 (stage-guards-15): the `shader` column's uniforms follow the frame. A ShaderExtension whose uniform
      # was the frame over 89 (blue to red) drew frame 0's blue all video long on main, and SG9 failed it ("frame 89
      # card: 30136 pixels differ from the frame rendered alone"); now it goes blue, purple, red and SG9 passes. Its
      # uniforms go as `own:uniforms` rows, as a <Material>'s do (runtime/look own_uniforms, `ext` for a 3D mesh), and
      # raea-sys-shader own_live.rs packs them into the shaded material's slots when they change (ShaderExt slots_for).
      # `three` sends its uniforms each frame as `three:uniforms` rows (R3F11).
      # Status 11-22 (branch stage-guards-23): done. The Ribbon is engine/render/tests/diag/ribbon.tsx, a chain of 48
      # boxes whose newest grows, drawn over 90 frames, in frame-pure-3d's set: on 0.1.9 frames 0, 30, 60 and 89 of the
      # sequence match the frames alone at SSIM 1.000, no pixel off. Its cost against the same 48 boxes whole from frame
      # 0 (640x360, three runs each, the median): 2.99 s against 2.88 s; with the 1.4 s start-up taken out, 1.59 s
      # against 1.48 s, 7.5% more, inside 10%.

    @done
    Scenario: SG2. A part never inherits another part's row
      The cause was live.rs: a text block kept the id its frame gave it (`text0` is the frame's first block), where a
      mesh, model, panel or decal has its identity's (its name, else what it draws). So a block mounted as another
      unmounted got the old one's row: the engine kept the old words and took the new glyphs' colours.
      Given the plan
        | crate              | file                                  | lines | job                                                                                  |
        | raea-runtime-scene | runtime/scene/src/live.rs             | +3    | text blocks by identity, as the groups are; their glyphs follow their block's id     |
        | raea-runtime-scene | runtime/scene/src/live.rs             | +10   | plates, splats, sequences, fog, fires and grass by identity                          |
        | raea-runtime-scene | runtime/scene/src/lib.rs              | 0     | their tracks cut where what they take whole changes (split_kept)                     |
        | raea-core-scene    | core/scene/src/kept.rs                | +18   | WHOLE: each kind's columns that say what it is, and the props they come from         |
        | raea-runtime-live  | runtime/live/tests/stage_guards.rs    | +30   | a_new_plate_never_takes_an_old_ones_row                                              |
      Then kept-props3.tsx passes: the new RichText draws its own words in its own colour, and the old one is gone
      And a part that unmounts while another mounts beside it changes nothing else's row
      # Status 11-21 (branch stage-guards): text blocks are rows by identity; a_new_text_never_takes_an_old_ones_row
      # passes. Branch stage-guards-6: plates, splats, mesh sequences, fog volumes, fires and grass are rows by identity
      # too, and what each takes whole from its first frame (a plate's clip, a splat's or a sequence's file, a fog's
      # density texture, a fire's simulation, a field's grids) spawns it again when it changes (core/scene kept.rs
      # WHOLE): a_new_plate_never_takes_an_old_ones_row. An unnamed fire or fog has only its order to be known by, so
      # name the ones that come and go. Sprites, 2D shapes and bare entities keep their render ids, which components
      # target.
      # Status 11-22: done. kept-props3 passes SG0 on every sampled frame (SSIM 0.9998 or more), and
      # runtime/live/tests/stage_guards.rs holds "changes nothing else's row" for words and plates in L1.

    @done
    Scenario: SG3. The atlas allocates before it draws
      Given bevy_rich_text3d 0.8 reads the atlas's size before it allocates a glyph (tess_glyph)
      When raea patches it ([patch] in Cargo.toml, a vendored copy, with the upstream issue filed)
      Given the plan
        | crate              | file                                        | lines | job                                                                              |
        | bevy_rich_text3d   | vendor/bevy_rich_text3d/src/tess.rs         | +5    | the atlas image's size read after the glyph is allocated, in both branches (stroked, filled) |
        | (workspace)        | Cargo.toml, vendor/vendor.toml              | +8    | the [patch] and the vendor entry: why, and the exit (an upstream release that reads the size after allocating) |
        | raea-render (test) | engine/render/tests/diag/glyph-atlas-probe.tsx | 35 | "cold" (the film's glyph set into a fresh atlas) and "warm" (six Inter weights first, out of shot) |
        | (script)           | tools/frame-pure-3d.sh                      | +25   | a repro with cold and warm compositions: the two frames must match            |
      Then glyph-atlas-probe.tsx draws every glyph whole with no warm-up text
      And projects/mobile-ads components/stage.tsx Warm() comes out once the project renders on a raea with SG3 (0.1.9)
      # Status 11-21: on a build without the fix the cold probe cuts a Space Mono H (179 pixels off against warm); with
      # it, 0 pixels off (SSIM 0.99996). The upstream issue is drafted, not filed: filing it is a public post.

    @done
    Scenario: SG4. A part that won't despawn fails the render
      # Status 11-21: a live scene now drops the rows under another first (a block's glyphs, a part's components), in a
      # respawn and when rows are forgotten, so a block no longer takes its glyphs' entities before their rows go.
      Given the plan
        | crate              | file                                  | lines | job                                                                              |
        | raea-sys-frame     | systems/frame/src/store.rs            | +25   | despawn: try_despawn; a row whose entity had already gone leaves the store and is named (its kind and id) in the error; the store test |
        | raea-render        | engine/render/src/engine/spawn.rs, engine.rs | +8 | remove_rows_from and Engine::remove_rows return it                           |
        | raea-runtime-export | runtime/export/src/fed.rs, bevy_frame.rs | +4 | a render fails on it, with the frame                                             |
        | raea-runtime-react, raea-web | scene/plugin.rs, lib.rs       | +2    | a plain App and the web player pass it on                                        |
      Then "Could not despawn entity" is an error naming the part and the frame, never a log line
      # Status 11-21 (branch stage-guards-4): built ("frame N: Could not despawn entity: the text part 'quote' had
      # already gone, despawned with another part it hung from"); the store test covers it.

    @done
    Scenario: SG4a. A long render's memory stays flat
      Given the plan
        | crate               | file                          | lines | job                                                                   |
        | raea-runtime-export | runtime/export/src/program.rs | +5    | program_of: a frame's program with its 3D left out, for the sound alone |
        | raea-runtime-export | runtime/export/src/fed.rs     | +22   | a live render's sound step leaves the 3D out; stills_live, one look at the wanted frames with frames_hold_3d |
        | raea-runtime-export | runtime/export/src/render.rs  | +12   | a still past frame 3,600 made live (LIVE_PAST); stills keep their scenes for the end checks |
        | raea-sys-stage      | systems/stage/src/manifest.rs | +8    | the remounts counted as the frames go, not all at once                |
        | raea-core-scene     | core/scene/src/lib.rs         | +30   | spans_of: each part over the frames it's in; tracks_of built on it    |
        | raea-sys-contact    | systems/contact/src/lib.rs    | 0     | the clipping check over spans; each pair's stretches kept as they come |
        | raea-sys-contact    | systems/contact/src/model.rs  | +12   | forget: the files' triangles and the parts' solids let go             |
        | raea-runtime-export | runtime/export/src/scene.rs   | +18   | told_taking: each frame's scene let go as its parts are made; forget and the memory marks after each end step |
        | raea-core-shape     | core/shape/src/lib.rs         | +25   | cached: shapes held to 1 GB in memory, and only those that took 2 ms or more kept on disk |
        | raea-io-codec       | io/codec/src/samples.rs       | +88   | Samples: an encoder's samples on disk as they come, read back one at a time |
        | raea-io-codec       | io/codec/src/remux.rs         | +5    | annexb_mux_from: the mux reads each frame as it writes it             |
        | raea-io-apple       | io/apple/src/vt.rs            | +7    | VideoToolbox's samples kept in Samples                                |
        | raea-io-soft        | io/soft/src/h264.rs           | +6    | openh264's samples kept in Samples                                    |
        | raea-io-av1         | io/av1/src/lib.rs             | +4    | rav1e's packets kept in Samples                                       |
        | raea-sys-mix        | systems/mix/src/offline.rs    | +4    | the bounce fills each channel as the frames come, sized once from the frame count |
        | raea-sys-share      | systems/share/src/lib.rs      | +43   | the mesh share (moved from runtime/scene3d) lets go of what no part holds |
        | raea-sys-contact    | systems/contact/src/solid.rs  | +96   | Held: a shape's solid made at its part's first frame, let go after its last; occluders moved here |
        | raea-sys-contact    | systems/contact/src/lib.rs    | 0     | the clipping check holds each part's solid over its frames only (Held), 3 lines shorter |
        | raea-app-direction  | apps/direction/src/pure.rs    | +3    | frame purity's stills skip the clipping and staging checks it never reads |
        | raea-runtime-export | runtime/export/src/scene.rs   | +1    | a memory mark after what's on screen                                  |
        | raea-io-kept        | io/kept/src/lib.rs            | +245  | Kept: a live render's parts on disk as its frames come, each props map, string and material once by its content, a part's own place left out but a camera's; read back for the end checks |
        | raea-runtime-export | runtime/export/src/fed.rs     | +7    | a live render keeps its parts in Kept, not its scenes; a memory mark after its sound and video |
        | raea-runtime-export | runtime/export/src/scene.rs   | +1    | the end checks over the kept parts as read back (checked, for fed.rs); the parts' count in the mark |
        | raea-core-shape     | core/shape/src/lib.rs         | +34   | Held: shapes in two generations of 16 MB, what's asked for again kept |
        | muxide              | vendor/muxide/src/muxer/mp4.rs | +61  | Spill: the writer's samples in a temp file as they come, read back one at a time as the file is finished |
        | raea-io-codec       | io/codec/src/resample.rs      | +55   | soft_resample_blocks: the resampled sound a block at a time, the same bits |
        | raea-io-codec       | io/codec/src/audio.rs         | 0     | the true peak, its envelope and the loudness a block at a time, the same bits; the sound borrowed at 48 kHz (7 lines shorter) |
        | raea-io-codec       | io/codec/src/audio/tests.rs   | +117  | audio.rs's tests, and the block-at-a-time ones against the whole     |
        | raea-sys-mix-level  | systems/mix/level/src/joins.rs | +1   | filled: the sound itself when there's no gap, not a copy             |
        | raea-sys-mix        | systems/mix/src/offline.rs    | +10   | master_mix hands back its passes' freed copies (give_back); Encode   |
        | raea-io-trace       | io/trace/src/lib.rs           | +20   | give_back: the allocator's freed memory returned to the system now   |
        | raea-io-home        | io/home/src/lib.rs            | +15   | scratch_file: a temp file gone when raea is (the encoder's samples, Kept's parts) |
        | raea-io-codec       | io/codec/src/samples.rs       | 0     | Samples' temp file from scratch_file (8 lines shorter)                |
        | raea-core-scene     | core/scene/src/lib.rs         | +12   | ids_of: a frame's parts with the ids spans_of follows them by; spans_of built on it |
        | raea-sys-stage      | systems/stage/src/lib.rs      | +51   | Staging: the staging checks fed a frame at a time (push), their findings at the end; told and check_with feed it |
        | raea-sys-stage      | systems/stage/src/text.rs     | +9    | Texts: each block of words' frames kept as they come                  |
        | raea-sys-stage      | systems/stage/src/motion.rs   | +12   | Snaps: each named part's size and place on screen kept as they come   |
        | raea-sys-stage      | systems/stage/src/framing.rs  | +14   | Framing: each frame's share covered, whether it can be judged, and its subject, as they come |
        | raea-sys-stage      | systems/stage/src/page.rs     | +10   | Over: the page's words over the parts, as the frames come             |
        | raea-sys-stage      | systems/stage/src/manifest.rs | +28   | Mounts: what the manifest says of the parts, a frame at a time         |
        | raea-sys-contact    | systems/contact/src/clip.rs   | +175  | Clipping: each frame's pairs tested as it comes; an overlap's two parts and solids kept for its sentence |
        | raea-sys-contact    | systems/contact/src/solid.rs  | +27   | Held: a part's solid made as it first shows, let go once it's been gone 120 frames (made again if it comes back) |
        | raea-sys-contact    | systems/contact/src/lib.rs    | 0     | deepest, shared by the clipping check and the lint; the slice check and told gone (116 lines shorter) |
        | raea-runtime-endchecks | runtime/endchecks/src/lib.rs | +55 | EndChecks: the clipping check, the staging and the parts on screen fed each frame's parts as it's drawn |
        | raea-runtime-export | runtime/export/src/fed.rs     | +3    | a live render's frames go to EndChecks as they're drawn               |
        | raea-runtime-export | runtime/export/src/scene.rs   | 0     | a still's frames go to EndChecks one at a time, its parts made and let go (3 lines shorter) |
        | raea-sys-mix        | systems/mix/src/offline.rs    | +23   | Streamer: each frame's samples to a scratch file as they come, read back once after the last (13 KB a frame held before) |
        | raea-runtime-export | runtime/export/src/fed.rs     | +1    | a render with nothing to hear lets its bounce go unread               |
      Then a 13,000-frame 3D render's memory stays within 10% of its first minute's
      # Status 11-21 (branch stage-guards-8): the film (13,175 frames, 7:19) renders whole with every end check on, at
      # a 9.4 to 13.6 GB peak. Before, memguard killed it past 24 GB (28 to 33 GB) after its last frame. Four causes,
      # each measured: the sound step at a live render's end built the 3D's rows again from every frame's kept scene
      # (28 GB; it takes the sound's rows alone now); the clipping check held a slot for every part on every frame
      # (each part over the frames it's in now); the remounts' counts were all kept at once (counted as they go now);
      # and SG9's 64 stills, reached by rows, built a scene for every frame up to each (over 30 GB, killed). A still
      # past frame 3,600 makes its 3D live now, as a game's does, unless a compute in it carries state from frame to
      # frame: raea check --quick on the film went from 10.4 GB and 39 s to 0.8 GB and 9 s. Live stills at frames 300,
      # 6000 and 13000 match the film's frames exactly, and the two routes' stills at 3293, 6587 and 13174 match each
      # other in pixels and findings, since a still's end checks now run on its kept scene once its page is laid
      # out, on either route (the live route ran none before: a game's stills were never staged). Not met: memory
      # still grows with the frames (about 2 GB at the first minute, 5.7 GB at the last frame), because each frame's
      # scene is kept for the end checks. Flat needs the checks to run over a window as the frames come.
      # Branch stage-guards-13: RAEA_MEM_MARKS=1 prints the footprint every RAEA_MEM_MARKS_EVERY frames of a live render
      # and after each end step. On the film: 1.2 GB at frame 0, 3.5 GB at 2000, 5.0 GB at 13000; 3.9 GB with each
      # frame's parts made (each scene let go as its parts are: about 1 GB); 7.6 GB after the clipping check, whose
      # solids (one per model file or shape expression, kept for the process) were still held while frame purity
      # rendered its stills; 3.3 GB once the end checks let them and the parts go. The render's peak fell from 10.5-10.9
      # GB to 9.3 GB, its 435 errors, 699 warns and 47 notes the same (the clipping check's in order; it keeps each
      # pair's stretches as they come, not every frame's hit). Still not met: memory grows by about 40% after the first
      # minute (the kept scenes), and the solids of every part are made at once, 3.8 GB on the film.
      # Branch stage-guards-16: raea-core-shape's cache of each shape expression's triangles held every one for the
      # process, in memory and on disk, with nothing let go. A part whose size changes each frame is a new expression
      # each frame, so a long render grew by one mesh a part a frame, and the disk cache gained 2.46 million files
      # (9.7 GB, 2.2 million of them boxes) in 12 hours of renders on 11-21, filling the disk to 1 GB free. Memory is
      # now held to 1 GB of triangles (let go past it, made or read again as asked), and only a shape that took 2 ms
      # or more to make is kept on disk (the boolean cuts it was for; a box takes microseconds).
      # Branch stage-guards-18: the kept scenes were the smaller part of the growth. Found with stack logging
      # (malloc_history's live call tree at frames 2,500 and 7,500, diffed by the raea function that made each
      # allocation) and footprint's regions. Every encoder (VideoToolbox, openh264, rav1e) kept each frame's sample in
      # memory to the end, 1.04 GB on the film, and the mux copied them all again: raea-io-codec's Samples keeps them
      # in a temp file as they come, and the mux reads them back one at a time. The mesh share kept every shape it ever
      # made, and a part whose size changes each frame is a new shape each frame: 234,945 meshes a minute in (450 MB of
      # heap, and their GPU buffers), nearly all for shapes no part had any more; it lets go of what only it holds each
      # time it doubles (raea-sys-share, out of runtime/scene3d, which is at 2,000 lines). The bounce held the whole
      # sound twice at its end; it fills each channel as the frames come. The film with every check on: 2.4 GB at frame
      # 1,000 (3.4 before), 3.7 GB at 13,000 (5.75), the clipping check's 5.5 GB (6.5), a 8.6 GB peak (9.5); the same
      # findings and a video of 13,175 frames with its sound. Still not met: memory grows by about 45% after the first
      # minute. Left: the kept scenes and the checks' per-frame data (0.48 GB over the film, measured by turning them
      # off), pages the allocator holds while they're partly free (Malloc Small's dirty memory 440 to 825 MB while the
      # heap in use grew 150 MB: a mesh made and let go each frame for each part that changes size), the sound as it's
      # bounced (0.17 GB, the film's sound itself) and each frame's manifest entry kept as JSON values (6 KB a frame).
      # Branch stage-guards-19: the end's peak was two things, each measured on the film (resident memory every 2 s
      # beside the memory marks). The clipping check made every part's solid before its first frame and held them all to
      # its end: 1,323 shape expressions' solids, 2.9 GB of cells and 4 GB of footprint, against 26 MB for its 4 models'
      # (a part whose size or words change is a new expression). A shape's solid is made at its part's first frame now,
      # shared while parts hold it and let go after the last of them, unless an overlap of it is still to be said: the
      # footprint after the check fell from 6.4 GB to 3.2 GB, in the same 19 s. And frame purity's 44 stills, rendered by
      # a second raea while the first still held its memory, ran their own clipping and staging checks (1.6 GB), though
      # purity reads only their pictures and parts on screen; they run without them now, their pictures the same. The
      # film with every check on: 2.4 GB at frame 1,000, 3.7 GB at 13,000, a 6.0 GB peak (8.5 before), the same 439
      # errors, 705 warns and 47 notes. mimalloc in place of the system allocator was measured and dropped: no less
      # growth over the frames (3.74 GB at 13,000 against 3.89), and 7.8 GB kept after the end checks (2.3) for a 10.4 GB
      # peak. Still not met: memory grows by about 45% after the first minute, for the causes left above.
      # Branch stage-guards-20: measured on the film and on a control (one scene for 13,200 frames, nothing added after
      # its first: 48 parts that move, a bar whose size changes each frame, 3D words that count, the page's words), each
      # with a memory mark after the frames, sound and video. Four things grew with the length. Each frame's scene was
      # kept for the end checks: its parts go to a temp file as the frames come now (raea-io-kept), each props map,
      # string and material written once by its content, and a part's own place left out of its props (the checks read
      # it from its matrix, a camera's alone from its props), so a moving part's frames share one map: the control's
      # parts read back fell from 1.35 GB to 0.19 GB. The MP4 writer (muxide) held every sample to the end, after the
      # encoder had kept them on disk: 1.1 GB on the film, 0.55 GB on the control; they go to a temp file as they come.
      # The sound's master oversampled the whole sound 4x three times (676 MB each on the film), weighted all of it in
      # f64 for every loudness reading (about 0.8 GB a reading, up to a dozen), and copied it when it had no gap to fill:
      # the true peak, its envelope and the loudness go a block at a time now, the same bits (each tested against the
      # whole), and what the master freed is handed back. And the shape cache held 1 GB: two 16 MB generations now.
      # The film with every check on: 1.56 GB at frame 1,000, 1.91 GB at 2,000, 2.43 GB at 13,000 (3.7 before), 2.73 GB
      # after its sound and video (5.75 in the run before the mux and master fixes), 3.0 GB with its 2.04 million parts
      # read back, a 4.8 GB peak (6.0), the same 439 errors, 705 warns and 47 notes, at 34.3 ms a frame. The control:
      # 1.05 GB at frame 1,000, 1.27 GB at 13,000 (1.50, +44%, before), a 1.5 GB peak (2.85). Still not met: the
      # control's frames grow 19% after its first minute: each frame's manifest entry kept as JSON values (3 KB a frame)
      # and memory the system allocator keeps once the frames have freed it (Malloc Large's dirty pages grew 97 MB over
      # 8,000 frames while the live heap grew 23 MB; handing memory back every 500 frames left it as it was). The film's
      # frames grow 27%, most of it its later scenes' own (2.07 to 2.55 GB in the 1,000 frames from 5:33). The end
      # checks hold every frame's parts and each part's solid over its frames (the clipping check's 4.7 GB resident
      # peak); flat needs them to run as the frames come.
      # Branch stage-guards-21: the end checks run as the frames come. raea-sys-stage's Staging and raea-sys-contact's
      # Clipping take each frame's parts as it's drawn (raea-runtime-endchecks hands them in) and keep only what their
      # verdicts read: each frame's camera, each block of words' and each named part's place on screen, each frame's
      # cover and subject, the words over parts, and the solids of the parts on screen (a part's let go once it's been
      # gone 120 frames, an overlap's kept until it's said; the pair tests reused are the last frame's alone). Nothing
      # keeps a render's parts now: raea-io-kept is gone. The film: the same 439 errors, 705 warns and 47 notes, in the
      # same order; the end checks take 0.6 s (22.7 s: their work is done as the frames come); a 4.06 GB resident peak
      # (4.76). The control: its end checks add nothing, 1.32 GB after its last frame and after them (they rose to 1.52
      # GB before). Still not met: the frames grow. The control's 1.06 GB at frame 1,000 is 1.32 GB at 13,000 (+25%;
      # +20% over frame 2,000), as before; the film's frames now carry the clipping check's solids for the parts on
      # screen, 2.31 GB at frame 1,000 and 2.91 GB at 13,000 (+26%), 4.13 GB in its densest minute (frame 5,000).
      # Most of the control's growth was the bounce: its streamer reserved both channels for the whole render and
      # filled them a frame at a time (12.8 KB a frame, Malloc Large's resident pages climbing at a constant size). Its
      # samples now go to a scratch file as they come, read back once after the last frame, and not at all when
      # nothing sounded. The streamer alone, pushed the control's 13,200 silent frames: 26.8 MB at frame 1,800 and 167
      # MB at 13,199 before, 4.8 MB and 5.8 MB after. Met, on the release build of the whole stack (11-22): the
      # control's 1,064.5 MB at its first minute (frame 1,800) is 1,151.4 MB at frame 13,000, +8.2% (it was +20%), and
      # 1,148.2 MB after the end checks; marked every 200 frames, it rises 7.9 KB a frame, about 3 KB of it each frame's
      # manifest entry kept as JSON values, which is what to move next for more room under the bar.

  Rule: The SDK stops allowing it

    @done
    Scenario: SG5. One clock
      Given the plan
        | crate        | file                              | lines | job                                                                                  |
        | @raea/render | sdk/render/src/3d/text.tsx        | +12   | RichText refuses an `at` or `exitAt` outside its Sequence's frames                   |
        | @raea/render | sdk/render/src/3d/clock.ts        | 45    | the same check for every part that takes a frame of its Sequence (Animate3d's first key, BevyComponent's from) |
      Then a reveal set after the composition ends is refused at its first frame: "RichText 'quote': at 1013 is after
        this composition's last frame (149). Give `at` in this Sequence's frames (useCurrentFrame), or put the scene
        in a <Sequence from>."
      And the 3D guide says a scene is a <Sequence>, never a frame passed down as a prop
      # Status 11-21 (branch stage-guards): RichText refuses an `at` or `exitAt` at or past its Sequence's last frame,
      # naming the prop and the fix. Branch stage-guards-4: clock.ts holds the check, and RichText, a mesh's and a
      # material's `animate` (its first key: an animation the Sequence's end cuts short is fine, one that never starts
      # isn't), a lens effect's and MotionBlur's `animate`, and BevyComponent's `from` use it
      # (sdk/raea/conformance/one-clock.test.tsx). Left out, on purpose: Video's startFrom (a frame of the source
      # clip, which can run past the Sequence) and a VolumeSource's from/until (seconds). Screen's `from` isn't
      # checked yet: screenContent isn't a component, so it can't read the Sequence.

    @done
    Scenario: SG6. Lines are lines
      Given the plan
        | crate        | file                              | lines | job                                                                                  |
        | @raea/render | sdk/render/src/3d/line.tsx        | 160   | Line3d: points, width, drawn up to p, round joins, one mesh (a swept outline); Tube for a round section |
        | raea         | sdk/raea/src/mesh.ts              | +2    | raea/mesh exports them                                                               |
        | raea (test)  | sdk/raea/conformance/line3d.test.tsx | 80 | one part at every share drawn, the cut at p, the width and plane, the tube's radius and untwisted rings, caps, refusals |
        | (guide)      | sdk/guides/craft/bevy/meshes.md   | +22   | the curve recipe, and a chain of parts as a common mistake                            |
      Then a curve is one part, solid at every frame, and the 3D guide's curve recipe uses it
      And SG12 flags a chain of more than 8 parts named `NAME-i` whose ends touch as "a line made of parts: Line3d"
      # Status 11-21 (branch stage-guards-5): Line3d and Tube are built, one BufferMesh each, cut at p; renders of a
      # 120-point Tube and an 80-point Line3d growing over 40 frames read smooth with round ends at every frame. The
      # meshes guide's recipe uses them. The flag is raea-sys-stage's line-of-parts (a warning, motion.rs): a chain of
      # more than 8 parts named NAME-i that grows, which on the film names its 108 fields of 16 parts each.

    @done
    Scenario: SG7. A camera path that can't lurch
      Given the plan
        | crate        | file                                    | lines | job                                                                                  |
        | @raea/render | sdk/render/src/camera/path.ts           | 150   | cameraPath(keys): keys timed in frames, a cubic per channel paced by time with one velocity a key (so speed carries across it), `hold` keys where it stops, an ease only at the ends, no channel past a key (monotone); pos, look and fov at any frame; camera(f), Camera3d's props; check(), the keys whose timing still brakes hard |
        | raea         | sdk/raea/src/camera.ts                  | +2    | raea/camera exports it                                                               |
        | raea (test)  | sdk/raea/conformance/camera-path.test.ts | 140  | speed across keys (the per-segment path jumps over 2x on the same keys), keys passed through, ends, holds, no overshoot, Camera3d's props, check(), refusals |
        | (guide)      | sdk/guides/craft/bevy.md                | +7    | rule 10, the camera recipe, uses it                                                  |
      Then the camera's speed is continuous across every key it moves through, and no channel passes a key and comes back
      And on keys that don't set a quick move against a slow one, cam-diag.ts finds no speed change over 1.5x across a
        key and no framing reversal within 2 s the keys don't make themselves
      And path.check() names each key that does, with the fix (more frames for the quick move, or a hold)
      And the 3D guide's camera recipe uses it, and hand-written paths are left to the check (SG13)
      # Status 11-21: the speed is continuous across every key, so a path never jumps; a channel never passes a key and
      # comes back. On the film's 155 keys (cam-diag's measure) framing reversals went from 24 to 5 and speed changes
      # of 2x within 3 frames from 51 to 29. What's left is in the keys: the 5 reversals are turns the keys make
      # within 2 s; 11 of the 29 are stops the keys ask for (a pose given twice); the other 18 sit where a quick move
      # meets a slow one (10 m in 30 frames against 0.7 m in 134), which any path through those keys at those frames
      # must brake for, and check() names exactly those 18 with the fix. One scene's keys weren't in time order: the
      # old path drew them anyway, and cameraPath refuses them.

    @done
    Scenario: SG8. Labels that read
      Given the plan
        | crate        | file                              | lines | job                                                                                  |
        | @raea/render | sdk/render/src/3d/label.tsx       | 112   | Label: type drawn in the page at a part's projected point, from the engine's own camera; sized in pixels of the frame (at least 22 at 1080); kept clear of the parts' projected boxes |
        | raea         | sdk/raea/src/camera.ts            | +2    | raea/camera exports Label                                                            |
        | @raea/render | sdk/render/package.json           | +1    | the ./3d/label subpath                                                               |
        | raea (test)  | sdk/raea/conformance/label.test.tsx | 60  | the point through a camera and a cameraPath, ortho, title-safe, px of the frame, refused under 22 |
      Then type that points at a part is never at an angle, never tiny and never on top of the part
      And RichText stays for type that is itself part of the scene (a sign, a page in a book)
      # Status 11-21 (branch stage-guards-7): Label (raea/camera) projects its point through the props the shot's
      # Camera3d gets (a cameraPath's camera(frame), perspective or orthographic) and draws flat page type there: its
      # capitals 32 px at 1080 by default and refused under 22, its box inside title-safe and growing away from the
      # nearer edge, a line and a dot to the point, nothing while the point is behind the camera or out of frame
      # (sdk/raea/conformance/label.test.tsx). It stands `gap` px off the point you give it: give the part's top.
      # Status 11-22 (branch stage-guards-23): done. Never at an angle (flat page type), never tiny (refused under 22
      # px), never on the part (it stands off the point given, and page words over a part are SG11's type-over-part
      # error). Its point now reaches the stage checks (the manifest's `points`), which make the part there the shot's
      # subject (SG14).

  Rule: The render fails when a frame would look broken
    Each check below reads the frame's scene (as systems/contact does) and the page's layout, runs in `raea render`
    and `raea check`, and reports the part, the frame and the fix, as P1 unless it says otherwise.
    SG10, SG12, SG13 and SG14 are raea-sys-stage (systems/stage): they read each frame's parts through its camera
    (view.rs; lib.rs finds the shots' cuts), run at the end of every render beside the clipping check
    (runtime/export/src/scene.rs), and run without drawing in `raea scene stage`, which moved with lint, explain and
    light out of raea-runtime-export into raea-app-scene (apps/scene) so the export crate keeps room under 2,000 lines.

    @done
    Scenario: SG9. A frame looks the same however it is reached
      Given a render of more than 60 frames with 3D in it
      When raea render re-renders a sample alone: every 300th frame, each chunk's first, and the frame after any
        frame where more than 20 parts mount or unmount
      Then any part whose pixels differ beyond the encoder's noise is an error naming the part, the frame and the props
        that differ
      And this catches SG1–SG4 and any later fault of the same kind before a person sees it
      # Status 11-21 (branch stage-guards-8): built for a video with 3D in it, more than 60 frames. At the render's end
      # one more raea renders the sample with `--at` (each frame reached by a jump) and each still is held to the
      # video's frame: a channel off by more than 96 levels through a 3x3 erosion (tools/frame-pure-3d.sh's bar, which
      # drops the encoder's and a glyph edge's lone pixels), on more than 1e-4 of the frame. The largest channel, not
      # luma: the positive control (a box moved by a counter outside the frame) put an orange box where the alone frame
      # has grey floor, 68 levels apart in luma and 116 in blue. A frame that differs is rendered
      # alone (`--frame N`, its own process): still different, a `frame-pure` error naming the frame, the patch's box,
      # the parts under it (the still's manifest now lists each frame's parts on screen, raea-sys-stage's on_screen)
      # and the two pictures, kept in OUT.frame-pure/; matching alone, a `frame-pure-jump` warning (raea's fault, not
      # the composition's). The sample is every 300th frame, the last (most history behind it, so a render under 300
      # frames is sampled too) and the frame after each remount, from the manifest's new `remounts`; at most 64.
      # Left: "each chunk's first" (raea render has no chunks of its own); "the props that differ" (React is
      # frame-pure, so both routes hand the engine the same props: what differs is the engine's state, and the
      # error names the parts instead). RAEA_FRAME_PURE=0 skips it. Measured before building, alone (`--frame N`)
      # against the sequence on four real scenes of 91-180 frames: three match exactly (SSIM 1.0); one under Solari
      # GI differs in SSIM (0.97-0.99, the GI's history) with no patch past the bar after frame 0. On the whole film
      # (13,175 frames), its 64 stills past frame 3,600 made live (SG4a): none differs. The positive control is still
      # caught and six real scenes are clean on that route.
      # Stay rendered whole (11-22, the release build of the 0.1.9 stack): 12 frame-pure errors from frame 230 on, each
      # a shot after the black open drawn far darker than the same frame rendered alone (the pool, the canyon, the
      # montage, the paint, the sea), the fault Stay's own code works round by rendering a shot a process
      # (tools/film.sh). Its 35 chunks on that route: 0 errors.
      # Status 11-22 (branch stage-guards-22): why. A part a scene leaves is hidden, not despawned (its row stays, from
      # the first frame the render holds to its last), and Solari traced hidden parts unless <SolariLighting dynamic>, a
      # trial, was on (raea-render-raytrace solari.rs `hidden` ran only under it). So every place's parts, the ones a
      # film had left and the ones still to come, stayed in the traced scene unseen and shaded whatever was on screen.
      # A render's rows are every part any of its frames has; a frame rendered alone holds only its own, so it had
      # none of them. kept-scenes.tsx (a black open, a yard, a room): on 0.1.9 the yard at frame 30 0.923 SSIM to the
      # frame alone (the room's parts, hidden, shading it), the room at 60 and 89 0.933 (2,478 pixels past the bar);
      # kept-light.tsx solari-gone (a box unmounted at 15) left its shade on the floor 44 frames on (0.987). Every single
      # change of a light or the look follows the frame (SSIM 1.0, and 0.9975 under Solari, its noise). Fix: `hidden`
      # runs on every traced render, on the parts whose visibility changed or that were just traced; kept-scenes
      # 0.996-0.997, solari-gone 0.997-0.998 (measured on 0.1.9 with RAEA_SOLARI_DYNAMIC=1, which ran the same system).
      # And a reset (a cut) now empties Solari's world cache whole, as a render's first frame finds it: it reset only
      # the cells it picked to update that frame, the rest keeping the last place's light at that place's exposure
      # (an exposure cut 0.992 to its still 25 frames on, 0.996 with it). tools/frame-pure-3d.sh holds both repros,
      # each composition a repro names at the bar it names (Solari's 0.99), and L1 runs it on Solari's and the traced
      # scene's code too. Landed 7e2df514de: in L1, kept-scenes 0.996-0.997 and solari-gone 0.997-0.998 to the frames
      # alone, none past the pixel bar.
      Given the plan
        | crate               | file                         | lines | job                                                                    |
        | raea-app-direction  | apps/direction/src/pure.rs   | 234   | the sample, the stills and the alone frame, the patch, the parts under it, the findings |
        | raea-app-direction  | apps/direction/src/rendered.rs | +2  | a video render's end runs it beside the text rules                     |
        | raea-sys-stage      | systems/stage/src/manifest.rs | +40  | the manifest's `parts` (stills) and `remounts` (longer renders)        |
        | raea-runtime-export | runtime/export/src/lib.rs    | +4    | write_manifest takes them                                              |
        | raea-sys-stage      | systems/stage/src/framing.rs | +12   | on_screen: each part's box on screen, named                            |

    @done
    Scenario: SG10. 3D type that reads
      Given the plan
        | crate          | file                         | lines | job                                                                             |
        | raea-sys-stage | systems/stage/src/view.rs    | 118   | the frame's camera (Camera3d's quaternion or a lookAt), a point and a box on screen |
        | raea-sys-stage | systems/stage/src/lib.rs     | 183   | told and check, the frames as the checks read them, shots, runs, findings     |
        | raea-sys-stage | systems/stage/src/text.rs    | 217   | each RichText's capitals on screen, its turn from the camera, its box against title-safe, its reveal, its reading time; `read={false}` for scenery |
        | @raea/render   | sdk/render/src/3d/text.tsx   | +3    | RichText's `read` prop                                                          |
        | raea-sys-stage | systems/stage/src/cover.rs   | 113   | parts in front of the words: a 3x3 grid's lines to the eye against each part's own box |
        | @raea/render   | sdk/render/src/3d/shapes.tsx | +14   | Text3D's `read`, carried on its mesh                                            |
        | raea-app-scene | apps/scene/src/stage.rs      | 38    | `raea scene stage`: every frame stepped without drawing, the checks, errors fail it |
        | raea-app-scene | apps/scene/src/lib.rs        | 59    | `raea scene`, moved from raea-runtime-export (lint and explain) with light.rs  |
        | raea-app-scene | apps/scene/src/light.rs      | 58    | `raea scene light`, moved                                                       |
        | raea-runtime-export | runtime/export/src/scene.rs | 35 | a render's items, and its end checks: clipping, then staging                    |
      Then every RichText and Text3D is checked on every frame it shows:
        | measure                              | limit                                              |
        | cap height on screen                 | at least 22 px at 1080 (scaled to the frame)       |
        | its face against the line to camera  | within 35°                                         |
        | its box                              | inside the title-safe area                         |
        | parts in front of it                 | none covering it                                   |
        | time held still on screen            | its reading time (words ÷ 3 a second, plus 0.5 s) before it moves |
        | its reveal                           | finished before it unmounts                        |
      And on the film these flag the placement wall's labels (16 px), the floor labels (over 35°) and the store quote
        (a reveal that never starts)
      # Status 11-21 (branch stage-guards-6): RichText is checked on every frame (systems/stage/src/text.rs): capitals
      # under 22 px at 1080 are an error; turned past 35° a warning, past 50° an error (squashed under 64%, a floor seen
      # low); outside title-safe a warning; a reveal never started or never finished an error; held still for less than
      # its reading time a warning. `read={false}` leaves scenery out. A still stepped alone (raea check --quick, render
      # --at) is judged for size, angle and safe area, and says nothing of reveals or time. Measured with `raea scene
      # stage` on film2 (13,174 frames, 64 s): 182 small-type errors (code on screens at 3 to 7 px, labels at 17 to 19
      # px), 152 angle errors (195 labels at 60 to 75°: 'ASA RULING · UK' at 74° for 12 s, 'KIT' at 71°), no false
      # error on 33 clean scenes (gambit, sheet3d, lightbench, Stay, floor3, blender-loop). The film was rewritten since
      # the notes: its placement wall no longer exists, so the 16 px labels can't be named. Left: Text3D, and parts in
      # front of the words.
      # Status 11-22 (branch stage-guards-23): done. A Text3D is held to the same checks: its words and em come from its
      # mesh's shape (text3d("WORDS", size, ...)), one line centred on its origin; `read={false}` on the Text3D (or its
      # mesh) makes it scenery, and a Text3D cut into a solid is the solid's. Parts in front (cover.rs): the words' face
      # sampled on a 3x3 grid through the middle of the letters, each point's line to the eye tested against every
      # other part's own box in that part's space; more than half stopped more than an em short of the words, for half
      # a second, is a type-hidden error naming the part. Words never hide words (a block is mostly air), nor do
      # see-through parts (opacity under 0.5), a part round the camera, or a model (no box here). raea scene stage over
      # the 48 clean scenes: no finding added or lost. Over 14 scenes with 3D type: the heros game's spoken captions
      # (Text3D at 26 px capitals on a 1920-line frame, 39 needed, carried by a turning camera) give one type-small
      # error and two type-time warnings; nothing else changes. On the essay film: 26 type-hidden errors, every other
      # count the same. Looked at: its APK's file names behind the phone (3:17-3:23, none visible), the apk card's
      # words turned away behind the cards (3:28-3:31), and MON, WED and THU under the BRIEF, ASSETS and LIVE tokens
      # (1:21-1:48: the row reads TUE, FRI, SAT, SUN). First measured with a 3x2 grid and half the points: 35, the
      # other 9 false (words over words: "ASSETS behind VARIATION" read clearly) or partial (day labels whose lower
      # third the next tile cuts, still read).

    @done
    Scenario: SG11. 2D type stays off the parts
      Given the plan
        | crate               | file                          | lines | job                                                                   |
        | raea-sys-stage      | systems/stage/src/page.rs     | 157   | each run of page words over a part's box on screen past 10%, the parts under them named |
        | raea-sys-stage      | systems/stage/src/manifest.rs | 97    | each frame's page (words, videos, pictures) read from the manifest's frames; the parts and remounts written into it (moved from raea-runtime-export, at its 2,000 lines) |
        | raea-sys-stage      | systems/stage/src/lib.rs      | +14   | check_with and told take each frame's page                            |
        | raea-sys-stage      | systems/stage/src/framing.rs  | +16   | the backdrop rule shared; the page's words and videos fill the frame (SG14) |
        | raea-runtime-export | runtime/export/src/run.rs     | +5    | a still's end checks wait for its page                                |
        | raea-page           | engine/page/src/motion/manifest.rs | +45 | the manifest's pictures (img, svg, background image or colour) and words marked allowOverlap |
        | raea-runtime-page   | runtime/page/src/bevy_page.rs | +2    | a frame's manifest entry carries them                                 |
      Then a page text box that covers more than 10% of a 3D part's projected box is an error, unless one of them says
        allowOverlap
      And on the film this flags the words over the curves at 2:08 and the headline on the AppLovin card at 6:40
      # Status 11-21 (branch stage-guards-8): page.rs. Words in the page (the manifest's text: its ink's box, opacity
      # 0.1 or more) over more than 10% of a 3D part's box on screen (inside the frame), for a quarter second or more
      # or on a still alone, are a `type-over-part` error: one for each run of the words, naming the parts under them
      # with their shares. A backdrop (framing's slab rule) and a part with allowOverlap are left out. On the film,
      # 31 errors: the caption over the curves from 2:08.02 to 2:16.04 (16 parts: the week's label at 53%, the curves
      # at 28%), the words over the brand cards from 6:33 to 6:37 (the first card at 61%) and 'social winners often
      # don't' over the hero and the network's logo from 6:38 to 6:41. Some are words over a wall of small parts (54 ad tiles
      # under '185 ads · 11 games' at 5:01, a chart's points at 1:03), which the rule as written counts: a wall of
      # tiles isn't a backdrop by the slab rule. Branch stage-guards-9: words in an element that says
      # data-allow-overlap may sit on any part (the manifest marks them allowOverlap): a headline across a card is an
      # error, the same words under data-allow-overlap pass. Stay (approved), rendered whole with its page known: two
      # errors, both meant, 'turn' drawn over the plant at 0:49 and the end title 'JUST THAT.' across the screen at
      # 1:20; each takes data-allow-overlap (Stay already fails on its dolly reversal at 1:12, SG13). Branch
      # stage-guards-12: the 3D guide's rule 9 and the skill's 3D router name type-over-part and data-allow-overlap
      # (raea-skills #112). Branch stage-guards-13: the findings are said in the order they start (a HashMap's order
      # changed from run to run). Left: `raea scene stage` doesn't know the page, since it draws none.
      # Status 11-22 (branch stage-guards-23): done. `raea scene stage` lays out no page, by design: it is the look
      # before a render and draws nothing. It says the page's checks run in raea render, and a frame it reads empty is
      # a warning there, since the page's words or pictures may fill it; a render knows the page and fails it.

    @done
    Scenario: SG12. Nothing snaps
      Given the plan
        | crate          | file                         | lines | job                                                                             |
        | raea-sys-stage | systems/stage/src/motion.rs  | 311   | cuts; a named part's size or place on screen jumping in one frame, alone; a growing chain of parts named NAME-i (SG6's flag) |
      Then a part whose projected size changes by more than 25% from one frame to the next, with no cut, is an error
      And so is a RichText whose words change without a remount, until SG1 lands
      And on the film this flags the red dot at 1:10.06 and every chunk's first frame in a curve's drawing
      # Status 11-21 (branch stage-guards-6): part-snap (motion.rs) is a named part's size on screen changing over 25%
      # in one frame between steady frames, no cut: an error. Measured first as written, it said 1,446 errors on the
      # film and 1,252 on four clean scenes; what was left out, each a measured false error: a part coming in (no
      # steady frame before it), a name several items carry in one frame, four or more parts at once (the layout
      # changing: the creatures reel's 30 tiles), parts reaching over 6 m (water, floors), and a part whose words or
      # shape change that frame (spawned again as new content, SG1: kept-props2's counter). A jump across a tenth of
      # the frame is a warning (a tile recycled, a part thrown). Now 0 errors on film2 and on 33 clean scenes. Left:
      # the red dot at 1:10.06 and the chunks' first frames were drawn wrong by the engine from right props, which only
      # SG9's re-render sees; and RichText words changing without a remount (SG1).
      # Status 11-22: done. Wrong pictures from right props are what SG9's re-render sees, on every render of a film
      # with 3D (its whole-film fault fixed on stage-guards-22), and RichText words that change follow the frame (SG1,
      # kept-props in SG0).

    @done
    Scenario: SG13. A camera that settles
      Given the plan
        | crate          | file                         | lines | job                                                                             |
        | raea-sys-stage | systems/stage/src/motion.rs  | 311   | inside each shot, a speed kink in the camera's travel or turn (2x an error, 1.5x a warning), and a dolly or pan undone within 1.5 s |
      Then the camera's speed changing by more than 2x within 3 frames, or its framing reversing (in then out, one way
        then the other) within 1.5 s, is an error, and 1.5x is a warning
      And a camera still moving while type is up past the type's reading time is a warning
      And `raea fingerprint things`'s lurch check folds into this one, so it runs on every render
      # Status 11-21 (branch stage-guards-6): camera-lurch and camera-reverse (motion.rs) run inside each shot on every
      # render. Speeds under 0.1 degrees or 3 mm a frame are below a change anyone sees; turns are measured by atan2
      # (acos of an f32 dot read 0 and 0.02 degrees on alternate frames and said 48 false lurches on Stay). A reversal
      # is the whole there-and-back within 1.5 s; a dolly in and back out is an error, a pan there and back a warning
      # (a first-person look round). Words up while the camera moves past their reading time are type-time's warning.
      # On film2: 41 lurch errors, each at a key where the stage's Catmull-Rom segments differ in length (its speed
      # jumps there); on the clean scenes, 13 lurches on cameras locked to a walking body (sheet3d) or a first-person
      # walk starting with no ease, and two dolly reversals (soma_dressed's 0.15 m, Stay's 0.84 m in and 0.43 m out at
      # 1:12). Left: `raea fingerprint things`' lurch check, still its own.
      # Branch stage-guards-16: folded in. That check (camera-speed, P2) read the picture's motion and compared each
      # half second's mean speed with the one before, inside a move away from its eases; a speed that ramps up over a
      # few frames passes the one-frame kink above and was caught only there, by hand. motion.rs `ramps` runs the same
      # on the camera itself on every render, as a camera-lurch warning at 2x (none near a kink already said). On the
      # 29 clean scenes: 3 warnings, all real (creatures' camera locked to the raccoon as it breaks from a walk to a
      # run, 2.4x); the 13 errors are the same. On film2: 6 warnings beside its 41 errors. `raea fingerprint things`
      # keeps its own for video with no 3D scene (footage).
      # Stay's own reversal (11-22): its montage's glasshouse pair (72.26-73.50 s) slid past the plant while aimed at
      # it; the move now ends where it comes nearest, and its words over the picture ('turn', the end title) say
      # data-allow-overlap. Rendered whole it has neither error.
      # Status 11-22: done; the left lurch check was folded in on stage-guards-16.

    @done
    Scenario: SG14. The frame is composed
      Given the plan
        | crate          | file                          | lines | job                                                                                                    |
        | raea-sys-stage | systems/stage/src/framing.rs  | 314   | the share of the frame any part covers (a grid of their boxes), and the shot's subject cut by the edge |
        | raea-sys-stage | systems/stage/src/manifest.rs | +17   | a CSS background over half the frame is the page's ground, not shown                                   |
        | @raea/render   | sdk/render/src/3d/label.tsx   | +2    | a Label writes its point (data-points)                                                                 |
        | raea-page      | engine/page/src/motion/manifest.rs | +17 | the manifest carries a Label's point on its words (`points`)                                         |
        | raea-sys-stage | systems/stage/src/framing.rs  | +20   | the named part nearest a Label's point is the shot's subject; its cut an error                         |
      Then the part a shot is about (one a Label or pin points at, else the nearest named part) cut by the frame's edge
        for more than 10 frames is an error
      And under 8% of the frame showing any part, for more than 1 s, is an error
      And on the film these flag the AppLovin logo cut at 6:32, the dashboard cut at 0:40 and 7:00, and the empty frames
        at 3:40, 6:04 and 6:08
      # Status 11-21 (branch stage-guards-6): framing.rs says both, as warnings for now: the page's own 2D (a headline
      # over an empty 3D) isn't known to them until SG11. Coverage leaves out a slab over 12 m (a floor, a wall: a frame
      # of floor and fog is empty), and a part wholly behind the camera. On film2: empty frames at 1:59, 3:07 and 3:41
      # (the notes' 3:40), and 21 subject cuts. Branch stage-guards-8: the page's words and videos fill the frame too
      # (the manifest's, as SG11 reads them), so a headline over an empty 3D isn't an empty frame. Still warnings: the
      # page's pictures (img, svg, a CSS background) aren't in the manifest, so one over an empty 3D would be a false
      # error. Branch stage-guards-9: the manifest lists the page's pictures (each img, inline svg, background image or
      # colour at least half opaque, past a quarter percent of the frame), and they fill the frame too. Measured on
      # Stay (approved) with its page known: its stretches at 0:39 and 0:45 clear (the page's words), and 0:02-0:18 and
      # 0:29-0:33 still read empty. Those frames show a pool room whose walls, floor and water are backdrop by the slab
      # rule, so frame-empty stays a warning: a room's inside is content the rule can't see. Past frame 1699 (once
      # nobuild's Solari dummy buffer was sized for its 64-byte vertex; at 48 the GPU refused that frame), 1:05 and
      # 1:09-1:12 read empty too: a dark sphere half in frame, then a canyon's walls filling it, backdrop by the same
      # rule. Left: the subject a Label points at; a room's inside as content.
      # Branch stage-guards-17: frame-empty is an error. Three causes of false and missed empties, each measured. A
      # room's walls were backdrop by the slab rule: a standing slab (its thin side within 45 degrees of level) fills
      # the frame now, though it is never the shot's subject; a floor, water or a ceiling still doesn't. A model's size
      # wasn't known: its file's box (read once a render) adds coverage now but never judges a frame, since raea poses
      # a person's joints off its rest box (two character sheets read empty when it did), and a room model round the
      # camera leaves the frame unjudged, as a mesh across the camera's plane does. And the page's ground counted as
      # shown: since stage-guards-9 a frame with a background colour read as full, so the film's two full-frame cream
      # fills hid all five of its empty stretches, the blank frame at 6:04 among them; a CSS background (a colour or a
      # gradient) over half the frame is ground now, an <img> or svg still shown. A frame pass or a set of copies
      # (<Instances>) leaves a frame unjudged, as grass does, and fire is of unknown size, as a model is. raea scene
      # stage over the 29 clean scenes, Stay and the film: Stay's 8 empty stretches are gone, the film's 5 stay (1:59,
      # 3:07, 3:41, 5:54 and 6:03-6:07: the notes' 3:40, 6:04 and 6:08), no clean scene has one, and every other
      # check's findings are the same. Rendered whole with their pages known: the film fails on four (3:07, 3:41, 5:54
      # and 6:03-6:07; 1:59 holds a caption that fills it), 439 errors where it had 435, and Stay (approved) on none,
      # its findings otherwise the same as before (its three meant errors, three warnings). Left: the subject a Label
      # points at; subject-cut stays a warning (33, on Stay and four clean scenes).
      # Status 11-22 (branch stage-guards-23): done. A Label writes its point (data-points), the manifest carries it on
      # its words (`points`), and the named part nearest it (within a quarter of the part's size plus 0.3 m: a Label is
      # given the part's top) is the shot's subject. A part a Label names, cut by the frame's edge for more than 10
      # frames, is a subject-cut error; the part showing most is still a guess at the subject, so its cut stays a
      # warning. A room's inside is content by stage-guards-17's standing-slab rule. On the clean scenes and the essay
      # film, which use no Label, every finding is the same.

    @done
    Scenario: SG15. A broken render can't be posted
      Given the plan
        | crate    | file                              | lines | job                                                                       |
        | -        | engine/cli/src/held.ts            | 82    | the newest render that wrote each file shown, from its project's record, and its errors less what was let through |
        | -        | engine/cli/src/app.ts             | +6    | show refuses before it posts or opens anything; Here lists a folder        |
        | -        | engine/cli/src/show.ts            | +14   | `--despite CHECK "why"`, and the why on the item's ask line                 |
        | -        | engine/cli/conformance/held.test.ts | 69  | refused, cleared by a later render, let through either way, frames and absolute outs, the flag |
        | raea-sys-stage | systems/stage/src/lib.rs    | +12   | a still stepped alone stands for its moment; reveals and reading times only over frames all stepped |
      Then `raea show` refuses a render whose record holds an open SG9–SG14 error, naming each, unless the agent passes
        `--despite CHECK "why"`, which the person sees on the item
      And the agent's hooks run SG10–SG14 on a composition's stills after each save (`raea check --quick`), so the
        fault shows before the first full render
      # Status 11-21 (branch stage-guards-6): held.ts refuses `raea show` of any file whose newest render in its
      # project's record holds an error it wasn't let through with, every error named, before it posts or opens
      # anything (all of a render's errors, not only SG9–SG14's: each one already exited the render 1). `--despite
      # CHECK "why"` passes one, and the why goes on the item's ask line. The quick check's stills are a render's, so
      # the stage checks run on them (a still stepped alone is judged for what a still shows).

  Rule: SG16. The 3D guide says it first
    @done
    Scenario: SG16. The guide's 3D rules, before any recipe
      Then sdk/guides (3d.md and craft/floor.md) and the raea skill say, before any recipe:
        | rule                                                                                                 |
        | type goes in the page over the scene; a Label points at a part; RichText only for type in the world  |
        | a scene is a <Sequence>; SDK parts read useCurrentFrame, never a frame passed down as a prop          |
        | a curve is a Line3d; a camera is a cameraPath                                                         |
        | grow a part by its transform's scale until SG1 lands; remount it when its words change                |
        | render frame-pure: SG9 runs on every render; a chunked render is a workaround, never the plan         |
      And each rule names the check that enforces it
      # Status 11-21 (branch stage-guards): sdk/guides/craft/bevy.md's rules 5 and 8-10 say these (grow by scale, one
      # clock, type that reads, a camera that settles). Branch stage-guards-7: rules 4, 5, 9, 10 and 11 name the check
      # that holds each (SG2's names, line-of-parts and part-snap, the type checks and read={false}, camera-lurch and
      # camera-reverse, raea scene stage, frame-empty and subject-cut, raea show --despite), Line3d and cameraPath are
      # the recipes, and craft/floor.md refuses labels written into the 3D, naming Label (SG8). Left: SG9's re-render.
      # Status 11-22 (branch stage-guards-23): done. craft/floor.md refuses a 3D film rendered a shot at a time
      # (`frame-pure`: a shot a process works round an engine fault, so report it with raea bug), and its Labels line
      # names the new checks (words behind a part, subject-cut). bevy/solari.md drops its 0.1.2 cautions, none of which
      # 0.1.9 shows (a sun mounted at frame 15 lights frame 40 as it does alone, SSIM 0.9988; a 10 s --at jump, 0.9994;
      # the sun's strength follows, kept-light), and says to render a film whole. The skill carries both on its main
      # (raea-skills #116, 74ab897); raea's pin takes it with the next pin after the story branch's (ST8a pins 4a77a0b,
      # not yet landed: pinning past it here would ship that branch's skill pages before its code).

  Rule: SG17. A frame alone is the render's frame, on a page too
    @done
    Scenario: SG17. A CSS animation in a frame rendered alone is where the render has it
      Given a page composition with CSS animations: one outside any <Sequence>, one inside <Sequence from={10}>
        (sdk/react/bench/css/anim.tsx)
      When frame 15 and frame 25 are rendered alone (`render --frame N`) and in a render of every frame
      Then each animation is at the same place in both: from the composition's start outside a Sequence, from the
        Sequence's start inside one
      And L2 runs it on every release candidate with the other frame-order benches (bench/l2-checks.txt: at-0, at-time,
        at-3d, compute-frames, css), each held to what is right, since P26's baseline carries a fault its raea had
      # Status 11-22: 0.1.9 drew anim.tsx's A at x 100, not 300, at frame 15 alone: a frame alone mounts every element in
      # one pass, and the page resolved them all at the earliest <Sequence> start (B's, frame 10), so A began there. On
      # a page with no <Sequence>, every animation began on the frame itself. engine/page motion.rs now resolves at the
      # earliest start, with an element outside any Sequence starting at 0, and moves each later element's animations
      # to its own start (blitz-dom delay_animations). P26 was blind to it: it renders `--at` from frame 0, where every
      # element mounts on its own frame, and its baseline is 0.1.9's. SG9 still samples only 3D renders: a CSS
      # transition begun by a style change depends on the frames before it by design, so a page's sample would flag
      # every one; the engine's side is held here instead. Measured on the fix: css/check.sh's animations PASS (A at
      # 300 and 500 alone, B at 100 and 300); a page with no <Sequence>, frame 15 alone: x 300 (0.1.9: x 0); the five
      # L2 bench checks 5/5.
