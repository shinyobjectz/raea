// The Ides of March: a narrated history short in raea. Fish reads script.json (media/voice/<line>.mp3, at 0.85 speed);
// the picture is cut to the narration's words (voice.json: each script word with its time, from whisper-cli). Two
// Lyria cues score it: a still drone for the open and the close, and the score whose climax (88 s into it) lands on
// "Casca struck first". timeline.json places every line; shots.tsx has the scenes.
import { AbsoluteFill, Audio, Composition, Img, Mix, Sequence, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import { AmbientOcclusion, AntiAliasing, Bloom, Canvas3D, DepthOfField, Environment, Fog, Grade, PerspectiveCamera, Shadows, ToneMapping, VolumetricFog } from "@raea/motion/3d";
import { sao } from "@raea/motion/tone";
import { FONTS } from "./fonts.gen";
import { Caps, Italic, Letters, Reveal, Rule, capsW, gold, italW, ivory, shown } from "./type";
import { LIGHTNING, QUOTED, SCENES, sceneAt } from "./shots";
import { BAR, FPS, LINES, TL, clamp01, smooth, words } from "./time";

const FRAMES = Math.ceil(TL.end * FPS);

// ---------------------------------------------------------------- the words on screen

/** The narration in phrases: broken at punctuation, at most seven words, each shown from its first word to its last. */
const PHRASES = LINES.flatMap((l) => {
  const out: { a: number; b: number; text: string }[] = [];
  let cur: { w: string; a: number; b: number }[] = [];
  const flush = () => {
    if (cur.length) out.push({ a: l.at + cur[0].a, b: l.at + cur[cur.length - 1].b, text: cur.map((w) => w.w).join(" ") });
    cur = [];
  };
  for (const w of words(l.id)) {
    cur.push(w);
    if (/[.,;:?!]$/.test(w.w) || cur.length >= 7) flush();
  }
  flush();
  return out;
});

function Captions({ t }: { t: number }) {
  const p = PHRASES.find((x) => t >= x.a - 0.05 && t < x.b + 0.25);
  if (!p || QUOTED.some(([a, b]) => p.a >= a - 0.2 && p.b <= b + 0.6)) return null;
  // inside the picture, 44 px above the letterbox, over the band (Film) and in the letters' own shadow; no plate
  return (
    <div style={{ position: "absolute", left: 160, width: 1600, top: BAR, height: 1080 - 2 * BAR - 44, display: "flex", alignItems: "flex-end", justifyContent: "center" }}>
      <div style={{ textAlign: "center", fontFamily: "Inter", fontWeight: 500, fontSize: 36, lineHeight: 1.3, letterSpacing: "0.005em", color: ivory, textShadow: "0 0 14px rgba(0,0,0,0.95), 0 0 4px rgba(0,0,0,0.9), 0 2px 2px rgba(0,0,0,0.8)" }}>{p.text}</div>
    </div>
  );
}

// chapter marks, top left, as each part begins: the part in Latin over its name in italic
const CHAPTERS: { line: string; n: string; name: string }[] = [
  { line: "dictator", n: "PARS PRIMA", name: "The Dictator" },
  { line: "plot", n: "PARS SECVNDA", name: "The Conspiracy" },
  { line: "omens", n: "PARS TERTIA", name: "The Omens" },
  { line: "curia", n: "PARS QVARTA", name: "The Ides" },
  { line: "funeral", n: "PARS QVINTA", name: "Aftermath" },
];

function Chapter({ t }: { t: number }) {
  const c = [...CHAPTERS].reverse().find((c) => t >= TL.at[c.line] - 0.6);
  if (!c) return null;
  const at = TL.at[c.line] - 0.6;
  const u = shown(t, at, at + 5.6, 1.2, 0.8);
  const w = Math.round(Math.max(italW(c.name, 72), capsW(c.n, 23, 0.42))) + 12;
  return (
    <Reveal u={u} w={w} h={120} left={120} top={BAR + 56}>
      <Caps size={23} track={0.42} color={gold}><Letters text={c.n} t={t} at={at + 0.2} step={0.025} /></Caps>
      <Rule u={u} w={56} />
      <Italic size={72} weight={300} color={ivory}>{c.name}</Italic>
    </Reveal>
  );
}

// ---------------------------------------------------------------- the frame

function Film() {
  const f = useCurrentFrame();
  const t = f / FPS;
  const s = sceneAt(t);
  const cam = s.cam(t);
  // through black at each scene's edges where the scene asks for it
  const sky = SKIES[s.name] ?? INDOORS;
  const dip = Math.max(0, ...SCENES.map((x) => (x.dip ? Math.max(1 - Math.abs(t - x.from) / x.dip, 0) : 0)));
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <style>{FONTS}</style>
      <Mix lufs={-14} ceiling={-1.5} />
      <Sound />
      <Canvas3D>
        <PerspectiveCamera position={cam.pos} lookAt={cam.look} fov={cam.fov ?? 35} focus={cam.focus} />
        <ToneMapping mode="agx" exposure={1.0} />
        <Bloom intensity={0.22} threshold={0.92} />
        <Fog color="#0c0907" near={22} far={110} />
        <DepthOfField aperture={2.2} maxBlur={26} />
        {/* light in air: the scenes' FogVolumes and their volumetric lights (32 steps, dithered, marched at half
            resolution and upsampled by depth: the shafts at SSIM 0.998+ against 128 steps at full resolution;
            docs/throughput.md) */}
        <VolumetricFog steps={32} dither resolution={0.5} />
        {/* contact shadow in every crease of the stone, clean edges, and the sun's shadows fitted to a street's depth */}
        <AmbientOcclusion quality="high" thickness={0.25} />
        <AntiAliasing mode="smaa" />
        <Shadows mapSize={4096} distance={90} firstCascade={10} cascades={4} overlap={0.3} />
        {/* each scene's sky (Poly Haven, CC0): seen outdoors, and everywhere a faint fill of its light in the stone */}
        <Environment src={staticFile(`sky/${sky.id}.hdr`)} intensity={sky.light} rotation={sky.turn ?? 0} background={sky.seen > 0} backgroundIntensity={sky.seen} />
        {/* the film's one grade: warm, a little less colour in the highlights, blacks lifted a touch, a lens vignette */}
        <Grade temperature={0.035} saturation={0.92} shadows={{ lift: 0.012, saturation: 0.85 }} midtones={{ contrast: 1.06 }} highlights={{ saturation: 0.8, gain: 0.97 }} vignette={{ intensity: 0.55, radius: 0.85, smoothness: 3 }} aberration={0.0025} />
        {s.draw(t)}
      </Canvas3D>
      {/* the film's grain, eight frames of it in turn; under the type */}
      <Img src={staticFile(`grain/${f % 8}.png`)} style={{ position: "absolute", left: 0, top: 0, width: 1920, height: 1080, opacity: 0.07 }} />
      {/* a soft dark band at the foot of the picture, where the captions sit */}
      <div style={{ position: "absolute", left: 0, top: 1080 - BAR - 260, width: 1920, height: 260, background: "linear-gradient(180deg, rgba(0,0,0,0), rgba(0,0,0,0.42) 70%, rgba(0,0,0,0.55))" }} />
      {/* the page (type, and full-frame pictures like the sand drawings), then the letterbox over it, then the dips */}
      {s.page ? s.page(t) : null}
      <div style={{ position: "absolute", left: 0, top: 0, width: 1920, height: BAR, background: "#000000" }} />
      <div style={{ position: "absolute", left: 0, top: 1080 - BAR, width: 1920, height: BAR, background: "#000000" }} />
      <div style={{ position: "absolute", inset: 0, background: "#000000", opacity: dip }} />
      <Chapter t={t} />
      <Captions t={t} />
    </AbsoluteFill>
  );
}

