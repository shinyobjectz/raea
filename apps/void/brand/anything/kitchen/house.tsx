// The kitchen as a whole room, so V2's island can be shot from any side (board.md, "V2: the island"): walls with
// windows and a door, a ceiling, the cabinets and things of a lived-in kitchen, and outside a garden, all from the
// residential pack's pieces placed one by one (never its example maps) and our own boxes where the pack has nothing.
//
// The room, metres, looking down (+x right, +z toward the front of the house): the fridge's wall at x -0.6, the back
// wall (the counter run, the sink window) at z -0.25, the far wall at x 3.12 (the pantry, the back door standing
// open onto the patio), the front wall at z 4.4 (the window the island's sun comes through, a café curtain). The
// ceiling at 2.7. Two set-ups, as on a stage: "fridge" (V1: the door lit through a window cheat, kitchen.tsx's
// SunWindow; no ceiling, which would shade it) and "island" (V2: the island lit by a low morning sun through the
// front window, the room closed in).
import { staticFile } from "@raea/motion";
import { AmbientOcclusion, Bloom, Box, DirectionalLight, Environment, Group, ImagePlane, Mesh, MeshPhysicalMaterial, MeshStandardMaterial, Model, PointLight, SpotLight, ToneMapping } from "@raea/motion/3d";

type V3 = [number, number, number];
const R = "assets/residential/";
const P = "assets/polyhaven/";
export const ROOM = { x0: -0.6, x1: 3.12, z0: -0.25, z1: 4.4, h: 2.7 };

/** the front window (the pack's short window, its trim 1.36 x 1.71 m): its centre */
const FRONT_WIN: V3 = [2.1, 1.71, ROOM.z1];
/** the sink's window in the back wall, the pack's short window at 0.8 scale */
const SINK_WIN: V3 = [2.36, 1.86, ROOM.z0];
const SINK_K = 0.8;
/** the back counter run's work surface (kitchen.tsx's COUNTER.top; its backsplash lip is higher) */
const COUNTER_TOP = 1.0;
/** the back door, open onto the patio: the opening's middle along the far wall, its size */
const DOOR = { z: 3.0, w: 1.3, h: 2.3 };

/** the island's sun: a morning sun through the front window's upper panes, over the café curtain (which shades the
 * lower half), raking across the island top in the window's shape (the fridge's sun, kitchen.tsx's SUN_DIR, is
 * another; a set-up each, the change hidden in the whip pan) */
const ISLAND_AIM: V3 = [1.45, 1.0, 2.6];
const SUN_THROUGH: V3 = [FRONT_WIN[0], FRONT_WIN[1] + 0.5, FRONT_WIN[2]];
export const ISLAND_SUN: V3 = [SUN_THROUGH[0] - ISLAND_AIM[0], SUN_THROUGH[1] - ISLAND_AIM[1], SUN_THROUGH[2] - ISLAND_AIM[2]];

/** The island set's light, a morning room lit the way a real one is: the sun through the front window the key,
 * raking the island top in the window's shape; the sky in each window and the open back door a soft cool fill
 * (spots just inside them, wide and soft-edged, their shadows blurred to the opening's size, as an area light's);
 * the sunlit stone and boards throwing a warm bounce up onto the ceiling and the undersides; the panorama only a
 * little, since it shines through walls. A filmic curve, as nothing here has to match Paint's colours (the fridge's
 * set-up does, kitchen.tsx). */
/** `ev`: the camera's stop against the rig's exposure, as a real camera opens or closes on one light (a low shot into
 * the sunlit stone closes down); the light itself never changes shot to shot */
