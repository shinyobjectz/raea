// The dino (apps/void/shell.feature: Controllers): a runner that performs the composition. The stage plays while the dino
// runs, its ground the composition's frames; each cut (a clip starting or ending, void:timeline) is an obstacle to jump
// (space, up, a click, or the agent's jump). Clear it and the edit plays on; hit it and the stage pauses on that cut, and
// the next jump runs it again from the clip before. Reaching the end is a clean take.
import { AbsoluteFill, applyActions, useGameState, voidIn } from "@raea/motion";
import type { FrameInput, PanelActions } from "@raea/motion";
import { nextControl } from "@raea/terminal";
import type { Clip, Player, PlayerControl } from "@raea/terminal";

// the dino on the screen, the ground, and the world: a stage frame is PX points of ground
const DINO_X = 90, DINO_W = 26, DINO_H = 28, GROUND = 46, PX = 7, OB_W = 14, OB_H = 26;
// a jump, in points and points per panel frame (60 a second)
const LIFT = 8.6, FALL = 0.55;

type Run = "ready" | "running" | "crashed" | "done";
type S = { player: Player; clips: Clip[]; control: PlayerControl; y: number; vy: number; run: Run; takes: number; cleared: number; down: boolean; held: boolean };

// the cuts: where a clip starts or ends, inside the composition, once each
const cuts = (clips: Clip[], frames: number) => [...new Set(clips.flatMap((c) => [c.start, c.end]))].filter((f) => f > 4 && f < frames - 2).sort((a, b) => a - b);

const go = (s: S, what: "play" | "pause" | "seek" | "playFrom", frame = 0): S => ({ ...s, control: nextControl(s.control, what, frame) });

// the run from `frame`: the player taken to be there now (void's word on it comes a frame or two later; the old one,
// at the end, read as a clean take at once)
const from = (s: S, frame: number): S => go({ ...s, player: { ...s.player, frame, playing: true }, run: "running", y: 0, vy: LIFT, takes: s.takes + 1 }, "playFrom", frame);

const jump = (s: S): S => {
  if (s.run === "ready" || s.run === "done") return { ...from(s, 0), cleared: 0 };
  // again from the clip before the cut it hit
  if (s.run === "crashed") return from(s, Math.max(0, ...s.clips.map((c) => c.start).filter((f) => f < s.player.frame - 2)));
  return s.y <= 0 ? { ...s, vy: LIFT } : s;
};

export const actions: PanelActions<S> = {
  jump: { about: "Jump (or start the run, or run again after a crash)", run: (s) => jump(s) },
};

const step = (state: S, input: FrameInput): S => {
  let s = applyActions(actions, state, input);
  const p = voidIn<Player>(input, "player");
  if (p) s = { ...s, player: p };
  const tl = voidIn<{ clips: Clip[] }>(input, "timeline");
  if (tl) s = { ...s, clips: tl.clips };
  // a press: space, up, w, or the pointer going down
  const held = (input.keys ?? []).some((k) => k === " " || k === "ArrowUp" || k === "w");
  const down = !!input.pointer?.down;
  if ((held && !s.held) || (down && !s.down)) s = jump(s);
  s = { ...s, held, down };
  if (s.run !== "running") return s;
  // the jump
  const vy = s.y > 0 || s.vy > 0 ? s.vy - FALL : 0;
  const y = Math.max(0, s.y + vy);
  s = { ...s, y, vy: y > 0 ? vy : 0 };
  const { frame, frames } = s.player;
  if (frames > 0 && frame >= frames - 2) return go({ ...s, run: "done" }, "pause");
  // a cut under the dino while it's low: a crash, paused on it
  const hit = cuts(s.clips, frames).find((f) => Math.abs((f - frame) * PX) < (DINO_W + OB_W) / 2 - 3);
  if (hit !== undefined && s.y < OB_H - 4) return go({ ...s, run: "crashed" }, "pause");
  const cleared = cuts(s.clips, frames).filter((f) => f < frame).length;
  return cleared === s.cleared ? s : { ...s, cleared };
};

