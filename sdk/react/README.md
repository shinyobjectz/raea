# react

`raea-react` and its CLI, `raea-motion`: the engine that runs [`@raea/motion`](../motion) compositions. React runs on
JavaScriptCore inside Bevy; each frame's tree is read back and lowered to [`rdb`](../../engine/rdb) rows, which Bevy draws.

    raea-motion render EDIT.tsx -o OUT.mp4 | OUT.png | DIR   [--frame N | --at A,B]
    raea-motion play EDIT.tsx           play live in a window (games take keyboard and pointer input)
    raea-motion run SCRIPT.ts           run a script's default export inside the engine
    raea-motion test FILE...            run bun:test-style files inside the engine
    raea-motion types -o FILE.ts        write the native calls' TypeScript types
    raea-motion shell ROOT -c CMD       run a command in the agent's shell

- `src/`: the engine (`motion.rs`, `live.rs`, `play.rs`, `audio.rs`, ...) and `bin/raea-motion.rs`.
- `js/`: the JavaScript side of the bridge (bundles are made by raea-bundle's library: Rolldown, in process).
- `ios/`: the same engine on the iPhone, as a library a probe app links.
- `bench/`: example games (`games/`), a render-vs-play corpus (`corpus/`), timeline checks and a playtest prototype.

Build with `cargo build --release -p raea-react --bin raea-motion` from the repository root. Depends on [`engine/html/`](../../engine/html),
[`engine/rdb/`](../../engine/rdb), [`engine/media/`](../../engine/media), [`engine/native/`](../../engine/native), [`engine/modules/`](../../engine/modules), and optionally
[`engine/browser/`](../../engine/browser) and the sandbox's `wsx`.

Status: in use; the main way to render with raea.
