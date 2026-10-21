// LEDGE, repaired (sdk/react/bench/playtest fixture): the gate's platformer with a higher jump and a ledge over every gap, so the
// level can be finished; the playtest bot reports the original as unfinishable at its first gap. LEDGE: a side-scrolling platformer, the gate's second game (pong is the first). Run and jump across a long level of
// ledges, take the coins, keep off the walkers. The game is a pure fold: live arrow keys when someone plays, a scripted
// run (a simple autopilot) when nobody does. The level is drawn in its own units and scaled to the stage.
import {
  Composition, Overlay, registerRoot, useCurrentFrame, useGameEvents, useGameState, useInput, useVideoConfig,
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
    // a raised ledge over every other stretch
    if (k % 2 === 1 || !ground) out.push({ x: x + 80, y: FLOOR - 220 - Math.floor(seed(k + 7) * 180), w: Math.max(200, w - 220), h: 36 });
    x += w + (ground ? 0 : 0) + (k % 5 === 3 ? 260 : 0);
    k++;
  }
  return out;
})();

export const COINS: { x: number; y: number }[] = LEDGES.filter((b) => b.h < 100).flatMap((b, i) =>
  [0, 1, 2].map((j) => ({ x: b.x + 40 + j * Math.max(50, (b.w - 80) / 3), y: b.y - 70 - (i % 2) * 20 })),
);

// walkers pace a ground stretch: where one stands is a function of the frame alone
export const WALKERS: { x0: number; span: number; speed: number }[] = LEDGES.filter((b, i) => b.y === FLOOR && i % 3 === 2 && b.w > 520).map((b, i) => ({
  x0: b.x + 60, span: b.w - 180, speed: 2.2 + (i % 3) * 0.8,
}));
export const walkerAt = (w: { x0: number; span: number; speed: number }, frame: number) => {
  const t = (frame * w.speed) % (w.span * 2);
  return w.x0 + (t < w.span ? t : w.span * 2 - t);
};

// ---- the game core (pure) ---------------------------------------------------------------------------------
const PW = 56;
const PH = 84;
const RUN = 9;
const AIR = 0.6;
const JUMP = -28; // playtest fixture: -21 rose 232 px, under every raised ledge (315 to 400 px up)
const GRAV = 0.95;

export interface Game {
  x: number;
  y: number;
  vx: number;
  vy: number;
  ground: boolean;
  /** one flag per coin, as a string of 0/1 (a pure value) */
  got: string;
  score: number;
  lives: number;
  hurt: number;
  human: boolean;
}

export const INITIAL: Game = {
  x: 120, y: FLOOR - PH, vx: 0, vy: 0, ground: true,
  got: "0".repeat(COINS.length), score: 0, lives: 3, hurt: 0, human: false,
};

const overlap = (a: Box, b: Box) => a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;

/** one frame of the game. Pure: new object out, the same input in every time. */
export function step(s: Game, keys: string[], frame: number): Game {
  const left = keys.indexOf("ArrowLeft") >= 0;
  const right = keys.indexOf("ArrowRight") >= 0;
  const jump = keys.indexOf("ArrowUp") >= 0 || keys.indexOf(" ") >= 0;
  const want = (right ? RUN : 0) - (left ? RUN : 0);
  let vx = s.ground ? want : s.vx + (want - s.vx) * AIR * 0.2;
  let vy = s.vy + GRAV;
  if (jump && s.ground) vy = JUMP;
  let x = Math.max(0, Math.min(LEVEL_W - PW, s.x + vx));
  let y = s.y + vy;
  let ground = false;
  // land on a ledge from above, bump a ledge's underside from below
  for (const b of LEDGES) {
    if (x + PW <= b.x || x >= b.x + b.w) continue;
    if (s.y + PH <= b.y && y + PH >= b.y && vy >= 0) {
      y = b.y - PH;
      vy = 0;
      ground = true;
    } else if (b.h < 100 && s.y >= b.y + b.h && y < b.y + b.h && vy < 0) {
      y = b.y + b.h;
      vy = 0;
    }
  }
  // the coins it touches
  let got = s.got;
  let score = s.score;
  const me = { x, y, w: PW, h: PH };
  for (let i = 0; i < COINS.length; i++) {
    if (got[i] === "1") continue;
    const c = COINS[i];
    if (overlap(me, { x: c.x - 18, y: c.y - 18, w: 36, h: 36 })) {
      got = got.slice(0, i) + "1" + got.slice(i + 1);
      score += 10;
    }
  }
  // a walker or a fall: back to the start of the stretch, a life gone
  let { lives, hurt } = s;
  hurt = Math.max(0, hurt - 1);
  const hit = hurt === 0 && WALKERS.some((w) => overlap(me, { x: walkerAt(w, frame), y: FLOOR - 70, w: 70, h: 70 }));
  if (hit || y > VH + 200) {
    lives = lives > 1 ? lives - 1 : 3;
    hurt = 90;
    const back = Math.max(120, x - 600);
    return { ...s, x: back, y: FLOOR - PH - 400, vx: 0, vy: 0, ground: false, got, score, lives, hurt };
  }
  return { ...s, x, y, vx, vy, ground, got, score, lives, hurt };
}

