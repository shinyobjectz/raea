// ROOM: a walk round a small room, the games gate's fixture for the scene SDK's interiors played live. Steer with the
// arrow keys (a scripted loop round the room when nobody does); the camera follows from the front wall. A pure fold, as arena3d is.
//
// What it exercises in live 3D: a picture hung by hang() (its place from its own measured box, on the wall it faces
// from), a cabinet re-covered by <Refinish> (its wood painted), a sticker (<Decal>) on the floor fading as the player
// nears it, a radio (<Sound3D>) heard from the chase camera (panning as it turns about the room), and a garden outside
// (plant(), <Outside>) swaying with the game's clock.
import { AbsoluteFill, Composition, registerRoot, staticFile, useGameState, type FrameInput, type Playtest } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, Decal, DirectionalLight, Mesh, MeshStandardMaterial, Model, PerspectiveCamera, Refinish } from "@raea/motion/3d";
import { defineRoom, Hang, hang, onWall, Outside, plant, Room } from "@raea/motion/room";
import { Sound3D } from "@raea/motion/seedling";

export const CW = 1280;
export const CH = 720;
const FPS = 120;
const FRAMES = FPS * 60;
const SPEED = 0.03;

const ROOM = defineRoom({ width: 5, depth: 4, height: 2.6 });
const M = (n: string) => staticFile(n);
const CABINET = M("cabinet.glb"), PICTURE = M("picture.glb"), TREE = M("tree.glb");
/** the picture over the cabinet's middle, on the back wall */
const HUNG = hang(ROOM, "back", [{ src: PICTURE, name: "picture" }], { at: 2.5 });
const GARDEN = plant(ROOM, [{ src: TREE, count: 10, band: [1.5, 6], spacing: 1.6, scale: [0.8, 1.3], sway: 0.04 }], 7);
/** the radio on a half-width cabinet by the right wall (its stone 0.9 m up), playing */
const RADIO: [number, number, number] = [4.3, 0.965, 0.25];
const STICKER: [number, number, number] = [2.5, 0, 2.6];

interface Game { x: number; z: number; heading: number; lap: number }
const INITIAL: Game = { x: 1, z: 3, heading: 0, lap: 0 };

/** The loop a render walks when nobody plays: round the room's middle, corner to corner. */
const LOOP: [number, number][] = [[1, 3], [4, 3], [4, 1.6], [1, 1.6]];
function scripted(g: Game): string[] {
  const [tx, tz] = LOOP[g.lap % LOOP.length];
  if (Math.abs(tx - g.x) > 0.05) return [tx > g.x ? "ArrowRight" : "ArrowLeft"];
  if (Math.abs(tz - g.z) > 0.05) return [tz > g.z ? "ArrowDown" : "ArrowUp"];
  return ["next"];
}

function step(g: Game, input: FrameInput): Game {
  const keys = input.keys.length || (input as { live?: boolean }).live ? input.keys : scripted(g);
  if (keys.includes("next")) return { ...g, lap: g.lap + 1 };
  const dx = (keys.includes("ArrowRight") ? 1 : 0) - (keys.includes("ArrowLeft") ? 1 : 0);
  const dz = (keys.includes("ArrowDown") ? 1 : 0) - (keys.includes("ArrowUp") ? 1 : 0);
  const x = Math.max(0.4, Math.min(ROOM.width - 0.4, g.x + dx * SPEED)), z = Math.max(0.9, Math.min(ROOM.depth - 0.4, g.z + dz * SPEED));
  return { ...g, x, z, heading: dx || dz ? Math.atan2(dx, dz) : g.heading };
}

export const playtest: Playtest<Game> = {
  initial: INITIAL,
  step,
  actions: [["ArrowUp"], ["ArrowDown"], ["ArrowLeft"], ["ArrowRight"], []],
  progress: (g) => Math.min(1, g.lap / 8),
  outcome: () => null,
};

function Walk() {
  const g = useGameState(INITIAL, step);
  const near = Math.hypot(g.x - STICKER[0], g.z - STICKER[2]);
  // the camera by the front wall, following the player across the room toward the back wall's things: it turns as
  // the player goes, and the radio swings across the ears with it
  const cam: [number, number, number] = [1.0 + g.x * 0.6, 1.8, ROOM.depth - 0.25];
  return (
    <AbsoluteFill style={{ background: "#cfe3f0" }}>
      <Canvas3D>
        <PerspectiveCamera position={cam} lookAt={[g.x, 0.8, g.z - 1.2]} fov={70} />
        <AmbientLight intensity={0.5} />
        <DirectionalLight position={[3, 6, 5]} intensity={1.2} />
        <Room room={ROOM} openings={[{ wall: "right", at: 2, width: 1.4, bottom: 0.9, top: 2.1, kind: "window" }]} ceiling={false} />
        <Outside plants={GARDEN} centre={[ROOM.width / 2, ROOM.depth / 2]} size={30} />
        <Refinish materials={[{ material: "Wood", color: "#7f9479", roughness: 0.55 }]}>
          <Model name="cabinet" src={CABINET} position={onWall(ROOM.walls.back, 2.5)} />
        </Refinish>
        <Hang room={ROOM} hung={HUNG} />
        <Model name="radio-stand" src={CABINET} position={onWall(ROOM.walls.back, 4.3)} scale={[0.5, 1, 1]} />
        <Mesh name="radio-box" position={RADIO}><Box args={[0.22, 0.13, 0.1]} /><MeshStandardMaterial color="#b43c2c" roughness={0.4} /></Mesh>
        <Sound3D src={M("radio.wav")} at={RADIO} loop volume={0.4} />
        <Decal name="sticker" src={M("ring.png")} position={STICKER} size={0.5} spin={0.4} opacity={Math.min(1, near / 1.5)} />
        <Mesh name="player" position={[g.x, 0.35, g.z]} rotation={[0, g.heading, 0]}><Box args={[0.4, 0.7, 0.4]} /><MeshStandardMaterial color="#f0ebd8" roughness={0.4} /></Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="edit" component={Walk} width={CW} height={CH} fps={FPS} durationInFrames={FRAMES} />);
