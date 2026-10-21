# Materials

## What to do

- Each Mesh holds one material (toon shading: see "Toon" below):
  - `MeshStandardMaterial {color, roughness, metalness, emissive, emissiveIntensity, opacity, transparent, map,
    normalMap, roughnessMap, metalnessMap, metallicRoughnessMap, aoMap, repeat}`: the
    default, physically based. `roughness` 0 is a mirror finish, 1 is chalk; `metalness` 1 is bare metal (its colour
    tints reflections), 0 is plastic, paint, wood.
  - `MeshPhysicalMaterial`: the same plus `clearcoat` (0-1, a lacquer layer: car paint, a phone's glass back),
    `clearcoatRoughness` and `reflectance`.
  - `MeshBasicMaterial {color, opacity, map}`: unlit, a flat colour no light changes (a screen, a sign, a backdrop).
- Glow: `emissive` (hex) with `emissiveIntensity` 1-3 makes a surface give off light-coloured glow; with `<Bloom>`
  (`bevy/post`), 3-10 makes it bloom like a neon tube. Emissive surfaces don't light their neighbours.
- See-through: `transparent opacity={0.3-0.6}` for glass or a fade. Keep `roughness` low for glass.
- A picture on a surface: `map={staticFile("label.png")}` (PNG or JPEG in the media folder), or written inline as a
  data: URL: an SVG label (`data:image/svg+xml;utf8,<svg ...>`, with `#` written `%23`) or a PNG
  (`data:image/png;base64,...`). On a Box each side gets the whole picture once; for a flat picture on its own use
  `ImagePlane` (`bevy/assets`), whose `src` takes the same.
- Real surfaces (stone, brick, marble, wood, plaster): a PBR texture set, one file per map, on a Standard or Physical
  material. Poly Haven's CC0 sets name them `_diff` (`map`), `_nor_gl` (`normalMap`), `_rough` (`roughnessMap`), `_ao`
  (`aoMap`) and `_arm` (ambient occlusion, roughness, metalness packed: give it as both `metallicRoughnessMap` and
  `aoMap`). The normal map is what makes bricks and veins catch a raking light; the roughness map makes polished
  patches shine and worn ones go matt. With a roughness or metalness map, `roughness` and `metalness` scale it (1 by
  default). Use the 1k JPGs; 2k only for a surface that fills the frame.
- `repeat={[u, v]}` tiles every map of the material: each face of a Box or Plane is covered once, times repeat. A
  12 m floor with `repeat={[6, 6]}` shows a 2 m texture six times each way; pick repeat so the texture's real size
  (Poly Haven gives it, usually 1-4 m) matches the face's. Faces of different sizes on one Box share one repeat, so
  give a plinth a material whose texture reads at both sizes, or build it from separate meshes.
- Animate `color` (a hex string per frame), `opacity`, `emissiveIntensity`, `roughness` and `metalness` freely: each
  frame's value is drawn. To fade a colour, mix the two hexes yourself from the frame.

## Toon (cel) shading and ink outlines

- `MeshToonMaterial {color, steps, shadowColor, rim, highlight, outline}`: flat bands of colour instead of smooth
  shading, the look of a cartoon. `steps` is how many bands (2 is classic cel, 3-4 softer); the top band is `color`,
  the bottom `shadowColor` (by default a darker, slightly violet-shifted `color`, never grey), the ones between mixed.
  The bands follow the scene's own lights and shadows; lights pick the band, they don't brighten it, so a sun of
  intensity 2 doesn't wash the colour out. More AmbientLight flattens the bands (0.2-0.4 for a strong terminator).
- For the palette exactly as written, add `<ToneMapping mode="none" />` (`bevy/post`): the default tone mapping mutes
  saturated colours (a `#3aa0ff` lit band shows as `#488fd3`), on toon surfaces as on every other.
- One part toon in a standard scene is this; the whole frame as a cartoon (every part, a boiling line) is `<Look
  mode="cartoon">` (`bevy/post`).
- `outline={{ width: 3 }}` draws an ink line round the silhouette and along creases, `width` in pixels of the
  composition (the same near and far); `color` defaults to a very dark version of the surface colour. 2-4 px at
  1080p reads as a cartoon line.
- `rim={{ intensity: 0.6, width: 0.25 }}` a crisp light band at the edge (white unless `color`); `highlight={{ size:
  0.2 }}` a hard specular spot. `true` takes the defaults.
- An explicit ramp: `bands={[{ at: 0, color: "#3b2a5c" }, { at: 0.35 }, { at: 0.8, color: "#fff4c2" }]}`, darkest
  first; `at` is how much of the light a point faces (0-1) where the band starts.
- A glTF character: put `<MeshToonMaterial outline={{ width: 3 }} />` inside the `<Model>`: every material of the file
  goes toon with its own colours and textures (rigged and animated models included). Several, each with `material=
  "Skin"` (the glTF material's name from Blender), give parts their own; one without a name covers the rest.
- Animate `color`, `shadowColor`, `emissiveIntensity`, `opacity`, `rimIntensity`, `highlightIntensity` and
  `outlineWidth` (an outline popping in) from the frame or with `animate`; the bands, rim, highlight and outline
  themselves are the first frame's.

```tsx
// cel-shaded: a Manifold banana with two bands, a rim and an ink line, and a rigged glTF robot gone toon (its own
// colours kept), waving
import { AbsoluteFill, Composition, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Canvas3D, DirectionalLight, Mesh, MeshToonMaterial, Model, PerspectiveCamera, ToneMapping, manifold } from "@raea/motion/3d";

// a banana: a smoothed sphere stretched long, bent along an arc and pinched at the ends
const BANANA = manifold.sphere(1).smooth({ refine: 4 }).scale(1.1, 0.27, 0.27).refineToLength(0.06).warp((x, y, z) => {
  const t = x / 1.1;
  const pinch = 1 - 0.35 * t * t * t * t;
  return [x, y * pinch + 0.5 * t * t, z * pinch];
});

function Toon() {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "#fdf3dc" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.6, 7]} lookAt={[0, 1, 0]} fov={40} />
        <AmbientLight intensity={0.3} />
        <DirectionalLight position={[3, 5, 4]} intensity={1.2} />
        <ToneMapping mode="none" />
        <Mesh shape={BANANA} position={[-1.6, 1, 0]} rotation={[0.2, f * 0.05, 0.3]}>
          <MeshToonMaterial color="#ffd23f" steps={2} rim={{ intensity: 0.7, color: "#fff6c8", width: 0.22 }} outline={{ width: 3 }} />
        </Mesh>
        <Model src={staticFile("models/robot-expressive/RobotExpressive.glb")} animation="Wave" position={[1.4, 0, 0]} scale={0.45} rotation={[0, -0.5, 0]}>
          <MeshToonMaterial steps={2} outline={{ width: 2.5 }} />
        </Model>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="toon" component={Toon} width={1280} height={720} fps={30} durationInFrames={60} />);
```

## Decals: pictures pressed onto surfaces

- `<Decal src={staticFile("label.png")} position={...} normal={[0, 0, 1]} size={[w, h]} />` lays a picture onto
  whatever surface it crosses: a label on a box, a coffee ring on a counter, a patch of tiles behind a stove, a sticker
  on a laptop. `normal` is the way the surface faces (out of it). It defaults to `[0, 1, 0]`, a floor or counter. On a
  wall the picture stands upright; `spin` turns it about the normal. On a room's wall, use `{...onSurface(ROOM,
  "right", u, y)}` (`rooms.md`).
- The picture's clear parts show the surface through. `color` tints it and `opacity` fades it; both animate, so a
  stain can darken or a label fade in. `roughness`, `metalness` and `normalMap` give it a finish.
- A decal moves like any part: put it inside a moving Group, or key its position, and it follows. This works in
  renders and in games.
- A decal lays itself on *everything* its box crosses. Keep it clear of edges you don't want it on: a backsplash
  should stop short of the counter, the hood and the side of the cabinet beside it, or it smears onto them.
  Glossy decals (`roughness` under about 0.4) reflect the sky and look like glass. Use 0.5 or more for tile and paper.

## Re-covering a model's materials

- `<Model src={CHAIR}><MeshStandardMaterial material="Fabric" color="#7a3b2e" /></Model>` changes one glTF material
  (by its name). Without `material`, it changes all of them. Only the props you give change; the rest of the model's
  material (its maps, its finish) is kept. `map={staticFile(...)}` swaps the picture; `map={null}` takes it off (wood
  painted over).
- `<Refinish materials={[{ material: "MI_KitchenWood", color: "#7f9479", map: null, roughness: 0.55 }]}>...</Refinish>`
  does the same for every Model inside it: a run of cabinets with its doors and drawers, a whole set of chairs. A
  model's own MeshStandardMaterial children go first.
- Read a glb's material names from its JSON chunk (`materials[].name`) or from the pack's docs.

## Why

Materials become Bevy's StandardMaterial on each mesh (a toon one extends it with its own lighting, and its outline is
the mesh drawn a second time, inside out and pushed out a few pixels). Its values are columns keyed per frame, so a
colour sweep or a glow pulse plays exactly as it renders.

## Common mistakes

- `metalness` or `roughness` outside 0-1, or `opacity` above 1: they throw.
- Faking toon shading by splitting a mesh along the light or by placing black copies behind parts: use
  `MeshToonMaterial` with `outline` instead.
- A toon `outline` on a see-through (`opacity` < 1) part: the line fades with it, but the part shows its own hull
  through itself. Turn the outline off (`outlineWidth` 0) while it's faded.
- A colour as a CSS variable or a Tailwind class: hex only (names like "white" and `rgb()` are converted).
- Expecting an emissive object to light the room: add a PointLight at it for that.
- Everything at `metalness={1}` with no `<Environment>`: metal shows only what it reflects, and without a panorama
  (`bevy/lights`) that's the lights' small highlights, so fully metallic surfaces look dark. Add an Environment for
  real metal, or keep `metalness` at 0.2-0.6 with low roughness.
- `map` (or any map) on a Sphere or a Torus: the picture is projected box-wise and smears on curves. Use maps on
  boxes, planes and Manifold solids with flat faces.
- A `_nor_dx` normal map (DirectX layout, green down): bumps light from the wrong side. Use `_nor_gl`.
- A tiled floor with no `repeat`: one 1k texture stretched over 12 m reads as a blurry giant tile. Tile it.
- Forgetting the texture's real size: bricks twice the height of a door. Count the texture's courses or tiles and set
  repeat from the face's size.

## Example: a marble floor, a brick wall and a plinth

```tsx
// media/: Poly Haven CC0 1k JPGs (marble_01_*, castle_brick_07_*)
const marble = {
  map: staticFile("marble_01_diff_1k.jpg"),
  normalMap: staticFile("marble_01_nor_gl_1k.jpg"),
  roughnessMap: staticFile("marble_01_rough_1k.jpg"),
  aoMap: staticFile("marble_01_ao_1k.jpg"),
};
const brick = {
  map: staticFile("castle_brick_07_diff_1k.jpg"),
  normalMap: staticFile("castle_brick_07_nor_gl_1k.jpg"),
  metallicRoughnessMap: staticFile("castle_brick_07_arm_1k.jpg"),
  aoMap: staticFile("castle_brick_07_arm_1k.jpg"),
};