// ---------------------------------------------------------------- the sound

/** The music's level (dB) at film time t: low under the voice, up in the gaps and at the climax, down for the end. */
function musicDb(t: number) {
  const speaking = LINES.some((l) => t >= l.at - 0.3 && t < l.at + l.seconds + 0.2);
  const near = Math.min(...LINES.map((l) => Math.max(l.at - 0.3 - t, t - (l.at + l.seconds + 0.2), 0)));
  const open = clamp01(near / 0.8); // how far into a gap, eased over 0.8 s
  const base = speaking ? -21 : -21 + 8 * open;
  const climax = smooth((t - TL.hit + 0.4) / 0.4) * (1 - smooth((t - TL.hit - 6) / 4)) * 5;
  return base + climax;
}

type Cue = { at: number; src: ReturnType<typeof sao> | string; db: number; seconds?: number; name: string };

// foley: physical moments only (SAO, local)
const fx = (prompt: string, seconds: number) => sao(prompt, { seconds });
const W = (line: string, word: string, k = 0) => TL.word(line, word, k);
/** the sky over each scene, every one at night (stars: tools/stars.py; the rest Poly Haven): `seen` is how bright it's drawn behind the set (0: not at all; kept low, so the stone stays
 * the brightest thing in the picture), `light` is how much of its light reaches the stone */
