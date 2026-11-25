// raea bench denoise's small cases (docs/denoise.feature DN0): one closed room, 8 m wide, 3 high and 12 deep, with a
// door to a second room behind its far wall, each composition a thing denoisers get wrong. Every case is Solari-lit,
// so the bench's truth (RAEA_LIGHT=pathtracer) draws the same scene converged. 960x540 at 30 fps; "cost" is the same
// hall at 1920x1080, played live for a frame's cost.
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "raea";
import { ToneMapping } from "raea/3d";
import { Grade } from "raea/post_process";
import { StandardMaterial, VolumetricFog, FogVolume } from "raea/pbr";
import { Cuboid, Mesh3d } from "raea/mesh";
import { DirectionalLight, SolariLighting } from "raea/light";
import { LinearRgba } from "raea/color";
import { Transform } from "raea/transform";
import { Camera3d } from "raea/camera";

type V3 = [number, number, number];
type Box = { name: string; min: V3; max: V3; color?: string; rough?: number; metal?: number; glow?: V3 };

// the room's shell: 20 cm boxes, the far wall (z -12) with a door 1.2 m wide and 2.2 high into the back room
const SHELL: Box[] = [
  { name: "floor", min: [-4.2, -0.2, -20.2], max: [4.2, 0, 0.2], color: "#8a8276" },
  { name: "ceiling", min: [-4.2, 3, -20.2], max: [4.2, 3.2, 0.2] },
  { name: "wall-l", min: [-4.2, 0, -20.2], max: [-4, 3, 0.2] },
  { name: "wall-near", min: [-4.2, 0, 0], max: [4.2, 3, 0.2] },
  { name: "wall-back", min: [-4.2, 0, -20.2], max: [4.2, 3, -20] },
  { name: "mid-l", min: [-4, 0, -12.2], max: [-0.6, 3, -12] },
  { name: "mid-r", min: [0.6, 0, -12.2], max: [4, 3, -12] },
  { name: "mid-top", min: [-0.6, 2.2, -12.2], max: [0.6, 3, -12] },
  // the right wall with a window from z -9 to -5, 1 to 2.2 m up
  { name: "wall-r-low", min: [4, 0, -20.2], max: [4.2, 1, 0.2] },
  { name: "wall-r-high", min: [4, 2.2, -20.2], max: [4.2, 3, 0.2] },
  { name: "wall-r-back", min: [4, 1, -20.2], max: [4.2, 2.2, -9] },
  { name: "wall-r-front", min: [4, 1, -5], max: [4.2, 2.2, 0.2] },
];
// things to light and reflect: crates, a column, a coloured panel
const PROPS: Box[] = [
  { name: "crate-a", min: [-3, 0, -8], max: [-2, 1, -7], color: "#b04a32" },
  { name: "crate-b", min: [1.5, 0, -10], max: [2.7, 0.8, -8.8], color: "#3a6ea8" },
  { name: "column", min: [-0.3, 0, -5.3], max: [0.3, 3, -4.7] },
  { name: "panel", min: [-3.95, 0.6, -4], max: [-3.9, 1.8, -2], color: "#d8b13a" },
];
// the back room's light: a ceiling panel seen through the door
const BACK_LAMP: Box = { name: "back-lamp", min: [-1, 2.95, -17], max: [1, 3, -15], color: "#ffffff", glow: [900, 860, 780] };
// the hall's ceiling tubes
const TUBES: Box[] = [-2, 2].flatMap((x) => [-3, -8].map((z) => ({ name: `tube-${x}-${z}`, min: [x - 0.06, 2.96, z - 1] as V3, max: [x + 0.06, 3, z + 1] as V3, color: "#ffffff", glow: [2600, 2700, 2500] as V3 })));

function Part({ b, k = 1 }: { b: Box; k?: number }) {
  const c: V3 = [(b.min[0] + b.max[0]) / 2, (b.min[1] + b.max[1]) / 2, (b.min[2] + b.max[2]) / 2];
  const half: V3 = [(b.max[0] - b.min[0]) / 2, (b.max[1] - b.min[1]) / 2, (b.max[2] - b.min[2]) / 2];
  const g = b.glow ? new LinearRgba(b.glow[0] * k, b.glow[1] * k, b.glow[2] * k, 1) : undefined;
  return (
    <Mesh3d name={b.name} castShadow={!b.glow} transform={Transform.fromXyz(...c)}>
      <Cuboid half_size={half} />
      <StandardMaterial base_color={b.color ?? "#d6d0c2"} perceptual_roughness={b.rough ?? 0.75} metallic={b.metal ?? 0} {...(g ? { emissive: g } : {})} />
    </Mesh3d>
  );
}

type Case = {
  cam: (f: number) => Transform;
  extra?: Box[];
  floor?: Partial<Box>;
  tubes?: (f: number) => number;
  sun?: boolean;
  fog?: boolean;
  gain?: number;
  hide?: (f: number) => string[];
};

// a slow dolly down the hall, looking at the door
const dolly = (f: number) => Transform.fromXyz(1.5, 1.6, -1 - f * 0.03).lookingAt([0, 1.2, -16]);
const still = () => Transform.fromXyz(2.8, 1.6, -1.5).lookingAt([-1, 1, -11]);

