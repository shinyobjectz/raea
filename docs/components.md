# Components: raea's registry, compatible with shadcn's

How raea's components are shared: one registry format that the shadcn CLI and its MCP server can read, extended for
what shadcn has no type for (3D objects, materials, shaders, looks, story-timed sets, assets, checks). The guide an
agent works from is `raea guide components` (sdk/guides/craft/components.md); this file is the design and the plan.
Written 11-23.

## Where it starts

- `raea component list | add NAME [--shared] [--force]` (engine/cli/src/index.ts, docs/layout.feature LY4) copies a
  set or a single component from `REGISTRY`, which tools/pack.ts packs into engine/cli/src/registry.gen.ts from
  examples/parts. Two sets today, both 2D motion: `brand` (9 parts) and `reel`. Copy-in, owned by the project, never
  updated behind its back: shadcn's model already.
- `--shared` puts it in the workspace's `shared/components/`, imported as `@shared/components/NAME`.
- raea records which component drew each part, so raea/check's `named(world, part, "component:Orb")` and a note's ask
  can name a component (sdk/guides/craft/checks/no-part.ts).
- Each registry component has its own P26 row (docs/agents.md, L1).
- shadcn's own components already run in raea as apps on a Blitz page (`<Screen app>`, app panels:
  sdk/guides/craft/motion/apps.md), and docs/targets.feature T3 holds shadcn-style apps to Chrome.
- `raea brand` reads shadcn-style theme variables (engine/cli/src/scrape.ts).

What's missing: no 3D, material, shader or look items; no registries beyond the one compiled in; no assets with an
item; no checks or fixture travelling with an item; nothing for a component's engine range; no way to publish.

## The decision: shadcn's format, raea's kinds in `meta`

Every raea registry item is a valid shadcn `registry-item.json`
(ui.shadcn.com/schema/registry-item.json), and every raea registry serves a valid `registry.json` index. So:

- the shadcn CLI can `add` a raea item into any project and gets its code;
- the shadcn MCP server lists and searches raea's registry with no work on our side;
- a raea item can depend on a shadcn item by address (`@shadcn/chart`), and shadcn items can depend on raea's;
- raea doesn't define a competing interface kit. shadcn is the interface kit; raea is where shadcn's components go
  into 3D scenes, rendered video and games, and raea adds the kinds an interface kit doesn't have.

The mapping:

