# Post-processing: bloom, fog, tone mapping

## What to do

Put these inside the `<Canvas3D>`, once each:

- `<Bloom intensity threshold />`: glow around bright parts. `intensity` 1 is a natural glow, 2 strong. `threshold`
  is how bright a pixel must be to glow: 0.8-1 lets only emissive surfaces and hot highlights bloom (the usual
  choice), 0 makes everything a little dreamy. Bloom earns its place on neon, screens, sparks and lit signs; give
  those materials `emissive` with `emissiveIntensity` 3-10.
- `<Fog color near far />`: things fade into `color` between `near` and `far` (scene units from the camera), or
  `density={0.1}` for exponential haze. Use the background colour as the fog colour so distant parts dissolve into it.
- `<ToneMapping mode exposure />`: how light maps to the screen. `"tony"` is the default (neutral, holds colour);
  `"aces"` is filmic and contrasty (darker, punchier); `"agx"` is soft and holds highlights; `"neutral"`,
  `"reinhard"` and `"none"` are there too. `exposure` is in stops: `+1` twice as bright, `-1` half.

## Why

These are Bevy's own camera effects (Bloom, DistanceFog, Tonemapping, Exposure) on the scene's camera. Bloom turns
the camera HDR so bright parts can exceed white before they bloom.

## Common mistakes

- Bloom with nothing bright in the scene: nothing glows. Add emissive parts or raise `exposure`.
- `threshold={0}` with a light background: the whole frame goes milky.
- Fog `far` shorter than the distance to the subject: the subject disappears.
- Animating them: their first frame's values hold for the whole piece. Animate the materials and lights instead.
- `aces` making brand colours muddy: check a still against the kit; `tony` keeps colours closer.

## Example: a neon sign in fog

```tsx
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Bloom, Box, Canvas3D, Fog, Mesh, MeshStandardMaterial, PerspectiveCamera, Text3D, ToneMapping } from "@raea/motion/3d";

function Neon() {
  const f = useCurrentFrame();
  const flicker = f % 37 === 0 ? 1 : 6; // a flicker on fixed frames, never random
  return (
    <AbsoluteFill style={{ background: "#07080c" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 0.4, 6]} lookAt={[0, 0.2, 0]} fov={45} />
        <AmbientLight intensity={0.1} />
        <Bloom intensity={1.6} threshold={0.9} />
        <Fog color="#07080c" near={5} far={14} />
        <ToneMapping mode="agx" />
        <Mesh position={[0, 0.4, 0]}>
          <Text3D size={0.7} depth={0.1}>OPEN</Text3D>
          <MeshStandardMaterial color="#1a1a1a" emissive="#ff2d75" emissiveIntensity={flicker} />
        </Mesh>
        {[0, 1, 2, 3, 4].map((i) => (
          <Mesh key={i} position={[(i - 2) * 2.2, -0.6, -2 - i * 2.5]}>
            <Box args={[0.6, 2, 0.6]} />
            <MeshStandardMaterial color="#2a2f3a" roughness={0.8} />
          </Mesh>
        ))}
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="neon" component={Neon} width={1280} height={720} fps={30} durationInFrames={60} />);
```
