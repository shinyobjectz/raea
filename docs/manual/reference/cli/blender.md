---
title: "raea blender"
summary: "Run a Blender Python script headless, with raea's library of mesh scripts: convert, decimate, repair, split, bake, rig, cloth."
order: 108
sources: ["apps/tools/src/blender.rs", "apps/engines/src/blender_ts.rs"]
---

`raea blender run` runs `blender --background --python SCRIPT.py -- ARGS` with the Blender [`raea setup
blender`](setup.md) found. A script named `blender/X.py` that the project doesn't have is copied in from raea's
library first: `convert`, `decimate`, `repair`, `split`, `bake`, `rig` and `cloth`. The copy is the project's own
to change.

A `.ts` script runs on raea's engine against a headless Blender instead, through `raea/blender`: Blender's whole
Python API (bpy, bmesh, mathutils) typed. `FILE.blend` is opened first, the script reads ARGS as Python does, from
`sys.argv` after `--`, and Blender quits when its default export returns. The guide is `raea guide bevy/blender`.

```sh
raea blender run blender/decimate.py -- model.glb 0.3 model-low.glb
raea blender run blender/repair.py -- scan.glb scan-fixed.glb
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea setup blender [--check] [--download]   (find Blender or download the LTS into ~/.raea/tools/blender, and install raea's add-on and the MCP add-on)
raea blender run SCRIPT.py [ARGS...]   (blender --background --python SCRIPT.py -- ARGS, headless; blender/X.py missing from the project is copied from the library: convert, decimate, repair, split, bake, rig, cloth)
raea blender run SCRIPT.ts [FILE.blend] [ARGS...]   (a TypeScript script against Blender's whole Python API through raea/blender: bpy, bmesh, mathutils, typed; a headless Blender, FILE.blend opened, quit when the script ends)
```

Headless: it needs nothing running and opens no window. Its arguments go to a parser of its own, whose forms are listed above.

<!-- generated:end -->
