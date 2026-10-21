// PONG — a table-tennis table seen from directly overhead in an empty night hall,
// turned sideways into a classic court: you play the left paddle, the computer the right.
// The game is a pure fold: live arrow keys when someone plays, the scripted attract
// match (input.ts) when nobody does. The court is drawn in its own units and scaled
// to fill whatever stage it is given.
import {
  Audio, Composition, Easing, Mix, Sequence, interpolate, registerRoot, sao,
  useCurrentFrame, useGameEvents, useGameState, useInput, useVideoConfig,
} from "@raea/motion";
import { AbsoluteFill } from "@raea/motion";
import { css } from "./styles.gen";
import { INPUT } from "./input";

// ---- sim:physics:start ------------------------------------------------- the game core (pure)
export const CW = 1920;
export const CH = 1080;
export const WALL_T = 66;
export const WALL_B = 1014;
export const END_L = 50;
export const END_R = 1870;
export const BOUNCE_L = 680;
export const BOUNCE_R = 1240;
export const PADDLE_LEN = 300;
export const PADDLE_TH = 56;
export const PXX = 190;
export const CXX = 1730;
export const BR = 28;
export const V0 = 50;
export const VMAX = 84;
export const P_SPEED = 50;
export const C_SPEED = 40;
export const TARGET = 3;

export type Contact = "paddleP" | "paddleC" | "wall" | "table" | "serve" | null;
export interface Mark {
  x: number;
  y: number;
  at: number;
  kind: "paddleP" | "paddleC" | "wall" | "table" | "serve" | "pointP" | "pointC";
}
export interface Game {
  bx: number;
  by: number;
  vx: number;
  vy: number;
  py: number;
  cy: number;
  sP: number;
  sC: number;
  serve: number;
  rally: number;
  phase: "play" | "pause" | "over";
  pause: number;
  contact: Contact;
  point: "P" | "C" | null;
  ended: number | null;
  human: boolean;
  humanAt: number | null;
  marks: Mark[];
}

export const INITIAL: Game = {
  bx: CW / 2, by: CH / 2, vx: 0, vy: 0,
  py: CH / 2, cy: CH / 2,
  sP: 0, sC: 0,
  serve: 0, rally: 0,
  phase: "pause", pause: 8,
  contact: null, point: null, ended: null,
  human: false, humanAt: null,
  marks: [],
};

const clamp = (v: number, a: number, b: number) => (v < a ? a : v > b ? b : v);
const seed = (n: number) => ((((n * 37 + 11) % 97) + 97) % 97) / 97;

