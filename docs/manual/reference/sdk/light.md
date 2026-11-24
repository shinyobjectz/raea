---
title: "raea/light"
summary: "Bevy's lights in Bevy's units, environments, skies and daylight."
order: 64
sources: ["sdk/raea/src/light.ts"]
---

These are Bevy's lights in Bevy's physical units: a `DirectionalLight` in lux, a `PointLight` and `SpotLight` in
lumens, a `RectLight` for soft area light, and ambient light. `<Daylight>` and `daylight()` place the sun for a time
and place; `<Environment>`, `<EnvironmentMapLight>` and `<Skybox>` light a scene from an image; `<Atmosphere>` draws
Bevy's sky. `roomExposure` gives the camera exposure that suits an indoor scene's lights.

```tsx
import { AmbientLight, DirectionalLight, PointLight } from "raea/light";
import { Transform } from "raea/transform";

function Lights() {
  return (
    <>
      <AmbientLight brightness={160} />
      <DirectionalLight transform={Transform.fromXyz(3, 5, 4).lookingAt([0, 0, 0])} illuminance={11200} shadow_maps_enabled />
      <PointLight transform={Transform.fromXyz(-1, 2, 1)} intensity={800000} color="#ffd9a0" />
    </>
  );
}
```

<!-- generated:begin by tools/manual-ref/sdk.ts from the SDK's exports and doc comments; edit outside this block -->

```ts
import { AmbientLight, Atmosphere, bounce, clearSun } from "raea/light"
```

`raea/light` exports 15 components, 9 functions, 2 values, 15 types. Bevy's counterpart: `bevy_light`. Source: [sdk/raea/src/light.ts](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/src/light.ts).

## Components

### `AmbientLight`

Bevy's AmbientLight (on the camera in 0.20; GlobalAmbientLight is the same light as a resource): light from
everywhere, in cd/m².

Bevy: `bevy_light::ambient_light::AmbientLight`.

