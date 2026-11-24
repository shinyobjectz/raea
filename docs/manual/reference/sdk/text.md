---
title: "raea/text"
summary: "Text in the 3D world, fonts, and time anchored to the words said."
order: 44
sources: ["sdk/raea/src/text.ts"]
---

Two kinds of text live here. `<RichText>` and `<Text3D>` put text into a 3D scene, each glyph a lit entity that the
same named moves as 2D can animate glyph by glyph. `word()`, `findPhrase` and `useWords` anchor time to speech: the
frame a phrase is said, from the transcript [`raea cut`](../cli/cut.md) writes, wherever the cut puts it.

```tsx
import { Sequence } from "raea";
import { word } from "raea/text";

function Callout() {
  const at = word("forty countries");
  return (
    <Sequence from={at} durationInFrames={60}>
      <h1 style={{ fontSize: 96, color: "#fff" }}>40 countries</h1>
    </Sequence>
  );
}
```

See [Cutting by words](../../guides/words.md).

<!-- generated:begin by tools/manual-ref/sdk.ts from the SDK's exports and doc comments; edit outside this block -->

```ts
import { findPhrase, loadFont, RichText, Span } from "raea/text"
```

`raea/text` exports 3 components, 1 hook, 3 functions, 6 types. Bevy's counterpart: `bevy_text`. Source: [sdk/raea/src/text.ts](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/src/text.ts).

## Components

### `RichText`

Text in the 3D world: every glyph a real lit, shaded entity (bevy_rich_text3d's mesh over its glyph atlas, a
StandardMaterial of its own), laid out as one line of shaped text (kerning, spans of other fonts and sizes) and
then moved, turned, scaled, recoloured, faded and revealed glyph by glyph with the motion code 2D uses: the named
moves staggered (`enter="rise" each={2}`), a typewriter's `reveal`, or a `glyph` function of the frame.

 <RichText name="title" size={0.6} enter="pop" each={3}>Hello <Span color="#ffd400" weight={900}>world</Span></RichText>
 <RichText glyph={(g) => ({ position: [0, Math.sin(frame / 6 + g.index) * 0.05, 0] })}>WAVE</RichText>

