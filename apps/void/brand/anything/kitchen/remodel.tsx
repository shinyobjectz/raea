// The remodelled kitchen drawn (remodel-plan.tsx lays it out): its shell with the sink's window, the front window and
// the garden door, the runs and their wall cabinets, the island and its stools, a few things on the counters, under a
// morning light: the sun through the front window, the sky in the windows, the room's panorama.
//   raea-motion render kitchen/remodel.tsx --composition plan|front|back|corner|island -o x.png
import { AbsoluteFill, Composition, registerRoot, staticFile } from "@raea/motion";
import { AmbientOcclusion, Box, Canvas3D, Decal, Group, Refinish, Mesh, MeshPhysicalMaterial, MeshStandardMaterial, Model, PerspectiveCamera, Shadows, ToneMapping, type Vec3 } from "@raea/motion/3d";
import { Daylight, daylight, roomExposure, WindowLight } from "@raea/motion/daylight";
import { Backsplash, backsplash, Hang, onWall, Outside, plant, Room, Run, type Opening } from "@raea/motion/room";
import { modelBounds, standOn } from "@raea/motion/scene";
import { BACK, BACK_UP, BAY, CORNER_UP, DECOR, ISLAND_AT, ISLAND_SRC, ISLAND_YAW, islandFronts, NOOK, RIGHT, RIGHT_UP, ROOM, STOOL_SRC, STOOLS } from "./remodel-plan";

const R = "assets/residential/";
const P = "assets/polyhaven/";

/** the pack's window (its trim 1.36 by 1.71 m at scale 1) at `k`, its middle `cy` up: the hole it needs, and itself */
function windowAt(wall: Opening["wall"], at: number, cy: number, k: number): Opening {
  return {
    wall, at, kind: "window", width: 1.24 * k, bottom: cy - 0.72 * k, top: cy + 0.93 * k,
    children: (
      <Group>
        <Model src={staticFile(R + "Architecture/Window/SM_Window_1m_Short.glb")} position={[0, cy, 0]} rotation={[0, Math.PI, 0]} scale={k} />
        <Mesh name={`${wall}-glass`} position={[0, cy + 0.12 * k, -0.05]} shadow={false}>
          <Box args={[0.86 * k, 1.34 * k, 0.006]} />
          <MeshPhysicalMaterial color="#dfe8ea" roughness={0.03} opacity={0.12} transparent clearcoat={1} />
        </Mesh>
        {/* a shallow sill (7 cm into the room): a tap under the window stands 9.5 cm off the wall, and a deeper one ran into it */}
        <Mesh name={`${wall}-sill`} position={[0, cy - 0.74 * k - 0.015, 0.035]}><Box args={[1.5 * k, 0.03, 0.07]} /><MeshStandardMaterial color="#f4efe4" roughness={0.6} /></Mesh>
      </Group>
    ),
  };
}

/** the garden door in the left wall, off the walkway past the island (the nook has the front corner): the pack's frame,
 * its door shut (swung open, the leaf filled the views from the front) */
const DOOR_W = 1.3;
export const OPENINGS: Opening[] = [
  windowAt("back", (BACK.units[3].u0 + BACK.units[3].u1) / 2, 1.86, 0.8),
  // the nook's bay
  BAY,
  {
    wall: "left", at: ROOM.depth - 3.6, width: DOOR_W, bottom: 0, top: 2.3, kind: "door",
    children: (
      <Group>
        <Model src={staticFile(R + "Architecture/Doors/SM_DoorFrame.glb")} position={[0, 0, -0.05]} rotation={[0, Math.PI, 0]} />
        <Model src={staticFile(R + "Architecture/Doors/SM_InsideDoor.glb")} position={[DOOR_W / 2 - 0.02, 0, 0.07]} rotation={[0, Math.PI, 0]} />
      </Group>
    ),
  },
];

