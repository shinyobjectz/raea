// ARENA: a small 3D arena, the games gate's 3D fixture. Steer the cube with the arrow keys, take the coins, keep off
// the sentries. The camera follows the player. The game is a pure fold: live arrow keys when someone plays, a scripted
// run (an autopilot towards the nearest coin) when nobody does, so a render of a play's logged input is the same game.
//
// What it exercises in live 3D (sdk/react/src/motion/scene3d.rs LiveScene): a mesh moved, turned and squashed by keys, a
// colour that changes (hurt), a mesh that blinks out and back (the respawn blink), coins taken (despawned) and new ones
// placed with fresh names (spawned), sentries on state machines (patrol, look round, chase, return), a point light
// following the player, and a chase camera.
import { AbsoluteFill, Audio, Composition, Sequence, machine, registerRoot, sao, stepAll, useCurrentFrame, useGameEvents, useGameState, type FrameInput, type Playtest, type Snapshot } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, Cylinder, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, PointLight, Sphere, Torus } from "@raea/motion/3d";

export const CW = 1280;
export const CH = 720;
const FPS = 120;
const FRAMES = FPS * 60;
/** the arena's half width, in world units */
const R = 6;
const SPEED = 0.06;
const COINS = 5;
const HURT = 90;
const BLINK = 120;

// a seeded number in [0, 1) from n (the same arena every time)
const seed = (n: number) => ((((n * 9301 + 49297) % 233280) + 233280) % 233280) / 233280;

interface Player { x: number; z: number; heading: number; lives: number; sm: Snapshot }
interface Sentry { id: number; x: number; z: number; hx: number; hz: number; dir: number; sm: Snapshot }
interface Coin { id: number; x: number; z: number }
interface Game { player: Player; sentries: Sentry[]; coins: Coin[]; next: number; score: number; taken: boolean; hit: boolean; flow: Snapshot }

interface PlayerEnv { dx: number; dz: number; hit: boolean }
export const player = machine<Player, PlayerEnv>()({
  id: "player",
  initial: "idle",
  states: {
    idle: { always: [{ to: "hurt", when: (_p, e) => e.hit }, { to: "move", when: (_p, e) => e.dx !== 0 || e.dz !== 0 }] },
    move: {
      tick: (p, e) => ({ ...p, x: clamp(p.x + e.dx * SPEED, R - 0.5), z: clamp(p.z + e.dz * SPEED, R - 0.5), heading: e.dx || e.dz ? Math.atan2(e.dx, e.dz) : p.heading }),
      always: [{ to: "hurt", when: (_p, e) => e.hit }, { to: "idle", when: (_p, e) => e.dx === 0 && e.dz === 0 }],
    },
    hurt: { entry: (p) => ({ ...p, lives: p.lives - 1 }), after: { [HURT]: "respawn" } },
    respawn: { entry: (p) => ({ ...p, x: 0, z: 2.5 }), after: { [BLINK]: "idle" } },
  },
});

interface SentryEnv { px: number; pz: number }
export const sentry = machine<Sentry, SentryEnv>()({
  id: "sentry",
  initial: "patrol",
  states: {
    patrol: {
      tick: (s) => ({ ...s, x: s.x + s.dir * 0.025 }),
      always: [{ to: "chase", when: (s, e) => dist(s.x, s.z, e.px, e.pz) < 1.6 }, { to: "look", when: (s) => Math.abs(s.x - s.hx) > 1.8 }],
    },
    look: { entry: (s) => ({ ...s, dir: -s.dir, x: s.hx + Math.sign(s.x - s.hx) * 1.8 }), after: { 60: "patrol" } },
    chase: {
      tick: (s, e) => {
        const d = Math.max(dist(s.x, s.z, e.px, e.pz), 1e-6);
        return { ...s, x: s.x + ((e.px - s.x) / d) * 0.02, z: s.z + ((e.pz - s.z) / d) * 0.02 };
      },
      always: { to: "home", when: (s, e) => dist(s.x, s.z, e.px, e.pz) > 2.6 },
    },
    home: {
      tick: (s) => {
        const d = Math.max(dist(s.x, s.z, s.hx, s.hz), 1e-6);
        return d < 0.04 ? { ...s, x: s.hx, z: s.hz } : { ...s, x: s.x + ((s.hx - s.x) / d) * 0.03, z: s.z + ((s.hz - s.z) / d) * 0.03 };
      },
      always: [{ to: "chase", when: (s, e) => dist(s.x, s.z, e.px, e.pz) < 1.6 }, { to: "patrol", when: (s) => s.x === s.hx && s.z === s.hz }],
    },
  },
});

