# Meshes and solids

## What to do

- A `<Mesh position rotation scale>` holds one geometry and one material. Sizes are scene units (think metres: a
  phone is 0.15, a person 1.8, a room 4). The geometries, all centred on the Mesh's position:
  - `Box args={[w, h, d]}`
  - `Sphere args={[r]}`
  - `Cylinder args={[radiusTop, radiusBottom, height]}`, standing along y
  - `Cone args={[radius, height]}`, point up
  - `Torus args={[radius, tube]}`, lying in the x-y plane (rotate `[Math.PI / 2, 0, 0]` to lay it flat)
  - `Plane args={[w, h]}`, facing +z (a wall); for a floor use a thin `Box args={[10, 0.1, 10]}`
  - `Text3D size depth`: solid letters, centred, e.g. `<Text3D size={0.6} depth={0.2}>SALE</Text3D>`
- Model anything else as a Manifold solid and pass it as `<Mesh shape={solid}>`: `manifold.box(w, h, d)`,
  `roundedBox(w, h, d, r)`, `sphere(r)`, `cylinder(r, h)` (along z), `cone(r1, r2, h)`, `torus(R, r)` and
  `text3d(text, size, depth)`, combined with `.union()`, `.minus()`, `.intersect()`, `.hull()`, moved with
  `.move(x, y, z)`, `.rotate(x, y, z)` (**degrees**) and `.scale(...)`. Build a solid once at module level, never per
  frame, and move the Mesh instead.
- `scale` is a number or `[x, y, z]` and may change every frame (a pillar growing, a squash on landing).
- Give a Mesh a `name` when it comes and goes, or swaps places with another of the same shape.

## Manifold in full (`@raea/motion/3d`)

Everything below returns a new Solid (or CrossSection) and leaves the old one as it was. Build at module level.

- **Solids from scratch**: `manifold.box`, `roundedBox`, `sphere`, `cylinder` (along z), `cone(r1, r2, h)`, `torus`,
  `text3d`; `manifold.extrude(outline, h, {divisions, twist, scaleTop})` and `manifold.revolve(outline, {degrees,
  segments})` from a 2D shape; `manifold.levelSet(sdf, {min, max, resolution})` from a signed distance function
  (negative inside); `manifold.hull([[x, y, z], ...])` from points; `manifold.fromMesh(verts, tris)` and
  `manifold.import("x.glb")` from triangles.
- **Booleans**: `a.union(b, c)`, `a.minus(b)`, `a.intersect(b)`; for many at once `manifold.union(...list)`,
  `manifold.difference(first, ...cutters)`, `manifold.intersection(...list)`; `manifold.compose(...)` holds solids
  that don't touch as one, `s.decompose()` splits a solid into its separate pieces.
- **Moves**: `.move(x, y, z)`, `.rotate(x, y, z)` (degrees), `.scale(s)` or `.scale(x, y, z)`, `.mirror(nx, ny, nz)`,
  `.transform([12 numbers, column by column])`, `.warp((x, y, z) => [x, y + x * x, z])` (refine first so there are
  vertices to bend: `.refineToLength(0.05).warp(...)`).
- **Cutting**: `.trim(nx, ny, nz, offset)` keeps the normal's side of a plane; `.splitByPlane(...)` gives both halves;
  `.split(cutter)` gives [inside, outside].
- **Surfaces**: `.refine(n)`, `.refineToLength(len)`, `.refineToTolerance(tol)`, `.smooth({angle, refine})` (a box
  becomes a pebble), `.simplify(tol)`; `.hull(...others)` wraps them; `.minkowski(manifold.sphere(0.05))` rounds every
  edge, `.minkowskiDiff(...)` erodes.
