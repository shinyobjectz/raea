---
title: "raea/pbr"
summary: "Materials and volumes: StandardMaterial, extended and toon materials, custom shaders, decals, fog and fire."
order: 65
sources: ["sdk/raea/src/pbr.ts"]
---

`<StandardMaterial>` is Bevy's physically based material, with its fields named as Bevy names them (`base_color`,
`perceptual_roughness`, `metallic`, `emissive` and the rest). An `ExtendedMaterial` adds a shader extension on top of
it; `ToonMaterial` and the paint and wear extensions give stylised looks; `ShaderMesh` and `SurfaceShader` take your
own shader. Fog, volumetric fog, decals and fire are here too.

```tsx
import { Sphere } from "raea/3d";
import { Mesh3d } from "raea/mesh";
import { StandardMaterial } from "raea/pbr";

const Ball = () => (
  <Mesh3d>
    <Sphere radius={0.5} />
    <StandardMaterial base_color="#c03030" perceptual_roughness={0.2} metallic={0.1} clearcoat={1} />
  </Mesh3d>
);
```

<!-- generated:begin by tools/manual-ref/sdk.ts from the SDK's exports and doc comments; edit outside this block -->

```ts
import { AmbientOcclusion, Decal, ExtendedMaterial, Fire } from "raea/pbr"
```

`raea/pbr` exports 23 components, 1 function, 1 value, 37 types. Bevy's counterpart: `bevy_pbr, bevy_material`. Source: [sdk/raea/src/pbr.ts](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/src/pbr.ts).

## Components

### `AmbientOcclusion`

Screen-space ambient occlusion: creases, corners and the ground under things darken where the sky's and the room's
light can't reach. It darkens ambient and environment light (an <AmbientLight>, an <Environment>), not the direct
light of a sun or a lamp. Turns multisampling off, so the edges are smoothed by SMAA instead (see <AntiAliasing>).
Put it inside the Canvas3D, once; each frame's is its own, so a shot can have it and the next not.

Bevy: `bevy_pbr::ssao::ScreenSpaceAmbientOcclusion`.

