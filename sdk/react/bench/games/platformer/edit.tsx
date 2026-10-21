// LEDGE: a side-scrolling platformer, the gate's second game (pong is the first). Run and jump across a long level of
// ledges, take the coins, keep off the walkers, reach the far right end. The game is a pure fold: live arrow keys when
// someone plays, a scripted run (a simple autopilot) when nobody does. The level is drawn in its own units and scaled to
// the stage.
//
// Its logic is state machines (sdk/motion/src/machine.ts, sdk/react/docs/state-machines.md), each stepped once a frame inside
// the fold: the game's flow (playing, won, game over), the player (idle, run, jump, fall, land, hurt, respawn, dead) and
// one per walker (patrolling right and left, looking round at each end). Each machine's snapshot is plain data inside
// the state, so a render, a play and a playtest bot all see the same states on the same frames.
//
// The level is the repaired one (sdk/react/bench/playtest/fixtures/ledge-fixed): the jump rises above the raised ledges and
// a ledge spans every gap, so the playtest bots can finish it (the first LEDGE stopped every bot at its first gap).
import {
  Composition, Overlay, machine, registerRoot, stepAll, useCurrentFrame, useGameEvents, useGameState, useInput, useVideoConfig,
  type Snapshot,
} from "@raea/motion";
import { AbsoluteFill } from "@raea/motion";
import type { Playtest } from "@raea/motion";

// ---- the level ------------------------------------------------------------------------------------------
export const VW = 1920;
export const VH = 1080;
export const LEVEL_W = 12000;
const FLOOR = 980;

type Box = { x: number; y: number; w: number; h: number };

// ledges: a seeded run of platforms over gaps and steps (the same level every time)
const seed = (n: number) => ((((n * 9301 + 49297) % 233280) + 233280) % 233280) / 233280;
export const LEDGES: Box[] = (() => {
  const out: Box[] = [];
  let x = 0;
  let k = 0;
  while (x < LEVEL_W) {
    const ground = k % 5 !== 3;
    const w = 420 + Math.floor(seed(k) * 520);
    if (ground) out.push({ x, y: FLOOR, w, h: VH - FLOOR + 40 });
    // a raised ledge over every other stretch, and over every gap (the gap is crossed by way of it)
    if (k % 2 === 1 || !ground) out.push({ x: x + 80, y: FLOOR - 220 - Math.floor(seed(k + 7) * 180), w: Math.max(200, w - 220), h: 36 });
    x += w + (k % 5 === 3 ? 260 : 0);
    k++;
  }
  return out;
})();
const FLOORS = LEDGES.filter((b) => b.y === FLOOR);

export const COINS: { x: number; y: number }[] = LEDGES.filter((b) => b.h < 100).flatMap((b, i) =>
  [0, 1, 2].map((j) => ({ x: b.x + 40 + j * Math.max(50, (b.w - 80) / 3), y: b.y - 70 - (i % 2) * 20 })),
);

// ---- tuning -----------------------------------------------------------------------------------------------
const PW = 56;
const PH = 84;
const RUN = 9;
const AIR = 0.6;
/** rises 28^2 / (2 * 0.95) = 412 px, above the highest raised ledge (400 px up); -21 rose 232 px, under every one */
const JUMP = -28;
const GRAV = 0.95;
/** frames: the landing squash, the hurt pause, the respawn blink, the safe time after it, the end screens */
const LAND = 6;
const HURT = 40;
const RESPAWN = 30;
const SAFE = 120;
const WON_HOLD = 600;
const OVER_HOLD = 360;
const GOAL_X = LEVEL_W - PW - 1;
const WALKER = 70;

const overlap = (a: Box, b: Box) => a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;

// ---- a walker: patrols its stretch, and looks round at each end -------------------------------------------------
export interface Walker { x: number; x0: number; span: number; speed: number; sm: Snapshot }
const walker = machine<Walker, null>()({
  id: "walker",
  initial: "right",
  states: {
    right: { tick: (w) => ({ ...w, x: Math.min(w.x0 + w.span, w.x + w.speed) }), always: { to: "lookLeft", when: (w) => w.x >= w.x0 + w.span } },
    lookLeft: { after: { 24: "left" } },
    left: { tick: (w) => ({ ...w, x: Math.max(w.x0, w.x - w.speed) }), always: { to: "lookRight", when: (w) => w.x <= w.x0 } },
    lookRight: { after: { 24: "right" } },
  },
});
const freshWalkers = (): Walker[] => LEDGES.filter((b, i) => b.y === FLOOR && i % 3 === 2 && b.w > 520).map((b, i) => ({
  x: b.x + 60, x0: b.x + 60, span: b.w - 180, speed: 2.2 + (i % 3) * 0.8, sm: walker.start(),
}));

