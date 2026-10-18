# Lights

## What to do

- Start every scene with a key light and a little fill: one `<DirectionalLight position intensity>` (the sun: parallel
  rays from `position` towards `target`, the origin by default) and an `<AmbientLight intensity={0.2-0.5}>` so the
  shadow side isn't black.
- Add `<PointLight position intensity color distance>` for a bulb or a warm rim, and `<SpotLight position target
  angle penumbra intensity>` for a stage spot or a hero pool of light. `angle` is the cone's half-angle in radians
  (0.3-0.6 for a pool), `penumbra` 0-1 softens its edge.
- Brightness: `intensity` 1 on a DirectionalLight is bright daylight on a matte surface. A PointLight or SpotLight
  of intensity 1 lights a surface 3 units away about as brightly; it falls off with distance squared, so a bulb 6
  units away needs about 4. `distance` is how far it reaches at all (20 by default).
- Shadows: a DirectionalLight casts them by default (`castShadow={false}` to stop). Point and spot lights cast none
  unless `castShadow`. Every mesh casts and receives. A floor (a wide thin Box) is what makes shadows read.
- Light colour is a hex string: warm key `#ffe2c0`, cool fill `#9fc4ff`. Animate `intensity`, `position` and
  `target` from the frame for a light sweep or a flicker (seeded, never `Math.random()`).

## Why

Lights are rows the engine moves every frame, the same in render and play. Bevy lights physically: a light's power
spreads with distance, so placing and sizing it matters more than in a flat renderer.

## Common mistakes

- No light at all, or only an AmbientLight: the scene renders flat or black.
- A PointLight inside a mesh: its light never leaves the mesh.
- Many shadow-casting point lights: each renders six shadow maps. Keep shadowed point lights to one or two.
- `angle` in degrees on a SpotLight (`angle={30}` throws: it's a half-angle in radians up to π/2).
- A light colour from CSS (`var(--brand-accent)`): hex only.

## Example: a soft spot on a product, a warm bulb circling it, a cool sun

```tsx
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, PointLight, Sphere, SpotLight } from "@raea/motion/3d";

function Stage() {
  const f = useCurrentFrame();
  const a = f * 0.06;
  return (
    <AbsoluteFill style={{ background: "#0a0b0f" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 2.5, 6]} lookAt={[0, 0.5, 0]} fov={45} />
        <AmbientLight intensity={0.15} />
        <DirectionalLight position={[-3, 6, 2]} intensity={0.3} color="#9fc4ff" />
        <SpotLight position={[0, 5, 1]} target={[0, 0, 0]} angle={0.45} penumbra={0.5} intensity={3} castShadow />
        <PointLight position={[Math.cos(a) * 2, 1.2, Math.sin(a) * 2]} intensity={0.8} color="#ffcf8a" distance={8} />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[10, 0.1, 10]} />
          <MeshStandardMaterial color="#8d939b" roughness={0.85} />
        </Mesh>
        <Mesh position={[0, 0.6, 0]}>
          <Sphere args={[0.6]} />
          <MeshStandardMaterial color="#d8d2c4" roughness={0.3} />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="stage" component={Stage} width={1280} height={720} fps={30} durationInFrames={90} />);
```
