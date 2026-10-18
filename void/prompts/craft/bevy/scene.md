# Scene and camera

## What to do

- Put the 3D in a `<Canvas3D>` inside an `AbsoluteFill`. The box holding the Canvas3D is cleared to show the 3D, and
  its `background` (a hex colour) becomes the 3D's clear colour. A gradient background doesn't reach the 3D (it
  clears to black); use a flat colour, or a large backdrop mesh.
- Give it one `<PerspectiveCamera position lookAt fov>`. `lookAt` is the point it faces (the origin by default);
  `fov` is vertical, in degrees (40-50 reads as a product lens, 60+ as wide). The y axis is up, and +z points at
  the viewer.
- Move the camera by computing `position` and `lookAt` from the frame: a dolly is `z` easing from 8 to 5, an orbit is
  `[r * Math.sin(a), h, r * Math.cos(a)]`. Both are drawn exactly per frame in render and play.
- Use `<Group position rotation scale>` for things that move together (a product on a turntable, a car and its
  wheels). Groups nest; their transforms compose.
- Put 2D over the 3D as ordinary page elements after the Canvas3D. Keep page parts over the 3D opaque: a partly
  transparent part over 3D blends a little differently in a render than in play (a known engine difference), so fade
  text in over a flat panel rather than over the 3D, or accept a slight mismatch while it fades.

## Why

The camera and the groups are rows the engine reads each frame, keyed per frame, so a camera move is as exact as a
title's slide. Groups are composed into world space before the engine sees them.

## Common mistakes

- `rotation={[0, 90, 0]}`: radians. Use `Math.PI / 2`.
- The camera at the subject's position, or looking away from it: nothing shows. Start at `[0, 1, 6]` looking at
  `[0, 0, 0]` and adjust.
- Two cameras to cut between angles. Use one camera whose position jumps at the cut frame.
- `useFrame` or refs from react-three-fiber. There's no render loop: write the frame's values as props.

## Example: a product on a turntable with a slow push-in and a title over it

```tsx
import { AbsoluteFill, Composition, Easing, interpolate, registerRoot, useCurrentFrame, useVideoConfig } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, Cylinder, DirectionalLight, Group, Mesh, MeshStandardMaterial, PerspectiveCamera } from "@raea/motion/3d";

function Turntable() {
  const f = useCurrentFrame();
  const { fps, durationInFrames } = useVideoConfig();
  const turn = (f / fps) * (Math.PI / 3); // a sixth of a turn a second
  const z = interpolate(f, [0, durationInFrames - 1], [7, 5], { easing: Easing.out(Easing.cubic), extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <AbsoluteFill style={{ background: "#14161c" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.6, z]} lookAt={[0, 0.5, 0]} fov={40} />
        <AmbientLight intensity={0.4} />
        <DirectionalLight position={[3, 5, 4]} intensity={1.4} />
        <Group rotation={[0, turn, 0]}>
          <Mesh position={[0, -0.1, 0]}>
            <Cylinder args={[1.4, 1.4, 0.2]} />
            <MeshStandardMaterial color="#2b2f3a" roughness={0.6} />
          </Mesh>
          <Mesh position={[0, 0.6, 0]}>
            <Box args={[0.9, 1.2, 0.9]} />
            <MeshStandardMaterial color="#e8b04a" roughness={0.35} metalness={0.2} />
          </Mesh>
        </Group>
      </Canvas3D>
      <div style={{ position: "absolute", top: 80, width: "100%", textAlign: "center", fontFamily: "Inter", fontWeight: 800, fontSize: 72, color: "#ffffff" }}>
        Made to be seen
      </div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="turntable" component={Turntable} width={1080} height={1080} fps={30} durationInFrames={90} />);
```
