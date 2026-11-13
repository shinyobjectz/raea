---
title: "raea/time"
summary: "The frame being drawn and the composition's time: useCurrentFrame, useVideoConfig and the Clock."
order: 22
sources: ["sdk/raea/src/time.ts"]
---

Time in raea is frames. `useCurrentFrame()` is the frame being drawn, counted from the start of the nearest
`<Sequence>`; it's Bevy's `Time<Virtual>` stepped at the composition's frame rate, so a render and a live play of the
same input draw the same frames. `useVideoConfig()` gives the composition's size, frame rate and length.
`<Clock>` is what steps them; the renderer and the app put one around every composition.

```tsx
import { useCurrentFrame, useVideoConfig } from "raea/time";

function Progress() {
  const frame = useCurrentFrame();
  const { durationInFrames, width } = useVideoConfig();
  return <div style={{ width: (frame / durationInFrames) * width, height: 8, background: "#fff" }} />;
}
```

<!-- generated:begin by tools/manual-ref/sdk.ts from the SDK's exports and doc comments; edit outside this block -->

```ts
import { Clock, useCurrentFrame, useLocalTime, useVideoConfig } from "raea/time"
```

`raea/time` exports 1 component, 3 hooks, 7 types. Bevy's counterpart: `bevy_time`. Source: [sdk/raea/src/time.ts](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/src/time.ts).

## Components

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

## Hooks

### `useCurrentFrame`

The frame being rendered, counted from the start of the nearest enclosing Sequence.

```ts
function useCurrentFrame(): number
```

### `useLocalTime`

The time where it's called: the nearest Sequence's (its local frame, where it starts, and the size it gives its
children), or the Clock's outside any. `caller` names the hook in errors. The hook under useCurrentFrame, for components of your own that need more
than the frame.

```ts
function useLocalTime(caller: string): LocalTime
```

### `useVideoConfig`

The settings of the composition being rendered (inside a Sequence, its width, height and length win).

```ts
function useVideoConfig(): VideoConfig
```

## Types

### `ClockProps`

The props of [`Clock`](#clock), tabled there.

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

### `InputTimeline`

Recorded input by composition frame; frames without an entry had no input.

```ts
type InputTimeline = Record<number, Partial<FrameInput> | undefined>
```

### `LocalTime`

The local time a subtree sees: its own frame, and the composition frame its frame 0 falls on.

| Field | Type | What it is |
| --- | --- | --- |
| `frame` (required) | `number` | the subtree's own frame: the composition's frame minus `origin` |
| `origin` (required) | `number` | the composition frame the subtree's frame 0 falls on |
| `width` (required) | `number` | width and height a Sequence overrides for its children |
| `height` (required) | `number` | in px, what the Sequence gives its children |
| `duration` (required) | `number` | frames the subtree runs for from its frame 0, which useVideoConfig reports as durationInFrames: a Sequence's own length cut where its parent's ends, a Loop's one pass, the composition's at the root |

### `Perception`

What perception saw at this frame: the largest face, and the platform's safe area (composition px).

| Field | Type | What it is |
| --- | --- | --- |
| `face` | `Rect` | the largest face's box, when there's one |
| `safe` | `{ top?: number; bottom?: number }` | px the platform's UI covers at the top and bottom |

### `TimelineWord`

| Field | Type | What it is |
| --- | --- | --- |
| `text` (required) | `string` | the word as spoken |
| `from` (required) | `number` | first frame of the word |
| `to` (required) | `number` | frame after the word ends (exclusive) |

### `VideoConfig`

| Field | Type | What it is |
| --- | --- | --- |
| `id` (required) | `string` | the composition's id ("main" by default) |
| `width` (required) | `number` | in px |
| `height` (required) | `number` | in px |
| `fps` (required) | `number` | frames a second |
| `durationInFrames` (required) | `number` | its length in frames |
| `defaultProps` (required) | `Record<string, unknown>` | the props the composition is rendered with |

<!-- generated:end -->