Takes [`AmbientLightProps`](#ambientlightprops)'s props.

Used in: [defaults/parts](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/defaults/parts.tsx), [shader_advanced/deferred_raymarch](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/shader_advanced/deferred_raymarch.tsx), [manual: turntable](https://github.com/shinyobjectz/raea/blob/main/docs/manual/examples/turntable.tsx).

### `Atmosphere`

A procedural sky: Bevy's physically based atmosphere (Earth's air), drawn behind everything and lit by the
DirectionalLights, so a sun low on the horizon makes a sunset. Inside the Canvas3D, once.

Bevy: `bevy_light::Atmosphere`.

Takes [`AtmosphereProps`](3d.md#atmosphereprops)'s props.

### `Daylight`

The day's sun and sky: the sun as a shadowed directional light at its lux and colour, the panorama as the sky seen
through the windows at its true brightness (it lights nothing: an Environment is not shadowed, and would light the
room as if it had no walls), the room's bounce (`indoor`, its daylight factor, 2% by default) as an even fill; the
windows' own light comes from <WindowLight>.

| Prop | Type | What it does |
| --- | --- | --- |
| `day` (required) | `Day` | the day outside, from `daylight` |
| `indoor` | `number` | the room's daylight factor, the share of the outside light its bounce gives as an even fill (0.02) |
| `softShadow` | `boolean \| number` | the sun's shadow softness (0.6): true, or a number, as a DirectionalLight's |

### `DirectionalLight`

Bevy's DirectionalLight: parallel light shining along its transform's -Z, in lux.

Bevy: `bevy_light::directional_light::DirectionalLight`.

| Prop | Type | What it does |
| --- | --- | --- |
| `illuminance` | `number` | lux on a surface facing it (Bevy's default 10,000, light_consts::lux::AMBIENT_DAYLIGHT; full sun is about 100,000) |
| `shadow_maps_enabled` | `boolean` | Bevy: Whether this light casts shadows. |
| `soft_shadow_size` | `number \| null` | soft shadows (PCSS): the sun's size as its shadows see it, in metres |
| `children` | `ReactNode` | Bevy components on the light: shadow settings, cascades |

Also takes the props of `LightCommon` (`color`, `name`, `visibility`, `transform`, `shadow_depth_bias`, `shadow_normal_bias`, `volumetric`).

Used in: [ui/render_ui_to_texture](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/render_ui_to_texture.tsx), [3d/fog_volumes](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/fog_volumes.tsx), [3d/shadow_caster_receiver](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/shadow_caster_receiver.tsx), [3d/split_screen](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/split_screen.tsx), [3d/pbr](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/pbr.tsx), [3d/mesh_ray_cast](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/mesh_ray_cast.tsx), [3d/atmospheric_fog](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/atmospheric_fog.tsx), [3d/3d_viewport_to_world](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/3d_viewport_to_world.tsx), and 16 more.

### `Environment`

Light from all around, out of a panorama: soft light from the sky and reflections of it in every glossy or metal
surface (image-based lighting), and with `background` the panorama as the sky. Put it inside the Canvas3D, once a
frame; its src, intensity, rotation and background may change from frame to frame (a cut from a hall to a street).
With `room` in place of a panorama it is three.js's RoomEnvironment, soft studio light with no file.
Bevy's EnvironmentMapLight with maps already filtered is <EnvironmentMapLight>.
Its panorama is filtered by Bevy's generator and, with `background`, drawn as the sky (engine/render
environment.rs):

Bevy: `bevy_light::probe::GeneratedEnvironmentMapLight. Bevy's Skybox from its own cubemap is <Skybox>.`.

Takes [`EnvironmentProps`](3d.md#environmentprops)'s props.

### `EnvironmentMapLight`

Bevy's EnvironmentMapLight with its own prefiltered maps, as Bevy's examples put one on the camera: light from all around and its reflections, from cubemaps already filtered (an <Environment> filters a panorama
itself). Put it inside the Canvas3D, once; the first frame's holds.

Bevy: `bevy_light::probe::EnvironmentMapLight`.

Takes [`EnvironmentMapLightProps`](3d.md#environmentmaplightprops)'s props.

Used in: [3d/rotate_environment_map](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/rotate_environment_map.tsx), [3d/animated_material](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/animated_material.tsx), [3d/pbr](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/pbr.tsx), [gltf/load_gltf](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/gltf/load_gltf.tsx).

### `GlobalAmbientLight`

The same light as a resource, as Bevy 0.20 names it. (A function, not a const alias: an alias would keep 3D code in
a 2D bundle through the prelude.)

Takes [`AmbientLightProps`](#ambientlightprops)'s props.

Used in: [3d/fog_volumes](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/fog_volumes.tsx), [3d/fog](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/fog.tsx), [3d/spherical_area_lights](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/spherical_area_lights.tsx), [3d/lightmaps](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/lightmaps.tsx), [3d/spotlight](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/spotlight.tsx), [animation/animated_mesh](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/animation/animated_mesh.tsx), [animation/animated_transform](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/animation/animated_transform.tsx), [gltf/gltf_skinned_mesh](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/gltf/gltf_skinned_mesh.tsx).

### `Pathtracer`

Bevy's reference path tracer for the <Camera3d> it's inside: slow and exact, accumulating over frames while the
view holds still. For stills to check a look against, not for motion. Same GPU needs as <SolariLighting>.

| Prop | Type | What it does |
| --- | --- | --- |
| `reset` | `boolean` | start the accumulation again |
| `samples` | `number` | how many times a render draws each frame, each draw one more sample a pixel, the last kept (1 by default): 256 or more converges a reference still. Its accumulation starts afresh each frame, so a moved part isn't smeared |
| `media` | `boolean` | whether its rays walk the fog volumes, scattering in the air (true by default); false for a frame without the air |

### `PointLight`

Bevy's PointLight: a bulb at its transform's translation, in lumens.

Bevy: `bevy_light::point_light::PointLight`.

| Prop | Type | What it does |
| --- | --- | --- |
| `intensity` | `number` | lumens (Bevy's default 1,000,000; a 60 W bulb is about 800) |
| `range` | `number` | metres it reaches (20) |
| `radius` | `number` | the bulb's radius in metres (0): the size of its highlight on shiny surfaces (a spherical area light), and soft shadows' size when soft_shadows_enabled |
| `shadow_maps_enabled` | `boolean` | Bevy: Whether this light casts shadows. |
| `soft_shadows_enabled` | `boolean` | Bevy: Whether soft shadows are enabled. |
| `paint` | `boolean` | raea's paint look draws this light (3d.tsx's PointLight `paint`) |

Also takes the props of `LightCommon` (`name`, `transform`, `visibility`, `color`, `shadow_depth_bias`, `shadow_normal_bias`, `volumetric`, `children`).

Used in: [3d/fog](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/fog.tsx), [3d/vertex_colors](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/vertex_colors.tsx), [3d/rotate_environment_map](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/rotate_environment_map.tsx), [3d/transparency_3d](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/transparency_3d.tsx), [3d/spherical_area_lights](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/spherical_area_lights.tsx), [3d/3d_shapes](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/3d_shapes.tsx), [3d/shadow_caster_receiver](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/shadow_caster_receiver.tsx), [3d/two_passes](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/two_passes.tsx), and 18 more.

### `RectLight`

Bevy's RectLight: a rectangle of light facing its transform's -Z, in lumens (no shadows in Bevy 0.20).

Bevy: `bevy_light::rect_light::RectLight`.

| Prop | Type | What it does |
| --- | --- | --- |
| `intensity` | `number` | lumens (1,000,000) |
| `range` | `number` | Bevy: Cut-off for the light's area-of-effect. Fragments outside this range will not be affected by this light at all, so it's important to tune this together with `intensity` to prevent hard lighting cut-offs. |
| `width` | `number` | its face's size in metres (1 x 1) |
| `height` | `number` | Bevy: Height of the light rectangle (along local Y). |

Also takes the props of `LightCommon` (`color`, `visibility`, `transform`, `shadow_depth_bias`, `shadow_normal_bias`).

Used in: [3d/rect_light](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/rect_light.tsx).

### `Shadows`

Shadow quality for every light in the Canvas3D: map size, the sun's cascades, biases, filtering. Put it inside the
Canvas3D, once; each frame's is its own. mapSize sets both shadow maps' size, filter the camera's
filtering, and the cascade props the sun's cascades (engine/render scene3d.rs):

Bevy: `bevy_light::directional_light::DirectionalLightShadowMap`, `bevy_light::point_light::PointLightShadowMap`, `bevy_light::ShadowFilteringMethod`, `bevy_light::cascade::CascadeShadowConfig`.

Takes [`ShadowsProps`](3d.md#shadowsprops)'s props.

Used in: [3d/split_screen](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/split_screen.tsx), [stress_tests/many_foxes](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/stress_tests/many_foxes.tsx), [animation/animated_mesh](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/animation/animated_mesh.tsx).

### `Skybox`

Bevy's Skybox: a cubemap drawn behind everything, as Bevy's examples put one on the camera. Put it inside the
Canvas3D, once; its brightness and rotation may change each frame (an <Environment>'s `background` is the sky of
a panorama raea filters).

Bevy: `bevy_light::probe::Skybox`.

Takes [`SkyboxProps`](3d.md#skyboxprops)'s props.

Used in: [3d/rotate_environment_map](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/rotate_environment_map.tsx).

### `SolariLighting`

Ray-traced direct and indirect light for the <Camera3d> it's inside (Bevy's SolariLighting): shadows, bounce
light and reflections traced against the scene, in place of shadow maps. Needs a GPU with ray queries (Apple
silicon on Metal, RTX on Vulkan); elsewhere the scene draws raster-lit and raea says so. On the Mac MetalFX's
temporal denoiser cleans it up (RT4; denoise={false} shows the raw noise). A <Character> (skinned, morphed) stays
raster-lit: Bevy 0.20's Solari can't trace it, unless `dynamic` gives a skinned one a traced twin.

| Prop | Type | What it does |
| --- | --- | --- |
| `restir` | `boolean` | ReSTIR: reuse light samples between pixels and frames (finer shadows, steadier in hard light; slower). Off by default, as Bevy has it |
| `bounces` | `number` | how many times light bounces (Bevy's default when left out) |
| `reset` | `boolean` | start the lighting's history again (a cut) |
| `confidenceCap` | `number` | with ReSTIR, the most frames a pixel's reused samples may count for: higher is steadier and slower to follow a change in the light (Bevy's 8) |
| `primarySamples` | `number` | direct light samples at the camera's first hit, a pixel a frame (Bevy's 8). After MetalFX's denoiser 4 bought the window scene nothing over 8 and saved 1.5 ms at 1080p's pixels |
| `secondarySamples` | `number` | direct light samples at each bounce (Bevy's 4) |
| `firstBounce` | `number` | the chance a pixel traces its first bounce this frame, its light divided by it (Russian roulette): 0.5 halves the bounce's tracing, most of a dense forest's (1, every pixel every frame) |
| `cache` | `{ /** frames a cell's light is gathered over: steadier and slower to follow a change (B…` | Solari's world cache, the bounce light it keeps in cells across the scene |
| `scale` | `number` | draw the view at this fraction of its size a side (a third to 1) and let MetalFX's temporal scaler bring it up to the full size, so a game traces fewer pixels: 0.67 traces 45% of them. On the Mac; elsewhere it draws at the full size and raea says so. 1 (or left out): the full size; RAEA_SOLARI_SCALE overrides it |
| `denoise` | `boolean` | MetalFX's temporal denoiser on the lighting. false shows Solari's raw noise. Default true; RAEA_SOLARI_DENOISE=0 overrides it |
| `guides` | `boolean` | the denoiser guided by Solari's own guides, where a mirror's pixel takes what it reflects; false guides it by the G-buffer. Default true; RAEA_SOLARI_PSR=0 overrides it |
| `reflectionDepth` | `boolean` | a glossy surface's pixel as far off as what it reflects in the denoiser's depth, so a reflection doesn't trail what it shows. Default true; RAEA_SOLARI_PSR_DEPTH=0 overrides it |
| `emissive` | `"taa" \| "split" \| "mask"` | an emissive surface's own light: "taa" taken out before the denoiser and gathered over frames (RT11, RT32), "split" each frame's alone, "mask" left in for the denoiser. Default "taa"; RAEA_SOLARI_EMISSIVE overrides it |
| `upscale` | `"one" \| "two"` | a view drawn small (scale below 1): denoised and brought up in "one" pass, or denoised small and scaled in "two". Default "one"; RAEA_SOLARI_UPSCALE overrides it |
| `preexposure` | `number` | what MetalFX divides the light by before it denoises and multiplies it by after. Higher keeps more of a rare bright sample (a lamp found by one path in many) and lets more fireflies through: 1 kept 0.74 of the desk's light, 16 0.86, 64 0.91. Default 16; RAEA_METALFX_PREEXPOSURE overrides it |
| `aa` | `boolean` | jitter a full-size view too, so the denoiser anti-aliases its edges as it gathers frames. Default true; RAEA_SOLARI_AA=0 overrides it |
| `jitter` | `"denoiser" \| "scaler" \| "both" \| "flip" \| "none"` | which of MetalFX's passes is told the camera's jitter: "denoiser", "scaler", "both", "flip" (the denoiser, negated), "none" (and the view isn't jittered). For A/Bs of RT9; default "denoiser"; RAEA_SOLARI_JITTER ("" the denoiser, "0" none) overrides it |
| `emissiveSharpness` | `number` | how fast a sample's weight falls with its distance from a pixel's centre in the emission's history, exp(-k d²) in output pixels: lower is softer. Default 6; RAEA_EMIT_TAA_SHARP overrides it |
| `jitterFrames` | `number` | the jitter's cycle at the full size, in frames: a view drawn at s of its size a side cycles through jitterFrames / s² offsets, and an emissive twig glimpsed through it may stay away that long. Default 8 (DLSS's) |
| `dynamic` | `boolean` | the traced scene follows what moves (a trial, false by default): a skinned character's traced twin is posed each frame, so it casts traced shadows and bounces light. Decided as each mesh is first traced, so set it from the first frame. (A hidden part, or one a frame leaves out, leaves the traced scene with or without it.) |
| `constants` | `{ /** how far ReSTIR looks for a neighbour's light samples, pixels (30): smaller keeps …` | constants in Solari's and the denoiser's shaders, compiled in when the scene loads (a value changed later in the composition isn't drawn); each left out keeps the shader's own, given in brackets. <ShaderConstants> reaches the rest |

### `SpotLight`

Bevy's SpotLight: a cone along its transform's -Z, in lumens.

Bevy: `bevy_light::spot_light::SpotLight`.

| Prop | Type | What it does |
| --- | --- | --- |
| `outer_angle` | `number` | the cone's half-angle in radians (Bevy's default PI/4), up to PI/2 |
| `inner_angle` | `number` | where the light starts to fall off, in radians from the axis (0) |

Also takes the props of [`PointLightProps`](#pointlightprops) (`intensity`, `range`, `radius`, `shadow_maps_enabled`, `soft_shadows_enabled`, `paint`); `LightCommon` (`name`, `transform`, `visibility`, `color`, `shadow_depth_bias`, `shadow_normal_bias`, `volumetric`, `children`).

Used in: [3d/spotlight](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/spotlight.tsx), [defaults/parts](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/defaults/parts.tsx).

### `WindowLight`

A window's light as a lamp (windowLight), wide and soft, shadowed.

| Prop | Type | What it does |
| --- | --- | --- |
| `day` (required) | `Day` | the day outside |
| `room` (required) | `Room` | the room the window is in |
| `opening` (required) | `Opening` | the window: one of the room's openings |
| `transmit` | `number` | the share of light the glass lets through (0.75) |
| `ground` | `number` | the ground's reflectance outside (0.2) |
| `name` | `string` | the light's name (window-WALL-AT by default) |

## Functions

### `bounce`

The room's bounce: the light every surface gets from the room's other surfaces once the windows' light has struck
them, as luminance (a daylight factor `indoor` of the day's light on the ground, over π) and colour (the sun's and the
sky's, mixed in mired by their shares of it). Not the sky's colour: a room's fill is its sunlit floor and walls.

```ts
function bounce(day: Day, indoor?: number): { nits: number; kelvin: number; }
```

### `clearSun`

A clear sky's direct sun at `elevation` degrees, lux on a surface facing it: the solar constant's light (1361 W/m²
at 93 lm/W) through the air mass (Kasten and Young) by Meinel's attenuation.

```ts
function clearSun(elevation: number): number
```

### `daylight`

The day from a sky panorama and, to put the sun where it was, a place and time (else the panorama's own sun). An
overcast panorama (no sun) is lit as an overcast sky of `overcastLux` on the ground (10 000 by default).

```ts
function daylight(o: { sky: string; lat?: number; lon?: number; time?: string; north?: number; overcastLux?: number; }): Day
```

### `kelvin`

A colour temperature as a light's colour (Tanner Helland's fit to the Planckian locus), sRGB hex.

```ts
function kelvin(k: number): string
```

### `readSky`

A sky panorama read by the engine: its sun (if it has one) and its light's split (scene.sky).

```ts
function readSky(src: string, rotation?: number): SkyRead
```

### `roomExposure`

The exposure a camera sets for the room, as <ToneMapping exposure>: an incident meter's EV100 (log2 E·100/250, as
`raea scene light` reads it) on the room's light, its bounce (`indoor`, as Daylight's) plus `extra` lux, and
the engine's exposure for it (EV100 = 9.7 - exposure). The windows burn out, as a room's do in a real camera.

```ts
function roomExposure(day: Day, indoor?: number, extra?: number): number
```

### `sunDirection`

The world direction toward a sun at `azimuth` and `elevation` (degrees), north along -z turned by `north` (radians
about +y: where the set's north is), east along +x.

```ts
function sunDirection(azimuth: number, elevation: number, north?: number): Vec3
```

### `sunPosition`

Where the sun stands at a place and moment (NOAA's low-precision solar position, good to a few tenths of a degree):
azimuth from north, clockwise, and elevation over the horizon, in degrees. `time` is ISO 8601 with its offset.

```ts
function sunPosition(lat: number, lon: number, time: string): { azimuth: number; elevation: number; }
```

### `windowLight`

What a window lets in besides the sun: its sky above and the ground below (`ground` its reflectance), seen through
glass of `transmit`, as a lamp in the opening that faces into the room with a window's candela on its axis (a
Lambertian pane: luminance times area). A bay's three faces count as its mouth.

```ts
function windowLight(day: Day, room: Room, o: Opening, { transmit, ground }?: { transmit?: number; ground?: number; }): { position: Vec3; target: Vec3; lumens: number; color: string; candela: number; }
```

## Values

### `light_consts`

Bevy's named light values (bevy_light light_consts), for a light's own units: `illuminance={light_consts.lux.DIRECT_SUNLIGHT}`,
`intensity={light_consts.lumens.VERY_LARGE_CINEMA_LIGHT}`.

Used in: [3d/shadow_caster_receiver](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/shadow_caster_receiver.tsx).

### `UNITS`

The engine's one relative unit left (core/scene props.rs ENV): an Environment of 1 is its panorama times 1000 cd/m².
Lights are given in Bevy's own units.

## Types

### `AmbientLightProps`

| Field | Type | What it is |
| --- | --- | --- |
| `name` | `string` | the entity's name, for picking, the inspector and world.* |
| `color` | `string` | its colour, as CSS writes one (a name, hex, rgb()) |
| `brightness` | `number` | cd/m² (Bevy's default 80) |

### `AtmosphereProps`

| Field | Type | What it is |
| --- | --- | --- |
| `light` | `boolean` | it lights the scene too, as a sky does (ambient and reflections from the sky it draws) |
| `groundAlbedo` | `number` | the ground's brightness the air's scattering sees, 0..1 (0.3 by default) |

### `Day`

The day outside, in physical units, for one set.

| Field | Type | What it is |
| --- | --- | --- |
| `sky` (required) | `string` | the sky panorama's file |
| `rotation` (required) | `number` | the panorama's turn that puts its sun where the day's is (radians, as <Environment rotation>) |
| `sunDir` (required) | `Vec3` | toward the sun, world |
| `sunLux` (required) | `number` | the sun's lux on a surface facing it, its colour temperature |
| `sunK` (required) | `number` | the sun's colour temperature, kelvin |
| `sunGround` (required) | `number` | the sun's and the sky's lux on level ground outside |
| `skyGround` (required) | `number` | the sky's lux on level ground outside |
| `skyNits` (required) | `number` | the sky's mean luminance (cd/m²) and colour temperature |
| `skyK` (required) | `number` | the sky's colour temperature, kelvin |
| `scale` (required) | `number` | cd/m² per unit of the panorama |
| `elevation` (required) | `number` | the sun's elevation (degrees) the light was made for, and the panorama's own (a different height shows in its shadows' length outside, never in the room's light) |
| `skyElevation` (required) | `number \| null` | the panorama's own sun elevation, degrees (null for an overcast one) |

### `DaylightProps`

The props of [`Daylight`](#daylight), tabled there.

### `DirectionalLightProps`

The props of [`DirectionalLight`](#directionallight), tabled there.

### `EnvironmentMapLightProps`

| Field | Type | What it is |
| --- | --- | --- |
| `diffuse_map` (required) | `string` | the prefiltered diffuse (irradiance) cubemap, a.ktx2 file: staticFile("environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2") |
| `specular_map` (required) | `string` | the prefiltered specular (radiance) cubemap with its mips, a.ktx2 file |
| `intensity` | `number` | cd/m² (Bevy's default 1) |
| `rotation` | `[number, number, number, number]` | its turn, glam's Quat [x, y, z, w] (Bevy's identity): Quat.from_rotation_y(0.2 * t) turns it each frame |

### `EnvironmentProps`

| Field | Type | What it is |
| --- | --- | --- |
| `src` | `string` | an equirectangular panorama in the media folder: a Radiance.hdr (Poly Haven's), an OpenEXR.exr, or an 8-bit.png/.jpg. Change it between frames to cut from one place to another. Give it or `room`, not both |
| `room` | `boolean` | three.js's RoomEnvironment in place of a panorama: a white studio room with soft light boxes, made by raea with no file. It is the light a three.js scene gets from PMREMGenerator.fromScene(new RoomEnvironment(), blur), which <R3F> turns into this |
| `blur` | `number` | with `room`, its blur in radians before it lights (fromScene's sigma: three's examples use 0.04); 0 by default |
| `intensity` | `number` | how strongly it lights the scene (1 by default: a sky about as bright as an AmbientLight of 2.5 overall); animate it, or set 0 to switch it off for a shot |
| `rotation` | `number` | a turn about the vertical axis, in radians, for both the light and the sky: puts the sun where the shot wants it |
| `background` | `boolean` | draw the panorama as the sky behind everything (off by default: light only) |
| `backgroundIntensity` | `number` | the sky's brightness, when it should differ from the light's (`intensity` by default) |
| `portals` | `[number, number, number][][]` | the openings the sky lights a room through (a skylight, a window), each its four corners in order round it, in metres (a rectangle or any parallelogram). Traced lighting (Solari, the path tracer) samples the sky through each as it samples a lamp, so a room lit through a small opening gets that light smooth and whole, where rays bounced at random find it too rarely for the denoiser to keep (photo 5's hall: half its floor's sky light). Light passes both ways; up to 8; the first frame's hold. Raster lighting doesn't read them |

### `PathtracerProps`

The props of [`Pathtracer`](#pathtracer), tabled there.

### `PointLightProps`

The props of [`PointLight`](#pointlight), tabled there.

### `RectLightProps`

The props of [`RectLight`](#rectlight), tabled there.

### `ShadowsProps`

| Field | Type | What it is |
| --- | --- | --- |
| `mapSize` | `1024 \| 2048 \| 4096 \| 8192` | each shadow map's size in texels (4096 by default; 8192 is sharper still, at four times the memory and about 2-3 ms a frame at 1080p) |
| `distance` | `number` | how far from the camera the sun's shadows reach, in metres (150 by default): the shorter, the sharper they are |
| `firstCascade` | `number` | where the sun's first, sharpest cascade ends, in metres (8 by default): the distance of the nearest subject |
| `cascades` | `number` | how many cascades split the distance (4 by default, 1-4) |
| `overlap` | `number` | how much neighbouring cascades overlap, 0-1 (0.3 by default) so the seam between them doesn't show |
| `bias` | `number` | depth bias: raise it if surfaces facing the light show stripes of their own shadow (acne); too high and shadows detach from what casts them (0.02 by default for the sun, as Bevy's) |
| `normalBias` | `number` | normal bias, in texels: the same trade-off along the surface's normal (1.8 by default for the sun) |
| `filter` | `"gaussian" \| "hardware"` | "gaussian" (the default: soft, smooth edges) or "hardware" (2x2, crisper and cheaper) |

### `SkyboxProps`

| Field | Type | What it is |
| --- | --- | --- |
| `image` (required) | `string` | the sky's cubemap, a.ktx2 file (Bevy's examples use the specular map: environment_maps/pisa_specular_rgb9e5_zstd.ktx2) |
| `brightness` | `number` | cd/m² (Bevy's default 0, a black sky: Bevy's examples give it 1000 to 5000) |
| `rotation` | `[number, number, number, number]` | its turn, glam's Quat [x, y, z, w] (Bevy's identity) |

### `SolariLightingProps`

The props of [`SolariLighting`](#solarilighting), tabled there.

### `SpotLightProps`

The props of [`SpotLight`](#spotlight), tabled there.

### `WindowLightProps`

The props of [`WindowLight`](#windowlight), tabled there.

<!-- generated:end -->
