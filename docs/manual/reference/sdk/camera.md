---
title: "raea/camera"
summary: "Cameras and the views they make: Camera3d, the Canvas3D a 2D scene draws in, HDR and matchmoved cameras."
order: 63
sources: ["sdk/raea/src/camera.ts"]
---

`<Camera3d>` is Bevy's 3D camera: a transform, a projection and what it renders to. `<Canvas3D>` is the stage a
Bevy-style 2D scene draws in, with Bevy's coordinates (the origin at the centre, +Y up, a unit a pixel).
`<Matchmove>` drives a camera from the solve [`raea track`](../cli/track.md) makes, so 3D stands in filmed footage.

```tsx
import { Camera3d } from "raea/camera";
import { Transform } from "raea/transform";

const Shot = () => <Camera3d transform={Transform.fromXyz(-2, 2.5, 5).lookingAt([0, 0, 0])} projection={{ Perspective: { fov: Math.PI / 4 } }} />;
```

<!-- generated:begin by tools/manual-ref/sdk.ts from the SDK's exports and doc comments; edit outside this block -->

```ts
import { Camera3d, cameraPath, Canvas3D, checkMatchmove } from "raea/camera"
```

`raea/camera` exports 6 components, 1 hook, 9 functions, 4 values, 19 types. Bevy's counterpart: `bevy_camera`. Source: [sdk/raea/src/camera.ts](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/src/camera.ts).

## Components

### `Camera3d`

Bevy's Camera3d with its Transform and Projection: it looks along its -Z, +Y up, as Bevy's cameras do.

Bevy: `bevy_camera::components::Camera3d`, `bevy_camera::camera::Camera`, `bevy_camera::projection::Projection`.

| Prop | Type | What it does |
| --- | --- | --- |
| `transform` | `TransformValue` | where the camera stands and faces (`Transform.fromXYZ(...).lookingAt(...)`) |
| `projection` | `Projection` | perspective (the default) or orthographic, as Bevy's Projection |
| `layers` | `number[]` | Bevy's RenderLayers on the camera: it draws only the parts on these layers (a Mesh3d's `layers`; [0] by default) |
| `order` | `number` | Bevy's Camera order: a camera with a higher one draws after (over) those with lower; 0 by default |
| `clear_color` | `ClearColorConfig` | Bevy's ClearColorConfig: "Default" (the clear colour), "None" (drawn over what the cameras before it drew, as a second pass), or { Custom: colour } |
| `viewport` | `CameraViewport` | Bevy's Viewport: the camera draws into this region of the window only, `physical_position` from its top left and `physical_size` big (whole px, at a scale factor of 1) |
| `is_active` | `boolean` | Bevy's Camera is_active: false and it draws nothing this frame, its lens and ambient with it. Several cameras in the scene, one active a frame, cut between them inside one render: the frame another camera becomes active (the first active one, in tree order, is the piece's) is a cut, drawn as a shot rendered alone draws its first frame (no motion blur from the frame before). True by default |
| `children` | `ReactNode` | components on the camera: post effects, tone mapping, a skybox |

Used in: [ui/render_ui_to_texture](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/render_ui_to_texture.tsx), [ui/widgets/viewport_node](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/widgets/viewport_node.tsx), [3d/fog_volumes](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/fog_volumes.tsx), [3d/fog](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/fog.tsx), [3d/vertex_colors](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/vertex_colors.tsx), [3d/rotate_environment_map](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/rotate_environment_map.tsx), [3d/transparency_3d](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/transparency_3d.tsx), [3d/spherical_area_lights](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/spherical_area_lights.tsx), and 61 more.

### `Canvas3D`

