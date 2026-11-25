---
title: "raea render"
summary: "Render a composition to an MP4, a PNG or a folder of frames, on the GPU and headless."
order: 2
sources: ["engine/cli/src/cli.rs", "runtime/export/src/render.rs", "runtime/bundle/src/lib.rs"]
---

`raea render` bundles a TSX composition, draws it on the GPU without opening a window, and writes a video, one still
or a folder of frames. It draws the same tree the viewer gets in the raea app, so its stills are the truth about
layout. It also renders an HTML composition and a bundle made by [`raea bundle`](bundle.md).

```sh
raea render piece.tsx -o piece.mp4                       # the whole video
raea render piece.tsx --frame 120 -o frame-120.png       # one frame
raea render piece.tsx --at 15,63,144 -o stills/          # some frames, in order: stills/frame-00000.png is frame 15
raea render piece.tsx --alpha -o frames/                 # every frame as a transparent PNG
raea render piece.tsx --alpha -o overlay.mov             # a transparent movie (ProRes 4444 on a Mac)
raea render piece.tsx --quality master -o final.mp4      # constant quality for type and thin lines
raea render scene.tsx --review review/                   # a 3D scene from six fixed cameras, in colour and clay
raea render piece.tsx -o piece.wav                       # the sound alone, no frames drawn
raea render piece.tsx -o piece.mp4 --audio piece.wav     # the video, and its sound beside it from the same pass
```

What `-o` names decides what you get: a name ending in `.mp4` is a video, `.png` is one frame, `.wav` is the sound
alone, and anything else is a folder of `frame-NNNNN.png`. The folder an output goes in must exist, except for a
`.wav`.

The sound alone is mixed and mastered as the video's is (its `<Mix>`'s loudness and ceiling), from the same cues, but
no frame is drawn, so a long film's sound is ready long before its video; the command prints its loudness and true
peak. `--audio` on a video render writes the very sound the video carries, before it is encoded to AAC.

A composition's size, frame rate and length come from its `<Composition>` (or an `export const config`); `--w`,
`--h`, `--fps` and `--frames` only apply to a file that states neither. A `words.json` and a `perception.json` next
to the composition are loaded on their own, as `raea play` loads them; a flag naming another file wins.

A whole video made from the same inputs as an earlier one is copied from the export cache rather than drawn again,
and the command says so.

Every render writes its manifest beside its output: `OUT.render.json` (`DIR/NAME.render.json` for a folder of
frames), or where `--manifest` says. It holds what each frame showed as laid out (each text's words, box, colour and
size, each video's source and time, each picture's box), the sounds heard and what the render found. `raea check`
reads it, and so does [`raea world`](world.md), which measures the output against it when a check asks.

