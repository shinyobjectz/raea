// The paint look (<Look mode="paint">): real 3D scenes drawn as flat editorial illustration. Three shots, each its own
// composition (a frame look is set from a composition's first frame, so a painting a composition), in 16:9 and 9:16:
//   houses      three tall white houses with red, blue and yellow roofs standing in teal water, seen from above, the
//               camera slowly orbiting, a low sun throwing long hard shadows; raea's pixel cat on the red roof
//               (<Model look="pixel">) to show a model's own look over the frame's (refs/ref-4.png)
//   containers  stacked ribbed shipping containers under a flat blue sky with cumulus clouds, the camera trucking
//               (refs/ref-5.png)
//   field       a flat green field with furrows receding to a cyan sky, a black smoke column leaning on the wind from
//               a small fire (refs/ref-6.png)
// One scene definition serves both aspects: `useVideoConfig()` picks each shot's framing (`wide`), not a crop.
// Render: ./render.sh (each shot in both aspects, joined into demo-16x9.mp4 and demo-9x16.mp4).
import { AbsoluteFill, Composition, registerRoot, staticFile, useCurrentFrame, useVideoConfig } from "@raea/motion";
import {
  Box, Canvas3D, Cone, MeshBasicMaterial, DirectionalLight, Group, Look, Mesh, MeshStandardMaterial, Model, PerspectiveCamera, Shadows, crossSection, manifold, type Solid,
} from "@raea/motion/3d";

const FPS = 30;
const SHOT = 90;
type V3 = [number, number, number];
const ease = (x: number) => (x <= 0 ? 0 : x >= 1 ? 1 : x * x * (3 - 2 * x));
const lerp = (a: number, b: number, u: number) => a + (b - a) * u;
/** a seeded random in [0, 1) */
const rnd = (i: number) => {
  const x = Math.sin(i * 127.1 + 311.7) * 43758.5453;
  return x - Math.floor(x);
};

/** wide (16:9) or tall (9:16) */
function useWide() {
  const { width, height } = useVideoConfig();
  return width >= height;
}

// ---------------------------------------------------------------- houses

let houseParts: { wall: Solid; roof: Solid; chimney: Solid } | null = null;
function house() {
  return (houseParts ??= {
    wall: manifold.box(1.2, 3.4, 1.2).move(0, 1.7, 0),
    // a gable: a triangle extruded along z, overhanging the walls a little
    roof: crossSection.polygon([[-0.7, 0], [0.7, 0], [0, 0.62]]).extrude(1.36).move(0, 3.4, -0.68),
    chimney: manifold.box(0.14, 0.42, 0.14),
  });
}

const HOUSES: { at: V3; turn: number; roof: string; h: number }[] = [
  { at: [-1.5, 0, 0.6], turn: 0.25, roof: "#1f68b0", h: 1.0 },
  { at: [0.3, 0, -1.4], turn: 0.1, roof: "#e2533a", h: 1.12 },
  { at: [1.5, 0, 0.9], turn: -0.15, roof: "#f0b52e", h: 0.92 },
];

function House({ at, turn, roof, h, n }: { at: V3; turn: number; roof: string; h: number; n: number }) {
  const p = house();
  const wall = "#f6f3ec";
  const win = (k: number, pos: V3) => (
    <Mesh key={k} name={`win${n}-${k}`} position={pos}>
      <Box args={[0.11, 0.2, 0.04]} />
      <MeshStandardMaterial color="#1a1614" />
    </Mesh>
  );
  return (
    <Group position={at} rotation={[0, turn, 0]} scale={[1, h, 1]}>
      <Mesh name={`wall${n}`} shape={p.wall}><MeshStandardMaterial color={wall} roughness={1} /></Mesh>
      <Mesh name={`roof${n}`} shape={p.roof}><MeshStandardMaterial color={roof} roughness={1} /></Mesh>
      <Mesh name={`chim${n}`} shape={p.chimney} position={[0.28, 3.75, 0.2]}><MeshStandardMaterial color={wall} roughness={1} /></Mesh>
      {win(0, [-0.25, 2.8, 0.61])}
      {win(1, [0.25, 2.2, 0.61])}
      {win(2, [0.61, 2.75, -0.2])}
      {win(3, [0.61, 2.0, 0.25])}
      {win(4, [-0.61, 2.6, 0.1])}
    </Group>
  );
}