// leaves: thin blades in a grid in front of the window, each turned by its place
const LEAVES: Box[] = Array.from({ length: 120 }, (_, i) => {
  const x = 3.2 + (i % 4) * 0.18, y = 0.9 + Math.floor(i / 4) % 10 * 0.13, z = -8.8 + Math.floor(i / 40) * 1.3 + ((i * 7) % 11) * 0.09;
  return { name: `leaf-${i}`, min: [x, y, z] as V3, max: [x + 0.12, y + 0.015, z + 0.05] as V3, color: "#3f6b2a", rough: 0.6 };
});

const CASES: Record<string, Case> = {
  // a dark room lifted 3 stops (18 dB) in the grade, as a camcorder's gain lifts it: only the back room's lamp, dimmed
  "dark-gain": { cam: still, tubes: () => 0, gain: 3 },
  // a waxed floor reflecting the tubes, the crates and the door
  "waxed-floor": { cam: dolly, floor: { color: "#5a544c", rough: 0.08 } },
  // still camera; at frame 15 the door's slab is gone, and the back room is seen at once (a disocclusion)
  doorway: { cam: still, extra: [{ name: "door", min: [-0.6, 0, -12.15], max: [0.6, 2.2, -12.05], color: "#5b4a3a" }], hide: (f) => (f >= 15 ? ["door"] : []) },
  // a whip pan: 100 degrees in 6 frames (12-17), then held
  "whip-pan": { cam: (f) => { const t = Math.min(1, Math.max(0, (f - 12) / 6)); const a = -0.5 + t * 1.75; return Transform.fromXyz(0, 1.6, -6).lookingAt([Math.sin(a) * 5, 1.4, -6 - Math.cos(a) * 5]); } },
  // neon: the tubes off, three thin saturated signs in a dark hall
  neon: { cam: dolly, tubes: () => 0, extra: [
    { name: "sign-r", min: [-3.95, 1.9, -9], max: [-3.9, 1.95, -6], color: "#ffffff", glow: [1800, 120, 260] },
    { name: "sign-g", min: [-3.95, 1.5, -9], max: [-3.9, 1.55, -6], color: "#ffffff", glow: [100, 1600, 400] },
    { name: "sign-b", min: [3.9, 2.3, -11], max: [3.95, 2.35, -7], color: "#ffffff", glow: [160, 300, 2200] },
  ] },
  // foliage: 120 leaves in front of a sunlit window
  foliage: { cam: (f) => Transform.fromXyz(1, 1.5, -3 - f * 0.02).lookingAt([4, 1.5, -7.5]), extra: LEAVES, sun: true, tubes: () => 0.2 },
  // a sunbeam through the window in dusty air
  fog: { cam: still, sun: true, fog: true, tubes: () => 0.2 },
  // a mirror on the left wall, the camera moving past it
  mirror: { cam: (f) => Transform.fromXyz(1.5, 1.6, -2 - f * 0.04).lookingAt([-4, 1.4, -7]), extra: [{ name: "mirror", min: [-3.95, 0.5, -9], max: [-3.9, 2.5, -5], color: "#ffffff", rough: 0, metal: 1 }] },
  // the tubes switched off at frame 15; the back room's lamp stays on
  "light-off": { cam: still, tubes: (f) => (f >= 15 ? 0 : 1) },
};

function Room({ c }: { c: Case }) {
  const f = useCurrentFrame();
  const hidden = c.hide?.(f) ?? [];
  const k = c.tubes?.(f) ?? 1;
  const shell = SHELL.map((b) => (b.name === "floor" ? { ...b, ...c.floor } : b));
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Camera3d transform={c.cam(f)} projection={{ Perspective: { fov: (60 * Math.PI) / 180 } }}>
        <SolariLighting bounces={3} />
      </Camera3d>
      <ToneMapping mode="agx" />
      {c.gain ? <Grade exposure={c.gain} /> : null}
      {c.sun ? <DirectionalLight illuminance={50000} shadow_maps_enabled volumetric={!!c.fog} transform={Transform.fromXyz(20, 8, -4).lookingAt([0, 0, -7])} /> : null}
      {c.fog ? <VolumetricFog /> : null}
      {c.fog ? <FogVolume name="dust" position={[0, 1.5, -6]} scale={[8, 3, 12]} density={0.08} /> : null}
      {[...shell, ...PROPS, ...(c.extra ?? [])].filter((b) => !hidden.includes(b.name)).map((b) => <Part key={b.name} b={b} />)}
      {k > 0 ? TUBES.map((b) => <Part key={b.name} b={b} k={k} />) : null}
      <Part b={BACK_LAMP} k={c.tubes && k === 0 && !c.gain ? 1 : c.gain ? 0.05 : 1} />
    </AbsoluteFill>
  );
}

registerRoot(() => (
  <>
    {Object.entries(CASES).map(([id, c]) => <Composition key={id} id={id} component={() => <Room c={c} />} width={960} height={540} fps={30} durationInFrames={40} />)}
    <Composition id="cost" component={() => <Room c={CASES["waxed-floor"]} />} width={1920} height={1080} fps={30} durationInFrames={240} />
  </>
));