/** one frame of the game. Pure: new object out, same input in every time. */
export function step(s: Game, keys: string[], frame: number): Game {
  const marks = s.marks.filter((m) => frame - m.at < 34);
  if (s.phase === "over") return { ...s, contact: null, marks };

  // the player's paddle: up and down
  const up = keys.indexOf("ArrowUp") >= 0 || keys.indexOf("w") >= 0 || keys.indexOf("W") >= 0;
  const down = keys.indexOf("ArrowDown") >= 0 || keys.indexOf("s") >= 0 || keys.indexOf("S") >= 0;
  const steer = (down ? 1 : 0) - (up ? 1 : 0);
  const py = clamp(s.py + steer * P_SPEED, WALL_T + PADDLE_LEN / 2, WALL_B - PADDLE_LEN / 2);

  // the opponent: chases the incoming ball with a seeded misjudgement that grows with the rally
  const chasing = s.phase === "play" && s.vx > 0;
  const aim = chasing ? s.by + (seed(s.serve * 5 + s.rally * 3) - 0.5) * (120 + Math.max(0, s.rally - 2) * 195) : CH / 2;
  const cy = clamp(s.cy + clamp(aim - s.cy, -C_SPEED, C_SPEED), WALL_T + PADDLE_LEN / 2, WALL_B - PADDLE_LEN / 2);

  // between points: the ball rests at the centre, then the serve snaps it away
  if (s.phase === "pause") {
    const pause = s.pause - 1;
    if (pause > 0) return { ...s, py, cy, pause, contact: null, point: s.point, marks };
    const dir = s.serve % 2 === 0 ? 1 : -1;
    const ang = (seed(s.serve * 11 + 3) - 0.5) * 2 * 12;
    return {
      ...s, py, cy,
      bx: CW / 2, by: CH / 2, vx: dir * Math.sqrt(V0 * V0 - ang * ang), vy: ang,
      phase: "play", pause: 0, rally: 0,
      contact: "serve", point: null,
      marks: [...marks, { x: CW / 2, y: CH / 2, at: frame, kind: "serve" }],
    };
  }

  // the ball
  let bx = s.bx + s.vx;
  let by = s.by + s.vy;
  let vx = s.vx;
  let vy = s.vy;
  let rally = s.rally;
  let contact: Contact = null;

  if (by - BR < WALL_T) {
    by = WALL_T + BR;
    vy = -vy;
    contact = "wall";
  } else if (by + BR > WALL_B) {
    by = WALL_B - BR;
    vy = -vy;
    contact = "wall";
  }
  const next: Mark[] = contact === "wall" ? [...marks, { x: bx, y: by, at: frame, kind: "wall" }] : marks;

  // the table bounce: once per trip, on the far side of the net
  const bouncedRight = vx > 0 && s.bx < BOUNCE_R && bx >= BOUNCE_R;
  const bouncedLeft = vx < 0 && s.bx > BOUNCE_L && bx <= BOUNCE_L;
  if (bouncedRight || bouncedLeft) {
    contact = "table";
    next.push({ x: bx, y: by, at: frame, kind: "table" });
  }

  // the player's paddle (the ball coming left)
  if (vx < 0 && bx - BR <= PXX + PADDLE_TH / 2 && s.bx - BR > PXX + PADDLE_TH / 2) {
    const off = (by - py) / (PADDLE_LEN / 2);
    if (Math.abs(off) <= 1 + BR / (PADDLE_LEN / 2)) {
      const speed = Math.min(Math.sqrt(vx * vx + vy * vy) * 1.05, VMAX);
      const nvy = clamp(off * speed * 0.62 + vy * 0.28, -speed * 0.85, speed * 0.85);
      vx = Math.sqrt(Math.max(speed * speed - nvy * nvy, speed * 0.45 * (speed * 0.45)));
      vy = nvy;
      bx = PXX + PADDLE_TH / 2 + BR;
      rally = s.rally + 1;
      contact = "paddleP";
      next.push({ x: bx, y: by, at: frame, kind: "paddleP" });
    }
  }

  // the opponent's paddle (the ball going right)
  if (vx > 0 && bx + BR >= CXX - PADDLE_TH / 2 && s.bx + BR < CXX - PADDLE_TH / 2) {
    const off = (by - cy) / (PADDLE_LEN / 2);
    if (Math.abs(off) <= 1 + BR / (PADDLE_LEN / 2)) {
      const speed = Math.min(Math.sqrt(vx * vx + vy * vy) * 1.05, VMAX);
      const nvy = clamp(off * speed * 0.62 + vy * 0.28, -speed * 0.85, speed * 0.85);
      vx = -Math.sqrt(Math.max(speed * speed - nvy * nvy, speed * 0.45 * (speed * 0.45)));
      vy = nvy;
      bx = CXX - PADDLE_TH / 2 - BR;
      rally = s.rally + 1;
      contact = "paddleC";
      next.push({ x: bx, y: by, at: frame, kind: "paddleC" });
    }
  }

  // a point: past either end
  const pastLeft = bx - BR < END_L;
  const pastRight = bx + BR > END_R;
  if (pastLeft || pastRight) {
    const who: "P" | "C" = pastLeft ? "C" : "P";
    const sP = who === "P" ? s.sP + 1 : s.sP;
    const sC = who === "C" ? s.sC + 1 : s.sC;
    const done = sP >= TARGET || sC >= TARGET;
    next.push({ x: bx, y: by, at: frame, kind: who === "P" ? "pointP" : "pointC" });
    return {
      ...s, py, cy,
      bx: CW / 2, by: CH / 2, vx: 0, vy: 0,
      sP, sC,
      serve: s.serve + 1, rally: 0,
      phase: done ? "over" : "pause",
      pause: done ? 0 : 18,
      contact: null,
      point: who,
      ended: done ? frame : null,
      marks: next,
    };
  }

  return {
    ...s, py, cy, bx, by, vx, vy, rally,
    phase: "play", contact, point: null, marks: next,
  };
}
// ---- sim:physics:end ---------------------------------------------------

