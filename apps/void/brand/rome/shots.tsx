// The scenes of the Ides of March, each from one narration word to another (time.ts). A scene is its camera, its
// 3D (models and its own lights, named, since they come and go) and its page (type over the picture).
// Models: the ruins pack (media/ruins, the founder's licence), Poly Haven's CC0 scans (media/statues), and the museum
// scans in assets/ (assets/SOURCES.txt). Every model is drawn at its real size in metres.
import type { ReactNode } from "react";
import { staticFile } from "@raea/motion";
import { AmbientLight, Box, Cone, Cylinder, DirectionalLight, Fire, FogVolume, Group, Mesh, MeshPhysicalMaterial, MeshStandardMaterial, Model, Plane, PointLight, Sphere, SpotLight, Torus, manifold, type Solid } from "@raea/motion/3d";
import SHARDS from "./media/statues/shards/marble_bust_01_shards.json";
import { Haze, SandDrawing } from "./mixed";
import { Caps, Italic, Letters, Reveal, Rule, ash, blood as bloodInk, capsW, gold, italW, ivory, serif, shade, shown, wrapItal } from "./type";
import { BAR, FPS, TL, clamp01, ease, lerp, mix3, rnd, smooth } from "./time";

type V = [number, number, number];
export type Cam = { pos: V; look: V; fov?: number; focus?: number };
/** the omens' lightning: when each bolt flashes (film seconds) and how bright (1 = the first); the sound keys to it */
export const LIGHTNING: { at: number; k: number }[] = [];
/** when a quote on screen says what the narrator says (the captions step aside): film seconds, from and to */
export const QUOTED: [number, number][] = [];
export type Scene = { name: string; from: number; to: number; dip?: number; cam: (t: number) => Cam; draw: (t: number) => ReactNode; page?: (t: number) => ReactNode };

const W = (line: string, word: string, k = 0) => TL.word(line, word, k);
LIGHTNING.push({ at: W("omens", "warnings"), k: 1 }, { at: W("omens", "Spurinna"), k: 0.6 }, { at: W("omens", "Ides"), k: 0.8 });
QUOTED.push([W("street", "The", 0) - 0.1, W("street", "Yes,") - 0.1], [W("street", "They", 0) - 0.1, W("street", "Someone") - 0.2]);
const dist = (a: V, b: V) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
/** a camera from a to b over the scene, eased, its focus on what it looks at */
const dolly = (s: { from: number; to: number }, a: Cam, b: Cam) => (t: number): Cam => {
  const u = smooth((t - s.from) / (s.to - s.from));
  const pos = mix3(a.pos, b.pos, u), look = mix3(a.look, b.look, u);
  return { pos, look, fov: lerp(a.fov ?? 35, b.fov ?? 35, u), focus: dist(pos, look) };
};
/** The same shot with what it looks at on the right third (the camera and its aim slid left together), leaving the
 * left of the picture for a line of type. */
const onRight = (cam: (t: number) => Cam) => (t: number): Cam => {
  const c = cam(t);
  const d = [c.look[0] - c.pos[0], c.look[2] - c.pos[2]];
  const n = Math.hypot(d[0], d[1]) || 1;
  const right = [-d[1] / n, d[0] / n]; // the camera's right, on the ground plane
  const half = Math.atan(Math.tan(((c.fov ?? 35) * Math.PI) / 360) * (16 / 9)); // half the horizontal field of view
  const k = (dist(c.pos, c.look) * Math.tan(half)) / 3;
  const shift = (v: V): V => [v[0] - right[0] * k, v[1], v[2] - right[1] * k];
  return { ...c, pos: shift(c.pos), look: shift(c.look) };
};
/** a hand-held drift, small */
const hand = (t: number, k = 1): V => [Math.sin(t * 0.9) * 0.012 * k + Math.sin(t * 2.3) * 0.004 * k, Math.sin(t * 1.1 + 1) * 0.01 * k, Math.sin(t * 0.7 + 2) * 0.008 * k];
const add = (a: V, b: V): V => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];

// ---------------------------------------------------------------- the props

const ruin = (n: string) => staticFile(`ruins/${n}.glb`);
const statue = (n: string) => staticFile(`statues/${n}.glb`);
const cast = (n: string) => staticFile(`cast/${n}.glb`);
/** a Poly Haven model (CC0; media/props, tools/polyhaven.py) */
const prop = (n: string) => staticFile(`props/${n}/${n}.gltf`);

/** the busts and statues by part, each with its native height (m) so it can be drawn at a chosen size, and where its
 * eyes are as a fraction of that height (measured on a ruler render), so a camera aims at the face, not the chest */
export const CAST = {
  caesar: { src: cast("caesar_bust_fitzwilliam"), h: 0.275, eye: 0.55 },
  // the Capitoline Brutus: traditionally Lucius Junius Brutus, the Republic's founder, whom Marcus claimed as ancestor
  brutus: { src: cast("brutus_capitoline_bust"), h: 0.806, eye: 0.85 },
  senator: { src: cast("marcus_aurelius_bust"), h: 0.606, eye: 0.55 },
  senator2: { src: cast("nero_head"), h: 0.4, eye: 0.51 },
  senator3: { src: cast("caesar_bust_leiden"), h: 0.315, eye: 0.59 },
  augustus: { src: cast("augustus_prima_porta"), h: 2.23, eye: 0.95 },
  // Pompey's statue in his Curia, at whose foot Caesar fell: a cuirassed emperor (Hadrian) stands in for it
  pompey: { src: cast("hadrian_statue"), h: 2.3, eye: 0.9 },
  // the horses Caesar had freed at the Rubicon, that wept and would not graze (Suetonius 81)
  horse: { src: statue("horse_statue_01"), h: 0.22, eye: 0.8 },
};

/** where a figure's eyes are, drawn at `at` and `h` tall: what a camera close on it looks at */
const eyeOf = (who: keyof typeof CAST, at: V, h: number): V => [at[0], at[1] + CAST[who].eye * h, at[2]];
function Figure({ who, name, at, yaw = 0, h }: { who: keyof typeof CAST; name: string; at: V; yaw?: number; h: number }) {
  const c = CAST[who];
  return <Model name={name} src={c.src} position={at} rotation={[0, yaw, 0]} scale={h / c.h} />;
}

/** a marble block: Poly Haven's marble_01 (CC0), tinted by `color` */
const marble = (name: string, s: Solid, at: V, color = "#d9d2c5") => (
  <Mesh name={name} shape={s} position={at}>
    <MeshStandardMaterial color={color} roughness={0.8} {...tex("marble_01")} />
  </Mesh>
);
let plinthS: Solid | null = null;
const plinth = () => (plinthS ??= manifold.roundedBox(0.62, 1.1, 0.62, 0.02).move(0, 0.55, 0));

/** a Poly Haven texture set (media/tex/<id>: colour, normal, roughness and ambient occlusion; CC0) as a material's
 * props, tiled `repeat` times across each face (a mesh's UVs span its bounds, so repeat = size / tile) */
const tex = (id: string, repeat: [number, number] = [1, 1]) => ({
  map: staticFile(`tex/${id}/diff.jpg`), normalMap: staticFile(`tex/${id}/nor_gl.jpg`), roughnessMap: staticFile(`tex/${id}/rough.jpg`), aoMap: staticFile(`tex/${id}/ao.jpg`), repeat,
});
/** the ground: a texture set tiled every `tile` metres, tinted by `color`; `wet` scales its roughness down */
function Floor({ name, color = "#a89a88", size = 400, surface = "large_sandstone_blocks_01", tile = 3, wet = 1 }: { name: string; color?: string; size?: number; surface?: string; tile?: number; wet?: number }) {
  return (
    <Mesh name={name} rotation={[-Math.PI / 2, 0, 0]}>
      <Plane args={[size, size]} />
      <MeshStandardMaterial color={color} roughness={wet} {...tex(surface, [size / tile, size / tile])} />
    </Mesh>
  );
}

/** embers: small glowing cells drifting up and out, seeded */
function Embers({ t, n, at, spread, rise, color = "#ff8a3d", name, glow = 6 }: { t: number; n: number; at: V; spread: number; rise: number; color?: string; name: string; glow?: number }) {
  return (
    <>
      {Array.from({ length: n }, (_, i) => {
        const life = 2.5 + rnd(i * 3) * 3;
        const u = ((t + rnd(i * 7) * life) % life) / life;
        const x = at[0] + (rnd(i * 11) - 0.5) * spread + Math.sin(t * 1.3 + i) * 0.15 * u;
        const z = at[2] + (rnd(i * 13) - 0.5) * spread + Math.cos(t * 1.1 + i) * 0.15 * u;
        const y = at[1] + u * rise;
        return (
          <Mesh key={i} name={`${name}-${i}`} position={[x, y, z]}>
            <Sphere args={[0.01 + rnd(i * 17) * 0.012]} />
            <MeshStandardMaterial color="#000000" emissive={color} emissiveIntensity={glow * Math.sin(Math.PI * u)} />
          </Mesh>
        );
      })}
    </>
  );
}

// ---------------------------------------------------------------- the pyre: benches and tables, the bier, the fire

/** Caesar's pyre (Suetonius 84: the crowd heaped benches and tables and burned him where he lay): five courses of
 * planks laid crosswise and a bier draped in white on top, charring with `burn` (0..1). The flames are footage. */
