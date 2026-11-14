@spec
Feature: The timeline SDK (0.1.5): CapCut's timeline, on the composition, in the feed, open to extension
  Shane, 2026-11-13 (goal, launching alongside the character SDK): "the goal with the timeline sdk is to provide
  agents pushing feed items or controllers / things we might want to be able to edit directly with timeline based
  adjustment which includes full on video editing in the feed watch/scene items if any components or compositions are
  in the sequence, text, effects, transitions etc. the goal should be to actually open and explore every inch and
  aspect of how capcut timeline works and basically do the exact same thing. we want full editing ability and to
  remove the timeline 'comments' as we can just do that in the viewer at whatever timecode im at ... the switcher on
  the left for video/audio and we should basically allow those to expand; we also can add some way of allowing for
  keyframes, changing the way you make bezier curves and things. the idea is that we provide the framework and parity
  functionality and feel to capcut while allowing it to be extended. This also means live editing buffering and
  playback of whole sequences, 3d scenes, etc need to be done to include buffering, caching, smart cache clearing,
  etc. these changes in the timeline are not suggestions they are actual edits sent to the agent. these should be
  deeply connected to the watch, listen, and scene feed experiences."

  What there is to build on (read 11-13):
    - docs/editing.feature E1 to E5: the composition's file is the document, a change is a change to its code made at
      once, the working version (NAME-vN.tsx), a <Sequence>'s span edited on a strip (host/sequences.ts), the engine's
      stage playing the working file live (E4a), an agent editing through the same operations (editor_edit, E5a).
    - raea-feed src/detail: the viewer's strip (blocks/Filmstrip.tsx: two tracks, a kind picker in each track's
      gutter, plugins' lanes, the comments' pins and a comment card on the strip), the hand editor (timeline/Editor.tsx,
      Strip.tsx: cuts over the render, parts retimed, undo and redo), the composition editor (src/compositions).
    - raea-feed PR #18 (11-13): a scrubber that only seeks, the strip collapsed to it, a resize handle on its top edge.
    - The proxy store (host/preview/proxies.ts: light copies, thumbnails, peaks, a size limit and a sweep), the export
      and bundle caches (runtime/bundle cache.rs), rr3's shot segments (docs/rerender.feature, behind RAEA_SEGMENTS=1).

  CapCut's desktop timeline, as its own help pages and guides describe it (read 11-13; no CapCut code is read or
  copied, and no other editor's under a licence that forbids it: docs/editing.feature's note on react-video-editor-pro):
    - Above the timeline, left: the pointer's mode (Select A, Split B, select everything left [ or right ]), undo and
      redo, Split (Cmd+B; Cmd+Shift+B every track), Delete left Q and Delete right W (of the playhead, on the picked
      clip, the gap closed), a marker M, Delete, and for a picked clip Freeze, Reverse, Mirror, Rotate, Crop.
      Right: record a voice, the main track magnet P, auto snapping N, linkage ~, the preview axis S (the frame under
      the pointer), fit Shift+Z, and zoom (Cmd +/-, Cmd+scroll; Option+scroll scrolls sideways).
    - Tracks: one main track that never holds a gap while its magnet is on (a clip dropped between two moves the rest;
      off, a clip dragged over others becomes an overlay on a track above); overlay picture tracks above it; text,
      sticker, effect, filter and adjustment tracks above those; sound tracks below. Each track's head locks it, hides
      it (eye) or mutes it (speaker).
    - A clip: picked by a click, several by Cmd+click or a box; moved by a drag, trimmed by its edges (back out again
      to the source's end), split, deleted, duplicated, copied and pasted, its attributes pasted onto another
      (Cmd+Shift+C/V), disabled (V), grouped (Cmd+G), made a compound clip (Option+G), its sound extracted
      (Cmd+Shift+S). I and O mark a range.
    - Moving in time: Space, Left/Right a frame, Shift+Left/Right several, Up/Down the cut before or after, Home/End.
    - Keyframes: a diamond beside each property in the inspector stores its value at the playhead; a second, later
      one with another value animates between them; the diamonds show on the clip; a graph shapes the change (linear,
      ease in, out, both, or a curve dragged by hand). Position, scale, rotation, opacity, colour, volume.
    - Speed: constant, or a curve of points from 0.1x to 10x with presets (Montage, Hero, Bullet, Jump cut, Flash in,
      Flash out) and a custom one; the clip's length follows.
    - A transition sits on the cut between two clips that touch; its length is dragged. Effects, filters and
      adjustments are clips on their own tracks, over the span they cover.
    - Sound: the waveform on the clip, a volume line, fade in and fade out as lengths at the clip's ends, beat markers
      on the waveform that edges snap to.
    - Playing while editing: lighter copies of heavy media (Proxy, 540 or 720 lines), a lower preview quality, parts
      that were worked out kept (the cache: its size shown, cleared by hand or on a schedule), all remade when needed;
      an export always reads the originals.
    - The inspector beside the player (read 11-14), its tabs by the picked clip's kind. Video: Basic (position, scale,
      rotation, opacity, blend mode, stabilise), Mask (line, mirror, circle, rectangle, heart, star; position, size,
      rotation, feather, round corners, invert), Cutout (auto cutout of a person, a custom cutout brushed, chroma key
      with its colour picked from the picture, strength and shadow), Retouch. Animation: In, Out, Combo. Speed:
      constant and curve. Adjust: brightness, contrast, saturation, exposure, sharpen, highlights, shadows,
      temperature, tint, fade, vignette, grain. Audio: volume, fades, noise reduction, voice changer. Text: font,
      style, effects, bubbles, animation, and its words read aloud.
      Here 11-14: Basic but stabilise (TL3h, TL5), Mask, chroma key and Blend (TL7f) with the key's colour picked from
      the picture (TL7h) and the mask's handles dragged there (TL7i), Animation (TL7d, TL7e), Speed (TL6), Audio's
      volume and fades (TL8a) and its noise reduction and voice changer (TL8d, a film's sounds; a composition's
      not yet), Text's words, styles and captions (TL7c) and its words read aloud (TL7j), Adjust as effect clips
      over a span (TL7b, raea's ten) and as the clip's own twelve sliders (TL7g, on the film and on a file from Media
      over it), every number of these keyed (TL5a). Not yet: stabilise, auto and brushed cutout, and Retouch.

  The model this file sets:
    1. One timeline model, in an SDK (sdk/timeline, TypeScript, no React, no DOM): tracks, clips, keys, markers, and
       the operations on them, each a pure function from a timeline to the next. The app's strip draws it and the
       host writes it; an agent reads and writes the same model. It replaces the strip's own cut and parts.
    2. The composition's file is still the document (docs/editing.feature): a timeline is read from a composition's
       tree, and each operation is also a code edit to the working version. What the code can't express as written
       (a span set by an expression) is shown locked, with why.
    3. An operation made by the person is an edit, applied at once, and sent to the item's agent as the edit that
       was made, never as a note asking for it.
    4. Parity first, in function and feel: CapCut's tools, keys, magnet, snapping and gestures, at 60 frames a second
       under the pointer. Then the points where it is extended: a kind of track, a kind of clip, a property and its
       editor, an easing, a transition, an effect, a lane, a tool, each a plugin's contribution.
    5. Comments are the viewer's. The timeline takes no comment and opens no comment card; a comment made on the
       picture at a moment shows as a marker on the timeline, which opens it.
    6. The picture under an edit is the engine playing the working file (E4a) where there is a stage, with what was
       drawn kept: a range's frames are kept by what draws them, an edit drops only the ranges it changed, and the
       store keeps itself under a size the person sets.

  Status, 11-13 (the first day). On raea main: sdk/timeline (model, ops with inverses, compound clips, keys and
  bezier easing, speed curves, freeze frames and reversed footage, snapping, the frame store's decisions, live
  playback's mapping, readers and writers for a composition's sequences (its footage and sound, its text, its
  effects, a recording, a project's file), a film's scenes and a sound's tracks, the palette a plugin adds to, a clip's cutout and Adjust; 106
  tests), and in
  sdk/video <Nudge keys mask blend chroma adjust> (keys for the placement and, by a dotted name, a mask's, a key's, a
  crop's and an adjustment's numbers; cutout.ts: a mask's six shapes, CSS's blend modes, a chroma key; adjust.ts:
  CapCut's twelve adjustments as one SVG filter and a vignette layer), <Transition in out combo> (fade, slide, wipe, zoom; a Combo's seven waves), <Effect kind amount in out> (blur, mono,
  sepia, vivid, faded, contrast, bright, dark, invert, hue: a filter on the picture under it) and a <Video>'s or
  an <Audio>'s speedCurve (the mixer carries its sound through the changing rate as one voice). In raea-feed, draft
  PR #19: the new timeline is the timeline (localStorage raea.timeline-sdk = 0 brings the old strip back); 45 browser
  tests in Chrome, all passing; in a headless raea window 43 of the 45 pass and two skip themselves there with their
  reason (the voice test, which would ask for this Mac's microphone, and TL10d, whose film the window decodes itself,
  so no route can hold its data), among them TL9c, an agent's edits, a composition's every edit, freeze and reverse, mirror and
  crop, effects, groups and compounds, the posted item's, a plugin's effect, text style, tool and snap grid, saved
  curves, Settings' kept frames, both kinds of sound taken out, the switcher, the export range, media added, the
  Option keys, captions made from what is said, a clip's In, Out and Combo, and its mask, blend and chroma key (whose
  sliders are a track of their own, as the volume's is: the window doesn't move a range input), their numbers and the
  crop's sides keyed (TL5a), its own Adjust (TL7g), its key's colour picked from the picture (TL7h: read from the
  clip's file by the host, since the window has no canvas to read), its mask's handles dragged there (TL7i), and a file
  from Media over the film with its own vignette and its key's colour picked from its own file, a text's words
  read aloud onto a sound track (TL7j), and a sound's noise reduced and its voice changed (TL8d: its menu is made
  again after each try, since the window keeps a picked option shown when the clip's didn't change). The window runner (blitz/pw.mjs) answers routes one at a time in the order asked, keeps a reload's
  address and each test's storage, and has keyboard down and up, toBeFocused and waitForResponse. The timeline's keys
  follow the pointer over it, not :hover, which raea's window doesn't match for a script, so there they only worked
  after a click in the timeline:
    built      TL0a-d, TL1a-b, TL1c (the switcher on the left of each track, picking what its clips show: the
               frames, the energy, the waveform, the spectrogram, the words said, with Transcribe offered until
               there are words; only what the film has is offered, each clip draws its own stretch of the film,
               and the choice is kept by the track's kind for the next film; a sound in Listen the same; a clip
               of one of the project's files, a sound track's or one added from Media, draws its own file's
               loudness, read once on the host (/host/asset-peaks); a plugin's lane as a row under the tracks;
               collapse and the top edge's resize),
               TL1d (the poster on the main track's head: a click makes the playhead's frame the poster),
               TL2a-e, TL2f (I and O in either order, Shift+X around the picked clips, its ends dragged on the
               ruler, Option+X or the × beside Export clearing it; kept with the item; Export says the span and
               the host cuts only it with raea media trim, raea's encoder signing it as every export: a real
               render's 1 s to 2.5 s came out 1.5 s, 90 frames and its sound), TL3a-g (and TL3b's Option-drag,
               leaving the clip and putting a copy down), TL3h (freeze and reverse played live from the film's own
               frames; mirror,
               rotate and crop shown on the player at once, as are a clip's keyed scale, place, turn and opacity),
               TL3i, TL3j (Media in the bar, Cmd+I: the project's footage, stills and sounds, each added at the
               playhead as long as its file, a still 3 s; on a film cut here footage and stills go over the
               picture, drawn on the player where they are on, a sound on a sound track), TL4a-b, TL4c (and a clip's own sound taken out with Cmd+Shift+S: the SDK's extract
               operation, a sound clip over the same span from the same place in the file, the footage marked
               muted; in a composition the host mutes the <Video> and writes an <Audio> of its file beside it, read
               back as clips; in a film cut here the player is silent over that footage and the sound plays from
               its place in the film wherever it is moved), TL5a-b, TL5c (and a curve shaped by hand saved by name for the workspace: kept as
               timeline/curves.json in its folder, a preset in every timeline there, forgotten by its ×), TL6a-b
               (and Option on a clip's edge changing its speed, Cmd+R to the speed, Shift+B to its curve),
               TL7a, TL7b for raea's ten effects (an effect is a clip of its own,
               shown on the player where it is on), TL7c (text added with T or in a style from the bar, drawn over a
               film where it is on, renamed by a double-click, a caption split at the caret with Enter; raea's
               four text styles and a plugin's, as data; and captions made from what is said, CapCut's auto
               captions: Captions in the bar transcribes the film first when it has no words yet, then puts the
               words, where the edited cut now says them (sdk/timeline captions.ts heard: a scene taken out takes
               its words, a duplicated one says them twice, a curved, reversed or frozen clip none), in lines (a
               sentence's end, a pause of 0.6 s, seven words or 42 characters end one) as text clips in the
               caption style on a Captions track of their own: one edit (the `captions` operation), undone
               whole, drawn over the film, told to the agent as one line with every caption's words and
               timecodes, and in a composition each line a sequence of words), TL7d's In and Out (a picked
               clip's own animation at each end, CapCut's Animation tab: fade, slide, wipe, zoom or a plugin's,
               and its frames, in the clip's row; a ramp on the clip; the film's picture and words over it show it
               at once, drawn by sdk/video edges.ts, the table <Transition> now draws from too; split, each half
               keeps its own end's; never longer than the clip; in a composition the sequence's <Transition>
               edge, taken off with no frames, read back as the clip's own where no cut's transition takes it;
               Send says how it comes in or goes out), TL7e (a Combo: seven of raea's, as waves in sdk/video
               edges.ts, and a plugin's as data; its cycle starts at the clip's length, playing once, and loops
               when shorter; with the In and Out at once on the film's picture and words; a line with a tick a
               cycle on the clip; <Transition combo> in a composition, written and read back), TL7f (a mask, a
               blend and a chroma key: the row's blend menu, and a mask and a key panel; a mask an SVG mask image
               laid over the frame, feathered by a blur, inverted by mask-composite; a blend CSS's mix-blend-mode;
               a key an SVG colour-matrix filter whose alpha falls as a colour's share of the key's hue rises; drawn
               by sdk/video cutout.ts in the preview and in <Nudge>, and by raea's renderer as Chrome draws them,
               seen on a render of a keyed green panel, a feathered heart, an inverted turned star and a screen
               blend; an SVG <mask> to invert overran Vello's buffers, so it isn't used; a clip's cutout is its
               props, a preset's to set later; Send says all three as they stand), TL7g (the clip's own twelve
               Adjust sliders, one SVG filter after its key and a vignette layer, by sdk/video adjust.ts in the
               preview and in <Nudge adjust>; raea's renderer within 0.4% RMSE of Chrome on a probe), TL7h (the
               key's colour picked from the picture: a press on the film or a file over it, undone through its
               placement, read by the host from the clip's file where the player is, a small patch's mean), TL7i
               (the mask's outline and CapCut's handles on the picture through the clip's placement: move, width,
               height, corner, turn, feather, round; a drag shown as it moves and one edit when let go, keyed
               numbers keyed), TL7j (a text's words read aloud: raea gen speech on the host, kept under
               media/speech by the words' hash, a sound clip from the text's start; a real line 8.4 s to make,
               43 ms read again), a plugin's effects as data (timeline.effects in its manifest: parts
               of CSS filter functions, drawn by <Effect parts> and by the preview alike), TL8a
               (waveform, level line, fades; Cmd+. and Cmd+, step the level by 0.1 dB, or key it at the
               playhead when it is keyed), TL8b's beats as ticks that edges snap to, each turned off by a
               click, TL8c (Record from the playhead, the picture playing; the recording a clip named by what was
               said, its file in the project's attachments), TL8d (a sound's noise reduced and its voice
               changed: host/dsp.ts in TypeScript, an FFT and Hann overlap-add, the noise's floor from the quietest
               tenth, a phase vocoder for pitch, biquads for the bands, a delay line for the echo; on made sounds hiss
               down 16 dB with the tone within 0.04 dB, 3 minutes in 2.3 s; on a real voice the chipmunk moves its
               spectral centroid x1.46 and deep x0.77, each setting 25-73 ms for 1.2 s, kept under media/voice and
               read again in under 1 ms), TL11a for easings, speed curves, transitions, effects, text styles, tools (presets put
               on the picked clips) and snap grids as data (a clip let go 6 px past a plugin grid's mark lands on
               it, under a browser test with the plugin installed; each e2e run now starts with no plugins
               installed, since an earlier run's grids had stayed approved and snapped a run whose plugin had none),
               TL9c (an agent's editor_edit made as raea/editor operations on the open timeline, which
               editor_selection gives by clip id: one step in the person's history, undo taking it back, Send
               telling it as the agent's own, the clips it touched ringing and the bar saying what it did), TL9d
               (an item's own timeline: an edit kept in the item's data, Send telling its agent), TL10d (below),
               TL10e (a composition's drafts and a scene's ranges kept in the proxy store, <store>/drafts and
               <store>/scene-frames, under its one limit with the copies: one line by last play across all of
               them, so a draft played an hour ago goes before a still looked at a minute ago; Settings › Light
               copies lists drafts of edits and scene ranges with their sizes and Clear takes them; finishing or
               discarding a sitting forgets its drafts at once, and any ranges of the discarded version; kept
               frames whose file is gone go at the next sweep, as a copy whose source is gone does. Seen on a real
               host on its own port: 1 draft and 2 ranges listed as 15,000 bytes, Clear freeing all three), TL11b (the model as raea/editor, with its reference page and an example), TL12a-b, TL14a-c (a
               scene's timeline as long as the scene, past its last sequence)
    in code    a composition's sequences: move, trim, split, delete (E3), keys on x, y, scale, rotate and opacity
               (TL5d), its still placement as its <Nudge>'s props, mirror and crop among them (TL3h), and its mask,
               blend and chroma key (TL7f: mask={{ kind, x, y, w, h, rotate, feather, round, invert }},
               blend="screen", chroma={{ color, strength, shadow }}, plain values only, any other the code's), a speed or a
               speed curve for a sequence that holds footage, split in two with it (TL6a-b), a transition on a cut
               (TL7a), an effect as a sequence holding an <Effect>, its kind and amount (TL7b), a sequence of
               words as a text clip, its words rewritten, words added as a new sequence (TL7c), a <Video>'s or an
               <Audio>'s level (TL8a), a recording as a sequence holding an <Audio> of its file (TL8c), a
               project's file as a sequence holding a <Video>, <Img> or <Audio> of it, named by its path in
               the composition's media folder (cloned into media/added/ first when it is elsewhere in the
               project), in at the nearest cut with the main track moving on and the composition growing (TL3j); each
               written into the working version the moment it is made, its draft played over the picture (TL9a,
               TL10d), finished and posted on Send with the agent told (TL9b). A scene's sequences: move, trim,
               split, delete written into the scene's own file, which the stage opens again (TL14c). A sound's
               clips: span, place in the file, level (TL14b)
    plays      TL10a for a film's own frames: cuts, moves, trims, speeds, freezes and reversed footage at once, the
               player's buffer on the ruler. TL10f: a timed 3D scene's frames are drawn ahead by the host a few
               seconds at a time at draft quality and kept by range, each range under what it is drawn from (the
               code outside the scene's sequences and the sequences on in it), so an edit to one sequence draws
               again only the ranges it is on and an undo plays what it had; the store stays under its one
               limit (TL10e), the least recently played going first, and beside the switch the
               scene's kept frames say what they take and are cleared by a click; the scene's timeline
               shows what is kept, and Play runs it over the stage from the scene's own camera. TL10d: where play
               reaches a range not kept yet while the frames are being drawn, the picture holds on it saying
               Buffering until a second from it is kept (with nothing being drawn, the live stage plays on), and
               a film that runs out of data says Buffering… over the picture. Seen in a headless raea window on
               the real host: the sample room drawn, kept, and played at its own rate; a 9 s copy of it opened
               again with every range kept at once (0.5 s); a sequence dragged across two of its three ranges
               drew those two again (9.4 s) and left every kept file as it was; and with nothing kept and Play
               pressed at once, Buffering to 7.7 s, the first range played, Buffering again at its end (11.0 s)
               until the next was kept (13.5 s)
    decided    TL10b, c and e in the SDK (which ranges an edit changes, a range's key, what goes first); a
               composition's drafts are kept by their source, so an undo plays its draft again at once (and under
               the store's one limit since TL10e); beside a composition's timeline the
               kept drafts say what they take and are cleared by a click. Drafts and a scene's kept frames are
               drawn smaller than they ship and laid out as they ship (raea's <Composition edit={{ scale }}>, the
               long side at 960 px; docs/lightspeed.feature PF4): a ray-traced 1080p shot's draft 2.75 s at full
               size, 1.7 s scaled
    not built  TL13a's drag at the window's rate with 400 clips (measured below: Chrome keeps it, raea's window
               draws 35 to 37 frames a second, and what is left is the window's own frame after a change; drawing
               only the clips in view would draw none fewer, since at its opening fit all 12 tracks and all 400
               clips are on screen at once). The
               skills guide's note on an item's `timeline` field waits for PR #19 to merge

  Background:
    Given the SDK is sdk/timeline in raea, the app's side is raea-feed src/detail (timeline, strip, blocks), the
      host's is raea-feed host (sequences.ts, edit.ts, preview/proxies.ts) and the engine's stage is engine/page/stage
    And every scenario below is a task (docs/agents.md "Tasks: rask"); a status line under a rule says what is built

  Rule: TL0. One model: tracks, clips, keys, markers
    # Plan (crate or package | file | lines | job)
    #   sdk/timeline | src/model.ts   | 150 | Timeline, Track, Clip, Key, Marker, Ease; ids; frames as integers
    #   sdk/timeline | src/ops.ts     | 300 | every operation as (timeline, op) -> timeline; inverse for undo
    #   sdk/timeline | src/snap.ts    | 120 | what an edge snaps to, and the magnet's closing of gaps
    #   sdk/timeline | src/keys.ts    | 160 | a property's value at a frame; cubic bezier easing; presets
    #   sdk/timeline | src/speed.ts   | 120 | a speed curve's source time at an output time, and its length
    #   sdk/timeline | src/read.ts    | 200 | a timeline from a composition's read tree (host/sequences.ts' rows)
    #   sdk/timeline | src/edits.ts   | 200 | an operation as code edits to the working version

    Scenario: TL0a. Time is whole frames
      Given a timeline at 30 frames a second
      Then every clip's start, length and trim, every key and every marker is a whole number of frames
      And a timecode is drawn from frames (mm:ss:ff), never the other way

    Scenario: TL0b. An operation is a pure function with an inverse
      Given any timeline and any operation that applies to it
      Then applying the operation and then its inverse gives the timeline back, to the frame
      And an operation that can't apply (a locked track, a clip trimmed past its source) is refused with its reason

    Scenario: TL0c. A timeline is read from a composition
      Given a composition with sequences, a <Video>, an <Audio>, a component and a <Series>
      Then each is a clip with its span, on a track by its kind and its depth in the tree, and names its element
        (file:line:col, docs/editing.feature E2)
      And a span written as an expression is a clip marked locked, with "set in code"

    Scenario: TL0d. A feed item or a controller declares a timeline
      Given an agent posts a feed item, or a controller has a field, that carries a timeline (tracks of clips over
        anything with a time: a film, a song, a scene's animation, a schedule)
      Then the app draws it with the same strip and tools, and its operations go back to the agent as TL9 says

  Rule: TL1. Tracks: a main track, overlays above, sound below, each with a head
    Scenario: TL1a. Tracks by kind, in CapCut's order
      Then from the top: effect and adjustment tracks, text tracks, overlay picture tracks, the main track, sound tracks
      And a new clip of a kind with no free track makes one in its place in that order

    Scenario: TL1b. A track's head
      Then each track's head has its kind's icon and name, a lock, an eye (picture) or a speaker (sound)
      And a locked track's clips can't be picked or changed, a hidden one isn't drawn in the picture, a muted one isn't heard
      And these are edits like any other (TL9): the composition is drawn that way until they are turned back

    Scenario: TL1c. Tracks expand
      Given the switcher on the left of a track (what it shows: frames, waveform, spectrogram, words, a plugin's lane)
      When I drag a track's lower edge, or double-click its head
      Then that track alone grows or shrinks, from a line to tall, and what it shows gains detail with its height
      And the whole timeline still resizes from its top edge and collapses to its scrubber (raea-feed PR #18)

    Scenario: TL1d. The main track's cover and the first frame
      Then the main track's head shows the film's poster, and picking it sets the frame used as the item's poster

  Rule: TL2. Moving through time
    Scenario: TL2a. The ruler and the playhead
      Then a ruler over the tracks marks time at the zoom's scale (frames, seconds, tens of seconds)
      And a press or a drag on the ruler or the scrubber moves the playhead and nothing else

    Scenario: TL2b. Keys, as CapCut's
      Then Space plays and pauses; Left and Right step a frame; Shift+Left and Shift+Right step ten
      And Up and Down go to the cut before and after; Home and End to the first and last frame
      And every key is listed in one place (the app's shortcuts), can be changed there, and shows in its button's tooltip

    Scenario: TL2c. Zoom and scroll
      Then Cmd+= and Cmd+- zoom about the playhead, Cmd+scroll about the pointer, Shift+Z fits the whole timeline
      And scroll moves through the tracks, Option+scroll (and a sideways scroll) through time
      And the zoom goes from the whole film in the strip's width to one frame at 12 px or more

    Scenario: TL2d. The preview axis
      Given the preview axis is on (S)
      When the pointer moves over the tracks
      Then the picture shows the frame under the pointer without moving the playhead, and goes back when it leaves

    Scenario: TL2e. Markers
      When I press M
      Then a marker is at the playhead (on the picked clip if one is picked, else on the timeline), which I can name,
        colour, drag and delete, and which edges snap to

    Scenario: TL2f. A range marked for export
      When I press I at one place and O at another (in either order), or Shift+X with a clip picked (CapCut's
        "select an area by clip"; capcut.com/help/export-clip-of-video)
      Then the span between them is marked on the ruler, its ends dragged, and Option+X clears it
      And an export of the item renders only that span, all its tracks; with none marked it is the whole timeline
      And the range is how the timeline is seen, not an edit: it isn't sent, and it is kept with the item's timeline
        until it is cleared, so a later export isn't cut short without it showing

  Rule: TL3. Clips: pick, move, trim, split, delete
    Scenario: TL3a. Picking
      Then a click picks a clip, Cmd+click adds or removes one, a box dragged from empty track picks what it touches
      And [ and ] pick every clip left or right of the playhead; Cmd+A all; Escape none
      And what is picked is what the viewer's inspector shows and what editor_selection tells the agent

    Scenario: TL3b. Moving
      When I drag a clip (or the picked clips together)
      Then it moves in time on its track, or to another track that takes its kind, snapping as TL4 says
      And Option held while dragging leaves a copy behind

    Scenario: TL3c. Trimming
      When I drag a clip's edge
      Then its start or end moves and, for media, its place in the source with it, never past the source's own ends
      And a trimmed edge can be dragged back out to what was there

    Scenario: TL3d. Splitting
      Then Cmd+B splits the picked clips at the playhead (every clip under it when none is picked)
      And Cmd+Shift+B splits every track; B makes each click a split until A; a split clip's keys stay at their frames

    Scenario: TL3e. Delete, delete left, delete right
      Then Delete removes the picked clips; on the main track with the magnet on, the gap closes
      And Q removes what is left of the playhead on the picked clip, W what is right of it, the gap closed
      And Shift+Delete closes the gap on any track

    Scenario: TL3f. Copy, paste, duplicate, attributes
      Then Cmd+C, Cmd+X and Cmd+V copy, cut and paste clips at the playhead on their tracks; Cmd+D duplicates after
      And Cmd+Shift+C and Cmd+Shift+V copy a clip's properties, keys and effects onto the picked ones

    Scenario: TL3g. Disable, group, compound
      Then V turns the picked clips off and on (drawn dimmed, left out of the picture and the sound)
      And Cmd+G groups clips so they pick and move as one; Cmd+Shift+G lets them go
      And Option+G makes the picked clips one compound clip (in code, one component holding them), opened by a
        double-click to edit inside; Option+Shift+G puts them back

    Scenario: TL3h. Freeze, reverse, mirror, rotate, crop
      Then with a picture clip picked the bar offers Freeze (a still of the playhead's frame, 3 s, inserted there),
        Reverse, Mirror, Rotate (90 degrees) and Crop, each an edit to that clip's element

    Scenario: TL3i. Undo and redo
      Then Cmd+Z and Cmd+Shift+Z step through every operation, the person's and the agent's (E5a), one history
      And a drag is one step however many frames it crossed

    Scenario: TL3j. The project's media, added at the playhead
      Given the project's footage, stills and sounds (the files its Source page's Assets tab lists)
      When I open Media in the bar and press a file's +, as CapCut's media panel adds one
      Then it is a clip at the playhead as long as its file (a still 3 s, CapCut's default): footage and stills on
        the main track, a sound on a sound track
      And in a composition it is a sequence holding a <Video>, an <Img> or an <Audio> of the file, named by its path
        in the composition's media folder (a file elsewhere in the project copied into media/added/ first, since a
        render reads that folder alone); on the main track with the magnet on, the clips after it move on by its
        length and it is drawn under what was over them
      And in a film cut here it goes over the picture on an overlay track (the film's own cut is its scenes) and
        plays there, a sound plays where it is placed, and Send tells the agent which file went where

  Rule: TL4. The magnet, snapping and linkage
    Scenario: TL4a. The main track's magnet (P)
      Given the magnet is on
      Then the main track has no gap: a clip removed closes it, a clip dropped between two moves the later ones on
      And a clip dragged past its neighbour's middle swaps places with it
      Given it is off
      Then clips stay where they are, a gap is allowed, and a clip dropped over another goes to a track above

    Scenario: TL4b. Snapping (N)
      Then a dragged clip's edges, a trimmed edge and the playhead snap within 8 px to clip edges on every track,
        the playhead, markers, beats and the film's ends, with a line drawn where it snapped
      And Cmd held while dragging turns snapping off for that drag

    Scenario: TL4c. Linkage (~)
      Given linkage is on
      Then what sits over a main-track clip (text, effects, overlays that start inside its span) moves, is split's
        left or right part, and is deleted with it
      And a video clip's own sound moves with it until Cmd+Shift+S extracts it to a sound track

  Rule: TL5. Keyframes and curves
    Scenario: TL5a. A key is a property's value at a frame
      Given a clip is picked and the playhead is inside it
      When I press the diamond beside a property (position, scale, rotation, opacity, volume, any prop its element
        declares as a number, a colour or a point)
      Then that property has a key there; changing the value at another frame adds a key there
      And between two keys the value follows the first key's ease; before the first and after the last it holds
      And the numbers inside a mask (its centre, size, turn, feather, corners), a chroma key (strength, shadow) and a
        crop (each side) have their keys beside them in their panels, as the placement's are in the row, and the
        picture follows them as it plays; in a composition they are the <Nudge keys> of a dotted name ("mask.feather",
        "chroma.strength", "crop.left"), a curve that overshoots held inside what the value can be

    Scenario: TL5b. Keys on the clip
      Then a clip with keys shows them as diamonds along it (Option+K shows or hides a row a property)
      And a diamond is dragged in time, copied, deleted; a click on one moves the playhead to it
      And the diamond beside the property steps to the key before and after

    Scenario: TL5c. Easing is a cubic bezier, edited by hand
      When I open a span between two keys
      Then a graph shows its curve with two handles; dragging them changes it; the picture plays it as I drag
      And presets fill it (linear, ease in, ease out, ease in-out, hold, and those a plugin adds), and a curve I made
        can be saved by name for the workspace
      And in code a key's ease is its four numbers ([x1, y1, x2, y2]), "linear" or "hold"

    Scenario: TL5d. Keys in code
      Then a property with keys is written on its element as one readable prop (keys={{ scale: [[0, 1], [45, 1.15,
        [0.2, 0, 0, 1]]] }} through one wrapper, as <Nudge> is for a still value), re-written in place on each change
      And a property animated by the composition's own code (interpolate, spring) is shown as a curve that can be
        read but not dragged, with "set in code"

  Rule: TL6. Speed
    Scenario: TL6a. A constant speed
      When I set a picked clip's speed (Cmd+R) from 0.1x to 100x, or drag its edge with Option held
      Then its length follows, its sound keeps its pitch unless I say otherwise, and the magnet closes or opens the gap

    Scenario: TL6b. A speed curve
      When I choose Curve (Shift+B)
      Then the clip has points from 0.1x to 10x over its source, with the presets Montage, Hero, Bullet, Jump cut,
        Flash in and Flash out, and a custom one whose points I drag, add and remove
      And the clip's length is the curve's, and a line over the clip shows where it is fast and slow

  Rule: TL7. Transitions, effects, text
    Scenario: TL7a. A transition sits on a cut
      Given two clips that touch on one track
      When I drop a transition on their cut (or pick one from the cut's own button)
      Then it is drawn across the cut, its length dragged from either end, within what both clips have beyond the cut
      And it is a component both clips' elements are wrapped by in code, one a plugin can add

    Scenario: TL7b. Effects, filters and adjustments are clips
      Then each is a clip on a track above the picture, covering the span it applies to, trimmed and keyed like any other
      And with linkage on it follows the main-track clip under its start

    Scenario: TL7c. Text and captions
      Then a text is a clip on a text track; double-click edits its words in the picture; Enter splits a caption in two
      And captions made from the words (the transcript) are clips on a caption track, each moved, trimmed and edited

    Scenario: TL7d. A clip's own In and Out animations
      Given a clip of footage, a still or text picked
      When I give it an In animation or an Out animation (CapCut's Animation tab: fade, slide, wipe, zoom, or a
        plugin's transition) and how many frames it takes
      Then it plays over the clip's own first or last frames, drawn as a ramp at that end of the clip, and the picture
        shows it at once
      And split, the first part keeps the In and the second the Out; it is never longer than the clip
      And in a composition it is the sequence's own <Transition in out>, the same component a cut's transition writes,
        and read back from it as the clip's own wherever no cut's transition takes it

    Scenario: TL7e. A clip's Combo animation
      Given a clip of footage, a still or text picked
      When I give it a Combo (CapCut's third kind: pulse, sway, float, shake, swing, spin, flicker, or a plugin's)
      Then it moves over the clip's whole length, a cycle every so many frames: the clip's own length (what it starts
        at) plays it once, as CapCut's Combo does, and fewer loop it, as CapCut's text Loop does
      And it plays with the clip's In and Out at once, drawn as a line along the clip with a tick at each cycle
      And a Combo is data, a wave of the clip's x, y, scale, rotate and opacity over one cycle, so a plugin adds one in
        its manifest and the preview draws it
      And in a composition it is the sequence's <Transition combo>, written and read back

    Scenario: TL7f. A clip's mask, blend and chroma key
      Given a clip of footage or a still picked
      When I give it a mask (CapCut's Mask tab: a line, a mirror band, a circle, a rectangle, a heart or a star)
      Then the picture is seen through it at once: a shape that starts round in the middle of the film, moved and
        sized in shares of the frame, turned, its edge feathered, a rectangle's corners rounded, or inverted to keep
        what is outside it
      When I give it a blend (Basic's Blend: darken, screen, overlay, multiply, lighten, hard and soft light, colour
        burn and dodge, add, difference, exclusion)
      Then it is drawn onto what is under it by that mode
      When I key a colour out of it (Cutout's Chroma key: a green or blue screen, or a colour typed, its strength and
        its shadow)
      Then that colour's hue goes from the picture, the screen's shadows let through half seen as far as the shadow
        says; a grey, which has no hue to key by, is refused in words
      And in a composition each is a prop of the sequence's <Nudge> (mask, blend, chroma), written and read back,
        and raea's renderer draws them as the preview does

    Scenario: TL7g. A clip's own Adjust
      Given a clip of footage or a still picked
      When I adjust it (CapCut's Adjust tab: brightness, contrast, saturation, exposure, sharpen, highlights, shadows,
        temperature, tint, fade, vignette, grain), each a slider from as shot, keyed as any number is (TL5a)
      Then the picture shows it at once, under the effects over it, after its chroma key; a file from Media over the
        film shows its own, its vignette over its own picture where that sits in the frame
      And in a composition it is the sequence's <Nudge adjust>, written and read back: its tone one curve read through
        a table, its colour two colour matrices, sharpen a convolution, grain a noise whose seed is the frame, and a
        vignette a darkening laid over the part's frame; raea's renderer draws them as Chrome does (11-14 probe: tone,
        colour, sharpen and vignette within 0.4% RMSE of Chrome, grain the same strength, its noise its own)
      And Send tells the agent the adjustments as they now stand

    Scenario: TL7h. The key's colour picked from the picture
      Given a clip of footage or a still picked, its chroma key open
      When I press pick and then press on the picture (CapCut's picker: a crosshair, a ring under the pointer)
      Then the key's colour is the clip's own at that point: read from its file where the player is in it, before its
        key, the mean of a small round patch so one noisy pixel doesn't set it, wherever the clip is placed, turned,
        scaled or mirrored on the film; a press off its picture is said, not read
      And the key takes it at once as a typed colour would, and the press leaves no comment and doesn't play
      And Esc, or pick again, stops picking and keeps the clip picked
      And the picture is read by the host from the file, not from the screen (raea's window has no canvas to read),
        so it is the same colour in raea's window and in Chrome

    Scenario: TL7i. A mask's handles dragged on the picture
      Given a clip with a mask picked, its mask panel open and the playhead on it
      Then the mask's outline is drawn on the picture where it is, through the clip's placement, with CapCut's
        handles at a constant size: the shape itself to move it, an edge's for its width or its height, a corner's for
        both in proportion, a turn handle under it, a feather handle over it that runs out as the edge softens, and a
        rectangle's corner rounding along its top edge (a line has its turn and feather; a mirror's band its height)
      When I drag one
      Then the picture's mask follows as it moves and it is one edit when let go, the panel's numbers with it: a number
        keyed (TL5a) takes a key at the playhead, the rest a still value
      And a press on them leaves no comment and doesn't play

    Scenario: TL7j. A text's words read aloud
      Given a text clip picked, in a film or a composition with a project
      When I press read aloud (CapCut's text to speech)
      Then its words are made into a voice on the host (raea gen speech: narration through raea's API, about a cent a
        line) and kept as a sound file of the project's, media/speech/ by the words and their hash, so the same words
        read again are the kept file and cost nothing
      And the voice is a sound clip on a sound track from where the text starts, as long as the voice, named by the
        words, as a file from Media is (TL3j); in a composition it is the host's edit, as that is
      And what goes wrong (no words, too many, no credit) is said beside the button, and nothing is added

  Rule: TL8. Sound
    Scenario: TL8a. The waveform, the volume line and fades
      Then a sound clip draws its waveform; a line across it is its volume, dragged up and down (Cmd+. and Cmd+,
        0.1 dB); a handle at each top corner drags a fade in and a fade out, drawn as the waveform's slope
      And volume takes keys as TL5 says

    Scenario: TL8b. Beats
      When I ask for a sound clip's beats
      Then they are marked along it (the bed's beat map where raea made the music, else found from the sound), each
        can be turned off, and edges snap to them (TL4b)

    Scenario: TL8c. A voice recorded onto the timeline
      When I press record and speak
      Then the take is a sound clip from the playhead on a sound track, and a file in the project

    Scenario: TL8d. A sound's noise reduced and its voice changed
      Given a sound clip picked in a film cut here (a recording, a sound file of the project's, or the film's own
        sound taken out of its footage)
      When I press reduce noise, or pick a voice (CapCut's voice changer: chipmunk, deep, robot, telephone,
        megaphone, echo), or both
      Then the host makes a copy of the clip's own sound, never of a copy, with that done to it (its steady noise
        found in its quietest tenth and turned down per frequency; the voice by a phase vocoder, a filter or a delay),
        mono and as long as its own, kept under the project's media/voice by its file, size, time and settings, so
        asking again is the kept file
      And the clip plays the copy from where it played its own, and says so with a mark on it
      And both turned off, it plays its own sound again
      And what goes wrong (no sound here, longer than 20 minutes) is said beside the controls, and nothing changes
      And the agent is told what was done and the file it now plays (TL9b)

  Rule: TL9. An edit is an edit, and the agent gets it as one
    Scenario: TL9a. Applied at once, to the working version
      When I make any operation above
      Then the working version's code is changed by it (docs/editing.feature E1a) and the picture shows it (TL10)
      And nothing asks me to confirm, and nothing is held as a suggestion

    Scenario: TL9b. Sent to the agent as what was done
      Given the item's agent has a session (or is started for it, as a note's send does)
      When my sitting's edits are sent (when I leave the editor, press Send, or pause a while)
      Then the agent receives them as edits already made: the operations in order, each with the clip's element, the
        frames before and after, and the file and lines changed, with the version they made
      And its prompt says they are done and not to be redone or undone, and what is its to do next (render, check,
        carry the change to what depends on it)

    Scenario: TL9c. The agent's edits are operations too
      When an agent changes the timeline (editor_edit, or the SDK's operations from its own code)
      Then they land in the open sitting's history as mine do, and I see each one happen

    Scenario: TL9d. A timeline with no composition behind it
      Given a feed item or a controller whose timeline is its own data (TL0d)
      Then an operation changes that data where it is kept and is sent to its agent the same way

  Rule: TL10. Playing while editing: buffer, cache, and clearing it
    Scenario: TL10a. The engine plays the working file
      Given a stage (the raea app)
      Then the picture is the engine drawing the working version at the playhead, for 2D, 3D and ray-traced scenes
      And a change shows on the next frame; no render stands between an edit and the picture (E4a)

    Scenario: TL10b. Frames drawn are kept, by what draws them
      Then a range of frames the stage has drawn is kept (in memory near the playhead, on disk beyond it) under a key
        of everything that draws it: the clips over it, their props and keys, the media's own fingerprints
      And playing a kept range shows its frames without drawing them again, at the film's own rate
      And a bar over the ruler shows which ranges are kept and which are being drawn

    Scenario: TL10c. An edit drops only what it changed
      When I move a clip from 2:00-2:05 to 2:10-2:15
      Then the kept frames of 2:00-2:05 and 2:10-2:15 are dropped and every other range plays from the store

    Scenario: TL10d. Buffering ahead
      Given a part of the film that can't be drawn at its rate (a ray-traced scene, a 4K clip)
      Then while paused, and while playing ahead of the playhead, the stage draws and keeps what comes next
      And heavy media plays from its light copy while editing (the proxy store), and an export reads the originals
      And when playing reaches a frame that isn't ready, the picture holds and says "Buffering" until 1 s is ready

    Scenario: TL10e. The store keeps itself small
      Then the kept frames live in the proxy store, under its one size limit, shown in Settings with what holds it
      And the least recently played ranges go first; a version that was closed or discarded goes at once; nothing a
        person made by hand is ever in it
      And clearing it by hand costs only time: everything in it is made again when needed

    Scenario: TL10f. Whole sequences and scenes
      Then a Scene item's animation and a film's 3D shots play, scrub and cache the same way
      And a sequence of several compositions plays through its cuts without a gap or a reload

  Rule: TL11. The framework: every part can be added to
    Scenario: TL11a. Contribution points
      Then a plugin (plugins/manifest.ts) can add: a kind of track, a kind of clip and how it draws on the strip, a
        property and its editor in the inspector, an easing preset, a speed preset, a transition, an effect, a lane,
        a tool in the bar with its key, and a snapping target
      And raea's own tracks, clips, easings and transitions are registered through the same points

    @done
    Scenario: TL11b. The SDK alone
      # Status 11-13: built. The model is one import, `raea/editor` (sdk/raea/src/editor.ts re-exports sdk/timeline;
      # raea/timeline was already the video Sequence module), resolved by a bundle as @raea/timeline. Its reference page
      # is docs/manual/reference/sdk/editor.md, every type and field documented (the manual-ref gate fails on one
      # without), with docs/manual/examples/edits.tsx as its example: a timeline built in code, split, keyed, sped up
      # and undone by the inverses, rendered (180 frames to 150, then "as it was"). The skills' SDK reference lists it
      # (tools/skills.sh now follows export *, raea-skills #65) and motion/footage teaches it (raea-skills #64)
      Then sdk/timeline has no React and no DOM: an agent's script, the host and a test use it as the app does
      And its reference page lists every type and operation with an example, in the skills reference

  Rule: TL12. Comments leave the timeline
    Scenario: TL12a. No comment is made on the timeline
      Then a press on the tracks never opens a comment card, with any tool on, and the strip's menu has no Comment item
      And Comment in the viewer pins a comment on the picture at the playhead's time, as it does now

    Scenario: TL12b. A comment shows as a marker
      Then each comment with a time shows as a marker on the ruler in its author's hue; a click opens it in the viewer

  Rule: TL13. Feel
    Scenario: TL13a. Under the pointer
      Then a drag, a trim and a scrub draw at the window's rate with 400 clips on 12 tracks, the dragged clip never
        more than a frame behind the pointer
      And a pick shows within 50 ms, a split within 100 ms, and the picture follows an edit within 100 ms where the
        stage plays it

    Scenario: TL13b. Measured, as 0.1.4's were
      Then each number above has a harness and a before and after in this file, in a headless window
      # 11-13, raea-feed e2e/timeline-perf.spec.ts (PERF=1), headless Chrome on the M5 Pro, an item's own timeline
      # of 400 clips on 12 tracks, three runs:
      #   a pick drawn        9, 10, 13 ms            (the bar: 50 ms)
      #   a split drawn       24, 26, 27 ms           (the bar: 100 ms; 12 clips cut at once)
      #   a 2 s drag          63 to 65 frames a second; a frame's median 14.2 to 14.8 ms, its 95th centile 27 ms,
      #                       its worst 34 to 49 ms: one frame in twenty is a frame late
      # The same in a headless raea window (blitz/pw-run.mjs, RAEA_PAGE_FPS=120), after each clip's and each
      # track's drawing was kept while nothing it shows changes (Chrome's drag went to 68 to 71 frames a second):
      #   a pick drawn        45 to 59 ms             (at the bar, not under it)
      #   a split drawn       59 to 64 ms
      #   a 2 s drag          27 to 31 frames a second, a frame's median 31 to 36 ms: not the window's rate
      #   nothing moving      63 frames a second; the pointer moving over it, 61
      # and with 48 clips in place of 400: nothing moving 106 frames a second, a drag 56, a pick 39 ms. So in the
      # window both scale with the clips on screen: what the window draws each frame (16 ms for 400 clips, 9 for
      # 48) and what a drag step adds to it (about 18 ms, 8 with 48). Keeping the clips' drawings did not move the
      # window's numbers, so the step's cost there is not React's work on the clips: it is not yet told apart
      # between the operation itself (snapping and settling 400 clips) and the window laying the moved clips out.
      # Told apart since (the measurement times a step's operation and React's commit of it): a drag step asked
      # the page what was under the pointer (document.elementFromPoint, for the track a clip is carried to), which
      # in raea's window took 7.7 ms a step (0.2 ms in Chrome). The tracks' places are now read once when a clip
      # is first carried: the operation is 0.1 ms a step, its commit 2.5 ms, and the window's drag 35 to 37 frames
      # a second (a frame's median 24 to 26 ms), with nothing moving 80. What is left is the window's: a frame
      # after any change to the page costs about twice a still one with 400 clips on screen (a clip placed by a
      # transform instead of its left edge made no difference), so the rest is the engine's layout, not the app's.
      # Not yet: that, and the picture following an edit within 100 ms on the stage.

  Rule: TL14. Watch, Listen and Scene are one timeline
    Scenario: TL14a. Watch
      Then a film in the feed opens with the timeline under it: the render's own reference (frames, waveform,
        words) when it has no composition, the composition's tracks when it has, edited in place (no separate editor)

    Scenario: TL14b. Listen
      Then a sound in the feed has the same timeline: its stems as sound tracks, notes and parts as clips where
        raea made the music, with TL8's tools

    Scenario: TL14c. Scene
      Then a 3D scene in the feed has the same timeline: its animation clips, cameras' cuts and keyed props as
        tracks, the stage as its picture, with TL5's keys on any animatable prop
