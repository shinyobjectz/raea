# Exporting a game

Any game written on `@raea/motion` can be exported as a program that plays on its own. It runs outside void and
outside the dev tree, with no bun, node or V8 and no repo. The export happens on the Mac with the dev tree; what it
makes needs only macOS.

## The command

    raea-motion export-game EDIT.tsx -o OUT [--target mac-app|mac-cli] [--name NAME] [--icon PNG] [--keys SETS]
                                         [--player BIN] [--no-strip]

- `--target mac-app` (the default when OUT ends in `.app`) makes `NAME.app`. `mac-cli` makes a folder holding the
  player and `game/`.
- `--name` is the window title, the app's name and the executable's name (slugged). The default comes from the game's
  folder, so `pong/edit.tsx` becomes "Pong".
- `--icon PNG` is turned into `AppIcon.icns` with `sips` and `iconutil`.
- `--keys` sets the key sets the self-test cycles through (see below). They are stored in the package.
- `--player` names the player binary to package. The default is the newest `raea-game` among the release build and the
  `game` profile build, found beside raea-motion.

Build the player once. It is a separate binary target behind the `game` feature, so the default raea-motion build
doesn't build or link it:

    cd react
    cargo build --profile game --bin raea-game --no-default-features --features game    # fat LTO, no symbols: target/game (73.9 MB)
    cargo build --release      --bin raea-game --no-default-features --features game    # quicker to build: target/release (127.7 MB once stripped)

An export takes 1 to 15 seconds. Most of that is working out the game's sound master (below).

## What an export does

