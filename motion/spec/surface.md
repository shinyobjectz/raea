# The surface M3 writes (2026-10-16)

How measured: MiniMax M3 (the agent's model, OpenRouter, no tools) wrote one complete composition in the familiar
React-video style for each of 50 of our own eval briefs, picked by medium (ugc 18, motion 10, deck 7, game 6, scene3d
5, app 2, doc 2), seed 7, $0.08. The counts are files using each API. The 50 outputs themselves are not kept in the
repo, so they never reach the SDK's builders.

## Build order (the first slice covers rows 1-13: everything used in more than one file)

| # | API | files | props and options M3 used (count) |
|---|---|---|---|
| 1 | `AbsoluteFill` | 50 | style (141) |
| 2 | `useCurrentFrame` | 50 | |
| 3 | `interpolate` | 40 | extrapolateRight (139), extrapolateLeft (55), easing (23) |
| 4 | `Sequence` | 35 | durationInFrames (60), from (59), name (1) |
| 5 | `useVideoConfig` | 34 | |
| 6 | `spring` | 26 | config (41): damping (41), stiffness (38), mass (27); frame (25), fps (6), from/to (3), durationInFrames (3), overshootClamping (1) |
| 7 | `staticFile` | 20 | |
| 8 | `Video` | 17 | src (20), style (19), startFrom (2), endAt (1), volume (1) |
| 9 | `Composition` | 14 | id, component, durationInFrames, fps, width, height (11 each) |
| 10 | `Audio` | 11 | src (3), volume (2), startFrom (2) |
| 11 | `registerRoot` | 10 | |
| 12 | `Img` | 8 | |
| 13 | `Easing` | 8 | cubic (17), out (13), inOut (7), bezier (2), back, exp, quad, in, sin (1 each) |

## The long tail (one file each; wait until it shows up again)

Series, OffthreadVideo, Still, delayRender/continueRender, useFont, a player component. Series, Loop and Freeze are in
the conformance suite anyway because the user named them.

## 3D: give M3 the shapes it reached for

The 5 scene3d briefs had M3 import three.js-style components (`Mesh`, `Box`, `Plane`, `AmbientLight`,
`DirectionalLight`, `PerspectiveCamera`, `MeshStandardMaterial` with color, emissive, metalness, roughness) and a 3D
canvas, from the wrong package. So `@raea/motion/3d` offers those shapes (geometry `args`, `position`, `rotation`,
standard material props, lights, a camera), drawn by Bevy, with Manifold for modelled geometry. That's point 6 of the
README, chosen by the data.

## Checked against the build (2026-10-16, @raea/motion 0.1.1)

The same 50 compositions, with only their imports pointed at @raea/motion, each loaded and rendered at its first,
middle and last frame through the test clock:

| result | count | what |
|---|---|---|
| render unchanged | 40 | |
| cut off by the survey's 4000-token limit | 4 | not code M3 finished; excluded |
| the SDK's gap, now closed in the spec | 0 | (4 were roots with no Composition; fixed in 0.1.1, now among the 40) |
| the renderer's bundler | 1 | imports an .mp4 as a module and uses it as a src: the Bevy bridge's bundle step should resolve media imports to URLs (an agent session) |
| the long tail | 1 | a player component (not a render API) |
| M3's own mistakes | 4 | an element used as a component, two packages that don't exist, an import of a file it never wrote |

So 40 of the 46 finished compositions (87%) run as written, and 40 of the 41 that are valid code using real APIs
(98%); the one left is the media import, which is the bundler's to resolve.