/** the garden: maples beyond a ring of hedge and shrubs, grass clumps and daisies near the walls, seeded so it stays put
 * from frame to frame and swaying a little; nothing in the bay's way */
const F = (p: string) => staticFile(R + "Foliage/" + p);
export const GARDEN = plant(ROOM, [
  { src: F("Trees/SM_MapleTree_Pivot.glb"), count: 7, band: [5, 16], spacing: 5, scale: [0.8, 1.15], sway: 0.008 },
  { src: F("Bushes/Shrub/SM_Shrub01.glb"), count: 12, band: [1.2, 4], spacing: 1.4, scale: [0.8, 1.3], sway: 0.02 },
  { src: F("Grass/SM_GrassClump01.glb"), count: 70, band: [0.4, 7], spacing: 0.5, scale: [0.8, 1.4], sway: 0.05 },
  { src: F("Flowers/GardenDaisy/SM_GardenDaisy01.glb"), count: 14, band: [0.5, 3], spacing: 0.6, sway: 0.04 },
], 21, BAY.seat ? [{ name: "bay", min: [BAY.seat.min[0], BAY.seat.min[1]], max: [BAY.seat.max[0], BAY.seat.max[1] + 0.6] }] : []);

/** the counters' stone (the pack's cupboards: 1.0 m, their backsplash lip higher at the back) */
const TOP = 1.0;
/** the day: London, 21 October, half past nine in the morning, the room's front wall (the bay's) facing south; the
 * sky a clear autumn morning's (Poly Haven, CC0), turned so its sun stands where that morning's does */
export const DAY = daylight({ sky: staticFile(P + "autumn_field_puresky_2k.hdr"), lat: 51.5, lon: -0.12, time: "2026-10-21T09:30:00+01:00", north: 0 });

/** the island's paint over the pack's wood (its grain taken off), the stone left as it is */
const ISLAND_PAINT = ["MI_KitchenWood", "MI_KitchenWood_Base"].map((material) => ({ material, color: "#7f9479", map: null, roughness: 0.55 }));

/** the hood over the stove: what stops the tile there */
const HOOD = staticFile(R + "Kitchen/Stove/SM_StoveHood.glb");
const HOOD_BOX = modelBounds(HOOD, [0, -Math.PI / 2, 0]);
/** the backsplash: round both runs, up to the wall cabinets, the hood and the sink window's sill; glazed subway tile
 * (ambientCG Tiles032, CC0, 15 cm tiles, 0.6 m a repeat) recoloured to the counters' darker speckle (#8b6b49), the
 * grout their light (#c4b094) */
const SPLASH = backsplash(ROOM, [BACK, RIGHT], {
  // where nothing hangs over it, up to the wall cabinets' line (1.52 m)
  counter: TOP, over: [BACK_UP, CORNER_UP, RIGHT_UP], openings: OPENINGS, height: 0.52,
  caps: [{ wall: "right", u0: 1.8 + HOOD_BOX.min[0], u1: 1.8 + HOOD_BOX.max[0], bottom: 1.78 + HOOD_BOX.min[1] }],
});

/** the island's stone, from its own model */
const ISLAND_TOP = modelBounds(ISLAND_SRC).max[1];

/** a model stood on the counter at a point of the plan, by its own lowest point */
function OnCounter({ name, src, at }: { name: string; src: string; at: Vec3 }) {
  return <Model name={name} src={src} position={[at[0], standOn(src, TOP), at[2]]} />;
}