| shadcn field | raea's use |
|---|---|
| `type` | `registry:component` for a single 2D TSX component, `registry:block` for a set, `registry:item` for every raea-only kind (its kind in `meta.raea.kind`) |
| `files[]` | text files with inline `content`. TSX targets `@components/NAME.tsx` (the placeholder resolves from `components.json`); shader sources, checks, specs and fixtures are `registry:file` with `~/` targets: `~/components/NAME/fixture.tsx`, `~/checks/NAME.ts`, `~/NAME.parts.ts` |
| `dependencies` | npm packages, installed by `raea add` (raea's own package manager; no Node needed) |
| `registryDependencies` | other items by address: `@raea/brushed-steel`, `@shadcn/card`, a URL, `owner/repo/item.TAG` |
| `cssVars` | brand tokens (`--brand-accent` and the rest, theme.css's names); see "Theme" |
| `docs` | the item's markdown: what it is, its props, its contracts, its known limits |
| `categories` | raea's kinds and domains: `3d`, `material`, `shader`, `motion`, `story`, `game` |
| `meta.raea` | everything shadcn has no field for (next section) |

### `meta.raea`

```jsonc
"meta": { "raea": {
  "kind": "object",              // part | set | object | material | shader | look | screen | behaviour | story-set | check
  "engine": ">=0.1.10",          // the raea versions it was checked on; `raea doctor` flags an install outside it
  "assets": [                    // binary files never go inline: fetched by URL, pinned, licensed
    { "target": "~/assets/kettle/kettle.glb", "url": "https://.../kettle-3f2a91c0.glb",
      "sha256": "3f2a91c0...", "bytes": 482113, "licence": "CC0-1.0", "source": "made with blender/kettle.py" }
  ],
  "contracts": { "size": [0.24, 0.26, 0.18], "stands": true, "band": null, "sound": "own" },
  "controls": { "tint": { "kind": "color" }, "grain": { "kind": "number", "min": 0, "max": 1 } },
  "pipelines": { "forward": "checked", "solari": "checked" },   // or "unchecked"; a shader or material says this
  "backends": { "metal": "checked", "vulkan": "checked", "dx12": "unchecked" },  // where its fixture was drawn (wgpu)
  "plugins": [],                 // Bevy plugin catalog entries it needs (sdk/raea/bench/bevy-plugins/package.json)
  "fixture": "~/components/kettle/fixture.tsx",
  "checks": ["stands-on", "~/checks/kettle-steam.ts"],
  "stills": [{ "frame": 0, "sha256": "...", "url": "https://.../kettle-0.png" }],   // what its fixture drew when published
  "cost": { "triangles": 18400, "draws": 3 },
  "story": { "shows": ["product", "proof"] },   // beat roles a story-set can fill (see "Story")
  "build": "~/blender/kettle.py"  // how the asset is made again, when it was made, not drawn
}}
```

Why each field exists:

- **assets**: shadcn's `content` is text. A `.glb`, a texture or a sound is fetched from its URL by `raea component add`,
  checked against its sha256, and written to its target. The shadcn CLI ignores `meta`, so an item it installs gets
  its code without its assets; the component's own code then fails at render naming the fix (`raea component add NAME`
  or `raea doctor --fix` fetches what's missing). Code stays usable everywhere; assets stay out of JSON.
- **engine**: shadcn items have no version range because React's surface is stable. raea's isn't yet (prop names,
  Bevy versions). An item names the raea it was checked on, and `raea doctor` (SK10) reports items outside the
  install's version.
- **contracts and controls**: what the guide's contract table says, as data, so a scene planner and the app's
  Controls panel can read them without running the code.
- **pipelines**: a shader or extended material may draw wrong or not at all under Solari (sdk/guides/craft/bevy/solari.md);
  an item says which pipelines its fixture was checked on.
- **backends**: raea draws through wgpu on Metal (Mac), Vulkan (Linux, and Mesa's lavapipe in the release's checks)
  and DX12 (Windows). WESL that one backend's compiler accepts can be refused or drawn differently by another (a
  reserved word, a precision difference, a binding limit), so a shader or material item says where its fixture was
  drawn, and `raea component check` on a machine adds its own backend.
- **fixture, checks, stills**: an item is published with the composition that renders it alone, the checks that pass
  there, and the stills it drew, so `raea component check NAME` can tell a consumer's install drew the same.
- **build**: an asset made in Blender or by a generator carries the script that makes it again, so the asset isn't a
  dead end (sdk/guides/craft/components.md, "Build it in Blender, then make it a component?").

### Kinds

| kind | what it is | its fixture checks |
|---|---|---|
| part | a 2D TSX component on a video (a card, captions) | readable, contrast, in its band, cued |
| set | several components added together (`brand`, `reel`, a material set) | each member's checks |
| object | a 3D thing at real size | stands-on, no clipping, triangle budget at its distance |
| material | a component returning a material (`<Mat job>`, `<BrushedSteel>`) | the four-finishes sheet looks like its name |
| shader | a component wrapping a `ShaderExtension` with its WESL | moves when it should; nothing dropped in the log; per declared pipeline |
| look | a grade, a fog, a paint or toon style placed once per shot | exposure and contrast measures on its fixture |
| screen | a 3D screen or panel showing a shadcn app, depending on `@shadcn/*` items | the app renders on the screen at its resolution |
| behaviour | a game system driving a catalog plugin | its headless program passes (`raea run`) |
| story-set | components that fill a story's beat roles together (a proof montage, a reveal) | each beat role filled; the set's checks |
| check | a raea/check system with its values and cheats, as craft/checks/ holds | its cheats fail it |

## Addresses and configuration

raea reads the same `components.json` the shadcn CLI writes, its `registries` field and its aliases, so one project
can hold `@shadcn/*`, `@raea/*` and a team's own namespace:

```json
{ "registries": {
    "@raea": "https://raea.io/r/{name}.json",
    "@studio": { "url": "https://registry.example.com/r/{name}.json", "headers": { "Authorization": "Bearer ${STUDIO_TOKEN}" } }
  },
  "aliases": { "components": "./components" } }
```

- `raea component add @raea/kettle @shadcn/card` resolves each from its registry, with its `registryDependencies`.
- A bare name (`raea component add reel`) keeps meaning raea's own, from the copy compiled into the binary, so it
  works offline. That compiled copy is the `@raea` registry as of the release.