Only for `layered`: the 3D is drawn in the page's order, at the box of the nearest positioned element around it
(the whole frame when there's none; B3 H7), at that box's size and aspect:

 <div style={{ position: "absolute", left: 100, top: 50, width: 200, height: 150 }}>
   <Canvas3D layered><Camera3d transform={...} /><Mesh3d>...</Mesh3d></Canvas3D>
 </div>
Without `layered` it is its children, as written straight in the page.

Takes [`Canvas3DProps`](3d.md#canvas3dprops)'s props.

### `Hdr`

Bevy's Hdr: put it inside the <Camera3d> to render it to a float target, tone-mapped after, as Bevy's Hdr camera
is (a Skybox's or an emissive's light above 1 then rolls off in the tone mapping instead of clipping).

Bevy: `bevy_camera::components::Hdr`.

Used in: [3d/rotate_environment_map](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/rotate_environment_map.tsx).

### `Label`

Page type that names a 3D part: drawn over the frame where the part's point lands through the shot's camera, at a
size in pixels of the frame, inside title-safe, with a line to the point. Hidden while the point is behind the camera
or out of frame. Put it beside the 3D, in the page, and give it the props the shot's Camera3d gets:

`<Label at={[0, 1.2, 0]} camera={path.camera(frame)}>Reviews</Label>`

| Prop | Type | What it does |
| --- | --- | --- |
| `at` (required) | `Vec3` | the point it names, in metres in the world: give the part's top, so the words stand clear above it |
| `camera` (required) | `LabelCamera` | the shot's camera: the props its Camera3d is given (`path.camera(frame)`), so the label follows it exactly |
| `children` (required) | `ReactNode` | its words |
| `size` | `number` | capital height in px at 1080 lines, scaled to the frame (32; at least 22, so it reads) |
| `side` | `"above" \| "below"` | which way the words stand from the point ("above") |
| `gap` | `number` | the words' distance from the point, in px at 1080 lines (40) |
| `leader` | `boolean` | a line from the words to a dot on the point (true) |
| `color` | `string` | the words' and the line's colour (white) |
| `style` | `CSSProperties` | more style for the words: font, weight, a background, padding |

### `Matchmove`

Footage with real 3D standing in it: the clip behind, its camera solved and followed, the subject in front. Fills
the frame (its camera's view is the frame's); its children are the 3D in the shot's world, lights included.

Takes [`MatchmoveProps`](3d.md#matchmoveprops)'s props.

### `TextureView`

the GPU texture's number

Bevy: `bevy_render::TextureView`.

| Prop | Type | What it does |
| --- | --- | --- |
| `texture` (required) | `number` | the id a system lists the Image under in the App's PageTextures |

Also takes the props of `ImgHTMLAttributes` (`height`, `alt`, `crossOrigin`, `decoding`, `fetchPriority`, `loading`, `referrerPolicy`, `sizes`, `srcSet`, `useMap`, `width`); `HTMLAttributes` (`slot`, `style`, `title`, `hidden`, `color`, `content`, `part`, `className`, `defaultChecked`, `defaultValue`, `suppressContentEditableWarning`, `suppressHydrationWarning`, `accessKey`, `autoCapitalize`, `autoFocus`, `contentEditable`, `contextMenu`, `dir`, `draggable`, `enterKeyHint`, `id`, `lang`, `nonce`, `spellCheck`, `tabIndex`, `translate`, `radioGroup`, `role`, `about`, `datatype`, `inlist`, `prefix`, `property`, `rel`, `resource`, `rev`, `typeof`, `vocab`, `autoCorrect`, `autoSave`, `itemProp`, `itemScope`, `itemType`, `itemID`, `itemRef`, `results`, `security`, `unselectable`, `popover`, `popoverTargetAction`, `popoverTarget`, `inert`, `inputMode`, `is`, `exportparts`); `DOMAttributes` (`children`, `dangerouslySetInnerHTML`, `onCopy`, `onCopyCapture`, `onCut`, `onCutCapture`, `onPaste`, `onPasteCapture`, `onCompositionEnd`, `onCompositionEndCapture`, `onCompositionStart`, `onCompositionStartCapture`, `onCompositionUpdate`, `onCompositionUpdateCapture`, `onFocus`, `onFocusCapture`, `onBlur`, `onBlurCapture`, `onChange`, `onChangeCapture`, `onBeforeInput`, `onBeforeInputCapture`, `onInput`, `onInputCapture`, `onReset`, `onResetCapture`, `onSubmit`, `onSubmitCapture`, `onInvalid`, `onInvalidCapture`, `onLoad`, `onLoadCapture`, `onError`, `onErrorCapture`, `onKeyDown`, `onKeyDownCapture`, `onKeyPress`, `onKeyPressCapture`, `onKeyUp`, `onKeyUpCapture`, `onAbort`, `onAbortCapture`, `onCanPlay`, `onCanPlayCapture`, `onCanPlayThrough`, `onCanPlayThroughCapture`, `onDurationChange`, `onDurationChangeCapture`, `onEmptied`, `onEmptiedCapture`, `onEncrypted`, `onEncryptedCapture`, `onEnded`, `onEndedCapture`, `onLoadedData`, `onLoadedDataCapture`, `onLoadedMetadata`, `onLoadedMetadataCapture`, `onLoadStart`, `onLoadStartCapture`, `onPause`, `onPauseCapture`, `onPlay`, `onPlayCapture`, `onPlaying`, `onPlayingCapture`, `onProgress`, `onProgressCapture`, `onRateChange`, `onRateChangeCapture`, `onSeeked`, `onSeekedCapture`, `onSeeking`, `onSeekingCapture`, `onStalled`, `onStalledCapture`, `onSuspend`, `onSuspendCapture`, `onTimeUpdate`, `onTimeUpdateCapture`, `onVolumeChange`, `onVolumeChangeCapture`, `onWaiting`, `onWaitingCapture`, `onAuxClick`, `onAuxClickCapture`, `onClick`, `onClickCapture`, `onContextMenu`, `onContextMenuCapture`, `onDoubleClick`, `onDoubleClickCapture`, `onDrag`, `onDragCapture`, `onDragEnd`, `onDragEndCapture`, `onDragEnter`, `onDragEnterCapture`, `onDragExit`, `onDragExitCapture`, `onDragLeave`, `onDragLeaveCapture`, `onDragOver`, `onDragOverCapture`, `onDragStart`, `onDragStartCapture`, `onDrop`, `onDropCapture`, `onMouseDown`, `onMouseDownCapture`, `onMouseEnter`, `onMouseLeave`, `onMouseMove`, `onMouseMoveCapture`, `onMouseOut`, `onMouseOutCapture`, `onMouseOver`, `onMouseOverCapture`, `onMouseUp`, `onMouseUpCapture`, `onSelect`, `onSelectCapture`, `onTouchCancel`, `onTouchCancelCapture`, `onTouchEnd`, `onTouchEndCapture`, `onTouchMove`, `onTouchMoveCapture`, `onTouchStart`, `onTouchStartCapture`, `onPointerDown`, `onPointerDownCapture`, `onPointerMove`, `onPointerMoveCapture`, `onPointerUp`, `onPointerUpCapture`, `onPointerCancel`, `onPointerCancelCapture`, `onPointerEnter`, `onPointerLeave`, `onPointerOver`, `onPointerOverCapture`, `onPointerOut`, `onPointerOutCapture`, `onGotPointerCapture`, `onGotPointerCaptureCapture`, `onLostPointerCapture`, `onLostPointerCaptureCapture`, `onScroll`, `onScrollCapture`, `onScrollEnd`, `onScrollEndCapture`, `onWheel`, `onWheelCapture`, `onAnimationStart`, `onAnimationStartCapture`, `onAnimationEnd`, `onAnimationEndCapture`, `onAnimationIteration`, `onAnimationIterationCapture`, `onToggle`, `onBeforeToggle`, `onTransitionCancel`, `onTransitionCancelCapture`, `onTransitionEnd`, `onTransitionEndCapture`, `onTransitionRun`, `onTransitionRunCapture`, `onTransitionStart`, `onTransitionStartCapture`); `AriaAttributes` (`aria-activedescendant`, `aria-atomic`, `aria-autocomplete`, `aria-braillelabel`, `aria-brailleroledescription`, `aria-busy`, `aria-checked`, `aria-colcount`, `aria-colindex`, `aria-colindextext`, `aria-colspan`, `aria-controls`, `aria-current`, `aria-describedby`, `aria-description`, `aria-details`, `aria-disabled`, `aria-dropeffect`, `aria-errormessage`, `aria-expanded`, `aria-flowto`, `aria-grabbed`, `aria-haspopup`, `aria-hidden`, `aria-invalid`, `aria-keyshortcuts`, `aria-label`, `aria-labelledby`, `aria-level`, `aria-live`, `aria-modal`, `aria-multiline`, `aria-multiselectable`, `aria-orientation`, `aria-owns`, `aria-placeholder`, `aria-posinset`, `aria-pressed`, `aria-readonly`, `aria-relevant`, `aria-required`, `aria-roledescription`, `aria-rowcount`, `aria-rowindex`, `aria-rowindextext`, `aria-rowspan`, `aria-selected`, `aria-setsize`, `aria-sort`, `aria-valuemax`, `aria-valuemin`, `aria-valuenow`, `aria-valuetext`).

## Hooks

### `useMatchmove`

The solve and this frame's camera, inside a <Matchmove>.

```ts
function useMatchmove(): MatchmoveState
```

## Functions

### `cameraPath`

A camera path through keys timed in frames, paced by time so the camera's speed carries smoothly across every key.
It eases at the first key and the last, stops at a key with `hold`, and never overshoots a key and comes back.

```tsx
const path = cameraPath([
  { at: 0, pos: [0, 2, 9], look: [0, 1, 0] },
  { at: 60, pos: [3, 2, 6], look: [0, 1, 0], hold: 30 },
  { at: 150, pos: [1, 1.5, 3], look: [0, 1.2, 0], fov: 30 },
]);
<Camera3d {...path.camera(useCurrentFrame())} />
```

```ts
function cameraPath(keys: readonly CameraKey[]): CameraPath
```

### `checkMatchmove`

Checks a Matchmove's props; throws a TypeError or RangeError naming the prop.

```ts
function checkMatchmove(p: MatchmoveProps): void
```

### `focusAt`

The focal distance (metres from the camera) at a frame: a DepthOfField's focal_distance.

```ts
function focusAt(f: Focus, path: CameraPath, frame: number): number
```

### `handheld`

A walking operator's motion on a path: the camera bobs with each step (1.8 Hz) and sways with each stride (0.9 Hz),
rolls a little with the sway, reframes slowly, and its aim follows the path's on a spring, so it lags a turn and
settles after it. Deterministic: the same path, fps and seed give the same frames. Nothing moves faster than 1.9 Hz.

```tsx
const walk = handheld(shotMove({ kind: "dolly", from: [0, 1.6, 6], look: [0, 1.5, 0], by: 4, frames: 300 }), { fps: 30 });
```

```ts
function handheld(path: CameraPath, o?: { profile?: HandheldProfile; fps?: number; seed?: number; amount?: number; }): CameraPath
```

### `lens`

The vertical field of view (degrees) of a focal length on a full-frame (36 x 24 mm) sensor.

```ts
function lens(mm: number): number
```

### `matchmoveIndex`

The solve's frame at a time in the clip: the latest at or before it, clamped to the solve.

```ts
function matchmoveIndex(solve: MatchmoveSolve, time: number): number
```

### `matchmoveView`

The footage's view in a frame of another shape: the whole view's size (the footage scaled to cover the frame, times
zoom) and the frame's window in it, in frame pixels.

```ts
function matchmoveView(footage: { width: number; height: number; }, frame: { width: number; height: number; }, crop?: number | [number, number], zoom?: number): { width: number; height: number; x: number; y: number; }
```

### `shotMove`

A crew's move as a camera path. Each eases out of its start and into its end, and never lurches.

```tsx
const push = shotMove({ kind: "dolly", from: [0, 1.6, 4], look: [0, 1.2, 0], by: 1.5, frames: 120, fov: lens(35) });
<Camera3d {...push.camera(useCurrentFrame())} />
```

```ts
function shotMove(m: ShotMove): CameraPath
```

### `walkPath`

A path drawn on the floor plan (`raea make plan`'s floor-plan.png: x right, z away from the entrance, metres) as a
camera walking it: at eye height, at an even walking pace (0.4-0.6 m/s reads as a person; 0.5 by default), easing
only where it starts and stops, its corners rounded (`turn` metres either side) so it turns as a walker does rather
than stopping; looking `ahead` metres along the way, or at `look` throughout. Give it to `handheld` for the step.

```ts
function walkPath(points: readonly [number, number][], o?: { speed?: number; eye?: number; fps?: number; ahead?: number; look?: CameraVec3; start?: number; fov?: number; turn?: number; }): CameraPath
```

## Values

### `LABEL_MIN`

The least capital height a Label takes, in px at 1080 lines (SG10's floor for words that read).

### `LENSES`

The focal lengths a lens chart shows.

### `STEP_HZ`

A walker's steps, and the sway at half of it (one foot then the other). Every motion here is at or under STEP.

### `WALK`

A walkthrough's pace and eye height, as AnyShot's shot language keeps them: a person walking a set slowly.

## Types

### `Camera3dProps`

The props of [`Camera3d`](#camera3d), tabled there.

### `CameraKey`

A key of a camera path: where the camera is at a frame, and what it looks at.

```ts
type CameraKey = { /** The frame the camera reaches this key, in the frames of the Sequence it plays in (useCurrentFrame). */ at: number; /** Where the camera is. */ pos: CameraVec3; /** The point it looks at. */ look: CameraVec3; /** Its vertical field of view in degrees. Left out, the key before's (38 at the fi…
```

### `CameraPath`

A camera path built by cameraPath.

```ts
type CameraPath = { /** The camera at a frame (a fraction is fine): the first key's before it, the last key's after it. */ at(frame: number): CameraPose; /** Camera3d's props at a frame: its transform, looking at the look point, and its projection (fov in radians, as *  Bevy's is; the keys' are degrees). `<Camera3…
```

### `CameraPose`

Where the camera is at a frame, what it looks at and its vertical field of view in degrees.

```ts
type CameraPose = { pos: CameraVec3; look: CameraVec3; fov: number }
```

### `CameraVec3`

```ts
type CameraVec3 = [number, number, number]
```

### `CameraViewport`

Bevy's Viewport on a Camera, its depth range Bevy's default 0..1.

| Field | Type | What it is |
| --- | --- | --- |
| `physical_position` (required) | `[number, number]` | the viewport's top-left corner, in px |
| `physical_size` (required) | `[number, number]` | the viewport's width and height, in px |

### `Canvas3DProps`

| Field | Type | What it is |
| --- | --- | --- |
| `layered` | `boolean` | the 3D takes its place in the page's tree order instead of lying under the whole page: page elements before the Canvas3D (and the backgrounds of the elements holding it) are under the 3D, showing wherever it has nothing, and hidden by its parts; positioned elements after it are over it. The 3D's background is transparent. Put the Canvas3D straight in a full-frame AbsoluteFill. |
| `clearColor` | `string` | only with `layered`: the 3D clears to this CSS colour instead of transparent, so its translucent parts blend over it in linear light, as Bevy's do over its ClearColor (a Bevy 2D scene's rgb(43, 44, 47)), not over the page in sRGB. The 3D then lies under the whole page, as Bevy's camera output lies under all its UI (a GlobalZIndex(-1) too). |
| `children` | `ReactNode` | the 3D scene: cameras, lights and parts |

### `ClearColorConfig`

Bevy's ClearColorConfig.

Bevy: `bevy_camera::clear_color::ClearColorConfig`.

```ts
type ClearColorConfig = "Default" | "None" | { Custom: Srgba | LinearRgba }
```

### `Focus`

Where the lens is focused.

```ts
type Focus = | { kind: "point"; at: CameraVec3 } | { kind: "track"; target: (frame: number) => CameraVec3 } /** from one subject to the other over `over` frames starting at `at`, held on each before and after */ | { kind: "rack"; from: CameraVec3; to: CameraVec3; at: number; over: number }
```

### `HandheldProfile`

```ts
type HandheldProfile = "walk" | "subtle"
```

### `LabelCamera`

The camera a Label projects through: the props its Camera3d is given.

| Field | Type | What it is |
| --- | --- | --- |
| `transform` | `TransformValue` | where the camera stands and faces, as its Camera3d has it |
| `projection` | `Projection` | its perspective (fov in radians, Bevy's default π/4) or orthographic view, as its Camera3d has it |

### `LabelProps`

Label's props: page type that points at a 3D part.

The props of [`Label`](#label), tabled there.

### `MatchmoveCamera`

| Field | Type | What it is |
| --- | --- | --- |
| `t` (required) | `number` | seconds into the clip |
| `position` (required) | `Vec3` | where the camera is, in the solved world (metres) |
| `quaternion` (required) | `[number, number, number, number]` | camera to world, [x, y, z, w]; the camera looks down its -Z with +Y up |
| `solved` (required) | `boolean` | false: COLMAP didn't register this frame and its pose was interpolated |

### `MatchmoveProps`

| Field | Type | What it is |
| --- | --- | --- |
| `src` (required) | `string` | the footage: staticFile("clip.mp4") |
| `from` | `number` | where in the clip the shot starts, and how long it runs (seconds; 0 = to the end) |
| `seconds` | `number` | how long the shot runs, seconds (0: to the clip's end) |
| `subject` | `[number, number, number, number]` | the subject to keep in front of the 3D and out of the solve: a box around it on the first frame, [x, y, w, h] in 0..1 of the frame (EdgeTAM tracks it from there) |
| `height` | `number` | the camera's height over the ground in metres (1.6 by default: handheld) |
| `fov` | `number` | the vertical field of view in degrees, for a shot without parallax (a tripod, a pan); a solve with structure finds its own |
| `smooth` | `number` | frames of smoothing on the solved path (1 by default; 0 for none) |
| `solve` | `MatchmoveSolve` | a solve to draw from instead of solving (`raea track CLIP -o solve.json`, imported) |
| `crop` | `number \| [number, number]` | when the frame's shape isn't the footage's (9:16 from 16:9), where the frame sits across it: 0 its left edge, 1 its right, 0.5 the middle (the default); [x, y] for both axes. It may change from frame to frame |
| `zoom` | `number` | 1 or more: a tighter window on the footage (a punch-in), still locked |
| `cutout` | `boolean` | draw the subject's cut-out over the 3D (true by default when there's a subject) |
| `toneMapping` | `"none" \| "tony" \| "aces" \| "agx" \| "neutral" \| "reinhard"` | the scene's tone mapping: "none" by default, which leaves the footage's colours as they are |
| `children` | `ReactNode` | the 3D that stands in the shot, in the solved world |

### `MatchmoveSolve`

What `raea track` writes and <Matchmove> draws from.

| Field | Type | What it is |
| --- | --- | --- |
| `mode` (required) | `"sfm" \| "rotation" \| "static"` | "sfm": a full solve with structure; "rotation": the camera only turns (too little parallax, or too few frames registered); "static": it doesn't move |
| `width` (required) | `number` | the footage's size in pixels, and its frame rate |
| `height` (required) | `number` | the footage's height in pixels |
| `fps` (required) | `number` | the footage's frame rate |
| `from` (required) | `number` | where in the clip the solve starts, seconds |
| `seconds` (required) | `number` | how long it runs, seconds |
| `frames` (required) | `number` | how many frames it solved |
| `fov` (required) | `number` | vertical field of view, degrees; focal length in pixels; the lens's radial distortion |
| `focal` (required) | `number` | the focal length in pixels, at the footage's full size |
| `k1` (required) | `number` | the lens's radial distortion |
| `registered` (required) | `number` | how many frames COLMAP registered; the rest were interpolated |
| `error` (required) | `number \| null` | mean reprojection error in pixels (null without structure) |
| `camera_height` (required) | `number` | the camera's height over the ground, metres, that set the world's scale |
| `cameras` (required) | `MatchmoveCamera[]` | the camera a frame |
| `points` (required) | `Vec3[]` | the sparse points, in the solved world |
| `ground` (required) | `{ y: number; normal: Vec3 }` | the ground plane: its height (0) and normal (+Y) |
| `cut` (required) | `string \| null` | the subject's cut-outs, a path with %04d for the frame (1-based), or null |
| `notes` (required) | `string[]` | what the solver noticed: fallbacks, an assumed field of view, interpolated frames |

### `MatchmoveState`

Each frame's state under a <Matchmove>, for the 3D inside it.

| Field | Type | What it is |
| --- | --- | --- |
| `solve` (required) | `MatchmoveSolve` | the solve it draws from |
| `index` (required) | `number` | the solve's frame shown now (0-based), and the clip's time |
| `time` (required) | `number` | the clip's time at this frame, seconds |
| `camera` (required) | `MatchmoveCamera` | the solve's camera for this frame |
| `project` (required) | `(p: Vec3) => [number, number, number]` | where a point of the solved world lands in the composition, in pixels ([x, y], and its distance in front of the camera; behind it when that's 0 or less) |

### `MoveKind`

```ts
type MoveKind = "static" | "dolly" | "truck" | "pedestal" | "crane" | "pan" | "tilt" | "orbit" | "dollyZoom" | "path"
```

### `Projection`

Bevy's Projection enum, as it reflects: fov in radians (Bevy's default PI/4).

```ts
type Projection = | { Perspective: { fov?: number; near?: number; far?: number } } | { Orthographic: { near?: number; far?: number; scaling_mode?: { FixedVertical: { viewport_height: number } } } }
```

### `ShotMove`

A move: where the camera starts, what it looks at, and how it moves over `frames` from frame `at`.

```ts
type ShotMove = { kind: MoveKind; /** The camera's start. */ from: CameraVec3; /** What it looks at (the subject). */ look: CameraVec3; /** The frame it starts moving (0 if left out). */ at?: number; /** How many frames it moves for. */ frames: number; /** Metres: dolly in (negative: out), truck right (negative:…
```

<!-- generated:end -->