type Sky = { id: string; light: number; seen: number; turn?: number };
const INDOORS: Sky = { id: "qwantani_night_puresky", light: 0.1, seen: 0 };
const SKIES: Record<string, Sky> = {
  dawn: { id: "stars", light: 0.3, seen: 1 },
  floor: { id: "stars", light: 0.3, seen: 1 },
  door: { id: "stars", light: 0.3, seen: 1 },
  street: { id: "stars", light: 0.3, seen: 1 },
  omens: { id: "stars", light: 0.3, seen: 0.6 },
  dream: { id: "qwantani_night_puresky", light: 0.08, seen: 0 },
  funeral: { id: "stars", light: 0.3, seen: 1 },
  augustus: { id: "stars", light: 0.3, seen: 1 },
  sunset: { id: "stars", light: 0.3, seen: 1.2 },
  title: { id: "qwantani_night_puresky", light: 0, seen: 0 },
  end: { id: "qwantani_night_puresky", light: 0, seen: 0 },
};

const FOLEY: Cue[] = [
  { name: "wind-open", at: 0.2, src: fx("cold night wind through ancient stone ruins, low and steady, no voices, no music", 10), db: -24 },
  { name: "boom-title", at: 2.0, src: fx("deep cinematic sub boom impact, dark, short tail, no music", 3), db: -14 },
  { name: "crowd-forum", at: TL.at.dictator, src: fx("a large ancient crowd murmuring in a stone forum, distant, many voices, no words, no music", 10), db: -30 },
  { name: "candles", at: TL.at.plot, src: fx("a quiet stone room at night, candles flickering, faint whispers, no music", 10), db: -28 },
  // every bolt of the omens' lightning has its strike on the frame it flashes: a near crack for a bright one, a farther
  // one for a dim one, the rumble after; then far thunder under Calpurnia's dream, with no flash
  ...LIGHTNING.map((b, i) => ({
    name: `lightning-${i}`,
    at: b.at - 0.03,
    src: b.k >= 0.8 ? fx("a lightning strike close by at night: a sharp electric crack then rolling thunder, no rain, no music", 6) : fx("a lightning strike a mile off at night: a dull crack then a long low rumble, no rain, no music", 6),
    db: -22 + 8 * b.k,
  })),
  { name: "rain", at: TL.at.omens - 0.4, seconds: W("omens", "Calpurnia") + 6 - TL.at.omens, src: fx("steady heavy rain falling on stone pavement at night, close, no thunder, no music", 10), db: -24 },
  { name: "thunder-far", at: W("omens", "Calpurnia"), src: fx("distant rolling thunder at night, low and long, no crack, no rain, no music", 6), db: -22 },
  { name: "door", at: W("decimus", "house"), src: fx("a heavy wooden door creaking open in a stone hall, close, no music", 3), db: -20 },
  { name: "steps", at: TL.at.street, src: fx("footsteps of leather sandals on stone pavement, a few people walking, outdoors, no music", 8), db: -26 },
  { name: "scroll", at: W("street", "scroll"), src: fx("an old papyrus scroll unrolling, dry crinkle, close, no music", 2), db: -18 },
  { name: "hall", at: TL.at.curia, src: fx("the hush of a large stone senate hall, low murmur echoing, no music", 10), db: -28 },
  { name: "cloth", at: W("strike", "seized"), src: fx("a heavy wool toga grabbed and pulled hard, cloth tearing, close, dry, no music", 1.5), db: -14 },
  { name: "draw", at: W("strike", "Casca") - 0.35, src: fx("a steel dagger drawn fast from a leather sheath, close, dry, no music", 1.2), db: -12 },
  // a thud on every blow the stabs scene shows (a dagger's shadow every 0.55 s from Casca's, shots.tsx Dagger), two takes in turn
  ...Array.from({ length: Math.ceil((W("strike", "saw") - 0.1 - W("strike", "Casca") - 0.4) / 0.55) + 1 }, (_, k) => ({
    name: `stab-${k}`,
    at: W("strike", "Casca") + k * 0.55 + 0.1,
    src: k % 2 ? fx("a dull heavy thud of a blade through thick cloth, close, dry, no scream, no music", 1) : fx("a short wet stab of a dagger through wool into flesh, close, dry, no scream, no music", 1),
    db: k === 0 ? -11 : -15,
  })),
  { name: "scuffle", at: W("strike", "came."), src: fx("a struggle: scuffling sandals on marble, cloth pulling, heavy breathing, close, no screams, no music", 3), db: -20 },
  { name: "fall", at: W("strike", "fell."), src: fx("a heavy body collapsing onto a marble floor, cloth rustle, close, no music", 2), db: -12 },
  { name: "shatter", at: W("strike", "fell.") + 0.25, src: fx("a marble bust shattering on a stone floor, crisp stone fragments scattering, close, no music", 3), db: -13 },
  { name: "drips", at: TL.at.wounds + 1, src: fx("slow thick drops of liquid dripping onto a stone floor in a quiet hall, close, sparse, no music", 10), db: -30 },
  { name: "chisel", at: TL.at.wounds, src: fx("a single chisel strike on marble, sharp, dry, no music", 0.6), db: -18 },
  { name: "crowd-roar", at: W("funeral", "liberty."), src: fx("an angry ancient crowd shouting in a stone forum, distant, no clear words, no music", 6), db: -22 },
  { name: "fire", at: W("funeral", "burned"), src: fx("a large crackling bonfire roaring, close, no music", 10), db: -18 },
  { name: "coin", at: W("coin", "coin."), src: fx("a single silver coin dropped onto a stone table, bright ring, close, no music", 2), db: -14 },
  { name: "wind-close", at: TL.at.close, src: fx("cold evening wind through ancient stone ruins, low and steady, no voices, no music", 10), db: -26 },
];