export function KitchenLight({ ev = 0 }: { ev?: number }) {
  const into = (w: V3, d: number): V3 => [w[0], w[1], w[2] + d];
  return (
    <>
      <ToneMapping mode="agx" exposure={L.exposure + ev} />
      <AmbientOcclusion quality="high" thickness={0.15} />
      <Environment src={staticFile(P + "kiara_interior_2k.hdr")} intensity={L.env} />
      {/* hard-edged shadow maps with Bevy's Gaussian filter (smooth, a texel or two of blur, as a sun's edge is):
          PCSS sizes were noisy at every edge, Bevy's soft shadows needing TAA, which a frame-pure render can't use */}
      <DirectionalLight name="sun" position={ISLAND_SUN} intensity={L.sun} color="#ffe9cc" castShadow />
      {/* the sky in the front window, the sink window and the back door */}
      <SpotLight name="sky-front" position={into(FRONT_WIN, -0.15)} target={[1.2, 0.6, 1.6]} angle={1.35} penumbra={1} intensity={L.sky} color="#dbe6f5" castShadow />
      <SpotLight name="sky-sink" position={into(SINK_WIN, 0.15)} target={[1.4, 0.7, 2.6]} angle={1.35} penumbra={1} intensity={L.sky * 0.7} color="#dfe8f4" castShadow />
      <SpotLight name="sky-door" position={[ROOM.x1 - 0.1, 1.4, DOOR.z]} target={[0.6, 0.6, 2.4]} angle={1.3} penumbra={1} intensity={L.sky * 0.6} color="#e4ecf4" castShadow />
      {/* the bounce off the sunlit top and the floor before the window: warm, from below, no shadows */}
      <PointLight name="bounce-top" position={[1.5, 1.25, 2.7]} intensity={L.bounce} color="#ffe2bd" distance={3.5} />
      <PointLight name="bounce-floor" position={[1.9, 0.25, 3.7]} intensity={L.bounce * 0.8} color="#f7dcb6" distance={3} />
      <Bloom intensity={0.25} threshold={1} />
    </>
  );
}
/** the kitchen's one light (KitchenLight), every shot's: in the SDK's units (intensity 1: 8000 lux for the sun, 900,000
 * lumens for a lamp; exposure 0: EV100 9.7). A hazy morning sun (24,000 lux) rather than a clear one, so its patch on
 * the stone holds texture at the room's exposure (a clear sun's clipped flat white, 8-26% of a frame); the sky and the
 * bounce raised for the shade side, standing in for the light the sunlit stone and floor throw back (2026-10-21: no
 * frame of v1 or v2 clips, shot a's landing 157 against b's 169 mean) */
export const L = { exposure: 1, env: 0.9, sun: 3, sky: 0.06, bounce: 0.012 };

const paint = () => <MeshStandardMaterial color="#efe0b4" roughness={0.9} />;

type Hole = [number, number, number, number]; // along from, to; up from, to
/** A wall `t` thick in the plane x = `at` (axis "x") or z = `at` (axis "z"), running `a0`..`a1` along it, full
 * height, with rectangular holes: slabs between and round them */
function Wall({ name, axis, at, a0, a1, holes = [], t = 0.1, h = ROOM.h }: { name: string; axis: "x" | "z"; at: number; a0: number; a1: number; holes?: Hole[]; t?: number; h?: number }) {
  const slabs: [number, number, number, number][] = [];
  let a = a0;
  for (const [h0, h1, y0, y1] of [...holes].sort((p, q) => p[0] - q[0])) {
    if (h0 > a) slabs.push([a, h0, 0, h]);
    slabs.push([h0, h1, 0, y0], [h0, h1, y1, h]);
    a = h1;
  }
  if (a < a1) slabs.push([a, a1, 0, h]);
  return (
    <>
      {slabs
        .filter(([p, q, y0, y1]) => q - p > 1e-3 && y1 - y0 > 1e-3)
        .map(([p, q, y0, y1], i) => {
          const mid = (p + q) / 2, len = q - p, ym = (y0 + y1) / 2, hh = y1 - y0;
          return (
            <Mesh key={i} name={`${name}-${i}`} position={axis === "z" ? [mid, ym, at] : [at, ym, mid]}>
              <Box args={axis === "z" ? [len, hh, t] : [t, hh, len]} />
              {paint()}
            </Mesh>
          );
        })}
    </>
  );
}

/** a pane of clear glass, `w` x `h`, square to z (turn it for a wall square to x): blended, and casting no shadow (a
 * blended surface casts one unless told not to, and the glass kept the sun out of the room) */
function Glass({ at, w, h, turn = 0 }: { at: V3; w: number; h: number; turn?: number }) {
  return (
    <Mesh name={`glass-${at.join("-")}`} position={at} rotation={[0, turn, 0]} shadow={false}>
      <Box args={[w, h, 0.006]} />
      <MeshPhysicalMaterial color="#dfe8ea" roughness={0.03} opacity={0.12} transparent clearcoat={1} />
    </Mesh>
  );
}