Takes [`AmbientOcclusionProps`](3d.md#ambientocclusionprops)'s props.

### `Decal`

A picture pressed onto whatever surface it lies across: a label on a jar, a stain on a counter, tiles on a wall, a
sticker on a laptop (Bevy's forward decal; the camera gets a depth prepass for it). Placed and keyed like any part,
so it moves and fades with a render's clock and a game's play alike.

Bevy: `bevy_pbr::mesh_material::MeshMaterial3d<bevy_pbr::extended_material::ExtendedMaterial<bevy_pbr::pbr_material::StandardMaterial, bevy_pbr::decal::forward::ForwardDecalMaterialExt>>`.

Takes [`DecalProps`](3d.md#decalprops)'s props.

### `ExtendedMaterial`

Bevy's ExtendedMaterial: a StandardMaterial with one of raea's extensions (GridExtension, PaintExtension, ToonExtension) drawn on it.

  <ExtendedMaterial base={<StandardMaterial base_color="#cfc8bb" />} extension={<GridExtension cell={0.5} major={2} />} />

Bevy: `bevy_pbr::ExtendedMaterial`.

| Prop | Type | What it does |
| --- | --- | --- |
| `base` (required) | `ReactElement` | a <StandardMaterial>, or another <ExtendedMaterial> to put a second extension on (grid and paint both) |
| `extension` (required) | `ReactElement` | a <GridExtension>, <PaintExtension> or <ToonExtension> |

Used in: [shader/extended_material](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/shader/extended_material.tsx), [shader/extended_material_bindless](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/shader/extended_material_bindless.tsx).

### `Fire`

A fire: a flame drawn as a volume, its tongues rising, curling and tearing off as it burns, with a flickering light
on its surroundings. Every pixel of it is a function of the frame (rising 3D noise in a tapering envelope), so a
render of any frame equals its play. Stand it on its fuel (a pyre, a brazier); put embers and smoke round it
yourself.

Bevy: `bevy_pbr::mesh_material::MeshMaterial3d<raea_render::engine::fire::FireMaterial>`.

Takes [`FireProps`](3d.md#fireprops)'s props.

### `Fog`

Distance fog: far things fade into `color`. Inside the <Camera3d> it is Bevy's DistanceFog on that camera, with all of
Bevy's fields (an atmospheric falloff, the sun's colour); inside the Canvas3D it fogs the scene's camera by near, far
or density.

Bevy: `bevy_pbr::fog::DistanceFog`.

Takes [`FogProps`](3d.md#fogprops)'s props.

### `FogVolume`

A box of air that light shows in: dust in a sunbeam, smoke, a haze of blood. Needs a <VolumetricFog> in the
Canvas3D; lights marked `volumetric` throw shafts through it, and every light's glow fills it softly. A part, so it
keeps its name across frames: animate its density, colour and placement.

Bevy: `bevy_light::volumetric::FogVolume`.

Takes [`FogVolumeProps`](3d.md#fogvolumeprops)'s props.

Used in: [3d/fog_volumes](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/fog_volumes.tsx).

### `GridExtension`

raea's grid over a StandardMaterial, as an ExtendedMaterial's extension (engine/render grid.rs):

Bevy: `bevy_pbr::mesh_material::MeshMaterial3d<bevy_pbr::extended_material::ExtendedMaterial<bevy_pbr::pbr_material::StandardMaterial, raea_render::engine::grid::Grid>>`.

| Prop | Type | What it does |
| --- | --- | --- |
| `cell` (required) | `number` | world units between grid lines |
| `major` | `number` | cells between heavier lines (4) |
| `color` | `string` | the lines' colour |
| `edge` | `string` | the colour each Box face is outlined in ("" for none) |

### `Lightmap`

A light bake on the meshes of a <Model> its `mesh` names, lighting them in place of the scene's lights, scaled by
their material's lightmap_exposure (a <StandardMaterial lightmap_exposure> in the same Model, by material). Goes in
a <Model>.

Bevy: `bevy_pbr::lightmap::Lightmap`.

| Prop | Type | What it does |
| --- | --- | --- |
| `mesh` (required) | `string` | the glTF mesh it lies on, by name (Bevy's GltfMeshName): every primitive of it |
| `image` (required) | `string` | the bake: a.ktx2 (or other picture) file, staticFile("lightmaps/room.ktx2"), sampled by the mesh's second UVs |
| `uv_rect` | `{ min: [number, number]; max: [number, number] }` | the rect in the image those UVs are relative to, for meshes packed in one atlas ({ min: [0, 0], max: [1, 1] } by default) |
| `bicubic_sampling` | `boolean` | bicubic sampling: smoother, slower, may leak light (false by default) |

Used in: [3d/lightmaps](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/lightmaps.tsx).

### `PaintExtension`

| Prop | Type | What it does |
| --- | --- | --- |
| `breakup` | `number` | how far tone boundaries wander (0.5) |
| `scale` | `number` | the noise's size in metres (1) |
| `color` | `string` | the colour patches take |
| `amount` | `number` | how far toward `color` the patches go (0.6) |
| `world` | `number` | under a <Look cuts>: the world this mesh is in. 0 (the default): outside every cut. 1 to 4: inside that cut and no later one. -1: everywhere. Given alone it leaves the breakup off |

### `ProceduralExtension`

raea's procedural surface over a StandardMaterial, as an ExtendedMaterial's extension (systems/procmat):

  <ExtendedMaterial base={<StandardMaterial />} extension={<ProceduralExtension pattern="brick" dirt={0.3} />} />

Bevy: `bevy_pbr::mesh_material::MeshMaterial3d<bevy_pbr::extended_material::ExtendedMaterial<bevy_pbr::pbr_material::StandardMaterial, raea_sys_procmat::Procedural>>`.

| Prop | Type | What it does |
| --- | --- | --- |
| `pattern` (required) | `ProceduralSurface["pattern"]` | what it draws: noise, wood, marble, stone, brick, tiles, fabric, brushed or plaster |
| `colors` | `[string, string]` | the pattern's two colours |
| `scale` | `number` | how many metres across its features are |
| `rough_a` | `number` | the first colour's roughness, 0..1 |
| `rough_b` | `number` | the second colour's roughness, 0..1 |
| `bump` | `number` | how deep the pattern's relief looks (0 or more) |
| `tile` | `[number, number]` | brick and tiles: a cell's [width, height] in metres |
| `gap` | `number` | brick and tiles: the joints' width in metres |
| `gap_color` | `string` | the joints' colour |
| `dust` | `number` | 0..1 dust on what faces up |
| `dirt` | `number` | 0..1 dirt in joints, hollows, undersides and along the floor line |
| `edges` | `number` | 0..1 wear on the edges, lighter |
| `dust_color` | `string` | the dust's colour |
| `dirt_color` | `string` | the dirt's colour |
| `seed` | `number` | which randomness the pattern uses |

### `Refinish`

Every <Model> inside re-covered: each of its glTF materials (all, or the one each names by `material`) given the
colour, texture or finish each gives, the rest of it kept (a run of cabinets painted, the doors and drawers with them;
`map: null` takes the wood's picture off). A model's own <MeshStandardMaterial> children go first.

| Prop | Type | What it does |
| --- | --- | --- |
| `materials` (required) | `MeshStandardMaterialProps[]` | the finishes, each a <MeshStandardMaterial>'s props, over every glTF material or the one its `material` names |
| `children` | `ReactNode` | the models refinished |

### `ShaderConstants`

Sets constants in one of the engine's own shaders (raea's, Bevy's, Solari's) for this scene. Each `const NAME =...;`
named is rewritten when the scene loads, before anything compiles, so a value is tried in a composition instead of a
build; the shader's own value is the default. Compiled in once: a value that changes later in the composition isn't
drawn, and raea says so. A name the shader doesn't declare is reported and left. One per shader. The common ones have
typed props (<SolariLighting constants>); this reaches the rest.

| Prop | Type | What it does |
| --- | --- | --- |
| `shader` (required) | `string` | the shader as the engine embeds it, `<crate>/<path under its src>`: "bevy_solari/realtime/restir.wesl", "raea_render_raytrace/emit_taa.wgsl". To read its constants, put an empty file of that name in the project's engine-shaders/ folder and render once: raea fills it with the shader |
| `values` (required) | `Record<string, ShaderConstantValue>` | each `const NAME` to set, and its value; the const keeps its type (an array's or a vector's values as a list) |

### `ShaderExtension`

Also takes the props of [`SurfaceShaderSpec`](3d.md#surfaceshaderspec).

Used in: [shader/extended_material](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/shader/extended_material.tsx), [shader/extended_material_bindless](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/shader/extended_material_bindless.tsx).

### `ShaderMesh`

A mesh drawn by a material written here in WESL or WGSL: one generic Bevy material
(rdb's ShaderMaterial, an AsBindGroup of 16 uniform slots and 4 textures) whose fragment shader is this source, so
a dissolve, a toon ramp, a hologram or a procedural pattern needs no Rust. Uniforms may change from frame to frame
(they're set again on any frame where they change); the shader isn't compiled again (#28).

Bevy: `bevy_pbr::mesh_material::MeshMaterial3d<raea_sys_material::shader_material::ShaderMaterial>`.

Takes [`ShaderMeshProps`](3d.md#shadermeshprops)'s props.

### `Splat`

A gaussian splat: a reconstructed scene or object, drawn back to front for the frame's camera, behind any mesh in
front of it, through the frame's look. Animate it with reveal, assemble, disperse, explode, wave, twist, morph and
pulse, all values of the frame.

Bevy: `bevy_pbr::mesh_material::MeshMaterial3d<raea_render::engine::splat::SplatMaterial>`, `bevy_pbr::mesh_material::MeshMaterial3d<raea_render::engine::splat::SplatMask>`.

Takes [`SplatProps`](3d.md#splatprops)'s props.

### `StandardMaterial`

Bevy's StandardMaterial for the Mesh3d it's inside. Unlit draws as MeshBasicMaterial; a clearcoat or transmission
as MeshPhysicalMaterial; anything else as MeshStandardMaterial, all on Bevy's StandardMaterial in the engine.

Bevy: `bevy_pbr::pbr_material::StandardMaterial`.

Also takes the props of [`StandardMaterialProps`](#standardmaterialprops).

Used in: [ui/render_ui_to_texture](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/render_ui_to_texture.tsx), [ui/widgets/viewport_node](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/ui/widgets/viewport_node.tsx), [3d/fog](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/fog.tsx), [3d/vertex_colors](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/vertex_colors.tsx), [3d/rotate_environment_map](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/rotate_environment_map.tsx), [3d/transparency_3d](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/transparency_3d.tsx), [3d/spherical_area_lights](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/spherical_area_lights.tsx), [3d/lightmaps](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/lightmaps.tsx), and 42 more.

### `SurfaceShader`

One shader over every lit standard surface in the scene that has none of its own (meshes, Models' materials,
Characters), so the whole scene shares one look. Unlit, toon, paint, flat and looked
surfaces and ShaderMesh keep theirs. In the 3D beside the camera; one per scene.

Bevy: `bevy_pbr::mesh_material::MeshMaterial3d<bevy_pbr::extended_material::ExtendedMaterial<bevy_pbr::pbr_material::StandardMaterial, raea_sys_shader::ShaderExt>>`.

Takes [`SurfaceShaderSpec`](3d.md#surfaceshaderspec)'s props.

### `ThreeShaderMaterial`

A three.js shader on the mesh it's in, as it is: a ShaderMaterial's GLSL (or a node material's WGSL) with its
uniforms, drawn by raea's engine with three's built-ins. `<R3F>` makes these from an R3F scene's materials.

Takes [`ThreeShaderMaterialProps`](3d.md#threeshadermaterialprops)'s props.

### `ToonExtension`

Also takes the props of [`ToonExtensionProps`](#toonextensionprops).

### `ToonMaterial`

raea's ToonMaterial: a StandardMaterial's colour, texture and emission drawn in bands, as
`<ExtendedMaterial base={<StandardMaterial />} extension={<ToonExtension />} />` is (the old MeshToonMaterial).

Bevy: `bevy_pbr::mesh_material::MeshMaterial3d<bevy_pbr::extended_material::ExtendedMaterial<bevy_pbr::pbr_material::StandardMaterial, raea_render::engine::toon::ToonExt>>`, `bevy_pbr::mesh_material::MeshMaterial3d<raea_render::engine::toon::OutlineMaterial>`.

Also takes the props of [`StandardMaterialProps`](#standardmaterialprops) (`material`, `base_color`, `base_color_texture`, `emissive`, `alpha_mode`); [`ToonExtensionProps`](#toonextensionprops).

### `Volume`

A simulated volume: density and heat in a box, put in by its sources and puffs, lifted by heat, weighed down,
carried by the wind and its own curls, drawn lit and shadowed by itself. Smoke, a cloud or a contrail is a choice of
these (examples/volumes).
Simulated and raymarched by engine/render volume.rs:

Bevy: `bevy_pbr::mesh_material::MeshMaterial3d<raea_render::engine::volume::VolumeMaterial>`.

Takes [`VolumeProps`](3d.md#volumeprops)'s props.

### `VolumetricFog`

Volumetric fog on the camera: light in the air, drawn inside every <FogVolume>, with shafts from the lights marked
`volumetric`. Put it inside the Canvas3D, once; without it fog volumes draw nothing.

Bevy: `bevy_light::volumetric::VolumetricFog`, `bevy_light::volumetric::VolumetricLight`.

Takes [`VolumetricFogProps`](3d.md#volumetricfogprops)'s props.

Used in: [3d/fog_volumes](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/fog_volumes.tsx).

### `WearExtension`

raea's chips, grime and rust over a StandardMaterial, as an ExtendedMaterial's extension: a surface shader of
raea's own (worn.ts) on the hook a ShaderExtension uses, so its Bevy type is ShaderExtension's.

| Prop | Type | What it does |
| --- | --- | --- |
| `amount` (required) | `number` | how worn it is, 0-1 |
| `scale` | `number` | a blotch's size in metres (0.08) |
| `chips` | `number` | chips on edges and a few spots, 0-1 of the wear (0.6) |
| `grime` | `number` | grime low and underneath, 0-1 of the wear (0.5) |
| `rust` | `number` | rust run down in streaks, 0-1 of the wear (0.25) |
| `seed` | `number` | moves the pattern |
| `chip_color` | `string` | what chips show beneath the paint |
| `grime_color` | `string` | the grime's colour |
| `rust_color` | `string` | the rust's colour |

## Functions

### `volumeShadow`

a volume's `shadow` as the row's number: 1 by default, 0 for none

```ts
function volumeShadow(owner: string, v: unknown): number
```

## Values

### `FogFalloff`

FogFalloff's constructors: the visibility distance (5% contrast left at it by default) to a density

Used in: [3d/atmospheric_fog](https://github.com/shinyobjectz/raea/blob/main/sdk/raea/bench/bevy-parity/3d/atmospheric_fog.tsx).

## Types

### `AlphaMode`

Bevy's AlphaMode, as it reflects.

```ts
type AlphaMode = "Opaque" | "Blend" | "Premultiplied" | "Add" | "Multiply" | "AlphaToCoverage" | { Mask: number }
```

### `AmbientOcclusionProps`

| Field | Type | What it is |
| --- | --- | --- |
| `quality` | `"low" \| "medium" \| "high" \| "ultra"` | how many samples a pixel takes: "low" (4), "medium" (8, the default), "high" (18), "ultra" (54) |
| `thickness` | `number` | how thick, in metres, a surface is taken to be behind what the camera sees (0.25 by default): larger darkens more behind thin things |

### `DecalProps`

| Field | Type | What it is |
| --- | --- | --- |
| `src` (required) | `string` | the picture pressed onto the surface: staticFile("label.png"); its clear parts show the surface through |
| `name` | `string` | the entity's name, for picking, the inspector and world.* |
| `normalMap` | `string` | a normal map pressed with the picture: the label's relief |
| `size` | `number \| [number, number]` | width by height in metres (a number: a square) |
| `position` | `Vec3` | where it stands, metres (x, y, z) |
| `normal` | `Vec3` | which way the surface faces at the decal (out of it; [0, 1, 0], a floor or counter, by default): turns the decal so the picture lies on it, upright on a wall, its top toward -z on a floor. Instead of rotation |
| `spin` | `number` | a turn about the normal (radians) |
| `rotation` | `Vec3` | radians, the decal's +y out of the surface (rather than normal) |
| `color` | `string` | a tint times the picture (keyed: a stain darkening) |
| `opacity` | `number` | 0 to 1 (keyed: a label fading in) |
| `roughness` | `number` | 0..1, smooth to matte |
| `metalness` | `number` | 0..1, a dielectric to a metal |
| `unlit` | `boolean` | drawn in its flat colour, without light or shade |
| `fade` | `number` | how far off its plane (metres) it still lies on a surface, fading to nothing there (0.05 by default). Anything nearer the plane than this takes it, a cabinet's side or a fridge's as well as the wall: keep it small |

### `ExtendedMaterialProps`

The props of [`ExtendedMaterial`](#extendedmaterial), tabled there.

### `FireProps`

| Field | Type | What it is |
| --- | --- | --- |
| `name` | `string` | parts are matched by name across frames: name a fire that comes and goes |
| `position` | `Vec3` | the flame's foot: where it stands on its fuel |
| `scale` | `number \| Vec3` | its size in metres: one number for its height (it's half as wide), or [width, height, depth] (1 by default) |
| `intensity` | `number` | how bright its core is, as a material's emissiveIntensity (6 by default): 3 a guttering fire, 10-20 a roaring one in the dark. Values over 1 bloom under a <Bloom> |
| `color` | `string` | the flame's body (orange, "#ff7a1f", by default); its edges go deep red and its core yellow-white |
| `seed` | `number` | which flame: fires with different seeds don't flicker together (0 by default) |
| `speed` | `number` | how fast its tongues rise, 1 by default (a big fire's rise more slowly than a small one's on their own) |
| `time` | `number` | seconds on its clock (by default the Sequence's time: frame / fps) |
| `steps` | `number` | samples along each ray through it (48 by default): fewer is cheaper and blurrier |
| `opacity` | `number` | how much of what's behind it its body hides (1 by default): raise it (3-6) in daylight, where a bright wall behind would wash out a flame that only adds light |
| `light` | `number` | the light it casts on what's around it: a flickering PointLight's intensity (1 by default; 0 for none) |

### `FogProps`

| Field | Type | What it is |
| --- | --- | --- |
| `color` | `FogColor` | what far things fade into: match the background |
| `near` | `number` | linear fog from `near` to `far` (in the Canvas3D 5 and 20 by default; on the camera Bevy's 0 and 100) |
| `far` | `number` | where linear fog is full, from the camera (20 in the Canvas3D, 100 on a camera) |
| `density` | `number` | or exponential fog of this density (0.05-0.3), instead of near and far |
| `falloff` | `FogFalloff` | on the camera: Bevy's FogFalloff, in place of near, far and density (Bevy's default Linear 0 to 100) |
| `directional_light_color` | `FogColor` | on the camera: the sun's colour in the fog looking towards it, its alpha its strength (Bevy's default none) |
| `directional_light_exponent` | `number` | on the camera: how tight the sun's glow is (Bevy's default 8) |

### `FogVolumeProps`

| Field | Type | What it is |
| --- | --- | --- |
| `name` | `string` | parts are matched by name across frames: name a volume that comes and goes |
| `position` | `Vec3` | the box's centre |
| `rotation` | `Vec3` | radians |
| `scale` | `number \| Vec3` | the box's size in metres: one number for a cube, or [x, y, z] (1 by default) |
| `density` | `number` | how thick the air is (0.1 by default: dusty; 0.02 a faint haze; 0.5 smoke). Animate it to let dust settle, smoke roll in or a volume vanish (0) |
| `color` | `string` | the fog's own colour, what light scattering out of it takes on (white by default; a dark red for blood in the air) |
| `absorption` | `number` | how much light it swallows (0.3 by default): higher reads as soot or smoke |
| `scattering` | `number` | how much light it throws back toward the eye (0.3 by default): higher reads as bright dust or mist |
| `anisotropy` | `number` | how far forward it scatters, -1 to 1 (0.5 by default): near 1 the shafts glow when you look toward the light |
| `falloff` | `number` | how fast it thins going up, per metre from the box's floor (0 by default: even throughout): 0.5 is a ground fog that's a third as thick two metres up. A box wide and tall with a falloff is height fog. Only <VolumetricFog mode="froxel"> draws it; the march draws the box evenly |
| `soft` | `number` | how far in from each face its density ramps up, as a share of the box's half-size, 0 to 1 (0 by default: a hard box): 0.5 and up hides a small volume's edges (steam, a smoke column). Only <VolumetricFog mode="froxel"> |
| `taper` | `number` | how much narrower its floor is than its top, 0 to 1 (0 by default: a box): 0.7 is a plume rising off a tank, widening as it climbs. Only <VolumetricFog mode="froxel"> |
| `density_texture` | `string` | Bevy's density_texture: a 3D.ktx2 image in the media folder whose values scale the density through the box (a cloud, a bunny of smoke); the box draws evenly without one. Only the march draws it |

### `GridExtensionProps`

raea's extension of StandardMaterial (a Bevy MaterialExtension of raea's): graph paper over the surface for
greybox levels, lines every `cell` world units (fixed in the world, so they run on across shapes), a heavier one
every `major` cells (4), in `color`, and each face of a Box outlined in `edge` ("" for no outlines). Its colours are
the first frame's. It goes in an ExtendedMaterial's `extension`.

The props of [`GridExtension`](#gridextension), tabled there.

### `LightmapProps`

The props of [`Lightmap`](#lightmap), tabled there.

### `MetallicRoughnessMaps`

Separate greyscale maps packed into Bevy's metallic_roughness_texture when they load: the roughness map's grey as
its green, the metalness map's as its blue (white without one, so metallic is its factor exactly). Poly Haven's
"_rough" sets come this way; a packed "_arm" map is a plain path.

| Field | Type | What it is |
| --- | --- | --- |
| `roughness` | `string` | a greyscale roughness map (white rough) |
| `metalness` | `string` | a greyscale metalness map (white metal) |

### `PaintExtensionProps`

raea's extension of StandardMaterial for a paint look (<Look mode="paint">): the surface broken up by world-space
noise of `scale` metres (1), its tone boundaries wandering by `breakup` (0.5) and patches of it taking `color`
(`amount`, 0.6, of the way): a mottled field, rust, a wall's uneven paint. It goes in an ExtendedMaterial's
`extension`.

The props of [`PaintExtension`](#paintextension), tabled there.

### `ProceduralExtensionProps`

raea's extension of StandardMaterial that draws a pattern on the GPU in place of its flat colour (the engine's
Procedural MaterialExtension, systems/procmat): `pattern` (noise, wood, marble, stone, brick, tiles, fabric,
brushed, plaster) in `colors` with roughness `rough_a`/`rough_b`, features `scale` metres across, a `bump`; brick
and tiles in `tile` [w, h] metre cells with `gap` metre joints in `gap_color`. Over it, each 0-1: `dust` on what
faces up, `dirt` in joints, hollows, undersides and the floor line, `edges` worn lighter. The pattern overrides the
base's colour and roughness; its lighting stays. It goes in an ExtendedMaterial's `extension`, alone.

The props of [`ProceduralExtension`](#proceduralextension), tabled there.

### `SampledImage`

A picture read with its own sampler: Bevy's load_with_settings, whose ImageLoaderSettings give the Image its
ImageSamplerDescriptor ({ path: "checker.png", sampler: { filter: "nearest" } } for hard pixels).

| Field | Type | What it is |
| --- | --- | --- |
| `path` (required) | `string` | the picture, in the media folder |
| `sampler` (required) | `SamplerSpec` | how it's sampled: filter, address modes |

### `ShaderConstantsProps`

The props of [`ShaderConstants`](#shaderconstants), tabled there.

### `ShaderConstantValue`

A shader constant's value: a number (a float or a whole number, as the const is), a boolean, or for an array or a
vector its values in order (as many as it holds)

```ts
type ShaderConstantValue = number | boolean | readonly number[]
```

### `ShaderExtensionProps`

raea's extension of StandardMaterial written in WESL: its
`fn surface` runs before Bevy's lighting and its `fn lit` after it, on the base's inputs, so the surface stays lit
and shadowed. raea/3d's <MeshStandardMaterial shader> is the same thing; the spec is SurfaceShaderSpec. It goes in an
ExtendedMaterial's `extension`, alone.

Bevy: `bevy_pbr::extended_material::ExtendedMaterial<bevy_pbr::pbr_material::StandardMaterial, raea_sys_shader::ShaderExt>`.

The props of [`ShaderExtension`](#shaderextension), tabled there.

### `ShaderMeshProps`

| Field | Type | What it is |
| --- | --- | --- |
| `shape` (required) | `string \| ({ primitive: string } & Record<string, unknown>)` | its geometry: a Manifold shape ("box(1, 1, 1)") or one of Bevy's primitives ({ primitive: "Sphere", radius: 1 }) |
| `wesl` | `string` | WESL (Bevy 0.20's standard; give this or `wgsl`): a `fn shade(in: VertexOutput) -> vec4<f32>` (in.uv, in.world_position, in.world_normal), or a module with its own |
| `wgsl` | `string` | the same in plain WGSL (compiled as WESL, its superset: Bevy 0.20 has no naga_oil, so no `#import`) |
| `uniforms` | `Record<string, number \| number[] \| string>` | up to 16: a number (f32), [x, y], [x, y, z], [x, y, z, w] or "#rrggbb" (a linear vec4) |
| `textures` | `Record<string, string \| Record<string, unknown>>` | up to 4: an asset path, a picture made in code ({ width, height, pixels }) or "render:NAME"; and one array, { layers: [picture,...] } or { src, layers: N }, read as texture_2d_array<f32> tex_NAME by layer |
| `defs` | `ShaderDefs` | shader defs: each NAME a WESL condition for `@if(NAME)`, and a whole number also `constants::NAME` |
| `alpha` | `"opaque" \| "blend" \| "add" \| "premultiplied"` | how it blends: opaque, blend, add or premultiplied |
| `doubleSided` | `boolean` | draw the back faces too |
| `name` | `string` | the entity's name, for picking, the inspector and world.* |
| `position` | `Vec3` | where it stands, metres (x, y, z) |
| `rotation` | `Vec3` | its rotation, Euler angles in radians (x, y, z) |
| `scale` | `Vec3 \| number` | its scale: one number, or x, y and z |

### `SplatMorph`

| Field | Type | What it is |
| --- | --- | --- |
| `to` (required) | `string` | the splat file it becomes |
| `amount` (required) | `number` | 0 this splat, 1 the other (each splat flies to the one matched to it) |
| `crop` | `number` | only the other's splats this many metres from its centre (a trained scene's subject without its surroundings) |

### `SplatProps`

| Field | Type | What it is |
| --- | --- | --- |
| `name` | `string` | parts are matched by name across frames: name a splat that comes and goes |
| `src` (required) | `string` | the splat file (staticFile("x.ply")): a 3DGS.ply (spherical harmonics up to degree 3), a.splat or a.spz. A `x.ply.json` beside it (raea splat train writes one) places it: +Y up, centred, metres |
| `position` | `Vec3` | where it stands, metres (x, y, z) |
| `rotation` | `Vec3` | radians, Euler XYZ |
| `scale` | `number \| Vec3` | its scale: one number, or x, y and z |
| `splatScale` | `number` | every gaussian's size times this (1 by default): below 1 the scene breaks into points |
| `opacity` | `number` | 0..1, multiplying every gaussian's |
| `brightness` | `number` | grade: colour times brightness, saturation and contrast about mid grey, times tint |
| `saturation` | `number` | 1 as trained; 0 grey, above 1 richer |
| `contrast` | `number` | 1 as trained; about mid grey |
| `tint` | `string` | a colour every gaussian's is multiplied by |
| `reveal` | `number` | reveal 0..1: each splat scales in as a front passes it, from `revealFrom` (a point; the centre by default) outward, or along `revealAxis` (a plane sweeping through) when that's given |
| `revealFrom` | `Vec3` | the point the reveal grows outward from (the centre by default) |
| `revealAxis` | `Vec3` | a direction: the reveal sweeps a plane along it instead of growing from a point |
| `revealSoft` | `number` | how wide the front is, as a share of the sweep (0.15) |
| `revealGlow` | `number` | the front's glow (0 none, 1 bright), in `glowColor` |
| `glowColor` | `string` | the colour of reveal and pulse glows (warm white) |
| `disperse` | `number` | disperse 0..1: blown off on a curl-noise wind, the upwind side first, shrinking to specks |
| `assemble` | `number` | assemble 0..1: from a cloud of dust round the scene into its shape (1 whole) |
| `wind` | `Vec3` | the wind's direction ([1, 0.15, 0]) |
| `curl` | `number` | how much the wind curls (1) |
| `spread` | `number` | the dust cloud's radius in metres (the splat's own size, before scale; 1.6 times its size by default) |
| `explode` | `number` | explode 0..1: thrown out from the centre, falling and fading |
| `wave` | `SplatWave` | a ripple running out through the splats: amplitude, wavelength and phase |
| `twist` | `number` | radians a metre about the vertical through the centre |
| `morph` | `SplatMorph` | becomes another splat file: `to` and `amount` 0..1 |
| `pulse` | `number` | a value the composition passes (a beat's envelope 0..1): bigger by pulseScale, brighter by pulseGlow |
| `pulseScale` | `number` | how much bigger a full pulse makes it |
| `pulseGlow` | `number` | how much brighter a full pulse makes it |
| `crop` | `number` | nothing past this many metres from the centre (trims a trained scene's floaters) |
| `bands` | `number` | the light in this many flat bands (0 none); a paint or cartoon <Look> sets it |
| `paint` | `number` | 0..1: the splats themselves painted (flat bands, grown into strokes with hard edges), for a painted shot in a timeline whose frame <Look> isn't paint |
| `seed` | `number` | which randomness: the scatter, the jitter (0) |

### `SplatWave`

| Field | Type | What it is |
| --- | --- | --- |
| `amplitude` (required) | `number` | how high the ripple lifts the splats, metres |
| `wavelength` | `number` | from crest to crest, metres (1 by default) |
| `phase` | `number` | where the ripple is in its cycle, radians: animate it (frame * 0.2) to run it outward |

### `StandardMaterialAnimate`

The StandardMaterial fields `animate` takes, each a track of its own kind of value. An emissive brighter than white
is LinearRgba keys of one colour: track({ 0: E.mul(0), 30: E.mul(2.5) }) with E = LinearRgba.from(Srgba.hex(c)).

| Field | Type | What it is |
| --- | --- | --- |
| `base_color` | `TrackSpec<string>` | a track of `base_color`, sampled each frame |
| `emissive` | `TrackSpec<string> \| TrackSpec<LinearRgba>` | a track of `emissive`, sampled each frame |
| `perceptual_roughness` | `TrackSpec<number>` | a track of `perceptual_roughness`, sampled each frame |
| `metallic` | `TrackSpec<number>` | a track of `metallic`, sampled each frame |
| `emissive_exposure_weight` | `TrackSpec<number>` | a track of `emissive_exposure_weight`, sampled each frame |
| `reflectance` | `TrackSpec<number>` | a track of `reflectance`, sampled each frame |
| `specular_tint` | `TrackSpec<string>` | a track of `specular_tint`, sampled each frame |
| `diffuse_transmission` | `TrackSpec<number>` | a track of `diffuse_transmission`, sampled each frame |
| `specular_transmission` | `TrackSpec<number>` | a track of `specular_transmission`, sampled each frame |
| `thickness` | `TrackSpec<number>` | a track of `thickness`, sampled each frame |
| `ior` | `TrackSpec<number>` | a track of `ior`, sampled each frame |
| `attenuation_distance` | `TrackSpec<number>` | a track of `attenuation_distance`, sampled each frame |
| `attenuation_color` | `TrackSpec<string>` | a track of `attenuation_color`, sampled each frame |
| `clearcoat` | `TrackSpec<number>` | a track of `clearcoat`, sampled each frame |
| `clearcoat_perceptual_roughness` | `TrackSpec<number>` | a track of `clearcoat_perceptual_roughness`, sampled each frame |
| `anisotropy_strength` | `TrackSpec<number>` | a track of `anisotropy_strength`, sampled each frame |
| `anisotropy_rotation` | `TrackSpec<number>` | a track of `anisotropy_rotation`, sampled each frame |
| `depth_bias` | `TrackSpec<number>` | a track of `depth_bias`, sampled each frame |
| `parallax_depth_scale` | `TrackSpec<number>` | a track of `parallax_depth_scale`, sampled each frame |
| `max_parallax_layer_count` | `TrackSpec<number>` | a track of `max_parallax_layer_count`, sampled each frame |
| `lightmap_exposure` | `TrackSpec<number>` | a track of `lightmap_exposure`, sampled each frame |

### `StandardMaterialProps`

Bevy's StandardMaterial fields (colours as CSS colours, textures as file paths), with Bevy's defaults.

| Field | Type | What it is |
| --- | --- | --- |
| `base_color` | `string \| Srgba` | a colour, or Bevy's Srgba (its alpha shows with alpha_mode "Blend": Srgba.hex("#88ccff").with_alpha(0.5)) |
| `base_color_texture` | `Tex \| null` | a picture in the media folder; null (Bevy's None) inside a Model drops the model's own base map |
| `emissive` | `string \| LinearRgba` | a colour, or Bevy's LinearRgba for one brighter than white (LinearRgba.from(Srgba.hex("#ff8800")).mul(3)) |
| `emissive_texture` | `Tex` | Bevy: The emissive map, multiplies pixels with `emissive` to get the final "emitting" color of a surface. |
| `perceptual_roughness` | `number` | 0.5 by Bevy's default |
| `metallic` | `number` | Bevy: How "metallic" the material appears, within `[0.0, 1.0]`. |
| `metallic_roughness_texture` | `Tex \| MetallicRoughnessMaps` | Bevy: Metallic and roughness maps, stored as a single texture. |
| `normal_map_texture` | `Tex` | Bevy: Used to fake the lighting of bumps and dents on a material. |
| `occlusion_texture` | `Tex` | Bevy: Specifies the level of exposure to ambient light. |
| `uv_transform` | `Affine2` | every texture's UVs scaled, turned and moved (Affine2.fromScale([5, 5]) tiles a map five times across each face; a scaled texture repeats) |
| `reflectance` | `number` | 0.5 by Bevy's default |
| `clearcoat` | `number` | Bevy: An extra thin translucent layer on top of the main PBR layer. This is typically used for painted surfaces. |
| `clearcoat_perceptual_roughness` | `number` | Bevy: The roughness of the clearcoat material. This is specified in exactly the same way as the `StandardMaterial::perceptual_roughness`. |
| `clearcoat_normal_texture` | `Tex` | the clearcoat layer's own normal map, read as data (not sRGB), as Bevy's examples load it with is_srgb false |
| `specular_transmission` | `number` | Bevy: The amount of light transmitted _specularly_ through the material (i.e. via refraction). |
| `ior` | `number` | Bevy: The index of refraction of the material. |
| `thickness` | `number` | Bevy: Thickness of the volume beneath the material surface. |
| `unlit` | `boolean` | Bevy: Whether to apply only the base color to this material. |
| `alpha_mode` | `AlphaMode` | Bevy: How to apply the alpha channel of the `base_color_texture`. |
| `double_sided` | `boolean` | Bevy: Support two-sided lighting by automatically flipping the normals for "back" faces within the PBR lighting shader. |
| `cull_mode` | `"Back" \| "Front" \| null` | the faces not drawn: "Back" (Bevy's default), "Front", or null for neither |
| `material` | `string` | in a <Model>: the glTF material it re-covers, as MeshStandardMaterial's |
| `fog_enabled` | `boolean` | false: left out of the scene's <Fog> |
| `specular_tint` | `string` | the highlight's colour, over `reflectance` |
| `anisotropy_strength` | `number` | stretched highlights (brushed metal, hair), 0..1, turned anisotropy_rotation radians along the tangent |
| `anisotropy_rotation` | `number` | Bevy: The direction of increased roughness, in radians relative to the mesh tangent. |
| `attenuation_distance` | `number` | light absorbed through a transmissive volume (with specular_transmission and thickness) |
| `attenuation_color` | `string` | Bevy: The resulting (non-absorbed) color after white light travels through the attenuation distance. |
| `depth_map` | `Tex` | a greyscale height map (white deep) the parallax fields carve a flat face with |
| `parallax_depth_scale` | `number` | 0.1 by Bevy's default |
| `parallax_mapping_method` | `"Occlusion" \| { Relief: { max_steps: number } }` | "Occlusion" (Bevy's default) or { Relief: { max_steps } } |
| `max_parallax_layer_count` | `number` | 16 by Bevy's default |
| `depth_bias` | `number` | drawn this much nearer in depth (a decal-like layer over a coplanar face) |
| `opaque_render_method` | `"Forward" \| "Deferred"` | "Forward" or "Deferred" (in the deferred pass, with a camera that has one) |
| `diffuse_transmission` | `number` | light passed through and scattered (leaves, paper, wax), 0..1; lightmap_exposure: a <Lightmap>'s bake times this |
| `lightmap_exposure` | `number` | Bevy: The exposure (brightness) level of the lightmap, if present. |
| `animate` | `StandardMaterialAnimate` | its fields' tracks, sampled by the engine from the frame clock: { base_color: track({ 0: "#3060ff", 30: "#ff4070" }) } |

### `SurfaceDrawProps`

How a surface's alpha and faces draw (Bevy's StandardMaterial alpha_mode, alpha cutoff, double_sided and cull_mode),
from the first frame.

| Field | Type | What it is |
| --- | --- | --- |
| `alphaMode` | `"opaque" \| "blend" \| "mask" \| "add" \| "multiply" \| "premultiplied" \| "coverage"` | "opaque" (the default), "blend" (what `transparent` gives), "mask" (cut out where the alpha is under `alphaCutoff`: leaves, chain-link, a sprite's edge; no sorting), "add" (glows add up: sparks, light beams), "multiply" (darkens what's behind: stained glass, a shadow decal), "premultiplied", "coverage" (soft cut-out edges from the alpha over MSAA's samples: hair cards, foliage; needs <AntiAliasing mode="msaa">, without it draws solid) |
| `alphaCutoff` | `number` | with alphaMode="mask": the alpha under which it's cut away, 0..1 (0.5) |
| `side` | `"front" \| "back" \| "double"` | which faces draw: "front" (the default), "back" (the inside only: a sky dome, a room seen from outside), "double" (both, lit on both: a leaf, a flag, a sheet of paper) |

### `SurfaceShaderSpec`

| Field | Type | What it is |
| --- | --- | --- |
| `wesl` | `string` | the source in WESL (Bevy 0.20's shader language, a superset of WGSL) |
| `wgsl` | `string` | the same in plain WGSL |
| `fragment` | `string` | the source as an asset path (staticFile("shaders/x.wesl")), as Bevy's MaterialExtension::fragment_shader names one: read as written but for its own bindings' numbers (any, in the order declared: one uniform struct, which gets its uniforms packed in its layout, up to 4 texture_2d and their one sampler; one under |
| `uniforms` | `Record<string, number \| number[] \| string>` | up to 16: a number (f32), [x, y], [x, y, z], [x, y, z, w] or "#rrggbb" (a linear vec4) |
| `textures` | `Record<string, ShaderTexture \| ShaderTexture[]>` | up to 4: an asset path, a picture made in code ({ width, height, pixels }) or "render:NAME"; with a sampler, { src, sampler } (or a picture with its `sampler`). One more may be an array, read as texture_2d_array<f32> by layer: { layers: [picture,...] } (one size and format) or { src, layers: N } (N layers stacked top to bottom). A <Material>'s may also bind one binding_array<texture_2d<f32>>, a list of 1 to 16 pictures (Bevy's texture_binding_array; a GPU without texture binding arrays refuses it) |
| `defs` | `ShaderDefs` | shader defs: each NAME a WESL condition for `@if(NAME)` (a number is true), and a whole number also `constants::NAME`; each set of defs compiles its own pipeline |
| `own` | `OwnMaterialFields` | a <Material>'s own: what its Material methods give, and its storage buffers; set by <Material> only |
| `buffers` | `Record<string, StorageValue>` | the material's storage buffers by binding name; set by <Material> only |

### `ThreeShaderMaterialProps`

| Field | Type | What it is |
| --- | --- | --- |
| `glsl` | `{ vertex: string; fragment: string; raw?: boolean; vertexColors?: boolean }` | a ShaderMaterial's GLSL, its `#include`s expanded and its `defines` as #define lines in front; `raw` for a RawShaderMaterial (which declares three's built-ins itself), `vertexColors` as the material's |
| `wgsl` | `{ vertex: string; fragment: string; uniforms?: { name: string; type: string; array?: nu…` | a node material's WGSL, as three's TSL builder writes it for raea (sdk/r3f's input/tsl.ts): the contract's `three_vertex` and `three_fragment`, its uniforms' types, the mesh attributes it reads beyond the standard four, whether it is a lit surface (three_surface), and the storage buffers it reads (`tsl_s0`, `tsl_s1`: each a buffer a <Compute>'s kernels share by key, its size in bytes and first contents) |
| `uniforms` | `Record<string, ThreeUniform>` | each uniform's value this frame |
| `textures` | `Record<string, string \| { src: string; colorSpace?: "srgb" \| "linear"; sampler?: Record…` | each sampler's picture: a file, a data URL, "render:NAME" (at most four); `colorSpace: "linear"` for one three samples raw (NoColorSpace: a distance field, a data map), "srgb" (the default) for a colour picture |
| `output` | `"display" \| "linear"` | "display": its colour is what three's canvas shows (a ShaderMaterial's); "linear": scene light, tone-mapped as the rest of the frame is (a node material's) |
| `transparent` | `boolean` | blended over what's behind it, as three's \`transparent\` |
| `blending` | `"normal" \| "additive" \| "add" \| "multiply" \| "none"` | how it blends: normal, additive (add), multiply or none |
| `side` | `"front" \| "back" \| "double"` | which faces draw: front, back or double |
| `depthWrite` | `boolean` | it writes the depth buffer |
| `depthTest` | `boolean` | it's hidden behind what's nearer |
| `alphaTest` | `number` | pixels less opaque than this are dropped |
| `time` | `[number, number, number]` | three's clock this frame: [seconds, delta, frame] |
| `color` | `string` | the colour it's drawn in where its shader can't run (named in the render's warnings) |

### `ToonBand`

| Field | Type | What it is |
| --- | --- | --- |
| `at` (required) | `number` | where the band starts, as r: the share of the light reaching a point that it faces (0 faces none of it, 1 faces every light); the first band starts at 0 whatever this says |
| `color` | `string` | the band's colour; left out, it's mixed (in Oklab) between the shadow colour and `color` by its place |

### `ToonExtensionAnimate`

| Field | Type | What it is |
| --- | --- | --- |
| `shadow_color` | `TrackSpec<string>` | a track of `shadow_color`, sampled each frame |
| `rim_intensity` | `TrackSpec<number>` | a track of `rim_intensity`, sampled each frame |
| `highlight_intensity` | `TrackSpec<number>` | a track of `highlight_intensity`, sampled each frame |
| `outline_width` | `TrackSpec<number>` | a track of `outline_width`, sampled each frame |

### `ToonExtensionProps`

raea's extension of StandardMaterial that cel-shades it (the engine's ToonExt; three.js's MeshToonMaterial): the
scene's lights in `steps` hard bands (2; `bands` for an explicit ramp, darkest first), the bottom one
`shadow_color` (the base colour darker, nudged towards violet), with an optional rim, highlight and ink outline
(true for their defaults). Its base gives base_color, base_color_texture, emissive, alpha_mode and material; a
toon surface has no roughness or metal. It goes in an ExtendedMaterial's `extension`, alone.

| Field | Type | What it is |
| --- | --- | --- |
| `steps` | `number` | how many hard bands the light falls into (2) |
| `bands` | `ToonBand[]` | an explicit ramp of bands, darkest first, in place of `steps` |
| `shadow_color` | `string` | the bottom band's colour (the base colour darker, nudged towards violet) |
| `rim` | `boolean \| ToonRim` | a rim light on the silhouette: true for its default, or its settings |
| `highlight` | `boolean \| ToonHighlight` | a hard specular highlight: true for its default, or its settings |
| `outline` | `boolean \| ToonOutline` | an ink outline: true for its default, or its settings |
| `rim_intensity` | `number` | how bright the rim is |
| `highlight_intensity` | `number` | how bright the highlight is |
| `outline_width` | `number` | the outline's width |
| `animate` | `ToonExtensionAnimate` | its fields' tracks, as a StandardMaterial's `animate` (its base animates base_color and emissive) |

### `ToonHighlight`

| Field | Type | What it is |
| --- | --- | --- |
| `intensity` | `number` | 0..1 (1 by default) |
| `color` | `string` | "#ffffff" by default |
| `size` | `number` | 0..1: the highlight's angular radius, up to half a radian (0.2 by default) |

### `ToonMaterialProps`

The props of [`ToonMaterial`](#toonmaterial), tabled there.

### `ToonOutline`

| Field | Type | What it is |
| --- | --- | --- |
| `width` | `number` | pixels at the composition's size, the same near and far (2 by default) |
| `color` | `string` | ink colour; by default the surface colour's hue, very dark |

### `ToonRim`

| Field | Type | What it is |
| --- | --- | --- |
| `intensity` | `number` | 0..1, how much the rim colour covers the band (1 by default) |
| `color` | `string` | "#ffffff" by default |
| `width` | `number` | 0..1 of the Fresnel range from the silhouette inwards (0.3 by default) |

### `VolumeCommon`

| Field | Type | What it is |
| --- | --- | --- |
| `name` | `string` | parts are matched by name across frames |
| `time` | `number` | seconds on the simulation's clock (the Sequence's time, frame / fps, by default) |
| `resolution` | `number` | cells along the box's longest side (64 by default; 32 is a quarter the cost a step, 96 finer) |
| `seed` | `number` | the same seed simulates the same volume (0) |
| `colors` | `[string, string, string]` | its tones under a paint look: [lit, mid, dark] |
| `threshold` | `number` | the density its painted surface sits at (0.12): lower is fatter, higher thinner |
| `erode` | `number` | how much noise tears the painted surface's edge (0.08), and the tears' size in metres (0.6) |
| `erodeSize` | `number` | the tears' size in metres (0.6) |
| `absorb` | `number` | how much light a unit of it stops (its shadow on itself; 1.2) |
| `wind` | `Vec3` | the wind, m/s ([0, 0, 0]); how fast the volume takes it up, a second (1.2 smoke, 0.6 otherwise); the height it's full from, metres, weaker below (0: full everywhere) |
| `windPull` | `number` | how fast the volume takes up the wind, a second (1.2 smoke, 0.6 otherwise) |
| `windHeight` | `number` | the height the wind is full from, metres; weaker below (0: full everywhere) |
| `drag` | `number` | velocity the still air takes away a second (0.4) |
| `vorticity` | `number` | curl kept curling (6); a drifting turbulent force, m/s² (0), and its size in metres (2) |
| `turbulence` | `number` | a drifting turbulent force, m/s² (0) |
| `turbulenceSize` | `number` | the turbulence's size in metres (2) |
| `steps` | `number` | samples along each ray (96) |
| `shadow` | `boolean \| number` | its shadow, cast in the scene's light as a mesh's is (on by default): soft moving shadows on the ground, meshes and models under it, and, with a `volumetric` sun and a <FogVolume> under <VolumetricFog>, the sun's shafts breaking through the gaps between clouds. The shadow is where the volume has stopped half the sun; a number scales the density it sees: 1 as drawn, more reaches into its thinner parts (a fatter shadow; up to 4, a thin contrail's made to show), less keeps to the dense cores. false or 0 casts none. Under a paint look the shadow is the painted shape. Soften its edge with the sun's `softShadow` (2-4 for clouds) |

### `VolumeProps`

| Field | Type | What it is |
| --- | --- | --- |
| `position` | `Vec3` | the box's foot (the centre of its floor), metres ([0, 0, 0]) |
| `size` (required) | `Vec3` | the box it lives in, [width, height, depth] in metres: what leaves it is cut |
| `sources` | `VolumeSource[]` | where density and heat come in over time, and the puffs of density it starts with ([x, y, z, radius] in metres from the foot) |
| `puffs` | `[number, number, number, number][]` | density it starts with: [x, y, z, radius] in metres from the foot |
| `buoyancy` | `number` | heat's lift (4) and the density's weight (0.4), m/s² a unit; the heat lost a second (0.8) and the density lost a second (0.02), fractions |
| `weight` | `number` | the density's weight, m/s² a unit (0.4) |
| `cooling` | `number` | the heat lost a second, a fraction (0.8) |
| `dissipation` | `number` | the density lost a second, a fraction (0.02) |
| `fire` | `[string, string, string] \| false` | fire where it's hot: three colours [red, orange, yellow] (paint's by default), or false for none; and the heat (a fraction of the hottest source's) each colour starts at ([0.35, 0.6, 0.85]) |
| `fireAt` | `[number, number, number]` | the heat (a fraction of the hottest source's) each fire colour starts at ([0.35, 0.6, 0.85]) |
| `name` | `string` | parts are matched by name across frames |
| `time` | `number` | seconds on the simulation's clock (the Sequence's time, frame / fps, by default) |
| `resolution` | `number` | cells along the box's longest side (64 by default; 32 is a quarter the cost a step, 96 finer) |
| `seed` | `number` | the same seed simulates the same volume (0) |
| `colors` | `[string, string, string]` | its tones under a paint look: [lit, mid, dark] |
| `threshold` | `number` | the density its painted surface sits at (0.12): lower is fatter, higher thinner |
| `erode` | `number` | how much noise tears the painted surface's edge (0.08), and the tears' size in metres (0.6) |
| `erodeSize` | `number` | the tears' size in metres (0.6) |
| `absorb` | `number` | how much light a unit of it stops (its shadow on itself; 1.2) |
| `wind` | `Vec3` | the wind, m/s ([0, 0, 0]); how fast the volume takes it up, a second (1.2 smoke, 0.6 otherwise); the height it's full from, metres, weaker below (0: full everywhere) |
| `windPull` | `number` | how fast the volume takes up the wind, a second (1.2 smoke, 0.6 otherwise) |
| `windHeight` | `number` | the height the wind is full from, metres; weaker below (0: full everywhere) |
| `drag` | `number` | velocity the still air takes away a second (0.4) |
| `vorticity` | `number` | curl kept curling (6); a drifting turbulent force, m/s² (0), and its size in metres (2) |
| `turbulence` | `number` | a drifting turbulent force, m/s² (0) |
| `turbulenceSize` | `number` | the turbulence's size in metres (2) |
| `steps` | `number` | samples along each ray (96) |
| `shadow` | `boolean \| number` | its shadow, cast in the scene's light as a mesh's is (on by default): soft moving shadows on the ground, meshes and models under it, and, with a `volumetric` sun and a <FogVolume> under <VolumetricFog>, the sun's shafts breaking through the gaps between clouds. The shadow is where the volume has stopped half the sun; a number scales the density it sees: 1 as drawn, more reaches into its thinner parts (a fatter shadow; up to 4, a thin contrail's made to show), less keeps to the dense cores. false or 0 casts none. Under a paint look the shadow is the painted shape. Soften its edge with the sun's `softShadow` (2-4 for clouds) |

### `VolumeSource`

where density and heat come in: a burner, or an engine moving `at` -> `to` (a contrail)

| Field | Type | What it is |
| --- | --- | --- |
| `at` (required) | `Vec3` | where (metres from the box's foot), and where it has moved to by `until` if it moves |
| `to` | `Vec3` | where a moving source has got to by `until` |
| `from` | `number` | when it puts out, seconds ([0, Infinity]) |
| `until` | `number` | when it stops putting out, seconds (Infinity) |
| `radius` | `number` | its radius (0.5 m); the density and heat it puts out a second at its centre (4 and 4); the velocity it pushes out with, m/s ([0, 0, 0]) |
| `rate` | `number` | the density it puts out a second at its centre (4) |
| `heat` | `number` | the heat it puts out a second at its centre (4) |
| `speed` | `Vec3` | the velocity it pushes out with, m/s ([0, 0, 0]) |

### `VolumetricFogProps`

| Field | Type | What it is |
| --- | --- | --- |
| `steps` | `number` | mode="march": samples along each ray through a fog volume (64 by default): too few stair-step a shaft's edges, so 128 for a render with hard-edged shafts, or 32 with `dither`; about 0.1 ms a step at 1920x1080 on an M5 Pro |
| `dither` | `boolean` | mode="march": start each pixel's march a fraction of a step in, by a fixed noise seeded from the pixel and the frame number (false by default): the shafts' steps become fine grain instead of bands, so fewer steps look smooth. The noise is a function of the frame, so a render still equals its play |
| `resolution` | `number` | mode="march": the fraction of the frame's resolution the fog is marched at (1 by default): 0.5 marches a quarter of the pixels and puts the result over the frame guided by depth, so an edge in front of a shaft keeps no fog from what's behind it. 0.5, 1/3 or 0.25; each frame on its own, so a render still equals its play |
| `ambient` | `number` | light the fog has everywhere, lit or not (0 by default). Bevy lights a volume's whole box with it whatever its density, so the box's edges show: keep it 0, or small with the camera inside the volume |
| `ambientColor` | `string` | that light's colour (white by default) |
| `mode` | `"march" \| "froxel"` | how the fog is drawn: "froxel" (the default: every volume written into one grid of cells along the view, lit once a cell and integrated front to back once, with the `hero` lights' shafts integrated exactly along epipolar lines: no steps, no dither, its cost the grid's whatever the resolution or the number of volumes, overlapping volumes mixed as one medium, see-through surfaces fogged at their own depth) or "march" (Bevy's ray march through each volume, as before). `steps`, `dither` and `resolution` are the march's; `slices`, `tile`, `samples` and `hero` the grid's |
| `slices` | `number` | the grid's cells along the view, spaced evenly in log depth from the camera to the farthest volume (64 by default; 32 to 256) |
| `tile` | `number` | the grid's cells across the frame, in pixels a side (16 by default; 4, 8 or 16) |
| `samples` | `number` | points lit in each cell, spread along its depth (1 by default; 1 to 8): more keeps thin shadows in the grid's soft light (a hero light's shafts don't need them) |
| `hero` | `boolean` | the grid's shafts: true (the default) integrates the first two volumetric spot lights and the first two volumetric directional lights exactly, over their shadow maps texel by texel, so a blind's bars or a doorway's edge are as crisp as the shadow map: integrated at samples along lines from the light across the frame (Chen et al.'s epipolar sampling), more of them where the depth or the light changes, and each pixel read from the samples either side of it at its depth; false lights them in the grid's cells with the rest |
| `shadow` | `"absorb" \| "delta" \| "full" \| "off" \| "bevy"` | what dims a light on its way through a volume to a point of it: "absorb" (the default: what the medium absorbs, which matched the path tracer best), "delta" (absorption and the share of scattering that turns light aside), "full" (the whole extinction), "off" (not dimmed) or "bevy" (Bevy's march's rule: each volume's light dimmed once by its own radius) |
| `heroExact` | `boolean` | the hero lights integrated at every pixel instead of at epipolar samples (false by default): several times slower, the reference the samples are measured against |
| `epipolar` | `{ lineGap?: number; spacing?: number; every?: number; relTolerance?: number; absToleran…` | the hero lights' epipolar samples: lines `lineGap` pixels apart round each light's rectangle on screen (8), samples `spacing` pixels apart along each (6), every `every`th traced (16) and those between traced too where they differ by more than `relTolerance` (0.2), `absTolerance` (2/255) or `depthTolerance` (0.02, relative) |
| `tricubic` | `boolean` | the hero passes read the grid's transmittance tricubic, as the apply pass reads its light (false by default: trilinear) |
| `solari` | `boolean \| { candidates?: number; frames?: number; skyRays?: number; lamps?: boolean; sk…` | under <SolariLighting>, the fog's light from Solari's scene: false lights it as without Solari; or its settings: light candidates a cell (8), frames its history averages (16), sky rays a cell (1), what lights it (`lamps`, the emissive meshes, and `sky`, both on), the history's filter (on), and what the sky's guide is made of (`guide` 3: 1 the cell's own last, 2 half a neighbour's, 3 both, 0 none) |

### `WearExtensionProps`

raea's extension of StandardMaterial that wears it (a surface shader of raea's own, worn.ts): its paint
mottled, chipped back to `chip_color` on edges and a few spots, `grime` gathered low and underneath and `rust` run
down in streaks, fixed in the part's own space as it moves. `amount` 0-1; `chips`, `grime` and `rust` each 0-1 of it
(0.6, 0.5, 0.25); `scale` a blotch's size in metres (0.08); `seed` moves the pattern. It goes in an
ExtendedMaterial's `extension`.

The props of [`WearExtension`](#wearextension), tabled there.

<!-- generated:end -->