// ---- sound: one family, celluloid, hollow wood, rubber soles ----
const POCK = sao("a single crisp ping-pong paddle strike on a celluloid ball, hollow wooden body, dry and close, very short", { seconds: 0.5 });
const TOCK = sao("a single ping-pong ball click against the rubber face of a paddle, duller and lower than wood, dry, very short", { seconds: 0.5 });
const THOCK = sao("a single dull thock of a ping-pong ball hitting the padded edge of a table, short and dry", { seconds: 0.4 });
const TAK = sao("a single soft tak of a celluloid ball bouncing once on a taut table surface, short and dry", { seconds: 0.4 });
const CLACK = sao("a single wooden scoreboard flip clack with a faint spring, short and dry", { seconds: 0.8 });
const SCUFF = sao("one quick rubber shoe squeak on a polished wooden sports hall floor, dry, short", { seconds: 0.35 });
const BELL = sao("one warm brass gym bell ding with a short tail, a match ending, dry", { seconds: 1.6 });
const HALL = sao("empty indoor sports hall room tone, a faint distant hum, very quiet and still", { seconds: 40 });

const EMPTY: string[] = [];
const scripted = (keys: string[] | undefined, frame: number): string[] =>
  keys && keys.length ? keys : INPUT[frame] || EMPTY;

const fold = (s: Game, i: { keys?: string[] }, f: number): Game => {
  const live = i && i.keys && i.keys.length ? i.keys : null;
  if (live && !s.human) {
    // the person takes over: their match begins from a fresh serve
    return { ...INITIAL, human: true, humanAt: f, phase: "pause", pause: 12 };
  }
  const keys = live ? live : s.human ? EMPTY : INPUT[f] || EMPTY;
  const out = step(s, keys, f);
  return s.human ? { ...out, human: true } : out;
};

/** the frames the player sets off: a shoe on the floor, spaced so they read as steps */
function stepsOf(timeline: Record<number, string[]>): number[] {
  const out: number[] = [];
  const frames = Object.keys(timeline)
    .map(Number)
    .sort((a, b) => a - b);
  let prev = "";
  let prevF = -99;
  let last = -99;
  for (const f of frames) {
    const dir = timeline[f][0] || "";
    if (dir && (dir !== prev || f - prevF > 6) && f - last >= 16) {
      out.push(f);
      last = f;
    }
    prev = dir;
    prevF = f;
  }
  return out;
}
const STEPS = stepsOf(INPUT);

