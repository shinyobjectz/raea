// "The cat learns it can make music by jumping": 30 s, 1920x1080, void's cat (glowing white, a flat shape like the
// logo) in a black void, the world in pixel art. story.py is the one timeline: the cat's Cinematic clip, every landing (frame, tile, note, colour) and its
// root's path (story.json). Each landing plays its note and lights its tile in the note's colour; the colours stay on
// the floor as a painted melody until the big jump floods them all.
//
// The world is pixel art drawn as pixel art: flat colours from a small palette, no light on it (so no gradients, sheen
// or fog), moving on twos, its pool of light three floor tones stepping out from the cat, each note a tile popping up in
// its colour and fading in hard steps; the frame then in cells of 10 pixels. The cat glows, always pure white, a flat
// shape like the logo, moving at the film's 30. A hand-held drift on every shot.
// Shots (s):  A 0-3 out of the dark, focus finding the cat   B 3-8.4 a low arc as it looks round, near tiles soft
//             C 8.4-11 the first note: a dolly-zoom on its stare   D 11-15 at floor level, tracking its first hops
//             E 15-19 cut on every bar between two angles   F 19-22 crane up over the painted melody
//             G 22-24.2 beside the crouch, pushing in; the lens opens as it leaps   H 24.2-27.2 straight down on the
//             flood, turning   I 27.2-30 focus pulled from the colours at our feet to the cat, and out
// Score (120 bpm, Tone on raea): a marimba (TokenSynth) on every landing, through a dotted-eighth delay; a pad drone
// that turns into C-Am-F-G when it finds the beat (15); kick (16) and bass (18); a riser under the crouch; the chord,
// a crash and a run at the big landing (24.2); a soft paw tap (Stable Audio Open) under each landing.
import { AbsoluteFill, Composition, Easing, Mix, interpolate, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import { Box, Canvas3D, Cylinder, Look, Mesh, MeshBasicMaterial, Model, PerspectiveCamera, Torus } from "@raea/motion/3d";
import * as Tone from "@raea/motion/tone";
import story from "./story.json";
import { FONTS } from "./fonts.gen";

const FPS = 30;
const FRAMES = 1020; // the film is 900; the outro 4 s more
const T = story.tile;
const CHORD = 24.2;
const ease = Easing.inOut(Easing.cubic);
const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;
const lerp = (a: number[], b: number[], u: number) => a.map((v, i) => v + (b[i] - v) * u);
const smooth = (u: number) => { const x = Math.min(1, Math.max(0, u)); return x * x * (3 - 2 * x); };
// the key light out of the dark at the start, and back into it at the end (story.light)
const light = (t: number) => smooth(t / 2.5) * (1 - smooth((t - 29.3) / 0.7));

// ---------------------------------------------------------------- the score

Tone.getTransport().bpm.value = story.bpm;
const reverb = new Tone.Reverb({ decay: 4.5, wet: 0.32 }).connect(Tone.buses.music);
const echo = new Tone.FeedbackDelay({ delayTime: "8n.", feedback: 0.28, wet: 0.22 }).connect(reverb);
const marimba = new Tone.Sampler({ urls: Tone.tokensynth("a soft wooden marimba, felt mallet, warm and round", ["C3", "C4", "C5"], { seconds: 3 }), volume: -4 }).connect(echo);
const pad = new Tone.PolySynth(Tone.Synth, { oscillator: { type: "fatsine" }, envelope: { attack: 2.2, decay: 1, sustain: 0.8, release: 3.5 }, volume: -21 }).connect(reverb);
const kick = new Tone.MembraneSynth({ octaves: 5, pitchDecay: 0.04, envelope: { attack: 0.001, decay: 0.35, sustain: 0 }, volume: -8 }).connect(Tone.buses.music);
const bass = new Tone.MonoSynth({ oscillator: { type: "triangle" }, filter: { Q: 1, type: "lowpass" }, filterEnvelope: { attack: 0.01, decay: 0.3, sustain: 0.4, baseFrequency: 120, octaves: 2 }, envelope: { attack: 0.02, decay: 0.3, sustain: 0.7, release: 0.6 }, volume: -12 }).connect(Tone.buses.music);
const hat = new Tone.MetalSynth({ envelope: { attack: 0.001, decay: 0.05, release: 0.01 }, harmonicity: 5.1, resonance: 5000, octaves: 1.5, volume: -32 }).connect(reverb);
const crash = new Tone.MetalSynth({ envelope: { attack: 0.001, decay: 2.2, release: 1 }, harmonicity: 3.1, resonance: 3200, octaves: 1.2, volume: -24 }).connect(reverb);
const riser = new Tone.NoiseSynth({ noise: { type: "pink" }, envelope: { attack: 1.7, decay: 0.05, sustain: 1, release: 0.08 }, volume: -20 }).connect(new Tone.Filter({ frequency: 2400, type: "bandpass", Q: 0.7 }).connect(reverb));
const air = new Tone.Noise({ type: "brown", volume: -38 }).connect(new Tone.Filter({ frequency: 380, type: "lowpass" }).connect(Tone.buses.ambience));
const paw = Tone.vary(new Tone.Sampler({ urls: { C4: Tone.sao("a single soft cat paw landing on a felt floor, tiny muffled thump, dry, no reverb", { seconds: 0.6 }) }, volume: -10 }), { pitch: 0.4, volume: 2 }).connect(Tone.buses.sfx);

air.start(0).stop(29.8);
// the outro's sting: the cat's note as the logo forms, a fifth over it with the name
marimba.triggerAttackRelease(["C4", "C5"], "2n", 30.25);
marimba.triggerAttackRelease(["G4", "E5"], "2n", 30.95);
pad.triggerAttackRelease(["C3", "G3", "E4"], 2.6, 30.3, 0.15);
// the drone, then the progression once it has the beat, then the chord it lands on
pad.triggerAttackRelease(["C3", "G3"], 14.6, 0.2);
const PROG: [number, string[]][] = [[15, ["C3", "E3", "G3", "B3"]], [17, ["A2", "C3", "E3", "G3"]], [19, ["F2", "A2", "C3", "E3"]], [21, ["G2", "B2", "D3", "F3"]]];
for (const [at, notes] of PROG) pad.triggerAttackRelease(notes, 1.9, at);
pad.triggerAttackRelease(["C3", "G3", "B3", "D4", "E4"], 5.0, CHORD);
// the rhythm comes in once the cat has found it, and stops for the crouch
for (let s = 16; s < 22; s += 1) kick.triggerAttackRelease("C1", "8n", s);
for (let s = 18.25; s < 22; s += 0.5) hat.triggerAttackRelease("32n", s);
const BASS: [number, string, number][] = [[16, "C2", 1.8], [18, "A1", 1.8], [20, "F1", 1.8], [21.5, "G1", 0.9]];
for (const [at, n, d] of BASS) bass.triggerAttackRelease(n, d, at);
riser.triggerAttackRelease(1.8, CHORD - 1.8);
// the big landing: the chord, a crash, a kick, a run up the scale
kick.triggerAttackRelease("C1", "4n", CHORD);
bass.triggerAttackRelease("C2", 3.5, CHORD);
crash.triggerAttackRelease("2n", CHORD);
// every landing: its note on the marimba, a paw tap under it
for (const l of story.landings) {
  if (l.note === "chord") {
    marimba.triggerAttackRelease(["C4", "E4", "G4"], "2n", l.t);
    ["C5", "E5", "G5", "C6"].forEach((n, i) => marimba.triggerAttackRelease(n, "8n", l.t + 0.25 + i * 0.125));
  } else {
    marimba.triggerAttackRelease(l.note, "4n", l.t);
  }
  paw.triggerAttackRelease("C4", "16n", l.t);
}
Tone.getTransport().start();

// ---------------------------------------------------------------- the camera

// the cat's root, eased so the camera follows it like an operator would, not locked to every hop
const FOLLOW: number[][] = [];
{
  let c = story.root[0].slice(0, 3);
  for (let f = 0; f <= story.frames; f++) {
    const r = story.root[f];
    c = lerp(c, [r[0], r[1] * 0.35, r[2]], 0.09);
    FOLLOW.push(c);
  }
}

type Shot = { pos: number[]; look: number[]; fov: number; focus?: number };

const dist = (a: number[], b: number[]) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);

