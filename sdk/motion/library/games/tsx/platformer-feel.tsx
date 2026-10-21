// record: coyote-time-jump-buffer, variable-jump-height, building-a-better-jump
// Platformer jump feel in raea's SDK: coyote time (a jump still counts 0.1 s after walking off a ledge), a jump buffer
// (a press up to 0.1 s before landing jumps on landing), a variable height (letting go early cuts the rise), and a
// jump whose height and time to apex are chosen first, with gravity and launch speed derived from them (Pittman's
// "Building a better jump"). Scripted input for the render: run right, press jump early before a landing, hold and
// release. A live player's keys win when `live`.
import { AbsoluteFill, Composition, registerRoot, useGameState, type FrameInput } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera } from "@raea/motion/3d";

const FPS = 60;
const FRAMES = 300;
const DT = 1 / FPS;
const H = 2.2, APEX = 0.38; // jump height (units) and time to apex (s)
const G = (2 * H) / (APEX * APEX); // gravity from the chosen arc
const V0 = (2 * H) / APEX; // launch speed from the chosen arc
const COYOTE = 0.1, BUFFER = 0.1, CUT = 0.45, RUN = 4;
// ledges: [x from, x to, top]
const GROUND: [number, number, number][] = [[-20, 2, 0], [3.5, 7, 0.8], [8.5, 30, 0]];
interface P { x: number; y: number; vy: number; grounded: boolean; sinceGround: number; sincePress: number; held: boolean; jumps: number }
const INITIAL: P = { x: -3, y: 0, vy: 0, grounded: true, sinceGround: 0, sincePress: 99, held: false, jumps: 0 };

function scripted(frame: number): string[] {
  const t = frame / FPS;
  const keys = t < 4.6 ? ["ArrowRight"] : [];
  // jump presses: a short tap, an early press before landing (buffered), a long hold
  if ((t > 0.6 && t < 0.7) || (t > 1.42 && t < 1.9) || (t > 2.8 && t < 3.4)) keys.push(" ");
  return keys;
}

const floorAt = (x: number) => GROUND.filter(([a, b]) => x >= a && x <= b).reduce((m, [, , top]) => Math.max(m, top), -99);

function step(s: P, input: FrameInput & { live?: boolean }, frame: number): P {
  const keys = input.live ? input.keys : scripted(frame);
  const jump = keys.includes(" ") || keys.includes("ArrowUp");
  const pressed = jump && !s.held;
  const x = s.x + ((keys.includes("ArrowRight") ? 1 : 0) - (keys.includes("ArrowLeft") ? 1 : 0)) * RUN * DT;
  let sincePress = pressed ? 0 : s.sincePress + DT;
  let sinceGround = s.grounded ? 0 : s.sinceGround + DT;
  let vy = s.vy, jumps = s.jumps;
  if (sincePress <= BUFFER && sinceGround <= COYOTE) { vy = V0; sincePress = 99; sinceGround = 99; jumps += 1; }
  if (!jump && vy > 0) vy *= CUT; // released early: cut the rise
  vy -= G * DT;
  let y = s.y + vy * DT;
  const floor = floorAt(x);
  let grounded = false;
  if (y <= floor && s.y >= floor - 0.05) { y = floor; vy = 0; grounded = true; }
  if (y < -6) return { ...INITIAL, jumps };
  return { x, y, vy, grounded, sinceGround, sincePress, held: jump, jumps };
}

function Platformer() {
  const p = useGameState(INITIAL, step);
  const stretch = Math.min(0.3, Math.abs(p.vy) * 0.02);
  return (
    <AbsoluteFill style={{ background: "#0e1726" }}>
      <Canvas3D>
        <PerspectiveCamera position={[p.x + 1.5, 2.4, 9]} lookAt={[p.x + 1.5, 1.2, 0]} fov={45} />
        <AmbientLight intensity={0.45} />
        <DirectionalLight position={[3, 8, 6]} intensity={1.2} castShadow />
        {GROUND.map(([a, b, top], i) => (
          <Mesh key={i} position={[(a + b) / 2, top - 0.5, 0]}>
            <Box args={[b - a, 1, 2]} />
            <MeshStandardMaterial color="#3d5a80" roughness={0.9} />
          </Mesh>
        ))}
        <Mesh name="player" position={[p.x, p.y + 0.4, 0]} scale={[1 - stretch * 0.5, 1 + stretch, 1 - stretch * 0.5]}>
          <Box args={[0.6, 0.8, 0.6]} />
          <MeshStandardMaterial color={p.grounded ? "#ee6c4d" : "#f6bd60"} roughness={0.5} />
        </Mesh>
      </Canvas3D>
      <div style={{ position: "absolute", left: 40, top: 28, fontFamily: "Inter", fontWeight: 700, fontSize: 30, color: "#e0fbfc" }}>{`jumps ${p.jumps}   coyote ${COYOTE}s   buffer ${BUFFER}s`}</div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="platformer" component={Platformer} width={1280} height={720} fps={FPS} durationInFrames={FRAMES} />);