1. **Bundles the game ahead of time** for live play's runtime (`--lite`, the same bundle `raea-motion play` makes at
   open), with the project's `words.json` and `perception.json` baked in. Imported media (`import x from
   "./assets/x.wav"`) is copied into the package's `media/` and named relatively (`bundle_composition`'s portable mode).
   bun runs here only.
2. **Gathers every file the bundle names.** Each string literal in the bundle that resolves to a file is copied in:
   `staticFile` paths (`/media/...`), paths relative to the game's folder, and absolute paths.
   - Files inside the game's folder keep their relative place.
   - Files outside it go to `media/_ext/`, and the literal is rewritten to point there.
   - A glTF brings the buffers and textures it names (`.gltf` and `.glb` JSON `uri`s).
   - The nearest `brand/brand.css` (as `brand_css` finds it) comes along, with the files its `url()`s name.
   - Generated sounds (`sao()`, `make()`) are looked up at play in `assets/made.jsonl`. Each row whose prompt the
     bundle carries comes along with its file, and the index is rewritten to only those rows.
   - Source files (`.ts`, `.tsx`, `.rrl`) and anything the bundle doesn't name are left out. For example, the 50-game's
     `reference/approved.mp4` isn't copied.
3. **Works out what play works out at open**, from the package itself, which also proves the package loads:
   - the composition's size, rate and length;
   - whether it is responsive (`useVideoConfig`);
   - whether it has 3D;
   - whether it has sound;
   - the game's master (the gain and ceiling its render masters its sound with). Play normally works this out in a
     child `raea-motion master-gain` process by stepping the scripted run through every frame. The exported player
     has no raea-motion beside it, so the master is computed once here and stored in `game.json`.
4. **Packages the player.** It copies `raea-game`, runs `strip -x` on it (a no-op for the stripped `game` profile),
   and for an app writes `Info.plist` and `PkgInfo`.
5. **Signs ad hoc** with `codesign --force --sign - --timestamp=none` (the whole app, or the executable for
   `mac-cli`).

## The package

    Pong.app/
      Contents/
        Info.plist          CFBundleName/DisplayName, CFBundleExecutable, ai.raea.game.<name>, CFBundleIconFile when
                            --icon; LSApplicationCategoryType public.app-category.games and GCSupportsGameMode (Game
                            Mode), NSHighResolutionCapable, LSMinimumSystemVersion 13.0
        PkgInfo
        MacOS/Pong          the player (raea-game, renamed)
        Resources/
          AppIcon.icns      with --icon
          game/
            game.json       name, size, fps, frames, responsive, has_3d, sound, master, selftest_keys, assets
            bundle.js       the game for live play's runtime (JavaScriptCore)
            media/ assets/ brand/ ...   the gathered files, at their places relative to the edit
        _CodeSignature/

    mac-cli:  OUT/Pong (the player)  OUT/game/ (as above)

The player finds its game in `../Resources/game`, or in `game/` beside the executable, or in `--game DIR`.

## The player (`raea-game`)

`src/bin/raea-game.rs` calls `raea_react::game::player_main`. It links only what play needs, built with
`--no-default-features` so the agent sandbox (wsx) and the in-process browser are left out. It runs the same `Live` as
`raea-motion play` and void's stage, opened from the prebuilt bundle (`Live::open_bundle`; no bun):

- JavaScriptCore runs the game.
- Blitz lays out the page and the GPU compositor presents it.
- Sound goes through the one mixer core (Firewheel), with the stored master applied before the clock starts.
- 3D goes through rdb's live engine on Bevy, drawn from each frame's own state.

It opens a window shaped like the composition, as large as fits in 85% of the screen. It plays at the composition's
rate, presented every display refresh with vsync. Keys and the pointer are the game's input, and the pointer is mapped
through the letterbox. Window controls:

- F11, Cmd+F or Ctrl+Cmd+F toggles borderless fullscreen.
- Cmd+Q or Cmd+W quits. Keys held with Cmd never reach the game.
- Keys are released when the window loses focus.

    raea-game --selftest 5s [--snapshot S OUT.png]... [--input-log F] [--json F] [--keys SETS] [--no-pointer]

`--selftest` plays offscreen, with no window and nothing on screen:

- **Input.** A thread changes the keys every 50-250 ms, cycling the package's key sets, as `play --input` does. A
  pointer sweeps the page and is pressed half of every second.
- **Sound.** The mixer runs as it would when heard. Its output is silenced and tapped, and the tap's peak and RMS are
  measured.
- **Report.** It prints frames expected, stepped, presented and missed; frame work and gap percentiles; input changes
  that reached a presented frame and their latency; whether sound was mixed (peak and RMS dBFS); the master; panics;
  and the environment it saw (HOME, and how many `RAEA_*` variables it was given).
- **Verdict.** The last line is PASS, or FAIL with the reasons. To pass:
  - at least 90% of the frames are presented and at most 5% + 1 are missed;
  - at least one input change reaches a frame;
  - a game with sound mixes some;
  - every snapshot is taken;
  - nothing panics.

`--input-log` writes each frame's input in the shape `raea-motion render --input` takes, so the dev tree can render
the same frames.

## Checks (sdk/react/bench/games/export.sh)

The harness takes a list of games, defaulting to pong, the platformer, arena3d, `engine/media/golden/50-game` and a void
agent's pong from `sdk/react/bench/corpus/list.txt`. For each game it:

1. exports the game as an app;
2. copies the app to a temp folder outside the repo;
3. runs the self-test there for 5 s with `env -i HOME=<empty dir> PATH=/usr/bin:/bin TMPDIR=<dir>`, under the shared
   measure lock;
4. renders the self-test's snapshot frames (at 1, 2.5 and 4 s) in the dev tree with `raea-motion render --frame N
   --input <the self-test's log>` and compares them with the snapshots by SSIM. A frame passes at >= 0.999.

Results on 2026-10-18, with the `game` profile's player. The load column is the 5-minute load average during the
self-test; other agents were building and rendering at the same time.

| game | export s | app MB | game data MB | presented | missed | input changes reached | sound peak | load | frame: SSIM vs dev render | verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| pong (120 fps, responsive, 8 generated sounds, master +8.1 dB) | 13.6 | 76.5 | 3.0 | 600/600 | 6 | 32 | -2.9 dBFS | 19.3 | 120: 1.000000, 300: 1.000000, 480: 1.000000 | PASS |
| platformer (120 fps, responsive, silent) | 0.3 | 73.6 | 0.09 | 600/600 | 3 | 32 | none | 84.9 | 120: 1.000000, 300: 1.000000, 480: 1.000000 | PASS |
| arena3d (live 3D, 120 fps) | 0.9 | 74.0 | 0.47 | 598/600 | 23 | 30 | -6.0 dBFS | 10.8 | 120: 1.000000, 300: 1.000000, 480: 1.000000 | PASS |
| engine/media/golden/50-game (Breakout, 30 fps, imported media) | 10.7 | 74.5 | 1.01 | 150/150 | 0 | 32 | -2.5 dBFS | 6.0 | 30: 1.000000, 75: 1.000000, 120: 1.000000 | PASS |
| a void agent's pong (corpus list) | 0.5 | 73.9 | 0.38 | 300/300 | 11 | 32 | -6.8 dBFS | 5.6 | 60: 1.000000, 150: 1.000000, 240: 1.000000 | PASS |