/** An operator's hands: a slow drift of a few millimetres, never the same twice (sums of incommensurate sines). */
function hands(t: number, amp: number): number[] {
  const w = (a: number, b: number, c: number) => Math.sin(t * a + c) * 0.6 + Math.sin(t * b + c * 1.7) * 0.4;
  return [w(0.71, 1.93, 0.3) * amp, w(0.53, 1.37, 1.1) * amp * 0.7, w(0.61, 1.71, 2.3) * amp];
}

/** The cat's head, where the camera mostly wants to be focused. */
function head(f: number): number[] {
  const r = story.root[f];
  return [r[0], 0.45 + r[1], r[2]];
}

function shot(f: number): Shot {
  const t = f / FPS;
  const c = FOLLOW[f];
  // A 0-3: out of the dark, the grain out of focus; focus finds the cat as the light does
  if (t < 3) {
    const u = ease(t / 3);
    const pos = lerp([0.4, 0.5, 6.4], [0.15, 0.45, 4.2], u);
    return { pos, look: [0, 0.45, 0], fov: 26, focus: interpolate(t, [0, 2.6], [0.6, dist(pos, [0, 0.45, 0])], { ...clamp, easing: ease }) };
  }
  // B 3-8.4: low, slow arc, hand-held, the near tiles soft in front of it
  if (t < 8.4) {
    const a = interpolate(t, [3, 8.4], [0.55, -0.45], { easing: ease });
    const pos = [3.3 * Math.sin(a), 0.3, 3.3 * Math.cos(a)];
    return { pos, look: [0, 0.4, 0], fov: 30 };
  }
  // C 8.4-11: the first note. A slow dolly-zoom as it stares at the tile: the camera backs off as the lens closes in,
  // so the cat holds its size and the world behind it falls away
  if (t < 11) {
    const u = ease(Math.min(1, Math.max(0, (t - 8.6) / 2.2)));
    const d = 1.55 + u * 1.9;
    const fov = (2 * Math.atan(0.72 / d) * 180) / Math.PI;
    return { pos: [0.55 * (d / 1.55), 0.3, d], look: [0, 0.42, 0], fov };
  }
  // D 11-15: at the floor's level, tracking beside it: tiles rush through the soft foreground
  if (t < 15) {
    return { pos: [c[0] + 1.0, 0.12, c[2] + 2.9], look: [c[0] + 0.15, 0.36, c[2]], fov: 30 };
  }
  // E 15-19: it has the beat; so does the edit. A cut every bar (2 beats) between two angles, each pushing in
  if (t < 19) {
    const bar = Math.floor((t - 15) / 1.0);
    const u = (t - 15 - bar) / 1.0;
    if (bar % 2 === 0) {
      return { pos: [c[0] - 1.6 + u * 0.25, 0.5, c[2] + 1.5 - u * 0.2], look: [c[0], 0.35, c[2]], fov: 34 };
    }
    const top = [c[0] + 0.5, 2.3 - u * 0.25, c[2] + 1.7];
    return { pos: top, look: [c[0], 0.05, c[2]], fov: 38, focus: dist(top, [c[0], 0.3, c[2]]) };
  }
  // F 19-22: up and away: the melody it's painted, the grain of the floor in focus
  if (t < 22) {
    const u = ease((t - 19) / 3);
    const pos = lerp([0.6, 2.4, 4.6], [0.2, 6.4, 3.4], u);
    const look = lerp([c[0], 0.3, c[2]], [0, 0, 0.2], u);
    return { pos, look, fov: 36 };
  }
  // G 22-24.2: the crouch. Pushing in on its face, the lens long, everything but the eyes going soft; then the leap
  if (t < 24.2) {
    const u = ease(Math.min(1, (t - 22) / 1.15));
    const r = story.root[f];
    // beside where it crouches, square to the leap (from there to the middle), so the jump crosses the frame
    const p0 = story.root[22 * FPS];
    const len = Math.hypot(p0[0], p0[2]) || 1;
    const side = [-p0[2] / len, p0[0] / len];
    const pos = lerp([p0[0] + side[0] * 2.4 + p0[0] * 0.3, 0.3, p0[2] + side[1] * 2.4 + p0[2] * 0.3], [p0[0] + side[0] * 1.5 + p0[0] * 0.2, 0.34, p0[2] + side[1] * 1.5 + p0[2] * 0.2], u);
    // pushed in until it leaves the floor (23.2), then the lens opens with the leap and the camera tilts all the way up with it
    const look = [r[0] * 0.8 + c[0] * 0.2, 0.4 + r[1] * 0.9, r[2] * 0.8 + c[2] * 0.2];
    return { pos, look, fov: interpolate(t, [22, 23.15, 23.6], [38, 30, 46], { ...clamp, easing: ease }), focus: dist(pos, head(f)) };
  }
  // H 24.2-27.2: the flood, from straight above, turning, pulling out
  if (t < 27.2) {
    const u = ease((t - 24.2) / 3);
    const a = 0.4 + (t - 24.2) * 0.35;
    const h = 2.0 + u * 3.2;
    const pos = [0.35 * Math.sin(a), h, 0.35 * Math.cos(a)];
    return { pos, look: [0, 0, 0], fov: 44, focus: h };
  }
  // I 27.2-30: down in front of it, focus pulled from the colours at our feet to the cat, and out
  const u = ease((t - 27.2) / 2.8);
  const pos = lerp([0.1, 0.26, 2.7], [0, 0.36, 2.25], u);
  return { pos, look: [0, 0.42, 0], fov: 28, focus: interpolate(t, [27.4, 28.6], [0.5, dist(pos, [0, 0.45, 0])], { ...clamp, easing: ease }) };
}