function Sounds({
  hits, walls, tables, points, over, steps,
}: {
  hits: number[]; walls: number[]; tables: number[]; points: number[]; over: number[]; steps: number[];
}) {
  return (
    <>
      <Sequence from={0} layout="none">
        <Audio src={HALL} gainDb={-22} fadeIn={1.4} fadeOut={2.6} />
      </Sequence>
      {hits.map((f, i) => (
        <Sequence key={"h" + i} from={f} layout="none">
          <Audio src={i % 2 ? TOCK : POCK} gainDb={i % 2 ? -6 : -4} fadeIn={0.012} fadeOut={0.08} />
        </Sequence>
      ))}
      {walls.map((f, i) => (
        <Sequence key={"w" + i} from={f} layout="none">
          <Audio src={THOCK} gainDb={-9} fadeIn={0.012} fadeOut={0.08} />
        </Sequence>
      ))}
      {tables.map((f, i) => (
        <Sequence key={"t" + i} from={f} layout="none">
          <Audio src={TAK} gainDb={-9} fadeIn={0.012} fadeOut={0.08} />
        </Sequence>
      ))}
      {steps.map((f, i) => (
        <Sequence key={"s" + i} from={f} layout="none">
          <Audio src={SCUFF} gainDb={-14} fadeIn={0.012} fadeOut={0.06} />
        </Sequence>
      ))}
      {points.map((f, i) => (
        <Sequence key={"p" + i} from={f} layout="none">
          <Audio src={CLACK} gainDb={-5} fadeIn={0.012} fadeOut={0.12} />
        </Sequence>
      ))}
      {over.map((f, i) => (
        <Sequence key={"o" + i} from={f} layout="none">
          <Audio src={BELL} gainDb={-7} fadeIn={0.012} fadeOut={0.4} />
        </Sequence>
      ))}
    </>
  );
}