A render checks what it draws: 3D parts passing into each other (the clipping check), how its 3D is staged (3D type
too small or turned to read, page type over a part, parts that snap, a camera that lurches, frames with almost nothing
on screen; `raea scene stage` runs these without drawing, all but the page's) and its pictures' exposure (clipping to
white, crushing to black). Page words meant to sit on a part go in an element with `data-allow-overlap`. A video with 3D
in it, longer than 60 frames, is also checked for frame purity: every 300th frame, its last, and the frame after each
change of more than 20 parts are rendered again on their own and held to the video's. A patch that differs is an error
naming the frame and the parts under it, with both pictures kept beside the video (`OUT.frame-pure/`), since a part
that keeps an earlier frame's state draws wrong in the video but right in a still. `RAEA_FRAME_PURE=0` skips it.
Each finding is said as it is found, written into the manifest, and
counted on the last line the command prints (`raea render: 1 error, 0 warns, 0 notes`). An error
makes the render exit 1 once its output is written; `--despite CHECK WHY` lets that check's errors through and puts
the reason in the manifest. A render of a file in a raea project also keeps its findings in the project's record
(`.raea/record/`, by render and commit), which `raea findings` and `raea next` read.

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea render EDIT.tsx -o OUT.mp4|OUT.png|OUT.wav|DIR [--audio OUT.wav] [--frame N | --at A,B] [--composition ID] [--w W] [--h H] [--fps FPS] [--frames N] [--words W.json] [--perception P.json] [--input I.json|inputs.rrl] [--props JSON|P.json] [--scale S] [--hold FRAME[:FRAMES]] [--manifest render.json] [--layers] [--stage WxH] [--10bit] [--aov] [--alpha [--alpha-codec prores|hevc|rle|png]] [--kbps N | --quality draft|high|master|0..1] [--clay] [--despite CHECK WHY]   (also: --lite --comps)
raea render EDIT.tsx --review DIR [--frame N]   (fixed cameras hero, front, right, top, iso, low in colour and clay: DIR/colour.png, clay.png, review.json)
raea render PAGE.html -o OUT.mp4|OUT.png|DIR [--at A,B] [--alpha]   (an HTML composition)
raea render --bundle NAME.comp.js -o OUT.png|DIR [--dir DIR] [--frame N]   (a bundle from raea bundle, as render_bundle draws it)
```

Headless: it needs nothing running and opens no window. Takes up to 1 argument besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--10bit` | – | Encode the video as 10-bit HEVC (Main 10), whose decoded colours match the render exactly; H.264 8-bit otherwise. |
| `--alpha` | – | Keep the background transparent: PNGs (`-o OUT.png` or a folder) or a transparent movie (`-o OUT.mov`, see `--alpha-codec`). An `.mp4` holds no alpha. |
| `--alpha-codec` | `prores\|hevc\|rle\|png` | With `--alpha` and `-o OUT.mov`: ProRes 4444 (the default on a Mac) or HEVC with alpha, both through VideoToolbox on a Mac; QuickTime Animation (lossless, the default elsewhere) or PNG frames, on every platform. Editors that take overlays (Final Cut, Premiere, DaVinci Resolve, CapCut) read all four, HEVC in the newest versions. |
| `--aov` | – | Write each frame's motion vectors and depth beside its colour, in `DIR/motion` and `DIR/depth`, for [`raea neural`](neural.md). `-o` must name a folder. |
| `--at` | `A,B` | Render only these frame numbers, comma-separated. |
| `--audio` | `OUT.wav` | With a video `-o`: also write its sound to `OUT.wav`, the same mastered sound the video carries (before AAC), from the same pass. |
| `--bundle` | `NAME.comp.js` | Render a bundle made by `raea bundle` instead of a TSX file, as `render_bundle` on the iPhone draws it. |
| `--clay` | – | Draw every standard 3D surface in plain grey, to judge shapes, light and contact without colour. |
| `--composition` | `ID` | Which registered `<Composition>` to render when the file holds several; the first one otherwise. |
| `--comps` | – | Record which components drew each frame in the frame's marker. Used by bakes and the app's live view, rarely by hand. |
| `--despite` | `CHECK WHY` | Let a check's errors through (`all` for every check), with the reason, which goes into the manifest. Without it, an error the render finds (3D parts clipping) makes it exit 1, after writing its output. A harness that renders many pieces and compares pixels sets `RAEA_DESPITE="CHECK: why"` instead. |
| `--dir` | `DIR` | With `--bundle`: the folder its relative media paths are read from (the bundle's own folder by default). |
| `--fps` | `FPS` | Frame rate for a file with no `<Composition>` or `config` (30 by default). |
| `--frame` | `N` | Render this one frame. |
| `--frames` | `N` | Length in frames for a file with no `<Composition>` or `config` (60 by default). |
| `--h` | `H` | Height in pixels for a file with no `<Composition>` or `config` (1920 by default). |
| `--hold` | `FRAME` | Draws that frame of a `<Composition>` again and again, 96 times, or as many as `FRAME:FRAMES` says. The scene and the camera hold still, so what changes between the frames is the renderer's own. `raea light steady` measures it. |
| `--input` | `I.json\|inputs.rrl` | A game's input over time: a JSON timeline, or the step keys of an `inputs.rrl`, so a recorded play-through renders the same every time. |
| `--kbps` | `N` | A fixed video bitrate in kbit/s, in place of the default that follows size and frame rate. |
| `--layers` | – | Write `NAME.layers` beside the export: which layer (a 3D part, an element, its text, an image or a video, by where its JSX is written) each pixel of each frame shows, for raea's player to select layers by clicking. The manifest (`--manifest`, or `NAME.render.json` beside it) names it. The export cache is skipped. |
| `--lite` | – | Draw with raea's live view runtime instead of React, as a live game does. Used by the app when it opens a game. |
| `--manifest` | `render.json` | Write the render's manifest to this file instead of beside the output: the composition's length in frames (`length`, whichever frames were rendered), every frame's element boxes as drawn, what the render found and the sounds heard, which `raea check` and `raea world` read. |
| `--perception` | `P.json` | The perception the composition's Clock reads (faces, boxes, one for every frame or by frame), when it isn't the `perception.json` beside it. |
| `--props` | `JSON\|P.json` | Input props, as a JSON object or a file holding one. `getInputProps()` returns them, and they go over the composition's `defaultProps` before `calculateMetadata` runs. |
| `--quality` | `draft\|high\|master\|0..1` | Constant-quality encoding: `draft` for previews, `high` for most deliveries, `master` for type and hard edges, or a number from 0 to 1. On Windows and Linux (OpenH264) it scales the bitrate instead, and once a frame fits at OpenH264's finest quantiser (12), a higher setting writes the same file. |
| `--review` | `DIR` | Render a 3D scene from six fixed cameras (hero, front, right, top, iso, low), in colour and in clay, to `DIR/colour.png`, `DIR/clay.png` and `DIR/review.json`. |
| `--scale` | `S` | Draws the composition at S times its size: `0.5` is a half-size draft, `2` a 1080p composition at 4K. Layout stays in the composition's own pixels. Above 0, 8 at most. |
| `--stage` | `WxH` | Render a responsive composition, one that lays out from `useVideoConfig()`, at this size. A fixed-size composition ignores it and says so. |
| `--w` | `W` | Width in pixels for a file with no `<Composition>` or `config` (1080 by default). |
| `--words` | `W.json` | A transcript for the Clock's words (captions and cuts read them), when it isn't the `words.json` beside the composition. |
| `-o` | `OUT.mp4\|OUT.png\|OUT.wav\|DIR` | Where it goes: an `.mp4` video, one `.png`, the sound alone as a `.wav`, or a folder of frames. |

<!-- generated:end -->

## See also

- [Rendering and checking](../../concepts/rendering.md) covers stills, quality, transparent frames and dark material in depth.
- [`raea check`](check.md) measures a render against the floor; [`raea play`](play.md) runs a game live.