<Canvas3D>
  <PerspectiveCamera position={[0, 1.7, 5.5]} lookAt={[0, 0.9, -1]} fov={50} />
  <AmbientLight intensity={0.15} />
  <SpotLight position={[sweep, 2.6, 0.6]} target={[0, 1.2, -3]} angle={0.7} penumbra={0.5} intensity={3} castShadow />
  <Mesh rotation={[-Math.PI / 2, 0, 0]}>
    <Plane args={[12, 12]} />
    <MeshStandardMaterial {...marble} repeat={[6, 6]} />
  </Mesh>
  <Mesh position={[0, 2, -3]}>
    <Box args={[12, 4, 0.4]} />
    <MeshStandardMaterial {...brick} repeat={[6, 2]} />
  </Mesh>
  <Mesh position={[0, 0.6, -0.5]}>
    <Box args={[0.9, 1.2, 0.9]} />
    <MeshPhysicalMaterial {...marble} color="#f0ece4" roughness={0.5} clearcoat={0.3} />
  </Mesh>
</Canvas3D>
```

`color` tints the base map (white keeps it); the plinth's `roughness={0.5}` halves its roughness map, a polished
stone. The whole fixture is `sdk/motion/bench/bevy/texmaps/edit.tsx` in raea.

## Example: four finishes side by side

```tsx
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Bloom, Canvas3D, DirectionalLight, Mesh, MeshBasicMaterial, MeshPhysicalMaterial, MeshStandardMaterial, PerspectiveCamera, Sphere, Torus } from "@raea/motion/3d";