/** Shot boundaries (s): each cut is softened by a quick pull out of focus and back (a focus wipe), the grain doing
 * the rest. */
const CUTS = [3, 8.4, 11, 15, 19, 22, 24.2, 27.2];

/** The camera at frame f: the shot, the operator's hands, and its focus (the shot's, else the cat's head). */
function camera(f: number): Required<Shot> {
  const t = f / FPS;
  const s = shot(f);
  const shake = hands(t, t >= 11 && t < 15 ? 0.018 : 0.008);
  const pos = s.pos.map((v, i) => v + shake[i]);
  const look = s.look.map((v, i) => v + shake[i] * 0.4);
  let focus = s.focus ?? dist(pos, head(f));
  const near = Math.min(...CUTS.map((c) => Math.abs(t - c)));
  focus *= 1 + 1.4 * Math.max(0, 1 - near / 0.18);
  return { pos, look, fov: s.fov, focus };
}

// ---------------------------------------------------------------- the floor: tiles, the colour each landing leaves, rings
//
// The world is drawn as pixel art, not lit and then pixelated: every surface is a flat colour (no light reaches it, so
// no gradient, sheen or fog), from a small palette, and it moves on twos (12 steps a second), the way pixel animation
// does; only the cat moves at the film's 30. Its pool of light is three floor tones stepping out from it, and each
// note is a tile popping up in its colour and fading in hard steps.

