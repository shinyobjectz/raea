# Components: build anything as parts that fit together

Everything raea makes is React components composed in one file: a 2D card, a 3D object, a material, a shader, a
look, a game system, a screen showing an app. This guide says what a good component is, how components fit together
without stepping on each other, where each kind gets made (TSX, solids, a generator, Blender, Godot, Rust), and how
components are shared through registries that are compatible with shadcn's. The design and its build plan are in
docs/components.md in the raea repository.

## Component first: the order of work

1. **List the components before writing any.** From the brief (and its story, and each hero's `NAME.parts.ts`), name
   every thing the piece needs as a component: `Kitchen`, `Kettle`, `Steam`, `BrushedSteel`, `MorningLook`,
   `PriceCard`. One component is one concern: one object, one surface, one effect, one card.
2. **Look before making.** `raea component list` shows raea's own; a project's `components/` and the workspace's
   `shared/components/` hold what this person already has; `raea library search "WORDS"` finds their assets. Interface
   pieces (forms, tables, charts, dashboards) come from shadcn, never rebuilt: see "shadcn" below.
3. **Add, then adapt.** `raea component add NAME` copies a component into `components/` (`--shared` into the
   workspace's `shared/components/`, imported as `@shared/components/NAME`). It's the project's own once copied: change
   it freely. Nothing updates it behind your back.
4. **Write what's missing as a component, with its fixture.** A component isn't done until it renders alone, in a
   small fixture composition, and passes its checks there (see "Done means checked alone").
5. **Compose.** The composition file places components and owns what belongs to the whole piece (below). Then render
   and check the piece as usual.

A piece written as one long file of meshes and numbers can't be reused, checked part by part, tuned by a person or
fixed by a note that names a thing. Write components even for a one-off: the next note will name one of them.

## What a component is

A raea component is a React function component that draws one thing from its props and the frame:

- **Frame-pure.** What it draws is a function of `useCurrentFrame()` and its props alone: no `Date`, no random number
  without a seed, no state carried from one frame to the next. A render in chunks, a still with `--at` and a live play
  must all draw the same frame.
- **JSON props.** Every prop is a plain JSON value (numbers, strings, booleans, arrays and objects of those), so it can
  sit in `defaultProps` and in `controls`. Anything a person might tune is a prop with a default, never a literal
  inside (`motion/compositions`, "Controls"). Pass elements only through `children` or a slot prop named for what goes
  there (`material`, `extension`).
- **Its own name on what it draws.** raea records which component drew each part, so a check or a note can name it:
  raea/check's `named(world, part, "component:Kettle")` finds every part `<Kettle>` drew, however its meshes are
  named (`craft/checks/no-part.ts`). Give components real nouns; a note says "the kettle", not "Group3".
- **A one-line header** saying what it is and how to use it. A component meant for `raea component add` starts with
  `// raea component add NAME: what it is, where it goes.`, the line `raea component list` shows.

### The contracts a component keeps

Each kind of component keeps the contracts that apply to it. They're what let components be dropped into a scene by
someone who didn't write them.

| contract | 2D (a card, a caption, a lower third) | 3D (an object, a set piece) | surface (a material, a shader) |
|---|---|---|---|
| **time** | placed by its parent's `<Sequence>`; cued by `say` (a phrase from the transcript) or `beat`, ends on `until`/`untilBeat` or its own length | still or moving as a function of the frame; clips started by a prop (`clip`, `at`), never by a timer | animated through `animate` tracks or a frame prop; uniforms that must move each frame need the workaround in "Shaders" |
| **space** | its band or `place` (`"top"`, `"middle"`); keeps out of the face and the captions | real metres; stands on y = 0 at its origin, so `stands-on` and the clipping check work; its size is a prop or fixed in its `NAME.parts.ts` | none: it covers whatever mesh it's given |
| **look** | colours and type from the brand kit (`text-brand-accent`, `bg-brand-surface`, `font-brand`, `theme.css`), never hex literals | materials from props or a material component, with the brand's accent as a default | finish values as props with real defaults (roughness, tint, scale in metres) |
| **sound** | brings its own foley on its moment, `sound={false}` turns it off | brings its own (a kettle's click) when it has one | none |
| **never** | sets the composition's size, the mix or the captions | adds a camera, a light rig, fog or post, or reads the scene's other objects | lights anything, or assumes a pipeline it doesn't declare |

### Who owns what

Components fit together because each concern has exactly one owner:

| owner | owns |
|---|---|
| the composition (`<Composition>` and its root component) | size, fps, length, the camera and its moves, the look and post effects, the mix (`<Mix>`), captions |
| the scene (or a set component like `<Kitchen>`) | the environment, the lights, the floor, where each object stands |
| an object component | its geometry, its materials (overridable by prop), its own animation and foley |
| a material or shader component | the surface only |

So an object never brings its own light or bloom to look good: if it needs a light to read, say so in its header, and
the scene adds it. A card never ducks the music: the mix is the root's. When a component seems to need something it
doesn't own, give it a prop and let the owner pass it down (or read it from context: the brand kit, the cues, the
story's beats).

### How data reaches a component

Down, through props and context, never sideways through globals or file reads inside a component:

- the brand kit and theme: the Tailwind classes and CSS variables every project compiles with;
- cues: the transcript's words (`useWords()`) and the beat map (`BEATS`), which a part turns into frames itself;
- tunables: `useControl("name")` deep in a tree, for a control the composition declares;
- the story: a composition times its scenes from the story's compiled script (raea/story's `compileScript`, one scene
  per beat), places one `<Sequence>` per scene, and passes each component what its beat shows. A component never
  imports the story itself; it takes the beat's content as props.

## Kinds of component, and where each gets made

Ask of each component: **what defines it?** That picks the tool. The TSX file is always the scene; everything else is
a workshop an asset passes through on its way into `assets/` (or `media/`).

| the component | defined by | made with | becomes a component as |
|---|---|---|---|
| a card, caption, lower third, chart in a video | layout and words | TSX and Tailwind (`motion/parts`, `motion/layout`) | the TSX itself |
| an interface on a screen or panel (a form, a dashboard) | an existing UI kit | shadcn components, run as an app (`motion/apps`) | `<Screen app="dash.tsx">` in 3D, or an app panel |
| an object with sizes that must fit (a shelf, a frame, a counter) | numbers | Manifold solids, `raea/room` (`bevy/meshes`, `bevy/rooms`) | TSX returning `<Mesh3d shape={...}>` with size props, plus its `NAME.parts.ts` |
| something grown or organic (a tree, terrain, a creature's body) | a rule | `sdf.*`, `grow.*`, `<Instances>` | TSX with the rule's parameters as props |
| a thing defined by its looks (a worn tool, food, a lamp) | its looks | `raea gen`, or a CC0 library model (`bevy/assets`) | `<Model src>` wrapped with props for its variants, clips and material overrides |
| a hand-authored asset: sculpted, rigged, hand-painted | a person's or a script's work in Blender | Blender, through a saved script (`raea blender run`, `bevy/blender`) | the `.glb` is an asset; the component wraps it as above |
| a Godot project's level, character or effect | a Godot project | Godot through `raea/godot` (`bevy/godot`) | its `.glb` export as a model, or its frames through `<GodotShot>` |
| a surface: stone, steel, cloth, wear | its finish | `<StandardMaterial>`, texture sets, `raea/procmat`, decals, wear (`bevy/materials`) | a material component (below) |
| an effect a material can't make: waves, a dissolve, a hologram | a shader | a `ShaderExtension` in WESL (`bevy/water` is the worked one) | a shader component (below) |
| a look: grade, fog, bloom, a paint or toon style | the shot | `<Look>`, post components (`bevy/post`) | a look preset the composition places once per shot |
| behaviour in a game: input, a character controller, pathfinding | a Bevy plugin | the plugin catalog and its adapters (`3d/plugins`) | a component that declares the plugin and drives it |
| an engine feature raea doesn't reach yet | Rust | the rows path (`3d/rows`), as an SDK gap with a roadmap task | a new raea component in `raea/3d`, then wrapped like any other |

### Build it in Blender, then make it a component?

Yes, when the thing is defined by hand-made looks (a sculpt, a rig, painted textures) or a check names a Blender
script (over its triangle budget, holes, no skeleton: `bevy/blender`). Then:

- Blender runs a **saved script** (`raea blender run blender/NAME.py`, or a typed step with `raea/blender`), so the
  asset can be made again from its source. A hand session in Blender ends by saving the script or the `.blend` that
  produced the `.glb`, beside the component.
- Blender's output is **one asset**: a `.glb` with its materials, skeleton and clips. It never places, lights, frames
  or times anything.
- The **component owns everything after**: where it stands, its size check, which clip plays when, material
  overrides, its sound. A note about the kettle's colour is fixed in `<Kettle>`'s props, not by going back to Blender.

No, when the thing is defined by numbers or a rule: a solid or `sdf`/`grow` in TSX is smaller, exact, checkable and
tunable by prop. "I know bpy" is no reason to model a shelf in Blender.

### Godot or Blender as raea's runtime?

No. raea has bindings to both (`raea/godot` types Godot's whole API; `raea/blender` types bpy steps), and they're for
workshops and for people's own Godot projects. raea's scenes run on one engine, Bevy through `raea/3d`, so that one
file renders and plays, every frame is a function of its time, and `raea check` sees every part. A second scene
runtime would split all three. Godot's and Blender's work comes in as assets (`.glb`, textures) or as frames
(`<GodotShot>`, `<BlenderShot>`), and the component wraps it.

## Materials as components

A finish used more than once is a component that returns a material, so a scene says what a surface is and a person
can tune it:

```tsx
// raea component add brushed-steel: a brushed stainless finish; put it inside a Mesh3d after its shape.
import { ExtendedMaterial, ProceduralExtension, StandardMaterial } from "raea/pbr";

/** Brushed steel. `grain` is the brushing's coarseness (0 polished, 1 coarse); `tint` a hex colour. */
export default function BrushedSteel({ grain = 0.4, tint = "#c9ccd1" }: { grain?: number; tint?: string }) {
  return (
    <ExtendedMaterial
      base={<StandardMaterial base_color={tint} metallic={1} perceptual_roughness={0.25 + 0.35 * grain} />}
      extension={<ProceduralExtension pattern="brushed" />}
    />
  );
}
// <Mesh3d><Cylinder radius={0.09} half_height={0.12} /><BrushedSteel grain={0.6} /></Mesh3d>
```

A material component goes inside a `<Mesh3d>` after its shape, like `<StandardMaterial>` itself.

- **A part asks for a job, never a colour.** A project's materials are one file of jobs (`<Mat job="carpet" w={4}
  h={3} />`: the floor, the signage, the steel), each mapped to the palette once, so a palette change reaches every
  room and a note about "the carpet" has one place to go.
- Props are the finish's real knobs with real defaults: roughness, tint, scale in metres (a texture's `uv_transform`
  from its real size, `bevy/materials`), wear amount. Keep Bevy's field names inside the component.
- Texture sets are assets the component names (`staticFile`), CC0 or the person's own.
- Its fixture is the four-finishes sheet (`bevy/materials`, "Example: four finishes side by side"): the material on a
  sphere and a cube under a raking light and a soft one, at two sizes, so a tiling or roughness mistake shows.
- A material set (a project's finishes: oak, terrazzo, brushed steel, painted plaster) is one folder of these, added
  together like the reel's set.

## Shaders as components

A shader effect is a component that wraps an `ExtendedMaterial` with a `ShaderExtension`, its WESL source a string
constant in the same file (`const SEA = /* wgsl */ \`...\``, as `bevy/water` does), and its uniforms as props:

- Declare in the header what it needs: the base material it extends, whether it's been checked under Solari
  (`bevy/solari` lists what Solari traces), and what it costs (a full-screen effect versus one object).
- Name things outside WGSL's reserved words. `shared`, `filter`, `common`, `smooth`, `target`, `module`, `meta` and
  `set` are reserved, and a pass named with one is dropped while the render still says 0 errors (`raea fix "WGSL pass
  silently does nothing"`).
- Uniforms keep the first frame's values today, and `globals.time` doesn't advance in stills. A shader that must move
  over time carries the frame through an animated material value, an `animate={{ emissive: track(KEYS) }}` with a key
  every frame, read in the shader (`raea fix "ShaderExtension animation frozen"`). Its fixture renders at least two
  frames apart and checks they differ.
- Its fixture renders it on the shapes it's meant for, in a lit scene, at two sizes.

## Done means checked alone

Every component you write or add gets a fixture: a small composition that renders it alone, beside the component
(`fixtures/NAME.tsx`, or a `<Composition>` per case in one file), with the cases that break components of its kind:

| kind | its fixture shows | and checks |
|---|---|---|
| 2D card or part | its longest and shortest text, both shapes (9:16 and 16:9), over a light and a dark frame | readable type and contrast (`raea check`), in its band, cued on its words |
| 3D object | it at its real size beside a 1.8 m figure or a 1 m cube, from its usual distance and close | `stands-on` (it touches the floor and floats nowhere), no clipping, within its triangle budget for the distance (`raea scene lint`) |
| material | the four-finishes sheet | it looks like its name under two lights (look at the sheet); tiling reads at the face's real size |
| shader | two frames apart, on its shapes, lit | it moves when it should; nothing dropped in the log |
| a set | every component of the set placed once | its components' checks, together |

A component that comes with checks of its own puts them beside it (a raea/check system in `checks/NAME.ts`, as
`craft/checks/` shows), and the composition's asks can name it: `{ check: "./checks/kettle-steam.ts", values: { what:
"component:Kettle" } }`. Check the fixture before using the component in a piece; when a note comes back about it,
fix the component and its fixture first, then the piece.

## shadcn: compatible, not competing

raea doesn't make another interface kit. shadcn's components are the interface kit, and raea puts them where shadcn
can't go: on a screen in a 3D scene, in an app panel, in a rendered video.

- A shadcn component runs as an app (`motion/apps`): `raea add` its npm packages, put the component's files in the
  app's folder (the shadcn CLI writes them, or copy them from its registry), and show it with
  `<Screen app="dash.tsx" resolution={[1280, 800]} />` or as a panel. Tailwind classes compile as in any raea project.
- The brand kit and shadcn's theme speak the same tokens: `raea brand` reads a site's shadcn-style variables
  (`--primary` and the rest, as `h s% l%`), so a site built on shadcn gives raea its colours.
- raea's own registry follows shadcn's registry format, so the shadcn CLI and its MCP server can read it, and a raea
  item can depend on shadcn's (a monitor in a scene that shows `@shadcn/chart`). What raea adds is the kinds shadcn
  has no type for: 3D objects, materials, shaders, looks, story-timed sets, with their assets and checks. The format
  is in docs/components.md.

## Promote what's reused

A component starts in a project's `components/`. When a second project wants it, move it to the workspace's
`shared/components/` (`raea component add --shared` for raea's own), with its fixture. When a team or the public
should have it, it goes in a registry. Each step keeps the same file: only where it lives changes.

## Common mistakes

- One giant scene file with no components: nothing can be reused, tuned, checked alone or named in a note.
- An object component that adds its own light, fog or bloom so it looks good alone. The scene owns light; the
  fixture adds the light the component needs.
- Literals for anything tunable inside a component (`color="#d04a3a"` deep in a mesh). Make it a prop with that
  default.
- A component that reads `Date`, `Math.random()` without a seed, or keeps state across frames: renders and plays
  disagree.
- Modelling a thing defined by numbers in Blender, or placing, lighting or timing anything there.
- Rebuilding an interface piece shadcn already has, instead of running shadcn's as an app.
- Using a component in a piece before its fixture passes.
