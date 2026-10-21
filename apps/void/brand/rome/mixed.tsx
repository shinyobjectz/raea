// Mixed media for the Ides of March: real footage and drawings among the 3D. Each piece is made by a script in tools/
// from sources in media/mixed/SOURCES.txt (Wikimedia Commons, public domain or CC BY), and drawn here.
//
//   <Footage>      fire from a real bonfire, standing in the scene (tools/fire.py)
//   <SandDrawing>  a painting traced into sand by an unseen finger, then blown away (tools/sand.py)
//   <DustShafts>   light falling through a dark room, dust turning in it
//   <Haze>         slow smoke or haze drifting across a scene
import { AbsoluteFill, Sequence, Video, staticFile } from "@raea/motion";
import { Group, ImagePlane, Mesh, MeshStandardMaterial, PointLight, Sphere } from "@raea/motion/3d";
import FLAMES from "./media/mixed/fire/flames.json";
import PYRE from "./media/mixed/fire/pyre.json";

type V = [number, number, number];
const FPS = 30;
const rnd = (i: number) => {
  const x = Math.sin(i * 127.1 + 311.7) * 43758.5453;
  return x - Math.floor(x);
};
/** the yaw (radians, about y) that turns a picture at `at` to face `face` */
const yawTo = (at: V, face: V) => Math.atan2(face[0] - at[0], face[2] - at[2]);

// ---------------------------------------------------------------- fire from footage

type Clip = { frames: number; w: number; h: number; foot: number; glow: number[] };
const CLIPS: Record<"pyre" | "flames", Clip> = { pyre: PYRE, flames: FLAMES };

/** Fire from footage (Feu.ogv, CC BY 2.0), looping every 2 s, standing in the scene: one ImagePlane per frame of the
 * loop (each loaded once; only the current one is drawn), turned about y to face `face` (pass the camera's position),
 * its foot on `at`. Unlit and keyed to straight alpha, it adds light over what's behind it like a real fire, and with
 * <Bloom> its white core blooms. A point light at its heart flickers with the footage's own brightness.
 *
 *   clip "pyre"    the whole burning stack of timber: a pyre by itself (the funeral, a wide shot)
 *   clip "flames"  only the flames, soft at the foot: set them on a pyre built in 3D (shots.tsx's planks)
 *   height         metres from foot to the flames' tips (a pyre ~5, flames over a 1 m stack ~3)
 *   light          the point light's intensity (1 = a full sun at 3 m; 0.06 lights a temple front 10 m off at
 *                  night as a pyre would), 0 for none
 *   phase          seconds to offset the loop by, so two fires don't flicker together */
export function Footage({ t, name, clip = "pyre", at, height, face, light = 0.06, color = "#ff9a4a", phase = 0 }: { t: number; name: string; clip?: "pyre" | "flames"; at: V; height: number; face: V; light?: number; color?: string; phase?: number }) {
  const c = CLIPS[clip];
  const k = ((Math.floor((t + phase) * FPS) % c.frames) + c.frames) % c.frames;
  const width = (height * c.w) / c.h;
  const y = at[1] + height * (c.foot - 0.5);
  return (
    <Group name={`${name}-fire`}>
      <ImagePlane
        key={k}
        name={`${name}-${clip}-${k}`}
        src={staticFile(`mixed/fire/${clip}_${String(k).padStart(3, "0")}.png`)}
        width={width}
        height={height}
        position={[at[0], y, at[2]]}
        rotation={[0, yawTo(at, face), 0]}
      />
      {light > 0 ? <PointLight name={`${name}-firelight`} position={[at[0], at[1] + height * 0.3, at[2]]} intensity={light * c.glow[k]} color={color} /> : null}
    </Group>
  );
}

// ---------------------------------------------------------------- drawings in the sand

/** A drawing traced into sand (tools/sand.py renders it: the grooves raked by a low light, dust kicked off the
 * fingertip, then a wind that fills the lines in and carries the sand off), played full frame over everything from
 * `from` (frames). Each drawing's MP4 is 1920x1080 at 30 fps, its timing in media/mixed/sand/<id>.json:
 * {seconds, drawn, wind} (when the last line is finished and when the wind starts, in seconds).
 *
 *   id     rubicon (Pinelli, 1819), pompey (Baltard's plan of the Theatre of Pompey, 1837), camuccini (the murder,
 *          1805), gerome (The Death of Caesar, 1867), antony (the funeral oration), coin (the EID MAR denarius)
 *   from   the composition frame it starts on; it runs its full length unless `seconds` cuts it short
 * Put it in a scene's page (over the Canvas3D, under the letterbox), or give it a scene of its own. */
export function SandDrawing({ id, from, seconds }: { id: string; from: number; seconds?: number }) {
  return (
    <Sequence name={`sand-${id}`} from={from} durationInFrames={seconds === undefined ? undefined : Math.round(seconds * FPS)} layout="none">
      <AbsoluteFill style={{ overflow: "hidden" }}>
        <Video name={`sand-${id}`} src={staticFile(`mixed/sand/${id}.mp4`)} muted style={{ position: "absolute", width: "100%", height: "100%", objectFit: "cover" }} />
      </AbsoluteFill>
    </Sequence>
  );
}

// ---------------------------------------------------------------- atmosphere

