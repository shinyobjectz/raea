// SOLIDS: Manifold's TypeScript SDK in one 20 s 3D timeline, the guard that it keeps rendering
// (sdk/react/bench/timelines/check.sh solids; CI's timelines gate). Nine solids, each built with one group of operations,
// turn on a 3 x 3 grid: it bakes, plays from its bake with no JS, and renders, and the three must agree (SSIM >= 0.999).
//   1 constructors     roundedBox, sphere, cylinder, cone, torus, text3d
//   2 booleans         union, minus, intersect, and the batches manifold.union / difference / intersection
//   3 transforms       move, rotate, scale, mirror, transform (3x4)
//   4 cutting          trim, splitByPlane, split, decompose, compose
//   5 surfaces         refine, refineToLength, smooth, simplify
//   6 hull, Minkowski  hull of solids, hull of points, minkowski
//   7 2D               crossSection booleans, offset, hull, extrude with twist, revolve
//   8 from 3D to 2D    project and crossSection(z), offset and extruded again
//   9 meshes          levelSet, warp, fromMesh (solids written under their hash, drawn from obj(...))
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, crossSection, manifold, outline, type Solid } from "@raea/motion/3d";

const W = 1280;
const H = 720;
const FPS = 30;

function constructors(): Solid {
  const base = manifold.roundedBox(1.4, 0.2, 1.4, 0.06).move(0, -0.55, 0);
  const ball = manifold.sphere(0.32).move(-0.4, -0.15, -0.35);
  const post = manifold.cylinder(0.12, 0.7).rotate(90, 0, 0).move(0.4, -0.1, -0.35);
  const cone = manifold.cone(0.25, 0, 0.6).rotate(-90, 0, 0).move(-0.4, -0.15, 0.35);
  const ring = manifold.torus(0.22, 0.07).rotate(90, 0, 0).move(0.4, -0.38, 0.35);
  const word = manifold.text3d("RAEA", 0.32, 0.12).move(0, 0.35, 0);
  return base.union(ball, post, cone, ring, word);
}

function booleans(): Solid {
  const cube = manifold.box(1, 1, 1);
  const holes = manifold.difference(cube, manifold.cylinder(0.3, 1.2), manifold.cylinder(0.3, 1.2).rotate(90, 0, 0), manifold.cylinder(0.3, 1.2).rotate(0, 90, 0));
  const rounded = holes.intersect(manifold.sphere(0.68));
  return manifold.union(rounded, manifold.sphere(0.18).move(0, 0.62, 0));
}

function transforms(): Solid {
  const wedge = manifold.box(0.5, 0.25, 0.25).move(0.45, 0, 0).rotate(0, 0, 25).scale(1, 1.4, 1);
  const pair = wedge.union(wedge.mirror(1, 0, 0));
  // a shear by its 3x4 matrix, column by column
  const sheared = manifold.box(0.3, 0.8, 0.3).transform([1, 0, 0, 0.35, 1, 0, 0, 0, 1, 0, 0, 0]);
  return pair.union(sheared.move(0, -0.2, 0));
}

function cutting(): Solid {
  const ball = manifold.sphere(0.6);
  const [top, bottom] = ball.splitByPlane(0, 1, 0, 0.1);
  const [core, shell] = manifold.box(0.8, 0.8, 0.8).split(manifold.sphere(0.5));
  const pieces = shell.trim(0, 0, 1, 0).decompose();
  return manifold.compose(top.move(0, 0.25, 0), bottom.minus(manifold.box(2, 2, 2).move(0, -1.4, 0)).trim(1, 0, 0, -0.5).move(0, -0.05, 0), core.scale(0.3).move(0, 0.85, 0), ...pieces.slice(0, 1).map((p) => p.scale(0.25).move(0, -0.8, 0)));
}

function surfaces(): Solid {
  const blob = manifold.box(0.8, 0.8, 0.8).smooth({ angle: 120, refine: 4 });
  const facets = manifold.box(0.5, 0.5, 0.5).refine(2).refineToLength(0.2).simplify(0.001).move(0, 0.7, 0);
  return blob.union(facets);
}

function hulls(): Solid {
  const capsule = manifold.hull(manifold.sphere(0.25).move(-0.4, 0, 0), manifold.sphere(0.25).move(0.4, 0, 0));
  const gem = manifold.hull([[0, 0.8, 0], [0.3, 0.45, 0.3], [-0.3, 0.45, 0.3], [0.3, 0.45, -0.3], [-0.3, 0.45, -0.3], [0, 0.25, 0]]);
  const soft = manifold.box(0.5, 0.15, 0.5).minkowski(manifold.sphere(0.08)).move(0, -0.55, 0);
  return capsule.union(gem, soft);
}

