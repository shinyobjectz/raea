// A room lit through one window (docs/bake.feature BK0): a closed 4 x 3 x 5 m room, the window in its right wall, the
// sun through it in a patch on the floor and the sky through it, a sofa and a low table. Almost all of what the camera
// sees is bounce light, the case lightmaps exist for. The wall is four boxes around the opening, not a boolean.
import { staticFile } from "raea";
import { Environment, ToneMapping, AmbientOcclusion, Shadows } from "raea/3d";
import { StandardMaterial } from "raea/pbr";
import { Cuboid, Mesh3d } from "raea/mesh";
import { DirectionalLight } from "raea/light";
import { Transform } from "raea/transform";
import { Camera3d } from "raea/camera";

export const SKY = "kloofendal_48d_partly_cloudy_puresky_1k.hdr";

export function Look() {
  return (
    <>
      <Camera3d transform={Transform.fromXyz(-1.6, 1.5, 2.2).lookingAt([1.2, 0.9, -1.2])} projection={{ Perspective: { fov: (60 * Math.PI) / 180 } }} />
      <Environment src={staticFile(SKY)} intensity={0.25} rotation={0.6} background />
      <DirectionalLight transform={Transform.fromXyz(8, 6, -1).lookingAt([1, 0, -0.5])} illuminance={45000} color="#fff1e0" shadow_maps_enabled />
      <Shadows distance={12} firstCascade={4} mapSize={4096} />
      <AmbientOcclusion quality="high" />
      <ToneMapping mode="agx" exposure={1.5} />
    </>
  );
}

const WALL = "#c8c2b6";
type Box = [string, [number, number, number], [number, number, number], string, number];
// name, centre, half size, colour, roughness. The room's inside is x -2..2, y 0..3, z -2.5..2.5; the window in the
// x = 2 wall, 1.6 wide (z -1.6..0) and 1.6 high (y 0.9..2.5)
const BOXES: Box[] = [
  ["floor", [0, -0.1, 0], [2.2, 0.1, 2.7], "#8a6a4a", 0.8],
  ["ceiling", [0, 3.1, 0], [2.2, 0.1, 2.7], "#e0ddd6", 1],
  ["left", [-2.1, 1.5, 0], [0.1, 1.6, 2.7], WALL, 1],
  ["back", [0, 1.5, -2.6], [2.2, 1.6, 0.1], WALL, 1],
  ["front", [0, 1.5, 2.6], [2.2, 1.6, 0.1], WALL, 1],
  ["sill", [2.1, 0.45, 0], [0.1, 0.45, 2.7], WALL, 1],
  ["lintel", [2.1, 2.75, 0], [0.1, 0.25, 2.7], WALL, 1],
  ["pier-back", [2.1, 1.7, -2.05], [0.1, 0.8, 0.45], WALL, 1],
  ["pier-front", [2.1, 1.7, 1.3], [0.1, 0.8, 1.3], WALL, 1],
  ["sofa", [-1.5, 0.35, -0.6], [0.45, 0.35, 1.1], "#3d5a73", 0.9],
  ["sofa-back", [-1.85, 0.75, -0.6], [0.12, 0.35, 1.1], "#3d5a73", 0.9],
  ["table", [0, 0.2, -0.5], [0.5, 0.2, 0.35], "#6b4a30", 0.6],
];

export function Parts() {
  return (
    <>
      {BOXES.map(([name, [x, y, z], half, color, rough]) => (
        <Mesh3d key={name} name={name} transform={Transform.fromXyz(x, y, z)}>
          <Cuboid half_size={half} />
          <StandardMaterial base_color={color} perceptual_roughness={rough} />
        </Mesh3d>
      ))}
    </>
  );
}