function Pyre({ t, at, burn, p }: { t: number; at: V; burn: number; p: string }) {
  const [x0, y0, z0] = at;
  // logs (Poly Haven's dead tree trunk, 3 m long and 0.29 thick, along X), laid crosswise in five courses of four
  const logs: ReactNode[] = [];
  for (let c = 0; c < 5; c++) {
    for (let k = 0; k < 4; k++) {
      const along = (k - 1.5) * 0.5 + (rnd(c * 17 + k) - 0.5) * 0.06;
      const y = y0 + 0.13 + c * 0.25;
      const pos: V = c % 2 ? [x0 + along, y, z0] : [x0, y, z0 + along];
      logs.push(<Model key={`${c}-${k}`} name={`${p}-log-${c}-${k}`} src={prop("dead_tree_trunk")} position={pos} scale={[0.8, 0.9, 0.9]} rotation={[rnd(c * 9 + k) * 6, (c % 2 ? Math.PI / 2 : 0) + (rnd(c * 5 + k) - 0.5) * 0.1, 0]} />);
    }
  }
  const top = y0 + 5 * 0.25;
  return (
    <Group name={`${p}-pyre`}>
      {logs}
      {/* the bier, and the body wrapped in its shroud */}
      <Mesh name={`${p}-bier`} position={[x0, top + 0.06, z0]}>
        <Box args={[2.3, 0.1, 0.9]} />
        <MeshStandardMaterial color="#6a4c34" {...tex("wood_planks_dirt", [1.2, 0.5])} />
      </Mesh>
      <Mesh name={`${p}-body`} shape={(shroudS ??= manifold.roundedBox(1.8, 0.26, 0.5, 0.12))} position={[x0, top + 0.24, z0]}>
        <MeshStandardMaterial color="#b9b0a0" roughness={1} emissive="#ff5a14" emissiveIntensity={burn * burn * 0.06} />
      </Mesh>
      {/* embers glowing in the stack as it catches */}
      <PointLight name={`${p}-coals`} position={[x0, y0 + 0.4, z0 + 0.9]} intensity={0.25 * burn * (0.7 + 0.3 * Math.sin(t * 7))} color="#ff4a10" />
    </Group>
  );
}
let shroudS: Solid | null = null;

// ---------------------------------------------------------------- the forum: the ruins laid out as one city

function Forum({ p }: { p: string }) {
  return (
    <>
      <Floor name={`${p}-ground`} color="#9c8e7c" surface="large_sandstone_blocks_01" tile={3} />
      <Model name={`${p}-temple`} src={ruin("007")} position={[0, 0, 0]} rotation={[0, 0.35, 0]} />
      <Model name={`${p}-arch`} src={ruin("001")} position={[-30, 0, 18]} rotation={[0, 1.2, 0]} />
      <Model name={`${p}-portico`} src={ruin("004")} position={[24, 0, 10]} rotation={[0, -0.6, 0]} />
      <Model name={`${p}-column`} src={ruin("006")} position={[14, 0, 30]} rotation={[0, 0.2, 0]} />
      <Model name={`${p}-ring`} src={ruin("005")} position={[-26, 0, -14]} />
      <Model name={`${p}-arch2`} src={ruin("009")} position={[-6, 0, 36]} rotation={[0, 0.1, 0]} />
    </>
  );
}

// ---------------------------------------------------------------- the shatter (the bust, pre-fractured)

const SH = (SHARDS as { centres: V[] }).centres;
function warp(t: number, tHit: number) {
  // full speed for 0.1 s, then down to a fifth over 0.25 s
  const dt = t - tHit;
  if (dt <= 0) return 0;
  if (dt < 0.1) return dt;
  if (dt < 0.35) { const k = (dt - 0.1) / 0.25; return 0.1 + 0.25 * (k - 0.4 * k * k); }
  return 0.1 + 0.25 * 0.6 + (dt - 0.35) * 0.2;
}
function Shatter({ t, tHit, at, h, name }: { t: number; tHit: number; at: V; h: number; name: string }) {
  const tau = warp(t, tHit);
  const hit: V = [0.3 * h, 0.7 * h, 0.25 * h];
  return (
    <Group position={at}>
      {SH.map((c0, i) => {
        const c: V = [c0[0] * h, c0[1] * h, c0[2] * h];
        const off: V = [c[0] - hit[0], c[1] - hit[1], c[2] - hit[2]];
        const dd = Math.hypot(...off) || 1;
        const near = Math.exp(-dd * 1.2);
        const v = [0, 1, 2].map((k) => [-0.7, 0.1, -0.5][k] * 2.8 * near + (off[k] / dd) * (1.4 * near + 0.08) + (rnd(i * (3 + 2 * k)) - 0.5) * (0.1 + near));
        const sp = 5 * (0.15 + near);
        const pos: V = [c[0] + v[0] * tau, c[1] + v[1] * tau - 1.4 * tau * tau, c[2] + v[2] * tau];
        return <Model key={i} name={`${name}-${i}`} src={staticFile(`statues/shards/marble_bust_01_shard${i}.glb`)} position={pos} rotation={[(rnd(i * 11) - 0.5) * sp * tau, (rnd(i * 13) - 0.5) * sp * tau, (rnd(i * 17) - 0.5) * sp * tau]} scale={h} />;
      })}
    </Group>
  );
}

// ---------------------------------------------------------------- type

// Type sits inside the picture, never in the letterbox, away from the subject (the shots keep it in the middle or on
// the right third): type.tsx sets it. A full-frame line (Quote, the wounds) dims the whole picture under it first.
/** A name or a date in a top corner of the picture: tracked capitals, a gold rule, an italic line; `slot` 1 sits
 * under slot 0. */
function Label({ t, at, until, text, sub, side = "right", slot = 0 }: { t: number; at: number; until?: number; text: string; sub?: string; side?: "left" | "right"; slot?: number }) {
  const u = shown(t, at, until);
  const w = Math.round(Math.max(capsW(text, 34, 0.3), sub ? italW(sub, 38) : 0)) + 8;
  return (
    <Reveal u={u} w={w} h={sub ? 104 : 38} top={BAR + 64 + slot * 150} align={side} pool={0.32} {...(side === "left" ? { left: 120 } : { right: 120 })}>
      <Caps size={34}><Letters text={text} t={t} at={at + 0.15} /></Caps>
      {sub ? <><Rule u={u} align={side} /><Italic size={38} weight={500}>{sub}</Italic></> : null}
    </Reveal>
  );
}
/** The picture between the bars darkened to 80%, for a line set over the middle of the frame. */
export function Scrim({ o, k = 0.8 }: { o: number; k?: number }) {
  return o > 0 ? <div style={{ position: "absolute", left: 0, top: BAR, width: 1920, height: 1080 - 2 * BAR, background: `rgba(0,0,0,${k * o})` }} /> : null;
}
/** A line said in the picture: large light italic in the left of the frame (left 160, 820 wide), beside the subject
 * the shot keeps on the right third, a gold quotation mark hung in the margin; the picture dims under it. */
function Quote({ t, a, b, text, size = 88 }: { t: number; a: number; b: number; text: string; size?: number }) {
  const u = shown(t, a - 0.25, b, 1.2);
  if (u <= 0) return null;
  const body = text.replace(/^[“"]|[”"]$/g, "");
  const quoted = body !== text;
  // an inscription (all capitals) is cut in tracked capitals; speech is set in italic, its quotation mark hung
  const carved = body === body.toUpperCase();
  const pad = quoted ? Math.round(size * 0.5) : 0, col = 820 - pad;
  const lines = carved ? Math.ceil(capsW(body, size, 0.14) / col) : wrapItal(body, size, col - 20);
  const h = Math.round(lines * size * 1.12);
  return (
    <>
      <Scrim o={smooth(u / 0.4)} k={0.5} />
      <Reveal u={u} w={820} h={h} left={160 - pad} top={Math.round(540 - h / 2)} pool={0.45}>
        <div style={{ position: "relative", paddingLeft: pad }}>
          {quoted ? <div style={{ position: "absolute", left: 0, top: -0.12 * size, fontFamily: serif, fontWeight: 300, fontSize: size * 1.25, lineHeight: 1, color: gold, textShadow: shade }}>“</div> : null}
          {carved
            ? <div style={{ width: col }}><Caps size={size} weight={300} track={0.14}><span style={{ lineHeight: 1.12 }}>{body}</span></Caps></div>
            : <Italic size={size} weight={300} color={ivory} style={{ width: Math.min(col, Math.ceil((italW(body, size) / lines) * 1.15)), lineHeight: 1.08 }}>{quoted ? body + "”" : body}</Italic>}
        </div>
      </Reveal>
    </>
  );
}

// ---------------------------------------------------------------- the scenes

/** A sand drawing (mixed.tsx; media/mixed/sand/<id>.json times it) as a scene of its own, from `from` until the next
 * scene: each is drawn to fit its window, so the wind is just starting to carry it off at the cut. */
const sand = (name: string, id: string, from: number, dip = 0.5) =>
  scene({ name, from, dip, cam: () => ({ pos: [0, 1, 5], look: [0, 1, 0], fov: 35 }), draw: () => null, page: () => <SandDrawing id={id} from={Math.round(from * FPS)} /> });

const SCN: Omit<Scene, "to">[] = [];
const scene = (s: Omit<Scene, "to">) => SCN.push(s);