- With no `components.json`, raea writes one the first time a namespaced item is added, with `@raea` in it.

## Theme

shadcn's theme and raea's brand kit are the same idea: named colour, radius and type variables that classes use.

- A shadcn theme (`registry:theme`, or a site's CSS variables) becomes a brand kit: `--primary` to the brand accent,
  `--background` and `--card` to the surfaces, `--radius` to `rounded-brand`, `--font-sans` to `font-brand`. `raea
  brand` already reads the variables from a site; reading a `registry:theme` item is the same mapping.
- A raea brand kit exports as a `registry:theme` with `cssVars`, so a person's shadcn app and their raea videos share
  one theme.
- 3D gets its colours from the same tokens through material jobs: a project's `<Mat job="accent">` maps the job to
  the brand accent once.

## Story

A story (raea/story, docs/story.md) compiles to a script, one scene per beat (`compileScript`). A composition places
one `<Sequence>` per scene and passes each component what its beat shows (`onScreenOf`). Components never import a
story. A story-set declares the beat roles it can fill (`meta.raea.story.shows`), so a planner can pick components
for a story's beats and a check can find a beat the story marks as shown with nothing on it.

## Checks

- A component's identity is already in the world (`component:NAME`), so a check written for a kind of component (no
  orbs, steam rises from the kettle) works on every piece that uses it.
- An item's checks are raea/check systems in `checks/`, the way sdk/guides/craft/checks/ are written: values, and the
  cheats each check fails.
- `raea component check NAME` renders the fixture, runs the item's checks, and compares the stills to `meta.raea.stills`
  (SSIM, as P26 does). It's the gate for publishing to `@raea`, and what a consumer runs after adapting a component.

## Where things get made

The routing rule, the same in the guide and in sdk/guides/craft/bevy/scenes.md: what defines a thing picks its tool
(numbers: solids; a rule: `sdf`/`grow`; looks: a generator or a CC0 model; hand-made looks: Blender through a saved
script; a Godot project: Godot through `raea/godot`; an engine feature raea lacks: Rust through the rows path). The
TSX is always the scene, and every route ends in a component. Godot and Blender aren't raea runtimes: raea's scenes run
on Bevy so one file renders and plays and `raea check` sees every part; their work comes in as assets or frames.

## Build order

Each step lands on its own and is useful alone.

- **CP1. The guide** (this landing): `raea guide components`, copied into the raea skill, and the skill routes to it.
- **CP2. examples/components with fixtures**: LY4's rename of examples/parts, each component gaining a fixture
  composition, and tools/pack.ts packing a `registry.json` and one `registry-item.json` per component beside
  registry.gen.ts. A test validates every item against shadcn's published schema (vendored, pinned by sha256).
- **CP3. First 3D items**: a material set (four finishes, from bevy/materials' examples), one shader (the sea, from
  bevy/water), one object with a `.glb` asset and a `build` script, one screen showing `@shadcn/card`. Each passes
  `raea component check`.
- **CP4. `raea component add` reads `components.json`**: namespaced and URL addresses, `registryDependencies`
  resolved, assets fetched and pinned, the shadcn CLI's file layout kept. Offline, bare names keep working.
- **CP5. `raea component check NAME`**: fixture, checks, stills; P26 rows read from it.
- **CP6. Serve `@raea`**: raea.io/r/registry.json and /r/{name}.json from the release's packed files, assets on the
  public models release or R2 by sha. List it in shadcn's registry directory.
- **CP7. `raea component publish DIR`**: a project's or workspace's components to a registry folder (static JSON), for a
  team's own namespace.
- **CP8. Theme both ways**: a `registry:theme` into a brand kit, and a brand kit out as one.
- **CP9. Story-sets**: `meta.raea.story.shows`, a planner that proposes components for a story's beats, and the check
  for a shown beat with nothing on it.
- **CP10. Props from one schema** (docs/untangle.md F1): when raea's own components' props are generated from their
  Rust structs, an item's `controls` and docs come from the same place.

## Open decisions

- Hosting `@raea` at raea.io/r (Cloudflare, beside the site) or as a GitHub registry (`shinyobjectz/raea-components`,
  addressed `owner/repo/item.TAG`, which shadcn now reads directly).
- Whether `@raea` items may carry assets on R2 (fast, ours) or only on GitHub release assets (public, permanent).
- Whether `raea component add` should run an item's `build` script when its asset URL is gone, or only report it.