- **2D shapes** (`crossSection`): `crossSection.rect(w, h)`, `circle(r)`, `polygon(points)`, `hull(points)`,
  `of(contours)`; then `.union/.minus/.intersect`, `.offset(d, {join: "round" | "miter" | "square" | "bevel"})` (grow
  or, negative, shrink), `.hull()`, `.minkowski()`, `.move(x, y)`, `.rotate(deg)`, `.scale()`, `.mirror()`,
  `.simplify()`, `.decompose()`, and `.extrude(h, ...)` / `.revolve(...)` into a solid. `outline.rect/circle/polygon`
  are the same shapes as plain arrays.
- **From 3D back to 2D**: `s.project()` is its shadow on the x-y plane (seen down the z axis, so project before
  standing a solid up), `s.crossSection(z)` its slice at height z,
  both CrossSections to offset and extrude again (a base plate that follows a model's footprint).
- **Asking**: `.volume()`, `.area()`, `.genus()` (holes through it), `.bounds()`, `.triangles()`, `.isEmpty()`,
  `.slice(z)` ({area, contours, polygons}), `.minGap(other, searchLength)`, `.rayCast(from, to)` (where a line meets
  the surface: put one thing on top of another), `.export("x.glb")`.

Extruded and revolved shapes grow along z: `.rotate(-90, 0, 0)` stands them up along y, the scene's up.

```tsx
// a vase: a profile revolved (along z) and hollowed, a base from its footprint (project looks down z), then the whole
// piece stood up along y
const profile = crossSection.polygon([[0, 0], [0.3, 0], [0.45, 0.3], [0.25, 0.8], [0.3, 1], [0, 1]]);
const outer = profile.revolve({ segments: 64 });
const body = outer.minus(outer.scale(0.88, 0.88, 1).move(0, 0, 0.06));
const base = body.project().offset(0.08).extrude(0.05).move(0, 0, -0.05);
const vase = body.union(base).rotate(-90, 0, 0);
```

## Why

Every geometry reaches the engine as a Manifold expression (`box(1, 2, 3)`), built once per distinct expression and
cached, then drawn by Bevy. A modelled solid and a primitive travel the same way.

## Common mistakes

- Two geometries, or none, in a Mesh: give it exactly one, plus one material.
- `args` that change per frame (a box growing by its `args`): that's a new shape each frame. Animate `scale`.
- Degrees in Mesh `rotation`, radians in `.rotate()`: it's the other way round.
- A Plane as a floor without rotating it: it stands up like a wall. Use a thin Box.
- Text3D with quotes or backslashes in it: they throw.
- A 2D outline revolved with points at negative x: revolve takes x >= 0 (the axis is x = 0).
- Building solids inside the component body: every frame rebuilds them. Build once (module level, `??=`).

## Example: a sign with the word cut through it, and solid letters in front

```tsx
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Canvas3D, Cone, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, Text3D, manifold, type Solid } from "@raea/motion/3d";

let sign: Solid | null = null;
const getSign = () => (sign ??= manifold.roundedBox(2.6, 1.1, 0.25, 0.1).minus(manifold.text3d("OPEN", 0.6, 0.6)));

function Sign() {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "#0f1720" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 0.3, 6]} lookAt={[0, 0, 0]} fov={45} />
        <AmbientLight intensity={0.5} />
        <DirectionalLight position={[2, 3, 4]} intensity={1.2} />
        <Mesh shape={getSign()} position={[0, 0.6, 0]} rotation={[0, Math.sin(f * 0.05) * 0.4, 0]}>
          <MeshStandardMaterial color="#e11d48" roughness={0.4} />
        </Mesh>
        <Mesh position={[0, -0.8, 0.5]}>
          <Text3D size={0.45} depth={0.15}>DAILY</Text3D>
          <MeshStandardMaterial color="#facc15" roughness={0.3} metalness={0.3} />
        </Mesh>
        <Mesh position={[2.2, -0.6, 0]}>
          <Cone args={[0.35, 0.9]} />
          <MeshStandardMaterial color="#38bdf8" roughness={0.5} />
        </Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="sign" component={Sign} width={1280} height={720} fps={30} durationInFrames={90} />);
```