function Houses() {
  const f = useCurrentFrame();
  const wide = useWide();
  const u = f / SHOT;
  const a = lerp(0.55, 0.85, u);
  const r = wide ? 11 : 14;
  const cam: V3 = [Math.sin(a) * r, wide ? 10.5 : 14, Math.cos(a) * r];
  const boat = lerp(-2.6, -1.3, u);
  return (
    <AbsoluteFill style={{ background: "#5b9497" }}>
      <Canvas3D>
        <PerspectiveCamera position={cam} lookAt={wide ? [0, 1.4, 0] : [0.2, 1.8, -0.2]} fov={wide ? 34 : 36} />
        <Look mode="paint" bands={3} shadow="#b4b6bc" mid="#cfa7a2" grain={0.35} rag={2.2} sky="#5b9497" />
        <DirectionalLight position={[-5, 3.2, 6]} intensity={1.2} />
        <Shadows distance={40} firstCascade={30} cascades={1} mapSize={4096} />
        <Mesh name="water" position={[0, -0.05, 0]}><Box args={[80, 0.1, 80]} /><MeshStandardMaterial color="#5b9497" roughness={1} /></Mesh>
        {HOUSES.map((h, i) => <House key={i} n={i} {...h} />)}
        <Mesh name="boat" position={[boat + 1.2, 0.04, 4.2 - (boat + 2.6) * 0.5]} rotation={[0, 0.45, 0]}>
          <Box args={[0.6, 0.12, 0.22]} />
          <MeshStandardMaterial color="#f6f3ec" roughness={1} />
        </Mesh>
        <Mesh name="boat-stripe" position={[boat + 1.2, 0.11, 4.2 - (boat + 2.6) * 0.5]} rotation={[0, 0.45, 0]}>
          <Box args={[0.28, 0.06, 0.16]} />
          <MeshStandardMaterial color="#d8463a" roughness={1} />
        </Mesh>
        {/* the pixel cat on the red roof's ridge, in its own look over the frame's */}
        <Model name="cat" src={staticFile("cat-white.glb")} animation="LookAround" time={f / FPS + 0.6} loop look="pixel" pixelSize={5} edgeColor="#140a28"
          position={[0.3, 3.4 * 1.12 + 0.6, -1.4]} rotation={[0, 0.9, 0]} scale={0.6} castShadow />
      </Canvas3D>
    </AbsoluteFill>
  );
}

// ---------------------------------------------------------------- containers

let containerShape: Solid | null = null;
/** a 6 m container, 2.5 m tall and deep, ribbed along its long faces (ribs as thin extrusions), a rail along the top */
function container() {
  if (containerShape) return containerShape;
  const ribs: Solid[] = [];
  for (let i = 0; i < 19; i++) ribs.push(manifold.box(0.12, 2.25, 2.8).move(-2.7 + i * 0.3, 0, 0));
  return (containerShape = manifold.box(6, 2.5, 2.5).union(manifold.union(...ribs)).union(manifold.box(6.04, 0.14, 2.62).move(0, 1.2, 0)));
}