// 0. the title out of the dark
scene({
  name: "title", from: 0,
  cam: () => ({ pos: [0, 1.2, 6], look: [0, 1.4, 0], fov: 30, focus: 6 }),
  draw: (t) => (
    <>
      <AmbientLight intensity={0.02} />
      {/* smoke over a brazier below the frame, lit from under it, embers rising through */}
      <SpotLight name="title-brazier" position={[0, -4, -2]} target={[0, 3, -2]} angle={1.0} penumbra={1} intensity={1.2 * ease(t, 0.2, 2.5) * (0.85 + 0.15 * Math.sin(t * 9) * Math.sin(t * 3.7))} color="#ff8a3d" volumetric />
      <FogVolume name="title-smoke" position={[0, 1.4, -2]} scale={[20, 12, 4]} density={0.03} color="#d9c9b8" scattering={0.6} absorption={0.2} anisotropy={0.3} />
      <Embers t={t} n={70} at={[0, -0.6, 0]} spread={5} rise={3.6} name="title-ember" glow={5 * ease(t, 0.3, 2)} />
    </>
  ),
  page: (t) => {
    // the letters arrive one by one and the tracking slowly opens; a gold rule draws out from the middle under them
    const o = 1 - smooth((t - 5.0) / 0.8);
    const r = smooth((t - 2.4) / 1.2);
    return (
      <div style={{ position: "absolute", left: 0, width: 1920, top: 420, display: "flex", flexDirection: "column", alignItems: "center", opacity: o }}>
        <div style={{ fontFamily: serif, fontWeight: 300, fontSize: 116, letterSpacing: `${0.2 + 0.02 * t}em`, marginRight: `-${0.2 + 0.02 * t}em`, lineHeight: 1, color: ivory, textShadow: shade }}>
          <Letters text="THE IDES OF MARCH" t={t} at={1.2} step={0.06} dur={0.9} />
        </div>
        <div style={{ width: 360 * r, height: 1, background: gold, marginTop: 34 }} />
        <div style={{ marginTop: 24, opacity: smooth((t - 3.0) / 1.0) }}><Italic size={38}>Rome, the fifteenth of March, 44 BC</Italic></div>
      </div>
    );
  },
});

// 1. Rome at dawn
scene({
  name: "dawn", from: TL.at.open - 0.3, dip: 0.8,
  cam: (t) => dolly({ from: TL.at.open - 0.3, to: W("open", "dead") }, { pos: [-46, 1.6, 52], look: [0, 5, 0], fov: 30 }, { pos: [-34, 9, 40], look: [0, 4, 0], fov: 34 })(t),
  draw: () => (
    <>
      <Forum p="dawn" />
      <AmbientLight name="dawn-amb" intensity={0.1} color="#5a6e9c" />
      <DirectionalLight name="sun" position={[-50, 6, 20]} intensity={3} color="#ff9a5a" castShadow />
    </>
  ),
  page: (t) => <Label t={t} at={TL.at.open + 0.2} text="ROME" sub="15 March, 44 BC" />,
});
// the stone floor
/** The dagger at rest on the slab and the blood that has run off it, both from the meshes (tools/blood.py): the slab's
 * top raster-cast on a 2 mm grid, the dagger dropped onto it to its lowest stable rest (on its pommel, grip and guard),
 * and a thin-film flow of a quarter litre off the blade's lower edge and the guard, pooling and running into the joint
 * the blade lies across. props/blood/floor_blood.glb holds the run of blood at 7 growing stages (glTF scenes 0-6) and
 * the film on the blade (scene 7, in the dagger's frame). */
const FLOOR_BLOOD = staticFile("props/blood/floor_blood.glb");
const FLOOR_DAGGER = { position: [0.20006, 0.37413, 0.50008] as V, rotation: [1.66896, -0.05693, 0.9056] as V };
/** each stage: when it has spread (seconds into the shot; the sim's own time, scaled), and how far under the stone it
 * starts (its newly wet part's thickest), so it rises into view thick parts first, the thin front last */
const BLOOD_DONE = [0.1, 0.525, 0.92, 1.408, 1.991, 2.511, 3.0];
const BLOOD_RISE = [0.0047, 0.003, 0.0031, 0.003, 0.0033, 0.0032, 0.0028];
/** a stage's height: risen while the stage before it is shown, put away under the ground once the next has risen */
const bloodY = (k: number, s: number) => {
  const last = BLOOD_DONE.length - 1;
  if (k < last && s >= BLOOD_DONE[k + 1]) return -2;
  if (k === 0) return 0;
  const a = BLOOD_DONE[k - 1], b = BLOOD_DONE[k];
  if (s < a) return -2;
  return -BLOOD_RISE[k] * (1 - clamp01((s - a) / (b - a)));
};
scene({
  name: "floor", from: W("open", "dead"),
  cam: (t) => { const u = ease(t, W("open", "dead"), W("open", "men")); return { pos: add([2.6 - u * 0.7, 1.15, 2.6], hand(t)), look: [0, 0.45, 0], fov: 38, focus: 3.4 }; },
  draw: (t) => (
    <>
      <Model name="floor-slab" src={ruin("008")} position={[0, 0, 0]} rotation={[0, 2.2, 0]} />
      {/* what will be dropped there: the dagger, lying in the low sun, its blade bloodied, in the blood that has run
          off it into the joint between the slabs */}
      <Group position={FLOOR_DAGGER.position} rotation={FLOOR_DAGGER.rotation} scale={2.6}>
        <Model name="floor-dagger" src={staticFile("props/ornate_medieval_dagger/ornate_medieval_dagger_bare.gltf")} />
        <Model name="floor-blade-blood" src={FLOOR_BLOOD} scene={7} />
      </Group>
      {BLOOD_DONE.map((_, k) => (
        <Model key={k} name={`floor-blood${k}`} src={FLOOR_BLOOD} scene={k} position={[0, bloodY(k, t - W("open", "dead")), 0]} />
      ))}
      <Floor name="floor-ground" color="#7d7264" surface="monastery_stone_floor" tile={2.5} />
      <AmbientLight name="floor-amb" intensity={0.08} color="#5a6e9c" />
      <DirectionalLight name="sun" position={[30, 2.2, -4]} intensity={2.6} color="#ff9a5a" castShadow />
    </>
  ),
});
// his friends: a bust against the light
scene({
  name: "friends", from: W("open", "men"),
  cam: (t) => dolly({ from: W("open", "men"), to: TL.at.dictator }, { pos: [0.5, 0.62, 2.6], look: eyeOf("senator", [0, 0, 0], 0.95), fov: 28 }, { pos: [0.25, 0.58, 1.9], look: eyeOf("senator", [0, 0, 0], 0.95), fov: 28 })(t),
  draw: () => (
    <>
      <Figure who="senator" name="friends-bust" at={[0, 0, 0]} h={0.95} yaw={0.4} />
      <AmbientLight name="friends-amb" intensity={0.02} />
      <SpotLight name="friends-rim" position={[-1.2, 1.6, -2.2]} target={[0, 0.7, 0]} angle={0.5} penumbra={0.8} intensity={1.2} color="#ffcf9a" />
      <SpotLight name="friends-rim2" position={[1.4, 1.2, -1.8]} target={[0, 0.7, 0]} angle={0.5} penumbra={0.8} intensity={0.44} color="#9fb4e0" />
      {/* a faint key, low and to the side, so the face just reads out of the dark */}
      <SpotLight name="friends-key" position={[1.6, 0.5, 1.8]} target={[0, 0.52, 0]} angle={0.35} penumbra={0.9} intensity={0.16} color="#ffd2a1" />
    </>
  ),
});

// 2. the dictator
const dict = { from: TL.at.dictator - 0.4, to: W("dictator", "That") };
scene({
  name: "caesar", from: dict.from, dip: 0.7,
  cam: (t) => {
    const a = lerp(-0.9, -0.2, smooth((t - dict.from) / (dict.to - dict.from)));
    // far enough back that the crown stays inside the letterbox (the framing lint's top-cut on cut 7)
    const r = 2.55;
    return { pos: add([Math.sin(a) * r, 1.62, Math.cos(a) * r], hand(t, 0.6)), look: eyeOf("caesar", [0, 1.1, 0], 0.82), fov: 30, focus: r };
  },
  draw: () => (
    <>
      {marble("caesar-plinth", plinth(), [0, 0, 0], "#b9b0a2")}
      <Figure who="caesar" name="caesar-bust" at={[0, 1.1, 0]} h={0.82} />
      <Model name="caesar-bg" src={ruin("004")} position={[3, 0, -16]} rotation={[0, 0.5, 0]} />
      <Floor name="caesar-ground" color="#8a8072" surface="stone_tiles_02" tile={2} />
      <AmbientLight name="caesar-amb" intensity={0.06} color="#8ea2c6" />
      <SpotLight name="caesar-key" position={[-2.4, 3.2, 1.6]} target={[0, 1.5, 0]} angle={0.45} penumbra={0.7} intensity={1.4} color="#ffd2a1" castShadow />
      <SpotLight name="caesar-rim" position={[1.6, 2.4, -2.4]} target={[0, 1.5, 0]} angle={0.5} penumbra={0.8} intensity={0.8} color="#9fb4e0" />
      <DirectionalLight name="sun" position={[20, 6, -40]} intensity={2.2} color="#ffb27a" />
    </>
  ),
  page: (t) => (
    <>
      <Label t={t} at={W("dictator", "Rubicon,") - 0.3} text="49 BC" sub="crosses the Rubicon" />
      <Label t={t} at={W("dictator", "civil") - 0.2} text="48 BC" sub="wins the civil war" slot={1} />
    </>
  ),
});
scene({
  name: "dictator", from: W("dictator", "That"),
  cam: onRight(dolly({ from: W("dictator", "That"), to: TL.at.plot }, { pos: [0.2, 0.55, 2.8], look: [0, 1.55, 0], fov: 32 }, { pos: [0.1, 0.75, 2.2], look: [0, 1.6, 0], fov: 30 })),
  draw: () => (
    <>
      {marble("dict-plinth", plinth(), [0, 0, 0], "#b9b0a2")}
      <Figure who="caesar" name="dict-bust" at={[0, 1.1, 0]} h={0.82} />
      <Model name="dict-bg" src={ruin("007")} position={[-4, 0, -34]} rotation={[0, 0.3, 0]} />
      <Floor name="dict-ground" color="#8a8072" surface="stone_tiles_02" tile={2} />
      <AmbientLight name="dict-amb" intensity={0.05} color="#8ea2c6" />
      <SpotLight name="dict-key" position={[1.8, 0.6, 2.2]} target={[0, 1.55, 0]} angle={0.5} penumbra={0.7} intensity={1.1} color="#ffc48a" castShadow />
      <DirectionalLight name="sun" position={[-30, 4, -60]} intensity={3} color="#ff9a5a" />
    </>
  ),
  page: (t) => <Quote t={t} a={W("dictator", "dictator", 0) - 0.1} b={TL.done.dictator - 0.6} text="DICTATOR PERPETVO" size={96} />,
});

