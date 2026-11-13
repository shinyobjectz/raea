---
title: "raea"
summary: "The main entry: compositions, sequences, time, interpolation, springs, footage, sound and words."
order: 2
sources: ["sdk/raea/src/index.ts"]
---

`raea` is the path nearly every composition starts from. A file registers its compositions with `registerRoot`;
each `<Composition>` has a size, a frame rate, a length and a component. Inside it, `useCurrentFrame()` is the frame
being drawn, `<Sequence>` shifts time for what it holds, and `interpolate` and `spring` turn frames into values. The
entry also re-exports the footage, sound, word and state helpers, which have pages of their own.

```tsx
import { AbsoluteFill, Composition, Sequence, interpolate, registerRoot, useCurrentFrame } from "raea";

function Title({ text }: { text: string }) {
  const frame = useCurrentFrame();
  const opacity = interpolate(frame, [0, 12], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return <AbsoluteFill style={{ justifyContent: "center", alignItems: "center", fontSize: 120, color: "#fff", opacity }}>{text}</AbsoluteFill>;
}

registerRoot(() => (
  <Composition id="main" width={1080} height={1920} fps={30} durationInFrames={90} component={() => (
    <AbsoluteFill style={{ background: "#000" }}>
      <Sequence from={0} durationInFrames={45}><Title text="Hello" /></Sequence>
      <Sequence from={45} durationInFrames={45}><Title text="world" /></Sequence>
    </AbsoluteFill>
  )} />
));
```

See [Your first video](../../start/first-video.md) for the whole loop, and
[Compositions](../../concepts/compositions.md) for how frames, sequences and time work.

<!-- generated:begin by tools/manual-ref/sdk.ts from the SDK's exports and doc comments; edit outside this block -->

```ts
import { AbsoluteFill, ACTION_KEY, actionsIn, Animate } from "raea"
```

`raea` exports 18 components, 8 hooks, 55 functions, 9 values, 91 types. Source: [sdk/raea/src/index.tsx](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/src/index.tsx).

## Components

### `AbsoluteFill`

A box over the whole of its parent, laid out as a flex column; `style` is merged over that.

| Prop | Type | What it does |
| --- | --- | --- |
| `style` | `CSSProperties` | merged over the fill's own style (absolute, inset 0, a flex column) |
| `className` | `string` | Tailwind or CSS classes |
| `children` | `ReactNode` | what fills it |

Used in: [ui/images/image_node](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/image_node.tsx), [ui/images/ui_texture_slice_flip_and_tile](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_slice_flip_and_tile.tsx), [ui/images/image_node_resizing](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/image_node_resizing.tsx), [ui/images/ui_texture_atlas_slice](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_atlas_slice.tsx), [ui/images/ui_texture_slice](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_slice.tsx), [ui/images/ui_texture_atlas](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_atlas.tsx), [ui/layout/grid](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/layout/grid.tsx), [ui/layout/ghost_nodes](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/layout/ghost_nodes.tsx), and 98 more.

### `Animate`

<Animate>, memoised by its props' content (React skips a render whose props fingerprint matches the last). The
fingerprint is on it as `Animate.fingerprint` for a runtime's own bailout. Its declaration is the engine's
Animation (@bevy raea_core_animate::spec::Animation), typed from the registry (animate.gen.ts), sent as data-raea-anim and
read back as that type by sdk/react animate.rs.