function Sound() {
  const scoreFrom = Math.round(TL.scoreAt * FPS);
  return (
    <>
      {/* the beds first: the drone, the score, the drone again for the close */}
      <Sequence name="drone-open" from={0} durationInFrames={Math.round((TL.scoreAt + 8) * FPS)} layout="none">
        <Audio src={staticFile("music/drone.mp3")} gainDb={(fr: number) => musicDb(fr / FPS) - 2} fadeIn={2} fadeOut={6} />
      </Sequence>
      <Sequence name="score" from={scoreFrom} layout="none">
        <Audio src={staticFile("music/score-a.mp3")} gainDb={(fr: number) => musicDb(TL.scoreAt + fr / FPS)} fadeIn={3} />
      </Sequence>
      <Sequence name="drone-close" from={Math.round((TL.scoreAt + 172) * FPS)} layout="none">
        <Audio src={staticFile("music/drone.mp3")} startFrom={60 * FPS} gainDb={(fr: number) => musicDb(TL.scoreAt + 172 + fr / FPS) + 2} fadeIn={5} fadeOut={3} />
      </Sequence>
      {LINES.map((l) => (
        <Sequence key={l.id} name={`voice-${l.id}`} from={Math.round(l.at * FPS)} layout="none">
          <Audio src={staticFile(`voice/${l.id}.mp3`)} gainDb={6} />
        </Sequence>
      ))}
      {FOLEY.map((c) => (
        // a cue with `seconds` is a bed: its clip loops that long and fades out
        <Sequence key={c.name} name={c.name} from={Math.round(c.at * FPS)} durationInFrames={c.seconds ? Math.round(c.seconds * FPS) : undefined} layout="none">
          <Audio src={c.src} gainDb={c.db} loop={!!c.seconds} fadeIn={c.seconds ? 1 : 0} fadeOut={c.seconds ? 1.5 : 0.3} />
        </Sequence>
      ))}
    </>
  );
}

registerRoot(() => (
  <>
    <Composition id="ides" component={Film} width={1920} height={1080} fps={FPS} durationInFrames={FRAMES} />
  </>
));
