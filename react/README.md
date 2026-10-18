# react

`raea-react` and its CLI, `raea-motion`: the engine that runs [`@raea/motion`](../motion) compositions. React runs on
JavaScriptCore inside Bevy; each frame's tree is read back and lowered to [`rdb`](../rdb) rows, which Bevy draws.

    raea-motion render EDIT.tsx -o OUT.mp4 | OUT.png | DIR   [--frame N | --at A,B]
    raea-motion play EDIT.tsx           play live in a window (games take keyboard and pointer input)
    raea-motion run SCRIPT.ts           run a script's default export inside the engine
    raea-motion test FILE...            run bun:test-style files inside the engine
    raea-motion types -o FILE.ts        write the native calls' TypeScript types
    raea-motion shell ROOT -c CMD       run a command in the agent's shell

- `src/`: the engine (`motion.rs`, `live.rs`, `play.rs`, `audio.rs`, ...) and `bin/raea-motion.rs`.
- `js/`: the bundler (`bundle.ts`, run with bun on the Mac) and the JavaScript side of the bridge.
- `ios/`: the same engine on the iPhone, as a library a probe app links.
- `bench/`: example games (`games/`), a render-vs-play corpus (`corpus/`), timeline checks and a playtest prototype.

Build with `cargo build --release --bin raea-motion` in this folder. Depends on [`engine/`](../engine),
[`rdb/`](../rdb), [`media/`](../media), [`native/`](../native), [`modules/`](../modules), and optionally
[`browser/`](../browser) and the sandbox's `wsx`.

Status: in use; the main way to render with raea.
