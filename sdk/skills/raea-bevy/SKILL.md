---
name: raea-bevy
description: Writing 3D scenes and 3D games in raea's TypeScript SDK (@raea/motion/3d), which Bevy 0.19 draws, and extending that SDK. Use when writing or fixing TSX with Canvas3D, Mesh, lights, cameras, materials, Model (glTF), ImagePlane, Bloom/Fog/ToneMapping or an ECS world drawn with Entities; when a 3D render or play looks wrong; or when adding a Bevy feature to the SDK (the TSX component, its rdb rows, the engine system).
---

# raea-bevy: Bevy through @raea/motion/3d

Agents write TSX; raea compiles each frame's 3D elements to rdb rows and its engine draws them with Bevy 0.19.1. You
never write Rust or shaders to make a scene, and you never use Bevy's API directly from TSX. A render and a live play
draw the same rows with the same code, so they match whenever every value comes from the frame.

This is the same library void's agent reads with `raea craft bevy/NAME` (`apps/void/prompts/craft/bevy.md` routes it);
the topic files here are links to those.

## Which file to open

| You are... | Open |
|---|---|
| setting up the Canvas3D, camera, camera moves, groups, 2D over 3D | `scene.md` |
| lighting: sun, bulbs, spots, ambient, shadows, brightness | `lights.md` |
| surfaces: colour, metal, glass, glow, clearcoat, picture maps, toon (cel) shading and ink outlines (`MeshToonMaterial`), decals (`Decal`), a model's materials re-covered (`Refinish`) | `materials.md` |
| shapes: primitives, Manifold solids, Text3D | `meshes.md` |
| interiors: rooms, runs of units, windows and bays, seats turned to tables (`faceToward`), layout checks, wall decor (`hang`), the garden outside, daylight, 3D sound (`Sound3D`) | `rooms.md` |
| motion: spins, springs, Sequences, model clips | `animation.md` |
| a 3D game: state, input, entities, collisions, state machines | `games.md` |
| a game's sound: one-shots on transitions (`<Tone.Sfx>`), buses, a bed ducked under a voice (Tone.js on raea) | `games.md` (Sound), then `raea craft motion/music` |
| glTF models, pictures in space, media files | `assets.md` |
| gaussian splats: a real scene or object trained from footage (`raea-motion splat train`), `<Splat>` and its reveal, assemble, disperse, explode, wave, twist, morph, pulse | `splats.md` |
| liquids and goo: coffee dumped from a mug, a tap's stream, honey, things melting (`<Goo>`, sph or mpm, baked) | `liquids.md` |
| bloom, fog, tone mapping, exposure, fisheye and lens distortion, motion blur, fire (`<Fire>`), frame looks (pixel, ink, painted illustration `<Look mode="paint">`, cartoon `<Look mode="cartoon">`) | `post.md` |
| budgets, render-equals-play rules, what isn't there yet | `limits.md` |
| finding free models, rigs, animations, Bevy examples' TSX, game-design patterns (`raea-meaning games-find`) | `library.md` |
| adding a Bevy feature to the SDK, or debugging the rows | `rows.md` |

## The rules that keep a render equal to its play

1. Every value from the frame (`useCurrentFrame()`, `interpolate`, `spring`, a game's state at the frame). No
   `Math.random()` unless seeded, no `Date`, `useState`, `useEffect`, refs or `useFrame`.
2. Rotation in radians on Mesh, Group, Model, ImagePlane; Manifold's `.rotate()` in degrees.
3. Colours are hex strings (CSS variables don't reach the 3D).
4. Name parts that come and go (`<Mesh name="...">`); parts are matched by name, else kind and shape in tree order.
5. A part's shape is fixed; animate position, rotation, scale, colour, opacity, glow, roughness, metalness.
6. One camera (the first), one set of effects (the first frame's; the lens and MotionBlur animate per frame).
7. Import every component you use from `@raea/motion/3d` (hooks, `Sequence`, `staticFile` from `@raea/motion`).
8. Render and look: `raea-motion render EDIT.tsx --frame N -o F.png`.

## Checks in the raea repo

- `sdk/motion/bench/bevy/check.sh [EDIT.tsx]`: render against baked play with 3D (SSIM >= 0.999) on the fixtures.
- `sdk/motion/bench/bevy/skills.sh`: every example in this library renders.
- `sdk/motion/docs/bevy-coverage.md`: the full coverage matrix and the proposed API for what's missing.
- `sdk/motion/docs/game-library.md`: the game library (catalogue, index, `games-find` / `games-fetch`, licences, sizes).