function Pong() {
  const frame = useCurrentFrame();
  const stage = useVideoConfig();
  const inp = useInput();
  const g = useGameState(INITIAL, fold);

  const hits = useGameEvents(INITIAL, fold, (b, a) => a.contact !== null && b.contact === null && a.contact !== "wall" && a.contact !== "table");
  const walls = useGameEvents(INITIAL, fold, (b, a) => a.contact === "wall" && b.contact !== "wall");
  const tables = useGameEvents(INITIAL, fold, (b, a) => a.contact === "table" && b.contact !== "table");
  const points = useGameEvents(INITIAL, fold, (b, a) => a.point !== null && b.point === null);
  const over = useGameEvents(INITIAL, fold, (b, a) => a.phase === "over" && b.phase !== "over");

  const scale = Math.min(stage.width / CW, stage.height / CH);
  const ox = (stage.width - CW * scale) / 2;
  const oy = (stage.height - CH * scale) / 2;

  const flashOf = (kind: Mark["kind"]) => {
    let best = 0;
    for (const m of g.marks) {
      const age = frame - m.at;
      if (m.kind === kind && age >= 0 && age < 6) best = Math.max(best, 1 - age / 6);
    }
    return best;
  };
  const steerLean = (g.bx > 0 && (INPUT[frame] || EMPTY)[0] === "ArrowDown") || (inp && inp.keys && inp.keys.indexOf("ArrowDown") >= 0) ? 1.6 : 0;

  // the outcome, and when it landed
  const won = g.sP >= TARGET || g.sC >= TARGET;
  const endAt = g.ended !== null && g.ended <= frame ? g.ended : frame - 6;
  const cardLocal = won ? Math.max(0, frame - endAt) : 0;
  const cardO = won ? Math.min(1, cardLocal / 6) : 0;
  const cardRise = won ? (1 - Math.min(1, cardLocal / 8)) * 46 : 0;

  const hintShow = frame < 78 || (g.human && g.humanAt !== null && frame - g.humanAt < 90);
  const hintO = hintShow ? (frame < 60 ? 1 : Math.max(0, 1 - (frame - 60) / 18)) : 0;

  return (
    <AbsoluteFill style={{ backgroundColor: "var(--brand-background)" }}>
      <style>{css}</style>

      {/* the court, scaled to fill the stage */}
      <div
        style={{
          position: "absolute", left: ox, top: oy, width: CW, height: CH,
          transform: `scale(${scale})`, transformOrigin: "top left",
        }}
      >
        {/* the hall's light: a pool on the table, the corners falling away */}
        <div
          style={{
            position: "absolute", left: 0, top: 0, width: CW, height: CH,
            background:
              "radial-gradient(118% 60% at 50% 46%, rgba(244,234,214,0.13), rgba(244,234,214,0.02) 58%, rgba(3,20,15,0.24) 100%)",
          }}
        />

        {/* the table's paint */}
        <div
          style={{
            position: "absolute", left: 40, top: 28, width: CW - 80, height: CH - 56,
            border: "10px solid rgba(244,234,214,0.6)", borderRadius: 10,
          }}
        />
        <div
          style={{
            position: "absolute", left: 50, top: CH / 2 - 4, width: CW - 100, height: 8,
            background: "repeating-linear-gradient(90deg, rgba(244,234,214,0.34) 0 38px, rgba(244,234,214,0) 38px 68px)",
          }}
        />
        {/* the net */}
        <div style={{ position: "absolute", left: CW / 2 - 16, top: 28, width: 32, height: CH - 56, background: "rgba(244,234,214,0.18)" }} />
        <div style={{ position: "absolute", left: CW / 2 - 5, top: 28, width: 10, height: CH - 56, background: "rgba(244,234,214,0.75)" }} />
        <div style={{ position: "absolute", left: CW / 2 - 24, top: 14, width: 48, height: 26, background: "var(--brand-ink)", borderRadius: 5 }} />
        <div style={{ position: "absolute", left: CW / 2 - 24, top: CH - 40, width: 48, height: 26, background: "var(--brand-ink)", borderRadius: 5 }} />

        {/* the hint */}
        <div
          style={{
            position: "absolute", left: 0, top: 150, width: CW, textAlign: "center",
            fontFamily: "var(--brand-font-heading)", fontWeight: 700, fontSize: "var(--brand-h3-size)",
            letterSpacing: "0.16em", color: "var(--brand-text)", opacity: hintO,
          }}
        >
          {g.human ? "YOUR MATCH — ARROW KEYS TO MOVE" : "FIRST TO 3"}
        </div>

        {/* the score, painted on the table */}
        <Score n={g.sP} x={170} y={260} live={g.point === "P"} align="left" />
        <Score n={g.sC} x={CW - 490} y={260} live={g.point === "C"} align="right" />

        {/* contact rings */}
        {g.marks.map((m, i) => {
          const age = frame - m.at;
          const isPoint = m.kind === "pointP" || m.kind === "pointC";
          const isTap = m.kind === "table";
          const life = isPoint ? 32 : isTap ? 18 : 22;
          const r = interpolate(age, [0, life], [BR, isPoint ? 210 : isTap ? 84 : 104], {
            extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: Easing.out(Easing.cubic),
          });
          const o = interpolate(age, [0, life], [isPoint ? 0.7 : 0.85, 0], {
            extrapolateLeft: "clamp", extrapolateRight: "clamp",
          });
          return (
            <div
              key={i}
              style={{
                position: "absolute", left: m.x - r, top: m.y - r, width: r * 2, height: r * 2,
                borderRadius: "50%", opacity: o,
                border: `7px solid ${isPoint ? "var(--brand-accent)" : "var(--brand-text)"}`,
              }}
            />
          );
        })}

        {/* the ball: ghosts, shadow, then the ball itself */}
        {g.phase === "play" ? (
          <>
            <div
              style={{
                position: "absolute", left: g.bx - g.vx * 1.15 * 3 - BR, top: g.by - g.vy * 1.15 * 3 - BR,
                width: BR * 2, height: BR * 2, borderRadius: "50%", background: "var(--brand-text)", opacity: 0.06,
              }}
            />
            <div
              style={{
                position: "absolute", left: g.bx - g.vx * 1.15 * 2 - BR, top: g.by - g.vy * 1.15 * 2 - BR,
                width: BR * 2, height: BR * 2, borderRadius: "50%", background: "var(--brand-text)", opacity: 0.1,
              }}
            />
            <div
              style={{
                position: "absolute", left: g.bx - g.vx * 1.15 - BR, top: g.by - g.vy * 1.15 - BR,
                width: BR * 2, height: BR * 2, borderRadius: "50%", background: "var(--brand-text)", opacity: 0.16,
              }}
            />
          </>
        ) : null}
        <div
          style={{
            position: "absolute", left: g.bx + 13 - BR, top: g.by + 18 - BR, width: BR * 2, height: BR * 2,
            borderRadius: "50%", background: "rgba(3,20,15,0.3)",
          }}
        />
        <div
          style={{
            position: "absolute", left: g.bx - BR, top: g.by - BR, width: BR * 2, height: BR * 2,
            borderRadius: "50%", background: "var(--brand-text)",
          }}
        />

        {/* the opponent's paddle, then the player's */}
        <Paddle x={CXX} y={g.cy} face="var(--brand-ink)" edge="var(--brand-color-3)" flash={flashOf("paddleC")} lean={0} edgeLeft={false} />
        <Paddle x={PXX} y={g.py} face="var(--brand-accent)" edge="var(--brand-text)" flash={flashOf("paddleP")} lean={steerLean} edgeLeft />

        {/* the outcome */}
        {won ? (
          <div style={{ position: "absolute", left: 0, top: 0, width: CW, height: CH, background: "rgba(3,20,15,0.42)", opacity: cardO }}>
            <div
              style={{
                position: "absolute", left: CW / 2 - 430, top: CH / 2 - 260 + cardRise, width: 860, height: 520,
                background: "var(--brand-accent)", borderRadius: "var(--brand-radius)",
                display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
                boxShadow: "0 30px 70px rgba(3,20,15,0.5)",
              }}
            >
              <div
                style={{
                  fontFamily: "var(--brand-font-heading)", fontWeight: 700, lineHeight: 1,
                  fontSize: "var(--brand-h3-size)", letterSpacing: "0.16em", color: "var(--brand-on-accent)",
                }}
              >
                {g.sP > g.sC ? "YOU WIN" : "YOU LOSE"}
              </div>
              <div
                style={{
                  marginTop: 34, fontFamily: "var(--brand-font-heading)", fontWeight: 700, lineHeight: 1,
                  fontSize: "var(--brand-h1-size)", letterSpacing: "0.01em", color: "var(--brand-on-accent)",
                  fontVariantNumeric: "tabular-nums",
                }}
              >
                {g.sP} — {g.sC}
              </div>
            </div>
          </div>
        ) : null}
      </div>

      <Sounds hits={hits} walls={walls} tables={tables} points={points} over={over} steps={STEPS} />
      <Mix lufs={-14} ceiling={-1.5} />
    </AbsoluteFill>
  );
}