// 3. the conspiracy: sixty senators, two names
const ROWS = 10, COLS = 6;
scene({
  name: "plot", from: TL.at.plot - 0.4, dip: 0.7,
  cam: (t) => {
    const s = TL.at.plot - 0.4, e = TL.at.omens;
    const named = ease(t, W("plot", "Gaius") - 0.4, W("plot", "Gaius") + 0.6);
    const named2 = ease(t, W("plot", "and", 0) - 0.3, W("plot", "Junius") + 0.2);
    const base = dolly({ from: s, to: W("plot", "Gaius") }, { pos: [-3.5, 1.9, 8], look: [0, 0.9, -6], fov: 34 }, { pos: [-2.2, 1.4, 4.2], look: [0, 0.9, -4], fov: 34 })(t);
    const c1: Cam = { pos: [-1.55, 1.42, 1.25], look: [-1.8, 1.32, 0], fov: 30 }, c2: Cam = { pos: [2.05, 1.52, 1.25], look: eyeOf("brutus", [1.8, 1.1, 0], 0.42), fov: 30 };
    // in on Cassius, then a slow truck along the row to Brutus (never back out to the wide between them)
    const on1 = { pos: mix3(base.pos, c1.pos, named), look: mix3(base.look, c1.look, named) };
    // arcing back a little mid-way, so the heads between them don't brush the lens
    const pos = add(mix3(on1.pos, c2.pos, named2), [0, 0.15 * Math.sin(Math.PI * named2), 1.4 * Math.sin(Math.PI * named2)]), look = mix3(on1.look, c2.look, named2);
    void e;
    return { pos: add(pos, hand(t, 0.5)), look, fov: 32, focus: dist(pos, look) };
  },
  draw: (t) => {
    const lit = (i: number) => clamp01((t - (TL.at.plot + 0.3 + i * 0.09)) / 0.6);
    return (
      <>
        {Array.from({ length: ROWS * COLS }, (_, i) => {
          const r = Math.floor(i / COLS), c = i % COLS;
          const x = (c - (COLS - 1) / 2) * 1.2, z = -r * 1.5;
          const two = r === 0 && (c === 1 || c === 4);
          return (
            <Group key={i} name={`plot-g${i}`}>
              {marble(`plot-pl${i}`, plinth(), [x, 0, z], "#7d756a")}
              <Figure who={two && c === 4 ? "brutus" : (["senator", "senator2", "senator3"] as const)[i % 3]} name={`plot-b${i}`} at={[x, 1.1, z]} h={0.42} yaw={(rnd(i) - 0.5) * 0.8} />
              <Mesh name={`plot-candle${i}`} position={[x + 0.24, 1.14, z + 0.2]}>
                <Sphere args={[0.012]} />
                <MeshStandardMaterial color="#000000" emissive="#ffb35a" emissiveIntensity={6 * lit(r * COLS + c) * (0.85 + 0.15 * Math.sin(t * 13 + i))} />
              </Mesh>
            </Group>
          );
        })}
        <Floor name="plot-ground" color="#5e554a" surface="monastery_stone_floor" tile={2.5} />
        <AmbientLight name="plot-amb" intensity={0.03} color="#8ea2c6" />
        <PointLight name="plot-glow" position={[0, 2.2, -4]} intensity={0.36 * ease(t, TL.at.plot, TL.at.plot + 3)} color="#ffae63" />
        <SpotLight name="plot-cassius" position={[-1.8, 3, 1.4]} target={[-1.8, 1.3, 0]} angle={0.3} penumbra={0.6} intensity={0.9 * ease(t, W("plot", "Gaius") - 0.3, W("plot", "Gaius") + 0.3)} color="#ffd2a1" />
        <SpotLight name="plot-brutus" position={[1.8, 3, 1.4]} target={[1.8, 1.3, 0]} angle={0.3} penumbra={0.6} intensity={0.9 * ease(t, W("plot", "Marcus") - 0.3, W("plot", "Marcus") + 0.3)} color="#ffd2a1" />
        {/* a high window's light through the room's dust, behind the two named men */}
        <SpotLight name="plot-shaft" position={[-4, 7, -12]} target={[1, 0, -5]} angle={0.16} penumbra={0.4} intensity={1.6 * ease(t, TL.at.plot, TL.at.plot + 3)} color="#cfdcf5" volumetric />
        <FogVolume name="plot-air" position={[0, 2, -4]} scale={[9, 4.4, 16]} density={0.08} color="#fff0dc" scattering={0.45} absorption={0.15} anisotropy={0.55} />
      </>
    );
  },
  page: (t) => (
    <>
      <Label t={t} at={W("plot", "sixty") - 0.2} text="LX" sub="more than sixty senators" />
      <Label t={t} at={W("plot", "Gaius") - 0.1} until={W("plot", "Marcus") - 0.5} text="GAIUS CASSIUS LONGINUS" side="left" />
      <Label t={t} at={W("plot", "Marcus") - 0.1} text="MARCUS JUNIUS BRUTUS" side="left" />
    </>
  ),
});

