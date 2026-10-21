// @raea/motion/daylight: a room's light from the day outside, in physical units, never hand-set intensities. A sky
// panorama (scene.sky reads where its sun is and how its light divides between the sun and the rest of the sky) is
// turned so its sun stands where the place and time put it (sunPosition), and a clear-sky model gives the sun's lux at
// that height; the panorama's own split then gives the sky's lux and its luminance. From that one Daylight:
//   <Daylight day={day} />                         the sun (lux, colour), the sky seen through the windows (cd/m²)
//   <WindowLight day={day} room={room} opening={o} />   the light a window lets in: its sky and ground, as a lamp
// so a shot's exposure comes from the light meter (`raea-motion scene light`) and every window agrees with the sky.
//
//   const day = daylight({ sky: staticFile("sky.hdr"), lat: 51.5, lon: -0.1, time: "2026-10-21T08:30:00+01:00" });
import { AmbientLight, DirectionalLight, Environment, SpotLight, type Vec3 } from "./3d";
import { hasNative, native } from "./native";
import { onWall, type Opening, type Room } from "./room";

/** The engine's light units (sdk/react/src/motion/scene_light.rs): a DirectionalLight of 1 is 8000 lux; a spot or point
 * light of 1 is 9e5 lumens, its candela a lumen over 4π; an Environment of 1 is its panorama times 1000 cd/m²; an
 * AmbientLight of 1 is 400 cd/m². */
export const UNITS = { lux: 8000, lumens: 9e5, envNits: 1000, ambientNits: 400 };

const rad = Math.PI / 180;

/** Where the sun stands at a place and moment (NOAA's low-precision solar position, good to a few tenths of a degree):
 * azimuth from north, clockwise, and elevation over the horizon, in degrees. `time` is ISO 8601 with its offset. */
export function sunPosition(lat: number, lon: number, time: string): { azimuth: number; elevation: number } {
  const ms = Date.parse(time);
  if (Number.isNaN(ms)) throw new Error(`sunPosition: time ${JSON.stringify(time)} is not ISO 8601 (2026-10-21T08:30:00+01:00)`);
  const n = ms / 86400000 + 2440587.5 - 2451545.0;
  const L = (280.46 + 0.9856474 * n) % 360, g = ((357.528 + 0.9856003 * n) % 360) * rad;
  const lambda = (L + 1.915 * Math.sin(g) + 0.02 * Math.sin(2 * g)) * rad, eps = (23.439 - 0.0000004 * n) * rad;
  const ra = Math.atan2(Math.cos(eps) * Math.sin(lambda), Math.cos(lambda)), dec = Math.asin(Math.sin(eps) * Math.sin(lambda));
  const gmst = (18.697374558 + 24.06570982441908 * n) % 24;
  const H = ((gmst + lon / 15) * 15) * rad - ra, phi = lat * rad;
  const el = Math.asin(Math.sin(phi) * Math.sin(dec) + Math.cos(phi) * Math.cos(dec) * Math.cos(H));
  const az = Math.atan2(-Math.sin(H), Math.tan(dec) * Math.cos(phi) - Math.sin(phi) * Math.cos(H));
  return { azimuth: ((az / rad) % 360 + 360) % 360, elevation: el / rad };
}

/** The world direction toward a sun at `azimuth` and `elevation` (degrees), north along -z turned by `north` (radians
 * about +y: where the set's north is), east along +x. */
export function sunDirection(azimuth: number, elevation: number, north = 0): Vec3 {
  const [a, e] = [azimuth * rad, elevation * rad];
  const d: Vec3 = [Math.sin(a) * Math.cos(e), Math.sin(e), -Math.cos(a) * Math.cos(e)];
  const [c, s] = [Math.cos(north), Math.sin(north)];
  return [d[0] * c + d[2] * s, d[1], -d[0] * s + d[2] * c];
}

/** A clear sky's direct sun at `elevation` degrees, lux on a surface facing it: the solar constant's light (1361 W/m²
 * at 93 lm/W) through the air mass (Kasten and Young) by Meinel's attenuation. */