const GRID: [number, number][] = [];
for (let i = -4; i <= 4; i++) for (let j = -4; j <= 4; j++) GRID.push([i, j]);
const FLOOD = ["#ff5e57", "#ff9f43", "#ffd93d", "#4ade80", "#38bdf8", "#a78bfa"];
const NOTES = story.landings.filter((l) => l.note !== "chord");
/** the world's clock, on twos */
const STEP = 12;
const twos = (t: number) => Math.floor(t * STEP + 1e-6) / STEP;
/** the floor's tones, near the cat to far, then the void's */
const TONES = ["#34343c", "#22222a", "#16161b", "#0c0c0f"];
const VOID = "#000000";
const hex = (c: string) => [1, 3, 5].map((k) => parseInt(c.slice(k, k + 2), 16));
const mixHex = (a: string, b: string, u: number) => "#" + hex(a).map((x, k) => Math.round(x + (hex(b)[k] - x) * u).toString(16).padStart(2, "0")).join("");
/** the light's level in three steps (the dark, half, full): the world comes up out of the dark and goes back in steps */
const lit = (t: number) => Math.round(light(t) * 3) / 3;
/** a floor tile's tone: by how far it is from the cat, in tile steps, dimmed with the light */
function floorTone(x: number, z: number, r: number[], t: number) {
  const d = Math.hypot(x - r[0], z - r[2]) / T;
  const k = d < 1.25 ? 0 : d < 2.5 ? 1 : d < 3.9 ? 2 : 3;
  const l = lit(t);
  return l <= 0 ? VOID : mixHex(VOID, TONES[k], l);
}
/** a colour laid on the floor at `at`: popped up full, then in hard steps down to what stays (`keep` of it) */
function stepped(colour: string, base: string, at: number, t: number, keep = 0.45) {
  const a = t - at;
  if (a < 0) return null;
  const u = a < 0.25 ? 1 : a < 0.5 ? (1 + keep) / 2 : keep;
  return mixHex(base, colour, u * (lit(t) > 0 ? 1 : 0));
}

