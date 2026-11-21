@spec
Feature: Hardware: controls that feel like instruments, heard on raea's own mixer
  Shane, 2026-11-14, pointing at SPEC-01 (spec-01.vercel.app, a shadcn registry of controls drawn as hardware, its
  sounds made with the browser's Web Audio): "i want to have this ui kit installed across the app where relevant with
  sounds especially for controllers ... wrap the app in <Hardware> ... and have a sound off button in settings. this
  could be used for so many things and feels aligned with our vibe and brand." Then, on how its sounds would play in
  raea's window: "why web audio isnt it in a raea engine its rendering wouldnt we use raea tone.js/firewheel?" And:
  "we would make our own version of it no need to use theirs for real unless it makes sense. we keep dark mode."

  So: raea's own kit, dark, in the idiom SPEC-01 shows (keys that go down and come back, latches, detents, knobs,
  faders, readouts), with its sounds written in raea/tone and played by raea's mixer on Firewheel. None of SPEC-01's
  code is taken; it is credited as the inspiration (Apache-2.0, Vercel Labs' ui-sdk).

  What there is to build on (read 11-14):
    - raea's window has no Web Audio graph on purpose (sdk/ui/src/platform/audio.ts: decodeAudioData only, AudioContext
      not installed, "so a page that looks for it finds it missing rather than half there"). A web kit's sounds are
      silent there.
    - raea/tone (sdk/audio/src/tone) is Tone.js's API on raea's one mixer core: instruments, effects and buses (one of
      them `ui`) as a graph (`<audio data-tone-graph>`) and events (`<audio data-tone-evs>`). Today only a composition
      reaches the mixer: a render bounces it (systems/mix offline.rs) and the stage plays it live (systems/mix live.rs,
      `Live::sync` a frame at a time). A page's own interactions have no way in.
    - The page's host calls (raea-runtime-js HostCalls; the stage's in engine/page/stage calls.rs), reached from a page
      as `__raea_engine.call(name, args)`.

  Status, 11-14: HW0 to HW4 built (engine lane `hardware`; the app's raea-feed branch `hardware`, src/hardware). HW0b
  measured: 200 presses 15 ms apart through Live::fire, 0 late, 200 in the tap (systems/mix tests/fire.rs, ignored as
  it opens a device); in raea's window, 40 presses on the kit's sheet fired 40 on the page's mixer, 0 late (raea-feed
  blitz/kit.sh, headless and muted). HW1b: the loudest voice, the primary key, peaks at -22 dB (raea-feed
  bench/kit-voices.sh). HW2b: the sheet (#/kit, ui-parity screen 27-kit) drawn by raea's window matches Chrome's by eye,
  and the switches' thumbs measure centred, 2 px all round, in both (Shane 11-14: "slightly off center": a thumb has no
  skirt, and its groove is shaded evenly, as a well's lit lip read a pixel low); the ui-parity run is still to do (it
  reads the app while it still shows its loading shapes). Shane 11-14: into 0.1.4. HW5 open.

  Background:
    Given the engine's part is systems/mix (the mixer) and engine/page/sound (the page's call), the SDK's is
      sdk/audio/src/tone (fire), and the app's is raea-feed src/hardware

  Rule: HW0. A page plays a sound now, on raea's mixer
    # Plan (crate or package | file | lines | job)
    #   raea-sys-mix    | src/live/fire.rs    | 80  | Live::fire: a graph built when it changes, events at the clock's now
    #   raea-page-sound | src/lib.rs          | 120 | the page's `sound.fire` call: a mixer of its own on a thread, resting when idle
    #   @raea/audio     | src/tone/fire.ts    | 80  | fire(play): a page's one-shot, as <Sfx> is a game's, sent to the call

    Scenario: HW0a. sound.fire
      Given a page in raea's window (the app, a plugin's page, any page the page host shows)
      When it calls sound.fire with a raea/tone graph and a list of events
      Then the graph is built on the page's own mixer when it is new or changed, and each event plays at the clock's
        now plus the mixer's head plus the event's own time: no frame and no timeline stand in between
      And the call answers at once; it never waits for the sound
      And RAEA_AUDIO_MUTE=1 silences it as it silences every mixer

    Scenario: HW0b. Heard within a frame of the press
      Then a fired event reaches the speaker within the mixer's head (two 256-sample blocks and 5 ms: 15.7 ms at
        48 kHz) of the call, measured on the mixer's own clock (the late-event count stays 0 over 200 fires)

    Scenario: HW0c. Nothing runs while nothing sounds
      Then the page's mixer opens the device on its first fire, and stops its stream after 10 s with nothing fired
        (the stage's rest, lightspeed R6); the next fire starts it again

    Scenario: HW0d. fire() in raea/tone
      Then `fire(play)` runs `play(time)` the way <Sfx> runs a game's one-shot: its triggers become events, keyed so
        no two fires share one, sent with the graph (only when the graph changed since the last fire) to sound.fire
      And with no raea engine under the page (a browser) it returns without a sound, and says so once in the console

  Rule: HW1. The kit's voices, in raea/tone
    Scenario: HW1a. A voice for each kind of touch
      Then a key going down thocks and coming up clicks lighter, a primary key deeper; a latch catches going down and
        springs going up; a detent (a knob's step, a switch's position, a wheel's row) ticks; anything on a screen (a
        readout, a list row) blips; a popup opens with a soft pop; typing clicks, deeper for space and heavier for
        Enter and Backspace; an outcome chimes (done, warning, error)
      And each is written with raea/tone's instruments and effects on the `ui` bus, never an audio file
      And no two presses sound quite alike: each varies its pitch and level a little, by raea/tone's seeded variation

    Scenario: HW1b. Quiet by default
      Then the kit's loudest voice peaks at least 18 dB under full scale on the ui bus, under any music or voice a
        stage plays at the same time

  Rule: HW2. The kit, dark, drawn by raea's window
    Scenario: HW2a. The controls
      Then the kit has a key (a button), a latch (a key that stays down), a switch and a slide switch, a knob with
        detents, a fader, a segmented selector, a stepper with its readout, a readout (an LCD-like screen), and a lamp
      And each is drawn on the app's dark palette (its ground, its raised greys, its ink and its accent), as anodised
        panels, keycaps and screens; there is no light skin
      And each works by pointer and by keyboard (arrows step a knob, a fader, a selector; Space and Enter press a key),
        with a focus ring shown only for the keyboard
      And each plays its HW1 voice as it moves, and nothing when sound is off (HW4)

    Scenario: HW2b. Drawn the same in raea's window and in Chrome
      Then every control is drawn with what raea's window paints (no `<input type=range>`, whose thumb the window doesn't
        move; no SVG for anything a pointer presses; a drag followed by window pointer listeners), and a screenshot of
        the kit's sheet in a headless raea window matches Chrome's within the ui-parity bar

    Scenario: HW2c. <Hardware>
      Then the app sits in one <Hardware> root, which installs the kit's sounds and keyboard-only focus once and changes
        nothing else about how the app looks

  Rule: HW3. Where the kit is used
    Scenario: HW3a. Controllers
      # 11-20 (projects.feature PJ9): controllers are gone, so this has no page to land on; the kit's controls are
      # Settings' (HW3b) and the editor's.
      Then a controller's fields are the kit's: a toggle is a switch, a choice a segmented selector (a dial past five
        options), a number a knob or a fader with its readout, and each change sounds as it is made

    Scenario: HW3b. Settings
      Then Settings' toggles and choices are the kit's switches and selectors

    Scenario: HW3c. The rest of the app, where a control is a control
      Then the Send keycap, the timeline's sliders and the player's transport use the kit's keys, faders and their
        voices; lists, menus and text stay the app's own

  Rule: HW4. Sound off
    Scenario: HW4a. Settings › General › Interface sounds
      Then one switch turns every interface sound off and on, at once, remembered on this Mac
      And it is on by default
      And it never silences a film, a sound or a game: only the interface

  Rule: HW5. Open
    Scenario: HW5a. Felt on a Mac's trackpad
      Then a knob's detent, a switch's catch and a fader's end are felt on a Force Touch trackpad (the system's haptic
        feedback, through a host call), as they are heard

    Scenario: HW5b. A browser hears them too
      Then the kit's voices are bounced by raea once into small files a browser plays, so the app's page in Chrome (the
        tutorials' stage on raea.io) clicks as raea's window does
