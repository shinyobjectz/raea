---
title: "raea godot"
summary: "Run a TypeScript script against Godot's whole API in a headless Godot with GodotJS, record what it draws as frames, or open the editor on the project."
order: 109
sources: ["apps/engines/src/godot.rs", "apps/engines/src/godot_run.rs"]
---

`raea godot run` bundles SCRIPT.ts and runs it in the Godot that [`raea setup godot`](setup.md) downloaded: Godot
4.6.1 with GodotJS, its TypeScript bindings. The script imports Godot's whole API, typed, from `raea/godot`. Its default
export is either a function given the SceneTree and ARGS, or a `SceneTree` subclass that is Godot's main loop. Godot
quits with 0 when the function returns, and with 1 when it or a callback throws.

Godot runs headless unless `--window` is given. `--movie FILE` opens Godot's window too, since a headless Godot draws
nothing, and records every frame it draws at a fixed `--fps`: `NAME.png` writes numbered PNGs, `NAME.avi` one video.
`raea godot types` writes the project's own typings, and `raea godot open` opens GodotJS's editor on the project for the
person. The guide is `raea guide bevy/godot`.

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea setup godot [--check]   (download Godot 4.6.1 with GodotJS, its TypeScript bindings (MIT), into ~/.raea/tools/godot: JavaScriptCore on the Mac, QuickJS on Windows and Linux; raea's own build (the Mac's notarized), else GodotJS's, each checked against its pinned sha256)
raea godot run [SCRIPT.ts] [--project DIR] [--window] [--movie FILE [--fps N] [--frames N]] [--scene res://X.tscn] [--size WxH] [--timeout S] [ARGS...]   (a TypeScript script against Godot's whole API through raea/godot: its default export gets the SceneTree and ARGS, headless unless --window; --movie writes every frame Godot draws (NAME.png numbered, or .avi) at a fixed --fps, 30 unless given; the project is DIR, else the nearest project.godot above the script, else an empty one; Godot quits when it returns, or after --frames N; --scene plays a scene of the project under it, with no script needed; a scene in --window plays until the window is closed)
raea godot types [DIR]   (the project's own TypeScript types from GodotJS: Godot's API with its scenes, resources and input actions, into DIR/typings)
raea godot open [DIR]   (the Godot editor on the project, GodotJS's, to edit its scenes)
```

Headless: it needs nothing running and opens no window. Its arguments go to a parser of its own, whose forms are listed above.

<!-- generated:end -->
