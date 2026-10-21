// The kitchen remodelled on @raea/motion/room (Shane, 10-21: "its currently very broken layout and ultimately we want
// the scene sdk to be clean and clear"): an L of base units on the back and right walls with an island, every unit
// laid against its wall and the one before from its own measured box, nothing placed by a typed coordinate but the
// room's size and where along a wall a run starts. Laid out to a kitchen's clearances (checkLayout): a 1.07 m work
// aisle round the island, 0.91 m walkways, room to walk behind the stools, the sink, stove and fridge a work triangle.
//   raea-motion run kitchen/remodel-check.ts      the plan's clearances
//   raea-motion render kitchen/remodel.tsx --composition plan|front|back|corner|island -o x.png
import { staticFile } from "@raea/motion";
import { Box, Group, Mesh, MeshStandardMaterial, Model } from "@raea/motion/3d";
import { bay, checkDecor, checkLayout, defineRoom, faceToward, hang, layRun, modelFootprint, seatFootprint, type Footprint, type Plan } from "@raea/motion/room";
import { modelBounds } from "@raea/motion/scene";
import { Fronts } from "./house";

const R = "assets/residential/";
const K = (p: string) => staticFile(R + "Kitchen/" + p);

/** 5.4 by 6.6 m, 2.7 m high: the back run (pantry, cupboards, the sink under the window, the corner) is 5.4 m long;
 * the right run goes on past the corner toward the front wall; the front of the room, past the stools, is the
 * breakfast nook's */
export const ROOM = defineRoom({ width: 5.4, depth: 6.6, height: 2.7 });

/** a board over a unit's top: the pack leaves the tops of its tall units and wall cabinets open (nobody sees over them
 * but a plan looking down) */
const cap = (w: number, d: number) => (
  <Mesh name="cap" position={[w / 2, 2.6, d / 2]}><Box args={[w, 0.006, d]} /><MeshStandardMaterial color="#b49a7e" roughness={0.6} /></Mesh>
);
// the pack's carcasses come bare: their doors and drawers, as house.tsx hangs them (each door from its upper hinge)
const base120 = (
  <>
    <Fronts n="SM_CupboardDoor1" y={0.65} z={0.928} />
    <Model src={K("SM_Drawer.glb")} position={[0.3, 0.84, 0.55]} />
    <Model src={K("SM_Drawer.glb")} position={[0.9, 0.84, 0.55]} />
  </>
);
const base60 = (
  <>
    <Model src={K("SM_CupboardDoor1_L.glb")} position={[0.035, 0.65, 0.928]} />
    <Model src={K("SM_Drawer.glb")} position={[0.3, 0.84, 0.55]} />
  </>
);
const sink = (
  <>
    <Fronts n="SM_CupboardDoorSink" y={0.81} z={0.928} />
    <Model src={K("SM_KitchenSink.glb")} />
  </>
);
const pantry = (
  <>
    <Fronts n="SM_CupboardDoor2" y={0.91} z={0.62} />
    <Fronts n="SM_CupboardDoor2" y={1.87} z={0.62} />
    <Fronts n="SM_CupboardDoor3" y={2.37} z={0.62} />
    {cap(1.2, 0.625)}
  </>
);
const top120 = (
  <>
    <Fronts n="SM_CupboardDoor2" y={2.35} z={0.62} />
    {cap(1.2, 0.625)}
  </>
);
const top60 = (
  <>
    <Model src={K("SM_CupboardDoor2_L.glb")} position={[0.035, 2.35, 0.62]} />
    {cap(0.6, 0.625)}
  </>
);

/** the fridge in its tall housing: the pack's fridge faces -x; turned to face the room, centred in the open width (the
 * _R housing's one side panel, 5 cm, is on its right: centred on the whole slot, the doors ran into it), its doors
 * standing 2 cm proud of the housing's front, the handles out past it, so they swing clear (set back, the handles were
 * boxed in behind the panel's edge); a pair of short doors on the housing's top, as on the pantry's */
const FRIDGE = K("Fridge/SM_FridgeClosed.glb");
/** the housing's side panel (x 1.15-1.20), the front of its carcass, and how far the fridge's front (its handles) stands
 * past that front */
