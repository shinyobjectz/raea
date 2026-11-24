// A Cornell box (docs/bake.feature BK0): white floor, ceiling and back, a red left wall and a green right one, two
// white blocks, lit only by an emissive panel in the ceiling. Nothing here is lit live: every bit of light on its
// walls is the panel's, directly or bounced, so it measures a bake's emissives and colour bleeding with no sun or sky.
import { LinearRgba } from "raea/color";
import { ToneMapping } from "raea/3d";
import { AmbientLight } from "raea/light";
import { StandardMaterial } from "raea/pbr";
import { Cuboid, Mesh3d } from "raea/mesh";
import { Quat, Transform } from "raea/transform";
import { Camera3d } from "raea/camera";

export function Look() {
  return (
    <>
      <Camera3d transform={Transform.fromXyz(0, 1, 3.4).lookingAt([0, 1, 0])} projection={{ Perspective: { fov: (40 * Math.PI) / 180 } }} />
      <AmbientLight brightness={0} />
      <ToneMapping mode="agx" exposure={0} />
    </>
  );
}

const WHITE = "#bdbdb6";
type Box = [string, [number, number, number], [number, number, number], string, number?];
// name, centre, half size, colour, turn about y; the walls 0.1 thick and overlapping at the corners, so no light leaks
const BOXES: Box[] = [
  ["floor", [0, -0.05, 0], [1.1, 0.05, 1.1], WHITE],
  ["ceiling", [0, 2.05, 0], [1.1, 0.05, 1.1], WHITE],
  ["back", [0, 1, -1.05], [1.1, 1.1, 0.05], WHITE],
  ["left", [-1.05, 1, 0], [0.05, 1.1, 1.1], "#a3261c"],
  ["right", [1.05, 1, 0], [0.05, 1.1, 1.1], "#2f8a2a"],
  ["tall", [-0.35, 0.6, -0.3], [0.3, 0.6, 0.3], WHITE, 0.3],
  ["short", [0.38, 0.3, 0.32], [0.3, 0.3, 0.3], WHITE, -0.3],
];

export function Parts() {
  return (
    <>
      {BOXES.map(([name, [x, y, z], half, color, turn]) => (
        <Mesh3d key={name} name={name} transform={Transform.fromXyz(x, y, z).withRotation(Quat.fromRotationY(turn ?? 0))}>
          <Cuboid half_size={half} />
          <StandardMaterial base_color={color} perceptual_roughness={1} />
        </Mesh3d>
      ))}
      <Mesh3d name="panel" transform={Transform.fromXyz(0, 1.995, 0)}>
        <Cuboid half_size={[0.25, 0.004, 0.25]} />
        <StandardMaterial base_color="#ffffff" emissive={new LinearRgba(4200, 3000, 1000, 1)} />
      </Mesh3d>
    </>
  );
}