// ---- the player ---------------------------------------------------------------------------------------------
export interface Player {
  x: number;
  y: number;
  vx: number;
  vy: number;
  /** standing on something after this frame's move */
  ground: boolean;
  /** touched a walker (while not safe) or fell out of the level this frame */
  hit: boolean;
  /** frames left of the safe time after a respawn */
  safe: number;
  lives: number;
  /** where a respawn puts the player: near the start of the last floor stretch it stood on */
  checkpoint: number;
  sm: Snapshot;
}
interface PlayerEnv { left: boolean; right: boolean; jump: boolean; walkers: readonly Walker[] }

/** One frame of movement: run or steer in the air, gravity, landing on ledges and bumping their undersides. */
function physics(p: Player, e: PlayerEnv): Player {
  const want = (e.right ? RUN : 0) - (e.left ? RUN : 0);
  const vx = p.ground ? want : p.vx + (want - p.vx) * AIR * 0.2;
  let vy = p.vy;
  const x = Math.max(0, Math.min(LEVEL_W - PW, p.x + vx));
  let y = p.y + vy;
  let ground = false;
  let checkpoint = p.checkpoint;
  for (const b of LEDGES) {
    if (x + PW <= b.x || x >= b.x + b.w) continue;
    if (p.y + PH <= b.y && y + PH >= b.y && vy >= 0) {
      y = b.y - PH;
      vy = 0;
      ground = true;
      if (b.y === FLOOR) checkpoint = Math.max(120, b.x + 40);
    } else if (b.h < 100 && p.y >= b.y + b.h && y < b.y + b.h && vy < 0) {
      y = b.y + b.h;
      vy = 0;
    }
  }
  if (!ground) vy += GRAV;
  const safe = Math.max(0, p.safe - 1);
  const me = { x, y, w: PW, h: PH };
  let touched = false;
  if (safe === 0) for (const w of e.walkers) if (overlap(me, { x: w.x, y: FLOOR - WALKER, w: WALKER, h: WALKER })) touched = true;
  return { ...p, x, y, vx, vy, ground, checkpoint, safe, hit: touched || y > VH + 200 };
}

const moving = (_p: Player, e: PlayerEnv) => e.left !== e.right;
export const player = machine<Player, PlayerEnv>()({
  id: "player",
  initial: "alive",
  states: {
    alive: {
      initial: "ground",
      tick: physics,
      always: { to: "hurt", when: (p) => p.hit },
      states: {
        ground: {
          initial: "idle",
          always: [{ to: "jump", when: (_p, e) => e.jump }, { to: "fall", when: (p) => !p.ground }],
          states: {
            idle: { always: { to: "run", when: moving } },
            run: { always: { to: "idle", when: (p, e) => !moving(p, e) } },
            land: { always: [{ to: "run", after: LAND, when: moving }, { to: "idle", after: LAND }] },
          },
        },
        air: {
          initial: "fall",
          always: { to: "land", when: (p) => p.ground },
          states: {
            jump: { entry: (p) => ({ ...p, vy: JUMP, ground: false }), always: { to: "fall", when: (p) => p.vy >= 0 } },
            fall: {},
          },
        },
      },
    },
    hurt: {
      entry: (p) => ({ ...p, lives: p.lives - 1, vx: 0, vy: 0, hit: false }),
      always: [{ to: "dead", after: HURT, when: (p) => p.lives <= 0 }, { to: "respawn", after: HURT }],
    },
    respawn: {
      entry: (p) => ({ ...p, x: p.checkpoint, y: FLOOR - PH, vx: 0, vy: 0, ground: true, safe: SAFE }),
      after: { [RESPAWN]: "alive" },
    },
    dead: { type: "final" },
  },
});
const freshPlayer = (): Player => ({ x: 120, y: FLOOR - PH, vx: 0, vy: 0, ground: true, hit: false, safe: 0, lives: 3, checkpoint: 120, sm: player.start() });

// ---- the game: its flow, and everything it steps ---------------------------------------------------------------
export interface Game {
  player: Player;
  walkers: Walker[];
  /** one flag per coin, as a string of 0/1 (a pure value) */
  got: string;
  score: number;
  human: boolean;
  /** the flow machine's snapshot */
  sm: Snapshot;
}
interface FlowEnv { keys: readonly string[] }