function Finishes() {
  const f = useCurrentFrame();
  const pulse = 3 + 2 * Math.sin(f * 0.2);
  return (
    <AbsoluteFill style={{ background: "#111318" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 0.8, 6]} lookAt={[0, 0, 0]} fov={40} />
        <AmbientLight intensity={0.3} />
        <DirectionalLight position={[2, 4, 5]} intensity={1.3} />
        <Bloom intensity={1.2} threshold={0.9} />
        <Mesh position={[-2.4, 0, 0]}>
          <Sphere args={[0.55]} />
          <MeshPhysicalMaterial color="#8a0f1e" metalness={0.5} roughness={0.45} clearcoat={1} clearcoatRoughness={0.05} />
        </Mesh>
        <Mesh position={[-0.8, 0, 0]}>
          <Sphere args={[0.55]} />
          <MeshStandardMaterial color="#cfe8ff" roughness={0.05} transparent opacity={0.35} />
        </Mesh>
        <Mesh position={[0.8, 0, 0]}>
          <Torus args={[0.45, 0.12]} />
          <MeshStandardMaterial color="#202020" emissive="#ff3d7f" emissiveIntensity={pulse} />
        </Mesh>
        <Mesh position={[2.4, 0, 0]}>
          <Sphere args={[0.55]} />
          <MeshBasicMaterial color="#f2c14e" />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="finishes" component={Finishes} width={1280} height={720} fps={30} durationInFrames={60} />);
```