function Floor({ t, r }: { t: number; r: number[] }) {
  const te = twos(t);
  return (
    <>
      {GRID.map(([i, j]) => (
        <Mesh key={`tile-${i}-${j}`} name={`tile-${i}-${j}`} position={[i * T, -0.03, j * T]}>
          <Box args={[T * 0.9, 0.05, T * 0.9]} />
          <MeshBasicMaterial color={floorTone(i * T, j * T, r, te)} />
        </Mesh>
      ))}
      {NOTES.map((l, k) => {
        // a tile landed on again gets its new colour over the old one, a hair higher (never level: then which shows is luck)
        const c = stepped(l.color, floorTone(l.x, l.z, r, te), l.t, te);
        const a = te - l.t;
        const pop = a < 0 ? 0 : a < 1 / STEP ? 1.16 : a < 2 / STEP ? 1.07 : 1;
        return (
          <Mesh key={`glow-${k}`} name={`glow-${k}`} position={[l.x, 0.006 + k * 0.0015 + (pop > 1 ? 0.02 : 0), l.z]} scale={pop || 0.0001}>
            <Box args={[T * 0.9, 0.012, T * 0.9]} />
            <MeshBasicMaterial color={c ?? VOID} />
          </Mesh>
        );
      })}
      {GRID.map(([i, j]) => {
        // the big jump floods the floor with colour, ring by ring out from the middle
        const d = Math.hypot(i, j);
        const c = stepped(FLOOD[Math.floor(d * 1.3) % FLOOD.length], floorTone(i * T, j * T, r, te), CHORD + Math.floor(d) * 0.12, te, 0.7);
        return (
          <Mesh key={`flood-${i}-${j}`} name={`flood-${i}-${j}`} position={[i * T, -0.002, j * T]} scale={c ? 1 : 0.0001}>
            <Box args={[T * 0.84, 0.012, T * 0.84]} />
            <MeshBasicMaterial color={c ?? VOID} />
          </Mesh>
        );
      })}
      {story.landings.map((l, k) => {
        // a flat ring spreading from each landing in steps, gone when it's spent
        const age = te - l.t;
        const life = l.big ? 1.8 : 1.1;
        const on = age >= 0 && age < life && lit(t) > 0;
        const u = Math.min(1, Math.max(0, age / life));
        const rr = on ? 0.05 + Easing.out(Easing.cubic)(u) * (l.big ? 4.2 : 1.1) : 0.0001;
        return (
          <Mesh key={`ring-${k}`} name={`ring-${k}`} position={[l.x, 0.03, l.z]} rotation={[Math.PI / 2, 0, 0]} scale={[rr, rr, 1]}>
            <Torus args={[1, 0.035]} />
            <MeshBasicMaterial color={u < 0.6 ? (l.big ? "#ffffff" : l.color) : mixHex(VOID, l.big ? "#ffffff" : l.color, 0.5)} />
          </Mesh>
        );
      })}
      {/* the cat's shadow: a flat dark disc under it */}
      <Mesh name="shadow" position={[r[0], 0.004, r[2]]}>
        <Cylinder args={[0.2, 0.2, 0.004]} />
        <MeshBasicMaterial color={lit(t) > 0 ? "#08080a" : VOID} />
      </Mesh>
    </>
  );
}

// ---------------------------------------------------------------- the film

function Film() {
  const frame = useCurrentFrame();
  const f = Math.min(frame, story.frames);  // the outro holds the film's last (dark) frame under it
  const t = f / FPS;
  const cam = camera(f);
  const r = story.root[f];
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Mix lufs={-14} ceiling={-1.5} />
      <Canvas3D>
        <PerspectiveCamera position={cam.pos as [number, number, number]} lookAt={cam.look as [number, number, number]} fov={cam.fov} focus={cam.focus} />
        {/* everything in the site's grain, a cell to 5 pixels: the cat, the floor, the light's falloff, the blur */}
        {/* the whole frame in cells of 10 pixels; the world's already flat colours, so the cells only snap its edges */}
        <Look mode="pixel" pixelSize={10} />
        <Floor t={t} r={r} />
        <Motes t={t} />
        {/* the cat glows (cat-glow.glb: white fur, emissive 6), so it's always pure white, never taking light or shadow:
            a flat shape against the world, as the logo is */}
        <Model src={staticFile("cat-glow.glb")} animation="Cinematic" loop={false} />
      </Canvas3D>
      {frame >= OUTRO * FPS ? <Outro t={frame / FPS - OUTRO} /> : null}
    </AbsoluteFill>
  );
}

// ---------------------------------------------------------------- the outro: the logo, the same cat, in the same grain