export function clearSun(elevation: number): number {
  if (elevation <= 0) return 0;
  const am = 1 / (Math.sin(elevation * rad) + 0.50572 * Math.pow(elevation + 6.07995, -1.6364));
  return 1361 * 93 * Math.pow(0.7, Math.pow(am, 0.678));
}

/** A colour temperature as a light's colour (Tanner Helland's fit to the Planckian locus), sRGB hex. */
export function kelvin(k: number): string {
  const t = Math.min(Math.max(k, 1000), 40000) / 100;
  const r = t <= 66 ? 255 : 329.698727446 * Math.pow(t - 60, -0.1332047592);
  const g = t <= 66 ? 99.4708025861 * Math.log(t) - 161.1195681661 : 288.1221695283 * Math.pow(t - 60, -0.0755148492);
  const b = t >= 66 ? 255 : t <= 19 ? 0 : 138.5177312231 * Math.log(t - 10) - 305.0447927307;
  const h = (v: number) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, "0");
  return `#${h(r)}${h(g)}${h(b)}`;
}

interface SkyRead {
  sun: { dir: Vec3; elevation: number; horizontal: number; cct: number | null } | null;
  sky: { horizontal: number; luminance: number; cct: number | null };
}

/** A sky panorama read by the engine: its sun (if it has one) and its light's split (scene.sky). */
export function readSky(src: string, rotation = 0): SkyRead {
  if (!hasNative()) throw new Error("readSky: reading a panorama needs the raea engine (raea-motion render)");
  return native("scene.sky", { src, rotation }) as SkyRead;
}

/** The day outside, in physical units, for one set. */
export interface Day {
  sky: string;
  /** the panorama's turn that puts its sun where the day's is (radians, as <Environment rotation>) */
  rotation: number;
  /** toward the sun, world */
  sunDir: Vec3;
  /** the sun's lux on a surface facing it, its colour temperature */
  sunLux: number;
  sunK: number;
  /** the sun's and the sky's lux on level ground outside */
  sunGround: number;
  skyGround: number;
  /** the sky's mean luminance (cd/m²) and colour temperature */
  skyNits: number;
  skyK: number;
  /** cd/m² per unit of the panorama */
  scale: number;
  /** the sun's elevation (degrees) the light was made for, and the panorama's own (a different height shows in its
   * shadows' length outside, never in the room's light) */
  elevation: number;
  skyElevation: number | null;
}

/** The day from a sky panorama and, to put the sun where it was, a place and time (else the panorama's own sun). An
 * overcast panorama (no sun) is lit as an overcast sky of `overcastLux` on the ground (10 000 by default). */
export function daylight(o: { sky: string; lat?: number; lon?: number; time?: string; north?: number; overcastLux?: number }): Day {
  const read = readSky(o.sky);
  const sun = read.sun;
  let rotation = 0, elevation = sun?.elevation ?? 0, sunDir: Vec3 = sun?.dir ?? [0, 1, 0];
  if (o.time !== undefined) {
    if (o.lat === undefined || o.lon === undefined) throw new Error("daylight: a time needs lat and lon");
    const p = sunPosition(o.lat, o.lon, o.time);
    if (p.elevation <= 0) throw new Error(`daylight: the sun is ${p.elevation.toFixed(1)} degrees under the horizon at ${o.time}`);
    elevation = p.elevation;
    sunDir = sunDirection(p.azimuth, p.elevation, o.north ?? 0);
    // turn the panorama so its sun's bearing is the day's (Quat::from_rotation_y: a turn r takes (x, z) to
    // (x cos r + z sin r, -x sin r + z cos r), so a bearing atan2(x, z) gains r)
    if (sun) rotation = Math.atan2(sunDir[0], sunDir[2]) - Math.atan2(sun.dir[0], sun.dir[2]);
  }
  const sunLux = sun ? clearSun(elevation) : 0;
  const sunGround = sunLux * Math.sin(elevation * rad);
  const skyGround = sun ? (sunGround * read.sky.horizontal) / sun.horizontal : (o.overcastLux ?? 10000);
  const scale = skyGround / read.sky.horizontal;
  return {
    sky: o.sky, rotation, sunDir, sunLux, sunK: sun?.cct ?? 6500, sunGround, skyGround,
    skyNits: scale * read.sky.luminance, skyK: read.sky.cct ?? 6500, scale, elevation, skyElevation: sun?.elevation ?? null,
  };
}

