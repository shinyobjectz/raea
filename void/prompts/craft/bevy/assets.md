# Assets: models and pictures in space

## What to do

- Files live in the space's media folder and are named with `staticFile("path/in/media.ext")` (from
  `@raea/motion`). Never a URL, never an absolute path.
- A glTF model: `<Model src={staticFile("chair.glb")} position rotation scale />`. `.glb` (one file) is best; a
  `.gltf` needs its `.bin` and textures beside it. Models come at their author's scale and origin: render one still
  first, then set `scale` and `position` so it sits on the floor at a sensible size.
- Its animation: `animation="Run"` (the clip's name in the file, or its index `0`) poses it at the Sequence's time;
  `time` and `loop={false}` as in `bevy/animation`. A wrong clip name fails the render and lists the clips the file
  has: read the message and use one of them.
- `scene={1}` picks another scene in a file that has several.
- A picture in the scene: `<ImagePlane src={staticFile("poster.png")} width={2} position rotation />` (PNG or JPEG):
  an unlit card, its height from the picture's shape unless you give `height`. For a picture on a solid's side, use
  `map` on its material (`bevy/materials`).
- Models are loaded once before the first frame is drawn; there's no loading state to handle.
- raea generates no imagery: models and pictures come from the brand, the person, or libraries they've given you.

## Why

The engine loads models with Bevy's glTF loader (materials, textures, skins and clips included) and waits for them,
so the first frame and every frame after show the whole model, in renders and in play alike.

## Common mistakes

- `src="models/chair.glb"` without `staticFile`, or a web URL: use `staticFile`.
- A model 100 times too big or sunk into the floor: set `scale` and `position` from a still.
- Guessing a clip's name: read the render's error, which lists them.
- An ImagePlane facing away from the camera: it faces +z by default; rotate it with `rotation={[0, angle, 0]}`.

## Example: a model playing its clip beside a poster

This one expects `media/model.glb` (with a clip named "Spin") and `media/poster.png` in the space.

```tsx
import { AbsoluteFill, Composition, registerRoot, staticFile } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, ImagePlane, Mesh, MeshStandardMaterial, Model, PerspectiveCamera } from "@raea/motion/3d";

function Showcase() {
  return (
    <AbsoluteFill style={{ background: "#e9e4da" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.8, 6]} lookAt={[0, 0.7, 0]} fov={40} />
        <AmbientLight intensity={0.5} />
        <DirectionalLight position={[3, 6, 4]} intensity={1.2} />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[10, 0.1, 10]} />
          <MeshStandardMaterial color="#cfc8ba" roughness={0.9} />
        </Mesh>
        <Model src={staticFile("model.glb")} animation="Spin" position={[-1.2, 0, 0]} />
        <ImagePlane src={staticFile("poster.png")} width={2} position={[1.4, 1, -0.5]} rotation={[0, -0.3, 0]} />
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="showcase" component={Showcase} width={1280} height={720} fps={30} durationInFrames={60} />);
```