const HOUSING_SIDE = 0.05, HOUSING_FRONT = 0.898, FRIDGE_PROUD = 0.06;
function fridgeIn(slot: number) {
  const b = modelBounds(FRIDGE, [0, Math.PI / 2, 0]);
  return (
    <>
      <Model name="fridge" src={FRIDGE} rotation={[0, Math.PI / 2, 0]} position={[(slot - HOUSING_SIDE) / 2 - (b.min[0] + b.max[0]) / 2, 0, HOUSING_FRONT + FRIDGE_PROUD - b.max[2]]} />
      <Fronts n="SM_CupboardDoor3" y={2.37} z={0.924} w={slot} />
      {cap(slot, 0.924)}
    </>
  );
}

/** the back wall, left to right: the pantry, a double cupboard, a single, the sink under the window, the corner */
export const BACK = layRun(ROOM, "back", 0, [
  { src: K("SM_Pantry.glb"), name: "pantry", children: pantry },
  { src: K("SM_Cupboard_120.glb"), name: "base-1", children: base120 },
  { src: K("SM_Cupboard_60.glb"), name: "base-2", children: base60 },
  { src: K("SM_Sink_Cupboard_120.glb"), name: "sink", role: "sink", children: sink },
  { src: K("SM_CounterCorner.glb"), name: "corner" },
]);
/** over it: wall cabinets over the double and the single (none over the pantry, which is full height, or the sink's
 * window), and a double into the corner (the pack's corner wall cabinet has no doors to hang) */
export const BACK_UP = layRun(ROOM, "back", 1.2, [
  { src: K("SM_TopCabinet_120.glb"), name: "upper-1", children: top120 },
  { src: K("SM_TopCabinet_60.glb"), name: "upper-2", children: top60 },
]);
export const CORNER_UP = layRun(ROOM, "back", 4.2, [{ src: K("SM_TopCabinet_120.glb"), name: "upper-corner", children: top120 }]);

/** the right wall, from past the corner: the stove, a single, the fridge in its housing, a single (the pack's end
 * panel, laid in a run, stood out across the floor; the carcass's side is finished) */
export const RIGHT = layRun(ROOM, "right", 1.2, [
  { src: K("Stove/SM_Stove.glb"), front: "+x", name: "stove", role: "stove", width: 1.2 },
  { src: K("SM_Cupboard_60.glb"), name: "base-3", children: base60 },
  { src: K("SM_FridgeCupboard_R.glb"), name: "fridge-housing", role: "fridge", children: fridgeIn(1.224) },
  { src: K("SM_Cupboard_60.glb"), name: "base-4", children: base60 },
]);
export const RIGHT_UP = layRun(ROOM, "right", 2.4, [{ src: K("SM_TopCabinet_60.glb"), name: "upper-3", children: top60 }]);

/** the island: the pack's shorter one, two base cabinets back to back (a single and a double, their doors and drawers
 * hung as on the walls) under one stone, a finished panel on the other long side with the stone over it by 0.21 m for
 * knees. Turned (yaw pi) so its cabinets face the sink across a work aisle and its seats face the room; a work aisle to
 * the stove, a walkway to the left wall. */
export const ISLAND_SRC = K("SM_Island02.glb");
export const ISLAND_YAW = Math.PI;
const ib = modelBounds(ISLAND_SRC, [0, ISLAND_YAW, 0]);
const CL = 1.07;
export const ISLAND_AT: [number, number, number] = [
  // a work aisle from the stove's front (the deepest unit on the right wall)
  ROOM.width - Math.max(...RIGHT.units.map((u) => u.depth)) - CL - ib.max[0],
  0,
  Math.max(...BACK.units.map((u) => u.depth)) + CL - ib.min[2],
];
export const ISLAND: Footprint = modelFootprint("island", ISLAND_SRC, ISLAND_AT, ISLAND_YAW);
/** in the island's own frame: the cabinets' backs 0.34 m behind its centre, the single from x -0.6, the double from 0 */
export const islandFronts = (
  <>
    <Group position={[-0.6, 0, -0.34]}>{base60}</Group>
    <Group position={[0, 0, -0.34]}>{base120}</Group>
  </>
);