// 4. the omens: a storm over the statues
const flash = (t: number, at: number) => (t >= at ? Math.exp(-(t - at) * 9) + (t - at > 0.12 ? Math.exp(-(t - at - 0.12) * 12) * 0.7 : 0) : 0);
/** rain: thin streaks falling at 9 m/s through a box around the shot, a little slanted, each seeded; lit by `L` */
function Rain({ t, n, at, size, L, name }: { t: number; n: number; at: V; size: V; L: number; name: string }) {
  return (
    <>
      {Array.from({ length: n }, (_, i) => {
        const y = size[1] - ((t * 9 + rnd(i * 3 + 1) * size[1]) % size[1]);
        return (
          <Mesh key={i} name={`${name}${i}`} position={[at[0] + (rnd(i * 5 + 2) - 0.5) * size[0] + y * 0.12, at[1] + y, at[2] + (rnd(i * 7 + 3) - 0.5) * size[2]]} rotation={[0, 0, 0.12]}>
            <Box args={[0.006, 0.42, 0.006]} />
            <MeshStandardMaterial color="#000000" emissive="#c9d6ee" emissiveIntensity={0.35 + 6 * L} transparent opacity={0.45} />
          </Mesh>
        );
      })}
    </>
  );
}
/** a bolt in the far sky: a jagged line of bright segments from the clouds down, seeded by `seed`, seen while `L` lasts */
function Bolt({ L, at, seed, name }: { L: number; at: V; seed: number; name: string }) {
  if (L < 0.04) return null;
  const pts: V[] = [[at[0], at[1], at[2]]];
  for (let i = 1; i <= 12; i++) {
    const p = pts[i - 1];
    pts.push([p[0] + (rnd(seed * 97 + i) - 0.5) * 3.2, p[1] - 2.4 - rnd(seed * 41 + i) * 1.2, p[2] + (rnd(seed * 13 + i) - 0.5) * 1.5]);
  }
  return (
    <>
      {pts.slice(1).map((q, i) => {
        const p = pts[i], d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]], len = Math.hypot(d[0], d[1], d[2]);
        return (
          <Group key={i} name={`${name}-y${i}`} position={mix3(p, q, 0.5)} rotation={[0, Math.atan2(d[0], d[2]), 0]}>
            <Mesh name={`${name}-s${i}`} rotation={[Math.atan2(Math.hypot(d[0], d[2]), d[1]), 0, 0]}>
              <Box args={[0.08, len, 0.08]} />
              <MeshStandardMaterial color="#000000" emissive="#e8eeff" emissiveIntensity={60 * L} />
            </Mesh>
          </Group>
        );
      })}
    </>
  );
}
scene({
  name: "omens", from: TL.at.omens - 0.4, dip: 0.7,
  cam: (t) => dolly({ from: TL.at.omens - 0.4, to: W("omens", "Calpurnia") }, { pos: [-7, 1.4, 9], look: [0, 1.6, 0], fov: 30 }, { pos: [-3.4, 1.2, 4.6], look: [0, 1.5, 0], fov: 32 })(t),
  draw: (t) => {
    const L = LIGHTNING.reduce((sum, b) => sum + flash(t, b.at) * b.k, 0);
    return (
      <>
        <Model name="omens-ring" src={ruin("005")} position={[2, 0, -4]} />
        {marble("omens-plinth", plinth(), [0.35, 0, 0], "#8d8579")}
        <Figure who="horse" name="omens-horse" at={[0, 1.1, 0]} h={1.0} yaw={0.6} />
        {/* Spurinna's sacrifice: a fire in a stone pit before the plinth, guttering in the rain, lighting its ring */}
        <Model name="omens-pit" src={prop("stone_fire_pit")} position={[1.2, 0.3, 1.7]} scale={[0.8, 0.8, 0.8]} />
        <Fire name="omens-fire" time={t} position={[1.2, 0.34, 1.7]} scale={[0.8, 1.2, 0.8]} intensity={5} seed={5} speed={1.3} light={0.5} />
        <Mesh name="omens-ground" rotation={[-Math.PI / 2, 0, 0]}>
          <Plane args={[400, 400]} />
          {/* wet: dark and glossy, so the flashes glint in it */}
          <MeshStandardMaterial color="#4a4640" roughness={0.35} {...tex("stone_tiles_02", [200, 200])} />
        </Mesh>
        <Rain t={t} n={320} at={[-3, 0, 3]} size={[10, 7, 9]} L={L} name="omens-rain" />
        {LIGHTNING.map((b, i) => <Bolt key={i} L={flash(t, b.at) * b.k} at={[36 + i * 5, 26, -40 + i * 7]} seed={i + 1} name={`omens-bolt${i}`} />)}
        <AmbientLight name="omens-amb" intensity={0.05} color="#6f86b8" />
        <DirectionalLight name="sun" position={[10, 12, 6]} intensity={0.6} color="#9fb4e0" castShadow />
        <DirectionalLight name="sky" position={[-6, 14, -10]} intensity={7 * L} color="#dfe8ff" />
      </>
    );
  },
  page: (t) => <Label t={t} at={W("omens", "Spurinna") - 0.1} text="SPURINNA" sub="a haruspex, reader of omens" />,
});
// Calpurnia's dream: his bust fallen on a marble floor at night, blood spreading from under it, moonlight through a
// high window in dusty air; the lightning that wakes her
const sDream = W("omens", "Calpurnia");
/** a pool spreading on a floor: overlapping flat lobes, each growing from its own start, glossy as wet */
function Pool({ t, at, from, grow, r, name, seed = 1 }: { t: number; at: V; from: number; grow: number; r: number; name: string; seed?: number }) {
  return (
    <>
      {Array.from({ length: 7 }, (_, i) => {
        const a = rnd(seed * 31 + i) * Math.PI * 2, d = i === 0 ? 0 : r * (0.35 + 0.4 * rnd(seed * 17 + i));
        const u = ease(t, from + i * 0.25, from + grow + i * 0.4);
        const k = r * (i === 0 ? 0.7 : 0.35 + 0.3 * rnd(seed * 7 + i)) * Math.max(0.001, u);
        return (
          <Mesh key={i} name={`${name}-${i}`} position={[at[0] + Math.cos(a) * d * u, at[1] + 0.002 + i * 0.0004, at[2] + Math.sin(a) * d * u]} scale={[k, 0.0015, k * (0.7 + 0.5 * rnd(seed * 3 + i))]}>
            <Sphere args={[1]} />
            <MeshPhysicalMaterial color="#4a0705" roughness={0.08} clearcoat={1} clearcoatRoughness={0.05} />
          </Mesh>
        );
      })}
    </>
  );
}
scene({
  name: "dream", from: sDream - 0.15,
  // low along the floor, pushing in on the face
  cam: (t) => dolly({ from: sDream, to: TL.at.decimus }, { pos: [1.7, 0.32, 1.9], look: [0, 0.18, 0], fov: 32 }, { pos: [0.85, 0.24, 1.05], look: [0, 0.2, 0], fov: 30 })(t),
  draw: (t) => (
    <>
      <Group name="dream-fallen" position={[0, 0.2, 0]} rotation={[0, 0.5, Math.PI / 2]}>
        <Figure who="caesar" name="dream-bust" at={[0, -0.4, 0]} h={0.8} />
      </Group>
      <Pool t={t} at={[0.05, 0, 0.3]} from={sDream + 0.2} grow={5} r={0.75} name="dream-blood" />
      <Mesh name="dream-floor" position={[0, -0.05, 0]}>
        <Box args={[8, 0.1, 8]} />
        <MeshStandardMaterial color="#9a948a" roughness={0.35} {...tex("marble_tiles", [3.3, 3.3])} />
      </Mesh>
      <AmbientLight name="dream-amb" intensity={0.015} color="#6f86b8" />
      {/* the moon through a high window, in the room's dust */}
      <SpotLight name="dream-moon" position={[-1.6, 5.5, 2.4]} target={[0, 0.15, 0.2]} angle={0.26} penumbra={0.4} intensity={2.2} color="#b4c6ec" castShadow volumetric />
      <FogVolume name="dream-air" position={[-1.1, 2.6, 0.6]} scale={[2.6, 5.2, 3]} density={0.05} color="#e8eeff" scattering={0.5} absorption={0.1} anisotropy={0.6} />
      <SpotLight name="dream-red" position={[1.8, 1.2, 1.4]} target={[0, 0.2, 0]} angle={0.5} penumbra={0.9} intensity={0.02 * ease(t, sDream, sDream + 1.2)} color="#c0392b" />
      <DirectionalLight name="sun" position={[6, 10, 4]} intensity={10 * flash(t, sDream + 0.15)} color="#dfe8ff" />
    </>
  ),
});

/** Caesar's house on the Via Sacra, around the gate (ruin 002: 6 m wide, 6.6 m tall, at the origin): plastered walls on
 * a stone base, a cornice and a tiled roof behind it */
function House({ p }: { p: string }) {
  return (
    <>
      {[-1, 1].map((side) => (
        <Group key={side} name={`${p}-wing${side}`} position={[side * 9.5, 0, -0.2]}>
          <Mesh name={`${p}-wall${side}`} position={[0, 2.9, 0]}>
            <Box args={[13, 5.8, 0.9]} />
            <MeshStandardMaterial color="#fff6e8" {...tex("white_plaster_rough_01", [6, 2.7])} />
          </Mesh>
          <Mesh name={`${p}-base${side}`} position={[0, 0.45, 0.08]}>
            <Box args={[13, 0.9, 1.05]} />
            <MeshStandardMaterial color="#b8a88e" {...tex("large_sandstone_blocks_01", [4.3, 0.3])} />
          </Mesh>
          <Mesh name={`${p}-cornice${side}`} position={[0, 5.9, 0.1]}>
            <Box args={[13.2, 0.26, 1.2]} />
            <MeshStandardMaterial color="#d9cdb6" roughness={0.7} />
          </Mesh>
          <Mesh name={`${p}-roof${side}`} position={[0, 6.4, -1.4]} rotation={[0.42, 0, 0]}>
            <Box args={[13.4, 0.18, 3.6]} />
            <MeshStandardMaterial color="#8c4a30" roughness={0.75} />
          </Mesh>
          {/* small high windows, dark */}
          {[-3.5, 0, 3.5].map((x, i) => (
            <Mesh key={i} name={`${p}-win${side}-${i}`} position={[x, 4.4, 0.46]}>
              <Box args={[0.7, 0.9, 0.04]} />
              <MeshStandardMaterial color="#17110c" roughness={1} />
            </Mesh>
          ))}
        </Group>
      ))}
    </>
  );
}

// 5. Decimus at the door
scene({
  name: "door", from: TL.at.decimus - 0.3, dip: 0.6,
  // from down the street, the house front raking away to the left, closing on the gate
  cam: (t) => dolly({ from: TL.at.decimus - 0.3, to: TL.at.street }, { pos: [9, 1.7, 19], look: [-1, 3.4, 0], fov: 34 }, { pos: [4.2, 1.7, 10], look: [0, 3.2, 0], fov: 34 })(t),
  draw: () => (
    <>
      <Model name="door-gate" src={ruin("002")} position={[0, 0, 0]} />
      <House p="door" />
      <Floor name="door-ground" color="#a39684" surface="large_sandstone_blocks_01" tile={3} />
      <AmbientLight name="door-amb" intensity={0.06} color="#4a5a8a" />
      <DirectionalLight name="sun" position={[40, 9, 6]} intensity={1.6} color="#a9bce8" castShadow volumetric />
      <FogVolume name="door-air" position={[0, 5, 8]} scale={[22, 10, 24]} density={0.01} color="#ffe9cc" scattering={0.35} absorption={0.1} anisotropy={0.6} />
    </>
  ),
});

