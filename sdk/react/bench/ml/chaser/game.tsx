// A model in a game (@raea/motion/ml): a chaser steered by a small neural net (media/policy.onnx, 487 bytes, written by
// tools/make_policy.py), run on the engine's ONNX Runtime inside the game's step. A CPU model gives the same outputs for
// the same inputs, so the fold stays pure: a render of a logged play is the same chase.
//
//   "chaser": a game; the arrow keys move the dot, the net chases it
//   "orbit":  a video; the target circles on its own and the net chases it, one model run per frame of the render
import { AbsoluteFill, staticFile, useCurrentFrame, useGameState, type FrameInput } from "@raea/motion";
import { loadModel, tensor } from "@raea/motion/ml";

export const W = 960;
export const H = 540;
export const FPS = 60;

interface S {
  target: [number, number];
  pos: [number, number];
  vel: [number, number];
  trail: [number, number][];
  thrust: [number, number];
}

const START: S = { target: [W * 0.7, H * 0.5], pos: [W * 0.2, H * 0.3], vel: [0, 0], trail: [], thrust: [0, 0] };

/** one step of the chase: the net reads where the target is and how the chaser moves, and says how to thrust */
function chase(s: S, target: [number, number]): S {
  const net = loadModel(staticFile("policy.onnx"));
  const obs = tensor([(target[0] - s.pos[0]) / W, (target[1] - s.pos[1]) / H, s.vel[0] / 10, s.vel[1] / 10], [1, 4]);
  const a = net.run({ obs }).action.data as Float32Array;
  const vel: [number, number] = [(s.vel[0] + a[0] * 0.6) * 0.96, (s.vel[1] + a[1] * 0.6) * 0.96];
  const pos: [number, number] = [s.pos[0] + vel[0], s.pos[1] + vel[1]];
  return { target, pos, vel, thrust: [a[0], a[1]], trail: [...s.trail.slice(-40), pos] };
}

function play(s: S, input: FrameInput): S {
  const k = input.keys ?? [];
  const dx = (k.includes("ArrowRight") ? 6 : 0) - (k.includes("ArrowLeft") ? 6 : 0);
  const dy = (k.includes("ArrowDown") ? 6 : 0) - (k.includes("ArrowUp") ? 6 : 0);
  const t: [number, number] = [Math.min(W - 20, Math.max(20, s.target[0] + dx)), Math.min(H - 20, Math.max(20, s.target[1] + dy))];
  return chase(s, t);
}

function orbit(s: S, _input: FrameInput, frame: number): S {
  const a = frame / FPS;
  return chase(s, [W / 2 + Math.cos(a * 0.9) * W * 0.32, H / 2 + Math.sin(a * 1.7) * H * 0.3]);
}

function View({ s }: { s: S }) {
  return (
    <AbsoluteFill style={{ background: "#0f1316" }}>
      {s.trail.map((p, i) => (
        <div key={i} style={{ position: "absolute", left: p[0] - 3, top: p[1] - 3, width: 6, height: 6, borderRadius: 3, background: "#5fb3a1", opacity: i / s.trail.length }} />
      ))}
      <div style={{ position: "absolute", left: s.target[0] - 12, top: s.target[1] - 12, width: 24, height: 24, borderRadius: 12, border: "3px solid #f2c14e" }} />
      <div style={{ position: "absolute", left: s.pos[0] - 10, top: s.pos[1] - 10, width: 20, height: 20, borderRadius: 10, background: "#5fb3a1" }} />
      <div style={{ position: "absolute", left: 24, bottom: 20, color: "#9aa", fontFamily: "Inter", fontSize: 16 }}>
        thrust {s.thrust[0].toFixed(3)} {s.thrust[1].toFixed(3)}
      </div>
    </AbsoluteFill>
  );
}

export function Chaser() {
  return <View s={useGameState(START, play)} />;
}

export function Orbit() {
  useCurrentFrame();
  return <View s={useGameState(START, orbit)} />;
}