let cloudShape: Solid | null = null;
/** a cumulus: lumpy puffs piled on a flat bottom, as one solid from a signed distance field */
function cloud() {
  if (cloudShape) return cloudShape;
  const puffs: { c: V3; r: number }[] = [];
  for (let i = 0; i < 26; i++) {
    const x = (rnd(i) - 0.5) * 9;
    const top = 4.2 - Math.abs(x) * 0.7;
    const r = 0.7 + rnd(i + 20) * 0.9;
    puffs.push({ c: [x, Math.max(r * 0.5, rnd(i + 40) * top), (rnd(i + 60) - 0.5) * 2.4], r: r + Math.max(0, top) * 0.18 });
  }
  const sdf = (x: number, y: number, z: number) => {
    let d = 1e9;
    for (const b of puffs) {
      const dx = x - b.c[0], dy = y - b.c[1], dz = z - b.c[2];
      // a smooth union: puffs bulge into each other with no crevice between them
      const e = Math.sqrt(dx * dx + dy * dy + dz * dz) - b.r;
      const h = Math.max(0.3 - Math.abs(d - e), 0) / 0.3;
      d = Math.min(d, e) - h * h * 0.3 * 0.25;
    }
    return Math.max(d, -y);
  };
  return (cloudShape = manifold.levelSet(sdf, { min: [-7, -0.5, -4], max: [7, 9, 4], resolution: 72 }));
}

// a front row of three, a row of three on it, and one more on the right, set forward so its shadow lands on the
// stack beside it
const STACK: { at: V3; c: string }[] = [
  { at: [-6.2, -1.25, 0], c: "#2b5cab" },
  { at: [0, -1.25, 0], c: "#b8382b" },
  { at: [6.2, -1.25, 0], c: "#b8382b" },
  { at: [-6.2, 1.25, 0], c: "#b8382b" },
  { at: [0, 1.25, 0], c: "#b8382b" },
  { at: [6.2, 1.25, 0], c: "#2b5cab" },
  { at: [-6.2, 3.75, 0], c: "#b8382b" },
  { at: [0, 3.75, 0], c: "#2b5cab" },
  { at: [6.2, 3.75, 0], c: "#b8382b" },
  { at: [6.2, 6.25, 1.2], c: "#2b5cab" },
];

