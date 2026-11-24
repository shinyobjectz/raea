---
title: "raea wear bake"
summary: "Bake what weathers a model: where rain reaches and runs, what the sun sees, edges, hollows and touch, as maps raea/wear reads."
order: 105
sources: ["runtime/wear/src/lib.rs", "systems/weathering/src"]
---

Weathering in raea has causes. A bake traces them through a `.glb`'s own geometry once: rain and the sky, the sun's
path over the year, edges and hollows, the height over the ground, where hands and feet touch, and where run-off
carries soot, rust or copper and leaves it. It returns the model with a second UV set and four maps over it, which
raea/wear's recipes read. A composition gets the same bake from the native call `wear.bake`; `raea wear bake` makes
it from a shell, in the current folder's asset store. The same request gives the same files, and a second run reads
the first's.

```sh
raea wear bake corner.json -o corner.bake.json
```

A request names the model and, optionally, the world it stands in and the class of each of its materials:

```json
{
  "src": "/media/corner.glb",
  "env": { "density": 8, "walk": ["ground"], "sun": { "lat": 52.5 } },
  "classes": { "facade": "plaster", "plinth": "sandstone", "ground": "ground" }
}
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea wear bake REQUEST.json [-o OUT.json]   (a .glb's weathering causes for raea/wear: the model with a second UV set and its cause maps, made once in the asset store)
```

Headless: it needs nothing running and opens no window. Takes up to 2 arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `-o` | `OUT.json` | Also write the result to this file, in place of printing it. |

<!-- generated:end -->

## See also

- [raea physics](physics.md) makes the other bakes a composition plays.