/** The room's bounce: the light every surface gets from the room's other surfaces once the windows' light has struck
 * them, as luminance (a daylight factor `indoor` of the day's light on the ground, over π) and colour (the sun's and the
 * sky's, mixed in mired by their shares of it). Not the sky's colour: a room's fill is its sunlit floor and walls. */
export function bounce(day: Day, indoor = 0.02): { nits: number; kelvin: number } {
  const all = day.sunGround + day.skyGround;
  const mired = (day.sunGround * (1e6 / day.sunK) + day.skyGround * (1e6 / day.skyK)) / all;
  return { nits: (indoor * all) / Math.PI, kelvin: 1e6 / mired };
}

/** The day's sun and sky: the sun as a shadowed directional light at its lux and colour, the panorama as the sky seen
 * through the windows at its true brightness (it lights nothing: an Environment is not shadowed, and would light the
 * room as if it had no walls), the room's bounce (`indoor`, its daylight factor, 2% by default) as an even fill; the
 * windows' own light comes from <WindowLight>. */
export function Daylight({ day, indoor = 0.02, softShadow = 0.6 }: { day: Day; indoor?: number; softShadow?: boolean | number }) {
  const at: Vec3 = [day.sunDir[0] * 30, day.sunDir[1] * 30, day.sunDir[2] * 30];
  const b = bounce(day, indoor);
  return (
    <>
      {day.sunLux > 0 && <DirectionalLight name="sun" position={at} target={[0, 0, 0]} intensity={day.sunLux / UNITS.lux} color={kelvin(day.sunK)} castShadow softShadow={softShadow} />}
      <Environment src={day.sky} rotation={day.rotation} intensity={0} background backgroundIntensity={day.scale / UNITS.envNits} />
      <AmbientLight name="bounce" intensity={b.nits / UNITS.ambientNits} color={kelvin(b.kelvin)} />
    </>
  );
}

/** What a window lets in besides the sun: its sky above and the ground below (`ground` its reflectance), seen through
 * glass of `transmit`, as a lamp in the opening that faces into the room with a window's candela on its axis (a
 * Lambertian pane: luminance times area). A bay's three faces count as its mouth. */
export function windowLight(day: Day, room: Room, o: Opening, { transmit = 0.75, ground = 0.2 } = {}) {
  const f = room.walls[o.wall];
  const area = o.kind === "door" ? 0 : o.width * (o.top - o.bottom);
  const groundNits = (ground * (day.sunGround + day.skyGround)) / Math.PI;
  const nits = (day.skyNits + groundNits) / 2;
  const candela = transmit * nits * area;
  const y = (o.bottom + o.top) / 2;
  return {
    position: onWall(f, o.at, o.kind === "bay" ? 0.05 : -0.02, y),
    target: onWall(f, o.at, 3, Math.max(0.3, y - 1.2)),
    intensity: (candela * 4 * Math.PI) / UNITS.lumens,
    color: kelvin((day.skyK + day.sunK) / 2),
    candela,
  };
}

/** A window's light as a lamp (windowLight), wide and soft, shadowed. */
export function WindowLight({ day, room, opening, transmit, ground, name }: { day: Day; room: Room; opening: Opening; transmit?: number; ground?: number; name?: string }) {
  const w = windowLight(day, room, opening, { transmit, ground });
  return <SpotLight name={name ?? `window-${opening.wall}-${opening.at.toFixed(2)}`} position={w.position} target={w.target} intensity={w.intensity} color={w.color} angle={Math.PI / 2} penumbra={1} castShadow />;
}

/** The exposure a camera sets for the room, as <ToneMapping exposure>: an incident meter's EV100 (log2 E·100/250, as
 * `raea-motion scene light` reads it) on the room's light, its bounce (`indoor`, as Daylight's) plus `extra` lux, and
 * the engine's exposure for it (EV100 = 9.7 - exposure). The windows burn out, as a room's do in a real camera. */
export function roomExposure(day: Day, indoor = 0.02, extra = 0): number {
  const ev = Math.log2(((indoor * (day.sunGround + day.skyGround) + extra) * 100) / 250);
  return 9.7 - ev;
}