Also takes the props of [`AnimateOwnProps`](animation.md#animateownprops).

### `Audio`

Sound in step with the frame; no picture. A muted or ended Audio is silent.

| Prop | Type | What it does |
| --- | --- | --- |
| `src` (required) | `string \| GeneratedAsset` | a file URL, or a generated asset from lyria() or sao() |

Also takes the props of [`MediaProps`](video.md#mediaprops).

### `Clock`

What time a composition is at: the frame, frame rate, size and length its hooks read, and its words, perception
and input. The renderer, the live view and tests put one around every composition; a composition never needs its
own.

| Prop | Type | What it does |
| --- | --- | --- |
| `frame` (required) | `number` | the composition frame to draw |
| `fps` (required) | `number` | frames a second |
| `width` (required) | `number` | in px |
| `height` (required) | `number` | in px |
| `durationInFrames` (required) | `number` | the composition's length in frames |
| `id` | `string` | the composition's id ("main" by default) |
| `defaultProps` | `Record<string, unknown>` | the props the composition is rendered with |
| `words` | `readonly TimelineWord[]` | the transcript, in composition frames, that captions and word cuts read |
| `perception` | `Perception` | what perception saw at this frame (faces, the safe area) |
| `input` | `InputTimeline` | a game's recorded input by frame |
| `children` | `ReactNode` | the composition |

### `Composition`

Declares a composition inside the root. It draws nothing itself; a renderer finds it through registerRoot.

| Prop | Type | What it does |
| --- | --- | --- |
| `id` (required) | `string` | the composition's id: letters, digits and dashes (`raea render --composition` picks it) |
| `component` (required) | `ComponentType<P>` | the React component it renders |
| `width` (required) | `number` | in px |
| `height` (required) | `number` | in px |
| `fps` (required) | `number` | frames a second |
| `durationInFrames` (required) | `number` | its length in frames |
| `defaultProps` | `P` | the props it's rendered with (JSON-like values only) |
| `edit` | `Edit` | a hand edit laid over the composition (docs/craft-system.md, L): written back by the app's timeline |

Used in: [ui/images/image_node](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/image_node.tsx), [ui/images/ui_texture_slice_flip_and_tile](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_slice_flip_and_tile.tsx), [ui/images/image_node_resizing](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/image_node_resizing.tsx), [ui/images/ui_texture_atlas_slice](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_atlas_slice.tsx), [ui/images/ui_texture_slice](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_slice.tsx), [ui/images/ui_texture_atlas](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_atlas.tsx), [ui/layout/grid](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/layout/grid.tsx), [ui/layout/ghost_nodes](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/layout/ghost_nodes.tsx), and 152 more.

### `Freeze`

Shows its children as they are at `frame`, whatever the parent's frame.

| Prop | Type | What it does |
| --- | --- | --- |
| `frame` (required) | `number` | the frame its children see, whatever the parent's frame |
| `children` | `ReactNode` | what's held |

### `Img`

An image; the renderer holds the frame until it has loaded and fails the render (naming src) if it can't.

Also takes the props of `ImgHTMLAttributes` (`alt`, `crossOrigin`, `decoding`, `fetchPriority`, `height`, `loading`, `referrerPolicy`, `sizes`, `src`, `srcSet`, `useMap`, `width`); `HTMLAttributes` (`defaultChecked`, `defaultValue`, `suppressContentEditableWarning`, `suppressHydrationWarning`, `accessKey`, `autoCapitalize`, `autoFocus`, `className`, `contentEditable`, `contextMenu`, `dir`, `draggable`, `enterKeyHint`, `hidden`, `id`, `lang`, `nonce`, `slot`, `spellCheck`, `style`, `tabIndex`, `title`, `translate`, `radioGroup`, `role`, `about`, `content`, `datatype`, `inlist`, `prefix`, `property`, `rel`, `resource`, `rev`, `typeof`, `vocab`, `autoCorrect`, `autoSave`, `color`, `itemProp`, `itemScope`, `itemType`, `itemID`, `itemRef`, `results`, `security`, `unselectable`, `popover`, `popoverTargetAction`, `popoverTarget`, `inert`, `inputMode`, `is`, `exportparts`, `part`); `AriaAttributes` (`aria-activedescendant`, `aria-atomic`, `aria-autocomplete`, `aria-braillelabel`, `aria-brailleroledescription`, `aria-busy`, `aria-checked`, `aria-colcount`, `aria-colindex`, `aria-colindextext`, `aria-colspan`, `aria-controls`, `aria-current`, `aria-describedby`, `aria-description`, `aria-details`, `aria-disabled`, `aria-dropeffect`, `aria-errormessage`, `aria-expanded`, `aria-flowto`, `aria-grabbed`, `aria-haspopup`, `aria-hidden`, `aria-invalid`, `aria-keyshortcuts`, `aria-label`, `aria-labelledby`, `aria-level`, `aria-live`, `aria-modal`, `aria-multiline`, `aria-multiselectable`, `aria-orientation`, `aria-owns`, `aria-placeholder`, `aria-posinset`, `aria-pressed`, `aria-readonly`, `aria-relevant`, `aria-required`, `aria-roledescription`, `aria-rowcount`, `aria-rowindex`, `aria-rowindextext`, `aria-rowspan`, `aria-selected`, `aria-setsize`, `aria-sort`, `aria-valuemax`, `aria-valuemin`, `aria-valuenow`, `aria-valuetext`); `DOMAttributes` (`children`, `dangerouslySetInnerHTML`, `onCopy`, `onCopyCapture`, `onCut`, `onCutCapture`, `onPaste`, `onPasteCapture`, `onCompositionEnd`, `onCompositionEndCapture`, `onCompositionStart`, `onCompositionStartCapture`, `onCompositionUpdate`, `onCompositionUpdateCapture`, `onFocus`, `onFocusCapture`, `onBlur`, `onBlurCapture`, `onChange`, `onChangeCapture`, `onBeforeInput`, `onBeforeInputCapture`, `onInput`, `onInputCapture`, `onReset`, `onResetCapture`, `onSubmit`, `onSubmitCapture`, `onInvalid`, `onInvalidCapture`, `onLoad`, `onLoadCapture`, `onError`, `onErrorCapture`, `onKeyDown`, `onKeyDownCapture`, `onKeyPress`, `onKeyPressCapture`, `onKeyUp`, `onKeyUpCapture`, `onAbort`, `onAbortCapture`, `onCanPlay`, `onCanPlayCapture`, `onCanPlayThrough`, `onCanPlayThroughCapture`, `onDurationChange`, `onDurationChangeCapture`, `onEmptied`, `onEmptiedCapture`, `onEncrypted`, `onEncryptedCapture`, `onEnded`, `onEndedCapture`, `onLoadedData`, `onLoadedDataCapture`, `onLoadedMetadata`, `onLoadedMetadataCapture`, `onLoadStart`, `onLoadStartCapture`, `onPause`, `onPauseCapture`, `onPlay`, `onPlayCapture`, `onPlaying`, `onPlayingCapture`, `onProgress`, `onProgressCapture`, `onRateChange`, `onRateChangeCapture`, `onSeeked`, `onSeekedCapture`, `onSeeking`, `onSeekingCapture`, `onStalled`, `onStalledCapture`, `onSuspend`, `onSuspendCapture`, `onTimeUpdate`, `onTimeUpdateCapture`, `onVolumeChange`, `onVolumeChangeCapture`, `onWaiting`, `onWaitingCapture`, `onAuxClick`, `onAuxClickCapture`, `onClick`, `onClickCapture`, `onContextMenu`, `onContextMenuCapture`, `onDoubleClick`, `onDoubleClickCapture`, `onDrag`, `onDragCapture`, `onDragEnd`, `onDragEndCapture`, `onDragEnter`, `onDragEnterCapture`, `onDragExit`, `onDragExitCapture`, `onDragLeave`, `onDragLeaveCapture`, `onDragOver`, `onDragOverCapture`, `onDragStart`, `onDragStartCapture`, `onDrop`, `onDropCapture`, `onMouseDown`, `onMouseDownCapture`, `onMouseEnter`, `onMouseLeave`, `onMouseMove`, `onMouseMoveCapture`, `onMouseOut`, `onMouseOutCapture`, `onMouseOver`, `onMouseOverCapture`, `onMouseUp`, `onMouseUpCapture`, `onSelect`, `onSelectCapture`, `onTouchCancel`, `onTouchCancelCapture`, `onTouchEnd`, `onTouchEndCapture`, `onTouchMove`, `onTouchMoveCapture`, `onTouchStart`, `onTouchStartCapture`, `onPointerDown`, `onPointerDownCapture`, `onPointerMove`, `onPointerMoveCapture`, `onPointerUp`, `onPointerUpCapture`, `onPointerCancel`, `onPointerCancelCapture`, `onPointerEnter`, `onPointerLeave`, `onPointerOver`, `onPointerOverCapture`, `onPointerOut`, `onPointerOutCapture`, `onGotPointerCapture`, `onGotPointerCaptureCapture`, `onLostPointerCapture`, `onLostPointerCaptureCapture`, `onScroll`, `onScrollCapture`, `onScrollEnd`, `onScrollEndCapture`, `onWheel`, `onWheelCapture`, `onAnimationStart`, `onAnimationStartCapture`, `onAnimationEnd`, `onAnimationEndCapture`, `onAnimationIteration`, `onAnimationIterationCapture`, `onToggle`, `onBeforeToggle`, `onTransitionCancel`, `onTransitionCancelCapture`, `onTransitionEnd`, `onTransitionEndCapture`, `onTransitionRun`, `onTransitionRunCapture`, `onTransitionStart`, `onTransitionStartCapture`).

### `InputRegion`

Its children hear the pointer in their own box (x, y, w, h in points; the page's pixels are `scale` points each), as
a panel does when it's a page of its own: a press inside the box is the box's until it's let go. Outside the box,
and while another box holds a press, they see no pointer; keys reach every box.

| Prop | Type | What it does |
| --- | --- | --- |
| `x` (required) | `number` | the box's left edge, composition px |
| `y` (required) | `number` | its top edge, composition px |
| `w` (required) | `number` | its width, px |
| `h` (required) | `number` | its height, px |
| `scale` | `number` | page px over the box's own points (1): the pointer is divided by it |
| `children` | `ReactNode` | the game or panel that sees the pointer as its own |

### `Loop`

Repeats its children every `durationInFrames` frames, `times` times (forever by default).

| Prop | Type | What it does |
| --- | --- | --- |
| `durationInFrames` (required) | `number` | the length of one loop, in frames |
| `times` | `number` | how many times it plays (forever by default) |
| `name` | `string` | its label in the timeline and the render manifest |
| `layout` | `SequenceLayout` | "absolute-fill" (default) or "none", as a Sequence's |
| `style` | `CSSProperties` | the wrapping box's style |
| `className` | `string` | the wrapping box's classes |
| `children` | `ReactNode` | what loops |

### `Mix`

The master: every sound in the composition brought to `lufs` integrated loudness (-14) under a true-peak limiter at
`ceiling` dBTP (-1.5), as the platforms play back. Put one in the composition, around the soundtrack or anywhere.

| Prop | Type | What it does |
| --- | --- | --- |
| `lufs` | `number` | integrated loudness the mix is brought to (EBU R128, LUFS) |
| `ceiling` | `number` | the limiter's true-peak ceiling (dBTP) |
| `children` | `ReactNode` | what it masters, when it's written around the soundtrack (<Mix><Voice /><Bed /></Mix>): rendered as they are |

### `Nudge`

Places its child: an element gets the move in front of its own transform and the fade times its own opacity; a
component, whose box it can't reach, is placed inside a full-frame layer.

| Prop | Type | What it does |
| --- | --- | --- |
| `x` | `number` | how far to move it, in the composition's pixels |
| `y` | `number` | how far to move it down, in the composition's pixels |
| `scale` | `number` | how much bigger (2) or smaller (0.5), about its centre |
| `rotate` | `number` | a turn, in degrees, about its centre |
| `opacity` | `number` | 0 (gone) to 1 (as it was), times its own opacity |
| `children` | `ReactNode` | the part it places: an element, or a component, which moves inside a full-frame layer |

### `Overlay`

Its children only in a render asked for the overlay `name`.

| Prop | Type | What it does |
| --- | --- | --- |
| `name` (required) | `string` | the overlay's name, as the render asks for it |
| `children` | `ReactNode` | what the overlay draws |

### `Region`

A box at the band its name stands for, clear of the face perception found at this frame.

| Prop | Type | What it does |
| --- | --- | --- |
| `name` | `RegionName` | the band: "auto" (default) takes the larger of top and bottom; top and bottom stay clear of the face; center is the middle third; full is the safe area |
| `margin` | `number` | the gap kept from the face and the sides, as a share of the width (0.06) |
| `style` | `CSSProperties` | merged under the box's own placement |
| `className` | `string` | Tailwind or CSS classes |
| `children` | `ReactNode` | what goes in the band; `useRegion()` reads its box |

### `Sequence`

Shifts time by `from` and shows its children only for `durationInFrames` frames.

| Prop | Type | What it does |
| --- | --- | --- |
| `from` | `number` | the parent frame its frame 0 falls on (0) |
| `durationInFrames` | `number` | how many frames it shows its children (forever by default) |
| `name` | `string` | its label in the timeline and the render manifest (`data-sequence`) |
| `layout` | `SequenceLayout` | "absolute-fill" (default) wraps the children in a full-size box; "none" adds no box |
| `width` | `number` | the width its children see, px (the parent's by default) |
| `height` | `number` | the height its children see, px (the parent's by default) |
| `style` | `CSSProperties` | the wrapping box's style |
| `className` | `string` | the wrapping box's classes |
| `children` | `ReactNode` | what plays in it, at its own local time |

Used in: [manual: hello](https://github.com/shinyobjectz/raea/blob/main/docs/manual/examples/hello.tsx).

### `Series`

Plays its Series.Sequence children back to back, each shifted by its own offset.

| Prop | Type | What it does |
| --- | --- | --- |
| `children` | `ReactNode` | the Series.Sequence slots, played back to back |

### `SongPlot`

The map drawn for a human to check: section bands named, the energy curve with builds, stops, dips and drops on
it, hits, the feature lanes, and the pacing (cuts per bar). `raea song FILE --plot OUT.png` renders it.

| Prop | Type | What it does |
| --- | --- | --- |
| `map` (required) | `SongMap` | the song's map, from \`raea song\` |
| `width` | `number` | in px |
| `height` | `number` | in px |
| `title` | `string` | the heading (the song's name, tempo, metre and length by default) |

### `Stagger`

Each child in its own <Animate>, entrances `each` frames apart (or by grid distance). The same content as last
render gives the same element back.

Also takes the props of [`StaggerProps`](animation.md#staggerprops) (`each`, `grid`, `exitEach`); [`AnimateOwnProps`](animation.md#animateownprops).

### `Video`

A video clip in step with the frame; `data-source-time` is the source time (seconds) to show.

| Prop | Type | What it does |
| --- | --- | --- |
| `framing` | `"face"` | ours: frame the take on the largest face |
| `punchIn` | `number` | ours: multiplies the framing's scale (1.15 is the usual) |

Also takes the props of [`MediaProps`](video.md#mediaprops).

## Hooks

### `useCurrentFrame`

The frame being rendered, counted from the start of the nearest enclosing Sequence.

```ts
function useCurrentFrame(): number
```

Used in: [ui/scroll_and_overflow/overflow_transform](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/scroll_and_overflow/overflow_transform.tsx), [ui/render_ui_to_texture](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/render_ui_to_texture.tsx), [ui/ui_material](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/ui_material.tsx), [ui/styling/gradients](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/styling/gradients.tsx), [ui/text/text_debug](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/text/text_debug.tsx), [ui/text/text](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/text/text.tsx), [ui/text/font_atlas_debug](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/text/font_atlas_debug.tsx), [3d/fog_volumes](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/fog_volumes.tsx), and 46 more.

### `useGameEvents`

The frames, in order, at which `isEvent(before, after)` holds: `after` is the state `step` makes at frame f from
`before`, the state at f - 1. It runs the same (kept) fold as useGameState through the composition's last frame,
counted from the nearest enclosing Sequence, so sound can be cued to what happens in the game. While a person plays
(live input) it runs only through this frame: what comes next hasn't been played yet. The list is read-only (frozen):
the same one is handed out while nothing new happened, so copy it before changing it.

```ts
function useGameEvents<S>(initial: S, step: Step<S>, isEvent: (before: S, after: S) => boolean): number[]
```

### `useGameState`

The game's state at this frame: `initial` at frame 0, then `step(state, input, g)` applied for each earlier frame g
in order. Frames count from the nearest enclosing Sequence, so a game starts when its Sequence does.

```ts
function useGameState<S>(initial: S, step: Step<S>): S
```

Used in: [shader/shader_prepass](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/shader/shader_prepass.tsx), [shader/sprite_material](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/shader/sprite_material.tsx), [manual: squash](https://github.com/shinyobjectz/raea/blob/main/docs/manual/examples/squash.tsx).

### `useInput`

This frame's input ({keys: []} when there was none). Its `gamepads` are Bevy's (bevy_gilrs's Gamepad entities, read
by engine/render's gamepad_keys):

Bevy: `bevy_input::gamepad::Gamepad`.

```ts
function useInput(): FrameInput
```

Used in: [shader/shader_prepass](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/shader/shader_prepass.tsx), [shader/sprite_material](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/shader/sprite_material.tsx).

### `useRegion`

The Region this is rendered inside: its name and box in composition px.

```ts
function useRegion(): RegionBox
```

### `useSong`

Where the song is at the current frame (frame 0 of the enclosing Sequence is `offset` seconds into the song), with
the map bound to the composition's fps as `song`.

```ts
function useSong(map: SongMap, options?: UseSongOptions): SongMoment & { song: Song; }
```

### `useVideoConfig`

The settings of the composition being rendered (inside a Sequence, its width, height and length win).

```ts
function useVideoConfig(): VideoConfig
```

Used in: [manual: title](https://github.com/shinyobjectz/raea/blob/main/docs/manual/examples/title.tsx), [manual: turntable](https://github.com/shinyobjectz/raea/blob/main/docs/manual/examples/turntable.tsx).

### `useWords`

Every word heard on the timeline, in the frames of the nearest enclosing Sequence.

```ts
function useWords(): TimelineWord[]
```

## Functions

### `actionsIn`

The actions handed in this frame, in order.

```ts
function actionsIn(input: FrameInput): { name: string; args: any; }[]
```

### `applyActions`

`state` with this frame's handed-in actions run on it, in order (an unknown name is skipped).

```ts
function applyActions<S>(actions: PanelActions<S>, state: S, input: FrameInput): S
```

### `cancelRender`

Fail the render: the error goes where a composition's throw goes (a data-raea-error the render reports and stops on).

```ts
function cancelRender(err: unknown): void
```

### `continueRender`

Release a hold delayRender opened; the last one released wakes the frame. A handle already released does nothing.

```ts
function continueRender(handle: number): void
```

### `curveFn`

A curve name as a function (the engine's easing::Easing::parse, the same names).

```ts
function curveFn(name: string, fps?: number): (t: number) => number
```

### `delay`

The same move later by `frames`.

```ts
function delay(m: MoveLike, frames: number): Move
```

### `delayRender`

Hold the frame until `continueRender(handle)`. `label` names it if it's still open when its time runs out.

```ts
function delayRender(label?: string, options?: { timeoutInMilliseconds?: number; } | undefined): number
```

### `fade`

opacity 0 to 1 over `frames` (6).

```ts
function fade(o?: { frames?: number; ease?: Curve; }): Move
```

### `feedIn`

What the app tells the panels, by name ("transcript", "object"); undefined until it has.

```ts
function feedIn<T = any>(input: FrameInput, what: string): T | undefined
```

### `fill`

a bar filling: width 0 to `to` (100%) over `frames` (24), out-cubic.

```ts
function fill(o?: { to?: number; frames?: number; unit?: "px" | "%"; ease?: Curve; }): Move
```

### `findPhrase`

Finds the composition frame where `phrase` is said in a transcript.

```ts
function findPhrase(words: readonly HeardWord[], phrase: string, options?: WordOptions): number
```

### `fish`

Retired on 2026-11-05: Fish narration is no longer part of raea. Narrate with `<Speech text start>` from
raea/speech (Pocket TTS, on this computer), or make a line with a `raea gen` step on `fish:<model>` and your own
FISH_API_KEY. Kept as a name only, so an older edit says what to use instead of failing to bundle.

```ts
function fish(text: string, _options?: Record<string, unknown>): never
```

### `getCompositions`

The compositions the registered root declares, in order.

```ts
function getCompositions(): RegisteredComposition[]
```

### `gridDelay`

A grid item's distance from an origin, in steps (raea-animate's StaggerPattern::Grid, the same rule).

```ts
function gridDelay(cols: number, rows: number, from: GridFrom, index: number): number
```

### `inState`

Whether `name` is active in the snapshot (the leaf or any state around it), as XState's `state.matches`.

```ts
function inState<N extends string>(snap: Snapshot<N>, name: N): boolean
```

### `interpolate`

`x` mapped from the `input` range onto the `output` range, piece by piece: `input` rises, and the two have the same
length. `options.easing` shapes each piece; `extrapolateLeft` and `extrapolateRight` say what happens outside the
range (`extend`, the default, carries on; `clamp` holds the end value; `identity` returns `x`; `wrap` repeats).

```ts
function interpolate(x: number, input: readonly number[], output: readonly number[], options?: InterpolateOptions): number
```

Used in: [manual: turntable](https://github.com/shinyobjectz/raea/blob/main/docs/manual/examples/turntable.tsx), [manual: hello](https://github.com/shinyobjectz/raea/blob/main/docs/manual/examples/hello.tsx).

### `isGeneratedAsset`

Whether a value is a generated asset (what sao, tokensynthNote and lyria return), not a file path.

```ts
function isGeneratedAsset(v: unknown): v is GeneratedAsset
```

### `keyEdges`

The input edges between two frames' keys, as events for a machine's `on`: "keydown:K" for each key pressed this
frame, "keyup:K" for each released (keep last frame's keys in the game state to have `prev`).

```ts
function keyEdges(prev: readonly string[] | undefined, now: readonly string[] | undefined): string[]
```

### `lyria`

Music an older edit generated (30 seconds by default). Retired: a take already made plays, a new one is refused.

```ts
function lyria(prompt: string, options?: { seconds?: number; }): GeneratedAsset
```

### `machine`

A machine for contexts of type C, stepped with an env of type E (a game's frame input and world, a workflow's facts).
Curried so the state names are inferred from the definition while C and E are given:
`machine<Player, Env>()({ id, initial, states })`. A transition to a state that doesn't exist, or a guard reading a
field C lacks, fails to compile; what types can't hold (an initial that isn't a substate, unreachable states, dead
ends) throws here, when the module loads.

```ts
function machine<C, E = unknown>(): <const N extends string, const K extends string = "sm">(config: MachineConfig<C, E, N, K>) => Machine<C, E, N, K>
```

### `measureSpring`

How many frames a spring with these settings takes to settle (before any stretching).

```ts
function measureSpring(opts: { fps: number; config?: SpringConfig; durationRestThreshold?: number; }): number
```

### `move`

A move from its tracks, by property: `move({ opacity: track({ 0: 0, 6: 1 }), y: track({ 0: 30, 6: 0 }) })`. Its
length is its last key. The named moves (rise, pop and the rest) are built with it.

```ts
function move(tracks: Record<string, TrackSpec<string | number> | TrackSpec<string | number>[] | Keys<string | number>>): Move
```

### `overlays`

The overlays this render was asked for ([] in a clean render and in live play).

```ts
function overlays(): string[]
```

### `parallel`

Every step at once.

```ts
function parallel(...steps: Step[]): Group
```

### `pop`

scale `from` (0.6), `peak` (1.08) at `frames`/2, 1 at `frames` (8); in over 3 frames.

```ts
function pop(o?: { from?: number; peak?: number; frames?: number; ease?: Curve; }): Move
```

### `proofBoxes`

Checks measured part boxes; returns one fault per problem, empty when all is well.

```ts
function proofBoxes({ width, height, face, parts }: ProofInput): string[]
```

### `propsFingerprint`

A props object's fingerprint: what <Animate> and <Stagger> compare to know a render would give the same element,
and what a runtime's bailout can key on (`Animate.fingerprint`).

```ts
function propsFingerprint(props: object): number
```

### `pushIn`

a slow push-in on footage: scale 1 to 1 + `amount` (0.04) across `frames` (90), linear.

```ts
function pushIn(o?: { amount?: number; frames?: number; }): Move
```

### `regionBand`

Works out the vertical band each name stands for, given perception and the composition size.

```ts
function regionBand(name: RegionName, width: number, height: number, perception: Perception, margin?: number): Band
```

### `registerRoot`

Names the root component the renderer starts from, and records its compositions. Call it once.

```ts
function registerRoot(given: ReactElement<unknown, string | JSXElementConstructor<any>> | ComponentType): void
```

Used in: [ui/images/image_node](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/image_node.tsx), [ui/images/ui_texture_slice_flip_and_tile](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_slice_flip_and_tile.tsx), [ui/images/image_node_resizing](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/image_node_resizing.tsx), [ui/images/ui_texture_atlas_slice](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_atlas_slice.tsx), [ui/images/ui_texture_slice](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_slice.tsx), [ui/images/ui_texture_atlas](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_atlas.tsx), [ui/layout/grid](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/layout/grid.tsx), [ui/layout/ghost_nodes](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/layout/ghost_nodes.tsx), and 152 more.

### `replayDecider`

A decider that replays recorded decisions: `log[machine][n]` is the edge chosen at that machine's step n (missing:
wait). How a model's or a person's choices, taken outside the step, re-enter it exactly.

```ts
function replayDecider(log: Record<string, Record<number, number | { edge: number; evidence?: unknown; }>>, name?: string): Decider
```

### `reverse`

A move played backwards: it leaves the way it came (each curve reversed, so an out-curve becomes the matching
in-curve). An exit is its entrance reversed.

```ts
function reverse(m: MoveLike): Move
```

### `rise`

y offset `distance` to 0 over `frames` (30 px over 6), fading in over `fade` (5).

```ts
function rise(o?: { distance?: number; frames?: number; fade?: number; ease?: Curve; }): Move
```

### `sampleAnimation`

What an Animation sets at composition frame `frame`: page style keys, a counter's text, or 3D props
(raea-animate's Sampled, here for plain bun).

```ts
function sampleAnimation(a: Animation, frame: number): { style: Record<string, string | number>; text?: string; values: Record<string, string | number>; }
```

### `sao`

A sound effect to generate (2 seconds by default).

```ts
function sao(prompt: string, options?: { seconds?: number; }): GeneratedAsset
```

### `sequence`

Each step when the one before it ends (`gap` frames apart: sequenceGap).

```ts
function sequence(...steps: Step[]): Group
```

### `sequenceGap`

Each step when the one before it ends, `gap` frames later (a negative gap overlaps them).

```ts
function sequenceGap(gap: number, ...steps: Step[]): Group
```

### `slam`

a hit: scale `from` (1.3), `dip` (0.96) at `frames`/2, 1 at `frames` (8); in over 2 frames.

```ts
function slam(o?: { from?: number; dip?: number; frames?: number; ease?: Curve; }): Move
```

### `slide`

arrives from a side: `distance` px (30) over `frames` (7), fading in over 5. `from` is where it comes from.

```ts
function slide(o?: { from?: Dir; distance?: number; frames?: number; ease?: Curve; }): Move
```

### `song`

A song map bound to a frame rate (and where frame 0 sits in the song): frames from song times, and the reverse.

```ts
function song(map: SongMap, options: SongOptions): Song
```

### `songAt`

Where the song is at `t` seconds: a pure function of the map.

```ts
function songAt(map: SongMap, t: number): SongMoment
```

### `spring`

A damped spring's value at `frame`: from `from` (0) to `to` (1), started `delay` frames late, by its `config`
(mass 1, damping 10, stiffness 100). It overshoots and settles unless `overshootClamping` is set; `durationInFrames`
stretches it to end at that frame. `measureSpring` says how long one takes to settle.

```ts
function spring(opts: SpringOptions): number
```

Used in: [manual: title](https://github.com/shinyobjectz/raea/blob/main/docs/manual/examples/title.tsx).

### `stagger`

The parts `each` frames apart, each with `m`. With `grid`, delays spread by grid distance from an origin.

```ts
function stagger(parts: string[], each: number, m: MoveLike, grid?: { cols: number; rows?: number; from?: GridFrom; } | undefined): Group
```

### `stateOf`

The active leaf of a snapshot (undefined before the first step).

```ts
function stateOf<N extends string>(snap: Snapshot<N>): N | undefined
```

### `staticFile`

The URL of a file in the space's media folder, each path segment URL-encoded.

```ts
function staticFile(path: string): string
```

Used in: [ui/images/image_node](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/image_node.tsx), [ui/images/ui_texture_slice_flip_and_tile](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_slice_flip_and_tile.tsx), [ui/images/image_node_resizing](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/image_node_resizing.tsx), [ui/images/ui_texture_atlas_slice](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_atlas_slice.tsx), [ui/images/ui_texture_slice](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_slice.tsx), [ui/images/ui_texture_atlas](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/images/ui_texture_atlas.tsx), [ui/layout/grid](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/layout/grid.tsx), [ui/layout/ghost_nodes](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/layout/ghost_nodes.tsx), and 100 more.

### `stepAll`

One frame for each entity of a list, each carrying its own machine (one per enemy), in order.

```ts
function stepAll<C, E, N extends string, K extends string>(m: Machine<C, E, N, K>, entities: readonly C[], env: E, opts?: readonly string[] | StepOptions): C[]
```

### `strike`

a strike-through line drawn: width 0 to 100% over `frames` (8).

```ts
function strike(o?: { frames?: number; ease?: Curve; }): Move
```

### `tick`

a number counting from `from` (0) to `to` over `frames` (24), written as the element's text: `digits` decimals,
`separator` between its thousands ("," or true writes 12,345; none by default), `prefix` and `suffix` around it
("$", " pts"). Hidden until it starts, then in over `fade` frames (2; 0 shows it at `from` the whole time).

```ts
function tick(o: { to: number; from?: number; frames?: number; digits?: number; ease?: Curve; prefix?: string; suffix?: string; separator?: string | boolean; fade?: number; }): Move
```

### `tickClocks`

Tells every mounted Clock's frame readers that the frame is about to move on, in the same batch as the update that
moves it, so they render in that one commit. A host that steps frames back to back calls it with its frame update:
else each Clock tells them as its frame commits, a second commit each frame, and React stops a chain of more than 50
(raea's bake steps a range of frames as one chain: many_cubes stalled at frame 26).

```ts
function tickClocks(): void
```

### `timeline`

Steps placed in time (a sequence of them). Each part's start is decided here, once.

```ts
function timeline(...steps: Step[]): Plan
```

### `toMove`

A name, a move or several moves together, as one move.

```ts
function toMove(m: MoveLike): Move
```

### `track`

A track: `track({ 0: 30, 6: 0 })` is a 30 px rise over 6 frames, out-cubic by default.

```ts
function track<V = string | number>(keys: Keys<V>, ease?: Curve | Curve[], opts?: Omit<TrackSpec<string | number>, "ease" | "keys">): TrackSpec<V>
```

### `transitionOf`

The transition a step took, read from two snapshots of one machine (null when none fired): how a trace that only
sees states (a playtest over a game's pure fold) recovers the TransitionRecord a sink would have had, minus evidence.

```ts
function transitionOf(m: AnyMachine, before: Snapshot<string> | undefined, after: Snapshot<string>): TransitionRecord | null
```

### `wipe`

a reveal wiping across from `from` (left) over `frames` (8), in-out-cubic, by clipping.

```ts
function wipe(o?: { from?: Dir; frames?: number; ease?: Curve; }): Move
```

### `word`

The composition frame where `phrase` is said. Call it while rendering (it reads the Clock).

```ts
function word(phrase: string, options?: WordOptions): number
```

## Values

### `ACTION_KEY`

The prefix the app puts on an action handed in as input: a key `action:NAME ARGS_JSON`, held for one frame.

### `curve`

Curve helpers: `curve.bezier(0.05, 0.7, 0.1, 1)`, `curve.spring({ damping: 15, stiffness: 120 })`.

### `Easing`

The easing curves, each a function of progress 0..1: `linear`, `quad`, `cubic`, `poly(n)`, `sin`, `circle`, `exp`,
`back`, `elastic`, `bounce` and `bezier(x1, y1, x2, y2)`, eased in as they are; `Easing.out(f)` and `Easing.inOut(f)`
turn one round. Pass one as `interpolate`'s `easing`.

Used in: [manual: turntable](https://github.com/shinyobjectz/raea/blob/main/docs/manual/examples/turntable.tsx).

### `FEED_KEY`

The prefix of the keys the app hands every panel for what it knows: `feed:transcript` (the conversation's last lines,
[{ who: "you" | "agent" | "tool" | "note", text, live }]), `feed:object` (the object on the stage: { name, kind }) and
`feed:stage` (what the object publishes with data-feed-publish, as a panel's publishes reach the object) and
`feed:player` (the stage's player: { frame, frames, rate, playing }; frames is 0 for a game, which runs on).
Handed in only on the frame it changes (and when the panel opens), so keep it in state:
`useGameState([], (s, input) => feedIn(input, "transcript") ?? s)`. A panel standing in for the terminal sends by
publishing { send: { n, text } } (a higher n is a new message); a panel drives the player by publishing
{ control: { n, do: "play" | "pause" | "seek", frame } } the same way.

### `firstDecider`

The default decider: the first candidate.

### `HOLD_TIMEOUT_MS`

How long a hold may stay open before the render fails naming it, unless delayRender is told otherwise.

### `MEDIA_ROOT`

Where the space's media folder is served from.

### `moves`

The named moves by name, as `enter="rise"` finds them.

### `SPACE_KEY`

The prefix of the key the app holds on the stage for each panel that publishes (`data-feed-publish` on an element):
`space:PANEL JSON`, held while the value stands, so the object on the stage reads what the panels picked.

## Types

### `AbsoluteFillProps`

The props of [`AbsoluteFill`](#absolutefill), tabled there.

### `Action`

An action: a pure function returning the new context (never a changed old one).

```ts
type Action = (ctx: C, env: E) => C
```

### `Actions`

```ts
type Actions = Action<C, E> | readonly Action<C, E>[]
```

### `Analysis`

| Field | Type | What it is |
| --- | --- | --- |
| `id` (required) | `string` | the machine's id |
| `states` (required) | `{ name: N; parent: N \| null; depth: number; kind: "leaf" \| "compound" \| "final"; initia…` | every state with its parent, depth, kind and whether it's an initial state |
| `edges` (required) | `Edge<N>[]` | every transition |
| `reachable` (required) | `N[]` | states some sequence of transitions reaches from the initial state (guards assumed satisfiable) |
| `unreachable` (required) | `N[]` | states no sequence of transitions reaches |
| `deadEnds` (required) | `N[]` | non-final leaves with no transition out (their own or an enclosing state's): once entered, never left |

### `AnimateOwnProps`

| Field | Type | What it is |
| --- | --- | --- |
| `enter` | `MoveLike` | the entrance: a move's name, a move, or several together |
| `exit` | `MoveLike` | the exit, played at `exitAt` (an entrance reversed: `exit="rise"` sinks back out) |
| `animate` | `Record<string, TrackSpec \| TrackSpec[] \| Keys> \| Move` | free tracks by property, in local frames: `{ backgroundColor: track({ 0: "#fff", 20: "#000" }) }` |
| `at` | `number` | local frame the entrance starts (0) |
| `exitAt` | `number` | local frame the exit starts |
| `timeline` | `Plan` | a timeline's plan and this element's part in it: the part's move, at its start (plus `at`) |
| `part` | `string` | the part in \`timeline\` whose move this element plays |
| `mount` | `"during" \| "always"` | "always" (default): in the tree, and its layout, the whole time (invisible before an entrance that fades in); "during": only from the entrance's start until the exit ends (it takes its space when it arrives) |
| `as` | `string` | the element: "div" by default |
| `style` | `CSSProperties` | inline style |
| `className` | `string` | Tailwind or CSS classes |
| `children` | `ReactNode` | what moves |

### `AnimateProps`

```ts
type AnimateProps = AnimateOwnProps & { [attr: string]: unknown }
```

### `Animation`

One element's declared animation.

Rust: `raea_core_animate::spec::Animation`.

| Field | Type | What it is |
| --- | --- | --- |
| `origin` (required) | `number` | the composition frame the element's local frame 0 falls on (where its Sequence starts) |
| `freeze` (required) | `number \| null` | a local frame to hold at whatever the composition's frame (inside a Freeze) |
| `fps` (required) | `number` | the frames per second its frames count; springs measure their settle time with it (0: the clock's) |
| `tracks` (required) | `(AnimTrack)[]` | the element's keyframed properties, one track each |
| `show_from` (required) | `number \| null` | the local frame it's shown from, when it shows only for a window (mount="during"); before it, display none |
| `show_until` (required) | `number \| null` | the local frame it's hidden from again |

### `AnimKey`

One key: a value at a local frame, and the curve from it to the next key.

Rust: `raea_core_animate::spec::AnimKey`.

| Field | Type | What it is |
| --- | --- | --- |
| `at` (required) | `number` | the local frame it's at |
| `value` (required) | `number` | a number property's value there |
| `color` (required) | `string` | a colour property's value (CSS text); empty for numbers |
| `ease` (required) | `string` | the curve to the next key (easing::Easing::parse); empty is linear |

### `AnimTrack`

Keyframes on one property.

Rust: `raea_core_animate::spec::AnimTrack`.

| Field | Type | What it is |
| --- | --- | --- |
| `prop` (required) | `string` | the property it keys: opacity, x, y, z, scale, rotate, width, height, blur, clip, a colour, `text`, or another number |
| `keys` (required) | `(AnimKey)[]` | its keyframes in frame order |
| `unit` (required) | `string` | a length's unit: "px" (when empty) or "%" |
| `repeat` (required) | `string` | past the last key: "" or "hold", "loop", "pingpong" (for ever), or "loop:N" / "pingpong:N" |
| `digits` (required) | `number` | decimals a `text` number is written with |
| `prefix` (required) | `string` | written before and after a `text` number (a counter's "$" and " pts") |
| `suffix` (required) | `string` | written after a `text` number |
| `separator` (required) | `string` | between a `text` number's thousands ("," writes 12,345); empty: none |

### `AnyMachine`

```ts
type AnyMachine = Machine<any, any, string, string>
```

### `AssetKind`

```ts
type AssetKind = "lyria" | "sao" | "tokensynth"
```

### `AudioProps`

The props of [`Audio`](#audio), tabled there.

### `ChoicePoint`

A choice point as a decider sees it: which machine, where, and the candidates whose guards hold.

| Field | Type | What it is |
| --- | --- | --- |
| `machine` (required) | `string` | the machine's id |
| `state` (required) | `string` | the state whose `choose` list this is, and the active leaf |
| `leaf` (required) | `string` | the active leaf state |
| `n` (required) | `number` | the machine's step |
| `candidates` (required) | `{ edge: number; to: string; description?: string }[]` | the candidates whose guards hold, in their order (each an edge of machine.edges) |
| `ctx` (required) | `C` | the context the machine drives |
| `env` (required) | `E` | whatever the step was given as env |

### `ClockProps`

The props of [`Clock`](#clock), tabled there.

### `CompositionProps`

The props of [`Composition`](#composition), tabled there.

### `Curve`

```ts
type Curve = CurveName | (string & {})
```

### `CurveName`

Curve names. Any of these, or `bezier(x1,y1,x2,y2)`, `steps(n)`, `spring(mass,damping,stiffness)`,
`reverse(<curve>)`.

```ts
type CurveName = | "linear" | "ease" | `${"in" | "out" | "in-out"}-${"quad" | "cubic" | "quart" | "quint" | "sine" | "expo" | "circ" | "back" | "elastic" | "bounce"}`
```

### `CutOptions`

| Field | Type | What it is |
| --- | --- | --- |
| `by` | `"section" \| "bar"` | "section" (default) takes each section's rate, steady across it; "bar" each bar's own |
| `scale` | `number` | multiplies every rate (2: twice as many cuts), kept on the ladder 0.25..4 |
| `drops` | `boolean` | cut on every drop (default true) |
| `stops` | `boolean` | cut on a stop's hits (default true) |
| `builds` | `boolean` | double the rate through a build's second half (default true) |
| `minGap` | `number` | cuts closer than this many frames are merged, the earlier kept (default a quarter second) |
| `from` | `number` | first frame (inclusive), default 0 |
| `to` | `number` | last frame (exclusive), default the song's end |

### `Decider`

Picks a transition at a choice point. Called inside the step, so it must answer synchronously: a model or a person
is asked outside the step and their answer replayed (`replayDecider`). Deterministic deciders keep a fold pure.

| Field | Type | What it is |
| --- | --- | --- |
| `name` (required) | `string` | its name in the trace's `by` column |
| `decide` (required) | `(point: ChoicePoint<unknown, unknown>) => Decision` | picks a candidate at a choice point, synchronously |

### `Decision`

What a decider answers: the edge it picks (one of the candidates' `edge`), or null to wait this step; `evidence` is
whatever supports the choice (a bot's score, a model's probabilities), recorded beside the transition.

| Field | Type | What it is |
| --- | --- | --- |
| `edge` (required) | `number \| null` | the edge picked, one of the candidates' `edge`; null waits this step |
| `evidence` | `unknown` | what supports the choice (a bot's score, a model's probabilities), recorded with the transition |

### `EasingFn`

```ts
type EasingFn = (p: number) => number
```

### `Edge`

| Field | Type | What it is |
| --- | --- | --- |
| `index` (required) | `number` | its place in `machine.edges` |
| `from` (required) | `N` | the state it leaves |
| `to` (required) | `N` | the state it enters |
| `kind` (required) | `"always" \| "after" \| "on" \| "done" \| "choose"` | what fires it: a guard polled each step (always), a delay (after), an event (on), the substate finishing (done) or a decider (choose) |
| `event` | `string` | the event an `on` edge waits for |
| `after` (required) | `number` | steps in `from` before it may fire |
| `guarded` (required) | `boolean` | whether it has a guard (`when`) |
| `description` | `string` | what the transition means, from its config |

### `Edit`

A hand edit: `segments` are the composition's own frame ranges, [from, to), played one after another, so a trim,
a split, a cut word or a moved span is a change to this list; the length becomes their sum.

| Field | Type | What it is |
| --- | --- | --- |
| `segments` | `readonly (readonly [number, number])[]` | the composition's own frame ranges, [from, to), played one after another |

### `ExtrapolateMode`

```ts
type ExtrapolateMode = "extend" | "clamp" | "identity" | "wrap"
```

### `FrameInput`

One frame's input for a game.

| Field | Type | What it is |
| --- | --- | --- |
| `keys` (required) | `string[]` | the keys held (KeyboardEvent.key names: "ArrowLeft", "a", " ") |
| `pointer` | `{ x: number; y: number; down: boolean }` | the primary pointer in composition px, and whether it's pressed |
| `buttons` | `string[]` | the mouse buttons held besides the primary one: "Right", "Middle", "Back", "Forward" (Bevy's MouseButton names) |
| `wheel` | `{ x: number; y: number }` | the wheel turned this frame, in CSS px, down and right positive (the DOM's deltaX and deltaY) |
| `gamepads` | `{ buttons: string[]; axes: Record<string, number> }[]` | each connected gamepad: the buttons held and the sticks' and triggers' axes, by Bevy's GamepadButton and GamepadAxis names ("South", "DPadUp", "RightTrigger2"; "LeftStickX" -1..1, "LeftZ") |
| `touches` | `{ id: number; x: number; y: number }[]` | the fingers down, in composition px |

### `FrameRange`

| Field | Type | What it is |
| --- | --- | --- |
| `from` | `number` | first frame (inclusive), default 0 |
| `to` | `number` | last frame (exclusive), default the song's end |

### `FreezeProps`

The props of [`Freeze`](#freeze), tabled there.

### `GeneratedAsset`

| Field | Type | What it is |
| --- | --- | --- |
| `kind` (required) | `AssetKind` | which generator makes it: lyria (music), sao (sound effects) or tokensynth (an instrument's note) |
| `prompt` (required) | `string` | what to make, in words |
| `seconds` (required) | `number` | how long it is |
| `id` (required) | `string` | a stable hash of kind, prompt and seconds |
| `src` (required) | `string` | "asset:<kind>/<id>" |
| `options` | `Record<string, unknown>` | the generator's own settings (a TokenSynth note's midi, velocity, seed and device) |

### `GridFrom`

```ts
type GridFrom = "top-left" | "top-right" | "bottom-left" | "bottom-right" | "center" | "top" | "bottom" | "left" | "right"
```

### `Group`

| Field | Type | What it is |
| --- | --- | --- |
| `kind` (required) | `"sequence" \| "parallel"` | sequence: one after another; parallel: together |
| `steps` (required) | `Step[]` | its steps, in order |
| `gap` (required) | `number` | frames between steps of a sequence (negative overlaps) |

### `Guard`

A trigger: polled each step while its state is active (the context, and whatever the step passes as env).

```ts
type Guard = (ctx: C, env: E) => boolean
```

### `ImgProps`

The props of [`Img`](#img), tabled there.

### `InputTimeline`

Recorded input by composition frame; frames without an entry had no input.

```ts
type InputTimeline = Record<number, Partial<FrameInput> | undefined>
```

### `InterpolateOptions`

| Field | Type | What it is |
| --- | --- | --- |
| `easing` | `EasingFn \| EasingFn[]` | the curve between keyframes: one for all, or one a segment |
| `extrapolateLeft` | `ExtrapolateMode` | below the first keyframe: extend the line (default), clamp to it, identity (the input itself) or wrap |
| `extrapolateRight` | `ExtrapolateMode` | past the last keyframe: extend (default), clamp, identity or wrap |

### `Keys`

Keyframes by local frame: numbers, or colour text for colour properties (a material's `animate` takes other
values for its own fields, LinearRgba for an emissive).

```ts
type Keys = Record<number, V>
```

### `LoopProps`

The props of [`Loop`](#loop), tabled there.

### `Machine`

| Field | Type | What it is |
| --- | --- | --- |
| `id` (required) | `string` | the machine's name in traces and reports |
| `key` (required) | `K` | the context's field holding its snapshot ("sm") |
| `states` (required) | `readonly N[]` | every state, in definition order (outer before inner) |
| `edges` (required) | `readonly Edge<N>[]` | every transition, in the order they're checked within a state |
| `start` (required) | `() => Snapshot<N>` | a snapshot before the first step: the first step enters the initial states (running their entry actions) |
| `step` (required) | `(ctx: C, env: E, opts?: readonly string[] \| StepOptions \| undefined) => C` | One step: poll the active states' transitions (innermost first, then outward; within a state `on` for this step's events, then `always`, `after`, `onDone`, then the `choose` candidates for the decider), take the first that fires (at most one a step), then run the active states' tick actions. Returns the new context with its snapshot; the old one is unchanged. The third argument is this step's events, or StepOptions. |
| `state` (required) | `(ctx: C) => N` | the active leaf (the initial leaf before the first step) |
| `path` (required) | `(ctx: C) => N[]` | the active states, outermost first |
| `is` (required) | `(ctx: C, name: N) => boolean` | whether `name` is active (the leaf or a state around it) |
| `steps` (required) | `(ctx: C, name?: N \| undefined) => number` | steps since `name` (the leaf by default) was entered; -1 when it isn't active |
| `entered` (required) | `(before: C, after: C, name?: N \| undefined) => boolean` | whether the step from `before` to `after` entered `name` (any state when omitted) |
| `watch` (required) | `(ctx: C) => Watched` | this machine's state in `ctx`, for a trace |
| `analyze` (required) | `() => Analysis<N>` | the static analysis machine checks read |

### `MachineConfig`

| Field | Type | What it is |
| --- | --- | --- |
| `id` (required) | `string` | the machine's name in traces and reports |
| `initial` (required) | `NoInfer<N>` | the state it starts in |
| `states` (required) | `{ [S in N]?: StateConfig<C, E, N> }` | its states by name, each a StateConfig (substates nest inside) |
| `key` | `K` | the context's field holding the snapshot ("sm") |
| `strict` | `boolean` | refuse unreachable states and non-final dead ends when the machine is built (true) |

### `MediaProps`

| Field | Type | What it is |
| --- | --- | --- |
| `src` (required) | `string` | the file: staticFile("clip.mp4"), or a generated asset |
| `startFrom` | `number` | where in the media it starts playing (0), counted in the composition's frames, not the media's own: a 24 fps clip in a 60 fps film starts at its 4 s with startFrom={240}; the same as trimBefore |
| `endAt` | `number` | where in the media it stops (exclusive), in the composition's frames like startFrom: it holds the last frame, or loops; the same as trimAfter |
| `trimBefore` | `number` | how much of the media is cut off its start, in the composition's frames (the newer name for startFrom) |
| `trimAfter` | `number` | where the media is cut, in the composition's frames (the newer name for endAt) |
| `volume` | `VolumeProp` | linear gain, 1 being unity (0.5 is about -6 dB, 1.78 about +5 dB), or a function of the media's own frame |
| `gainDb` | `VolumeProp` | ours: the gain in decibels (or a function of the media's own frame returning it); wins over volume |
| `fadeIn` | `number` | ours: seconds to ramp the sound in from silence at its start (applied by the renderer at the audio's rate) |
| `fadeOut` | `number` | ours: seconds to ramp the sound out to silence by its end: the end of its Sequence or Loop pass, or endAt, whichever comes first (one removed some other way fades out after it's gone) |
| `playbackRate` | `number` | how fast it plays (1): 2 is double speed |
| `muted` | `boolean` | play no sound |
| `loop` | `boolean` | start again from `startFrom` when it reaches `endAt` |
| `bus` | `string` | the bus it plays into: a named channel of the edit's Tone graph (music, sfx, voice, ambience, ui), so it takes that bus's volume, effects and ducking (raea/tone `buses`) |
| `name` | `string` | the entity's name, for picking, the inspector and world.* |
| `className` | `string` | Tailwind or CSS classes |
| `style` | `CSSProperties` | inline style |

### `MixProps`

The props of [`Mix`](#mix), tabled there.

### `Move`

Tracks by property, in frames from the move's own start.

| Field | Type | What it is |
| --- | --- | --- |
| `tracks` (required) | `Record<string, TrackSpec \| TrackSpec[]>` | keyframes by property, in frames from the move's start |
| `frames` (required) | `number` | how long it runs (its last key) |

### `MoveLike`

```ts
type MoveLike = MoveName | Move | MoveLike[]
```

### `MoveName`

```ts
type MoveName = "rise" | "pop" | "slam" | "slide" | "wipe" | "fill" | "strike" | "pushIn" | "fade"
```

### `NudgeProps`

The props of [`Nudge`](#nudge), tabled there.

### `PanelAction`

One action: what it does (for the agent), its arguments by name with a few words each, and what it does to the
panel's state.

| Field | Type | What it is |
| --- | --- | --- |
| `about` (required) | `string` | what the action does, for the agent |
| `args` | `Record<string, string>` | its arguments by name, a few words each |
| `run` (required) | `(state: S, args: A) => S` | the panel's state after it |

### `PanelActions`

```ts
type PanelActions = Record<string, PanelAction<S>>
```

### `Perception`

What perception saw at this frame: the largest face, and the platform's safe area (composition px).

| Field | Type | What it is |
| --- | --- | --- |
| `face` | `Rect` | the largest face's box, when there's one |
| `safe` | `{ top?: number; bottom?: number }` | px the platform's UI covers at the top and bottom |

### `Plan`

A timeline's plan: each part's start frame and move, and the whole length.

| Field | Type | What it is |
| --- | --- | --- |
| `parts` (required) | `Record<string, { at: number; move: Move }>` | each part's start frame and move, by its name |
| `frames` (required) | `number` | the whole timeline's length in frames |

### `Playtest`

What a playtester needs from a game, exported beside the view as `export const playtest: Playtest<S> = {...}`
(sdk/react/docs/playtest.md). Optional: a game without one is played from pixels and events alone. Everything here is
pure, so a bot steps the game headlessly, restores any state it has seen (states are values) and searches from it;
whatever it plays is an input timeline `raea render --input` turns back into the same frames.

| Field | Type | What it is |
| --- | --- | --- |
| `initial` (required) | `S` | the state at frame 0, as useGameState is given it |
| `step` (required) | `Step<S>` | the fold useGameState is given: the same function, so a bot plays the game the render shows |
| `actions` (required) | `string[][]` | the key chords worth trying ([] is "nothing held"); a bot's moves are these, each held some frames |
| `observe` | `(state: S, frame: number) => O` | what a player could know at this frame, as plain JSON (positions, lives, what's near): a model reads it |
| `progress` | `(state: S, frame: number) => number` | how far toward the goal, 0..1: search follows it, and "stuck" means it stopped rising |
| `outcome` | `(state: S, frame: number) => "won" \| "lost" \| null` | "won" when the goal is reached, "lost" on game over, null while play goes on |
| `events` | `Record<string, (before: S, after: S) => boolean>` | named moments as useGameEvents takes them (a death, a coin): counted, placed and rendered for the judge |
| `machines` | `(state: S) => Record<string, Watched>` | every state machine in the state, by entity name (`{ player: player.watch(s.player), flow: flow.watch(s) }`): the trace records each one's state and transitions, and the machine checks (coverage, softlocks, unreachable states, reached) read them (sdk/react/docs/state-machines.md) |

### `ProofInput`

| Field | Type | What it is |
| --- | --- | --- |
| `width` (required) | `number` | the composition's width, px |
| `height` (required) | `number` | the composition's height, px |
| `face` | `Rect` | the face's box, when perception found one |
| `parts` (required) | `ProofPart[]` | the measured boxes of the parts to check |

### `ProofPart`

| Field | Type | What it is |
| --- | --- | --- |
| `name` (required) | `string` | the part's name, in the faults |
| `band` | `[number, number]` | the band the part must fit in, [top, bottom] in px |
| `x` (required) | `number` | the left edge, px |
| `y` (required) | `number` | the top edge, px |
| `w` (required) | `number` | width, px |
| `h` (required) | `number` | height, px |

### `RaeaCSSProperties`

| Field | Type | What it is |
| --- | --- | --- |
| `baselineSource` | `Globals \| "auto" \| "first" \| "last" \| undefined` | which baseline of a box aligns it in its line: its first line's or its last's (css-inline-3) |
| `flexLineCount` | `Globals \| number \| undefined` | a flex container's line count when it balances its items across lines (css-flexbox-2): a positive integer |
| `maskPositionX` | `Property.BackgroundPositionX<TLength> \| undefined` | a mask image's horizontal position, as background-position-x takes it (css-masking-1) |
| `maskPositionY` | `Property.BackgroundPositionY<TLength> \| undefined` | a mask image's vertical position, as background-position-y takes it (css-masking-1) |
| `textDecorationInset` | `Globals \| TLength \| "auto" \| (string & {}) \| undefined` | how far an underline or line-through reaches past (or short of) the text at each end: auto, or one or two lengths (css-text-decor-4) |

### `Rect`

A box in composition px.

| Field | Type | What it is |
| --- | --- | --- |
| `x` (required) | `number` | the left edge, px |
| `y` (required) | `number` | the top edge, px |
| `w` (required) | `number` | width, px |
| `h` (required) | `number` | height, px |

### `RegionBox`

The band a Region occupies, in composition px.

| Field | Type | What it is |
| --- | --- | --- |
| `name` (required) | `RegionName` | the band it took: top, bottom, center or full ("auto" resolves to top or bottom) |
| `x` (required) | `number` | the left edge, px |
| `y` (required) | `number` | the top edge, px |
| `w` (required) | `number` | width, px |
| `h` (required) | `number` | height, px |

### `RegionName`

```ts
type RegionName = "auto" | "top" | "bottom" | "center" | "full"
```

### `RegionProps`

The props of [`Region`](#region), tabled there.

### `RegisteredComposition`

| Field | Type | What it is |
| --- | --- | --- |
| `id` (required) | `string` | the composition's id: letters, digits and dashes (`raea render --composition` picks it) |
| `component` (required) | `ComponentType<any>` | the React component it renders |
| `width` (required) | `number` | in px |
| `height` (required) | `number` | in px |
| `fps` (required) | `number` | frames a second |
| `durationInFrames` (required) | `number` | its length in frames |
| `defaultProps` (required) | `Record<string, unknown>` | the props it's rendered with (JSON-like values only) |

### `SequenceLayout`

```ts
type SequenceLayout = "absolute-fill" | "none"
```

### `SequenceProps`

The props of [`Sequence`](#sequence), tabled there.

### `SeriesSequenceProps`

| Field | Type | What it is |
| --- | --- | --- |
| `offset` | `number` | frames from the previous slot's end to this one's start (0); negative overlaps |
| `height` | `number` | the height its children see, px (the parent's by default) |
| `style` | `CSSProperties` | the wrapping box's style |
| `children` | `ReactNode` | what plays in it, at its own local time |
| `className` | `string` | the wrapping box's classes |
| `name` | `string` | its label in the timeline and the render manifest (`data-sequence`) |
| `width` | `number` | the width its children see, px (the parent's by default) |
| `durationInFrames` | `number` | how many frames it shows its children (forever by default) |

### `Snapshot`

The machine's runtime state, kept in the context it drives. `path` is the active states outermost first ([] before
the first step); `at[i]` is the step on which path[i] was entered; `n` counts the machine's steps from 0; `e` is the
edge the last transition took (an index into machine.edges); `by` names the decider that chose it (absent: a
guard); `hist` is each history state's last substate.

| Field | Type | What it is |
| --- | --- | --- |
| `path` (required) | `N[]` | the active states, outermost first ([] before the first step) |
| `at` (required) | `number[]` | the step each state in `path` was entered on |
| `n` (required) | `number` | the machine's steps so far, from 0 |
| `e` (required) | `number` | the edge the last transition took, an index into `machine.edges` |
| `by` | `string` | the decider that chose the last transition (absent: a guard did) |
| `hist` | `{ [S in N]?: N }` | each history state's last substate |

### `Song`

| Field | Type | What it is |
| --- | --- | --- |
| `map` (required) | `SongMap` | the song's map, from \`raea song\` |
| `fps` (required) | `number` | the composition's frame rate, that frames are counted in |
| `offset` (required) | `number` | seconds into the song frame 0 falls on (0) |
| `frame` (required) | `(seconds: number) => number` | the frame a song time falls on |
| `seconds` (required) | `(frame: number) => number` | the song's seconds at a frame |
| `at` (required) | `(frame: number) => SongMoment` | where the song is at a frame |
| `onBeat` (required) | `(every?: number \| undefined, range?: FrameRange \| undefined) => number[]` | frames of every `every`-th beat (1: each beat), counted from the song's first |
| `onDownbeat` (required) | `(every?: number \| undefined, range?: FrameRange \| undefined) => number[]` | frames of every `every`-th downbeat |
| `eventsOf` (required) | `(kind: SongEventKind, range?: FrameRange \| undefined) => { frame: number; event: SongEv…` | frames of every event of a kind (a build's or stop's start), with the event |
| `cutsAt` (required) | `(options?: CutOptions \| undefined) => number[]` | frames to cut on, paced to the energy: each section's (or bar's) rate, on its beats, plus drops, a stop's hits, and a build's second half at double rate |

### `SongBar`

| Field | Type | What it is |
| --- | --- | --- |
| `time` (required) | `number` | seconds from the song's start where the bar begins (its downbeat) |
| `beat` (required) | `number` | the index of its first beat |
| `beats` (required) | `number` | how many beats it holds |
| `energy` (required) | `number` | its mean energy, 0..1 |
| `cuts` (required) | `number` | suggested cuts in this bar: 0.25, 0.5, 1, 2 or 4 |

### `SongEvent`

| Field | Type | What it is |
| --- | --- | --- |
| `kind` (required) | `SongEventKind` | what the event is: build, drop, stop, hit, dip or break |
| `time` (required) | `number` | seconds from the song's start: a beat (drop, a downbeat) or the grid's nearest beat or half beat (hit) |
| `end` | `number` | spans (build, stop, dip, break) end here |
| `beat` (required) | `number` | the index of the beat it starts on, in `SongMap.beats` |
| `bar` (required) | `number` | the index of the bar it starts in |
| `beats` | `number` | a span: how many beats it lasts |
| `strength` (required) | `number` | 0..1 |
| `into` | `number \| null` | a build or stop: the drop it leads into, seconds |
| `hits` | `SongHit[]` | a stop: its sparse hits |
| `rise` | `number` | a drop: the energy of the bar after minus the bar before (negative when the band cuts after one big hit) |
| `holds` | `boolean` | a drop: the bar after stays up (false: one hit, then the band cuts out) |
| `afterStop` | `boolean` | a drop: it comes straight out of a stop |
| `afterBuild` | `boolean` | a drop: it lands at the end of a build (within two bars of it) |
| `gapBefore` | `boolean` | a drop: a breath of near-silence just before it |
| `at` | `number` | a hit: the onset's own time; how far over the groove it stands |
| `over` | `number` | a hit: how far over the groove it stands |

### `SongEventKind`

```ts
type SongEventKind = "build" | "drop" | "stop" | "hit" | "dip" | "break"
```

### `SongFeatures`

| Field | Type | What it is |
| --- | --- | --- |
| `loudness` (required) | `number[]` | loudness (RMS) a beat, 0..1 over the song |
| `onsets` (required) | `number[]` | onset strength (spectral flux) a beat, 0..1: how much is struck |
| `brightness` (required) | `number[]` | brightness (spectral centroid) a beat, 0..1 |
| `low` (required) | `number[]` | the low end (kick and bass, under 150 Hz) a beat, 0..1 |
| `high` (required) | `number[]` | the high band (over 3 kHz) a beat, 0..1 |
| `energy` (required) | `number[]` | the combined energy a beat, 0..1: loudness, low end, onsets and highs together |

### `SongHit`

A hit inside a stop: `time` on the beat grid (a beat or half beat), `at` where the onset is.

| Field | Type | What it is |
| --- | --- | --- |
| `time` (required) | `number` | seconds from the song's start, snapped to the beat grid (a beat or half beat) |
| `at` (required) | `number` | seconds from the song's start where the onset itself is |
| `strength` (required) | `number` | how hard the hit is, 0..1 |

### `SongLevel`

```ts
type SongLevel = "intro" | "low" | "mid" | "high" | "outro"
```

### `SongMap`

| Field | Type | What it is |
| --- | --- | --- |
| `version` (required) | `number` | the map format's version (1) |
| `duration` (required) | `number` | the song's length in seconds |
| `tempo` (required) | `number \| null` | beats per minute, or null when no beat was found |
| `beatsPerBar` (required) | `number` | beats in a bar (4 for 4/4) |
| `beats` (required) | `number[]` | every beat's time in seconds, ascending |
| `downbeats` (required) | `number[]` | every bar's first beat, in seconds |
| `features` (required) | `SongFeatures` | one value a beat, 0..1 over the song |
| `bars` (required) | `SongBar[]` | the bars in order, each with its energy and suggested cut rate |
| `sections` (required) | `SongSection[]` | the song's sections in order (intro, verse, chorus and so on) |
| `events` (required) | `SongEvent[]` | builds, drops, stops, hits, dips and breaks, in time order |
| `sectionsFrom` | `"markers" \| "novelty"` | where the sections came from: the song file's markers, or novelty in the music itself |
| `boundaries` | `{ time: number; beat: number; bar: number; strength: number }[]` | the candidate section boundaries novelty found, each with its strength 0..1 |
| `source` | `string \| null` | the song file's name |

### `SongMoment`

Where a moment of the song is. Seconds are the song's; infinities when there's no such event.

| Field | Type | What it is |
| --- | --- | --- |
| `time` (required) | `number` | the moment, seconds from the song's start |
| `beat` (required) | `number` | the beat at or before `time` (-1 before the first) |
| `bar` (required) | `number` | the bar at or before `time` (-1 before the first beat) |
| `beatInBar` (required) | `number` | 0-based position in the bar |
| `phase` (required) | `number` | 0..1 through the beat, and through the bar |
| `barPhase` (required) | `number` | 0..1 through the bar |
| `energy` (required) | `number` | the features at `time`, interpolated between beats |
| `loudness` (required) | `number` | loudness at `time`, 0..1 |
| `onsets` (required) | `number` | onset strength at `time`, 0..1 |
| `brightness` (required) | `number` | brightness at `time`, 0..1 |
| `low` (required) | `number` | the low end at `time`, 0..1 |
| `section` (required) | `SongSection \| null` | the section `time` is in (null before the first) |
| `sectionIndex` (required) | `number` | that section's index in `SongMap.sections` (-1 for none) |
| `sectionProgress` (required) | `number` | 0..1 through the section |
| `cutsPerBar` (required) | `number` | the bar's suggested cut rate (cuts per bar) |
| `drop` (required) | `SongEvent \| null` | the last drop at or before `time`, the next after it |
| `nextDrop` (required) | `SongEvent \| null` | the next drop after `time` |
| `sinceDrop` (required) | `number` | seconds since the last drop (Infinity when there's none) |
| `untilDrop` (required) | `number` | seconds until the next drop (Infinity when there's none) |
| `build` (required) | `SongEvent \| null` | the build `time` is in, and 0..1 through it |
| `inBuild` (required) | `boolean` | whether `time` is inside a build |
| `buildProgress` (required) | `number` | 0..1 through that build |
| `stop` (required) | `SongEvent \| null` | the stop `time` is in |
| `inStop` (required) | `boolean` | whether `time` is inside a stop |
| `dip` (required) | `SongEvent \| null` | the dip or break `time` is in |
| `next` (required) | `SongEvent \| null` | the next event of any kind after `time` |

### `SongOptions`

| Field | Type | What it is |
| --- | --- | --- |
| `fps` (required) | `number` | the composition's frame rate |
| `offset` | `number` | seconds into the song at frame 0 (where an <Audio> of it starts: its startFrom over fps) |

### `SongPlotProps`

The props of [`SongPlot`](#songplot), tabled there.

### `SongSection`

| Field | Type | What it is |
| --- | --- | --- |
| `name` (required) | `string` | the section's marker name when the song file has markers, else its level |
| `level` (required) | `SongLevel` | how high the section sits in the song: intro, low, mid, high or outro |
| `time` (required) | `number` | seconds from the song's start where the section begins |
| `end` (required) | `number` | seconds from the song's start where it ends |
| `beat` (required) | `number` | the index of its first beat |
| `bar` (required) | `number` | the index of its first bar |
| `energy` (required) | `number` | its mean energy, 0..1 |
| `cuts` (required) | `number` | suggested cuts per bar: 0.25, 0.5, 1, 2 or 4 |

### `SpringConfig`

| Field | Type | What it is |
| --- | --- | --- |
| `mass` | `number` | how heavy the spring's weight is (1): heavier is slower and swings further |
| `damping` | `number` | how fast the swing dies (10): lower bounces more |
| `stiffness` | `number` | how hard the spring pulls (100): stiffer is quicker |
| `overshootClamping` | `boolean` | stop at `to` the first time it gets there, with no overshoot (false) |

### `SpringOptions`

| Field | Type | What it is |
| --- | --- | --- |
| `frame` (required) | `number` | the frame to sample, usually useCurrentFrame() |
| `fps` (required) | `number` | frames a second, from useVideoConfig() |
| `from` | `number` | the value it starts at (0) |
| `to` | `number` | the value it settles at (1) |
| `config` | `SpringConfig` | the spring: mass, damping, stiffness, overshootClamping |
| `durationInFrames` | `number` | stretch or squeeze the spring to settle in this many frames |
| `durationRestThreshold` | `number` | how close to `to`, as a share of the travel, counts as settled when timing it (0.005) |
| `delay` | `number` | frames before it starts (0) |
| `reverse` | `boolean` | play it backwards: from `to` back to `from` |

### `StaggerProps`

| Field | Type | What it is |
| --- | --- | --- |
| `each` (required) | `number` | frames between one child's entrance and the next |
| `grid` | `{ cols: number; rows?: number; from?: GridFrom }` | spread by grid distance instead of order |
| `exitEach` | `number` | the exit staggered too, `exitEach` frames apart (0: all together) |
| `style` | `CSSProperties` | inline style |
| `animate` | `Record<string, TrackSpec \| TrackSpec[] \| Keys> \| Move` | free tracks by property, in local frames: `{ backgroundColor: track({ 0: "#fff", 20: "#000" }) }` |
| `as` | `string` | the element: "div" by default |
| `exit` | `MoveLike` | the exit, played at `exitAt` (an entrance reversed: `exit="rise"` sinks back out) |
| `children` | `ReactNode` | what moves |
| `enter` | `MoveLike` | the entrance: a move's name, a move, or several together |
| `at` | `number` | local frame the entrance starts (0) |
| `exitAt` | `number` | local frame the exit starts |
| `mount` | `"during" \| "always"` | "always" (default): in the tree, and its layout, the whole time (invisible before an entrance that fades in); "during": only from the entrance's start until the exit ends (it takes its space when it arrives) |
| `className` | `string` | Tailwind or CSS classes |

### `StateConfig`

| Field | Type | What it is |
| --- | --- | --- |
| `entry` | `Actions<C, E>` | run when the state is entered (outer states before inner ones) |
| `exit` | `Actions<C, E>` | run when the state is left (inner states before outer ones) |
| `tick` | `Actions<C, E>` | run every step the state is active, after this step's transition (outer before inner, the entry step too) |
| `always` | `Transitions<C, E, N>` | triggers polled every step, in order: the first whose guard holds fires (XState's eventless `always`) |
| `after` | `{ readonly [steps: number]: Transitions<C, E, N> }` | delays in steps: `after: { 30: "fall" }` fires once the state has been active 30 steps (and its guard holds) |
| `on` | `{ readonly [event: string]: Transitions<C, E, N> }` | named events the step is given this time (input edges: games.ts keyEdges gives "keydown:ArrowUp") |
| `choose` | `Transitions<C, E, N>` | a choice point: of these candidates, those whose guards hold go to the step's Decider, which picks one or waits (no decider: firstDecider takes the first). Polled after `always` and `after`. |
| `initial` | `NoInfer<N>` | a compound state's first substate |
| `states` | `{ [K in N]?: StateConfig<C, E, N> }` | substates: names are unique across the whole machine, so `to: "run"` names one state wherever it is |
| `history` | `"shallow" \| "deep"` | re-entering this compound state resumes the substate it left ("shallow"), or that substate's own (deep) |
| `onDone` | `Transitions<C, E, N>` | a compound state's transition taken when its active substate is final |
| `type` | `"final"` | a final state: no transitions out (a dead enemy, the game won, the work shipped) |
| `description` | `string` | what the state means, for reports and a decider |

### `StateName`

The state names of a machine.

```ts
type StateName = M extends Machine<any, any, infer N, any> ? N : never
```

### `Step`

A timeline step: [part, move], a group from sequence/parallel/stagger, or a pause in frames.

```ts
type Step = [part: string, move: MoveLike] | { part: string; move: MoveLike; offset?: number } | Group | number
```

### `StepOptions`

What a step may be given besides the context and env.

| Field | Type | What it is |
| --- | --- | --- |
| `events` | `readonly string[]` | named events this step (an `on` edge waits for one) |
| `decide` | `Decider` | picks at choice points (default firstDecider) |
| `trace` | `(r: TransitionRecord) => void` | receives every transition this step takes, as a row |

### `TimelineWord`

| Field | Type | What it is |
| --- | --- | --- |
| `text` (required) | `string` | the word as spoken |
| `from` (required) | `number` | first frame of the word |
| `to` (required) | `number` | frame after the word ends (exclusive) |

### `TrackSpec`

One property's keyframes: values by frame, the curve between each pair (one for all, or one per segment).

| Field | Type | What it is |
| --- | --- | --- |
| `keys` (required) | `[number, V][]` | [frame, value] pairs in frame order, frames from the move's start |
| `ease` (required) | `Curve \| Curve[]` | the curve between keys: one for all, or one a segment |
| `unit` | `"px" \| "%"` | a length's unit: px (the default) or % |
| `repeat` | `"hold" \| "loop" \| "pingpong" \| `${"loop" \| "pingpong"}:${number}`` | past the last key: hold (the default), loop or pingpong, for ever or N times ("loop:3") |
| `digits` | `number` | decimals a `text` counter writes |
| `prefix` | `string` | a `text` counter's text before and after the number, and between its thousands ("," writes 12,345) |
| `suffix` | `string` | written after a `text` counter's number (" pts") |
| `separator` | `string` | between a `text` counter's thousands ("," writes 12,345) |

### `TransitionConfig`

A transition: where to go (`to`, or XState's `target`), when (`when`, or XState's `guard`; none: always), after how
many steps in the state at least (`after`), and what to do on the way (`actions`).

```ts
type TransitionConfig = Target<N> & When<C, E> & { /** steps in the state before this transition may fire (0) */ after?: number; actions?: Actions<C, E>; /** what the transition means: a decider reads it (a model's option description), reports show it */ description?: string; }
```

### `TransitionRecord`

One transition as a row: what a trace keeps and rdb loads (step, machine, from, to, edge, decider, evidence). `from` is
null for the first step's entry into the initial states.

| Field | Type | What it is |
| --- | --- | --- |
| `machine` (required) | `string` | the machine's id |
| `step` (required) | `number` | the machine's step it happened on |
| `from` (required) | `string \| null` | the leaf it left (null for the first entry) |
| `to` (required) | `string` | the leaf it entered |
| `edge` (required) | `number` | the edge it took, an index into `machine.edges` |
| `kind` (required) | `Edge["kind"] \| "start"` | what kind of edge: always, after, on, choose, done (an onDone), or start for the first entry |
| `by` (required) | `string` | "guard" for a trigger, "start" for the first entry, else the decider's name |
| `evidence` | `unknown` | the decider's evidence for the choice |

### `Transitions`

A target's name alone, one transition, or several checked in order.

```ts
type Transitions = | NoInfer<N> | TransitionConfig<C, E, N> | readonly (NoInfer<N> | TransitionConfig<C, E, N>)[]
```

### `UseSongOptions`

| Field | Type | What it is |
| --- | --- | --- |
| `offset` | `number` | seconds into the song at frame 0 of the enclosing Sequence |

### `VideoConfig`

| Field | Type | What it is |
| --- | --- | --- |
| `id` (required) | `string` | the composition's id ("main" by default) |
| `width` (required) | `number` | in px |
| `height` (required) | `number` | in px |
| `fps` (required) | `number` | frames a second |
| `durationInFrames` (required) | `number` | its length in frames |
| `defaultProps` (required) | `Record<string, unknown>` | the props the composition is rendered with |

### `VideoProps`

The props of [`Video`](#video), tabled there.

### `VolumeProp`

```ts
type VolumeProp = number | ((mediaFrame: number) => number)
```

### `Watched`

A machine's state in some context, for a trace (`Playtest.machines`).

| Field | Type | What it is |
| --- | --- | --- |
| `machine` (required) | `AnyMachine` | the machine |
| `snapshot` (required) | `Snapshot` | its runtime state in that context |

### `WordOptions`

| Field | Type | What it is |
| --- | --- | --- |
| `occurrence` | `number` | which time the phrase is said, from 1 |
| `edge` | `"start" \| "end"` | "start": the phrase's first frame; "end": the frame after its last word ends |

<!-- generated:end -->