/** A frame of play: the walkers, then the player against them, then the coins the player touches. */
function play(g: Game, e: FlowEnv): Game {
  const walkers = stepAll(walker, g.walkers, null);
  const k = e.keys;
  const p = player.step(g.player, {
    left: k.indexOf("ArrowLeft") >= 0, right: k.indexOf("ArrowRight") >= 0, jump: k.indexOf("ArrowUp") >= 0 || k.indexOf(" ") >= 0, walkers,
  });
  let got = g.got;
  let score = g.score;
  if (player.is(p, "alive")) {
    const me = { x: p.x, y: p.y, w: PW, h: PH };
    for (let i = 0; i < COINS.length; i++) {
      if (got[i] === "1") continue;
      const c = COINS[i];
      if (overlap(me, { x: c.x - 18, y: c.y - 18, w: 36, h: 36 })) {
        got = got.slice(0, i) + "1" + got.slice(i + 1);
        score += 10;
      }
    }
  }
  return { ...g, walkers, player: p, got, score };
}

export const flow = machine<Game, FlowEnv>()({
  id: "flow",
  initial: "playing",
  states: {
    playing: {
      // a new run: the level as it starts (the first step's entry gives exactly INITIAL's level)
      entry: (g) => ({ ...g, player: freshPlayer(), walkers: freshWalkers(), got: "0".repeat(COINS.length), score: 0 }),
      tick: play,
      always: [{ to: "won", when: (g) => g.player.x >= GOAL_X }, { to: "gameOver", when: (g) => player.is(g.player, "dead") }],
    },
    won: { after: { [WON_HOLD]: "playing" } },
    gameOver: { after: { [OVER_HOLD]: "playing" } },
  },
});

export const INITIAL: Game = { player: freshPlayer(), walkers: freshWalkers(), got: "0".repeat(COINS.length), score: 0, human: false, sm: flow.start() };

// the scripted run when nobody plays: right, and a jump before every gap, ledge and walker
const EMPTY: string[] = [];
function autopilot(s: Game, frame: number): string[] {
  const p = s.player;
  const ahead = p.x + PW + 70;
  const gap = !FLOORS.some((b) => ahead >= b.x && ahead <= b.x + b.w);
  const near = s.walkers.some((w) => {
    const d = w.x - p.x;
    return d > 0 && d < 190;
  });
  const ledge = LEDGES.some((b) => b.h < 100 && b.x - p.x > 40 && b.x - p.x < 160);
  return gap || near || ledge || frame % 97 === 0 ? ["ArrowRight", "ArrowUp"] : ["ArrowRight"];
}

const fold = (s: Game, i: { keys?: string[] }, f: number): Game => {
  const live = i && i.keys && i.keys.length ? i.keys : null;
  if (live && !s.human) return { ...INITIAL, human: true };
  return flow.step(s, { keys: live ? live : s.human ? EMPTY : autopilot(s, f) });
};

// ---- the playtest (sdk/react/docs/playtest.md): what a bot steps, sees and aims for -------------------------------------
export const playtest: Playtest<Game> = {
  initial: INITIAL,
  step: fold,
  actions: [["ArrowRight"], ["ArrowRight", "ArrowUp"], ["ArrowUp"], [], ["ArrowLeft"], ["ArrowLeft", "ArrowUp"]],
  observe: (s) => {
    const p = s.player;
    const under = FLOORS.find((b) => p.x + PW > b.x && p.x < b.x + b.w);
    const next = FLOORS.find((b) => b.x > p.x + PW);
    const edge = under ? under.x + under.w : p.x;
    const near = s.walkers.map((w) => w.x - p.x).filter((d) => d > -80 && d < 900).sort((a, b) => a - b)[0];
    const ledge = LEDGES.find((b) => b.h < 100 && b.x + b.w > p.x && b.x < p.x + 900);
    return {
      x: Math.round(p.x), y: Math.round(p.y), vx: +p.vx.toFixed(1), vy: +p.vy.toFixed(1), onGround: p.ground,
      lives: p.lives, score: s.score, safe: p.safe, goalDx: Math.round(GOAL_X - p.x),
      groundEndsIn: Math.round(edge - p.x - PW), gapWidth: next ? Math.round(next.x - edge) : null,
      walkerDx: near === undefined ? null : Math.round(near),
      ledgeAhead: ledge ? { dx: Math.round(ledge.x - p.x), w: ledge.w, above: FLOOR - ledge.y } : null,
      jumpRises: Math.round((JUMP * JUMP) / (2 * GRAV)), runSpeed: RUN,
    };
  },
  progress: (s) => Math.min(1, s.player.x / GOAL_X),
  outcome: (s) => (flow.is(s, "won") ? "won" : flow.is(s, "gameOver") ? "lost" : null),
  events: {
    died: (a, b) => player.entered(a.player, b.player, "hurt"),
    coin: (a, b) => b.score > a.score,
  },
  machines: (s) => {
    const out: Record<string, ReturnType<typeof flow.watch>> = { flow: flow.watch(s), player: player.watch(s.player) };
    for (let i = 0; i < s.walkers.length; i++) out["walker-" + i] = walker.watch(s.walkers[i]);
    return out;
  },
};

