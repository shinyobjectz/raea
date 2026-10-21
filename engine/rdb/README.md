# rdb

`rdb`: an edit as typed tables, run on Bevy's World, and the renderer that draws it. `@raea/motion` compositions are
lowered to rdb's rows by [`sdk/react/`](../../sdk/react) and rendered here, so a render and a live play draw from the same rows
with the same code.

- `src/engine.rs` and `src/engine/`: the Bevy renderer: 2D layers, 3D scenes (meshes, lights, cameras, glTF models,
  post effects), video frames, captions and sound.
- `src/cli.rs`: the `rdb` command: create and edit an edit with SQL snippets (`new`, `apply`, `undo`, `moves`,
  `query`), `render`, `stills`, `compare`, and native media verbs (`rdb media ...`, plus `rdb ffmpeg` and
  `rdb ffprobe`, which read ffmpeg-style command lines without running ffmpeg).
- `examples/`: small edits and benchmarks.

Depends on [`engine/html/`](../html), [`engine/kgpu/`](../kgpu), [`engine/media/`](../media) and [`engine/gputrace/`](../gputrace). The shell
runs its commands in process.

Status: in use as the renderer behind `raea-motion`.