export const flow = machine<Game, null>()({
  id: "flow",
  key: "flow",
  initial: "playing",
  states: {
    playing: { always: { to: "over", when: (g) => g.player.lives <= 0 } },
    over: { after: { 360: "playing" }, exit: (g) => ({ ...g, player: { ...g.player, lives: 3 }, score: 0 }) },
  },
});

function clamp(v: number, m: number) {
  return Math.max(-m, Math.min(m, v));
}
function dist(ax: number, az: number, bx: number, bz: number) {
  return Math.hypot(ax - bx, az - bz);
}
function coinAt(id: number): Coin {
  // near where the player starts, so a few seconds of anyone's play takes some
  return { id, x: -2.5 + seed(id * 3 + 1) * 5, z: 0.5 + seed(id * 7 + 5) * 4 };
}

const INITIAL: Game = {
  player: { x: 0, z: 2.5, heading: 0, lives: 3, sm: player.start() },
  sentries: [0, 1, 2, 3].map((i) => {
    const hx = [-4, 4, -4, 4][i];
    const hz = [-4, -4, 4.5, 4.5][i];
    return { id: i, x: hx, z: hz, hx, hz, dir: i % 2 ? 1 : -1, sm: sentry.start() };
  }),
  coins: [0, 1, 2, 3, 4].map(coinAt),
  next: COINS,
  score: 0,
  taken: false,
  hit: false,
  flow: flow.start(),
};

/** The scripted run a render plays when nobody presses keys: towards the nearest coin, an axis at a time. */
function scripted(g: Game): string[] {
  const p = g.player;
  let best: Coin | null = null;
  for (const c of g.coins) if (!best || dist(p.x, p.z, c.x, c.z) < dist(p.x, p.z, best.x, best.z)) best = c;
  if (!best) return [];
  if (Math.abs(best.x - p.x) > 0.1) return [best.x > p.x ? "ArrowRight" : "ArrowLeft"];
  if (Math.abs(best.z - p.z) > 0.1) return [best.z > p.z ? "ArrowDown" : "ArrowUp"];
  return [];
}

function step(g: Game, input: FrameInput): Game {
  // a person playing (live input) is never driven by the autopilot, keys or none
  const keys = input.keys.length || (input as { live?: boolean }).live ? input.keys : scripted(g);
  const over = flow.is(g, "over");
  const dx = over ? 0 : (keys.includes("ArrowRight") ? 1 : 0) - (keys.includes("ArrowLeft") ? 1 : 0);
  const dz = over ? 0 : (keys.includes("ArrowDown") ? 1 : 0) - (keys.includes("ArrowUp") ? 1 : 0);
  const p0 = g.player;
  const safe = player.is(p0, "hurt") || player.is(p0, "respawn") || over;
  const hit = !safe && g.sentries.some((s) => dist(s.x, s.z, p0.x, p0.z) < 0.85);
  const p = player.step(p0, { dx, dz, hit });
  const sentries = stepAll(sentry, g.sentries, { px: p.x, pz: p.z });
  let coins = g.coins;
  let next = g.next;
  let score = g.score;
  const got = player.is(p, "hurt") ? undefined : coins.find((c) => dist(c.x, c.z, p.x, p.z) < 0.8);
  if (got) {
    coins = [...coins.filter((c) => c !== got), coinAt(next)];
    next += 1;
    score += 1;
  }
  return flow.step({ ...g, player: p, sentries, coins, next, score, taken: Boolean(got), hit }, null);
}

export const playtest: Playtest<Game> = {
  initial: INITIAL,
  step,
  actions: [["ArrowUp"], ["ArrowDown"], ["ArrowLeft"], ["ArrowRight"], []],
  progress: (g) => Math.min(1, g.score / 20),
  outcome: (g) => (g.score >= 20 ? "won" : flow.is(g, "over") ? "lost" : null),
  events: { coin: (_b, a) => a.taken, hit: (_b, a) => a.hit },
  machines: (g) => ({ player: player.watch(g.player), flow: flow.watch(g), ...Object.fromEntries(g.sentries.map((s) => ["sentry-" + s.id, sentry.watch(s)])) }),
};

const POCK = sao("a single crisp ping-pong paddle strike on a celluloid ball, hollow wooden body, dry and close, very short", { seconds: 0.5 });
const BELL = sao("one warm brass gym bell ding with a short tail, a match ending, dry", { seconds: 1.6 });

