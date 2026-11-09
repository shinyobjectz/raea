// Bevy v0.20.0 examples/2d/rotation.rs in raea's TSX (docs/control.feature FC2): the ship
// (textures/simplespace/ship_C.png) at the origin, which moves only under the arrow keys, so it stays there; two
// enemy_A sprites at (-300, 0) and (0, -160) snapped each fixed update to face it (Quat::from_rotation_arc from +Y), and
// two enemy_B at (300, 0) and (0, 160) turning towards it at 45 and 90 degrees a second; "Up Arrow: Move Forward\nLeft
// / Right Arrow: Turn" 12 px from the top left. FixedUpdate at 60 Hz under the harness's 60 fps clock runs once each
// frame after the first, its delta 16_666_667 ns as f32 seconds; the turns are glam's in f32.
import { AbsoluteFill, Composition, registerRoot, staticFile, useCurrentFrame } from "raea";
import { Canvas3D, Sprite } from "raea/3d";
import { Quat } from "raea/transform";
import { Text, px } from "raea/ui";

const f = Math.fround;
const DT = f(16_666_667e-9);
const EPSILON = 1.1920929e-7;
type V2 = [number, number];

const mul = (a: Quat, b: Quat): Quat => Quat.mul(a, b).map(f) as Quat;
const rotZ = (a: number): Quat => [0, 0, f(Math.sin(f(a * 0.5))), f(Math.cos(f(a * 0.5)))];
/** glam's Quat * Vec3 for a vector in the plane, its x and y */
function turn(q: Quat, v: V2): V2 {
  const [x, y, z, w] = q;
  const b2 = f(f(f(x * x) + f(y * y)) + f(z * z));
  const s = f(f(w * w) - b2), d = f(f(f(v[0] * x) + f(v[1] * y)) * 2), w2 = f(w * 2);
  // rhs * s + b * d + b.cross(rhs) * w2, rhs = (v.x, v.y, 0): b x rhs = (-z v.y, z v.x, x v.y - y v.x)
  return [f(f(f(v[0] * s) + f(x * d)) + f(f(-z * v[1]) * w2)), f(f(f(v[1] * s) + f(y * d)) + f(f(z * v[0]) * w2))];
}
const norm = (v: V2): V2 => {
  const l = f(Math.sqrt(f(f(v[0] * v[0]) + f(v[1] * v[1]))));
  return [f(v[0] / l), f(v[1] / l)];
};
const dot = (a: V2, b: V2) => f(f(a[0] * b[0]) + f(a[1] * b[1]));

/** snap_to_player_system: Quat::from_rotation_arc(Vec3::Y, towards the origin) */
function snapped(at: V2): Quat {
  const to = norm([0 - at[0], 0 - at[1]]);
  const d = to[1];
  if (d > 1 - 2 * EPSILON) return [0, 0, 0, 1];
  // Y x to = (0, 0, -to.x), w = 1 + dot, normalised
  const q: Quat = [0, 0, f(-to[0]), f(1 + d)];
  const l = f(Math.sqrt(f(f(q[2] * q[2]) + f(q[3] * q[3]))));
  return [0, 0, f(q[2] / l), f(q[3] / l)];
}

/** rotate_to_player_system over `steps` fixed updates, for an enemy at `at` turning `speed` radians a second */
function turned(at: V2, speed: number, steps: number): Quat {
  let q: Quat = [0, 0, 0, 1];
  // player - enemy, as Bevy subtracts: 0 - 0 is +0 (-at's -0 flipped copysign for the enemy straight above, which
  // then turned the other way round)
  const to = norm([0 - at[0], 0 - at[1]]);
  for (let n = 0; n < steps; n++) {
    const fwd = dot(turn(q, [0, 1]), to);
    if (Math.abs(f(fwd - 1)) < EPSILON) continue;
    const r = dot(turn(q, [1, 0]), to);
    // -f32::copysign(1, r): -0 counts as negative
    const sign = r < 0 || Object.is(r, -0) ? 1 : -1;
    const max = f(Math.acos(Math.min(1, Math.max(-1, fwd))));
    q = mul(rotZ(f(sign * Math.min(f(speed * DT), max))), q);
  }
  return q;
}

const rad = (deg: number) => f((deg * Math.PI) / 180);

function Shot() {
  const steps = useCurrentFrame();
  const at = (p: V2) => ({ x: 640 + p[0], y: 360 - p[1] });
  return (
    <AbsoluteFill>
      <Canvas3D layered clearColor="rgb(43, 44, 47)">
        <Sprite src={staticFile("textures/simplespace/ship_C.png")} {...at([0, 0])} />
        {([[-300, 0], [0, -160]] as V2[]).map((p) => <Sprite key={`a${p}`} src={staticFile("textures/simplespace/enemy_A.png")} {...at(p)} quat={steps === 0 ? [0, 0, 0, 1] : snapped(p)} />)}
        {([[[300, 0], 45], [[0, 160], 90]] as [V2, number][]).map(([p, deg]) => <Sprite key={`b${p}`} src={staticFile("textures/simplespace/enemy_B.png")} {...at(p)} quat={turned(p, rad(deg), steps)} />)}
      </Canvas3D>
      <Text position_type="Absolute" top={px(12)} left={px(12)}>{"Up Arrow: Move Forward\nLeft / Right Arrow: Turn"}</Text>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="main" component={Shot} durationInFrames={181} fps={60} width={1280} height={720} />);