// ---- the view ----------------------------------------------------------------------------------------------
const SKY = "#1b2440";
const HILLS = ["#26345c", "#2f4170", "#3a5185"];

function Ledge({ b, camX }: { b: Box; camX: number }) {
  const ground = b.h >= 100;
  return (
    <div
      style={{
        position: "absolute", left: b.x - camX, top: b.y, width: b.w, height: b.h,
        background: ground ? "#3b6b3f" : "#c98b4a", borderRadius: ground ? 0 : 10,
        borderTop: ground ? "14px solid #5fa35a" : "none",
        boxShadow: ground ? "none" : "0 10px 0 rgba(0,0,0,0.25)",
      }}
    />
  );
}

function Coin({ x, y, frame, i }: { x: number; y: number; frame: number; i: number }) {
  const spin = Math.abs(Math.cos((frame + i * 7) / 9));
  return (
    <div
      style={{
        position: "absolute", left: x - 18, top: y - 18 + Math.sin((frame + i * 5) / 12) * 4, width: 36, height: 36,
        borderRadius: 18, background: "#f6c945", border: "4px solid #c7921e",
        transform: `scaleX(${0.25 + spin * 0.75})`,
      }}
    />
  );
}

function WalkerView({ x, frame, looking }: { x: number; frame: number; looking: number }) {
  const bob = looking ? 0 : Math.abs(Math.sin(frame / 6)) * 6;
  // the eyes look the way it walks (or round, while it turns)
  const eye = looking ? looking * 6 : 0;
  return (
    <div style={{ position: "absolute", left: x, top: FLOOR - WALKER - bob, width: WALKER, height: WALKER, borderRadius: "35px 35px 10px 10px", background: "#b8463a" }}>
      <div style={{ position: "absolute", left: 14 + eye, top: 18, width: 14, height: 14, borderRadius: 7, background: "#fff" }} />
      <div style={{ position: "absolute", left: 42 + eye, top: 18, width: 14, height: 14, borderRadius: 7, background: "#fff" }} />
    </div>
  );
}