/** three counter stools (Poly Haven's round wooden bar chair, seat 0.75 m under the 1.0 m stone) at the island's seat
 * side, tucked 0.1 m under the stone's edge */
export const STOOL_SRC = staticFile("assets/polyhaven/bar_chair_round_01.glb");
const ISLAND_MID: [number, number, number] = [(ISLAND.min[0] + ISLAND.max[0]) / 2, 0, (ISLAND.min[1] + ISLAND.max[1]) / 2];
const sb = modelBounds(STOOL_SRC);
export const STOOLS: { at: [number, number, number]; yaw: number }[] = [0.2, 0.5, 0.8].map((k) => {
  const at: [number, number, number] = [ISLAND.min[0] + (ISLAND.max[0] - ISLAND.min[0]) * k, 0, ISLAND.max[1] - 0.1 + (sb.max[2] - sb.min[2]) / 2];
  // each turned to face the island straight on (the stool's own front read from it)
  return { at, yaw: faceToward(STOOL_SRC, at, [at[0], 0, ISLAND_MID[2]]) };
});

/** the breakfast nook: a bay window out of the front wall, its seat cushioned, the round table before it, chairs round
 * the table's room side, every seat turned to the table by faceToward (its own front read from the model) */
const TABLE_SRC = staticFile(R + "Dining/SM_RoundDiningTable.glb");
const CHAIR_SRC = staticFile(R + "Dining/SM_DiningChair.glb");
const tb = modelBounds(TABLE_SRC);
export const NOOK_X = 1.95;
export const BAY = bay(ROOM, "front", ROOM.width - NOOK_X, { width: 2.6, depth: 0.7, angle: 45, sill: 0.62, head: 2.25, seat: 0.46 });
const TABLE_AT: [number, number, number] = [NOOK_X, 0, ROOM.depth - 0.12 - tb.max[2]];
export const NOOK = {
  table: TABLE_AT,
  chairs: ([
    [NOOK_X, 0, TABLE_AT[2] + tb.min[2] - 0.12],
    [NOOK_X + tb.max[0] + 0.12, 0, TABLE_AT[2] + 0.05],
    [NOOK_X + tb.min[0] - 0.12, 0, TABLE_AT[2] + 0.05],
  ] as [number, number, number][]).map((at) => ({ at, yaw: faceToward(CHAIR_SRC, at, TABLE_AT) })),
  tableSrc: TABLE_SRC,
  chairSrc: CHAIR_SRC,
};

export const PLAN: Plan = {
  room: ROOM,
  runs: [BACK, RIGHT],
  islands: [ISLAND],
  seats: [
    ...STOOLS.map((s, i) => seatFootprint(`stool ${i + 1}`, STOOL_SRC, s.at, s.yaw)),
    ...NOOK.chairs.map((c, i) => seatFootprint(`nook chair ${i + 1}`, CHAIR_SRC, c.at, c.yaw)),
    BAY.seat!,
  ],
  furniture: [modelFootprint("nook table", TABLE_SRC, NOOK.table)],
};

/** on the walls: a pair of pictures on the nook's side wall at eye height, a
 * picture on the right wall past the run, a shelf on the left wall between the door and the pantry; each by the decorator's rules (hang), its front read from it */
const PIC = (n: number) => staticFile(R + `Pictures/SM_Picture_0${n}.glb`);
export const DECOR = [
  ...hang(ROOM, "left", [{ src: PIC(2), name: "nook picture 1" }, { src: PIC(4), name: "nook picture 2" }], { at: 1.35 }),
  // (over base-4, 0.6 m, it ran into the fridge's housing: checkDecor)
  ...hang(ROOM, "left", [{ src: staticFile(R + "Shelves/SM_WallShelf.glb"), name: "wall shelf" }], { at: 4.9, centre: 1.6 }),
  ...hang(ROOM, "right", [{ src: PIC(5), name: "right picture" }], { at: RIGHT.end + (ROOM.depth - RIGHT.end) / 2 }),
];

export const findings = () => checkLayout(PLAN);
export const decorFindings = (openings: Parameters<typeof checkDecor>[2]["openings"]) => checkDecor(ROOM, DECOR, { openings, runs: [BACK, BACK_UP, CORNER_UP, RIGHT, RIGHT_UP] });