function Containers() {
  const f = useCurrentFrame();
  const wide = useWide();
  const x = lerp(-1.8, 1.8, ease(f / SHOT));
  return (
    <AbsoluteFill style={{ background: "#2f6fc4" }}>
      <Canvas3D>
        {/* wide: the whole stack, its bottom row cut by the frame; tall: its right half, low, under a tall sky */}
        <PerspectiveCamera position={wide ? [x, 2.6, 19] : [x + 3, 1.5, 24]} lookAt={wide ? [x + 0.5, 5.4, 0] : [x + 3, 8.5, 0]} fov={wide ? 44 : 50} />
        <Look mode="paint" bands={3} shadow="#6c6878" mid="#b9c3df" grain={0.4} rag={1.3} sky="#2f6fc4" />
        <DirectionalLight position={[7, 6, 6]} intensity={1.2} />
        <Shadows distance={40} firstCascade={35} cascades={1} mapSize={8192} />
        {STACK.map((s, i) => (
          <Mesh key={i} name={`box${i}`} shape={container()} position={s.at}>
            <MeshStandardMaterial color={s.c} roughness={1} />
          </Mesh>
        ))}
        <Mesh name="cloud-l" shape={cloud()} position={[-17, 6.5, -20]} rotation={[0, 0.3, 0]} scale={1.3} castShadow={false}><MeshStandardMaterial color="#f3f6fb" roughness={1} /></Mesh>
        <Mesh name="cloud-m" shape={cloud()} position={wide ? [-7, 6.4, -24] : [-4, 9, -24]} rotation={[0, 2.1, 0]} scale={1.0} castShadow={false}><MeshStandardMaterial color="#f3f6fb" roughness={1} /></Mesh>
        <Mesh name="cloud-r" shape={cloud()} position={wide ? [17, 8.5, -20] : [13, 13, -20]} rotation={[0, 1, 0]} scale={1.4} castShadow={false}><MeshStandardMaterial color="#f3f6fb" roughness={1} /></Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

// ---------------------------------------------------------------- field

let smokeShape: Solid | null = null;
/** the smoke: a column of lumpy blobs leaning downwind, as one solid from a signed distance field */
function smoke() {
  if (smokeShape) return smokeShape;
  const blobs: { c: V3; r: number }[] = [];
  for (let i = 0; i < 60; i++) {
    const t = Math.pow(i / 59, 0.85);
    // up and leaning downwind, wider as it goes, with billows off the spine
    const spine: V3 = [t * t * 13 + Math.sin(t * 7) * 1.2 * t, t * 15, 0];
    const w = 0.4 + t * 3;
    blobs.push({ c: [spine[0] + (rnd(i) - 0.5) * w * 1.4, spine[1] + (rnd(i + 9) - 0.5) * w * 0.8, (rnd(i + 19) - 0.5) * w], r: 0.3 + w * (0.45 + rnd(i + 30) * 0.4) });
  }
  const sdf = (x: number, y: number, z: number) => {
    let d = 1e9;
    for (const b of blobs) {
      const dx = x - b.c[0], dy = y - b.c[1], dz = z - b.c[2];
      d = Math.min(d, Math.sqrt(dx * dx + dy * dy + dz * dz) - b.r);
    }
    // lumps on the surface
    return d + 0.25 * Math.sin(x * 2.3 + y * 1.7) * Math.sin(z * 2.1 - y * 1.3);
  };
  return (smokeShape = manifold.levelSet(sdf, { min: [-4, -1, -6], max: [20, 22, 6], resolution: 110 }));
}

function Field() {
  const f = useCurrentFrame();
  const wide = useWide();
  const u = ease(f / SHOT);
  const furrows = [];
  for (let i = -30; i <= 30; i++) {
    furrows.push(
      <Mesh key={i} name={`furrow${i}`} position={[i * 1.2, 0.02, -40]}>
        <Box args={[0.22, 0.05, 120]} />
        <MeshStandardMaterial color="#5f9a1c" roughness={1} />
      </Mesh>,
    );
  }
  return (
    <AbsoluteFill style={{ background: "#1aa3e8" }}>
      <Canvas3D>
        <PerspectiveCamera position={[lerp(-0.6, 0.6, u), wide ? 2.2 : 2.6, lerp(16, 13, u)]} lookAt={wide ? [2, 5, -40] : [5, 9, -40]} fov={wide ? 46 : 62} />
        <Look mode="paint" bands={3} shadow="#5a5a5a" mid="#9a9a9a" grain={0.35} rag={2.6} sky="#1aa3e8" />
        <DirectionalLight position={[3, 8, 4]} intensity={1.2} />
        <Shadows distance={80} firstCascade={70} cascades={1} mapSize={4096} />
        <Mesh name="ground" position={[0, -0.05, -40]}><Box args={[300, 0.1, 160]} /><MeshStandardMaterial color="#8fd12a" roughness={1} /></Mesh>
        {furrows}
        <Mesh name="smoke" shape={smoke()} position={[3, 0, -36]} rotation={[0, 0, -0.08 - 0.03 * Math.sin(f / 20)]}>
          <MeshStandardMaterial color="#3a3a3c" roughness={1} />
        </Mesh>
        <Mesh name="fire" position={[3, 1.1, -36]} rotation={[0, 0, -0.3]}><Cone args={[1.3, 2.8]} /><MeshBasicMaterial color="#f2735a" /></Mesh>
        <Mesh name="fire-in" position={[3.4, 0.8, -35.2]} rotation={[0, 0, -0.35]}><Cone args={[0.8, 1.8]} /><MeshBasicMaterial color="#f7c64a" /></Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

// ---------------------------------------------------------------- compositions

const SHOTS = { houses: Houses, containers: Containers, field: Field };

registerRoot(() => (
  <>
    {Object.entries(SHOTS).map(([id, C]) => (
      <>
        <Composition key={`${id}-16x9`} id={`${id}-16x9`} component={C} width={1920} height={1080} fps={FPS} durationInFrames={SHOT} />
        <Composition key={`${id}-9x16`} id={`${id}-9x16`} component={C} width={1080} height={1920} fps={FPS} durationInFrames={SHOT} />
      </>
    ))}
  </>
));
