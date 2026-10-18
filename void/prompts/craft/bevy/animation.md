# Animation

## What to do

- Animate 3D exactly as you animate 2D: compute props from `useCurrentFrame()` with `interpolate` (clamped),
  `spring`, `Easing` and plain arithmetic. Spin: `rotation={[0, f * 0.04, 0]}`; drop-in: a `spring` from 0 to 1 mapped
  to `y`; reveal: `scale` from a spring.
- Time a part with `<Sequence from durationInFrames layout="none">` inside the Canvas3D. The part exists only for
  those frames and its own frame count starts at 0 there. A part that appears or disappears should have a `name`.
- Camera moves are props on the camera (`bevy/scene`). Light sweeps are props on the light.
- A glTF model's clip: `<Model src animation="Walk">` plays the clip at the Sequence's time (`frame / fps` seconds),
  looping; `time={...}` sets it yourself (slow motion: `time={f / fps * 0.5}`, a hold: a fixed number),
  `loop={false}` holds the last pose.
- Stagger many parts from their index: `const s = spring({frame: f - i * 4, fps})`.

## Why

The engine draws each frame from that frame's values, keyed per frame. Nothing advances on its own: a model's clip is
posed at the time you give it, not by a clock, so any frame renders alone and play matches the render.

## Common mistakes

- `useFrame((_, dt) => ref.current.rotation.y += dt)`: there's no render loop. `rotation={[0, f * speed, 0]}`.
- Changing `args` or `shape` to animate size: animate `scale`.
- A `spring` without `fps`, or an `interpolate` without clamping, overshooting into nonsense values.
- A part shown with `{f > 30 && <Mesh ...>}` among others of the same shape, without a `name`: give it one so it
  doesn't take over another part's motion.

## Example: blocks springing in one after another, then a turn

```tsx
import { AbsoluteFill, Composition, interpolate, registerRoot, Sequence, spring, useCurrentFrame, useVideoConfig } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Group, Mesh, MeshStandardMaterial, PerspectiveCamera, Sphere } from "@raea/motion/3d";

const COLORS = ["#f97316", "#22c55e", "#3b82f6", "#e11d48"];

function Blocks() {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const turn = interpolate(f, [45, 89], [0, Math.PI / 2], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <AbsoluteFill style={{ background: "#f5f1ea" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 2, 7]} lookAt={[0, 0.3, 0]} fov={40} />
        <AmbientLight intensity={0.6} />
        <DirectionalLight position={[3, 6, 4]} intensity={1.1} />
        <Mesh position={[0, -0.45, 0]}>
          <Box args={[10, 0.1, 6]} />
          <MeshStandardMaterial color="#d9d2c5" roughness={0.9} />
        </Mesh>
        <Group rotation={[0, turn, 0]}>
          {COLORS.map((c, i) => {
            const s = spring({ frame: f - i * 6, fps, config: { damping: 12 } });
            return (
              <Mesh key={c} name={`block-${i}`} position={[(i - 1.5) * 1.1, interpolate(s, [0, 1], [3, 0]), 0]} scale={Math.max(0.01, s)}>
                <Box args={[0.8, 0.8, 0.8]} />
                <MeshStandardMaterial color={c} roughness={0.4} />
              </Mesh>
            );
          })}
        </Group>
        <Sequence from={40} layout="none">
          <Mesh name="ball" position={[0, 1.1, 0]}>
            <Sphere args={[0.3]} />
            <MeshStandardMaterial color="#111827" roughness={0.2} />
          </Mesh>
        </Sequence>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="blocks" component={Blocks} width={1280} height={720} fps={30} durationInFrames={90} />);
```
