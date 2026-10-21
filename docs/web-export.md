# Games on the web: WebAssembly and WebGPU

A raea game, played in a browser tab: the same edit.tsx, the same fold, the same rows, drawn by the same engine.

## The shape of it

On the Mac a game runs as React inside JavaScriptCore, its page painted by Blitz and its 3D drawn by rdb's engine on
Bevy, both on the GPU. In a browser most of that is already there:

- **React runs in the browser's own JavaScript engine.** The game's bundle is a normal React app; its page (titles,
  keycaps, captions in the DOM) is real DOM, laid out and painted by the browser.
- **The 3D is rdb's engine, compiled to WebAssembly, on WebGPU.** It draws into a canvas behind the page. Bevy and wgpu
  both target wasm32 and WebGPU.
- **The 3D gets each frame's scene the way live play gives it** (sdk/react/src/motion/scene3d.rs LiveScene): the frame's
  3D parts, read from the page (every `<div data-3d>` the SDK's components render), turned into rows, and only what
  changed set on the engine (Engine::set_rows, remove_rows, draw).
- **Sound is Web Audio.** @raea/motion/tone is Tone.js's API already: in the browser it is Tone.js, with the instruments
  and foley the export generated (TokenSynth, SAO) as files beside the page.
- **The narrator is pre-spoken.** Pocket TTS in a tab is a download of hundreds of megabytes. The export says every
  line the game can say (each line from each place it can be resumed after an interruption, each aside at each of its
  takes) and ships them with their word timings, so `speechAt` answers from the file.

What stays native-only: Blitz and Stylo (the page is the browser's), the media stack (decode, encode, VideoToolbox),
the mixer, Core ML and ONNX Runtime, SQLite and the native calls, the GPU kernels and the readback.

## Milestones

1. **rdb's 3D engine builds for wasm32.** A `native` feature (on by default) carries the page engine (raea), media,
   mixer, kgpu, gputrace and everything that only exists to render or analyse on the Mac; without it, rdb is the
   relational core and the 3D engine (rows, timeline, scene3d, materials, models, skinning, fog, outlines).
   Done when `cargo check -p rdb --target wasm32-unknown-unknown --no-default-features --features web` passes.
2. **raea-web**: a wasm-bindgen crate opening Engine::new_live on a WebGPU canvas, with open / setRows / removeRows /
   draw from JavaScript, and LiveScene (frame items to rows) moved where both raea-react and raea-web can use it.
3. **The browser runtime of @raea/motion**: the clock (a fixed 60 steps a second off requestAnimationFrame, so a fold
   steps as it does natively), useGameState, useInput (keys, the pointer), staticFile (the export's asset URLs),
   Composition and registerRoot, and the 3D components' frame collected from the DOM each step.
4. **Sound and speech**: @raea/motion/tone as Tone.js with the exported samples; @raea/motion/speech from the
   pre-spoken lines and their timings.
5. **`raea-motion export-web EDIT.tsx -o DIR`**: index.html, the bundle, raea_web.wasm and the assets, ready for any
   static host. The Hero's Journey at raea.io/play.

## Notes

- Use rustup's toolchain for wasm (`~/.cargo/bin/cargo`): Homebrew's rustc comes first on PATH here and has no
  wasm32 standard library.
- A browser gives WebGPU on recent Chrome, Edge and Safari; the export should say so on a browser without it rather
  than fall back to a slower path.