// the scripted run when nobody plays: right, and a jump before every gap, ledge and walker
const EMPTY: string[] = [];
function autopilot(s: Game, frame: number): string[] {
  const ahead = s.x + PW + 70;
  const gap = !LEDGES.some((b) => b.y === FLOOR && ahead >= b.x && ahead <= b.x + b.w);
  const walker = WALKERS.some((w) => {
    const d = walkerAt(w, frame) - s.x;
    return d > 0 && d < 190;
  });
  const ledge = LEDGES.some((b) => b.h < 100 && b.x - s.x > 40 && b.x - s.x < 160);
  return gap || walker || ledge || frame % 97 === 0 ? ["ArrowRight", "ArrowUp"] : ["ArrowRight"];
}

const fold = (s: Game, i: { keys?: string[] }, f: number): Game => {
  const live = i && i.keys && i.keys.length ? i.keys : null;
  if (live && !s.human) return { ...INITIAL, human: true };
  const out = step(s, live ? live : s.human ? EMPTY : autopilot(s, f), f);
  return s.human ? { ...out, human: true } : out;
};

// ---- the playtest (sdk/react/docs/playtest.md): what a bot steps, sees and aims for -------------------------------------
// The level has no finish line of its own (it ends at a wall and lives wrap from 1 back to 3), so the goal here is the
// playtest's reading of the level: reach its right end.
const GOAL_X = LEVEL_W - PW - 1;
export const playtest: Playtest<Game> = {
  initial: INITIAL,
  step: fold,
  actions: [["ArrowRight"], ["ArrowRight", "ArrowUp"], ["ArrowUp"], [], ["ArrowLeft"], ["ArrowLeft", "ArrowUp"]],
  observe: (s, frame) => {
    const ground = LEDGES.filter((b) => b.y === FLOOR);
    const under = ground.find((b) => s.x + PW > b.x && s.x < b.x + b.w);
    const next = ground.find((b) => b.x > s.x + PW);
    const edge = under ? under.x + under.w : s.x;
    const walker = WALKERS.map((w) => walkerAt(w, frame) - s.x).filter((d) => d > -80 && d < 900).sort((a, b) => a - b)[0];
    const ledge = LEDGES.find((b) => b.h < 100 && b.x + b.w > s.x && b.x < s.x + 900);
    return {
      x: Math.round(s.x), y: Math.round(s.y), vx: +s.vx.toFixed(1), vy: +s.vy.toFixed(1), onGround: s.ground,
      lives: s.lives, score: s.score, hurt: s.hurt, goalDx: Math.round(GOAL_X - s.x),
      groundEndsIn: Math.round(edge - s.x - PW), gapWidth: next ? Math.round(next.x - edge) : null,
      walkerDx: walker === undefined ? null : Math.round(walker),
      ledgeAhead: ledge ? { dx: Math.round(ledge.x - s.x), w: ledge.w, above: FLOOR - ledge.y } : null,
      jumpRises: Math.round((JUMP * JUMP) / (2 * GRAV)), runSpeed: RUN,
    };
  },
  progress: (s) => s.x / GOAL_X,
  outcome: (s) => (s.x >= GOAL_X ? "won" : null),
  events: { died: (a, b) => b.hurt === 90 && a.hurt !== 90, coin: (a, b) => b.score > a.score },
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

function Walker({ x, frame }: { x: number; frame: number }) {
  const bob = Math.abs(Math.sin(frame / 6)) * 6;
  return (
    <div style={{ position: "absolute", left: x, top: FLOOR - 70 - bob, width: 70, height: 70, borderRadius: "35px 35px 10px 10px", background: "#b8463a" }}>
      <div style={{ position: "absolute", left: 14, top: 18, width: 14, height: 14, borderRadius: 7, background: "#fff" }} />
      <div style={{ position: "absolute", left: 42, top: 18, width: 14, height: 14, borderRadius: 7, background: "#fff" }} />
    </div>
  );
}

function Platformer() {
  const frame = useCurrentFrame();
  const stage = useVideoConfig();
  const g = useGameState(INITIAL, fold);
  const coins = useGameEvents(INITIAL, fold, (b, a) => a.score > b.score);
  const input = useInput();
  const scale = Math.min(stage.width / VW, stage.height / VH);
  const camX = Math.max(0, Math.min(LEVEL_W - VW, g.x - VW * 0.35));
  const flash = g.hurt > 0 && Math.floor(g.hurt / 6) % 2 === 0;
  const lastCoin = coins.length ? coins[coins.length - 1] : -99;
  const pop = Math.max(0, 1 - (frame - lastCoin) / 12);
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
        {WALKERS.map((w, i) => {
          const x = walkerAt(w, frame);
          return inView(x, 70) ? <Walker key={"w" + i} x={x - camX} frame={frame} /> : null;
        })}
        <div
          style={{
            position: "absolute", left: g.x - camX, top: g.y, width: PW, height: PH, borderRadius: 14,
            background: "#f2efe4", opacity: flash ? 0.35 : 1,
            transform: `scaleY(${g.ground ? 1 : 1.06}) skewX(${-g.vx * 0.6}deg)`,
          }}
        >
          <div style={{ position: "absolute", left: g.vx < 0 ? 8 : 30, top: 18, width: 16, height: 16, borderRadius: 8, background: "#1b2440" }} />
        </div>
        {/* debug overlays (a playtest's debug layer only, sdk/react/docs/playtest.md): what collides, and the state */}
        <Overlay name="hitboxes">
          {LEDGES.map((b, i) => (inView(b.x, b.w) ? <div key={"hb" + i} style={{ position: "absolute", left: b.x - camX, top: b.y, width: b.w, height: 4, background: "#00e5ff" }} /> : null))}
          {WALKERS.map((w, i) => {
            const x = walkerAt(w, frame);
            return inView(x, 70) ? <div key={"hw" + i} style={{ position: "absolute", left: x - camX, top: FLOOR - 70, width: 70, height: 70, border: "4px solid #ff3df5", boxSizing: "border-box" }} /> : null;
          })}
          <div style={{ position: "absolute", left: g.x - camX, top: g.y, width: PW, height: PH, border: "4px solid #00ff6a", boxSizing: "border-box" }} />
        </Overlay>
        <Overlay name="state">
          <div style={{ position: "absolute", left: 60, top: 130, padding: 16, background: "rgba(0,0,0,0.7)", fontFamily: "Menlo", fontSize: 30, lineHeight: "40px", color: "#00ff6a", whiteSpace: "pre" }}>
            {`frame ${frame}  keys ${input.keys.join("+") || "none"}\nx ${Math.round(g.x)}  y ${Math.round(g.y)}  vx ${g.vx.toFixed(1)}  vy ${g.vy.toFixed(1)}\n${g.ground ? "on ground" : "in air"}  hurt ${g.hurt}  lives ${g.lives}`}
          </div>
        </Overlay>
        <div style={{ position: "absolute", left: 60, top: 40, fontFamily: "Helvetica", fontWeight: 700, fontSize: 64, color: "#f6c945", transform: `scale(${1 + pop * 0.15})`, transformOrigin: "0 50%" }}>
          {g.score}
        </div>
        <div style={{ position: "absolute", right: 60, top: 52, display: "flex", gap: 14 }}>
          {[0, 1, 2].map((i) => (
            <div key={i} style={{ width: 40, height: 40, borderRadius: 20, background: i < g.lives ? "#e05a4c" : "rgba(255,255,255,0.15)" }} />
          ))}
        </div>
      </div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="edit" component={Platformer} width={VW} height={VH} fps={120} durationInFrames={36000} />);
