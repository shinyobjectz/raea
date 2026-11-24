---
title: "raea/post_process"
summary: "What's done to the frame after it's drawn: tone mapping, bloom, depth of field, motion blur, grading and outlines."
order: 68
sources: ["sdk/raea/src/post_process.ts"]
---

These are Bevy's post-processing passes, placed beside the camera: `<ToneMapping>`, `<Bloom>`, `<DepthOfField>`,
`<MotionBlur>`, `<AntiAliasing>`, lens distortion and fisheye, plus raea's `<Grade>` for colour grading, `<Outline>`
and `<Look>`, which sets a whole finish at once. `<FramePass>` and `<Compute>` run your own shaders on the frame.

```tsx
import { Camera3d } from "raea/camera";
import { Bloom, ToneMapping } from "raea/post_process";
import { Transform } from "raea/transform";

const Glow = () => (
  <>
    <Camera3d transform={Transform.fromXyz(-2, 2.5, 5).lookingAt([0, 0, 0])} />
    <ToneMapping mode="tony" />
    <Bloom intensity={1} threshold={0} />
  </>
);
```

<!-- generated:begin by tools/manual-ref/sdk.ts from the SDK's exports and doc comments; edit outside this block -->

```ts
import { AntiAliasing, Bloom, Compute, DepthOfField } from "raea/post_process"
```

`raea/post_process` exports 12 components, 1 function, 1 value, 15 types. Bevy's counterpart: `bevy_post_process, bevy_anti_alias`. Source: [sdk/raea/src/post_process.ts](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/src/post_process.ts).

## Components

### `AntiAliasing`

How the 3D's edges are smoothed. Put it inside the Canvas3D, once; each frame's is its own.

Bevy: `bevy_anti_alias::smaa::Smaa`, `bevy_anti_alias::fxaa::Fxaa`.