function shapes2d(): Solid {
  const star = crossSection.polygon(Array.from({ length: 10 }, (_, i) => {
    const r = i % 2 ? 0.22 : 0.5;
    const a = (Math.PI * 2 * i) / 10;
    return [r * Math.cos(a), r * Math.sin(a)] as [number, number];
  }));
  const ring = crossSection.circle(0.5, 64).minus(crossSection.circle(0.38, 64)).union(crossSection.rect(0.9, 0.08));
  const tower = star.offset(0.04).extrude(0.9, { divisions: 24, twist: 120, scaleTop: [0.4, 0.4] }).rotate(-90, 0, 0).move(0, -0.45, 0);
  const vase = crossSection.hull([[0.05, 0], [0.3, 0], [0.32, 0.3], [0.05, 0.3]]).union(crossSection.rect(0.12, 0.3, false).move(0.25, 0.3)).revolve({ segments: 48 }).rotate(-90, 0, 0).move(0, -0.65, 0).scale(0.8);
  return tower.union(ring.extrude(0.1).rotate(-90, 0, 0).move(0, 0.55, 0), vase.move(0.55, 0, 0));
}

function from3dTo2d(): Solid {
  const knot = manifold.torus(0.35, 0.12).rotate(60, 0, 0).union(manifold.sphere(0.2));
  const shadow = knot.project().offset(0.05, { join: "round" });
  const waist = manifold.box(0.8, 0.8, 0.8).minus(manifold.cylinder(0.25, 2)).crossSection(0).offset(-0.05, { join: "miter" });
  return knot.move(0, 0.3, 0).union(shadow.extrude(0.06).rotate(-90, 0, 0).move(0, -0.45, 0), waist.extrude(0.05).rotate(-90, 0, 0).move(0, -0.65, 0).scale(0.7));
}

function meshes(): Solid {
  const gyroid = manifold.levelSet((x, y, z) => Math.hypot(x, y, z) - 0.45 + 0.06 * Math.sin(8 * x) * Math.sin(8 * y), { min: [-0.6, -0.6, -0.6], max: [0.6, 0.6, 0.6], resolution: 36 });
  const bent = manifold.box(1.2, 0.15, 0.15).refineToLength(0.06).warp((x, y, z) => [x, y + 0.4 * x * x, z]).move(0, -0.7, 0);
  const m = manifold.box(0.2, 0.2, 0.2).mesh();
  const block = manifold.fromMesh(m.verts, m.tris).move(0, 0.65, 0);
  return gyroid.union(bent, block);
}

const BUILDS: [string, () => Solid, string][] = [
  ["constructors", constructors, "#e11d48"],
  ["booleans", booleans, "#f59e0b"],
  ["transforms", transforms, "#84cc16"],
  ["cutting", cutting, "#10b981"],
  ["surfaces", surfaces, "#06b6d4"],
  ["hulls", hulls, "#3b82f6"],
  ["shapes2d", shapes2d, "#8b5cf6"],
  ["from3dTo2d", from3dTo2d, "#d946ef"],
  ["meshes", meshes, "#f43f5e"],
];

// built once, at module level (never per frame); outline is the plain-array 2D helper, kept in the tree
let solids: Solid[] | null = null;
const getSolids = () => (solids ??= BUILDS.map(([, build]) => build()));
void outline;

function Solids() {
  const f = useCurrentFrame();
  const list = getSolids();
  return (
    <AbsoluteFill style={{ background: "#0f1720" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 0.4 + 0.3 * Math.sin(f / 90), 8.4]} lookAt={[0, 0, 0]} fov={45} />
        <AmbientLight intensity={0.45} />
        <DirectionalLight position={[3, 4, 5]} intensity={1.3} />
        {list.map((s, i) => (
          <Mesh key={BUILDS[i][0]} name={BUILDS[i][0]} shape={s} position={[((i % 3) - 1) * 2.3, (1 - Math.floor(i / 3)) * 2.05, 0]} rotation={[0.25, f * 0.02 + i * 0.7, 0]}>
            <MeshStandardMaterial color={BUILDS[i][2]} roughness={0.45} metalness={0.1} />
          </Mesh>
        ))}
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="edit" component={Solids} width={W} height={H} fps={FPS} durationInFrames={20 * FPS} />);