export function Remodel({ cam }: { cam: Cam }) {
  return (
    <Canvas3D>
      <PerspectiveCamera position={cam.pos} lookAt={cam.look} fov={cam.fov} up={cam.up} near={0.02} far={40} />
      {/* exposed for the room, as a meter reads it */}
      <ToneMapping mode="agx" exposure={roomExposure(DAY)} />
      <AmbientOcclusion quality="high" thickness={0.15} />
      {/* the light is the day's: the sun and the sky, and what each window lets in of them */}
      <Daylight day={DAY} />
      {OPENINGS.filter((o) => o.kind !== "door").map((o, i) => <WindowLight key={i} day={DAY} room={ROOM} opening={o} />)}
      <Shadows distance={9} firstCascade={3} cascades={3} mapSize={4096} />
      <Outside plants={GARDEN} centre={[ROOM.width / 2, ROOM.depth / 2]} />
      <Room room={ROOM} openings={OPENINGS} ceiling={cam.open ? false : undefined} />
      <Hang room={ROOM} hung={DECOR} />
      <Run run={BACK} />
      <Run run={BACK_UP} />
      <Run run={CORNER_UP} />
      <Run run={RIGHT} />
      <Run run={RIGHT_UP} />
      <Model name="stove-hood" src={HOOD} position={onWall(ROOM.walls.right, 1.8, 0.4, 1.78)} rotation={[0, ROOM.walls.right.yaw - Math.PI / 2, 0]} />
      {/* tile round the kitchen, a brown out of the counters' stone */}
      <Backsplash room={ROOM} pieces={SPLASH} src={staticFile("assets/decals/splash-subway-brown.jpg")} normalMap={staticFile("assets/decals/splash-subway_nor_gl.jpg")} tile={0.6} />
      {/* the island painted sage, the wall runs left in their wood: a two-tone kitchen */}
      <Group name="island" position={ISLAND_AT} rotation={[0, ISLAND_YAW, 0]}>
        <Refinish materials={ISLAND_PAINT}>
          <Model src={ISLAND_SRC} />
          {islandFronts}
        </Refinish>
      </Group>
      {STOOLS.map((s, i) => <Model key={i} name={`stool-${i + 1}`} src={STOOL_SRC} position={s.at} rotation={[0, s.yaw, 0]} />)}
      {[-0.5, 0.5].map((dx, i) => <Model key={dx} name={`ceiling-lamp-${i + 1}`} src={staticFile(R + "CeilingLamp/SM_CeilingLamp.glb")} position={[ISLAND_AT[0] + 0.2 + dx, ROOM.height, ISLAND_AT[2]]} />)}
      {/* the nook: its table and chairs before the bay's seat, a lamp over the table */}
      <Model name="nook-table" src={NOOK.tableSrc} position={NOOK.table} />
      {NOOK.chairs.map((c, i) => <Model key={i} name={`nook-chair-${i + 1}`} src={NOOK.chairSrc} position={c.at} rotation={[0, c.yaw, 0]} />)}
      <Model name="nook-lamp" src={staticFile(R + "CeilingLamp/SM_CeilingLamp.glb")} position={[NOOK.table[0], ROOM.height, NOOK.table[2]]} />
      <Model name="nook-vase" src={staticFile(R + "Vases/SM_Vase1.glb")} position={[NOOK.table[0], 0.777, NOOK.table[2]]} scale={0.6} />
      {/* on the counters, each stood on the stone by its own lowest point */}
      <OnCounter name="coffee-machine" src={staticFile(R + "CoffeeMachine/SM_CoffeeMachine.glb")} at={onWall(ROOM.walls.back, 1.55, 0.3)} />
      {/* its pot on the warming plate: the jug is modelled about its own foot, so it goes where the machine's plate is
          (its ring 7.35 cm up, centred 3.9 cm forward of the machine's middle) */}
      <Model name="coffee-pot" src={staticFile(R + "CoffeeMachine/SM_CoffeeMachineJug.glb")}
        position={((at) => [at[0] - 0.0027, standOn(staticFile(R + "CoffeeMachine/SM_CoffeeMachine.glb"), TOP) + 0.0735, at[2] + 0.039] as Vec3)(onWall(ROOM.walls.back, 1.55, 0.3))} />
      {/* a mug's ring left on the island's seat side (a decal) */}
      <Decal name="mug-ring" src={staticFile("assets/decals/coffee-ring.png")} position={[ISLAND_AT[0] - 0.35, ISLAND_TOP, ISLAND_AT[2] + 0.2]} size={0.12} spin={0.7} />
      <OnCounter name="kettle" src={staticFile(P + "vintage_electric_kettle.glb")} at={onWall(ROOM.walls.right, 2.7, 0.35)} />
    </Canvas3D>
  );
}