const OUTRO = 30.1;
// the site's pixel cat (apps/site/public/cat.svg), a cell to a character; its eyes are the gaps in rows 11-13
const CAT = [
  "........###.....................",
  "........###.....................",
  "........####....................",
  "........####....###.............",
  "........####...#####.....###....",
  "........####...#####....####....",
  "........#####.##############....",
  "........#####.##############....",
  "........#####.##############....",
  "........#####.##############....",
  "........####################....",
  ".......#############.####.###...",
  "......##############.####.###...",
  "......##############.####.###...",
  ".....########################...",
  ".....########################...",
  "....#########################...",
  "....#########################...",
  "....########################....",
  "....#######################.....",
  "...#########################....",
  "...#########################....",
  "...##########################...",
  "..######.####################...",
  "..#####....###################..",
  "..#####.....#########...######..",
  "..####......######.......######.",
  "..####......######........#####.",
  "...##.......#####...........##..",
  "............#####...............",
  "............#####...............",
  ".............###................",
];
const CELL = 13;
const BAYER = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

function Outro({ t }: { t: number }) {
  // the cat dithers in, a cell at a time (a Bayer threshold, swept up from its feet), as the film's grain did
  const grow = smooth(t / 0.9);
  const blink = t > 2.3 && t < 2.42;
  const word = smooth((t - 0.75) / 0.45);
  const url = "raea.io".slice(0, Math.max(0, Math.floor((t - 1.35) / 0.06)));
  const fade = 1 - smooth((t - 3.55) / 0.45);
  const x0 = 960 - 240 - 32 * CELL / 2, y0 = 540 - 16 * CELL - 30;
  return (
    <AbsoluteFill style={{ background: "#000000", opacity: fade }}>
      <style>{FONTS}</style>
      <svg width={1920} height={1080} style={{ position: "absolute", left: 0, top: 0 }}>
        {CAT.flatMap((row, j) => [...row].map((c, i) => {
          const eye = blink && j >= 11 && j <= 13 && (i === 20 || i === 25);
          if (c !== "#" && !eye) return null;
          const at = 0.55 * (BAYER[(j % 4) * 4 + (i % 4)] + 0.5) / 16 + 0.45 * (31 - j) / 31;
          return grow > at ? <rect key={`${i}-${j}`} x={x0 + i * CELL} y={y0 + j * CELL} width={CELL} height={CELL} fill="#ffffff" /> : null;
        }))}
      </svg>
      <div style={{ position: "absolute", left: 960 - 240 + 32 * CELL / 2 + 40, top: 540 - 150, fontFamily: "Geist", fontWeight: 600, fontSize: 210, letterSpacing: "-0.04em", color: "#ffffff", opacity: word, transform: `translateY(${(1 - word) * 18}px)` }}>raea</div>
      <div style={{ position: "absolute", left: 0, width: 1920, top: 540 + 16 * CELL + 70, textAlign: "center", fontFamily: "Geist Mono", fontWeight: 500, fontSize: 40, letterSpacing: "0.08em", color: "#9a9a9a" }}>{url}</div>
    </AbsoluteFill>
  );
}

// ---------------------------------------------------------------- dust in the light: single cells of grain drifting

const MOTES = Array.from({ length: 36 }, (_, i) => {
  const h = (k: number) => {
    const x = Math.sin(i * 12.9898 + k * 78.233) * 43758.5453;
    return x - Math.floor(x);
  };
  return { x: (h(1) - 0.5) * 5, y: 0.15 + h(2) * 1.6, z: (h(3) - 0.5) * 5, sp: 0.03 + h(4) * 0.05, ph: h(5) * 6.28 };
});

function Motes({ t }: { t: number }) {
  // single white cells, drifting on twos and blinking on and off
  const te = twos(t);
  return (
    <>
      {MOTES.map((m, i) => (
        <Mesh key={`mote-${i}`} name={`mote-${i}`} position={[m.x + Math.sin(te * 0.3 + m.ph) * 0.2, m.y + ((te * m.sp) % 1.2) - 0.3, m.z + Math.cos(te * 0.25 + m.ph) * 0.2]} scale={Math.sin(te * 1.3 + m.ph) > -0.4 && lit(t) > 0 ? 1 : 0.0001}>
          <Box args={[0.03, 0.03, 0.03]} />
          <MeshBasicMaterial color="#ffffff" />
        </Mesh>
      ))}
    </>
  );
}

registerRoot(() => <Composition id="cat-cinematic" component={Film} width={1920} height={1080} fps={FPS} durationInFrames={FRAMES} />);