function Score({ n, x, y, live, align }: { n: number; x: number; y: number; live: boolean; align: "left" | "right" }) {
  return (
    <div
      style={{
        position: "absolute", left: x, top: y, width: 320, textAlign: align,
        fontFamily: "var(--brand-font-heading)", fontWeight: 700, lineHeight: 1,
        fontSize: "var(--brand-h1-size)", letterSpacing: "0.01em",
        color: "var(--brand-text)", opacity: live ? 1 : 0.85,
        fontVariantNumeric: "tabular-nums",
        transform: `scale(${live ? 1.1 : 1})`, transformOrigin: align === "left" ? "0 50%" : "100% 50%",
      }}
    >
      {n}
    </div>
  );
}

function Paddle({
  x, y, face, edge, flash, lean, edgeLeft,
}: {
  x: number; y: number; face: string; edge: string; flash: number; lean: number; edgeLeft: boolean;
}) {
  return (
    <div
      style={{
        position: "absolute", left: x - PADDLE_TH / 2, top: y - PADDLE_LEN / 2,
        width: PADDLE_TH, height: PADDLE_LEN,
        background: face, borderRadius: "var(--brand-radius)",
        transform: `rotate(${lean}deg) scale(${1 + flash * 0.05})`,
        boxShadow: "0 14px 0 rgba(3,20,15,0.24)",
      }}
    >
      <div
        style={{
          position: "absolute", top: 18, bottom: 18, width: 9, borderRadius: 5,
          left: edgeLeft ? 8 : undefined, right: edgeLeft ? undefined : 8,
          background: edge, opacity: 0.5 + flash * 0.5,
        }}
      />
    </div>
  );
}

registerRoot(() => (
  <Composition id="edit" component={Pong} width={CW} height={CH} fps={120} durationInFrames={4668} />
));