/** The window: the pack's trim, our glass, a sill inside. `k` scales it. */
function Window({ at, k = 1, turn = 0, name }: { at: V3; k?: number; turn?: number; name: string }) {
  return (
    <Group position={at} rotation={[0, turn, 0]}>
      <Model src={staticFile(R + "Architecture/Window/SM_Window_1m_Short.glb")} scale={k} />
      <Glass at={[0, 0.12 * k, 0]} w={0.86 * k} h={1.34 * k} />
      <Mesh name={`${name}-sill`} position={[0, -0.74 * k - 0.015, -0.1]}><Box args={[1.5 * k, 0.03, 0.24]} /><MeshStandardMaterial color="#f4efe4" roughness={0.6} /></Mesh>
    </Group>
  );
}

/** the walls, set-up by set-up: the fridge's set leaves out the front wall's window and the ceiling (the door's sun
 * comes in over them through the cheat); the island's has both */
function Shell({ island }: { island: boolean }) {
  const { x0, x1, z0, z1, h } = ROOM;
  const fw: Hole = [FRONT_WIN[0] - 0.62, FRONT_WIN[0] + 0.62, FRONT_WIN[1] - 0.72, FRONT_WIN[1] + 0.93];
  const sw: Hole = [SINK_WIN[0] - 0.62 * SINK_K, SINK_WIN[0] + 0.62 * SINK_K, SINK_WIN[1] - 0.72 * SINK_K, SINK_WIN[1] + 0.93 * SINK_K];
  const door: Hole = [DOOR.z - DOOR.w / 2, DOOR.z + DOOR.w / 2, 0, DOOR.h];
  return (
    <>
      <Mesh name="floor-under" position={[1, -0.01, 1.5]}><Box args={[8, 0.02, 9]} /><MeshStandardMaterial color="#cbbf9f" roughness={0.85} /></Mesh>
      <Mesh name="side-wall" position={[x0, h / 2, (z0 + z1) / 2]}><Box args={[0.04, h, z1 - z0]} />{paint()}</Mesh>
      <Wall name="back-wall" axis="z" at={z0 - 0.05} a0={x0} a1={x1 + 0.05} holes={island ? [sw] : []} />
      <Wall name="far-wall" axis="x" at={x1 + 0.05} a0={z0} a1={z1} holes={[door]} />
      <Wall name="front-wall" axis="z" at={z1 + 0.05} a0={x0} a1={x1 + 0.1} holes={island ? [fw] : []} />
      {island && (
        <>
          <Mesh name="ceiling" position={[(x0 + x1) / 2, h + 0.02, (z0 + z1) / 2]}><Box args={[x1 - x0 + 0.2, 0.04, z1 - z0 + 0.2]} /><MeshStandardMaterial color="#f6f1e6" roughness={0.95} /></Mesh>
          <Window name="front-window" at={FRONT_WIN} />
          <Window name="sink-window" at={SINK_WIN} k={SINK_K} turn={Math.PI} />
        </>
      )}
      {/* the floor, the pack's boards, 3 m pieces (each spans -3..0 along x) */}
      {[
        [2.4, 0], [2.4, 3], [5.4, 0], [5.4, 3],
      ].map(([x, z]) => <Model key={`${x},${z}`} src={staticFile(R + "Architecture/Floors/SM_Floor_3m.glb")} position={[x, 0.001, z0 + z]} />)}
      {/* the back door: the pack's frame in the far wall, its door swung open into the room */}
      <Model src={staticFile(R + "Architecture/Doors/SM_DoorFrame.glb")} position={[x1 + 0.05, 0, DOOR.z]} rotation={[0, Math.PI / 2, 0]} />
      <Model src={staticFile(R + "Architecture/Doors/SM_InsideDoor.glb")} position={[x1 - 0.02, 0, DOOR.z + DOOR.w / 2 - 0.02]} rotation={[0, Math.PI / 2 + 1.25, 0]} />
    </>
  );
}

/** a cabinet's doors shut on its carcass: the pack's carcasses come bare, their hinges on the side stiles; a door's
 * origin is its upper hinge, HINGE in from the carcass's side, and the doors lie on the face frame (z = its depth) */