Two more checks:

- **A glTF model.** sdk/motion/bench/bevy/character (RobotExpressive.glb through `staticFile`) was exported as `mac-cli`,
  self-tested in the scrubbed environment (PASS), and its frame 30 matched the dev render at SSIM 1.000000.
- **Missed frames.** arena3d's 23 missed frames were the machine's load. Run again at load 22, the same package missed
  0 of 600, and the dev tree's `raea-motion play --offscreen --input` of the same game also missed 0.

## Sizes

The player is the same for every game; a game adds only its bundle (about 90 KB) and its files.

| build | raea-game |
|---|---|
| `--release`, as linked | 177.9 MB |
| `--release` + `strip -x` (what an export does to it) | 127.7 MB |
| `--profile game` with thin LTO | 130.4 MB (3m39s) |
| `--profile game`: fat LTO, `codegen-units = 1`, `panic = "abort"`, symbols stripped (the profile as committed) | **73.9 MB** (8m38s from scratch under load) |
| raea-motion for comparison (`--release`, default features) | 203.7 MB |

An exported app is 73.6-76.5 MB, and Pong.app zips to 34.0 MB. The player links only system frameworks
(JavaScriptCore, Metal, QuartzCore, AppKit, AVFoundation, VideoToolbox, CoreAudio, AudioToolbox, CoreText and the
like); `otool -L` shows nothing else.

With `panic = "abort"`, a panic on any thread ends the game at once instead of unwinding. This drops the unwind tables
(about 17 MB of `__eh_frame` and `__gcc_except_tab`) along with the landing pads in the code. The self-test still
counts panics through its hook before the abort. The `game` profile builds into target/game and touches nothing in
target/release, so raea-motion's build is unchanged.

What's still in the player and could go:

- swc, through raea-modules (`js-exec`, and bake.rs's spans);
- the encoders raea-media brings (rav1e, openh264's encoder);
- sqlparser, through rdb;
- rusqlite, through raea-native.

Each of these is a feature split across crates other agents own.

## Limits

- **macOS only, Apple silicon.** The player is a thin arm64 binary and links JavaScriptCore, VideoToolbox,
  AVFoundation, CoreAudio and Metal from the system; it needs nothing else. A universal binary would need an x86_64
  build of the player, joined with `lipo`.
  - **Windows** would need a JavaScript engine to stand in for JavaScriptCore. The native-call layer already has a V8
    binding, but the goal is no V8; QuickJS is an alternative. It would also need WASAPI through cpal (cpal already
    supports it), wgpu on DX12 or Vulkan, Media Foundation or a software decoder in raea-media, and a zip or
    installer in place of the `.app`.
  - **Linux** would need the same engine choice, ALSA or PipeWire through cpal, Vulkan, software decoding, and an
    AppImage or a tarball.
  - Both would need `export-game --target` to cross-compile the player, or a player built on that OS.
- **Fonts.** Text uses the engine's bundled faces (Inter, Montserrat and the other aliases) and fonts installed on the
  player's Mac. The live page never fetches from the network, in the dev tree or in an export. A Google family outside
  the bundled set, which the dev tree can show from its font cache, falls back in an export unless the game ships the
  font file and names it in `@font-face` (in `brand.css` or its own CSS).
- **Distribution.** An ad hoc signature runs on the Mac that made it, and on another Mac only after the user allows it
  in System Settings. To distribute:
  1. Join the Apple Developer Program and use a Developer ID Application certificate.
  2. Sign with `codesign --force --options runtime --timestamp --entitlements game.entitlements -s "Developer ID
     Application: ..." NAME.app`. JavaScriptCore JITs on macOS, so under the hardened runtime the entitlements need
     `com.apple.security.cs.allow-jit`.
  3. Zip with `ditto -c -k --keepParent NAME.app NAME.zip`.
  4. Notarize with `xcrun notarytool submit NAME.zip --keychain-profile ... --wait`.
  5. Staple with `xcrun stapler staple NAME.app`.

  None of this is done yet.
- **Saving.** A game's state lives in its fold and nothing is saved between runs.
- **Windowed play wasn't tested on screen.** No window was shown while testing, by request. The windowed loop is
  `raea-motion play`'s loop around the same `Live`, and the self-test covers everything below the window.