// 6. the street: arches, the seer's answer, the scroll
const streetCam = onRight(dolly({ from: TL.at.street - 0.3, to: W("street", "Someone") }, { pos: [0.5, 1.8, 51], look: [-6.6, 1.5, 40], fov: 34 }, { pos: [-2.5, 1.7, 47.5], look: [-6.6, 1.3, 40], fov: 34 }));
scene({
  name: "street", from: TL.at.street - 0.3, dip: 0.5,
  // a slow track sideways down the street toward the seer's altar on the right; the quotes take the left
  cam: streetCam,
  draw: (t) => (
    <>
      {/* Spurinna was a haruspex: his altar, a fire in a stone pit on a marble block, still smoking from the morning's
          sacrifice */}
      <Mesh name="street-altar" position={[-6.6, 0.45, 40]}>
        <Box args={[1.6, 0.9, 1.6]} />
        <MeshStandardMaterial color="#8e8476" {...tex("large_sandstone_blocks_01", [0.6, 0.35])} />
      </Mesh>
      <Fire name="street-flames" time={t} position={[-6.6, 1.1, 40]} scale={[1.1, 1.6, 1.1]} intensity={14} seed={2} opacity={1.5} light={0} />
      <Model name="street-pit" src={prop("stone_fire_pit")} position={[-6.6, 1.09, 40]} scale={[0.95, 0.95, 0.95]} />
      <Mesh name="street-coals" position={[-6.6, 1.12, 40]} scale={[1, 0.25, 1]}>
        <Sphere args={[0.5]} />
        <MeshStandardMaterial color="#000000" emissive="#ff5a1c" emissiveIntensity={14 * (0.8 + 0.2 * Math.sin(t * 9) * Math.sin(t * 3.7))} />
      </Mesh>
      <Embers t={t} n={40} at={[-6.6, 1.2, 40]} spread={0.7} rise={3.2} name="street-ember" glow={10} />
      <PointLight name="street-fire" position={[-6.6, 1.6, 40.6]} intensity={0.05 * (0.85 + 0.15 * Math.sin(t * 11))} color="#ff8a3d" distance={6} />
      <FogVolume name="street-smoke" position={[-6.7, 5.5, 39.9]} scale={[1.3, 6, 1.3]} density={0.45} color="#b4aba0" scattering={0.7} absorption={0.35} anisotropy={0.4} />
      <Model name="street-arch" src={ruin("009")} position={[-6, 0, 20]} />
      <Model name="street-arch2" src={ruin("001")} position={[-6, 0, -6]} rotation={[0, Math.PI / 2, 0]} />
      <Model name="street-cols" src={ruin("003")} position={[4, 0, 30]} rotation={[0, -0.4, 0]} />
      <Model name="street-col" src={ruin("006")} position={[-16, 0, 30]} />
      <Floor name="street-ground" color="#a39684" surface="large_sandstone_blocks_01" tile={3} />
      <AmbientLight name="street-amb" intensity={0.06} color="#4a5a8a" />
      <DirectionalLight name="sun" position={[-30, 16, 25]} intensity={1.0} color="#a9bce8" castShadow volumetric />
    </>
  ),
  page: (t) => (
    <>
      <Quote t={t} a={QUOTED[0][0]} b={QUOTED[0][1]} text="“The Ides of March have come.”" />
      <Quote t={t} a={QUOTED[1][0]} b={QUOTED[1][1]} text="“But they have not gone.”" />
    </>
  ),
});
// the scroll that named the plot, pressed into his hand on the way, unread: rolled, tied, its seal unbroken, on a step
scene({
  name: "scroll", from: W("street", "Someone") - 0.2, dip: 0.3,
  cam: (t) => dolly({ from: W("street", "Someone") - 0.2, to: TL.at.curia - 3.2 }, { pos: [0.55, 0.3, 0.75], look: [0, 0.05, 0], fov: 30 }, { pos: [0.36, 0.2, 0.5], look: [0, 0.05, 0], fov: 28 })(t),
  draw: () => (
    <>
      <Mesh name="scroll-step" position={[0, -0.15, -0.1]}>
        <Box args={[2.4, 0.3, 1.2]} />
        <MeshStandardMaterial color="#8a7f70" {...tex("large_sandstone_blocks_01", [1.2, 0.6])} />
      </Mesh>
      <Mesh name="scroll-step2" position={[0, 0.0, -1.0]}>
        <Box args={[2.4, 0.3, 0.6]} />
        <MeshStandardMaterial color="#756b5e" {...tex("large_sandstone_blocks_01", [1.2, 0.3])} />
      </Mesh>
      <Group name="scroll" position={[0, 0.038, 0.05]} rotation={[0, 0.5, 0]}>
        <Mesh name="scroll-roll" rotation={[0, 0, Math.PI / 2]}>
          <Cylinder args={[0.036, 0.036, 0.3]} />
          <MeshStandardMaterial color="#b39a70" roughness={0.85} />
        </Mesh>
        {[-0.12, 0.12].map((x, i) => (
          <Mesh key={i} name={`scroll-end-${i}`} position={[x * 1.32, 0, 0]} rotation={[0, 0, Math.PI / 2]}>
            <Cylinder args={[0.012, 0.012, 0.05]} />
            <MeshStandardMaterial color="#5a3e26" roughness={0.6} />
          </Mesh>
        ))}
        <Mesh name="scroll-cord" rotation={[0, Math.PI / 2, 0]}>
          <Torus args={[0.0375, 0.004]} />
          <MeshStandardMaterial color="#4a3220" roughness={0.8} />
        </Mesh>
        <Mesh name="scroll-seal" position={[0, 0.034, 0.012]} rotation={[0.3, 0, 0]}>
          <Cylinder args={[0.016, 0.016, 0.006]} />
          <MeshStandardMaterial color="#5e130f" roughness={0.4} />
        </Mesh>
      </Group>
      <AmbientLight name="scroll-amb" intensity={0.05} color="#8ea2c6" />
      <DirectionalLight name="sun" position={[30, 4, 6]} intensity={0.7} color="#a9bce8" castShadow />
    </>
  ),
});

// 7. the curia of Pompey
sand("curia-plan", "pompey", TL.at.curia - 3.2, 0.6);
scene({
  name: "curia", from: W("curia", "beneath") - 0.2,
  cam: (t) => dolly({ from: TL.at.curia - 0.4, to: TL.at.strike }, { pos: [0, 1.4, 16], look: eyeOf("pompey", [0, 1.6, 0], 2.2), fov: 36 }, { pos: [0, 1.5, 11], look: eyeOf("pompey", [0, 1.6, 0], 2.2), fov: 40 })(t),
  draw: () => (
    <>
      <Model name="curia-portico" src={ruin("004")} position={[-0.75, 0, -6]} />
      <Model name="curia-ring" src={ruin("005")} position={[12.6, 0, -3]} rotation={[0, 0.5, 0]} />
      {marble("curia-plinth", manifold.roundedBox(1.6, 1.6, 1.6, 0.04).move(0, 0.8, 0), [0, 0, 0], "#a69d90")}
      <Figure who="pompey" name="curia-pompey" at={[0, 1.6, 0]} h={2.2} />
      <Floor name="curia-ground" color="#8c857a" surface="marble_tiles" tile={2.4} />
      <AmbientLight name="curia-amb" intensity={0.05} color="#8ea2c6" />
      <SpotLight name="curia-shaft" position={[-4, 14, -2]} target={[0, 2, 0]} angle={0.22} penumbra={0.5} intensity={8.4} color="#ffd8a8" castShadow volumetric />
      <FogVolume name="curia-air" position={[0, 5, 0]} scale={[16, 10, 16]} density={0.06} color="#fff2e0" scattering={0.45} absorption={0.15} anisotropy={0.55} />
      <DirectionalLight name="sun" position={[10, 6, 20]} intensity={0.8} color="#ffb27a" />
    </>
  ),
  page: (t) => <Label t={t} at={W("curia", "beneath") - 0.2} text="THE CURIA OF POMPEY" sub="Campus Martius, Rome" />,
});

// 8. the strike
const sStrike = TL.at.strike - 0.2, sCasca = W("strike", "Casca"), sAll = W("strike", "Then", 0), sSaw = W("strike", "saw"), sPulled = W("strike", "Then", 1), sFell = W("strike", "fell.");
const redPulse = (t: number) => [sCasca + 0.45, sAll + 0.5, sAll + 0.9, sAll + 1.2, sAll + 1.6].reduce((m, a) => m + (t >= a ? Math.exp(-(t - a) * 10) : 0), 0);
const bustRoom = (p: string, t: number) => (
  <>
    {marble(`${p}-plinth`, plinth(), [0, 0, 0], "#b9b0a2")}
    <Floor name={`${p}-ground`} color="#5a5248" surface="marble_tiles" tile={2.4} />
    <AmbientLight name={`${p}-amb`} intensity={0.04} color="#8ea2c6" />
    <SpotLight name={`${p}-key`} position={[-2.0, 3.0, 1.8]} target={[0, 1.5, 0]} angle={0.45} penumbra={0.7} intensity={1.2} color="#ffd2a1" castShadow />
    <PointLight name={`${p}-red`} position={[1.2, 1.8, 1.2]} intensity={0.16 * redPulse(t)} color="#b3140c" />
    <FogVolume name={`${p}-blood`} position={[0, 1.6, 0]} scale={[7, 3.2, 7]} density={0.03 + 0.1 * Math.min(1, redPulse(t)) + 0.05 * ease(t, sCasca, sPulled)} color="#d0281c" scattering={0.6} absorption={0.12} anisotropy={0.3} />
  </>
);
scene({
  name: "seized", from: sStrike, dip: 0.3,
  cam: (t) => { const u = ease(t, sStrike, sCasca); return { pos: add([0.9 - u * 0.2, 1.62, 2.4 - u * 0.3], hand(t, 1.5)), look: eyeOf("caesar", [0, 1.1, 0], 0.82), fov: 30, focus: 2.4 }; },
  draw: (t) => <>{bustRoom("seized", t)}<Figure who="caesar" name="seized-bust" at={[0, 1.1, 0]} h={0.82} /></>,
});
scene({
  name: "stabs", from: sCasca + 0.4,
  cam: (t) => {
    // a new angle each blow
    const k = Math.floor((t - sCasca) / 0.55);
    const a = (rnd(k * 7) - 0.5) * 2.2, r = 1.7 + rnd(k * 3) * 0.5, y = 1.4 + rnd(k * 5) * 0.3;
    const shake = Math.exp(-((t - sCasca) % 0.55) * 8) * 0.04;
    return { pos: [Math.sin(a) * r + shake, y, Math.cos(a) * r], look: eyeOf("caesar", [0, 1.1, 0], 0.82), fov: 34, focus: r };
  },
  draw: (t) => <>{bustRoom("stabs", t)}<Figure who="caesar" name="stabs-bust" at={[0, 1.1, 0]} h={0.82} /><Dagger t={t} /></>,
});
/** each blow: a dagger's shadow stabs down across him, cast by the key light from a blade between it and the bust
 * (Poly Haven's ornate dagger, its blade along +Y); the thrust takes 0.16 s and withdraws before the next angle */