// the dino in blocks: rows of a 13 x 14 grid
const DINO = ["      ######", "      # ####", "      ######", "      ###   ", "#     #####", "##   ####  ", "### ######  ", "##########  ", " ########   ", "  ######    ", "   ## ##    ", "   #   #    ", "   ##  ##   "];

export default function Dino({ width, height }: { width: number; height: number }) {
  const s = useGameState<S>({ player: { frame: 0, frames: 0, rate: 30, playing: false }, clips: [], control: { n: 0, do: "", frame: 0 }, y: 0, vy: 0, run: "ready", takes: 0, cleared: 0, down: false, held: false }, step);
  const { frame, frames } = s.player;
  const ground = height - GROUND;
  const all = cuts(s.clips, frames);
  const named = (f: number) => s.clips.find((c) => c.start === f)?.name ?? s.clips.find((c) => c.end === f)?.name ?? "";
  const cell = 2;
  const say = s.run === "ready" ? "Space or click to run the edit" : s.run === "crashed" ? "Cut! Jump to take it again" : s.run === "done" ? `Clean take in ${s.takes}` : "";
  return (
    <AbsoluteFill data-void-publish={JSON.stringify({ control: s.control })} style={{ background: "#0e0e10", color: "#c9c9d1", fontFamily: "Inter", fontSize: 12, overflow: "hidden" }}>
      <div style={{ position: "absolute", left: 14, top: 10, display: "flex", gap: 14, fontVariantNumeric: "tabular-nums" }}>
        <span style={{ color: "#e8e8ea" }}>Perform</span>
        <span style={{ color: "#6b6b74" }}>{frames ? `cuts ${s.cleared} of ${all.length}` : "open something timed on the stage"}</span>
        <span style={{ color: "#6b6b74" }}>take {s.takes}</span>
      </div>
      {/* the ground, moving with the playhead */}
      <div style={{ position: "absolute", left: 0, right: 0, top: ground, height: 1, background: "#3a3a42" }} />
      {Array.from({ length: Math.ceil(width / 40) + 2 }, (_, i) => (
        <div key={i} style={{ position: "absolute", top: ground + 8 + (i % 3) * 5, left: i * 40 - ((frame * PX) % 40), width: 6 + (i % 4) * 3, height: 1, background: "#26262c" }} />
      ))}
      {/* the cuts ahead and behind, with the clip each starts */}
      {all.map((f) => {
        const x = DINO_X + (f - frame) * PX - OB_W / 2;
        if (x < -80 || x > width + 20) return null;
        return (
          <div key={f}>
            <div style={{ position: "absolute", left: x, top: ground - OB_H, width: OB_W, height: OB_H, borderRadius: 3, background: f < frame ? "#2a2a31" : "#2f9e6a" }} />
            <div style={{ position: "absolute", left: x - 30, width: OB_W + 60, top: ground - OB_H - 18, textAlign: "center", fontSize: 10, color: f < frame ? "#4a4a52" : "#8b8b94", whiteSpace: "nowrap" }}>{named(f)}</div>
          </div>
        );
      })}
      {/* the end of the composition */}
      {frames ? <div style={{ position: "absolute", left: DINO_X + (frames - frame) * PX, top: ground - 60, width: 2, height: 60, background: "#4f7bff" }} /> : null}
      {/* the dino */}
      {DINO.map((row, r) =>
        [...row].map((c, k) =>
          c === "#" ? <div key={`${r}.${k}`} style={{ position: "absolute", left: DINO_X - DINO_W / 2 + k * cell, top: ground - s.y - DINO_H + r * cell + 2, width: cell, height: cell, background: s.run === "crashed" ? "#d65b5b" : "#e8e8ea" }} /> : null,
        ),
      )}
      {say ? <div style={{ position: "absolute", left: 0, right: 0, top: Math.max(30, ground / 2 - 20), textAlign: "center", fontSize: 14, color: s.run === "crashed" ? "#d65b5b" : "#e8e8ea" }}>{say}</div> : null}
    </AbsoluteFill>
  );
}