/** Light falling through a dark interior (a high window, a door, the oculus) as soft beams, with dust motes drifting
 * through them: a mote catches the light inside a beam and all but vanishes outside one. The beams are pictures
 * (media/mixed/air/beam.png, made by tools/air.py) facing `face`; the motes are seeded, so a render equals its play.
 *
 *   at      the floor point under the middle of the beams
 *   beams   each beam's offset from `at` along x and z (m), and its lean (radians, + leans its top toward -x)
 *   height  how tall the beams are (m)
 *   motes   how many motes (~120 for a room); they fill a box `spread` wide around the beams
 * The beams lean along x, so frame them with the camera looking roughly along -z. They don't light anything: add the
 * light they stand for, a SpotLight from where they come in (at.x + x - tan(lean) * height, height) down onto what
 * they fall on, and set `at` so the beams pass over that (a beam's axis at height y is at.x + x - tan(lean) * y).
 * A beam hides a little of what's behind it as it lights it (a picture can't only add light), so set beams just
 * behind what they fall on rather than in front of it.
 * Suits the curia, the plot's candle room, Decimus's door, the dream (mixed-demo.tsx's `air` is a bust in one). */
export function DustShafts({ t, name, at, face, beams = [{ x: 0, z: 0, lean: 0.35 }], height = 5, motes = 120, spread = 4, color = "#ffd9a0" }: { t: number; name: string; at: V; face: V; beams?: { x: number; z: number; lean: number; width?: number }[]; height?: number; motes?: number; spread?: number; color?: string }) {
  const yaw = yawTo(at, face);
  // where a beam's axis is at height y (its foot at the floor, leaning)
  const axis = (b: { x: number; z: number; lean: number }, y: number): [number, number] => [at[0] + b.x - Math.tan(b.lean) * (y - at[1]), at[2] + b.z];
  return (
    <Group name={`${name}-air`}>
      {beams.map((b, i) => {
        const w = b.width ?? 1.1;
        const [cx, cz] = axis(b, at[1] + height / 2);
        return (
          <ImagePlane key={`b${i}`} name={`${name}-beam-${i}`} src={staticFile("mixed/air/beam.png")} width={w} height={height / Math.cos(b.lean)}
            position={[cx, at[1] + height / 2, cz]} rotation={[0, yaw, b.lean]} />
        );
      })}
      {Array.from({ length: motes }, (_, i) => {
        // each mote wanders on slow seeded sines, sinking a little, wrapping in a box about the beams
        const sx = rnd(i * 3.1) * spread, sz = rnd(i * 5.7) * spread * 0.6, sy = rnd(i * 7.3) * height;
        const x = at[0] - spread / 2 + ((sx + Math.sin(t * 0.21 + i) * 0.35 + t * 0.03) % spread + spread) % spread;
        const z = at[2] - spread * 0.3 + sz + Math.cos(t * 0.17 + i * 1.7) * 0.2;
        const y = at[1] + ((sy - t * 0.05 + Math.sin(t * 0.3 + i * 2.3) * 0.15) % height + height) % height;
        // how deep inside a beam it is: lit there, near dark outside
        const inBeam = Math.max(0, ...beams.map((b) => {
          const [bx] = axis(b, y);
          return 1 - Math.min(1, Math.abs(x - bx) / ((b.width ?? 1.1) * 0.5));
        }));
        const glint = 0.6 + 0.4 * Math.sin(t * (1.5 + rnd(i) * 2) + i);
        return (
          <Mesh key={`m${i}`} name={`${name}-mote-${i}`} position={[x, y, z]}>
            <Sphere args={[0.004 + rnd(i * 11) * 0.006]} />
            <MeshStandardMaterial color="#000000" emissive={color} emissiveIntensity={(0.03 + 5 * inBeam * inBeam) * glint} />
          </Mesh>
        );
      })}
    </Group>
  );
}

/** Slow haze or smoke drifting across a scene: a few soft sheets of it (media/mixed/air/haze_*.png, tools/air.py) at
 * different depths, facing `face`, each sliding sideways at its own pace. A sheet is two copies of a picture that
 * tiles left to right, side by side, so it slides on forever without a seam; make `width` wider than the view at that
 * depth, since a sheet's outer ends are hard. Unlit, so it reads as lit air. `tint`: "warm" (firelight, the funeral,
 * the forum at dusk) or "cool" (moonlight, the omens).
 *
 *   at      the middle of the haze; sheets spread `depth` m toward and away from `face`
 *   width   one copy's width (m); a sheet is twice it, and half as tall as one copy
 *   drift   metres a second the nearest sheet slides (the farther, the slower) */
export function Haze({ t, name, at, face, width = 14, depth = 6, sheets = 3, drift = 0.12, tint = "warm" }: { t: number; name: string; at: V; face: V; width?: number; depth?: number; sheets?: number; drift?: number; tint?: "warm" | "cool" }) {
  const yaw = yawTo(at, face);
  const [dx, dz] = [Math.sin(yaw), Math.cos(yaw)]; // toward the camera
  const [sx, sz] = [Math.cos(yaw), -Math.sin(yaw)]; // sideways across the frame
  return (
    <Group name={`${name}-haze`}>
      {Array.from({ length: sheets }, (_, i) => {
        const d = sheets > 1 ? (i / (sheets - 1) - 0.5) * depth : 0;
        const speed = drift * (0.5 + (0.5 * (i + 1)) / sheets);
        // the pair's offset: 0..width, then back to 0, where it looks the same
        const slide = (((rnd(i * 9) * width + t * speed) % width) + width) % width;
        return [0, 1].map((c) => {
          const s = slide + (c - 1) * width; // the pair always covers -width/2..width/2 about `at`
          return (
            <ImagePlane key={`${i}-${c}`} name={`${name}-haze-${i}-${c}`} src={staticFile(`mixed/air/haze_${tint}_${i % 3}.png`)} width={width}
              position={[at[0] + dx * d + sx * s, at[1], at[2] + dz * d + sz * s]} rotation={[0, yaw, 0]} />
          );
        });
      })}
    </Group>
  );
}