function Dagger({ t }: { t: number }) {
  const k = Math.floor((t - sCasca) / 0.55), u = (t - sCasca) % 0.55;
  const key: V = [-2.0, 3.0, 1.8], neck: V = [0, 1.45, 0];
  const s = ease(u, 0.0, 0.16) * (1 - ease(u, 0.32, 0.5));
  return (
    <Group name="stabs-shadow-yaw" position={add(mix3(key, neck, 0.66), [0, 0.25 - 0.5 * s, 0])} rotation={[0, -0.8 + (rnd(k * 19) - 0.5) * 0.6, 0]}>
      <Group name="stabs-shadow-pitch" rotation={[Math.PI - 0.5 + s * 0.6, 0, 0]}>
        <Model name="stabs-shadow" src={staticFile("props/ornate_medieval_dagger/ornate_medieval_dagger_bare.gltf")} position={[0, -0.346 * 1.4, 0]} scale={1.4} />
      </Group>
    </Group>
  );
}
scene({
  name: "brutus", from: sSaw - 0.1,
  cam: (t) => dolly({ from: sSaw, to: sPulled }, { pos: [0.1, eyeOf("brutus", [0, 1.1, 0], 0.82)[1] + 0.04, 1.8], look: eyeOf("brutus", [0, 1.1, 0], 0.82), fov: 30 }, { pos: [0.05, eyeOf("brutus", [0, 1.1, 0], 0.82)[1] + 0.02, 1.25], look: eyeOf("brutus", [0, 1.1, 0], 0.82), fov: 28 })(t),
  draw: () => (
    <>
      {marble("brutus-plinth", plinth(), [0, 0, 0], "#b9b0a2")}
      <Figure who="brutus" name="brutus-bust" at={[0, 1.1, 0]} h={0.82} yaw={-0.25} />
      <Floor name="brutus-ground" color="#4e483f" surface="marble_tiles" tile={2.4} />
      <AmbientLight name="brutus-amb" intensity={0.02} />
      <SpotLight name="brutus-key" position={[1.8, 2.4, 1.2]} target={[0, 1.5, 0]} angle={0.4} penumbra={0.8} intensity={1} color="#a9bde6" />
    </>
  ),
});
scene({
  name: "fall", from: sPulled - 0.1,
  // on the bust's face, then easing back as it shatters, so the pieces stay in the picture
  cam: (t) => { const u = ease(t, W("strike", "fell.") - 0.1, W("strike", "fell.") + 1.6); return { pos: [0.9 + u * 0.3, 1.7 - u * 0.1, 2.3 + u * 1.3], look: [0, 1.6 - u * 0.25, 0], fov: 32 + u * 6, focus: 2.4 + u * 1.3 }; },
  draw: (t) => (
    <>
      {marble("fall-plinth", plinth(), [0, 0, 0], "#b9b0a2")}
      <Shatter t={t} tHit={sFell + 0.1} at={[0, 1.1, 0]} h={0.82} name="fall-sh" />
      <Floor name="fall-ground" color="#5a5248" surface="marble_tiles" tile={2.4} />
      <AmbientLight name="fall-amb" intensity={0.03} color="#8ea2c6" />
      <SpotLight name="fall-key" position={[-2.0, 3.0, 1.8]} target={[0, 1.3, 0]} angle={0.5} penumbra={0.7} intensity={1.1} color="#ffd2a1" castShadow />
      <PointLight name="fall-red" position={[1.0, 1.4, 1.0]} intensity={0.28 * ease(t, sFell, sFell + 0.4)} color="#c0392b" />
      <FogVolume name="fall-blood" position={[0, 1.6, 0]} scale={[8, 3.2, 8]} density={0.08 + 0.06 * ease(t, sFell, sFell + 1)} color="#d0281c" scattering={0.6} absorption={0.12} anisotropy={0.3} />
    </>
  ),
});

