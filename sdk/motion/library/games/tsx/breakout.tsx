// record: bevy-games-breakout, juice-it-or-lose-it
// Bevy's Breakout showcase in raea's SDK, juiced as in "Juice it or lose it": bricks pop out with a squash and a
// flash, the paddle squashes on contact, and the camera kicks on every break. The state is a pure fold over the input;
// a person's keys move the paddle when `live`, otherwise the paddle tracks the ball (the scripted play for renders).
// Played live on void's stage, the 3D follows the person's keys frame by frame, as a render of their input does.
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame, useGameState, type FrameInput } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, Sphere } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 240;
const COLS = 8, ROWS = 4;
const W = 8, TOP = 4.2; // field: x in [-W/2, W/2], y from 0 (paddle) to TOP
interface Game { px: number; bx: number; by: number; vx: number; vy: number; alive: boolean[]; broke: number[]; paddleHit: number; score: number }
const INITIAL: Game = { px: 0, bx: 0, by: 0.6, vx: 0.07, vy: 0.09, alive: Array(COLS * ROWS).fill(true), broke: Array(COLS * ROWS).fill(-99), paddleHit: -99, score: 0 };
const brickPos = (i: number): [number, number] => [(i % COLS - (COLS - 1) / 2) * (W / COLS), TOP - 0.3 - Math.floor(i / COLS) * 0.42];

function step(s: Game, input: FrameInput & { live?: boolean }, frame: number): Game {
  const dir = input.live ? (input.keys.includes("ArrowRight") ? 1 : 0) - (input.keys.includes("ArrowLeft") ? 1 : 0) : Math.sign(s.bx - s.px) * Math.min(1, Math.abs(s.bx - s.px) * 4);
  const px = Math.max(-W / 2 + 0.7, Math.min(W / 2 - 0.7, s.px + dir * 0.12));
  let { bx, by, vx, vy, paddleHit, score } = s;
  bx += vx; by += vy;
  if (bx < -W / 2 + 0.12 || bx > W / 2 - 0.12) vx = -vx;
  if (by > TOP) vy = -Math.abs(vy);
  if (by < 0.15 && vy < 0 && Math.abs(bx - px) < 0.8) { vy = Math.abs(vy); vx += (bx - px) * 0.04; paddleHit = frame; }
  if (by < -1) { bx = px; by = 0.6; vy = 0.09; }
  const alive = s.alive.slice(), broke = s.broke.slice();
  for (let i = 0; i < alive.length; i++) {
    if (!alive[i]) continue;
    const [x, y] = brickPos(i);
    if (Math.abs(bx - x) < W / COLS / 2 && Math.abs(by - y) < 0.2) { alive[i] = false; broke[i] = frame; vy = -vy; score += 10; break; }
  }
  return { px, bx, by, vx, vy, alive, broke, paddleHit, score };
}

function Breakout() {
  const g = useGameState(INITIAL, step);
  const f = g.broke.reduce((m, b) => Math.max(m, b), -99);
  const now = useCurrentFrame();
  const kick = Math.max(0, 1 - (now - f) / 8); // a short camera kick after each break, decaying over 8 frames
  const shake = kick * kick * 0.08 * Math.sin(now * 2.7);
  const squash = Math.max(0, 1 - (now - g.paddleHit) / 6);
  return (
    <AbsoluteFill style={{ background: "#101422" }}>
      <Canvas3D>
        <PerspectiveCamera position={[shake, 2.1 + shake, 8.5]} lookAt={[0, 2.1, 0]} fov={42} />
        <AmbientLight intensity={0.4} />
        <DirectionalLight position={[2, 6, 6]} intensity={1.2} />
        <Mesh name="paddle" position={[g.px, 0, 0]} scale={[1 + 0.25 * squash, 1 - 0.35 * squash, 1]}>
          <Box args={[1.4, 0.2, 0.4]} />
          <MeshStandardMaterial color="#f0ebd8" roughness={0.4} />
        </Mesh>
        <Mesh name="ball" position={[g.bx, g.by, 0]}>
          <Sphere args={[0.13]} />
          <MeshStandardMaterial color="#ffd166" emissive="#ffd166" emissiveIntensity={0.6} />
        </Mesh>
        {g.alive.map((on, i) => {
          const [x, y] = brickPos(i);
          const t = (now - g.broke[i]) / 10; // 0..1 over the 10 frames after it breaks
          if (!on && t > 1) return null;
          const pop = on ? 1 : 1 + 0.6 * t;
          return (
            <Mesh key={i} name={`brick-${i}`} position={[x, y, on ? 0 : t * 0.8]} scale={[pop, on ? 1 : 1 - t, on ? 1 : 1 - t]}>
              <Box args={[W / COLS - 0.08, 0.32, 0.4]} />
              <MeshStandardMaterial color={on ? ["#ef476f", "#f78c6b", "#06d6a0", "#118ab2"][Math.floor(i / COLS)] : "#ffffff"} roughness={0.5} />
            </Mesh>
          );
        })}
      </Canvas3D>
      <div style={{ position: "absolute", left: 40, top: 28, fontFamily: "Inter", fontWeight: 800, fontSize: 40, color: "#f0ebd8" }}>{`SCORE ${g.score}`}</div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="breakout3d" component={Breakout} width={1280} height={720} fps={FPS} durationInFrames={FRAMES} />);
