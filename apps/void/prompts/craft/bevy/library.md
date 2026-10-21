# The game library: examples, games, design patterns and free 3D assets

## What to do

- Before building a game mechanic or a 3D scene from nothing, search the library by meaning with the `game_library`
  tool and a `query`. In void it is one of your tools, called like `shell`, never a command: the shell has no
  `raea-meaning` and no network, so don't look for one there, and never download a model any other way (the browser
  included). (In a terminal outside void, the same search is `raea-meaning games-find "QUERY" -k 8 [--kind K]`.)
  Describe what you need in plain words: "low-poly knight with sword animations", "coyote time and jump buffer",
  "tower defense template", "fog that hides the far end". It answers with records, best first.
- Narrow with `kind`: `asset` (models, kits, materials), `rig` (rigged characters), `animation` (clip sets),
  `pattern` (game design: the rules and numbers to implement), `example` (Bevy's own examples and tutorials),
  `game` (open-source Bevy games), `plugin` (Bevy crates, for extending the SDK).
- Read what a record gives you:
  - `tsx`: a raea TSX equivalent you can start from (`raea-meaning games-show ID` prints its source).
  - `topic`: the bevy craft file that covers it (`bevy/lights`, `bevy/games` ...); `bevy/limits` means raea can't
    do it yet.
  - `pattern`: the implementer's rules for a design pattern (timings, formulas). Write them into your game's step.
  - for assets: `format`, `polycount`, `rig`, `animations` (the clip names to pass to `<Model animation>`), `raea`
    (whether it's been checked to render here), `licence` and `attribution`.
- **Only glTF loads** (`format` holding `glb` or `gltf`). A search for a character often ranks FBX-only records first
  (Kenney's and OpenGameArt's rigs): skip them, and prefer a record whose `raea` says it renders. Two that do:
  `robot-expressive` (CC0, 14 clips: Idle, Walking, Running, Jump, Death, Punch, Wave ...) and `khronos-cesium-man`.
- To use a model, `fetch` the record's id (only records with `ship: "download"`): its files are fetched from their
  recorded URLs into `~/.raea/assets/games/ID/` and the model is copied into this object's `media/models/ID/`. The
  answer gives the path for `<Model src={staticFile("models/ID/FILE.glb")} animation="Walk" />`. A pack (a kit of many
  models) lists its models first: fetch again with `file` set to one of them.
- Render one still first (`raea-motion render EDIT.tsx --frame N -o F.png`), then set `scale` and `position`: models
  come at their author's scale. The record's `scale_hint`, when there is one, says how big the model is.
- Keep the attribution: a CC-BY asset must be credited where the piece is shown (an end card or the description).
  CC0 needs none, but naming the author is kind.

## Why

The library is a small catalogue with a vector index that ships with raea (a few MB), so searching costs nothing and
works offline; the model files themselves are fetched on demand, by licence-checked URL, and never bundled.

## Common mistakes

- Fetching a record with `ship: "link"`: its licence doesn't allow copying (Mixamo, non-commercial, a game's own
  art) or it has no direct file. Read it at its `source` instead, and don't copy its files.
- A model with `raea: "fails: JPEG textures"`: the engine can't load JPEG textures yet. Pick another model, or
  convert its textures to PNG first.
- Guessing clip names: use the record's `animations`, or read the render's error, which lists the clips.
- Copying a game's code or art from a `game` record: those are for how a mechanic was built, under their own
  licences. Write the mechanic yourself in TSX.

## Example: a rigged character found in the library

`game_library {query: "rigged animated robot character"}` finds `robot-expressive` (CC0, 14 clips);
`game_library {fetch: "robot-expressive"}` copies it to `media/models/robot-expressive/RobotExpressive.glb`.

```tsx
import { AbsoluteFill, Composition, interpolate, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, Model, PerspectiveCamera } from "@raea/motion/3d";

function Walk() {
  const f = useCurrentFrame();
  const x = interpolate(f, [0, 89], [-2.4, 0.6]);
  return (
    <AbsoluteFill style={{ background: "#1b2230" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 2.4, 7]} lookAt={[0, 0.9, 0]} fov={40} />
        <AmbientLight intensity={0.45} />
        <DirectionalLight position={[3, 6, 4]} intensity={1.3} castShadow />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[12, 0.1, 8]} />
          <MeshStandardMaterial color="#d9d4c7" roughness={0.9} />
        </Mesh>
        <Model src={staticFile("models/robot-expressive/RobotExpressive.glb")} animation="Walking" position={[x, 0, 0]} rotation={[0, Math.PI / 2, 0]} scale={0.4} />
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="walk" component={Walk} width={1280} height={720} fps={30} durationInFrames={90} />);
```