const roman = (n: number) => (n >= 20 ? "XX" : n >= 10 ? "X" : "") + ["", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX"][n % 10];
// 9. twenty-three wounds
scene({
  name: "wounds", from: TL.at.wounds - 0.4, dip: 0.4,
  cam: onRight(dolly({ from: TL.at.wounds - 0.4, to: TL.at.funeral }, { pos: [3.2, 0.6, 5.4], look: [0, 1.0, 0], fov: 34 }, { pos: [3.4, 1.3, 8], look: [0, 1.85, 0], fov: 36 })),
  draw: (t) => (
    <>
      {marble("wounds-plinth", manifold.roundedBox(1.6, 1.6, 1.6, 0.04).move(0, 0.8, 0), [0, 0, 0], "#8d8579")}
      <Figure who="pompey" name="wounds-pompey" at={[0, 1.6, 0]} h={2.2} />
      {/* his blood on Pompey's plinth: runs down its face and pools at its foot */}
      {Array.from({ length: 6 }, (_, i) => {
        const x = -0.55 + i * 0.21 + (rnd(i * 3) - 0.5) * 0.08, len = (0.6 + 0.95 * rnd(i * 5)) * ease(t, TL.at.wounds - 0.4 + i * 0.4, TL.at.wounds + 3.5 + i * 0.8);
        return (
          <Group key={i} name={`wounds-drip${i}`}>
            {/* a run: a flattened rod down the face, a bead gathering at its tip */}
            <Mesh name={`wounds-run${i}`} position={[x, 1.6 - len / 2, 0.8]} scale={[1, Math.max(0.001, len), 0.35]}>
              <Cylinder args={[0.014 + 0.012 * rnd(i * 7), 0.014 + 0.012 * rnd(i * 7), 1]} />
              <MeshPhysicalMaterial color="#5c0806" roughness={0.12} clearcoat={1} clearcoatRoughness={0.05} />
            </Mesh>
            <Mesh name={`wounds-bead${i}`} position={[x, 1.6 - len, 0.8]} scale={[1, 1.3, 0.5]}>
              <Sphere args={[0.022 + 0.014 * rnd(i * 7)]} />
              <MeshPhysicalMaterial color="#5c0806" roughness={0.12} clearcoat={1} clearcoatRoughness={0.05} />
            </Mesh>
          </Group>
        );
      })}
      <Pool t={t} at={[0.1, 0, 1.15]} from={TL.at.wounds + 1} grow={7} r={1.0} name="wounds-blood" seed={4} />
      <Floor name="wounds-ground" color="#5a5248" surface="marble_tiles" tile={2.4} />
      <AmbientLight name="wounds-amb" intensity={0.02} />
      {/* a cold white key, so his blood reads red on the marble, and a faint red from the side */}
      <SpotLight name="wounds-key" position={[2.5, 4, 3]} target={[0, 0.7, 0.8]} angle={0.55} penumbra={0.9} intensity={1.5} color="#ffe6cc" castShadow />
      {/* Pompey looks down on it: a dim cold light on the statue above, so the shot ends on his face */}
      <SpotLight name="wounds-face" position={[1.5, 7, 4]} target={[0, 3.3, 0]} angle={0.3} penumbra={1} intensity={0.9} color="#d8e0ff" />
      <SpotLight name="wounds-red" position={[-3, 1.5, 2]} target={[0, 0.8, 0]} angle={0.6} penumbra={0.9} intensity={0.25} color="#b3261e" />
    </>
  ),
  page: (t) => {
    const a = TL.at.wounds;
    const n = Math.min(23, Math.max(0, Math.floor((t - a) / 0.07)));
    const fade = 1 - smooth((t - W("wounds", "Shakespeare") + 0.6) / 0.5);
    const et = smooth((t - W("wounds", "Shakespeare") - 0.6) / 0.4);
    const strike = smooth((t - W("wounds", "ancient") + 0.1) / 0.4);
    const greek = smooth((t - W("wounds", "Greek.") - 0.1) / 0.5);
    return (
      <>
        <Scrim o={Math.max(smooth((t - a) / 0.3) * fade, et)} k={0.5} />
        {/* the count: a numeral that climbs with the strokes, the strokes drawn in gold, fives tied across */}
        <Reveal u={shown(t, a - 0.4, W("wounds", "Shakespeare") - 0.6)} w={760} h={330} left={160} top={300} pool={0.5}>
          <div style={{ fontFamily: serif, fontWeight: 300, fontSize: 210, lineHeight: 0.9, color: bloodInk, textShadow: shade, height: 190 }}>{roman(n)}</div>
          <div style={{ position: "relative", height: 70, width: 700, marginTop: 26 }}>
            {Array.from({ length: n }, (_, i) => {
              const g = Math.floor(i / 5), k = i % 5;
              return k < 4
                ? <div key={i} style={{ position: "absolute", left: g * 140 + k * 22, top: 0, width: 2, height: 64, background: gold, opacity: 0.9 }} />
                : <div key={i} style={{ position: "absolute", left: g * 140 - 10, top: 30, width: 90, height: 2, background: gold, transform: "rotate(-24deg)" }} />;
            })}
          </div>
        </Reveal>
        {/* the last words: Shakespeare's, struck through; then what Suetonius says was said */}
        <Reveal u={shown(t, W("wounds", "Shakespeare") - 0.2, TL.done.wounds - 0.3)} w={820} h={400} left={160} top={250} pool={0.5}>
          <div style={{ position: "relative", opacity: 1 - strike * 0.55 }}>
            <Italic size={118} weight={300} color={ivory}>Et tu, Brute?</Italic>
            <div style={{ position: "absolute", left: -8, top: "54%", height: 3, width: `${strike * 104}%`, background: bloodInk }} />
          </div>
          <Caps size={20} track={0.32} color={ash}>Shakespeare · Julius Caesar · 1599</Caps>
          <div style={{ opacity: greek, marginTop: 56 }}>
            <div style={{ fontFamily: "EB Garamond", fontStyle: "italic", fontWeight: 400, fontSize: 96, lineHeight: 1, color: ivory, textShadow: shade }}>καὶ σύ, τέκνον;</div>
            <Italic size={32} style={{ marginTop: 14 }}>“You too, child?” Suetonius, Life of Caesar 82</Italic>
          </div>
        </Reveal>
      </>
    );
  },
});

// 10. the funeral: the Forum by firelight
const funeralCam = (t: number): Cam => {
  const u = smooth((t - W("funeral", "crowd")) / 9);
  const pos: V = [lerp(3.6, 2.4, u), lerp(2.1, 1.8, u), lerp(15.6, 16.6, u)];
  return { pos, look: [0, 2.1, 24], fov: 40, focus: Math.hypot(pos[0], pos[2] - 24) };
};
sand("fled", "gerome", TL.at.funeral - 0.4, 0.8);
sand("oration", "antony", W("funeral", "At") - 0.2, 0.3);
scene({
  name: "funeral", from: W("funeral", "crowd") - 0.5, dip: 0.3,
  cam: funeralCam,
  draw: (t) => {
    const burn = ease(t, W("funeral", "burned") - 1, W("funeral", "burned") + 2);
    const fl = 0.8 + 0.2 * Math.sin(t * 17) * Math.sin(t * 5.3 + 1);
    return (
      <>
        <Forum p="fun" />
        <AmbientLight name="fun-amb" intensity={0.04} color="#6f86b8" />
        <DirectionalLight name="sun" position={[-20, 30, -30]} intensity={0.5} color="#9fb4e0" />
        <PointLight name="fun-fire" position={[0, 2.2, 24]} intensity={(0.6 + 2.4 * burn) * fl} color="#ff8a3d" castShadow />
        <Pyre t={t} at={[0, 0, 24]} burn={0.25 + 0.75 * burn} p="fun" />
        {/* the flames: a volume of fire through the stack and over the bier, roaring up as it burns */}
        <Fire name="fun-flames" time={t} position={[0, 0.2, 24]} scale={[2.6, 4.3, 2.0]} intensity={10 + 25 * burn} speed={0.9} opacity={2} light={0} />
        <Haze t={t} name="fun-haze" at={[0, 2.5, 28]} face={funeralCam(t).pos} width={16} depth={5} tint="warm" />
        <Embers t={t} n={120} at={[0, 1.4, 24]} spread={3} rise={12} name="fun-ember" glow={4 + 8 * burn} />
      </>
    );
  },
  page: (t) => <Label t={t} at={W("funeral", "crowd") - 0.3} text="THE FORUM" sub="20 March, 44 BC" />,
});

// 11. the coin: RRC 508/3, rebuilt from a Berlin Münzkabinett photograph (media/cast/eid_mar.glb, 19 mm, Brutus on
// +Z, the reverse on -Z): it opens on Brutus and turns to the cap, the daggers and EID MAR as the cap is named
scene({
  name: "coin", from: TL.at.coin - 0.4, dip: 0.7,
  cam: (t) => dolly({ from: TL.at.coin - 0.4, to: TL.at.close }, { pos: [0.1, 0.25, 4.6], look: [0, 0, 0], fov: 30 }, { pos: [0.05, 0.05, 3.7], look: [0, 0, 0], fov: 28 })(t),
  draw: (t) => (
    <>
      <Model name="coin" src={staticFile("cast/eid_mar.glb")} scale={66} rotation={[0.18 * Math.sin(t * 0.5), Math.PI * ease(t, W("coin", "cap") - 1.6, W("coin", "cap") - 0.1) + 0.22 * Math.sin((t - TL.at.coin) * 0.45) - 0.12, 0.05]} />
      {/* behind it, out of focus: dark worn stone, a warm pool of light on it */}
      <Mesh name="coin-wall" position={[0, 0, -1.6]}>
        <Box args={[7, 4, 0.1]} />
        <MeshStandardMaterial color="#5a4a3c" {...tex("worn_plaster_wall", [2.3, 1.3])} />
      </Mesh>
      <SpotLight name="coin-wash" position={[1.5, 1.2, 1.5]} target={[-0.6, 0.2, -1.6]} angle={0.4} penumbra={1} intensity={0.25} color="#ffb070" />
      <AmbientLight name="coin-amb" intensity={0.05} />
      <SpotLight name="coin-key" position={[-2.6, 2.2, 2.6]} target={[0, 0, 0]} angle={0.5} penumbra={0.8} intensity={1.2} color="#ffe1b8" />
      <SpotLight name="coin-rim" position={[2.4, -1.2, 1.4]} target={[0, 0, 0]} angle={0.6} penumbra={0.8} intensity={0.5} color="#9fb4e0" />
    </>
  ),
  page: (t) => <Label t={t} at={W("coin", "Eid") - 0.2} text="EID · MAR" sub="denarius of Brutus, 43–42 BC" />,
});

// 12. Augustus, then the city at sunset
const sWide = W("close", "The", 1);
scene({
  name: "augustus", from: TL.at.close - 0.4, dip: 0.7,
  cam: (t) => dolly({ from: TL.at.close - 0.4, to: sWide }, { pos: [0.6, 0.9, 5.2], look: [0, 1.9, 0], fov: 32 }, { pos: [0.4, 1.2, 4.0], look: eyeOf("augustus", [0, 0.6, 0], 2.1), fov: 32 })(t),
  draw: () => (
    <>
      <Figure who="augustus" name="aug-statue" at={[0, 0.6, 0]} h={2.1} />
      {marble("aug-plinth", manifold.roundedBox(1.2, 0.6, 1.2, 0.03).move(0, 0.3, 0), [0, 0, 0], "#a69d90")}
      {/* the city behind him, soft: a colonnade and a temple front in the low sun */}
      <Model name="aug-cols" src={ruin("003")} position={[-10, 0, -24]} rotation={[0, 0.3, 0]} />
      <Model name="aug-portico" src={ruin("004")} position={[12, 0, -30]} rotation={[0, -0.5, 0]} />
      <Floor name="aug-ground" color="#9c8e7c" surface="large_sandstone_blocks_01" tile={3} />
      <AmbientLight name="aug-amb" intensity={0.05} color="#4a5a8a" />
      <DirectionalLight name="sun" position={[-20, 9, 10]} intensity={1.8} color="#a9bce8" castShadow />
    </>
  ),
  page: (t) => <Label t={t} at={W("close", "Augustus.") - 0.2} text="AUGUSTUS" sub="first emperor of Rome, 27 BC" />,
});
scene({
  name: "sunset", from: sWide - 0.1,
  cam: (t) => dolly({ from: sWide, to: TL.done.close + 1.5 }, { pos: [-20, 3, 30], look: [0, 4, 0], fov: 34 }, { pos: [-48, 15, 66], look: [0, 5, 0], fov: 34 })(t),
  draw: () => (
    <>
      <Forum p="sun" />
      <AmbientLight name="sun-amb" intensity={0.06} color="#4a5a8a" />
      <DirectionalLight name="sun" position={[60, 14, 10]} intensity={1.6} color="#a9bce8" castShadow />
    </>
  ),
});

// 13. the end
scene({
  name: "end", from: TL.done.close + 1.6, dip: 1.2,
  cam: () => ({ pos: [0, 1.2, 6], look: [0, 1.4, 0], fov: 30, focus: 6 }),
  draw: (t) => (
    <>
      <SpotLight name="end-brazier" position={[0, -4, -2]} target={[0, 3, -2]} angle={1.0} penumbra={1} intensity={0.7 * (0.85 + 0.15 * Math.sin(t * 9) * Math.sin(t * 3.7))} color="#ff8a3d" volumetric />
      <FogVolume name="end-smoke" position={[0, 1.4, -2]} scale={[20, 12, 4]} density={0.02} color="#d9c9b8" scattering={0.6} absorption={0.2} anisotropy={0.3} />
      <Embers t={t} n={40} at={[0, -0.6, 0]} spread={5} rise={3.6} name="end-ember" glow={3} />
    </>
  ),
  page: (t) => {
    const s = TL.done.close + 1.6;
    const o = smooth((t - s - 0.6) / 1.0);
    const c = smooth((t - s - 2.4) / 1.0);
    return (
      <div style={{ position: "absolute", left: 0, width: 1920, top: 280, display: "flex", flexDirection: "column", alignItems: "center" }}>
        <div style={{ opacity: o, fontFamily: serif, fontWeight: 300, fontSize: 88, letterSpacing: "0.24em", marginRight: "-0.24em", lineHeight: 1, color: ivory, textShadow: shade }}>THE IDES OF MARCH</div>
        <div style={{ width: 260 * c, height: 1, background: gold, marginTop: 30 }} />
        <div style={{ opacity: c, fontFamily: serif, fontWeight: 500, fontSize: 25, lineHeight: 1.75, color: ash, textAlign: "center", marginTop: 30, textShadow: shade }}>
          {CREDITS.map((l) => <div key={l}>{l}</div>)}
        </div>
      </div>
    );
  },
});

export const CREDITS = [
  "Sources: Suetonius, Life of Julius Caesar · Plutarch, Caesar and Brutus · Appian, Civil Wars",
  "“Bust of Julius Caesar” and “Bust of Marcus Aurelius” by The Fitzwilliam Museum, “Roman Emperor Hadrian” by alieser,",
  "“Emperor Nero” by Moshe Caine (Sketchfab, CC BY 4.0, modified) · “Julius Caesar?”, Rijksmuseum van Oudheden (CC0)",
  "Casts of Augustus of Prima Porta and the Capitoline Brutus: SMK, National Gallery of Denmark (public domain) · Poly Haven (CC0)",
  "EID MAR denarius after a photograph by Reinhard Saczewski, Münzkabinett, Staatliche Museen zu Berlin (public domain)",
  "Drawn in sand after Baltard, Gérôme and Robertson · Fire, blood and the night sky simulated in raea",
  "Narration: Fish Audio · Music: Lyria · Made in raea · raea.io",
];

export const SCENES: Scene[] = SCN.map((s, i) => ({ ...s, to: i + 1 < SCN.length ? SCN[i + 1].from : TL.end }));
export const sceneAt = (t: number) => SCENES.find((s) => t >= s.from && t < s.to) ?? SCENES[SCENES.length - 1];
