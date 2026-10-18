# A 3D game

Read `motion/games` first: the step, `useGameState`, `useInput`, `useGameEvents` and the input script work the same
in 3D. This file is what changes when the play field is 3D.

## What to do

- Keep the game's state as plain numbers in world units (`x` from -3 to 3 across the lane, `z` towards the camera),
  step it purely, and draw it: each entity in the state is a `<Mesh>` whose `position` comes from the state at this
  frame. The step never touches meshes; the meshes never hold state.
- Give every entity a stable `name` from its id in the state (`name={"rock-" + r.id}`), so an entity that's removed
  doesn't hand its motion to the next one.
- Collisions are arithmetic in the step: distance between centres under the sum of radii, or overlapping boxes per
  axis. Keep the hit as a flag on the state for the frame it happens (`hit: true`) so `useGameEvents` can cue a sound.
- Randomness from the frame or the id: `const r = ((id * 9301 + 49297) % 233280) / 233280`, never `Math.random()`.
- A fixed camera behind and above the player (`position={[0, 4, 8]} lookAt={[0, 0, 0]}`) reads best; a chase camera
  is the player's position plus an offset, computed from the state.
- Score and prompts go on the page over the 3D, opaque.
- For hundreds of entities, keep them in an ECS world (`createWorld()` from `@raea/motion/ecs`, systems stepping it)
  and draw it with `<Entities world={game.at(useCurrentFrame())} />`: the engine draws each entity's `Mesh` component
  without a React element per entity.
- **Live play:** a 3D game renders with its input script, but the stage doesn't yet draw 3D for a game a person plays
  live (`bevy/limits`). For a game that must be played live, draw the field on the page (2D) and keep 3D for the
  scripted play-through video.

## Why

A 3D frame is a function of the state at that frame, and the state is a pure fold over the input, so a render, a
re-render and a play-through all show the same game.

## Common mistakes

- Moving a mesh by mutating a ref or an object inside the step: return new state, draw from it.
- Entities without names, drawn from an array that shrinks: the survivors swap motion.
- Collision against the mesh's look rather than the state's numbers: the engine never reports contacts back.
- `Math.random()` for spawns: every frame replays a different game.

## Example: steer a cube through falling rocks (scripted input for the render)

```tsx
import { AbsoluteFill, Composition, registerRoot, useGameState, type FrameInput } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, Sphere } from "@raea/motion/3d";

const FRAMES = 150;
interface Rock { id: number; x: number; z: number }
interface Game { x: number; rocks: Rock[]; next: number; hits: number; hit: boolean }
const INITIAL: Game = { x: 0, rocks: [], next: 0, hits: 0, hit: false };

/** The scripted run a render plays when nobody is pressing keys: left, then right, then left. */
function scripted(frame: number): string[] {
  const t = frame % 60;
  return t < 20 ? ["ArrowLeft"] : t < 45 ? ["ArrowRight"] : [];
}

function step(s: Game, input: FrameInput, frame: number): Game {
  const keys = input.keys.length ? input.keys : scripted(frame);
  const dx = (keys.includes("ArrowRight") ? 0.12 : 0) - (keys.includes("ArrowLeft") ? 0.12 : 0);
  const x = Math.max(-3, Math.min(3, s.x + dx));
  let rocks = s.rocks.map((r) => ({ ...r, z: r.z + 0.18 })).filter((r) => r.z < 6);
  let next = s.next;
  if (frame % 12 === 0) {
    const seed = ((next * 9301 + 49297) % 233280) / 233280;
    rocks = [...rocks, { id: next, x: -3 + seed * 6, z: -14 }];
    next += 1;
  }
  const struck = rocks.find((r) => Math.abs(r.z - 3) < 0.5 && Math.abs(r.x - x) < 0.8);
  if (struck) rocks = rocks.filter((r) => r !== struck);
  return { x, rocks, next, hits: s.hits + (struck ? 1 : 0), hit: Boolean(struck) };
}

function Dodge() {
  const g = useGameState(INITIAL, step);
  return (
    <AbsoluteFill style={{ background: "#0d1321" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 4, 9]} lookAt={[0, 0, 0]} fov={50} />
        <AmbientLight intensity={0.35} />
        <DirectionalLight position={[3, 8, 5]} intensity={1.2} />
        <Mesh position={[0, -0.55, -4]}>
          <Box args={[8, 0.1, 22]} />
          <MeshStandardMaterial color="#1d2d44" roughness={0.9} />
        </Mesh>
        <Mesh name="player" position={[g.x, 0, 3]} scale={g.hit ? 1.25 : 1}>
          <Box args={[0.8, 0.8, 0.8]} />
          <MeshStandardMaterial color={g.hit ? "#ef4444" : "#f0ebd8"} roughness={0.4} />
        </Mesh>
        {g.rocks.map((r) => (
          <Mesh key={r.id} name={`rock-${r.id}`} position={[r.x, 0, r.z]} rotation={[r.z, r.id, 0]}>
            <Sphere args={[0.45]} />
            <MeshStandardMaterial color="#748cab" roughness={0.8} />
          </Mesh>
        ))}
      </Canvas3D>
      <div style={{ position: "absolute", left: 48, top: 36, fontFamily: "Inter", fontWeight: 800, fontSize: 48, color: "#f0ebd8" }}>{`HITS ${g.hits}`}</div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="dodge3d" component={Dodge} width={1280} height={720} fps={30} durationInFrames={FRAMES} />);
```