const HINGE = 0.035;
export function Fronts({ n, y, z, w = 1.2 }: { n: string; y: number; z: number; w?: number }) {
  return (
    <>
      <Model src={staticFile(R + `Kitchen/${n}_L.glb`)} position={[HINGE, y, z]} />
      <Model src={staticFile(R + `Kitchen/${n}_R.glb`)} position={[w - HINGE, y, z]} />
    </>
  );
}

/** what's in the room besides the island (island.tsx) and the back counter run (kitchen.tsx) */
function Dressing() {
  const { x0, x1, z1, h } = ROOM;
  return (
    <>
      {/* the far wall: the pantry, a bin beside it, a coat rack by the door, a tall plant in the corner */}
      <Group position={[x1, 0, 0.75]} rotation={[0, -Math.PI / 2, 0]}>
        <Model src={staticFile(R + "Kitchen/SM_Pantry.glb")} />
        {/* its fronts (the pack's carcass comes bare): two tall pairs, a short pair at the top, each hung at its upper hinge */}
        <Fronts n="SM_CupboardDoor2" y={0.91} z={0.62} />
        <Fronts n="SM_CupboardDoor2" y={1.87} z={0.62} />
        <Fronts n="SM_CupboardDoor3" y={2.37} z={0.62} />
      </Group>
      <Model src={staticFile(R + "TrashBin/SM_TrashBin.glb")} position={[x1 - 0.22, 0, 2.12]} />
      <Model src={staticFile(R + "CoatRack/SM_CoatRackSmall.glb")} position={[x1, 1.7, 4.0]} rotation={[0, Math.PI, 0]} />
      <Model src={staticFile(R + "Plants/SM_SnakePlant.glb")} position={[x1 - 0.3, 0, z1 - 0.55]} />
      {/* the front wall: the window's café curtain on its rod, a bench under it */}
      <Model src={staticFile(R + "Curtain/SM_Curtain_Kitchen.glb")} position={[FRONT_WIN[0], FRONT_WIN[1] + 0.55, z1 - 0.02]} rotation={[0, Math.PI / 2, 0]} />
      <Model src={staticFile(P + "painted_wooden_bench.glb")} position={[FRONT_WIN[0] - 0.9, 0, z1 - 0.3]} />
      {/* the fridge's wall, past the fridge: the dining nook, a picture over it */}
      <Model src={staticFile(R + "Dining/SM_RoundDiningTable.glb")} position={[x0 + 0.85, 0, 3.4]} />
      <Model src={staticFile(R + "Dining/SM_DiningChair.glb")} position={[x0 + 1.6, 0, 3.45]} rotation={[0, -Math.PI / 2 - 0.2, 0]} />
      <Model src={staticFile(R + "Dining/SM_DiningChair.glb")} position={[x0 + 0.8, 0, 4.15]} rotation={[0, Math.PI + 0.15, 0]} />
      <Model src={staticFile(R + "Vases/SM_Vase1.glb")} position={[x0 + 0.85, 0.777, 3.4]} scale={0.6} />
      <Model src={staticFile(R + "Pictures/SM_Picture_01.glb")} position={[x0 + 0.02, 1.65, 3.4]} />
      {/* over the island, two flush ceiling lights (the pack's pendant is a chandelier, too much over a kitchen
          island); bar stools along its far side */}
      {[1.85, 2.85].map((z) => <Model key={z} src={staticFile(R + "CeilingLamp/SM_CeilingLamp.glb")} position={[1.45, h, z]} />)}
      {[1.75, 2.35, 2.95].map((z, i) => <Model key={z} src={staticFile(R + "BarStool/SM_BarStool.glb")} position={[2.25, 0, z]} rotation={[0, -Math.PI / 2 + (i - 1) * 0.12, 0]} />)}
      {/* by the sink: the draining board with a plate, the kettle */}
      <Model src={staticFile(R + "DrainingBoard/SM_DrainingBoard.glb")} position={[2.95 - 0.3, COUNTER_TOP + 0.0137, 0.3]} />
      <Model src={staticFile(P + "vintage_electric_kettle.glb")} position={[1.35, COUNTER_TOP + 0.0399, 0.2]} />
      <Model src={staticFile(R + "KitchenClutter/SM_SpoonCup.glb")} position={[1.6, COUNTER_TOP, 0.05]} />
      <Model src={staticFile(P + "wooden_cutting_board.glb")} position={[1.05, COUNTER_TOP, 0.25]} rotation={[0, 0.2, 0]} />
    </>
  );
}

