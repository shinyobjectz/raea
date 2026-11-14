---
title: "raea/editor"
summary: "The timeline the feed edits a film, a scene or a sound on, as data: tracks, clips, keys and markers, every edit an operation with its inverse, a film's scenes read in (fromRender) and its cut read back out as segments (segmentsOf). No React and no DOM: an agent's script, the app's host and a test use the same model the app's timeline does."
order: 48
sources: ["sdk/raea/src/editor.ts"]
---

The timeline the feed edits a film, a scene or a sound on, as data. A `Timeline` holds tracks of clips in whole
frames, with keys, markers and transitions; `apply` and `applyAll` make an edit and hand back its inverse, so every
change can be undone; the main track's magnet, snapping and linkage behave as the feed's timeline does. A film's
scenes are read in with `fromRender` and its cut read back out with `segmentsOf`, as a composition's
`edit={{ segments }}` takes it; a composition's sequences are read in with `fromSequences`, and `seqEdits` says what
an operation does to them. Easings, speed curves, snapping, the frame store's keys and what a plugin adds are here
too. No React and no DOM: an agent's script, the app's host and a test use the same model the app's timeline does.
See [the example](https://github.com/shinyobjectz/raea/blob/main/docs/manual/examples/edits.tsx) for a timeline
built, edited and undone.

<!-- generated:begin by tools/manual-ref/sdk.ts from the SDK's exports and doc comments; edit outside this block -->

```ts
import { ahead, animStyle, apply, applyAll } from "raea/editor"
```

`raea/editor` exports 53 functions, 16 values, 37 types. Bevy's counterpart: `raea's own; Bevy has no timeline editor`. Source: [sdk/raea/src/editor.ts](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/src/editor.ts).

## Functions

### `ahead`

The ranges to draw next while there is time: what isn't kept from the playhead on, nearest first, each
at most `chunk` frames, then what is before it.

```ts
function ahead(length: number, kept: Range[], playhead: number, chunk: number): Range[]
```

### `animStyle`

The clip's style `into` frames from its start: coming in by its In, going out by its Out (each over no more of it
than it has) and moving by its Combo, one of `combos` (a plugin's among them). Empty when none plays there.

```ts
function animStyle(c: Clip, into: number, combos?: ComboKind[]): EdgeStyle
```

### `apply`

The operation applied, or why it can't be.

```ts
function apply(t: Timeline, op: Op): Applied
```

### `applyAll`

Operations in order, stopping at the first refused: the timeline they give, and each one's inverse (last first).

```ts
function applyAll(t: Timeline, ops: Op[]): { timeline: Timeline; inverse: Op[]; refused?: { op: Op; why: string; } | undefined; }
```

Used in: [manual: edits](https://github.com/shinyobjectz/raea/blob/main/docs/manual/examples/edits.tsx).

### `bezier`

y at x on the cubic bezier (0,0) (x1,y1) (x2,y2) (1,1): x is solved by Newton's steps, then bisection where the
slope is too flat for them. The handles' x are kept in 0..1 so the curve is a function of time.

```ts
function bezier(x1: number, y1: number, x2: number, y2: number): (x: number) => number
```

### `captionLines`

The words gathered into caption lines: a line ends after a sentence does, at a pause, or once it holds as many
words or characters as it may. Each line runs from its first word to its last and ends by the next line's start.

```ts
function captionLines(words: Said[], fps: number, o?: LineRules): Said[]
```

### `changed`

The frames whose picture or sound differ between two states of a timeline: wherever what is drawn over a frame
isn't what was. Moving a clip from 2:00-2:05 to 2:10-2:15 gives those two ranges and nothing else; a
split of footage gives none.

```ts
function changed(before: Timeline, after: Timeline): Range[]
```

### `clipEdits`

The file clips that differ between the timeline as it was read and as it stands, each with its span before and
after: what a sound's hand edit carries (its `clips`). A clip that was removed or made here isn't one of these.

```ts
function clipEdits(read: Timeline, now: Timeline): { id: string; src: string; from: ClipSpan; to: ClipSpan; }[]
```

### `clips`

```ts
function clips(t: Timeline): Clip[]
```

### `cut`

The keys either side of a cut at `frame` (TL3d: a split clip's keys stay at their frames): the left clip's, and
the right clip's in its own frames. A cut between two keys puts a key of the value there on both sides, so each
half moves as the whole did up to the cut.

```ts
function cut(keys: Key[], frame: number): [Key[], Key[]]
```

### `eased`

How far along (0..1, or past it for a handle that overshoots) a value is at `p` of the way between two keys.

```ts
function eased(ease: Ease | undefined, p: number): number
```

### `effectFilter`

The CSS filter of `kind` at `amount` (0 to 1, clamped); undefined for a kind raea doesn't draw.

```ts
function effectFilter(kind: string, amount?: number): string | undefined
```

### `effectParts`

An effect clip's parts, kept in its props as JSON: read back when they are well formed.

```ts
function effectParts(v: unknown): EffectPart[] | undefined
```

### `empty`

```ts
function empty(fps?: number): Timeline
```

Used in: [manual: edits](https://github.com/shinyobjectz/raea/blob/main/docs/manual/examples/edits.tsx).

### `end`

```ts
function end(c: Clip): number
```

Used in: [manual: edits](https://github.com/shinyobjectz/raea/blob/main/docs/manual/examples/edits.tsx).

### `evict`

The keys to drop so the store is at most `limit` bytes: stale ones first, then the least recently played. What
is under the playhead (`hold`) stays while anything else can go.

```ts
function evict(kept: Kept[], limit: number, hold?: Set<string>): string[]
```

### `find`

```ts
function find(t: Timeline, id: string): { track: Track; clip: Clip; } | undefined
```

### `fits`

Whether `c` has room on the track (it overlaps none of the others).

```ts
function fits(track: Track, c: Pick<Clip, "start" | "length" | "id">): boolean
```

### `frameOf`

The timeline frame at which clip `id` shows source frame `source`, when it does (a constant-speed clip).

```ts
function frameOf(t: Timeline, id: string, source: number): number | undefined
```

### `fromRender`

A film with no composition behind it: its scenes as clips of one main track (cut where the picture cuts). Each
clip is the film's picture and its sound together, as footage is until its sound is extracted, so a cut,
a move or a speed takes both. Its magnet is on: a scene removed closes up, as a cut of footage does.

```ts
function fromRender(o: { frames: number; fps: number; cuts?: number[]; captions?: { text: string; start: number; end: number; }[] | undefined; }): Timeline
```

### `fromSequences`

The composition's sequences as a timeline: a track a lane, the first lane on top as the strip draws it. The first
lane is the main track, its magnet on, only when its clips already run from frame 0 with no gap: a composition's
sequences are where its code puts them, and nothing here moves one that wasn't dragged.

```ts
function fromSequences(rows: SeqRow[], fps: number): Timeline
```

### `fromTracks`

A sound's tracks as a timeline: a sound track each, by its bus; a clip of a file keeps its place in the file
(its offset) and its level. Notes and lines are clips too, locked here: they are edited where they are written
(the roll, the words). No magnet: a mix's clips are where they were put.

```ts
function fromTracks(tracks: SoundTrack[], fps: number): Timeline
```

### `gridFrames`

The frames a snap grid marks in a timeline `length` frames long (none for a grid finer than a frame, or of more than 5,000 marks).

```ts
function gridFrames(grid: SnapGrid, fps: number, length: number): number[]
```

### `heard`

The words as the edited timeline says them. `words` are in the film's own frames (where its transcript has them);
each is said where a main-track clip plays that part of the film, at that clip's speed. A word in a scene taken out
is gone, a scene duplicated says its words twice, and a clip curved, reversed or frozen says none.

```ts
function heard(t: Timeline, words: Said[]): Said[]
```

### `joined`

Ranges in order, touching and overlapping ones joined, empty ones dropped.

```ts
function joined(ranges: Range[]): Range[]
```

### `keyOf`

What a range of kept frames is kept under: everything that draws it. Two states of a timeline that draw the
range the same give the same key, so frames drawn before an edit elsewhere (or before an undo) are found again.

```ts
function keyOf(t: Timeline, [from, to]: Range, media?: (clip: Clip) => string): string
```

### `length`

The timeline's length: its last clip's end.

```ts
function length(t: Timeline): number
```

Used in: [manual: edits](https://github.com/shinyobjectz/raea/blob/main/docs/manual/examples/edits.tsx).

### `next`

Where the player goes when what it is playing ends at timeline frame `frame`: the next clip's first frame (at
or after it), or undefined at the film's end.

```ts
function next(t: Timeline, frame: number): number | undefined
```

### `palette`

Everything offered: raea's own, then each plugin's in order (a name a plugin before it took stays that one's).

```ts
function palette(plugins?: Adds[]): { eases: Record<string, Ease>; curves: Record<string, SpeedPoint[]>; transitions: TransitionKind[]; combos: ComboKind[]; tracks: TrackKindInfo[]; effects: EffectInfo[]; texts: TextStyle[]; tools: Preset[]; snaps: SnapGrid[]; }
```

### `partsFilter`

The CSS filter of an effect given as parts, at `amount` (0 to 1, clamped).

```ts
function partsFilter(parts: EffectPart[], amount?: number): string
```

### `played`

How many frames `source` frames of source take to play under the curve.

```ts
function played(curve: SpeedPoint[], source: number): number
```

### `playLength`

The timeline's length as it plays: its main track's end.

```ts
function playLength(t: Timeline): number
```

### `points`

The curve as points from 0 to 1 in order, each speed in range.

```ts
function points(curve: SpeedPoint[]): SpeedPoint[]
```

### `presetError`

Why `p` isn't a preset, or undefined when it is.

```ts
function presetError(p: unknown): string | undefined
```

### `profile`

The speed at each of `n` places along the clip's own time (the line drawn over the clip, TL6b).

```ts
function profile(curve: SpeedPoint[], source: number, n: number): number[]
```

### `readAdds`

A plugin's additions as written in its manifest, checked: what is well formed, and why the rest isn't.

```ts
function readAdds(raw: unknown): { adds: Adds; errors: string[]; }
```

### `readTimeline`

A timeline as an agent wrote it on a feed item or a controller's field (its own data: a schedule, a set list, a
shot plan), checked and made whole: what is well formed, and why the rest isn't. Frames are rounded to whole
ones; a track with no kind is an overlay; ids must be there and not repeat.

```ts
function readTimeline(raw: unknown): { timeline?: Timeline; errors: string[]; }
```

### `ready`

The timeline ranges that are ready to play, given the source ranges that are (frames of the file the player has
buffered): each main-track clip's part whose source is in one of them.

```ts
function ready(t: Timeline, source: Range[]): Range[]
```

### `segmentsOf`

A render's cut, as the frames of the source played in order (the hand edit's `segments`): what a film's main
track is after its scenes were moved, trimmed, split and removed.

```ts
function segmentsOf(t: Timeline): [number, number][]
```

### `seqEdits`

What the host is asked to do to the composition's sequences for `op` on timeline `t` (read from them), in order;
or why this operation isn't one the sequences can take yet. Each edit names sequences as they are before it.

```ts
function seqEdits(t: Timeline, op: Op): SeqEdit[] | { why: string; }
```

### `showing`

What shows at timeline frame `frame`: the main-track clip over it, the source frame, and the rate. Undefined
in a gap or past the end.

```ts
function showing(t: Timeline, frame: number): Showing | undefined
```

### `slot`

Where a track of this kind goes among the others: after the last of its own kind, else in ORDER's place.

```ts
function slot(tracks: Track[], kind: TrackKind): number
```

### `snap`

`frame` moved to the nearest target within `within` frames (8 px at the strip's zoom), and what it snapped to.

```ts
function snap(frame: number, to: Target[], within: number): { frame: number; to?: Target; }
```

### `snapMove`

A moved clip (its start and length) snapped by whichever of its edges is nearer a target: how far it moves.

```ts
function snapMove(start: number, len: number, to: Target[], within: number): { by: number; to?: Target; }
```

### `sorted`

The keys in time order (two at one frame: the later one written wins).

```ts
function sorted(keys: Key[]): Key[]
```

### `sourceAt`

The source frame showing `time` frames into the clip, under the curve over `source` frames of source.

```ts
function sourceAt(curve: SpeedPoint[], source: number, time: number): number
```

### `speedOf`

A clip's speed where it is constant.

```ts
function speedOf(c: Clip): number
```

### `targets`

Every frame an edge can snap to, leaving out the clips being moved (`not`).

```ts
function targets(t: Timeline, o?: { not?: string[]; playhead?: number; beats?: number[]; }): Target[]
```

### `textStyleError`

Why `t` isn't a text style, or undefined when it is.

```ts
function textStyleError(t: unknown): string | undefined
```

### `textStyleOf`

A text clip's style, kept in its props as JSON: read back when it is well formed.

```ts
function textStyleOf(v: unknown): TextStyle | undefined
```

### `timecode`

mm:ss:ff, drawn from frames.

```ts
function timecode(frames: number, fps: number): string
```

### `valueAt`

The property's value at `frame` (frames from its clip's start), or undefined with no keys.

```ts
function valueAt(keys: Key[] | undefined, frame: number): Value | undefined
```

Used in: [manual: edits](https://github.com/shinyobjectz/raea/blob/main/docs/manual/examples/edits.tsx).

### `without`

`ranges` with `cut` taken out of them.

```ts
function without(ranges: Range[], cut: Range[]): Range[]
```

## Values

### `CODE_KINDS`

The transitions a composition's <Transition> draws (sdk/video edges.ts).

### `COMBO_KINDS`

### `COMBOS`

raea's own Combos: the kinds <Transition combo> draws. A plugin adds its own as data the same way.

### `CURVES`

CapCut's curve presets, as points over the source; a plugin adds its own.

### `EASES`

The easing presets raea registers; a plugin adds its own beside them.

### `EFFECTS`

### `FASTEST`

### `KEYED`

The properties a sequence's <Nudge> can key (sdk/video nudge.tsx).

### `ORDER`

### `PRESET_PROPS`

The props a preset may set: a clip's placement (what the editor's own tools set).

### `SLOWEST`

### `TEXT_PROPS`

The CSS a text style may set: type and colour, nothing that lays the frame out.

### `TEXTS`

raea's own text styles. Sizes are the composition's pixels (a 1080-line frame).

### `TRACKS`

raea's own kinds of track, in the order they stack.

### `TRANSITIONS`

raea's own transitions: the kinds its <Transition> draws (sdk/video transition.tsx). Two parts that only
touch have no frames of each other to mix, so there is no dissolve: a fade goes through black.

### `under`

Kinds a plugin said stack under the picture, with the sound (registry.ts sets them).

## Types

### `Adds`

What one plugin adds.

| Field | Type | What it is |
| --- | --- | --- |
| `eases` | `Record<string, Ease>` | easings of its own, by name |
| `curves` | `Record<string, SpeedPoint[]>` | speed curves of its own, by name |
| `transitions` | `TransitionKind[]` | transitions of its own |
| `combos` | `ComboKind[]` | Combos of its own: how a clip moves over one cycle, as data the timeline's preview draws |
| `tracks` | `TrackKindInfo[]` | kinds of track of its own: a clip posted or added on one is drawn and edited like any other |
| `effects` | `EffectInfo[]` | effects of its own (TL7b, TL11a), each drawn by its parts: CSS filter functions from amount 0 to amount 1 |
| `texts` | `TextStyle[]` | text styles of its own (TL7c, TL11a) |
| `tools` | `Preset[]` | tools of its own: presets put on the picked clips |
| `snaps` | `SnapGrid[]` | marks edges snap to: grids in seconds |

### `Anim`

How a clip comes in or goes out on its own: a transition's kind over its own first or last frames.

| Field | Type | What it is |
| --- | --- | --- |
| `kind` (required) | `string` | fade, slide, wipe, zoom or a plugin's (TRANSITIONS) |
| `length` (required) | `Frames` | how many of the clip's frames it takes |

### `Applied`

```ts
type Applied = { ok: true; timeline: Timeline; inverse: Op } | { ok: false; why: string }
```

### `Clip`

A span of something on a track: footage, a sound, words, a composition's sequence, an effect.

| Field | Type | What it is |
| --- | --- | --- |
| `id` (required) | `string` | its own id, unique on the timeline |
| `kind` (required) | `string` | what it is: "video", "audio", "text", "sequence", "component", "effect", a plugin's own |
| `name` | `string` | what its head says (its file's or its sequence's name when unset) |
| `start` (required) | `Frames` | where it starts on the timeline |
| `length` (required) | `Frames` | how long it lasts on the timeline |
| `in` | `Frames` | media: the first source frame it shows |
| `source` | `Frames` | media: how many frames the source has (unset: no end to trim against) |
| `speed` | `number` | a constant speed (1 when unset, TL6a) |
| `curve` | `SpeedPoint[]` | a speed curve, which wins over `speed` |
| `shows` | `Frames` | under a curve: how many source frames the clip shows (its length is the time the curve takes over them) |
| `reverse` | `boolean` | footage played backward: its last source frame first |
| `freeze` | `boolean` | a still: the source frame `in` held for the clip's whole length |
| `off` | `boolean` | turned off: drawn dimmed, left out of the picture and the sound |
| `muted` | `boolean` | footage whose own sound was taken out onto a sound track: its picture plays silent |
| `locked` | `string` | it can't be changed here, and why ("set in code", TL0c) |
| `group` | `string` | clips with one group pick and move as one |
| `props` | `Record<string, Value \| boolean>` | its still values by property (x, y, scale, rotate, opacity, text, a plugin's): a keyed one is ignored |
| `keys` | `Record<string, Key[]>` | its keys by property, in frame order |
| `volume` | `number` | sound: its level, 1 as recorded |
| `fadeIn` | `Frames` | sound: how long it fades in from its start |
| `fadeOut` | `Frames` | sound: how long it fades out to its end |
| `enter` | `Anim` | the picture or words: how it comes in over its first frames (TL7d, CapCut's In animation) |
| `exit` | `Anim` | the picture or words: how it goes out over its last frames (TL7d, CapCut's Out animation) |
| `combo` | `Anim` | the picture or words: how it moves over its whole length (TL7e, CapCut's Combo and its text Loop), a cycle every `length` frames from its first; its own length plays it once |
| `at` | `string` | its element in the code |
| `markers` | `Marker[]` | markers on the clip itself, in frames from its start |
| `inside` | `{ kind: TrackKind; clip: Clip }[]` | a compound clip: the clips it holds, each with the kind of track it came from, their starts in frames from this clip's own start |

### `ClipSpan`

A clip's span and level, in seconds and dB: what the host writes back for a clip that was moved, trimmed or
turned up or down (raea-feed host/clips.ts).

| Field | Type | What it is |
| --- | --- | --- |
| `start` (required) | `number` | where it starts, in seconds of the render |
| `end` (required) | `number` | where it ends, in seconds of the render |
| `offset` (required) | `number` | where in its file it starts playing, in seconds |
| `gainDb` (required) | `number` | its level change, in dB (0 as recorded) |

### `ComboKind`

A kind of Combo: its name, what the person sees, and its wave.

| Field | Type | What it is |
| --- | --- | --- |
| `kind` (required) | `string` | its name, as code writes it |
| `title` (required) | `string` | what the person sees |
| `wave` (required) | `ComboWave` | how it moves over one cycle |
| `linear` | `boolean` | straight from value to value, not eased: a spin that keeps turning |

### `ComboWave`

How a part moves over one cycle of a Combo: each
property's values, evenly spaced over the cycle and eased between, the cycle starting again where it ends. x and y
move it by a share of its own size in percent, rotate turns it in degrees, scale sizes it, opacity fades it. A wave
that ends where it starts plays once over the part as CapCut's Combo does, and loops seamlessly as its text Loop does.

| Field | Type | What it is |
| --- | --- | --- |
| `x` | `number[]` | across, in percent of its width |
| `y` | `number[]` | down, in percent of its height |
| `scale` | `number[]` | its size, 1 as it is |
| `rotate` | `number[]` | turned, in degrees |
| `opacity` | `number[]` | how much of it shows, 0 to 1 |

### `Ease`

How a value travels from one key to the next: a straight line, no travel (it holds, then jumps), or a cubic
bezier's two handles [x1, y1, x2, y2] as CSS writes them.

```ts
type Ease = "linear" | "hold" | [number, number, number, number]
```

### `EdgeStyle`

What an edge does to the part at a frame: a move, a fade and a cut-away, as CSS writes them.

| Field | Type | What it is |
| --- | --- | --- |
| `transform` | `string` | a move or a scale |
| `opacity` | `number` | how much of it shows, 0 to 1 |
| `clipPath` | `string` | how much of it is covered |

### `EffectInfo`

An effect the palette offers: raea's own by its kind, a plugin's with the parts that draw it.

| Field | Type | What it is |
| --- | --- | --- |
| `kind` (required) | `string` | its kind, as <Effect kind> takes it |
| `title` (required) | `string` | what the bar and the clip's head say |
| `parts` | `EffectPart[]` | a plugin's: the CSS filter functions that draw it, each from amount 0 to amount 1 |

### `EffectKind`

| Field | Type | What it is |
| --- | --- | --- |
| `kind` (required) | `string` | its name in the composition's code: <Effect kind> |
| `title` (required) | `string` | what the person sees |
| `filter` (required) | `(amount: number) => string` | the CSS filter at `amount` |

### `EffectPart`

An effect as data: each part one CSS filter function, its value going
from `from` (amount 0, where it changes nothing) to `to` (amount 1), with its unit: ["blur", 0, 6, "px"].

```ts
type EffectPart = [fn: string, from: number, to: number, unit?: string]
```

### `Frames`

A count of frames, always whole.

```ts
type Frames = number
```

### `Kept`

A drawn range in the store, as the clearing reads it.

| Field | Type | What it is |
| --- | --- | --- |
| `key` (required) | `string` | the range's key (rangeKey): what it is drawn from |
| `bytes` (required) | `number` | what it takes on disk |
| `used` (required) | `number` | when it was last played (ms) |
| `stale` | `boolean` | it belongs to a version that was closed or discarded: it goes at once |

### `Key`

A property's value at a frame of its clip (frames from the clip's start), and how it leaves for the next key.

| Field | Type | What it is |
| --- | --- | --- |
| `at` (required) | `Frames` | the frame it is at, counted from its clip's start |
| `value` (required) | `Value` | the property's value there: a number, a list of them (a colour, a point) or a string |
| `ease` | `Ease` | how the value travels to the next key (linear when unset) |

### `LineRules`

How words are gathered into lines.

| Field | Type | What it is |
| --- | --- | --- |
| `words` | `number` | the most words on a line (7) |
| `chars` | `number` | the most characters on a line, spaces counted (42) |
| `gap` | `Frames` | a pause at least this long starts a new line (0.6 s) |

### `Marker`

A mark at a frame: a beat, a comment's time or one put there by hand.

| Field | Type | What it is |
| --- | --- | --- |
| `id` (required) | `string` | its own id, unique on the timeline |
| `at` (required) | `Frames` | its frame on the timeline (on a clip: from the clip's start) |
| `name` | `string` | what it says, shown on hover |
| `color` | `string` | its colour, any CSS colour |

### `Op`

```ts
type Op = | { op: "move"; ids: string[]; by: Frames; track?: string } /** a clip's start or end dragged to the timeline frame `to` */ | { op: "trim"; id: string; edge: "start" | "end"; to: Frames } /** the clips `ids` split at `at`; with none named, every clip under it (Cmd+B, Cmd+Shift+B: TL3d) */ | { op:…
```

### `Preset`

A tool a plugin adds: one press sets these on every picked clip, as one edit each.

| Field | Type | What it is |
| --- | --- | --- |
| `id` (required) | `string` | its id: lowercase letters, digits and dashes |
| `title` (required) | `string` | what the bar says |
| `set` (required) | `{ props?: Record<string, number \| boolean>; speed?: number; volume?: number }` | what it sets on each picked clip: placement props (PRESET_PROPS), a speed, a level |

### `Range`

```ts
type Range = [from: Frames, to: Frames]
```

### `Said`

A word, or a line of them: its text, its first frame and the frame after it ends.

| Field | Type | What it is |
| --- | --- | --- |
| `text` (required) | `string` | what is said: a word, or a line's words with a space between |
| `start` (required) | `Frames` | its first frame |
| `end` (required) | `Frames` | the frame after it ends |

### `SeqEdit`

The host's edit to a sequence: its new span, a split, a delete.

```ts
type SeqEdit = | { op: "move"; i: number; from: number; to: number } | { op: "split"; i: number; at: number } | { op: "delete"; i: number } /** the sequence's keyed properties as they now stand: written as its <Nudge keys>, one a property (TL5d) */ | { op: "keys"; i: number; keys: Record<string, [number, number…
```

### `SeqLook`

A sequence's still placement: its <Nudge>'s props (sdk/video nudge.tsx). `crop` is left, top, right, bottom.

| Field | Type | What it is |
| --- | --- | --- |
| `x` | `number` | moved right, in px of the composition |
| `y` | `number` | moved down, in px of the composition |
| `scale` | `number` | times its size (1 as it is) |
| `rotate` | `number` | turned clockwise, in degrees |
| `opacity` | `number` | from 0 (gone) to 1 (as it is) |
| `mirror` | `boolean` | flipped left to right |
| `crop` | `[number, number, number, number]` | cut in from each edge, in % of its size: left, top, right, bottom |

### `SeqRow`

A <Sequence> as the host lists it: its place among the file's elements, its span in the composition's frames,
the lane it sits on (nested ones below their parent, overlapping ones apart), and why it can't be rewritten.

| Field | Type | What it is |
| --- | --- | --- |
| `i` (required) | `number` | its place among the file's <Sequence> elements, in source order: what an edit names it by |
| `name` (required) | `string` | its name prop, or what the host calls it when it has none |
| `label` | `string` | what its clip's head says, when it isn't the name |
| `from` (required) | `number` | its first frame in the composition |
| `to` (required) | `number` | the frame after its last |
| `lane` (required) | `number` | the lane it sits on: 0 the top, nested ones below their parent, overlapping ones apart |
| `locked` | `string` | why it can't be rewritten (a span from a variable, a loop), when it can't |
| `keys` | `Record<string, [number, number, Ease?][]>` | the keys its <Nudge> holds, each [frame of the sequence, value, ease?] |
| `speed` | `number` | how fast the footage it holds plays (its media's playbackRate), when it holds footage at one rate |
| `curve` | `[number, number][]` | its footage's speed as a curve, each [place 0 to 1, speed] (its media's speedCurve), over `shows` source frames |
| `shows` | `number` | under `curve`: how many source frames it shows |
| `look` | `SeqLook` | how it is placed, as its <Nudge> says without keys: moved, scaled, turned, faded, mirrored, cropped |
| `text` | `string` | the words it shows, when words are all it shows: a text clip, its words changed by renaming it |
| `effect` | `{ kind: string; amount: number; /** a plugin's: the parts that draw it */ parts?: Effec…` | the effect it puts on the picture under it, when an <Effect> is all it holds: an effect clip |
| `in` | `{ kind: string; frames: number }` | how it comes in and goes out: its <Transition>'s in and out |
| `out` | `{ kind: string; frames: number }` | how it goes out: its <Transition>'s out |
| `combo` | `{ kind: string; frames: number }` | how it moves throughout: its <Transition>'s combo, a cycle every `frames` |
| `media` | `"video" \| "audio"` | a <Video> or an <Audio> of its own: it plays where the sequence it is in does, from `startFrom` of its file, at its own level |
| `startFrom` | `number` | its media's first frame used, in frames of its file |
| `volume` | `number` | its media's level, 1 as recorded |
| `muted` | `boolean` | a <Video> whose own sound was taken out (muted): its sound is an <Audio> of its own |

### `Showing`

What the player shows at a frame of the edited timeline.

| Field | Type | What it is |
| --- | --- | --- |
| `clip` (required) | `Clip` | the main track's clip there |
| `source` (required) | `number` | the source frame to show |
| `rate` (required) | `number` | how fast the source runs there (the player's rate) |

### `SnapGrid`

Marks edges snap to that a plugin adds: a grid, one mark every `every` seconds from the start.

| Field | Type | What it is |
| --- | --- | --- |
| `id` (required) | `string` | its id: lowercase letters, digits and dashes |
| `title` (required) | `string` | what the snap menu says |
| `every` (required) | `number` | seconds between its marks, from 0.01 to 600 |

### `SoundTrack`

A track of a sound in the feed as its host lists it (raea-feed host/sounds.ts), in seconds of the render.

| Field | Type | What it is |
| --- | --- | --- |
| `id` (required) | `string` | its id in the sound's file |
| `name` (required) | `string` | what its head says |
| `kind` (required) | `"notes" \| "clips" \| "lines"` | what it holds: notes (a part played from notes), clips (files placed in time) or lines (words sung or said) |
| `bus` (required) | `string` | the bus it plays through |
| `clips` (required) | `{ id: string; from: number; to: number; label: string; src?: string; offset?: number; g…` | its clips, each with its span in seconds, what it is, its file, where in the file it starts and its level in dB |

### `SpeedPoint`

A point of a speed curve: how fast the source plays at a place in it (0 its first frame shown, 1 its last).

| Field | Type | What it is |
| --- | --- | --- |
| `at` (required) | `number` | where in the source, from 0 (its first frame shown) to 1 (its last) |
| `speed` (required) | `number` | how fast it plays there: 1 as recorded, 2 twice as fast, 0.5 half |

### `Target`

A frame an edge snaps to.

| Field | Type | What it is |
| --- | --- | --- |
| `at` (required) | `Frames` | its frame on the timeline |
| `what` (required) | `"clip" \| "playhead" \| "marker" \| "beat" \| "start"` | what is there, for the line drawn where it snapped |

### `TextStyle`

A way words are put on the picture: where they stand, and how they are drawn (CSS, as React writes it).

| Field | Type | What it is |
| --- | --- | --- |
| `kind` (required) | `string` | its kind, kept in a text clip's props |
| `title` (required) | `string` | what the bar says |
| `place` (required) | `"top" \| "center" \| "bottom"` | where the words stand on the picture |
| `style` (required) | `Record<string, string \| number>` | how they are drawn: type and colour, as React writes CSS (TEXT_PROPS lists what it may set) |

### `Timeline`

A whole timeline: its tracks top to bottom, its marks, its transitions and its three switches.

| Field | Type | What it is |
| --- | --- | --- |
| `fps` (required) | `number` | frames a second, which every Frames value counts in |
| `tracks` (required) | `Track[]` | its tracks, top to bottom |
| `markers` (required) | `Marker[]` | marks on the ruler |
| `transitions` (required) | `Transition[]` | transitions on cuts |
| `magnet` (required) | `boolean` | the main track holds no gap (P, TL4a) |
| `snapping` (required) | `boolean` | edges snap to cuts, the playhead, marks and beats (N, TL4b) |
| `linkage` (required) | `boolean` | what sits over a main clip moves with it (~, TL4c) |

### `Track`

A row of clips that don't overlap, with its head's switches.

| Field | Type | What it is |
| --- | --- | --- |
| `id` (required) | `string` | its own id, unique on the timeline |
| `kind` (required) | `TrackKind` | what it holds, which places it in the stack |
| `name` | `string` | what its head says (its kind's title when unset) |
| `locked` | `boolean` | its clips can't be picked or changed |
| `hidden` | `boolean` | left out of the picture |
| `muted` | `boolean` | left out of the sound |
| `height` | `number` | drawn this tall, in px |
| `clips` (required) | `Clip[]` | its clips, in start order |

### `TrackKind`

A track's kind, in the order they stack from the top: effect, text, overlay, main, sound. A plugin adds
its own; an unknown kind sorts with the overlays.

```ts
type TrackKind = "effect" | "text" | "overlay" | "main" | "sound" | (string & {})
```

### `TrackKindInfo`

A kind of track: what its head says, and where it stacks (with the tracks over the picture, or with the sound
under it). A clip on it is of that kind.

| Field | Type | What it is |
| --- | --- | --- |
| `kind` (required) | `string` | its kind, as a Track's kind names it |
| `title` (required) | `string` | what its head says |
| `under` | `boolean` | it stacks with the sound, under the main track |

### `Transition`

A transition on the cut between two clips that touch on one track: the cut is the end of clip `after`,
so it follows that cut wherever an edit moves it.

| Field | Type | What it is |
| --- | --- | --- |
| `id` (required) | `string` | its own id, unique on the timeline |
| `kind` (required) | `string` | its kind: fade, slide, wipe, zoom or a plugin's (TRANSITIONS) |
| `after` (required) | `string` | the clip whose end is the cut it sits on |
| `length` (required) | `Frames` | how long it takes, centred on the cut |

### `TransitionKind`

| Field | Type | What it is |
| --- | --- | --- |
| `kind` (required) | `string` | the component's name in the composition's code |
| `title` (required) | `string` | what the cut's menu says |

### `Value`

A property's value: a number, a list of numbers (a colour, a point) or a string.

```ts
type Value = number | number[] | string
```

<!-- generated:end -->
