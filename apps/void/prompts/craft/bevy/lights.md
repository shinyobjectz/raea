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
- Shadow quality: by default the maps are 4096 texels a side and the sun's four cascades cover 150 m from the camera,
  the first ending at 8 m, so shadows are sharp up close and still there down a long street. For a different scale
  put one `<Shadows>` inside the Canvas3D: `distance` (metres the sun's shadows reach; shorter is sharper),
  `firstCascade` (about the distance of the nearest subject), `mapSize` (1024-8192), `cascades`, `overlap`, and
  `bias` / `normalBias` if a lit surface shows stripes of its own shadow (acne) or shadows float off what casts them.
  `filter="hardware"` gives crisper, cheaper 2x2 edges instead of the smooth Gaussian ones. Its first frame's values
  hold for the whole piece.

  ```tsx
  <Shadows distance={60} firstCascade={5} mapSize={4096} bias={0.03} normalBias={2} />
  ```
- Light colour is a hex string: warm key `#ffe2c0`, cool fill `#9fc4ff`. Animate `intensity`, `position` and
  `target` from the frame for a light sweep or a flicker (seeded, never `Math.random()`).

- Soft shadows: `softShadow` on a light (PCSS). On a DirectionalLight `true` or the sun's size as its shadows see it
  (0.5 by default; 5-10 for a studio's big softbox key); on a SpotLight or PointLight (with `castShadow`) `true` or the
  bulb's radius in metres (0.25). The shadow is sharp where it touches and softens with distance, like a real one.
  Bevy dithers the penumbra in a fixed pattern (the same on every render, but visible at 1:1); a larger `<Shadows
  mapSize>` and a smaller size reduce it, and `<Shadows bias={0.2} distance={30} />` keeps a big soft sun from
  speckling the floor it falls on (`sdk/motion/bench/bevy/studio` is a high-key product shot lit so).
- Light from a place: `<Environment src={staticFile("sky.hdr")} intensity rotation background />` lights the scene
  from an equirectangular panorama (Poly Haven's CC0 HDRIs, the 1k or 2k `.hdr`; `.exr`, `.png` and `.jpg` work too):
  soft light from every side, and the panorama reflected in every glossy and metal surface, which is what makes metal
  and polished stone look real. `background` draws the panorama as the sky; leave it off indoors behind walls, or when
  the page draws its own backdrop. `intensity` 1 is the panorama as shot at the default exposure; `rotation`
  (radians, counter-clockwise from above) turns the light and sky together, to put the sun behind the subject or on
  its side. Keep a DirectionalLight for crisp shadows: an environment lights but casts none. An AmbientLight adds to
  it.
- A film of many places through one Canvas3D: give each scene its own `src` (and intensity, rotation, background);
  the panorama changes on the frame the scene does, the light and reflections with it. Every panorama the piece uses
  is loaded before the first frame, so use a handful, not dozens.

## Why

Lights are rows the engine moves every frame, the same in render and play. Bevy lights physically: a light's power
spreads with distance, so placing and sizing it matters more than in a flat renderer.

## Common mistakes

- No light at all, or only an AmbientLight: the scene renders flat or black.
- A PointLight inside a mesh: its light never leaves the mesh.
- Many shadow-casting point lights: each renders six shadow maps. Keep shadowed point lights to one or two.
- Blurry sun shadows in a small scene: the cascades are spread over 150 m. A `<Shadows distance={20} firstCascade={3} />`
  puts the map's texels where the scene is.
- Speckle on a model's lit stone or brick that doesn't change with shadows on or off is its texture, not its shadow:
  the engine gives glTF textures mipmaps at load, so it shouldn't appear; if it does, the model's textures are float
  or compressed (KTX2/basis with no mips), which it leaves as they are.
- `angle` in degrees on a SpotLight (`angle={30}` throws: it's a half-angle in radians up to π/2).
- A light colour from CSS (`var(--brand-accent)`): hex only.
- An Environment and a bright ambient on top: the panorama already fills the shadows; an AmbientLight of 0.05-0.2 at
  most, or none.
- Expecting a sharp sky from a 1k panorama: it covers 360 degrees, so on a 50-degree lens a 1k (or even 2k) sky is
  soft. Use it for light and reflections, blur the background with depth of field, or put a picture plane in front
  for a crisp horizon.
- An outdoor HDRI indoors: the scene is lit by a sky it can't see. Match the panorama to the place (an interior's
  for a hall), and switch it when the film cuts.

## Example: one panorama outside, another inside, cut on frame 30

```tsx
import { AbsoluteFill, Composition, interpolate, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, Environment, Mesh, MeshStandardMaterial, PerspectiveCamera, Sphere, Torus } from "@raea/motion/3d";

function Shot() {
  const f = useCurrentFrame();
  const outside = f < 30;
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.2, 5]} lookAt={[0, 0.6, 0]} fov={50} />
        <AmbientLight intensity={0.05} />
        <Environment
          src={staticFile(outside ? "venice_sunset_1k.hdr" : "museum_of_ethnography_1k.hdr")}
          intensity={interpolate(f, [0, 59], [0.8, 1.2])}
          rotation={interpolate(f, [0, 59], [0, 1.2])}
          background
        />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[8, 0.1, 8]} />
          <MeshStandardMaterial color="#8a8580" roughness={0.6} />
        </Mesh>
        <Mesh position={[-1.4, 0.6, 0]}>
          <Sphere args={[0.6]} />
          <MeshStandardMaterial color="#ffffff" metalness={1} roughness={0.04} />
        </Mesh>
        <Mesh position={[1.4, 0.6, 0]} rotation={[0.4, f * 0.03, 0]}>
          <Torus args={[0.45, 0.16]} />
          <MeshStandardMaterial color="#e0b050" metalness={1} roughness={0.25} />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="places" component={Shot} width={960} height={540} fps={30} durationInFrames={60} />);
```

The panoramas live in the media folder (`media/venice_sunset_1k.hdr`): download them from polyhaven.com (CC0).

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