/** outside: grass, the patio off the back door with its table and chairs, a fence round the garden, a hedge, the neighbour's maple over the fence,
 * shrubs under the windows, and the sky (big unlit cards all round) */
/** the sky: one flat colour, a picture so its cards can be unlit and cast no shadow */
const SKY = "data:image/svg+xml," + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#cfe0ee"/></svg>');
function Garden() {
  return (
    <>
      <Mesh name="grass" position={[3, -0.03, 3]}><Box args={[60, 0.02, 60]} /><MeshStandardMaterial color="#6d7f45" roughness={0.95} /></Mesh>
      <Model src={staticFile(R + "Paths/SM_Patio_4m.glb")} position={[ROOM.x1 + 2.1, 0.0, DOOR.z]} />
      <Model src={staticFile(R + "OutsideTable/SM_OutsideTable.glb")} position={[ROOM.x1 + 2.3, 0, DOOR.z + 0.4]} />
      <Model src={staticFile(R + "OutsideChair/SM_OutsideChair.glb")} position={[ROOM.x1 + 1.5, 0, DOOR.z + 0.5]} rotation={[0, Math.PI / 2, 0]} />
      <Model src={staticFile(R + "OutsideChair/SM_OutsideChair.glb")} position={[ROOM.x1 + 3.1, 0, DOOR.z + 0.2]} rotation={[0, -Math.PI / 2, 0]} />
      {[-4, 0, 4, 8].map((z) => <Model key={z} src={staticFile(R + "WoodenFence/SM_WoodenFence.glb")} position={[9, 0, z]} />)}
      {[-1, 3, 7].map((x) => <Model key={x} src={staticFile(R + "WoodenFence/SM_WoodenFence.glb")} position={[x, 0, 10]} rotation={[0, Math.PI / 2, 0]} />)}
      <Model src={staticFile(R + "Foliage/Bushes/Hedge/SM_Hedge_4m.glb")} position={[1.5, 0, -2.6]} />
      <Model src={staticFile(R + "Foliage/Trees/SM_MapleTree_Pivot.glb")} position={[11, 0, 9]} />
      <Model src={staticFile(R + "Foliage/Bushes/Shrub/SM_Shrub01.glb")} position={[1.4, 0, ROOM.z1 + 0.9]} />
      <Model src={staticFile(R + "Foliage/Bushes/Shrub/SM_Shrub01.glb")} position={[3.0, 0, ROOM.z1 + 1.1]} rotation={[0, 1.7, 0]} />
      <Model src={staticFile(R + "Foliage/Flowers/GardenDaisy/SM_GardenDaisy01.glb")} position={[ROOM.x1 + 0.6, 0.0268, DOOR.z - 1.2]} />
      <Model src={staticFile(R + "Foliage/Flowers/GardenDaisy/SM_GardenDaisy02.glb")} position={[ROOM.x1 + 0.5, 0.0267, DOOR.z + 1.4]} />
      {/* the sky's cards cast no shadow: an opaque card 28 m off stood between the sun and the house, and the room
          had no sun at all */}
      <ImagePlane name="sky-e" src={SKY} shadow={false} width={70} height={40} position={[30, 10, 3]} rotation={[0, -Math.PI / 2, 0]} />
      <ImagePlane name="sky-w" src={SKY} shadow={false} width={70} height={40} position={[-25, 10, 3]} rotation={[0, Math.PI / 2, 0]} />
      <ImagePlane name="sky-s" src={SKY} shadow={false} width={70} height={40} position={[3, 10, 30]} rotation={[0, Math.PI, 0]} />
      <ImagePlane name="sky-n" src={SKY} shadow={false} width={70} height={40} position={[3, 10, -25]} />
    </>
  );
}

/** The room round the kitchen's counter run and fridge (kitchen.tsx draws those) */
export function House({ set }: { set: "fridge" | "island" }) {
  const island = set === "island";
  return (
    <>
      <Shell island={island} />
      <Dressing />
      <Garden />
    </>
  );
}