type Cam = { pos: Vec3; look: Vec3; fov: number; up?: Vec3; open?: boolean };
const W = ROOM.width, D = ROOM.depth;
export const VIEWS: Record<string, Cam> = {
  // straight down from high over the room, its ceiling off (from just under the ceiling a wide lens stretched the
  // fridge's doors down the frame)
  plan: { pos: [W / 2, 15, D / 2], look: [W / 2, 0, D / 2], fov: 27, up: [0, 0, -1], open: true },
  // from the front of the room, past the stools, toward the back run
  front: { pos: [3.9, 1.6, D - 0.6], look: [2.4, 0.95, 0.4], fov: 65 },
  // from the back corner by the pantry, toward the front window
  back: { pos: [0.35, 1.65, 1.1], look: [4.2, 0.9, 4.2], fov: 65 },
  // into the corner where the runs meet
  corner: { pos: [1.2, 1.55, 3.9], look: [5.0, 1.0, 0.6], fov: 60 },
  // close on the right wall, and on the corner where the runs meet
  rightwall: { pos: [2.6, 1.7, 3.0], look: [5.4, 1.6, 2.6], fov: 70 },
  cornerup: { pos: [3.2, 1.4, 2.2], look: [5.2, 2.0, 0.3], fov: 70 },
  // the left wall: the nook's pictures, the shelf past the door
  leftwall: { pos: [3.4, 1.55, D - 2.6], look: [0, 1.4, D - 3.2], fov: 72 },
  // the nook from the island, and the island's working side from the sink
  nook: { pos: [3.6, 1.45, D - 2.9], look: [1.95, 0.9, D + 0.2], fov: 60 },
  // the bay from in front of the table, low
  bay: { pos: [1.95, 1.2, D - 2.4], look: [1.95, 1.1, D + 0.4], fov: 62 },
  islandwork: { pos: [1.4, 1.6, 0.95], look: [2.6, 0.6, 2.6], fov: 65 },
  // the island, low across its top toward the stove
  island: { pos: [0.6, 1.25, 4.3], look: [3.4, 0.95, 2.0], fov: 50 },
  // close over the island's seat side, where the mug was
  // the sink under its window, the sill over the tap; the coffee machine and its pot
  sink: { pos: [3.15, 1.6, 1.45], look: [3.6, 1.2, 0.05], fov: 55 },
  fridge: { pos: [2.4, 1.3, 3.6], look: [5.4, 1.2, 3.6], fov: 50 },
  fridgeleft: { pos: [3.9, 1.9, 2.75], look: [4.9, 1.6, 3.2], fov: 50 },
  fridgeright: { pos: [3.9, 1.6, 4.5], look: [4.9, 1.4, 4.1], fov: 50 },
  coffee: { pos: [1.85, 1.4, 1.0], look: [1.55, 1.1, 0.2], fov: 45 },
  counter: { pos: [ISLAND_AT[0] - 0.1, 1.45, ISLAND_AT[2] + 0.9], look: [ISLAND_AT[0] - 0.35, 1.0, ISLAND_AT[2] + 0.2], fov: 45 },
};

function View({ v }: { v: string }) {
  return (
    <AbsoluteFill style={{ background: "#ddd" }}>
      <Remodel cam={VIEWS[v]} />
    </AbsoluteFill>
  );
}

registerRoot(() => <>{Object.keys(VIEWS).map((v) => <Composition key={v} id={v} component={() => <View v={v} />} width={1600} height={900} fps={30} durationInFrames={1} />)}</>);