function Arena() {
  const frame = useCurrentFrame();
  const g = useGameState(INITIAL, step);
  const coinsAt = useGameEvents(INITIAL, step, (_b, a) => a.taken);
  const hitsAt = useGameEvents(INITIAL, step, (_b, a) => a.hit);
  const p = g.player;
  const state = player.state(p);
  const hurt = state === "hurt";
  const blink = state === "respawn" && Math.floor(player.steps(p) / 10) % 2 === 1;
  const moving = state === "move";
  const t = (g.next * 37 + g.score) % 360;
  return (
    <AbsoluteFill style={{ background: "#0b1020" }}>
      <Canvas3D>
        <PerspectiveCamera position={[p.x * 0.6, 7.5, p.z * 0.6 + 8.5]} lookAt={[p.x * 0.8, 0, p.z * 0.8]} fov={55} />
        <AmbientLight intensity={0.3} />
        <DirectionalLight position={[4, 10, 6]} intensity={1.1} />
        <PointLight position={[p.x, 1.6, p.z]} intensity={hurt ? 0.9 : 0.35} color={hurt ? "#ff5a5a" : "#9cc4ff"} distance={6} />
        <Mesh name="floor" position={[0, -0.05, 0]}>
          <Box args={[2 * R + 1, 0.1, 2 * R + 1]} />
          <MeshStandardMaterial color="#1b2540" roughness={0.95} />
        </Mesh>
        {[0, 1, 2, 3].map((i) => (
          <Mesh key={"wall" + i} name={"wall-" + i} position={[i < 2 ? (i ? R + 0.5 : -R - 0.5) : 0, 0.3, i < 2 ? 0 : i === 3 ? R + 0.5 : -R - 0.5]}>
            <Box args={i < 2 ? [0.3, 0.6, 2 * R + 1.3] : [2 * R + 1.3, 0.6, 0.3]} />
            <MeshStandardMaterial color="#33415f" roughness={0.8} />
          </Mesh>
        ))}
        {!blink && (
          <Mesh name="player" position={[p.x, 0.4, p.z]} rotation={[0, p.heading, 0]} scale={moving ? [0.9, 1.1, 0.9] : 1}>
            <Box args={[0.7, 0.7, 0.7]} />
            <MeshStandardMaterial color={hurt ? "#ef4444" : "#f0ebd8"} roughness={0.35} />
          </Mesh>
        )}
        {g.sentries.map((s) => (
          <Mesh key={s.id} name={"sentry-" + s.id} position={[s.x, 0.45, s.z]}>
            <Sphere args={[0.45]} />
            <MeshStandardMaterial color={sentry.is(s, "chase") ? "#f97316" : sentry.is(s, "look") ? "#facc15" : "#748cab"} roughness={0.6} />
          </Mesh>
        ))}
        {g.coins.map((c) => (
          <Mesh key={c.id} name={"coin-" + c.id} position={[c.x, 0.5, c.z]} rotation={[Math.PI / 2, (t + c.id * 40) * (Math.PI / 180), 0]}>
            <Torus args={[0.25, 0.08]} />
            <MeshStandardMaterial color="#fbbf24" metalness={0.6} roughness={0.3} emissive="#7c4a00" />
          </Mesh>
        ))}
        <Mesh name="pillar" position={[0, 0.6, 0]} rotation={[0, (g.score * 15) * (Math.PI / 180), 0]}>
          <Cylinder args={[0.2, 0.2, 1.2]} />
          <MeshStandardMaterial color="#5b6b94" roughness={0.5} />
        </Mesh>
      </Canvas3D>
      <div style={{ position: "absolute", left: 32, top: 24, fontFamily: "Inter", fontWeight: 800, fontSize: 36, color: "#f0ebd8" }}>{`COINS ${g.score}`}</div>
      <div style={{ position: "absolute", right: 32, top: 24, fontFamily: "Inter", fontWeight: 800, fontSize: 36, color: "#f0ebd8" }}>{"♥".repeat(Math.max(0, p.lives))}</div>
      {flow.is(g, "over") && (
        <div style={{ position: "absolute", left: 0, right: 0, top: 300, textAlign: "center", fontFamily: "Inter", fontWeight: 800, fontSize: 72, color: "#ef4444" }}>GAME OVER</div>
      )}
      {/* only the sounds still playing: one Sequence per event ever would grow the tree for as long as anyone plays */}
      {coinsAt.filter((f) => frame - f < 60).map((f) => (
        <Sequence key={"c" + f} from={f} durationInFrames={60}>
          <Audio src={POCK} gainDb={-6} fadeIn={0.01} fadeOut={0.08} />
        </Sequence>
      ))}
      {hitsAt.filter((f) => frame - f < 190).map((f) => (
        <Sequence key={"h" + f} from={f} durationInFrames={190}>
          <Audio src={BELL} gainDb={-10} fadeIn={0.01} fadeOut={0.3} />
        </Sequence>
      ))}
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="edit" component={Arena} width={CW} height={CH} fps={120} durationInFrames={FRAMES} />);
