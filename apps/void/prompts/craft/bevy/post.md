# Post-processing: bloom, fog, light in the air, ambient occlusion, anti-aliasing, tone mapping, the lens, motion blur

## What to do

Put these inside the `<Canvas3D>`, once each:

- `<Bloom intensity threshold />`: glow around bright parts. `intensity` 1 is a natural glow, 2 strong. `threshold`
  is how bright a pixel must be to glow: 0.8-1 lets only emissive surfaces and hot highlights bloom (the usual
  choice), 0 makes everything a little dreamy. Bloom earns its place on neon, screens, sparks and lit signs; give
  those materials `emissive` with `emissiveIntensity` 3-10.
- `<Fog color near far />`: things fade into `color` between `near` and `far` (scene units from the camera), or
  `density={0.1}` for exponential haze. Use the background colour as the fog colour so distant parts dissolve into it.
- `<VolumetricFog ambient ambientColor />` with `<FogVolume>`s and `volumetric` lights: light you see in the air,
  dust in a sunbeam, smoke, a haze of blood. `<FogVolume name position scale density color absorption scattering
  anisotropy falloff />` is a box of air (`scale` is its size in metres; rotate it as a mesh). `density` 0.02 is a
  faint haze, 0.1 dust, 0.5 smoke; `color` tints what it scatters (a dark red for blood in the air); `absorption`
  darkens it like soot, `scattering` brightens it like dust; `anisotropy` near 1 makes a shaft glow when the camera
  looks toward the light; `falloff` thins it going up from the box's floor (ground fog: a wide, low box with
  `falloff={1}`). Mark the lights that should shaft through it `volumetric` (a SpotLight is the cheap one: a point
  light pays six shadow maps; a DirectionalLight works too); they cast shadows, so a wall with a window in it, a
  blind, or a bust carves the shaft.
  The fog is one medium in a grid along the view: every volume, its falloff and every `<Fire>`'s light go into the
  same cells, so overlapping volumes just mix (smoke rolling into ground fog), a volume with no density shows nothing,
  and the cost is the grid's whatever the frame's size or the number of volumes. The first two volumetric spots and
  the first two volumetric directional lights are hero lights: their shafts are integrated exactly along each
  pixel's ray over their shadow maps, so a blind's bars and a doorway's edge are as sharp as the shadow and never
  step or grain. Glass, image planes and fires (anything see-through) are fogged at their own depth. It has no
  noise to dither, so it encodes small: Mean Streets' fog shots took a third of the bitrate the march's grain did.
  Measured against a 512-step ground truth (sdk/motion/bench/bevy/fog-score.py): the rome film's curia SSIM 0.9999 at
  38 ms a frame (the march 0.9988 at 30), Mean Streets 0.979 at 11 ms (the march 0.894 at 7), the volumetric fixture
  0.966 / 52.7 dB at 7.7 ms (the march 0.751 / 28.8 dB at 3.4). `hero={false}` drops the exact shafts for the grid's
  softer ones (Mean Streets 2.8 ms a frame): use it when no shaft has a hard edge in shot. `slices` (128), `tile`
  (8 px) and `samples` (2 a cell) set the grid: more slices for a deep shot with fog close to the camera and far off.
- `<VolumetricFog mode="march" steps dither resolution />`: Bevy's ray march through each volume instead, as raea
  drew fog before the grid. Only for matching an older piece: it grains or steps where the grid doesn't, composites
  overlapping volumes in row order, and lights a volume's whole box with `ambient`. `steps` (64) is how finely each
  ray is sampled; `dither` turns too few steps into fine grain; `resolution={0.5}` marches a quarter of the pixels.
- `<AmbientOcclusion quality thickness />`: creases, corners, the seam where a wall meets the floor and the ground
  under a statue darken where the room's light can't reach, which seats things in the scene. It darkens indirect light
  only (an `<AmbientLight>`, an `<Environment>`), not a sun or a lamp, so it shows in scenes lit mostly by the room or
  the sky. `quality` "medium" (the default) is right for most shots, "high" for close-ups of carved stone; "ultra" costs
  about 1 ms a frame at 1080p. `thickness` (metres, 0.25) is how thick things are taken to be behind what's seen.
- `<AntiAliasing mode />`: how edges are smoothed. `"msaa"` (4x multisampling) is the default; with
  `<AmbientOcclusion>` it can't run, and `"smaa"` (morphological, high preset) takes over. `"fxaa"` is the cheapest
  and softest, `"none"` leaves stair-stepped edges for a hard pixel look. There's no temporal anti-aliasing: it
  blends in the frames before, so a frame rendered on its own wouldn't equal the same frame in play.