function Platformer() {
  const frame = useCurrentFrame();
  const stage = useVideoConfig();
  const g = useGameState(INITIAL, fold);
  const coins = useGameEvents(INITIAL, fold, (b, a) => a.score > b.score);
  const input = useInput();
  const p = g.player;
  const state = player.state(p);
  const scale = Math.min(stage.width / VW, stage.height / VH);
  const camX = Math.max(0, Math.min(LEVEL_W - VW, p.x - VW * 0.35));
  const blink = state === "hurt" || state === "respawn" || p.safe > 0;
  const flash = blink && Math.floor(frame / 6) % 2 === 0;
  const lastCoin = coins.length ? coins[coins.length - 1] : -99;
  const pop = Math.max(0, 1 - (frame - lastCoin) / 12);
  // the body reads the state: squashed landing, stretched in the air, tipped over when hurt
  const squash = state === "land" ? 0.86 : state === "jump" || state === "fall" ? 1.06 : 1;
  const tip = state === "hurt" || state === "dead" ? 90 : 0;
  const ending = flow.state(g);
  // what shows: the ledges, coins and walkers inside the view (and a margin)
  const inView = (x: number, w: number) => x + w > camX - 200 && x < camX + VW + 200;
  return (
    <AbsoluteFill style={{ background: SKY }}>
      <div style={{ position: "absolute", left: (stage.width - VW * scale) / 2, top: (stage.height - VH * scale) / 2, width: VW, height: VH, transform: `scale(${scale})`, transformOrigin: "0 0", overflow: "hidden" }}>
        {HILLS.map((c, i) => (
          <div key={"h" + i} style={{ position: "absolute", left: -((camX * (0.15 + i * 0.2)) % 1200), top: 520 + i * 110, width: VW + 2400, height: 600 }}>
            {[0, 1, 2, 3].map((j) => (
              <div key={j} style={{ position: "absolute", left: j * 1200 + i * 230, top: 0, width: 900, height: 600, borderRadius: "450px 450px 0 0", background: c }} />
            ))}
          </div>
        ))}
        {LEDGES.map((b, i) => (inView(b.x, b.w) ? <Ledge key={"l" + i} b={b} camX={camX} /> : null))}
        {COINS.map((c, i) => (g.got[i] === "0" && inView(c.x, 0) ? <Coin key={"c" + i} x={c.x - camX} y={c.y} frame={frame} i={i} /> : null))}
        {g.walkers.map((w, i) => {
          const ws = walker.state(w);
          return inView(w.x, WALKER) ? <WalkerView key={"w" + i} x={w.x - camX} frame={frame} looking={ws === "lookLeft" ? -1 : ws === "lookRight" ? 1 : 0} /> : null;
        })}
        <div
          style={{
            position: "absolute", left: p.x - camX, top: p.y, width: PW, height: PH, borderRadius: 14,
            background: "#f2efe4", opacity: flash ? 0.35 : 1, transformOrigin: "50% 100%",
            transform: `rotate(${tip}deg) scaleY(${squash}) skewX(${-p.vx * 0.6}deg)`,
          }}
        >
          <div style={{ position: "absolute", left: p.vx < 0 ? 8 : 30, top: 18, width: 16, height: 16, borderRadius: 8, background: "#1b2440" }} />
        </div>
        {/* debug overlays (a playtest's debug layer only, sdk/react/docs/playtest.md): what collides, and the state */}
        <Overlay name="hitboxes">
          {LEDGES.map((b, i) => (inView(b.x, b.w) ? <div key={"hb" + i} style={{ position: "absolute", left: b.x - camX, top: b.y, width: b.w, height: 4, background: "#00e5ff" }} /> : null))}
          {g.walkers.map((w, i) => (inView(w.x, WALKER) ? <div key={"hw" + i} style={{ position: "absolute", left: w.x - camX, top: FLOOR - WALKER, width: WALKER, height: WALKER, border: "4px solid #ff3df5", boxSizing: "border-box" }} /> : null))}
          <div style={{ position: "absolute", left: p.x - camX, top: p.y, width: PW, height: PH, border: "4px solid #00ff6a", boxSizing: "border-box" }} />
        </Overlay>
        <Overlay name="state">
          <div style={{ position: "absolute", left: 60, top: 130, padding: 16, background: "rgba(0,0,0,0.7)", fontFamily: "Menlo", fontSize: 30, lineHeight: "40px", color: "#00ff6a", whiteSpace: "pre" }}>
            {`frame ${frame}  keys ${input.keys.join("+") || "none"}\nx ${Math.round(p.x)}  y ${Math.round(p.y)}  vx ${p.vx.toFixed(1)}  vy ${p.vy.toFixed(1)}\n${flow.path(g).join(".")}  ${player.path(p).join(".")} (${player.steps(p)})  lives ${p.lives}`}
          </div>
        </Overlay>
        <div style={{ position: "absolute", left: 60, top: 40, fontFamily: "Helvetica", fontWeight: 700, fontSize: 64, color: "#f6c945", transform: `scale(${1 + pop * 0.15})`, transformOrigin: "0 50%" }}>
          {g.score}
        </div>
        <div style={{ position: "absolute", right: 60, top: 52, display: "flex", gap: 14 }}>
          {[0, 1, 2].map((i) => (
            <div key={i} style={{ width: 40, height: 40, borderRadius: 20, background: i < p.lives ? "#e05a4c" : "rgba(255,255,255,0.15)" }} />
          ))}
        </div>
        {ending !== "playing" ? (
          <div style={{ position: "absolute", left: 0, top: 360, width: VW, textAlign: "center", fontFamily: "Helvetica", fontWeight: 800, fontSize: 120, color: ending === "won" ? "#f6c945" : "#e05a4c" }}>
            {ending === "won" ? "LEVEL CLEAR" : "GAME OVER"}
          </div>
        ) : null}
      </div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="edit" component={Platformer} width={VW} height={VH} fps={120} durationInFrames={36000} />);
