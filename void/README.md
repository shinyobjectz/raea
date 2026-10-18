# void

void, the Mac app built on raea: a black window you talk to. Describe a reel, a promo, a deck or a game; its agent
writes the composition as a React + TypeScript project, renders it, checks the frames and the sound, and plays it on
its stage.

- The window is a black canvas with a stage and a terminal, drawn by raea's own engine through Dioxus Native (no
  webview). The terminal is a real terminal emulator (alacritty_terminal) showing void's own lines.
- void works in a folder you pick. Each thing it makes is an object in its own folder; chats are sessions.
- The agent (`src/brain.rs`) works with a shell over the project folder ([`shell/`](../shell), with the `raea` command
  from [`cli/`](../cli)) and a tool to look at stills. Its procedure is `prompts/sops/ts.md` and its craft library is
  `prompts/craft/`. The model is set in `~/.raea/env`; see [docs/settings.md](../docs/settings.md).
- Voice (`src/voice`): voice activity, end-of-turn detection, speech-to-text and text-to-speech, four models on ONNX
  Runtime, downloaded on first use.
- An MCP server on `127.0.0.1:7457` (`src/mcp.rs`): `state`, `screenshot`, `send`, `act`, `wait`, `logs`, `hear`,
  `say`, so another agent can drive it.
- `void eval` (`src/eval.rs`) runs the agent headless on the Gherkin scenarios in `evals/` and grades the renders.

Other folders: `parts/` (React parts for edits and the brand-kit writer, `brand.ts`), `brand/` (logos), `assets/` and `tw/` (styles), `native/` (an early
spike of the window on Blitz), `vendor/dioxus-native` (a vendored copy).

Build with `cargo build --release` in this folder and run `./target/release/void`.

Status: canary; changes daily.