- `<ToneMapping mode exposure />`: how light maps to the screen. `"tony"` is the default (neutral, holds colour);
  `"aces"` is filmic and contrasty (darker, punchier); `"agx"` is soft and holds highlights; `"neutral"`,
  `"reinhard"` and `"none"` are there too. `exposure` is in stops: `+1` twice as bright, `-1` half.
- `<DepthOfField aperture mode maxBlur />`: a lens's depth of field. What's nearer or farther than the camera's
  `focus` (metres; by default the `lookAt` point's distance) blurs. `aperture` is the f-number: 1.4 dreamy, 2.8 (the
  default) cinematic, 8 nearly sharp. Pull focus by animating `focus` on the `<PerspectiveCamera>`: it changes every
  frame, as `fov` does.
- `<Look mode pixelSize theme color lift gain />`: the whole frame in a model look's grain (`bevy/assets`): `"site"`
  (raea.io's six greys, Bayer-dithered) or `"pixel"`, a cell to `pixelSize` pixels. Bloom, fog and depth of field
  happen inside the grain. `color` (0..1, 1 by default) is how much of each cell's hue its grey keeps, so emissive
  colours stay coloured. `lift` 0 keeps black black. Use it instead of a model's own `look` when the world should
  share the model's grain; void's cat in `apps/void/brand/cat3d/cinematic.tsx` is the worked example.
- `<Look mode="paint" />`: the frame as a flat editorial illustration. Every surface is lit by the suns
  (DirectionalLights; point lights, ambient and specular are ignored) in `bands` flat tones: lit is its own colour,
  turned from the sun its colour times `mid` (form shadow), in a cast shadow its colour times `shadow`; 4+ bands step
  the lit side. Cast shadows are cut hard, so they read as shapes: give it one sun and a tight `<Shadows distance
  firstCascade cascades={1} />`. Then every edge wanders `rag` pixels (`ragScale` across; `boil` frames re-seeds it),
  a fixed paper `grain` lies over it, `sky` (one colour or `[top, bottom]`) fills wherever nothing's drawn, and
  `palette` (up to 16 colours) snaps every pixel to the nearest. `bands`, `grain`, `rag` and the seed are keyed per
  frame; the rest is the first frame's, so it's one painting a composition. Light colours: a white surface takes
  exactly `mid`/`shadow` (`shadow="#b4b6bc" mid="#cfa7a2"` is ref-4's cool grey and warm pink).
- `<Look mode="cartoon" />`: paint's cartoon preset: two bands (a fill and one blush shade, `shadow="#f2c7b8"`), a
  thick even outline round silhouettes (`outline` colour, `outlineWidth` px, 6), and the line boiling on twos (`boil`
  2, `rag` 1.5). Lines come where the depth steps by 4% or more. `apps/void/brand/anything/paint/demo.tsx` is the worked
  example of paint (three shots, 16:9 and 9:16).

```tsx
<Canvas3D>
  <PerspectiveCamera position={[8, 10, 8]} lookAt={[0, 1.4, 0]} fov={34} />
  <Look mode="paint" bands={3} shadow="#b4b6bc" mid="#cfa7a2" grain={0.35} rag={2.2} sky="#5b9497" />
  <DirectionalLight position={[-5, 3.2, 6]} intensity={1.2} />
  <Shadows distance={40} firstCascade={30} cascades={1} mapSize={4096} />
  {/* ... */}
</Canvas3D>
```
- `<Grade temperature tint saturation shadows midtones highlights vignette aberration />`: one colour grade over every
  scene, so a film of many shots looks of a piece. `shadows`/`midtones`/`highlights` each take `{saturation, contrast,
  gamma, gain, lift}` (1, 1, 1, 1, 0 keep it); `temperature` warms (+) or cools (-) by about 0.03 a step; `vignette` is
  0..1 or `{intensity, radius, smoothness, color}`; `aberration` 0.002–0.005 is a lens's fringe. Like every effect,
  the first frame's grade holds for the whole piece. `apps/void/brand/rome/film.tsx` is the worked example.

## The lens: fisheye and barrel

- `<Fisheye strength={0..1} fov={120} />`: bends the 3D like a fisheye lens, corner to corner (nothing goes black).
  `strength` 0 is the camera's own picture, pixel for pixel, so ramp it in and out on beats with `animate`. `fov` is
  how wide the camera opens up as the fisheye comes in (degrees; 100-150 for a real fisheye feel), `mode="stereographic"`
  is gentler at the edges.
- `<LensDistortion k1 k2 />`: barrel (`k1` 0.1-0.4) or pincushion (`k1` below 0), zoomed just enough that the corners
  stay filled.
- Both take `edges="black"` (to show where the lens looks past the picture; edge pixels by default), and every number
  animates per frame, unlike the effects above. The lens's vignette and colour fringes are `<Grade>`'s.
- The page (HTML over the 3D: titles, captions, kinetic type) is never bent: the lens is a pass over the 3D only. To
  put words through the lens, make them 3D (`Text3D`, or a picture on an `ImagePlane`).
- Ink outlines (`MeshToonMaterial outline`) are drawn before the lens, so a fisheye fattens them in the middle and thins
  them towards the edges (by about the lens's squeeze there).

## Motion blur

- `<MotionBlur shutterAngle={0.5} samples={4} />`: fast moves smear (0.5 is a film camera's 180-degree shutter; 1 a
  whole frame). It animates per frame, and a frame without the element has none, so `{fast ? <MotionBlur /> : null}`
  blurs only the whip.
- It blurs inside moving parts and whole-frame camera moves; a part's silhouette over a still background, and toon ink
  lines, stay sharp (Bevy's method).
- With it, each frame is drawn after the one before it, so a frame's blur is the same in a render, a seek and play.
  That costs a second draw a frame.

## Why

These are Bevy's own camera effects (Bloom, DistanceFog, VolumetricFog, Tonemapping, Exposure) on the scene's camera. Bloom turns
the camera HDR so bright parts can exceed white before they bloom.

## Common mistakes

- Bloom with nothing bright in the scene: nothing glows. Add emissive parts or raise `exposure`.
- `threshold={0}` with a light background: the whole frame goes milky.
- Fog `far` shorter than the distance to the subject: the subject disappears.
- Animating them: their first frame's values hold for the whole piece (the camera's `focus` and the fog volumes are the exceptions: a volume's place, size, density and colour change every frame). Animate the materials and lights instead.
- Fog volumes that show as boxes, or stair-stepped shafts: that's `mode="march"` (its `ambient` lights each box
  whole; too few `steps` step). Drop the mode: the grid has neither.
- A third shaft with a hard edge: only two spots and two directional lights are hero lights; the rest are lit in the
  grid's cells, softer. Put the shaft that matters on one of the first two.
- Glass in front of a hero shaft: the shaft in front of it is dimmed by the glass's opacity (the hero light is
  integrated to the solid surface behind).
- A `volumetric` light with no `<VolumetricFog>` or outside every `<FogVolume>`: no shaft. The fog volume has to
  contain the air the light crosses, and the camera's view of it.
- `<AmbientOcclusion>` in a scene lit only by a DirectionalLight or spots: nothing changes. It shades indirect light;
  add an AmbientLight or an Environment.
- Expecting `<AntiAliasing mode="msaa" />` next to `<AmbientOcclusion>`: SMAA is used instead (ambient occlusion
  can't multisample). SMAA can't recover sub-pixel detail as MSAA does: very thin wires and far railings shimmer more.
- `aces` making brand colours muddy: check a still against the kit; `tony` keeps colours closer.
- Text on the page expected to bend with a Fisheye: the page stays straight; make it 3D.
- MotionBlur on a part that only turns slowly: nothing shows. It reads on whips, spins and fast passes.

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

## Example: a sunbeam through a high window

```tsx
// a dark stone room; a spot light outside a window high in the back wall makes a dusty shaft over a bust
<Canvas3D>
  <PerspectiveCamera position={[4.4, 1.6, 4.2]} lookAt={[-0.9, 2.9, -1.6]} fov={55} />
  <ToneMapping mode="agx" />
  <VolumetricFog />
  <Mesh shape={manifold.box(10, 7, 0.5).minus(manifold.box(1.2, 1.6, 2).move(-1.4, 0.7, 0))} position={[0, 3.5, -3]}>
    <MeshStandardMaterial color="#5a5248" roughness={0.95} />
  </Mesh>
  <Model src={staticFile("bust.glb")} position={[0, 1.1, 0]} scale={1.6} />
  <SpotLight position={[-2.8, 7.4, -6]} target={[0, 1, 0]} angle={0.2} penumbra={0.15} intensity={2.6} color="#ffe6c4" volumetric />
  <FogVolume name="air" position={[0, 3.5, 1]} scale={[9.5, 7, 7.8]} density={0.1} color="#fff2e0" scattering={0.45} absorption={0.15} anisotropy={0.55} />
</Canvas3D>
```

For violence, the same room in blood: `density={0.16} color="#d0281c" scattering={0.6} absorption={0.12}`, the light a
little stronger and warmer, and `<VolumetricFog ambient={1.2} ambientColor="#6a0a08" />` so the whole
room sits in a red murk (the grid scatters the ambient light by the fog's density, so it thickens with the haze).

## Example: a fisheye beat and a ball whipping past

```tsx
import { AbsoluteFill, Composition, registerRoot, track, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Fisheye, Mesh, MeshToonMaterial, MotionBlur, PerspectiveCamera, Sphere } from "@raea/motion/3d";

const LENS = { strength: track({ 0: 0, 20: 1, 40: 0 }, "in-out-sine") };

function Room() {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "#0b0d14" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.6, 7.5]} lookAt={[0, 1.2, 0]} fov={45} />
        <AmbientLight intensity={0.3} />
        <DirectionalLight position={[4, 6, 5]} intensity={1.6} />
        <Fisheye strength={0} fov={110} animate={LENS} />
        <MotionBlur shutterAngle={1} />
        <Mesh position={[0, -0.15, 0]}><Box args={[10, 0.3, 14]} /><MeshToonMaterial color="#8a6a52" steps={2} /></Mesh>
        <Mesh position={[0, 3, -5]}><Box args={[10, 6, 0.3]} /><MeshToonMaterial color="#454c64" steps={2} /></Mesh>
        <Mesh position={[0.3, 1, 0]}><Sphere args={[0.8]} /><MeshToonMaterial color="#ffd23f" steps={2} outline={{ width: 3 }} /></Mesh>
        <Mesh name="ball" position={[4 - f * 0.15, 0.5, 2]}><Sphere args={[0.4]} /><MeshToonMaterial color="#3aa0ff" steps={2} outline={{ width: 3 }} /></Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="room" component={Room} width={1280} height={720} fps={30} durationInFrames={60} />);
```

## Fire

`<Fire name position scale intensity color seed speed opacity light />` is a fire standing in the scene: a flame drawn
as a volume, its tongues quick at the foot and slow above, curling and tearing off into licks, every pixel a function of the frame (rising 3D noise inside
a tapering flame), so a render of any frame equals its play. It isn't a picture on a card: it has depth, the camera can
circle it, and with an `<AmbientOcclusion>` (which brings a depth pass) a log or a wall inside its box hides the flame
behind it.

- `position` is the flame's foot: stand it on its fuel (the top of a pyre's lowest logs, the bowl of a brazier).
- `scale` is its height in metres (half as wide), or `[width, height, depth]`. Keep it fixed; grow a fire by its
  `intensity`, since its tongues' speed depends on its height.
- `intensity` is how bright its core is, in a material's emissiveIntensity units. At night or indoors 6-15; a pyre
  roaring in the dark 20-35. Its hottest few percent is a white-yellow core well past 1, which a `<Bloom>` blooms.
  Too high and the whole flame goes to the core and reads white; small fires want less (a pit fire 4-6).
- `opacity` (1 by default) is how much of what's behind it the flame's body hides. Fire is light added over what's
  behind it, so it reads best against the dark (night sky, shadow, a dark doorway) and pales in front of a brightly
  lit surface; `opacity={1.5}` to `{3}` gives it a darker body there.
- `color` is the body's colour ("#ff7a1f"); the edges go deep red, the core yellow. `seed` makes fires differ;
  `speed` (1) scales how fast the tongues rise; `time` (seconds) overrides the Sequence's.
- `light` (1 by default; 0 for none) is a flickering PointLight at a third of its height, its flicker a function of
  the time. Turn it off when the scene has its own firelight. Mind the distance to what's behind the fire: a light of
  1 gives 8000 lux at 3 m but about 110,000 at 0.8 m, which turns a wall just behind the flame white, and an additive
  flame over white disappears. For a brazier against a wall, put the light on the camera's side of the flame at a few
  hundredths (Rome's street altar: 0.05, 0.6 m in front of the bowl).
- Put embers (small emissive spheres on seeded paths) and smoke (a `<FogVolume>` starting just above the flame's tips,
  not around it: fog in front of the flame hides it) round it yourself.

```tsx
<Fire name="pyre" position={[0, 0.2, 24]} scale={[2.6, 5, 2]} intensity={10 + 25 * burn} speed={0.9} opacity={2} light={0} />
```

Cost: a fire is a box whose pixels each march 32 samples (`steps`), so it costs by its area on screen: about 1 ms a
frame at 1920x1080 for a fire filling a third of the frame (the fixture's pyre, measured as the difference between
48 and 240 steps), far less for one in the distance. For several big fires in play, lower `steps` (16-24).