Takes [`AntiAliasingProps`](3d.md#antialiasingprops)'s props.

### `Bloom`

Glow around bright parts (emissive materials, strong highlights). Put it inside the Canvas3D.

Bevy: `bevy_post_process::bloom::settings::Bloom`.

Takes [`BloomProps`](3d.md#bloomprops)'s props.

### `Compute`

A compute over the frame, in the 3D beside the camera; several run in the order written, before the cameras draw.

Rust: `raea_sys_compute::Computes`.

### `DepthOfField`

A lens's depth of field: what's nearer or farther than the camera's `focus` blurs. Put it inside the Canvas3D;
pull focus by animating the camera's `focus`. Inside a Camera3d it is Bevy's component on the camera, its
focal_distance the camera's focus.

Bevy: `bevy_post_process::dof::DepthOfField`.

Takes [`DepthOfFieldProps`](3d.md#depthoffieldprops)'s props.

### `Fisheye`

A fisheye lens on the camera: a pass over the 3D after tone mapping (the page over it stays straight). Every value
animates per frame; a lens's vignette and colour fringes are <Grade>'s. Put it inside the Canvas3D.

Takes [`FisheyeProps`](3d.md#fisheyeprops)'s props.

### `FramePass`

A full-screen pass over the scene camera's frame, beside the camera; several run in the order written. With a
frame <Look> they run on its cells, before the look's composite.

Rust: `raea_sys_framepass::FramePasses`.

Takes [`FramePassProps`](3d.md#framepassprops)'s props.

### `Grade`

The camera's colour grade (lift, gamma, gain and saturation per range of light, before tone mapping), a lens
vignette and colour fringes: one grade over every scene, so a film looks of a piece. Put it inside the Canvas3D.

Bevy: `bevy_render::view::ColorGrading`, `bevy_post_process::effect_stack::chromatic_aberration::ChromaticAberration`, `bevy_post_process::effect_stack::vignette::Vignette`.

Takes [`GradeProps`](3d.md#gradeprops)'s props.

### `LensDistortion`

Barrel or pincushion distortion (Brown-Conrady k1, k2) on the camera, a pass after tone mapping as Fisheye's.
Put it inside the Canvas3D.

Bevy: `bevy_post_process::LensDistortion`.

Takes [`LensDistortionProps`](3d.md#lensdistortionprops)'s props.

### `Look`

The whole frame drawn as a model's look draws its model: every part of the scene in the one grain, its bloom,
fog and blur inside it. Put it inside the Canvas3D. mode="paint" or "cartoon" draws every standard material as
paint (engine/render paint.rs):

Bevy: `bevy_pbr::mesh_material::MeshMaterial3d<bevy_pbr::extended_material::ExtendedMaterial<bevy_pbr::pbr_material::StandardMaterial, raea_render::engine::paint::Paint>>`.

Takes [`LookProps`](3d.md#lookprops)'s props.

### `MotionBlur`

Motion blur from each part's and the camera's motion over the frame before (Bevy's MotionBlur): with it, each frame
is drawn after the one before it, so a frame's blur is the same in a render, a seek and play. A frame without it has
none. Put it inside the Canvas3D.

Bevy: `bevy_post_process::motion_blur::MotionBlur`.

Takes [`MotionBlurProps`](3d.md#motionblurprops)'s props.

### `Outline`

Ink lines where the scene's depth steps or its surfaces turn, drawn before bloom and tone mapping; under a <Look>
a cell wide (Return of the Obra Dinn's line). Put it inside the Canvas3D.

Takes [`OutlineProps`](3d.md#outlineprops)'s props.

### `ToneMapping`

The camera's tone mapping and exposure. Put it inside the Canvas3D.

Bevy: `bevy_render::view::Tonemapping`, `bevy_camera::camera::Exposure`.

Takes [`ToneMappingProps`](3d.md#tonemappingprops)'s props.

## Functions

### `gradeLook`

A grade look's props, with your changes over it: `<Grade {...gradeLook("cinematic", { vignette: 0.1 })} />`.

```ts
function gradeLook(look: GradeLook, over?: GradeProps): GradeProps
```

## Values

### `GRADES`

## Types

### `AntiAliasingProps`

| Field | Type | What it is |
| --- | --- | --- |
| `mode` (required) | `"msaa" \| "smaa" \| "fxaa" \| "none" \| "steady"` | "msaa" (4x multisampling, the default), "smaa" (morphological, high preset: the default with AmbientOcclusion, which can't multisample), "fxaa" (cheapest, softest), "none", or "steady": raea's temporal filter, each frame drawn from a slightly different place inside its pixels and blended with the frames before it, so what one frame can only draw as a pattern of pixels (levels cross-fading by distance, far detail a pixel wide, noisy light) settles to its true share; single-sampled, with depth and motion prepasses (on Berlin's fly, faster than msaa). The others draw a frame from that frame alone, so a render equals its play; "steady" needs the frames before, so a render that starts partway through differs from its play for its first few frames |
| `steady` | `{ still?: number; moving?: number; px?: number }` | "steady"'s rates: the frames a still pixel's history averages (32), a moving one's (8), and the motion in pixels at which a pixel counts as moving (4). Fewer frames follow change sooner and settle less |

### `BloomProps`

| Field | Type | What it is |
| --- | --- | --- |
| `intensity` | `number` | 1 is a natural glow, 2 strong; 0 none |
| `threshold` | `number` | only what's brighter than this glows (0 = everything a little; 1 = only emissive parts and highlights) |
| `skip` | `string[]` | composition passes (a part's `pass`) left out of it: they neither glow nor take the glow |
| `passes` | `Record<string, number>` | each named pass's share of it, 0 (as skip) to 1 (all of it, as "scene" has) |
| `additive` | `boolean` | the glow added on top of the frame, brightening it, as three.js's and postprocessing's bloom do; by default it takes its light from around the bright parts (Bevy's energy-conserving bloom) |

### `ComputeProps`

A compute shader (raea-sys-compute). Its source has one `@compute` entry point and reads `time()` (seconds),
`u_NAME()` for each uniform, each buffer as `NAME: array<f32>` (read and written) and each image as
`NAME: texture_storage_2d<rgba8unorm, write>`.

| Field | Type | What it is |
| --- | --- | --- |
| `file` | `string` | a shader file compiled as written, staticFile("shaders/x.wesl"), in place of wesl: its `@group(0)` storage textures and uniforms its own, as a Bevy example's render-graph node binds them |
| `entry` | `string \| null` | a file's entry point this frame (null or left out: nothing dispatched), as a node picks its pipeline |
| `bind` | `Record<string, string>` | a file's storage textures this frame, each an image's key ({ input: "a", output: "b" }), as a node picks its bind group (a ping-pong swaps them frame by frame) |
| `wesl` | `string` | the source in WESL (a superset of WGSL) |
| `wgsl` | `string` | the same in plain WGSL |
| `workgroups` (required) | `number[]` | how many workgroups a frame dispatches: [x], [x, y] or [x, y, z] |
| `uniforms` | `Record<string, number \| number[] \| string \| Record<string, number \| number[] \| string \|…` | up to 16, as a surface shader's; a file's struct field may be { start: NAME } or { end: NAME }: the range of the mesh buffer NAME binds in its slab, in its array's elements (compute_mesh's `range.start * 8`) |
| `buffers` | `Record<string, { size: number } \| number[] \| StorageValue \| MeshSlab>` | up to 4, the compute's own, kept from frame to frame: { size } (that many f32s, zeroed) or its first contents. A file's: each `var<storage>` it declares, its first contents in its type's layout or a mesh's slab |
| `images` | `Record<string, string \| { src: string; sampler?: SamplerSpec }>` | up to 4 "render:NAME" (or "render:NAME:WxH") images it writes, which any shader's texture reads by that name (a file's may be { src, sampler }: the sampler it is drawn with) |
| `defs` | `ShaderDefs` | shader defs, as a surface shader's |

### `DepthOfFieldProps`

| Field | Type | What it is |
| --- | --- | --- |
| `aperture` | `number` | the lens's f-number: lower blurs more (1.4 dreamy, 2.8 cinematic, 8 nearly sharp); 2.8 by default |
| `mode` | `"bokeh" \| "gaussian"` | "bokeh" (round, as a real lens; the default) or "gaussian" (soft, cheaper) |
| `maxBlur` | `number` | the largest blur, in pixels of the frame (64 by default) |
| `focal_distance` | `number` | Bevy's fields, as Bevy's DepthOfField component on a Camera3d (inside it), Bevy's defaults: the distance in focus (10; the camera's `focus`), the f-number (1), the largest blur in pixels (64), "Bokeh" or "Gaussian" |
| `aperture_f_stops` | `number` | the f-number (1): lower is a shallower focus |
| `max_circle_of_confusion_diameter` | `number` | the largest blur, in px (64) |

### `FisheyeProps`

| Field | Type | What it is |
| --- | --- | --- |
| `strength` | `number` | 0..1 (1 by default): 0 is the camera's own picture, pixel for pixel; ramp it in and out on beats |
| `mode` | `"equidistant" \| "stereographic"` | "equidistant" (the default) or "stereographic" (gentler at the edges) |
| `fov` | `number` | the vertical field of view, in degrees, the camera widens to at strength 1 (its own by default): 100-150 for a real fisheye; the picture is still remapped corner to corner, so nothing goes black |
| `zoom` | `number` | above 1 crops in, below 1 shows past the picture's edge (see `edges`) |
| `edges` | `"clamp" \| "black"` | what shows where the lens looks past the picture: its edge pixels ("clamp", the default) or "black" |
| `animate` | `Animate3d` | declared animation of its numbers (strength, fov, k1, k2, zoom) |

### `FramePassProps`

A shader over the whole frame. Its source defines
 fn shade(in: FullscreenVertexOutput) -> vec4<f32>
(or its own

| Field | Type | What it is |
| --- | --- | --- |
| `file` | `string` | a shader file compiled as written, staticFile("shaders/x.wesl"), in place of wesl: its `@group(0)` bindings its own, as custom_post_processing's pipeline binds them: each texture_2d<f32> the frame (the one `frame` names) or an image of `textures`, each sampler Bevy's default (nearest) or `samplers`' one, each var<uniform> its value in `uniforms` (which may change frame by frame) |
| `frame` | `string` | a file's texture that reads the frame as drawn so far |
| `entry` | `string` | a file's fragment entry point ("fragment" by default) |
| `samplers` | `Record<string, SamplerSpec>` | a file's samplers by name |
| `wesl` | `string` | the source in WESL (a superset of WGSL) |
| `wgsl` | `string` | the same in plain WGSL |
| `uniforms` | `Record<string, number \| number[] \| string \| StorageValue>` | up to 16, as a surface shader's (a file's: each var<uniform>, a struct's { field: value }); each frame's own, so a camera's matrices, a time or a frame index handed in move through a render (each keeps its first frame's type) |
| `textures` | `Record<string, string \| Record<string, unknown>>` | up to 4, as a surface shader's, each read with its own sampler; the first frame's for the whole render (pick a layer of a texture array with a uniform to change what's read frame by frame) |
| `defs` | `ShaderDefs` | shader defs, as a surface shader's |
| `at` | `"before-tonemap" \| "after-tonemap"` | "after-tonemap" (display colours; the default) or "before-tonemap" (the HDR frame, after bloom) |
| `prepass` | `boolean \| { depth?: boolean; normal?: boolean }` | the camera's depth and normal prepasses, for depth(uv) and normal(uv) (turns its multisampling off) |

### `GradeLook`

```ts
type GradeLook = "clean" | "cinematic" | "gritty" | "vintage"
```

### `GradeProps`

| Field | Type | What it is |
| --- | --- | --- |
| `temperature` | `number` | warmer (+) or cooler (-): a shift of the CIE x chromaticity, about -0.3 to 0.3 |
| `tint` | `number` | magenta (+) or green (-) |
| `saturation` | `number` | the whole frame's saturation after tone mapping (1 keeps it) |
| `exposure` | `number` | stops of exposure on every pixel just before tone mapping, unlit colours and emission included (0 keeps it): <ToneMapping exposure> is the camera's, which brightens only what lights shade |
| `shadows` | `GradeSection` | the grade on the shadows: saturation, contrast, gamma, gain, lift |
| `midtones` | `GradeSection` | the grade on the midtones |
| `highlights` | `GradeSection` | the grade on the highlights |
| `vignette` | `number \| { intensity?: number; radius?: number; smoothness?: number; color?: string }` | the corners darkened: 0 none, 1 black; or {intensity, radius (0.75), smoothness (5), color} |
| `aberration` | `number` | a lens's colour fringes at edges, as a fraction of the frame (0.005 subtle, 0.02 strong) |
| `skip` | `string[]` | composition passes (a part's `pass`) the grade leaves as drawn: no fringes or vignette on them, and on an untonemapped camera (a paint Look's) no colour grading either |
| `passes` | `Record<string, number>` | each named pass's share of the grade, 0 (as skip) to 1 |

### `GradeSection`

| Field | Type | What it is |
| --- | --- | --- |
| `saturation` | `number` | 1 keeps the colour; 0 grey; above 1 richer |
| `contrast` | `number` | 1 keeps it; above 1 harder |
| `gamma` | `number` | 1 keeps it; above 1 darker mids |
| `gain` | `number` | a multiplier: 1 keeps it |
| `lift` | `number` | added: 0 keeps it, positive lifts the blacks |

### `LensCommon`

| Field | Type | What it is |
| --- | --- | --- |
| `edges` | `"clamp" \| "black"` | what shows where the lens looks past the picture: its edge pixels ("clamp", the default) or "black" |
| `animate` | `Animate3d` | declared animation of its numbers (strength, fov, k1, k2, zoom) |

### `LensDistortionProps`

| Field | Type | What it is |
| --- | --- | --- |
| `k1` | `number` | Brown-Conrady: r' = r (1 + k1 r^2 + k2 r^4), r a share of the half-diagonal. Above 0 barrel, below 0 pincushion |
| `k2` | `number` | the r^4 term of the distortion |
| `zoom` | `number` | by default just enough for a barrel's corners to stay inside the picture (1 for pincushion) |
| `edges` | `"clamp" \| "black"` | what shows where the lens looks past the picture: its edge pixels ("clamp", the default) or "black" |
| `animate` | `Animate3d` | declared animation of its numbers (strength, fov, k1, k2, zoom) |

### `LookProps`

### `MotionBlurProps`

| Field | Type | What it is |
| --- | --- | --- |
| `shutterAngle` | `number` | the fraction of a frame the shutter is open (0.5 by default, a 180-degree shutter); above 1 smears further |
| `samples` | `number` | samples each way per pixel (4 by default) |
| `kind` | `"shutter" \| "bevy"` | "shutter" (the default: raea's, two taps a pixel for each sample each way) or "bevy" (Bevy's MotionBlur, to compare against) |
| `animate` | `Animate3d` | declared animation of its numbers |

### `OutlineProps`

| Field | Type | What it is |
| --- | --- | --- |
| `color` | `string` | the line's colour, a hex colour (black by default) |
| `depth` | `number` | a silhouette: how far behind the nearer surface (a fraction of its distance) a neighbour must be (0.04) |
| `normal` | `number` | a crease: how far the surface must turn between neighbours, 1 - cos of the angle (0.3, about 45 degrees; 0 for none) |
| `width` | `number` | the line's width in pixels (1 by default; a cartoon's line is about 6) |
| `far` | `number` | how far from the camera lines reach, in metres: they fade out over its last third, so a far field meeting the sky draws no line along the horizon (no limit by default) |
| `skip` | `string[]` | composition passes (a part's `pass`) drawn with no line: none on their pixels and no crease against them (a scene object's silhouette in front of one still draws, on the object) |

### `ToneMappingProps`

| Field | Type | What it is |
| --- | --- | --- |
| `mode` | `"tony" \| "aces" \| "agx" \| "neutral" \| "reinhard" \| "none"` | how bright light maps to the screen: "tony" (Bevy's default), "aces" (filmic, contrasty), "agx", "neutral", "reinhard", "none" |
| `exposure` | `number` | stops brighter (+) or darker (-) than the default exposure |

<!-- generated:end -->
