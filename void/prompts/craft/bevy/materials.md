# Materials

## What to do

- Each Mesh holds one material:
  - `MeshStandardMaterial {color, roughness, metalness, emissive, emissiveIntensity, opacity, transparent, map}`: the
    default, physically based. `roughness` 0 is a mirror finish, 1 is chalk; `metalness` 1 is bare metal (its colour
    tints reflections), 0 is plastic, paint, wood.
  - `MeshPhysicalMaterial`: the same plus `clearcoat` (0-1, a lacquer layer: car paint, a phone's glass back),
    `clearcoatRoughness` and `reflectance`.
  - `MeshBasicMaterial {color, opacity, map}`: unlit, a flat colour no light changes (a screen, a sign, a backdrop).
- Glow: `emissive` (hex) with `emissiveIntensity` 1-3 makes a surface give off light-coloured glow; with `<Bloom>`
  (`bevy/post`), 3-10 makes it bloom like a neon tube. Emissive surfaces don't light their neighbours.
- See-through: `transparent opacity={0.3-0.6}` for glass or a fade. Keep `roughness` low for glass.
- A picture on a surface: `map={staticFile("label.png")}` (PNG or JPEG in the media folder). On a Box each side gets
  the whole picture once; for a flat picture on its own use `ImagePlane` (`bevy/assets`).
- Animate `color` (a hex string per frame), `opacity`, `emissiveIntensity`, `roughness` and `metalness` freely: each
  frame's value is drawn. To fade a colour, mix the two hexes yourself from the frame.

## Why

Materials become Bevy's StandardMaterial on each mesh. Its values are columns keyed per frame, so a colour sweep or
a glow pulse plays exactly as it renders.

## Common mistakes

- `metalness` or `roughness` outside 0-1, or `opacity` above 1: they throw.
- A colour as a CSS variable or a Tailwind class: hex only (names like "white" and `rgb()` are converted).
- Expecting an emissive object to light the room: add a PointLight at it for that.
- Everything at `metalness={1}`: metal needs something to reflect, and our scenes have no environment map yet
  (`bevy/limits`), so fully metallic surfaces look dark. Use 0.2-0.6 with low roughness for a polished look.
- `map` on a Sphere or a Torus: the picture is projected box-wise and smears on curves. Use it on boxes and planes.

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