Takes [`RichTextProps`](3d.md#richtextprops)'s props.

### `Span`

A run of a <RichText>'s text in its own font, weight, size or colour. Only inside a <RichText>.

Takes [`SpanProps`](3d.md#spanprops)'s props.

### `Text3D`

Solid 3D letters (Manifold's text3d), as a Mesh's geometry: `<Mesh><Text3D size={0.6}>SALE</Text3D>...</Mesh>`.

Takes [`Text3DProps`](3d.md#text3dprops)'s props.

## Hooks

### `useWords`

Every word heard on the timeline, in the frames of the nearest enclosing Sequence.

```ts
function useWords(): TimelineWord[]
```

## Functions

### `findPhrase`

Finds the composition frame where `phrase` is said in a transcript.

```ts
function findPhrase(words: readonly HeardWord[], phrase: string, options?: WordOptions): number
```

### `loadFont`

The page gets Google Fonts' `family` ("Inter", "Space Grotesk"); `fontFamily` is the name to style text with.

```ts
function loadFont(family: string): { fontFamily: string; }
```

### `word`

The composition frame where `phrase` is said. Call it while rendering (it reads the Clock).

```ts
function word(phrase: string, options?: WordOptions): number
```

## Types

### `GlyphInfo`

What a glyph is, for a <RichText>'s `glyph` function.

| Field | Type | What it is |
| --- | --- | --- |
| `index` (required) | `number` | its place among the drawn glyphs (spaces aren't glyphs), from 0 |
| `count` (required) | `number` | how many glyphs the block has |
| `char` (required) | `string` | the character it draws |
| `word` (required) | `number` | its word's place among the words, from 0, and how many there are |
| `words` (required) | `number` | how many words the block has |
| `inWord` (required) | `number` | its place in its word, and the word's glyph count |
| `wordLength` (required) | `number` | how many glyphs its word has |
| `line` (required) | `number` | its line (by the newlines in the text), from 0 |
| `span` (required) | `number` | its run: the <Span> it's in (0 for text outside any, counted in order) |

### `GlyphStyle`

A glyph's motion this frame, about its own centre, in the block's space: added to (position, rotation),
multiplied into (scale, opacity) or replacing (color, visible) what the block's moves give it.

| Field | Type | What it is |
| --- | --- | --- |
| `position` | `Vec3` | metres, from its place in the block |
| `rotation` | `Vec3` | radians, XYZ |
| `scale` | `number \| Vec3` | multiplies its size: one number, or x, y and z |
| `color` | `string` | replaces its colour |
| `opacity` | `number` | multiplies its opacity, 0..1 |
| `visible` | `boolean` | false hides it this frame |

### `RichTextProps`

| Field | Type | What it is |
| --- | --- | --- |
| `name` | `string` | what it's called across frames, and its glyphs' names over world.* (`${name}:${index}`) |
| `read` | `boolean` | false: scenery, not words to read (a shop sign seen in passing, a page in a book), so raea render doesn't hold it to type that reads: its size on screen, its angle to the camera, its reveal and its reading time |
| `pass` | `string` | the composition pass it draws in ("scene" by default): a frame-wide effect told to skip a pass (<Look skip>, <Outline skip>, <Bloom skip>, <Grade skip>) leaves its pixels as drawn, and <Look passes> gives one its own paint; a piece names at most three passes besides "scene"; keyed per frame |
| `position` | `Vec3` | where the block's origin is, metres (x, y, z) |
| `rotation` | `Vec3` | the block's rotation, radians (x, y, z) |
| `scale` | `number \| Vec3` | the block's scale: one number, or x, y and z |
| `size` | `number` | the em in metres (0.5 by default) |
| `font` | `string` | a family (raea's bundled faces: Inter by default, Montserrat, Oswald, Playfair Display...; or one on this machine) |
| `weight` | `number` | 100..900 (700 by default) |
| `italic` | `boolean` | slanted (false by default) |
| `color` | `string` | each glyph's colour unless its <Span> gives one |
| `align` | `"left" \| "center" \| "right"` | where the origin is: the left edge, the middle (default) or the right edge of the lines; the block's middle vertically |
| `lineHeight` | `number` | times the em (1.2 by default) |
| `maxWidth` | `number` | wrapped at this width in metres (no wrapping by default) |
| `roughness` | `number` | 0..1, smooth to matte |
| `metalness` | `number` | 0..1, a dielectric to a metal |
| `emissive` | `string` | the colour it glows with |
| `emissiveIntensity` | `number` | how strongly it glows (1 by default; 2-10 with bloom) |
| `unlit` | `boolean` | drawn in its flat colour, without light or shade |
| `castShadow` | `boolean` | letter-shaped shadows (true by default) |
| `receiveShadow` | `boolean` | other things' shadows fall on it |
| `layers` | `number[]` | the render layers it's on ([0] by default): a camera draws only the layers it's given |
| `enter` | `MoveLike` | each glyph's (or word's) entrance: a move's name, a move or several (rise, pop, slam, slide, fade... the 2D ones, their px read at 100 an em, their degrees as degrees), `each` frames apart from `at` |
| `each` | `number` | frames between one glyph's (or word's) entrance and the next (2 by default) |
| `by` | `"letter" \| "word"` | the entrance and exit staggered by letter (default) or by word |
| `at` | `number` | the local frame the first entrance starts (0) |
| `exit` | `MoveLike` | each glyph's (or word's) exit: a move or its name, played backwards from `exitAt` |
| `exitAt` | `number` | the local frame the first exit starts; an exit needs it |
| `exitEach` | `number` | frames between exits (0: together) |
| `reveal` | `number` | how many glyphs are shown, a typewriter's count: the next one fades in over its fraction |
| `glyph` | `(g: GlyphInfo) => GlyphStyle` | each glyph's own motion this frame, from anything the frame gives (useCurrentFrame, spring, interpolate) |
| `children` | `ReactNode` | the text: strings and <Span> runs |

### `SpanProps`

| Field | Type | What it is |
| --- | --- | --- |
| `children` | `ReactNode` | the run's text |
| `font` | `string` | a family (raea's bundled faces: Inter, Montserrat, Oswald, Playfair Display...; or one on this machine) |
| `weight` | `number` | 100..900 (the block's by default) |
| `italic` | `boolean` | slanted (the block's by default) |
| `size` | `number` | times the block's size |
| `color` | `string` | the run's colour (the block's by default) |

### `Text3DProps`

| Field | Type | What it is |
| --- | --- | --- |
| `children` | `string` | the words, extruded |
| `text` | `string` | the words, when they aren't its child (no quotes or backslashes) |
| `size` | `number` | letter height in scene units (default 0.5) |
| `depth` | `number` | extrusion depth (default 0.2) |
| `font` | `string` | a family (Inter by default, or another of raea's bundled faces) |
| `weight` | `number` | 100..900 (800 by default) |
| `read` | `boolean` | false: scenery (a sign seen in passing), not held to the type checks raea render runs on 3D type: its size on screen, its angle to the camera, the title-safe area, its reading time and parts in front of it. Words meant to be read are better as page type: a Label |

### `WordOptions`

| Field | Type | What it is |
| --- | --- | --- |
| `occurrence` | `number` | which time the phrase is said, from 1 |
| `edge` | `"start" \| "end"` | "start": the phrase's first frame; "end": the frame after its last word ends |

<!-- generated:end -->
